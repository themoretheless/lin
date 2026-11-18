use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use rustc_hash::{FxHashMap, FxHashSet};

use crate::ast::{Decl, EdgeTarget, InsertEdge, Pred, Record, Stmt, TypeExpr, Value};
use crate::error::Error;
use crate::persist::Pack;
use crate::store::{
    Cell, Edge, Row, acquire_row, cell_from_arc_inline, content_hash_inline, now_ms, row_text,
};

use super::Db;
use super::eval::{
    eval_pred, point_key, pred_needs_now, record_needs_now, value_cell, value_needs_now,
    value_text, value_to_arc_str,
};
use super::graph_ops::edge_row;
use super::tx::{RowUndo, Undo};

pub(crate) type StmtExecOut = (Vec<Row>, Option<String>, Option<Pack>, bool, usize);

#[cfg(test)]
thread_local! {
    pub(super) static PREENCODE_TEST_MODE: std::cell::Cell<u8> = const { std::cell::Cell::new(0) };
    pub(super) static PREENCODE_TEST_ATTEMPTS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

pub(crate) struct PackCtx<'a> {
    pub(crate) track_written: bool,
    pub(crate) allow_preencode: bool,
    pub(crate) want_rows: bool,
    pub(crate) prepared_encoding: Option<crate::persist::PreparedEncoding>,
    pub(crate) snap: BTreeMap<(String, String), String>,
    pub(crate) written: BTreeSet<(String, String)>,
    pub(crate) undo: &'a mut Undo,
}

fn mark_written(ctx: &mut PackCtx, collection: &str, rows: &[Row]) {
    if !ctx.track_written {
        return;
    }
    for r in rows {
        if let Some(id) = row_text(r, "id") {
            ctx.written.insert((collection.to_string(), id.to_string()));
        }
    }
}

pub(crate) fn stmt_writes(s: &Stmt) -> bool {
    match s {
        Stmt::Query(_) | Stmt::Let { .. } | Stmt::IdbSlice { .. } => false,
        Stmt::IdbPull { .. }
        | Stmt::IdbPush
        | Stmt::Snapshot { .. }
        | Stmt::Restore { .. }
        | Stmt::Pin { .. }
        | Stmt::Unpin { .. } => false,
        Stmt::AppendFacts { .. }
        | Stmt::AppendEdges { .. }
        | Stmt::Insert { .. }
        | Stmt::InsertRows { .. }
        | Stmt::Update { .. }
        | Stmt::Delete { .. }
        | Stmt::DeleteEdge { .. }
        | Stmt::Reembed { .. }
        | Stmt::Decl(_) => true,
    }
}

pub(crate) fn stmt_append_only(s: &Stmt) -> bool {
    matches!(
        s,
        Stmt::AppendFacts { .. }
            | Stmt::AppendEdges { .. }
            | Stmt::Insert { .. }
            | Stmt::InsertRows { .. }
    )
}

pub(crate) fn stmt_schema(s: &Stmt) -> bool {
    matches!(s, Stmt::Decl(_))
}

pub(crate) fn pack_has_schema(pack: &Pack) -> bool {
    match pack {
        Pack::SchemaCol { .. }
        | Pack::SchemaRel { .. }
        | Pack::SchemaIndex { .. }
        | Pack::SchemaFilter { .. }
        | Pack::SchemaOwned { .. } => true,
        Pack::Batch { packs } => packs.iter().any(pack_has_schema),
        _ => false,
    }
}

fn pack_index_label(collection: &str, fields: &[String]) -> String {
    format!("{collection}[{}]", fields.join(","))
}

pub(crate) fn fold_packs(packs: Vec<Pack>) -> Option<Pack> {
    match packs.len() {
        0 => None,
        1 => packs.into_iter().next(),
        2.. => Some(Pack::Batch { packs }),
    }
}

fn cas_each_ok(collection: &str, row: &Row, got: &str, ctx: &PackCtx) -> Result<(), Error> {
    if !ctx.track_written {
        return Ok(());
    }
    let Some(id) = row_text(row, "id") else {
        return Ok(());
    };
    let key = (collection.to_string(), id.to_string());
    if ctx.written.contains(&key) {
        return Err(Error::runtime(format!(
            "cas mismatch: row {id} changed mid-pack"
        )));
    }
    if let Some(expect) = ctx.snap.get(&key)
        && expect.as_str() != got
    {
        return Err(Error::runtime(format!(
            "cas mismatch: expected {expect:?}, found {got:?}"
        )));
    }
    Ok(())
}

impl Db {
    pub(crate) fn exec_program_ext(
        &mut self,
        stmts: &[Stmt],
        undo: &mut Undo,
        allow_preencode: bool,
        want_rows: bool,
    ) -> Result<(StmtExecOut, Option<crate::persist::PreparedEncoding>), Error> {
        if self.store.has_deferred_doc_ops() {
            self.store.flush_fts_deferred_docs();
        }
        for stmt in stmts {
            if let Stmt::Insert { collection, .. }
            | Stmt::Update { collection, .. }
            | Stmt::Delete { collection, .. } = stmt
                && self.store.is_disk_backed(collection)
            {
                self.store.collection_mut(collection);
            }
        }
        let mut bindings: BTreeMap<String, Vec<Row>> = BTreeMap::new();
        // CAS-each snapshot is only required when multiple statements execute in the same program,
        // because within a single statement `written` is empty and `snap[k]` equals `got`.
        let need_snap = stmts.len() > 1
            && stmts.iter().any(|s| {
                matches!(
                    s,
                    Stmt::Update { cas_each: true, .. } | Stmt::Delete { cas_each: true, .. }
                )
            });
        let mut ctx = PackCtx {
            track_written: need_snap,
            allow_preencode: allow_preencode && stmts.len() == 1 && self.is_durable(),
            want_rows,
            prepared_encoding: None,
            snap: if need_snap {
                self.hash_snapshot()
            } else {
                BTreeMap::new()
            },
            written: BTreeSet::new(),
            undo,
        };
        let mut last_rows = Vec::new();
        let mut last_msg = None;
        let mut packs = Vec::new();
        let mut any_wrote = false;
        let mut last_affected = 0usize;
        let last_idx = stmts.len().saturating_sub(1);

        for (si, stmt) in stmts.iter().enumerate() {
            if let Stmt::Let { name, query } = stmt {
                let rows = self.exec_query(query, &bindings)?;
                if want_rows && si == last_idx {
                    last_rows = rows.clone();
                } else {
                    last_rows.clear();
                }
                last_msg = None;
                last_affected = rows.len();
                bindings.insert(name.clone(), rows);
                continue;
            }
            ctx.want_rows = want_rows && si == last_idx;
            let (rows, message, pack, wrote, affected) =
                self.exec_stmt(stmt, &bindings, &mut ctx)?;
            last_rows = rows;
            last_msg = message;
            any_wrote |= wrote;
            last_affected = affected;
            if let Some(p) = pack {
                packs.push(p);
            }
        }
        Ok((
            (
                last_rows,
                last_msg,
                fold_packs(packs),
                any_wrote,
                last_affected,
            ),
            ctx.prepared_encoding,
        ))
    }

    fn hash_snapshot(&self) -> BTreeMap<(String, String), String> {
        let mut m = BTreeMap::new();
        for (col, rows) in &self.store.collections {
            for r in rows {
                if let Some(id) = row_text(r, "id") {
                    m.insert(
                        (col.clone(), id.to_string()),
                        row_text(r, "hash").unwrap_or("").to_string(),
                    );
                }
            }
        }
        m
    }

    fn exec_stmt(
        &mut self,
        stmt: &Stmt,
        bindings: &BTreeMap<String, Vec<Row>>,
        ctx: &mut PackCtx,
    ) -> Result<StmtExecOut, Error> {
        let durable = self.is_durable();
        match stmt {
            Stmt::Query(q) => {
                let rows = self.exec_query(q, bindings)?;
                let n = rows.len();
                Ok((rows, None, None, false, n))
            }
            Stmt::AppendFacts { records } => {
                let needs_now = records.iter().any(record_needs_now);
                let now = if needs_now { now_ms() } else { 0 };
                let n = records.len();
                let mut bulk_s = Vec::with_capacity(n);
                let mut bulk_p = Vec::with_capacity(n);
                let mut bulk_o = Vec::with_capacity(n);
                let is_standard_spo = records.first().is_some_and(|r| {
                    r.fields.len() == 3
                        && r.fields[0].0 == "s"
                        && r.fields[1].0 == "p"
                        && r.fields[2].0 == "o"
                });
                static EMPTY_ARC: std::sync::LazyLock<Arc<str>> =
                    std::sync::LazyLock::new(|| Arc::from(""));
                if is_standard_spo {
                    let mut last_p_val: Option<&Value> = None;
                    let mut last_p_arc: Option<Arc<str>> = None;
                    for record in records {
                        if record.fields.len() == 3
                            && record.fields[0].0 == "s"
                            && record.fields[1].0 == "p"
                            && record.fields[2].0 == "o"
                        {
                            bulk_s.push(value_to_arc_str(&record.fields[0].1, now));
                            let pv = &record.fields[1].1;
                            if last_p_val == Some(pv)
                                && let Some(pa) = &last_p_arc
                            {
                                bulk_p.push(Arc::clone(pa));
                            } else {
                                let pa = value_to_arc_str(pv, now);
                                last_p_arc = Some(Arc::clone(&pa));
                                last_p_val = Some(pv);
                                bulk_p.push(pa);
                            }
                            bulk_o.push(value_to_arc_str(&record.fields[2].1, now));
                        } else {
                            let mut s = None;
                            let mut p = None;
                            let mut o = None;
                            for (k, v) in &record.fields {
                                match k.as_str() {
                                    "s" => s = Some(value_to_arc_str(v, now)),
                                    "p" => p = Some(value_to_arc_str(v, now)),
                                    "o" => o = Some(value_to_arc_str(v, now)),
                                    _ => {}
                                }
                            }
                            bulk_s.push(s.unwrap_or_else(|| Arc::clone(&EMPTY_ARC)));
                            bulk_p.push(p.unwrap_or_else(|| Arc::clone(&EMPTY_ARC)));
                            bulk_o.push(o.unwrap_or_else(|| Arc::clone(&EMPTY_ARC)));
                        }
                    }
                } else {
                    for record in records {
                        let mut s = None;
                        let mut p = None;
                        let mut o = None;
                        for (k, v) in &record.fields {
                            match k.as_str() {
                                "s" => s = Some(value_to_arc_str(v, now)),
                                "p" => p = Some(value_to_arc_str(v, now)),
                                "o" => o = Some(value_to_arc_str(v, now)),
                                _ => {}
                            }
                        }
                        bulk_s.push(s.unwrap_or_else(|| Arc::clone(&EMPTY_ARC)));
                        bulk_p.push(p.unwrap_or_else(|| Arc::clone(&EMPTY_ARC)));
                        bulk_o.push(o.unwrap_or_else(|| Arc::clone(&EMPTY_ARC)));
                    }
                }
                let want_rows = ctx.want_rows && n <= 128;
                let (rows, changed_n) = self
                    .store
                    .append_facts_spo_bulk(&bulk_s, &bulk_p, &bulk_o, want_rows);
                let msg = if changed_n == 0 {
                    Some("idempotent".into())
                } else {
                    None
                };
                let wrote = changed_n > 0;
                let affected = if wrote { n } else { rows.len() };
                let pack = if wrote && durable {
                    Some(Pack::AppendFactsBulk {
                        s: bulk_s,
                        p: bulk_p,
                        o: bulk_o,
                    })
                } else {
                    None
                };
                Ok((rows, msg, pack, wrote, affected))
            }
            Stmt::AppendEdges { edges } => {
                let n = edges.len();
                self.store.edges.reserve(n);
                self.store.edge_keys.reserve(n);
                let mut rows = if ctx.want_rows {
                    Vec::with_capacity(n)
                } else {
                    Vec::new()
                };
                let mut changed_n = 0usize;
                let mut bulk_rel = if durable {
                    Vec::with_capacity(n)
                } else {
                    Vec::new()
                };
                let mut bulk_from = if durable {
                    Vec::with_capacity(n)
                } else {
                    Vec::new()
                };
                let mut bulk_to = if durable {
                    Vec::with_capacity(n)
                } else {
                    Vec::new()
                };
                for e in edges {
                    let (row, changed) =
                        self.append_edge(&e.rel, &e.from, &e.to, ctx.want_rows || durable)?;
                    if changed {
                        changed_n += 1;
                        if durable {
                            bulk_rel.push(e.rel.clone());
                            bulk_from.push(row_text(&row, "from").unwrap_or("").to_string());
                            bulk_to.push(row_text(&row, "to").unwrap_or("").to_string());
                        }
                    }
                    if ctx.want_rows {
                        rows.push(row);
                    }
                }
                let msg = if changed_n == 0 {
                    Some("idempotent".into())
                } else {
                    None
                };
                let wrote = changed_n > 0;
                let pack = if wrote && durable {
                    Some(Pack::AppendEdgesBulk {
                        rel: bulk_rel,
                        from: bulk_from,
                        to: bulk_to,
                    })
                } else {
                    None
                };
                Ok((rows, msg, pack, wrote, n))
            }
            Stmt::Insert { .. } | Stmt::InsertRows { .. } => {
                let (collection, edges): (&String, &[InsertEdge]) = match stmt {
                    Stmt::Insert {
                        collection, edges, ..
                    } => (collection, edges.as_slice()),
                    Stmt::InsertRows { collection, .. } => (collection, &[]),
                    _ => unreachable!("insert arm"),
                };
                let start = self.store.collection(collection).len();
                let (rows, new_edges, staged) = match stmt {
                    Stmt::Insert { records, .. } => self.insert_bulk_ext(
                        collection,
                        records,
                        edges,
                        ctx.allow_preencode,
                        ctx.want_rows,
                    )?,
                    _ => {
                        let (built, shape) = self.pending_insert_rows.take().ok_or_else(|| {
                            Error::runtime("insert_batch: no staged rows for this statement")
                        })?;
                        self.insert_bulk_built(
                            collection,
                            built,
                            shape,
                            None,
                            &[],
                            ctx.allow_preencode,
                            ctx.want_rows,
                        )?
                    }
                };
                let inserted = &self.store.collection(collection)[start..];
                let affected = inserted.len();
                if ctx.track_written {
                    mark_written(ctx, collection, inserted);
                }
                let pack = if durable {
                    let pack = if let Some(staged) = staged {
                        ctx.prepared_encoding = Some(staged.encoding);
                        staged.pack.unwrap_or_else(|| {
                            crate::persist::rows_to_insert_pack(
                                collection.clone(),
                                inserted,
                                new_edges,
                            )
                        })
                    } else if ctx.allow_preencode
                        && edges.is_empty()
                        && new_edges.is_empty()
                        && crate::persist::insert_cols_lossless(inserted)
                        && let Some(persist) = self.persist.as_mut()
                        && let Some((magic, fields)) = crate::persist::encode_insert_rows_direct(
                            self.store.r#gen.wrapping_add(1),
                            self.store.next_id,
                            collection,
                            inserted,
                            &mut persist.encode_buf,
                        )
                    {
                        let n = inserted.len() as u32;
                        let generation = self.store.r#gen.wrapping_add(1);
                        let next_id = self.store.next_id;
                        ctx.prepared_encoding = Some(crate::persist::PreparedEncoding::Direct {
                            generation,
                            next_id,
                            collection: collection.clone(),
                            fields: fields.clone(),
                            rows: n,
                            magic,
                        });
                        Pack::InsertCols {
                            collection: collection.clone(),
                            fields,
                            cols: Vec::new(),
                            n,
                            edges: Vec::new(),
                        }
                    } else {
                        crate::persist::rows_to_insert_pack(collection.clone(), inserted, new_edges)
                    };
                    Some(pack)
                } else {
                    None
                };
                Ok((rows, None, pack, true, affected))
            }
            Stmt::Update {
                collection,
                pred,
                cas,
                cas_each,
                record,
            } => {
                let (rows, affected, pack) = self.update_cas_ext(
                    collection,
                    pred.as_ref(),
                    cas.as_deref(),
                    *cas_each,
                    record,
                    ctx,
                    durable,
                )?;
                Ok((rows, None, pack, true, affected))
            }
            Stmt::Delete {
                collection,
                pred,
                cas,
                cas_each,
            } => {
                let mut rows =
                    self.delete_rows(collection, pred, cas.as_deref(), *cas_each, ctx)?;
                let affected = rows.len();
                let pack = if durable {
                    Some(Pack::Delete {
                        collection: collection.clone(),
                        rows: if ctx.want_rows {
                            rows.clone()
                        } else {
                            std::mem::take(&mut rows)
                        },
                    })
                } else {
                    None
                };
                let ret_rows = if ctx.want_rows { rows } else { Vec::new() };
                Ok((ret_rows, None, pack, true, affected))
            }
            Stmt::DeleteEdge { rel, from, to } => {
                let now = if value_needs_now(from) || value_needs_now(to) {
                    now_ms()
                } else {
                    0
                };
                let from_s = value_text(from, now);
                let to_s = value_text(to, now);
                if !self.store.remove_edge(rel, &from_s, &to_s) {
                    return Err(Error::runtime("delete: edge not found"));
                }
                let out_rows = if ctx.want_rows {
                    vec![edge_row(rel, &from_s, &to_s)]
                } else {
                    Vec::new()
                };
                let pack = if durable {
                    Some(Pack::DeleteEdge {
                        rel: rel.clone(),
                        from: from_s,
                        to: to_s,
                    })
                } else {
                    None
                };
                Ok((out_rows, None, pack, true, 1))
            }
            Stmt::Let { name, query } => {
                let rows = self.exec_query(query, bindings)?;
                let n = rows.len();
                Ok((rows, Some(format!("let {name}")), None, false, n))
            }
            Stmt::Reembed { collection, .. } => {
                let n = self.reembed_collection(collection)?;
                let id = self
                    .embedder
                    .as_ref()
                    .map(|e| e.id().to_string())
                    .unwrap_or_else(|| self.store.embed_id.clone());
                let pack = if durable { Some(Pack::Reembed) } else { None };
                Ok((
                    Vec::new(),
                    Some(format!("reembed {collection}: {n} rows; embed_id={id}")),
                    pack,
                    true,
                    0,
                ))
            }
            Stmt::Decl(d) => self.exec_decl(d),
            Stmt::IdbSlice { query } => {
                let rows = self.exec_query(query, bindings)?;
                let n = rows.len();
                Ok((rows, Some("idb slice: native".into()), None, false, n))
            }
            Stmt::IdbPull { since, take } => {
                let since = (*since).max(0) as u64;
                let frames = self.export_wal_since(since)?;
                self.pulled_wal = frames.clone();
                let mut rows = Vec::new();
                let mut n = 0i64;
                let limit = take.unwrap_or(i64::MAX);
                crate::persist::for_each_wal_frame(&frames, |rec| {
                    if n >= limit {
                        return Ok(());
                    }
                    let mut row = Row::new();
                    row.insert("gen", Cell::Int(rec.r#gen as i64));
                    row.insert("next_id", Cell::Int(rec.next_id as i64));
                    row.insert("bytes", Cell::Int(0));
                    rows.push(row);
                    n += 1;
                    Ok(())
                })?;
                if rows.is_empty() {
                    let mut row = Row::new();
                    row.insert("gen", Cell::Int(since as i64));
                    row.insert("frames", Cell::Int(0));
                    row.insert("bytes", Cell::Int(0));
                    rows.push(row);
                } else {
                    for r in &mut rows {
                        r.insert("bytes", Cell::Int(frames.len() as i64));
                        r.insert("frames", Cell::Int(n));
                    }
                }
                let out_n = rows.len();
                Ok((
                    rows,
                    Some(format!(
                        "pull idb since {since}: {} bytes (push idb applies)",
                        frames.len()
                    )),
                    None,
                    false,
                    out_n,
                ))
            }
            Stmt::IdbPush => {
                let frames = std::mem::take(&mut self.pulled_wal);
                if frames.is_empty() {
                    return Ok((
                        Vec::new(),
                        Some("push idb: nothing pulled".into()),
                        None,
                        false,
                        0,
                    ));
                }
                if self.persist.is_some() && !self.follower {
                    return Err(Error::runtime(
                        "push idb: refused on durable primary — pull/push on memory or follower",
                    ));
                }
                let n = self.apply_wal(&frames)?;
                let where_ = if self.follower { "follower" } else { "memory" };
                Ok((
                    Vec::new(),
                    Some(format!("push idb: applied {n} frames ({where_})")),
                    None,
                    false,
                    0,
                ))
            }
            Stmt::Snapshot { name } | Stmt::Pin { name } => {
                self.pins.insert(name.clone(), self.store.mem_backup_pin());
                Ok((
                    Vec::new(),
                    Some(format!(
                        "memory pin {name:?} at gen={} (not durable; use checkpoint/backup for disk)",
                        self.store.r#gen
                    )),
                    None,
                    false,
                    0,
                ))
            }
            Stmt::Restore { name } | Stmt::Unpin { name } => {
                let Some(pin) = self.pins.get(name).cloned() else {
                    return Err(Error::runtime(format!("unknown memory pin {name:?}")));
                };
                if self.persist.is_some() {
                    return Err(Error::runtime(
                        "memory pin restore: not allowed on durable Db — use reader()/backup",
                    ));
                }
                self.store.mem_restore(pin);
                self.plan_cache.clear();
                self.query_cache.clear();
                Ok((
                    Vec::new(),
                    Some(format!(
                        "memory pin restore {name:?} → gen={}",
                        self.store.r#gen
                    )),
                    None,
                    false,
                    0,
                ))
            }
        }
    }

    fn append_edge(
        &mut self,
        rel: &str,
        from: &Value,
        to: &Value,
        build_row: bool,
    ) -> Result<(Row, bool), Error> {
        let now = if value_needs_now(from) || value_needs_now(to) {
            now_ms()
        } else {
            0
        };
        let from = value_text(from, now);
        let to = value_text(to, now);
        let changed = self.store.append_edge_parts(rel, &from, &to);
        let row = if build_row {
            edge_row(rel, &from, &to)
        } else {
            Row::new()
        };
        Ok((row, changed))
    }

    /// An `update` that assigns `id` must keep the primary key unique: it may rename one
    /// row to an id no other row holds. `targets` are the positions of the matched rows.
    pub(crate) fn check_update_id(
        &self,
        collection: &str,
        record: &crate::ast::Record,
        targets: &[usize],
        now: i64,
    ) -> Result<(), Error> {
        let Some((_, value)) = record.fields.iter().find(|(k, _)| k == "id") else {
            return Ok(());
        };
        let cell = value_cell(value, now);
        let Some(new_id) = cell.text() else {
            return Ok(());
        };
        let taken = match targets {
            [] => false,
            [only] => self
                .store
                .row_index(collection, new_id)
                .is_some_and(|holder| holder != *only),
            _ => true,
        };
        if taken {
            return Err(Error::runtime(format!("duplicate id: {new_id}")));
        }
        Ok(())
    }

    pub(crate) fn check_row_fks(&self, collection: &str, row: &Row) -> Result<(), Error> {
        for fk in &self.catalog.fks {
            if fk.from_col != collection {
                continue;
            }
            let Some(val) = row_text(row, &fk.from_field) else {
                continue;
            };
            if val.is_empty() {
                continue;
            }
            if !self.fk_target_exists(&fk.to_col, &fk.to_field, val) {
                return Err(Error::runtime(format!(
                    "fk: {collection}.{} → {}.{} missing {val}",
                    fk.from_field, fk.to_col, fk.to_field
                )));
            }
        }
        Ok(())
    }

    /// Refuse to delete rows a foreign key still points at. `deleting` are the rows about
    /// to leave `collection`; a referencing row that is itself being deleted (a key into
    /// its own collection) does not count. Each referencing collection is read once.
    pub(crate) fn ensure_not_referenced(&self, collection: &str, deleting: &[Row]) -> Result<(), Error> {
        for fk in &self.catalog.fks {
            if fk.to_col != collection {
                continue;
            }
            let targets: FxHashSet<&str> = deleting
                .iter()
                .filter_map(|r| row_text(r, &fk.to_field))
                .filter(|v| !v.is_empty())
                .collect();
            if targets.is_empty() {
                continue;
            }
            let self_ref = fk.from_col == collection;
            let deleted_ids: FxHashSet<&str> = if self_ref {
                deleting.iter().filter_map(|r| row_text(r, "id")).collect()
            } else {
                FxHashSet::default()
            };
            let referenced = if self.store.is_disk_backed(&fk.from_col) {
                // No in-memory rows to scan: ask the engine per value. A self-reference
                // inside the deleted set cannot be told apart here, so it counts.
                targets
                    .iter()
                    .find(|v| self.store.disk_has_key(&fk.from_col, &fk.from_field, v))
                    .map(|v| v.to_string())
            } else {
                self.store.collection(&fk.from_col).iter().find_map(|r| {
                    let v = row_text(r, &fk.from_field)?;
                    if !targets.contains(v) {
                        return None;
                    }
                    if self_ref && row_text(r, "id").is_some_and(|id| deleted_ids.contains(id)) {
                        return None;
                    }
                    Some(v.to_string())
                })
            };
            if let Some(val) = referenced {
                return Err(Error::runtime(format!(
                    "fk: {}.{} → {collection}.{} still references {val}; delete or update the referencing rows first",
                    fk.from_col, fk.from_field, fk.to_field
                )));
            }
        }
        Ok(())
    }

    fn fk_target_exists(&self, to_col: &str, to_field: &str, val: &str) -> bool {
        // A disk-backed target has no in-memory row map: ask the engine, for `id` as well.
        if self.store.is_disk_backed(to_col) {
            return self.store.disk_has_key(to_col, to_field, val);
        }
        if to_field == "id" {
            return self.store.get_by_id(to_col, val).is_some();
        }
        self.store
            .collection(to_col)
            .iter()
            .any(|r| row_text(r, to_field) == Some(val))
    }

    fn assert_unique_index(&self, collection: &str, fields: &[String]) -> Result<(), Error> {
        let def = crate::catalog::IndexDef {
            collection: collection.to_string(),
            unique: true,
            fields: fields.to_vec(),
            pred: None,
            bitmap_card_field: None,
        };
        let mut live = crate::index::LiveIndex::new(def);
        for (i, row) in self.store.collection(collection).iter().enumerate() {
            if let Err(e) = live.insert_at(i, row) {
                return Err(Error::runtime(e));
            }
        }
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn insert_bulk(
        &mut self,
        collection: &str,
        records: &[Record],
        edges: &[InsertEdge],
        allow_preencode: bool,
    ) -> Result<(Vec<Row>, Vec<Edge>, Option<crate::persist::PreparedInsert>), Error> {
        self.insert_bulk_ext(collection, records, edges, allow_preencode, true)
    }

    pub(crate) fn insert_bulk_ext(
        &mut self,
        collection: &str,
        records: &[Record],
        edges: &[InsertEdge],
        allow_preencode: bool,
        caller_wants_rows: bool,
    ) -> Result<(Vec<Row>, Vec<Edge>, Option<crate::persist::PreparedInsert>), Error> {
        let n = records.len();
        let needs_embed = self.embedder.is_some()
            && self
                .catalog
                .collection(collection)
                .is_some_and(|c| c.has_vec());

        // Ultra-fast 1-row path: zero FxHashSet allocations, zero worker pool checks,
        // conditional `now_ms()` syscall, direct O(1) duplicate check!
        if n == 1 {
            let rec = &records[0];
            let edges_need_now = edges.iter().any(|e| match &e.target {
                EdgeTarget::Value(v) => value_needs_now(v),
                EdgeTarget::Page(_) => false,
            });
            let now = if record_needs_now(rec) || edges_need_now {
                now_ms()
            } else {
                0
            };
            let mut row = acquire_row();
            for (k, v) in &rec.fields {
                row.insert(k.clone(), value_cell(v, now));
            }
            if row.get("id").and_then(Cell::text).is_none() {
                row.insert("id", Cell::text_arc(self.store.alloc_id()));
            }
            if row.get("hash").and_then(Cell::text).is_none()
                && let Some(body) = row.get("body").and_then(Cell::text)
            {
                let h = content_hash_inline(body);
                row.insert("hash", Cell::SmallText(h));
            }
            let col_len = self.store.collection(collection).len();
            if col_len > 0 {
                if let Some(id) = row.get("id").and_then(Cell::text)
                    && self.store.get_by_id(collection, id).is_some()
                {
                    return Err(Error::runtime(format!("duplicate id: {id}")));
                }
                if collection == "docs"
                    && let Some(uri) = row.get("uri").and_then(Cell::text)
                    && self.store.get_by_uri(uri).is_some()
                {
                    return Err(Error::runtime(format!("duplicate uri: {uri}")));
                }
            }
            if !self.catalog.fks.is_empty() {
                self.check_row_fks(collection, &row)?;
            }
            if needs_embed {
                self.maybe_embed_rows(collection, std::slice::from_mut(&mut row));
            }
            let mut new_edges = Vec::with_capacity(edges.len());
            if !edges.is_empty() {
                let from_id = row_text(&row, "id").unwrap_or("");
                self.store.edges.reserve(edges.len());
                self.store.edge_keys.reserve(edges.len());
                for e in edges {
                    let to = match &e.target {
                        EdgeTarget::Page(uri) => self
                            .store
                            .find_doc_key(uri)
                            .and_then(|r| row_text(r, "id"))
                            .map(str::to_string)
                            .unwrap_or_else(|| uri.clone()),
                        EdgeTarget::Value(v) => value_text(v, now),
                    };
                    let _ = self.store.append_edge_parts(&e.rel, from_id, &to);
                    new_edges.push(Edge {
                        rel: e.rel.clone(),
                        from: from_id.to_string(),
                        to,
                    });
                }
            }
            let start = self.store.collection(collection).len();
            self.store.index_insert_row(collection, start, &row)?;
            self.store.fts_insert_row(collection, start, &row);
            if let Some(Cell::Vec(v)) = row.get("embedding")
                && let Some(hnsw) = self.store.hnsw.get_mut(collection)
            {
                hnsw.insert(start, Arc::clone(v));
            }
            self.store.row_maps_register_row(collection, start, &row);
            self.store.update_col_count(collection, 1);
            if caller_wants_rows {
                let ret = row.clone();
                self.store.collection_mut(collection).push(row);
                return Ok((vec![ret], new_edges, None));
            } else {
                self.store.collection_mut(collection).push(row);
                return Ok((Vec::new(), new_edges, None));
            }
        }

        let now = now_ms();
        let hashing_dim = if needs_embed {
            self.embedder.as_ref().and_then(|e| e.hashing_dim())
        } else {
            None
        };
        let (built, all_uniform, all_embedded, lossless_cols) =
            build_insert_rows(records, now, needs_embed, hashing_dim);
        // Any record without an explicit id gets a generated one, wherever it sits in the
        // batch (deciding from the first record left later id-less rows without an id).
        let needs_id = records.is_empty()
            || records
                .iter()
                .any(|r| !r.fields.iter().any(|(k, _)| k == "id"));
        self.insert_bulk_built(
            collection,
            built,
            BuiltShape {
                all_uniform,
                all_embedded,
                needs_id,
                lossless_cols,
                hashing_dim,
                now,
            },
            Some(records),
            edges,
            allow_preencode,
            caller_wants_rows,
        )
    }

    /// Bulk insert of rows already built from the statement (or straight from a
    /// [`crate::batch::RecordBatch`]): ids and hashes, uniqueness, foreign keys,
    /// embeddings, edges, the pre-encoded log record and the store update. `records`,
    /// when given, are the source records and enable the key check that works on them
    /// directly.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn insert_bulk_built(
        &mut self,
        collection: &str,
        mut built: Vec<Row>,
        shape: BuiltShape,
        records: Option<&[Record]>,
        edges: &[InsertEdge],
        allow_preencode: bool,
        caller_wants_rows: bool,
    ) -> Result<(Vec<Row>, Vec<Edge>, Option<crate::persist::PreparedInsert>), Error> {
        let BuiltShape {
            all_uniform,
            all_embedded,
            needs_id,
            lossless_cols,
            hashing_dim,
            now,
        } = shape;
        let n = built.len();
        let col_len = self.store.collection(collection).len();
        let store_empty = col_len == 0;
        let has_fks = self.catalog.fks.iter().any(|fk| fk.from_col == collection);
        let is_docs = collection == "docs";
        let first_record = records.and_then(|r| r.first());
        let id_rec_pos = first_record.and_then(|r| r.fields.iter().position(|(k, _)| k == "id"));
        let uri_rec_pos = if is_docs {
            first_record.and_then(|r| r.fields.iter().position(|(k, _)| k == "uri"))
        } else {
            None
        };

        let fast_checked = if let Some(records) = records
            && all_uniform
            && !needs_id
            && !has_fks
            && id_rec_pos.is_some()
            && (!is_docs
                || (uri_rec_pos.is_some()
                    && self.store.docs_moved_ids.is_empty()
                    && self.store.docs_moved_idxs.is_empty()
                    && self.store.retired_doc_ids.is_empty()))
        {
            let id_pos = id_rec_pos.unwrap();
            let uri_pos_opt = uri_rec_pos;
            let existing_ids = if !store_empty {
                self.store.by_id.get(collection)
            } else {
                None
            };
            let docs_by_uri = &self.store.docs_by_uri;
            check_batch_unique_records(
                records,
                id_pos,
                uri_pos_opt,
                existing_ids,
                docs_by_uri,
                col_len,
            )?
        } else {
            false
        };
        if !fast_checked {
            let mut batch_ids: FxHashSet<&str> = FxHashSet::default();
            batch_ids.reserve(n);
            let mut batch_uris: FxHashSet<&str> = FxHashSet::default();
            if is_docs {
                batch_uris.reserve(n);
            }
            for row in &mut built {
                if needs_id && row.get("id").and_then(Cell::text).is_none() {
                    row.insert("id", Cell::text_arc(self.store.alloc_id()));
                }
                if row.get("hash").and_then(Cell::text).is_none()
                    && let Some(body) = row.get("body").and_then(Cell::text)
                {
                    let h = content_hash_inline(body);
                    row.insert("hash", Cell::SmallText(h));
                }
                if let Some(id) = row.get("id").and_then(Cell::text)
                    && (!batch_ids.insert(id)
                        || (!store_empty && self.store.get_by_id(collection, id).is_some()))
                {
                    return Err(Error::runtime(format!("duplicate id: {id}")));
                }
                if is_docs
                    && let Some(uri) = row.get("uri").and_then(Cell::text)
                    && (!batch_uris.insert(uri)
                        || (!store_empty && self.store.get_by_uri(uri).is_some()))
                {
                    return Err(Error::runtime(format!("duplicate uri: {uri}")));
                }
                if has_fks {
                    self.check_row_fks(collection, row)?;
                }
            }
        }
        if !all_embedded {
            self.maybe_embed_rows(collection, &mut built);
        }

        let mut new_edges = Vec::with_capacity(edges.len());
        if let Some(first) = built.first()
            && !edges.is_empty()
        {
            let from_id = row_text(first, "id").unwrap_or("");
            self.store.edges.reserve(edges.len());
            self.store.edge_keys.reserve(edges.len());
            for e in edges {
                let to = match &e.target {
                    EdgeTarget::Page(uri) => self
                        .store
                        .find_doc_key(uri)
                        .and_then(|r| row_text(r, "id"))
                        .map(str::to_string)
                        .unwrap_or_else(|| uri.clone()),
                    EdgeTarget::Value(v) => value_text(v, now),
                };
                let _ = self.store.append_edge_parts(&e.rel, from_id, &to);
                new_edges.push(Edge {
                    rel: e.rel.clone(),
                    from: from_id.to_string(),
                    to,
                });
            }
        }

        let start = self.store.collection(collection).len();
        let uniform_keys = all_uniform && !needs_id;
        // The pre-encoded record is the compact columnar one; rows it cannot represent
        // exactly (missing fields, mixed types) are logged row by row after the insert.
        let prefer_direct = allow_preencode
            && self.persist.is_some()
            && uniform_keys
            && (512..4096).contains(&n)
            && std::env::var_os("LIN_BENCH_DIRECT_SMALL").is_some();
        let staged = if !prefer_direct && allow_preencode
            && (512..=16384).contains(&n)
            && edges.is_empty()
            && self.persist.is_some()
            && ((lossless_cols && !needs_id) || crate::persist::insert_cols_lossless(&built))
            && (n >= 4096
                || built
                    .first()
                    .and_then(|row| row.get("embedding").and_then(Cell::as_vec))
                    .is_some_and(|vector| vector.len().saturating_mul(n) >= 512 * 1024))
            && {
                let floats = if all_embedded && let Some(dim) = hashing_dim {
                    n.saturating_mul(dim)
                } else {
                    built
                        .iter()
                        .filter_map(|row| row.get("embedding").and_then(Cell::as_vec))
                        .fold(0usize, |total, vector| total.saturating_add(vector.len()))
                };
                let minimum = if n < 4096 {
                    512 * 1024
                } else {
                    2 * 1024 * 1024
                };
                (minimum..=16 * 1024 * 1024).contains(&floats)
            } {
            let generation = self.store.r#gen.wrapping_add(1);
            let next_id = self.store.next_id;
            let bytes = self
                .persist
                .as_mut()
                .map(|p| std::mem::take(&mut p.encode_buf))
                .unwrap_or_default();
            #[cfg(test)]
            let mode = PREENCODE_TEST_MODE.with(|mode| mode.get());
            #[cfg(not(test))]
            let mode = 0u8;
            #[cfg(test)]
            PREENCODE_TEST_ATTEMPTS.with(|attempts| attempts.set(attempts.get() + 1));
            let shared = Arc::new(std::mem::take(&mut built));
            let task = crate::persist::start_insert_encoding(
                self.persist.as_mut().unwrap(),
                generation,
                next_id,
                collection,
                shared.clone(),
                uniform_keys,
                bytes,
                mode,
            );
            let maintenance = self.maintain_insert_slab(collection, start, &shared);
            let staged = match task {
                Ok(reply) => reply.recv().unwrap_or_else(|_| {
                    crate::persist::prepare_insert_columns_ext(
                        generation,
                        next_id,
                        collection,
                        &shared,
                        uniform_keys,
                        Vec::new(),
                    )
                }),
                Err(job) => {
                    if let Err(error) = maintenance {
                        return Err(error);
                    }
                    job.encode()
                }
            };
            built = Arc::try_unwrap(shared).expect("encoder retained input after reply");
            maintenance?;
            Some(staged)
        } else if allow_preencode && n >= 512 && edges.is_empty() && self.persist.is_some() {
            let generation = self.store.r#gen.wrapping_add(1);
            let next_id = self.store.next_id;
            use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
            let mut encode_buf = self
                .persist
                .as_mut()
                .map(|p| std::mem::take(&mut p.encode_buf))
                .unwrap_or_default();
            let built_ref: &[Row] = &built;
            struct DirectEncTask {
                generation: u64,
                next_id: u64,
                col_ptr: usize,
                col_len: usize,
                rows_ptr: usize,
                rows_len: usize,
                uniform_keys: bool,
                state: AtomicU8,
                done: AtomicBool,
                panicked: AtomicBool,
                caller: std::thread::Thread,
                buf_slot: std::sync::Mutex<Option<Vec<u8>>>,
                res_slot: std::sync::Mutex<Option<(Option<([u8; 4], Vec<String>)>, Vec<u8>)>>,
            }
            static DIRECT_ENC_TX: std::sync::OnceLock<
                Option<std::sync::mpsc::SyncSender<std::sync::Arc<DirectEncTask>>>,
            > = std::sync::OnceLock::new();
            let tx_opt = DIRECT_ENC_TX.get_or_init(|| {
                let (tx, rx) =
                    std::sync::mpsc::sync_channel::<std::sync::Arc<DirectEncTask>>(8);
                let spawned = std::thread::Builder::new()
                    .name("lin-direct-enc".into())
                    .spawn(move || {
                        while let Ok(task) = rx.recv() {
                            if task
                                .state
                                .compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire)
                                .is_err()
                            {
                                continue;
                            }
                            struct WorkerDone<'a>(&'a DirectEncTask, bool);
                            impl Drop for WorkerDone<'_> {
                                fn drop(&mut self) {
                                    if self.1 {
                                        self.0.panicked.store(true, Ordering::Release);
                                    }
                                    self.0.done.store(true, Ordering::Release);
                                    self.0.caller.unpark();
                                }
                            }
                            let mut wd = WorkerDone(&task, true);
                            // SAFETY: `DirectEncGuard` keeps `collection` and `built_ref` live
                            // until `task.done` is stored `true`.
                            let col = unsafe {
                                std::str::from_utf8_unchecked(std::slice::from_raw_parts(
                                    task.col_ptr as *const u8,
                                    task.col_len,
                                ))
                            };
                            let rows = unsafe {
                                std::slice::from_raw_parts(
                                    task.rows_ptr as *const Row,
                                    task.rows_len,
                                )
                            };
                            let mut job_buf = task
                                .buf_slot
                                .lock()
                                .ok()
                                .and_then(|mut g| g.take())
                                .unwrap_or_default();
                            let res = crate::persist::encode_insert_rows_direct_ext(
                                task.generation,
                                task.next_id,
                                col,
                                rows,
                                task.uniform_keys,
                                &mut job_buf,
                            );
                            if let Ok(mut g) = task.res_slot.lock() {
                                *g = Some((res, job_buf));
                            }
                            wd.1 = false;
                        }
                    });
                if spawned.is_ok() {
                    Some(tx)
                } else {
                    None
                }
            });
            let (maint_res, direct_res) = if let Some(tx) = tx_opt.as_ref() {
                let task = std::sync::Arc::new(DirectEncTask {
                    generation,
                    next_id,
                    col_ptr: collection.as_ptr() as usize,
                    col_len: collection.len(),
                    rows_ptr: built_ref.as_ptr() as usize,
                    rows_len: built_ref.len(),
                    uniform_keys,
                    state: AtomicU8::new(0),
                    done: AtomicBool::new(false),
                    panicked: AtomicBool::new(false),
                    caller: std::thread::current(),
                    buf_slot: std::sync::Mutex::new(Some(encode_buf)),
                    res_slot: std::sync::Mutex::new(None),
                });
                struct DirectEncGuard<'a>(&'a DirectEncTask);
                impl Drop for DirectEncGuard<'_> {
                    fn drop(&mut self) {
                        if self
                            .0
                            .state
                            .compare_exchange(0, 2, Ordering::AcqRel, Ordering::Acquire)
                            .err()
                            == Some(1)
                        {
                            while !self.0.done.load(Ordering::Acquire) {
                                std::thread::park_timeout(std::time::Duration::from_micros(50));
                            }
                        }
                    }
                }
                let _guard = DirectEncGuard(&task);
                let _ = tx.try_send(std::sync::Arc::clone(&task));
                let maint = self.maintain_insert_slab(collection, start, built_ref);
                let direct = if task
                    .state
                    .compare_exchange(0, 2, Ordering::AcqRel, Ordering::Acquire)
                    .is_ok()
                {
                    let mut buf = task
                        .buf_slot
                        .lock()
                        .ok()
                        .and_then(|mut g| g.take())
                        .unwrap_or_default();
                    let res = crate::persist::encode_insert_rows_direct_ext(
                        generation,
                        next_id,
                        collection,
                        built_ref,
                        uniform_keys,
                        &mut buf,
                    );
                    (res, buf)
                } else {
                    for _ in 0..64 {
                        if task.done.load(Ordering::Acquire) {
                            break;
                        }
                        std::hint::spin_loop();
                    }
                    while !task.done.load(Ordering::Acquire) {
                        std::thread::park_timeout(std::time::Duration::from_micros(50));
                    }
                    if task.panicked.load(Ordering::Acquire) {
                        panic!("direct encode worker panicked");
                    }
                    task.res_slot
                        .lock()
                        .ok()
                        .and_then(|mut g| g.take())
                        .unwrap_or((None, Vec::new()))
                };
                (maint, direct)
            } else {
                let maint = self.maintain_insert_slab(collection, start, built_ref);
                let res = crate::persist::encode_insert_rows_direct_ext(
                    generation,
                    next_id,
                    collection,
                    built_ref,
                    uniform_keys,
                    &mut encode_buf,
                );
                (maint, (res, encode_buf))
            };
            if let Some(p) = self.persist.as_mut() {
                p.encode_buf = direct_res.1;
            }
            maint_res?;
            direct_res
                .0
                .map(|(magic, fields)| crate::persist::PreparedInsert {
                    pack: Some(Pack::InsertCols {
                        collection: collection.to_string(),
                        fields: fields.clone(),
                        cols: Vec::new(),
                        n: n as u32,
                        edges: Vec::new(),
                    }),
                    encoding: crate::persist::PreparedEncoding::Direct {
                        generation,
                        next_id,
                        collection: collection.to_string(),
                        fields,
                        rows: n as u32,
                        magic,
                    },
                })
        } else {
            self.maintain_insert_slab(collection, start, &built)?;
            None
        };
        self.store
            .update_col_count(collection, built.len() as isize);
        let want_rows = caller_wants_rows && built.len() <= 128;
        if want_rows {
            self.store
                .collection_mut(collection)
                .extend(built.iter().cloned());
            Ok((built, new_edges, staged))
        } else {
            self.store.collection_mut(collection).extend(built);
            Ok((Vec::new(), new_edges, staged))
        }
    }

    fn maintain_insert_slab(
        &mut self,
        collection: &str,
        start: usize,
        built: &[Row],
    ) -> Result<(), Error> {
        self.store.maintain_insert_slab(collection, start, built)
    }

    fn mutation_positions(
        &self,
        collection: &str,
        pred: Option<&Pred>,
        now: i64,
    ) -> smallvec::SmallVec<[usize; 2]> {
        if let Some(pred) = pred
            && let Some((field, key)) = point_key(pred)
            && (field == "id" || (field == "uri" && collection == "docs"))
        {
            if matches!(pred, Pred::Cmp { .. }) {
                return self
                    .store
                    .point_index(collection, field, key)
                    .into_iter()
                    .collect();
            }
            return self
                .store
                .point_index(collection, field, key)
                .into_iter()
                .filter(|&i| {
                    self.store
                        .get_by_idx(collection, i)
                        .is_some_and(|row| eval_pred(pred, row, now))
                })
                .collect();
        }
        if let Some(pred) = pred
            && let Some((uses, covered)) =
                crate::index::pick_index_with_coverage(&self.catalog, collection, pred)
            && let Some(idxs) = self.store.index_seek(collection, &uses, now)
        {
            if covered {
                return idxs.into_iter().collect();
            }
            return idxs
                .into_iter()
                .filter(|&i| {
                    self.store
                        .get_by_idx(collection, i)
                        .is_some_and(|row| eval_pred(pred, row, now))
                })
                .collect();
        }
        let mut slots = pred.map(super::eval::PredSlots::new);
        self.store
            .collection(collection)
            .iter()
            .enumerate()
            .filter_map(|(i, row)| slots.as_mut().is_none_or(|s| s.eval(row, now)).then_some(i))
            .collect()
    }

    #[allow(clippy::too_many_arguments)]
    fn update_cas_ext(
        &mut self,
        collection: &str,
        pred: Option<&Pred>,
        cas: Option<&str>,
        cas_each: bool,
        record: &Record,
        ctx: &mut PackCtx,
        durable: bool,
    ) -> Result<(Vec<Row>, usize, Option<Pack>), Error> {
        if !cas_each && cas.is_none() {
            return Err(Error::runtime("update requires cas"));
        }
        let has_guards = !self.catalog.guards.is_empty();
        let needs_now = pred.is_some_and(pred_needs_now) || record_needs_now(record) || has_guards;
        let now = if needs_now { now_ms() } else { 0 };

        let idxs = self.mutation_positions(collection, pred, now);
        if idxs.is_empty() {
            return Err(Error::runtime("update: no matching row"));
        }
        {
            let rows = self.store.collection(collection);
            for i in &idxs {
                let row = &rows[*i];
                let got = row_text(row, "hash").unwrap_or("");
                if cas_each {
                    cas_each_ok(collection, row, got, ctx)?;
                } else {
                    let cas = cas.ok_or_else(|| Error::runtime("update requires cas"))?;
                    if got != cas {
                        return Err(Error::runtime(format!(
                            "cas mismatch: expected {cas:?}, found {got:?}"
                        )));
                    }
                }
            }
        }
        self.check_update_id(collection, record, &idxs, now)?;
        let has_body = record.fields.iter().any(|(k, _)| k == "body");
        // The embedding is computed from title / body / snippet: when one of them changes
        // and the statement does not set the embedding itself, compute it again, as an
        // insert would, instead of keeping the vector of the old text.
        let reembed = self.embedder.is_some()
            && record
                .fields
                .iter()
                .any(|(k, _)| matches!(k.as_str(), "title" | "body" | "snippet"))
            && !record.fields.iter().any(|(k, _)| k == "embedding");
        let has_fks = self.catalog.fks.iter().any(|fk| fk.from_col == collection);
        let affected = idxs.len();
        let need_materialized = ctx.want_rows || durable || ctx.track_written;
        let mut out = if need_materialized {
            Vec::with_capacity(affected)
        } else {
            Vec::new()
        };

        for i in idxs {
            let before = self.store.collection(collection)[i].clone();
            if has_guards {
                for (k, _) in &record.fields {
                    for g in &self.catalog.guards {
                        if g.collection == collection
                            && &g.field == k
                            && eval_pred(&g.pred, &before, now)
                        {
                            return Err(Error::runtime(format!(
                                "immutable field: {collection}.{k}"
                            )));
                        }
                    }
                }
            }
            let mut updated = before.clone();
            for (k, v) in &record.fields {
                updated.insert(k.clone(), value_cell(v, now));
            }
            if has_body && let Some(body) = row_text(&updated, "body") {
                let h = content_hash_inline(body);
                updated.insert("hash", Cell::SmallText(h));
            }
            if reembed {
                updated.remove("embedding");
                self.maybe_embed_rows(collection, std::slice::from_mut(&mut updated));
            }
            if has_fks {
                self.check_row_fks(collection, &updated)?;
            }
            self.store
                .index_update_row(collection, i, &before, &updated)?;
            if need_materialized {
                out.push(updated.clone());
            }
            if let Some(hnsw) = self.store.hnsw.get_mut(collection) {
                if let Some(Cell::Vec(v)) = updated.get("embedding") {
                    hnsw.insert(i, v.clone());
                } else if before.contains_key("embedding") {
                    hnsw.remove(i);
                }
            }
            // A changed `id` / `uri` rebuilds the row maps, which must see the new row.
            let identity_moved = row_text(&before, "id") != row_text(&updated, "id")
                || row_text(&before, "uri") != row_text(&updated, "uri");
            self.store
                .row_maps_sync_updated(collection, i, &before, &updated);
            self.store.collection_mut(collection)[i] = updated;
            if identity_moved {
                self.store.rebuild_row_maps_collection(collection);
            }
            self.store.fts_sync_at(collection, i, &before);
            if let Undo::Rows(events) | Undo::Mixed { events } = ctx.undo {
                events.push(RowUndo::Update(collection.to_string(), i, before));
            }
        }
        if ctx.track_written {
            mark_written(ctx, collection, &out);
        }
        let pack = if durable {
            Some(Pack::Update {
                collection: collection.to_string(),
                rows: if ctx.want_rows {
                    out.clone()
                } else {
                    std::mem::take(&mut out)
                },
            })
        } else {
            None
        };
        let ret_rows = if ctx.want_rows { out } else { Vec::new() };
        Ok((ret_rows, affected, pack))
    }

    fn delete_rows(
        &mut self,
        collection: &str,
        pred: &Pred,
        cas: Option<&str>,
        cas_each: bool,
        ctx: &mut PackCtx,
    ) -> Result<Vec<Row>, Error> {
        let now = if pred_needs_now(pred) { now_ms() } else { 0 };
        let append_only = self
            .catalog
            .collection(collection)
            .is_some_and(|c| c.append_only);
        if !append_only && cas.is_none() && !cas_each {
            return Err(Error::runtime("delete requires cas"));
        }
        let mut idxs = self.mutation_positions(collection, Some(pred), now);
        let rows = self.store.collection(collection);
        if idxs.is_empty() {
            return Err(Error::runtime("delete: no matching row"));
        }
        if !append_only {
            for i in &idxs {
                let got = row_text(&rows[*i], "hash").unwrap_or("");
                if cas_each {
                    cas_each_ok(collection, &rows[*i], got, ctx)?;
                } else {
                    let cas = cas.ok_or_else(|| Error::runtime("delete requires cas"))?;
                    if got != cas {
                        return Err(Error::runtime(format!(
                            "cas mismatch: expected {cas:?}, found {got:?}"
                        )));
                    }
                }
            }
        }
        idxs.sort_unstable();
        if self.catalog.fks.iter().any(|fk| fk.to_col == collection) {
            let deleting: Vec<Row> = idxs.iter().map(|&i| rows[i].clone()).collect();
            self.ensure_not_referenced(collection, &deleting)?;
        }
        let out = self.store.delete_rows_at(collection, &idxs);

        if let Undo::Rows(events) | Undo::Mixed { events } = ctx.undo {
            events.push(RowUndo::Delete(
                collection.to_string(),
                idxs.to_vec(),
                out.clone(),
            ));
        }
        mark_written(ctx, collection, &out);
        Ok(out)
    }

    fn exec_decl(&mut self, d: &Decl) -> Result<StmtExecOut, Error> {
        let durable = self.is_durable();
        self.catalog.apply_decl(d)?;
        match d {
            Decl::Col {
                name,
                append,
                fields,
            } => {
                self.store.collection_mut(name);
                let pack = Pack::SchemaCol {
                    name: name.clone(),
                    append: *append,
                    fields: fields
                        .iter()
                        .map(|(n, ty)| {
                            (
                                n.clone(),
                                match ty {
                                    TypeExpr::Named(t) => t.clone(),
                                    TypeExpr::Vec { model, .. } => format!("vec@{model}"),
                                },
                            )
                        })
                        .collect(),
                };
                self.store.apply_pack(&pack);
                self.plan_cache.clear();
                self.query_cache.clear();
                Ok((
                    Vec::new(),
                    Some(format!("col {name}")),
                    if durable { Some(pack) } else { None },
                    true,
                    0,
                ))
            }
            Decl::Rel { name, stub } => {
                let pack = Pack::SchemaRel {
                    name: name.clone(),
                    stub: *stub,
                    reverse_of: None,
                };
                self.store.apply_pack(&pack);
                self.plan_cache.clear();
                self.query_cache.clear();
                Ok((
                    Vec::new(),
                    Some(format!("rel {name}")),
                    if durable { Some(pack) } else { None },
                    true,
                    0,
                ))
            }
            Decl::RelReverse { name, of } => {
                let pack = Pack::SchemaRel {
                    name: name.clone(),
                    stub: false,
                    reverse_of: Some(of.clone()),
                };
                self.store.apply_pack(&pack);
                self.plan_cache.clear();
                self.query_cache.clear();
                Ok((
                    Vec::new(),
                    Some(format!("rel {name} = reverse {of}")),
                    if durable { Some(pack) } else { None },
                    true,
                    0,
                ))
            }
            Decl::Index {
                collection,
                unique,
                fields,
                pred,
            } => {
                if *unique {
                    self.assert_unique_index(collection, fields)?;
                }
                let pack = Pack::SchemaIndex {
                    collection: collection.clone(),
                    unique: *unique,
                    fields: fields.clone(),
                    pred_src: pred.as_ref().map(crate::plan::fmt_pred),
                };
                self.store.apply_pack(&pack);
                self.plan_cache.clear();
                self.query_cache.clear();
                Ok((
                    Vec::new(),
                    Some(format!("index {}", pack_index_label(collection, fields))),
                    if durable { Some(pack) } else { None },
                    true,
                    0,
                ))
            }
            Decl::Fk { .. } | Decl::Guard { .. } => Ok((
                Vec::new(),
                Some("schema: session only".into()),
                None,
                false,
                0,
            )),
            Decl::Filter {
                collection,
                pred: _,
                src,
            } => {
                let pack = Pack::SchemaFilter {
                    collection: collection.clone(),
                    pred_src: if src.is_empty() {
                        None
                    } else {
                        Some(src.clone())
                    },
                };
                self.store.apply_pack(&pack);
                self.plan_cache.clear();
                self.query_cache.clear();
                let msg = if src.is_empty() {
                    format!("unfilter {collection}")
                } else {
                    format!("filter {collection} {src}")
                };
                Ok((
                    Vec::new(),
                    Some(msg),
                    if durable { Some(pack) } else { None },
                    true,
                    0,
                ))
            }
            Decl::Owned { name, fields } => {
                let pack = Pack::SchemaOwned {
                    name: name.clone(),
                    fields: fields
                        .iter()
                        .map(|(n, ty)| {
                            (
                                n.clone(),
                                match ty {
                                    TypeExpr::Named(t) => t.clone(),
                                    TypeExpr::Vec { model, .. } => format!("vec@{model}"),
                                },
                            )
                        })
                        .collect(),
                };
                self.store.apply_pack(&pack);
                self.plan_cache.clear();
                self.query_cache.clear();
                Ok((
                    Vec::new(),
                    Some(format!("owned {name}")),
                    if durable { Some(pack) } else { None },
                    true,
                    0,
                ))
            }
        }
    }
}

thread_local! {
    static BATCH_DEDUP_SETS: std::cell::RefCell<(
        FxHashSet<&'static str>,
        FxHashSet<&'static str>,
    )> = std::cell::RefCell::new((
        FxHashSet::default(),
        FxHashSet::default(),
    ));
}

struct DedupSetsClearGuard<'a> {
    guard: std::cell::RefMut<'a, (FxHashSet<&'static str>, FxHashSet<&'static str>)>,
}

impl Drop for DedupSetsClearGuard<'_> {
    fn drop(&mut self) {
        self.guard.0.clear();
        self.guard.1.clear();
    }
}

fn check_batch_unique_records(
    records: &[Record],
    id_pos: usize,
    uri_pos_opt: Option<usize>,
    existing_ids: Option<&FxHashMap<Arc<str>, usize>>,
    docs_by_uri: &FxHashMap<Arc<str>, usize>,
    col_len: usize,
) -> Result<bool, Error> {
    BATCH_DEDUP_SETS.with(|cell| {
        let mut clear_guard = DedupSetsClearGuard {
            guard: cell.borrow_mut(),
        };
        let (batch_ids, batch_uris) = &mut *clear_guard.guard;
        batch_ids.clear();
        batch_ids.reserve(records.len());
        if uri_pos_opt.is_some() {
            batch_uris.clear();
            batch_uris.reserve(records.len());
        }
        let mut res = Ok(true);
        for record in records {
            if let Some((_, Value::String(id_arc))) = record.fields.get(id_pos) {
                let id = id_arc.as_ref();
                if id.is_empty() {
                    res = Ok(false);
                    break;
                }
                // SAFETY: `clear_guard` guarantees `batch_ids` and `batch_uris` are cleared
                // before `records` can go out of scope, even if a panic/unwind occurs.
                let id_static: &'static str = unsafe { std::mem::transmute(id) };
                if !batch_ids.insert(id_static)
                    || existing_ids.is_some_and(|m| m.get(id).is_some_and(|&i| i < col_len))
                {
                    res = Err(Error::runtime(format!("duplicate id: {id}")));
                    break;
                }
            } else {
                res = Ok(false);
                break;
            }
            if let Some(u_pos) = uri_pos_opt {
                if let Some((_, Value::String(uri_arc))) = record.fields.get(u_pos) {
                    let uri = uri_arc.as_ref();
                    let uri_static: &'static str = unsafe { std::mem::transmute(uri) };
                    if !batch_uris.insert(uri_static)
                        || (col_len > 0 && docs_by_uri.get(uri).is_some_and(|&i| i < col_len))
                    {
                        res = Err(Error::runtime(format!("duplicate uri: {uri}")));
                        break;
                    }
                } else {
                    res = Ok(false);
                    break;
                }
            }
        }
        res
    })
}

#[inline]
fn cell_lossless_kind(cell: &Cell) -> u8 {
    match cell {
        Cell::Null => 0,
        Cell::Text(_) | Cell::SmallText(_) => 1,
        Cell::Int(_) => 2,
        Cell::Float(_) => 3,
        Cell::Bool(_) => 4,
        Cell::Time(_) => 5,
        Cell::Vec(v) => {
            if v.is_empty() {
                0
            } else {
                6
            }
        }
    }
}

/// What [`build_insert_rows`] (or a column batch) established about the built rows.
#[derive(Clone, Copy)]
pub(crate) struct BuiltShape {
    /// Every row has the same fields in the same order.
    pub all_uniform: bool,
    /// Every row that needs an embedding already has one.
    pub all_embedded: bool,
    /// Some row has no `id` and needs a generated one.
    pub needs_id: bool,
    /// Every built row shares a single shape and uniform non-null column types.
    pub lossless_cols: bool,
    pub hashing_dim: Option<usize>,
    /// Time the rows were built at (edge targets such as `now` resolve to it).
    pub now: i64,
}

fn build_insert_rows(
    records: &[Record],
    now: i64,
    needs_embed: bool,
    hashing_dim: Option<usize>,
) -> (Vec<Row>, bool, bool, bool) {
    let n = records.len();
    if n == 0 {
        return (Vec::new(), false, !needs_embed, false);
    }
    if n >= 512 {
        use std::sync::atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering};
        struct BuildTask {
            records_ptr: usize,
            out_ptr: usize,
            n: usize,
            chunk_size: usize,
            total_chunks: usize,
            now: i64,
            needs_embed: bool,
            hashing_dim: Option<usize>,
            next_chunk: AtomicUsize,
            done_chunks: AtomicUsize,
            neg_flags: AtomicU8,
            panicked: AtomicBool,
            finished: AtomicBool,
            caller: std::thread::Thread,
        }
        fn run_build_chunks(task: &BuildTask, is_caller: bool) {
            loop {
                let c = task.next_chunk.fetch_add(1, Ordering::AcqRel);
                if c >= task.total_chunks {
                    break;
                }
                struct ChunkDone<'a>(&'a BuildTask, bool, bool);
                impl Drop for ChunkDone<'_> {
                    fn drop(&mut self) {
                        if self.2 {
                            self.0.panicked.store(true, Ordering::Release);
                        }
                        if self.0.done_chunks.fetch_add(1, Ordering::AcqRel) + 1
                            == self.0.total_chunks
                            && !self.1
                        {
                            self.0.finished.store(true, Ordering::Release);
                            self.0.caller.unpark();
                        }
                    }
                }
                let mut guard = ChunkDone(task, is_caller, true);
                let start = c * task.chunk_size;
                let end = (start + task.chunk_size).min(task.n);
                let recs = unsafe {
                    &std::slice::from_raw_parts(task.records_ptr as *const Record, task.n)
                        [start..end]
                };
                let out = unsafe {
                    &mut std::slice::from_raw_parts_mut(
                        task.out_ptr as *mut std::mem::MaybeUninit<Row>,
                        task.n,
                    )[start..end]
                };
                let (u, e, l) = build_batch_shape_rows_into(
                    out,
                    recs,
                    task.now,
                    task.needs_embed,
                    task.hashing_dim,
                    task.n,
                );
                let mask = u8::from(!u) | (u8::from(!e) << 1) | (u8::from(!l) << 2);
                if mask != 0 {
                    task.neg_flags.fetch_or(mask, Ordering::Relaxed);
                }
                guard.2 = false;
            }
        }
        static BUILD_WORKERS: std::sync::OnceLock<
            Vec<std::sync::mpsc::SyncSender<std::sync::Arc<BuildTask>>>,
        > = std::sync::OnceLock::new();
        let workers = BUILD_WORKERS.get_or_init(|| {
            let threads = std::thread::available_parallelism()
                .map(|p| p.get())
                .unwrap_or(1)
                .min(8);
            if threads <= 1 {
                return Vec::new();
            }
            let num_workers = threads - 1;
            let mut txs = Vec::with_capacity(num_workers);
            for idx in 0..num_workers {
                let (tx, rx) =
                    std::sync::mpsc::sync_channel::<std::sync::Arc<BuildTask>>(16);
                if std::thread::Builder::new()
                    .name(format!("lin-build-{idx}"))
                    .spawn(move || {
                        while let Ok(task) = rx.recv() {
                            run_build_chunks(&task, false);
                        }
                    })
                    .is_ok()
                {
                    txs.push(tx);
                }
            }
            txs
        });
        if !workers.is_empty() {
            let chunk_size = 64usize;
            let total_chunks = n.div_ceil(chunk_size);
            let mut rows: Vec<Row> = Vec::with_capacity(n);
            let spare = &mut rows.spare_capacity_mut()[..n];
            let task = std::sync::Arc::new(BuildTask {
                records_ptr: records.as_ptr() as usize,
                out_ptr: spare.as_mut_ptr() as usize,
                n,
                chunk_size,
                total_chunks,
                now,
                needs_embed,
                hashing_dim,
                next_chunk: AtomicUsize::new(0),
                done_chunks: AtomicUsize::new(0),
                neg_flags: AtomicU8::new(0),
                panicked: AtomicBool::new(false),
                finished: AtomicBool::new(false),
                caller: std::thread::current(),
            });
            struct WaitBuildGuard<'a>(&'a BuildTask);
            impl Drop for WaitBuildGuard<'_> {
                fn drop(&mut self) {
                    if std::thread::panicking() {
                        self.0
                            .next_chunk
                            .store(self.0.total_chunks, Ordering::Release);
                    }
                    while self.0.done_chunks.load(Ordering::Acquire) < self.0.total_chunks {
                        if !self.0.finished.load(Ordering::Acquire) {
                            std::thread::park_timeout(std::time::Duration::from_micros(50));
                        }
                    }
                }
            }
            let _wait_guard = WaitBuildGuard(&task);
            let wake = (total_chunks - 1).min(workers.len());
            for worker in workers.iter().take(wake) {
                let _ = worker.try_send(std::sync::Arc::clone(&task));
            }
            run_build_chunks(&task, true);
            for _ in 0..64 {
                if task.done_chunks.load(Ordering::Acquire) >= total_chunks {
                    break;
                }
                std::hint::spin_loop();
            }
            while task.done_chunks.load(Ordering::Acquire) < total_chunks {
                if !task.finished.load(Ordering::Acquire) {
                    std::thread::park_timeout(std::time::Duration::from_micros(50));
                }
            }
            if task.panicked.load(Ordering::Acquire) {
                panic!("build rows worker panicked");
            }
            unsafe {
                rows.set_len(n);
            }
            let neg = task.neg_flags.load(Ordering::Acquire);
            let mut all_uniform = (neg & 1) == 0;
            let all_embedded = (neg & 2) == 0;
            let mut lossless_cols = (neg & 4) == 0;
            if (all_uniform || lossless_cols) && let Some(fs) = rows[0].shape_arc() {
                let first_cells = rows[0].cells_slice();
                for c in 1..total_chunks {
                    let bi = c * chunk_size;
                    let same_shape = rows[bi].shape_arc().is_some_and(|s| Arc::ptr_eq(s, fs));
                    all_uniform &= same_shape;
                    if lossless_cols {
                        lossless_cols = same_shape
                            && rows[bi].cells_slice().len() == first_cells.len()
                            && rows[bi]
                                .cells_slice()
                                .iter()
                                .zip(first_cells)
                                .all(|(a, b)| cell_lossless_kind(a) == cell_lossless_kind(b));
                    }
                }
            } else {
                all_uniform = false;
                lossless_cols = false;
            }
            return (rows, all_uniform, all_embedded, lossless_cols);
        }
    }

    let mut rows: Vec<Row> = Vec::with_capacity(n);
    let spare = &mut rows.spare_capacity_mut()[..n];
    let (all_uniform, all_embedded, lossless_cols) =
        build_batch_shape_rows_into(spare, records, now, needs_embed, hashing_dim, n);
    unsafe {
        rows.set_len(n);
    }
    (rows, all_uniform, all_embedded, lossless_cols)
}

fn build_batch_shape_rows_into(
    out: &mut [std::mem::MaybeUninit<Row>],
    records: &[Record],
    now: i64,
    needs_embed: bool,
    hashing_dim: Option<usize>,
    total_n: usize,
) -> (bool, bool, bool) {
    let Some(first) = records.first() else {
        return (false, !needs_embed, false);
    };
    if total_n < 128 || first.fields.is_empty() || first.fields.len() > 32 {
        for (slot_out, record) in out.iter_mut().zip(records) {
            slot_out.write(record_row(record, now));
        }
        return (false, !needs_embed, false);
    }
    let body_idx = first.fields.iter().rposition(|(k, _)| k == "body");
    let has_hash = first.fields.iter().any(|(k, _)| k == "hash");
    let needs_hash = !has_hash && body_idx.is_some();
    let has_embedding = first.fields.iter().any(|(k, _)| k == "embedding");
    let can_inline_embed = needs_embed && !has_embedding && hashing_dim.is_some();
    let title_idx = first.fields.iter().rposition(|(k, _)| k == "title");
    let snippet_idx = first.fields.iter().rposition(|(k, _)| k == "snippet");

    let mut template = first
        .fields
        .iter()
        .map(|(key, _)| (key.clone(), Cell::Null))
        .collect::<Row>();
    if needs_hash {
        template.insert("hash", Cell::Null);
    }
    if needs_embed {
        template.insert("embedding", Cell::Null);
    }
    let Some(shape) = template.shape_arc().cloned() else {
        return (false, !needs_embed, false);
    };

    #[derive(Clone, Copy)]
    enum ShapeSlot {
        Source(usize),
        Body(usize),
        Hash(usize),
        Embed,
    }

    let slots = template
        .keys()
        .map(|key| {
            if key == "hash" && needs_hash {
                ShapeSlot::Hash(body_idx.unwrap())
            } else if key == "embedding" && needs_embed {
                ShapeSlot::Embed
            } else {
                let src = first
                    .fields
                    .iter()
                    .rposition(|(field, _)| field == key)
                    .unwrap();
                if key == "body" {
                    ShapeSlot::Body(src)
                } else {
                    ShapeSlot::Source(src)
                }
            }
        })
        .collect::<Vec<_>>();
    let width = slots.len();
    if width > 32 {
        for (slot_out, record) in out.iter_mut().zip(records) {
            slot_out.write(record_row(record, now));
        }
        return (false, !needs_embed, false);
    }

    let mut all_same = true;
    let mut all_embedded = !needs_embed || can_inline_embed;
    let mut lossless_cols = true;
    let mut first_kinds = [0u8; 32];
    let mut have_first_kinds = false;

    let mut process_rows = |mut embed_batch: Option<&mut crate::embed::HashingBatch<'_>>| {
        for (slot_out, record) in out.iter_mut().zip(records) {
            let same_shape = record.fields.len() == first.fields.len()
                && record
                    .fields
                    .iter()
                    .zip(&first.fields)
                    .all(|(a, b)| a.0 == b.0);
            if !same_shape {
                all_same = false;
                lossless_cols = false;
                if needs_embed {
                    all_embedded = false;
                }
                slot_out.write(record_row(record, now));
                continue;
            }
            let mut emb_vec = if let Some(batch) = embed_batch.as_deref_mut() {
                let t_opt = title_idx.map(|i| &record.fields[i].1);
                let b_opt = body_idx.map(|i| &record.fields[i].1);
                let s_opt = snippet_idx.map(|i| &record.fields[i].1);
                let t_str = match t_opt {
                    Some(Value::String(s)) => Some(s.as_ref()),
                    None => None,
                    _ => {
                        all_embedded = false;
                        None
                    }
                };
                let b_str = match b_opt {
                    Some(Value::String(s)) => Some(s.as_ref()),
                    None => None,
                    _ => {
                        all_embedded = false;
                        None
                    }
                };
                let s_str = match s_opt {
                    Some(Value::String(s)) => Some(s.as_ref()),
                    None => None,
                    _ => {
                        all_embedded = false;
                        None
                    }
                };
                if all_embedded && t_str.or(b_str).or(s_str).is_some_and(|s| !s.is_empty()) {
                    Some(batch.embed_parts([t_str, b_str, s_str]))
                } else {
                    all_embedded = false;
                    None
                }
            } else {
                None
            };

            macro_rules! eval_slot_cell {
                ($slot:expr) => {
                    match $slot {
                        ShapeSlot::Source(src) => value_cell(&record.fields[*src].1, now),
                        ShapeSlot::Body(src) => match &record.fields[*src].1 {
                            Value::String(s) => cell_from_arc_inline(s),
                            other => value_cell(other, now),
                        },
                        ShapeSlot::Hash(b_src) => {
                            if let Value::String(body) = &record.fields[*b_src].1 {
                                Cell::SmallText(content_hash_inline(body.as_ref()))
                            } else {
                                all_same = false;
                                lossless_cols = false;
                                Cell::Null
                            }
                        }
                        ShapeSlot::Embed => match emb_vec.take() {
                            Some(v) => Cell::Vec(v),
                            None => Cell::Null,
                        },
                    }
                };
            }

            if width <= 8 {
                let mut cells: smallvec::SmallVec<[Cell; 8]> =
                    smallvec::SmallVec::with_capacity(width);
                if !have_first_kinds {
                    for (ci, slot) in slots.iter().enumerate() {
                        let cell = eval_slot_cell!(slot);
                        let k = cell_lossless_kind(&cell);
                        first_kinds[ci] = k;
                        if k == 0 {
                            lossless_cols = false;
                        }
                        cells.push(cell);
                    }
                    have_first_kinds = true;
                } else {
                    for (ci, slot) in slots.iter().enumerate() {
                        let cell = eval_slot_cell!(slot);
                        if lossless_cols && cell_lossless_kind(&cell) != first_kinds[ci] {
                            lossless_cols = false;
                        }
                        cells.push(cell);
                    }
                }
                slot_out.write(Row::from_parts(Arc::clone(&shape), cells));
            } else {
                let mut cells = Vec::with_capacity(width);
                if !have_first_kinds {
                    for (ci, slot) in slots.iter().enumerate() {
                        let cell = eval_slot_cell!(slot);
                        let k = cell_lossless_kind(&cell);
                        first_kinds[ci] = k;
                        if k == 0 {
                            lossless_cols = false;
                        }
                        cells.push(cell);
                    }
                    have_first_kinds = true;
                } else {
                    for (ci, slot) in slots.iter().enumerate() {
                        let cell = eval_slot_cell!(slot);
                        if lossless_cols && cell_lossless_kind(&cell) != first_kinds[ci] {
                            lossless_cols = false;
                        }
                        cells.push(cell);
                    }
                }
                slot_out.write(Row::from_vec_parts(Arc::clone(&shape), cells));
            }
        }
    };
    if let Some(dim) = hashing_dim.filter(|_| can_inline_embed) {
        crate::embed::with_hashing_scratch(dim, |batch| process_rows(Some(batch)));
    } else {
        process_rows(None);
    }
    (all_same, all_embedded, lossless_cols && all_same)
}

#[cfg(test)]
pub(super) fn append_batch_shape_rows(
    rows: &mut Vec<Row>,
    records: &[Record],
    now: i64,
    needs_embed: bool,
) -> bool {
    let n = records.len();
    if n == 0 {
        return false;
    }
    rows.reserve(n);
    let old_len = rows.len();
    let spare = &mut rows.spare_capacity_mut()[..n];
    let (all_same, _, _) = build_batch_shape_rows_into(spare, records, now, needs_embed, None, n);
    unsafe {
        rows.set_len(old_len + n);
    }
    all_same
}

pub(crate) fn record_row(record: &Record, now: i64) -> Row {
    let mut row = acquire_row();
    for (k, v) in &record.fields {
        row.insert(k.clone(), value_cell(v, now));
    }
    row
}
