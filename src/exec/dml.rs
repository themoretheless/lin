use std::collections::BTreeMap;
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
    pub(crate) snap: FxHashMap<String, FxHashMap<String, String>>,
    pub(crate) written: FxHashMap<String, FxHashSet<String>>,
    pub(crate) undo: &'a mut Undo,
}

fn mark_written(ctx: &mut PackCtx, collection: &str, rows: &[Row]) {
    if !ctx.track_written || rows.is_empty() {
        return;
    }
    let set = match ctx.written.get_mut(collection) {
        Some(s) => s,
        None => ctx.written.entry(collection.to_string()).or_default(),
    };
    let mut id_slot = crate::store::row::FieldSlot::new("id");
    for r in rows {
        if let Some(id) = id_slot.get(r).and_then(Cell::text)
            && !set.contains(id)
        {
            set.insert(id.to_string());
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

fn cas_each_ok(
    collection: &str,
    row: &Row,
    id_slot: &mut crate::store::row::FieldSlot<'_>,
    got: &str,
    ctx: &PackCtx,
) -> Result<(), Error> {
    if !ctx.track_written {
        return Ok(());
    }
    let Some(id) = id_slot.get(row).and_then(Cell::text) else {
        return Ok(());
    };
    if ctx
        .written
        .get(collection)
        .is_some_and(|ids| ids.contains(id))
    {
        return Err(Error::runtime(format!(
            "cas mismatch: row {id} changed mid-pack"
        )));
    }
    if !ctx.snap.is_empty()
        && let Some(expect) = ctx.snap.get(collection).and_then(|m| m.get(id))
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
        // CAS-each tracking is only required when multiple statements execute in the same program;
        // a full hash snapshot is only needed if `Restore` / `Unpin` / `IdbPush` could mutate rows
        // outside `mark_written`.
        let need_snap = stmts.len() > 1
            && stmts.iter().any(|s| {
                matches!(
                    s,
                    Stmt::Update { cas_each: true, .. } | Stmt::Delete { cas_each: true, .. }
                )
            });
        let need_full_hash_snap = need_snap
            && stmts.iter().any(|s| {
                matches!(
                    s,
                    Stmt::Restore { .. } | Stmt::Unpin { .. } | Stmt::IdbPush
                )
            });
        let mut ctx = PackCtx {
            track_written: need_snap,
            allow_preencode: allow_preencode && stmts.len() == 1 && self.is_durable(),
            want_rows,
            prepared_encoding: None,
            snap: if need_full_hash_snap {
                self.hash_snapshot()
            } else {
                FxHashMap::default()
            },
            written: FxHashMap::default(),
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
                last_msg = None;
                last_affected = rows.len();
                if si == last_idx {
                    last_rows = if want_rows { rows } else { Vec::new() };
                } else {
                    last_rows.clear();
                    bindings.insert(name.clone(), rows);
                }
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

    fn hash_snapshot(&self) -> FxHashMap<String, FxHashMap<String, String>> {
        let mut m: FxHashMap<String, FxHashMap<String, String>> = FxHashMap::default();
        for (col, rows) in &self.store.collections {
            let mut col_map = FxHashMap::default();
            col_map.reserve(rows.len());
            let mut id_slot = crate::store::row::FieldSlot::new("id");
            let mut hash_slot = crate::store::row::FieldSlot::new("hash");
            for r in rows {
                if let Some(id) = id_slot.get(r).and_then(Cell::text) {
                    col_map.insert(
                        id.to_string(),
                        hash_slot
                            .get(r)
                            .and_then(Cell::text)
                            .unwrap_or("")
                            .to_string(),
                    );
                }
            }
            m.insert(col.clone(), col_map);
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
                let now = if edges
                    .iter()
                    .any(|e| value_needs_now(&e.from) || value_needs_now(&e.to))
                {
                    now_ms()
                } else {
                    0
                };
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
                    let (from_s, to_s, changed) =
                        self.append_edge(&e.rel, &e.from, &e.to, now);
                    if ctx.want_rows {
                        rows.push(edge_row(&e.rel, &from_s, &to_s));
                    }
                    if changed {
                        changed_n += 1;
                        if durable {
                            bulk_rel.push(e.rel.clone());
                            bulk_from.push(from_s);
                            bulk_to.push(to_s);
                        }
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
                        && let Some((magic, fields)) = crate::persist::with_dictionary_writer_opt(
                            persist.wal_dictionary,
                            || {
                                crate::persist::encode_insert_rows_direct(
                                    self.store.r#gen.wrapping_add(1),
                                    self.store.next_id,
                                    collection,
                                    inserted,
                                    &mut persist.encode_buf,
                                )
                            },
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
                let (mut rows, affected) =
                    self.delete_rows(collection, pred, cas.as_deref(), *cas_each, ctx, durable)?;
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
                self.clear_plan_cache();
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

    #[inline]
    fn append_edge(&mut self, rel: &str, from: &Value, to: &Value, now: i64) -> (String, String, bool) {
        let from = value_text(from, now);
        let to = value_text(to, now);
        let changed = self.store.append_edge_parts(rel, &from, &to);
        (from, to, changed)
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

    /// Slot-accelerated, deduplicated batch FK checker that can be stepped row-by-row
    /// (preserving exact first-offending-row error ordering across `duplicate id`, unique
    /// indexes, and foreign keys) while probing `fk_target_exists` at most once per distinct key.
    pub(crate) fn batch_fk_watchers<'a>(
        &'a self,
        collection: &str,
    ) -> smallvec::SmallVec<
        [(
            &'a crate::catalog::Fk,
            crate::store::row::FieldSlot<'a>,
            FxHashSet<String>,
        ); 2],
    > {
        self.catalog
            .fks
            .iter()
            .filter(|fk| fk.from_col == collection)
            .map(|fk| {
                (
                    fk,
                    crate::store::row::FieldSlot::new(&fk.from_field),
                    FxHashSet::default(),
                )
            })
            .collect()
    }

    #[inline]
    pub(crate) fn check_row_fks_dedup<'a>(
        &self,
        collection: &str,
        row: &Row,
        watchers: &mut [(
            &'a crate::catalog::Fk,
            crate::store::row::FieldSlot<'a>,
            FxHashSet<String>,
        )],
    ) -> Result<(), Error> {
        for (fk, slot, verified) in watchers.iter_mut() {
            let Some(val) = slot.get(row).and_then(Cell::text) else {
                continue;
            };
            if val.is_empty() || verified.contains(val) {
                continue;
            }
            if !self.fk_target_exists(&fk.to_col, &fk.to_field, val) {
                return Err(Error::runtime(format!(
                    "fk: {collection}.{} → {}.{} missing {val}",
                    fk.from_field, fk.to_col, fk.to_field
                )));
            }
            verified.insert(val.to_string());
        }
        Ok(())
    }

    /// Refuse to delete rows a foreign key still points at. `deleting` are the rows about
    /// to leave `collection`; a referencing row that is itself being deleted (a key into
    /// its own collection) does not count.
    pub(crate) fn ensure_not_referenced(&self, collection: &str, deleting: &[Row]) -> Result<(), Error> {
        self.ensure_not_referenced_iter(collection, || deleting.iter())
    }

    pub(crate) fn ensure_not_referenced_idxs(
        &self,
        collection: &str,
        rows: &[Row],
        idxs: &[usize],
    ) -> Result<(), Error> {
        self.ensure_not_referenced_iter(collection, || idxs.iter().map(|&i| &rows[i]))
    }

    fn ensure_not_referenced_iter<'a, I, F>(
        &self,
        collection: &str,
        deleting_rows: F,
    ) -> Result<(), Error>
    where
        I: Iterator<Item = &'a Row>,
        F: Fn() -> I,
    {
        for fk in &self.catalog.fks {
            if fk.to_col != collection {
                continue;
            }
            let mut to_slot = crate::store::row::FieldSlot::new(&fk.to_field);
            let targets: FxHashSet<&str> = deleting_rows()
                .filter_map(|r| to_slot.get(r).and_then(Cell::text))
                .filter(|v| !v.is_empty())
                .collect();
            if targets.is_empty() {
                continue;
            }
            let self_ref = fk.from_col == collection;
            let deleted_ids: FxHashSet<&str> = if self_ref {
                let mut id_slot = crate::store::row::FieldSlot::new("id");
                deleting_rows()
                    .filter_map(|r| id_slot.get(r).and_then(Cell::text))
                    .collect()
            } else {
                FxHashSet::default()
            };
            let referenced = if self.store.is_disk_backed(&fk.from_col) {
                // No in-memory rows to scan: ask the engine per value. A self-reference
                // inside the deleted set cannot be told apart here, so it counts.
                targets
                    .iter()
                    .find(|v| self.store.disk_has_key(&fk.from_col, &fk.from_field, v))
                    .map(|v| (*v).to_string())
            } else if fk.from_field == "id" && !self_ref {
                targets
                    .iter()
                    .find(|&&v| self.store.get_by_id(&fk.from_col, v).is_some())
                    .map(|v| (*v).to_string())
            } else if let Some(live_idx) = self.store.indexes.values().find(|ix| {
                ix.def.collection == fk.from_col
                    && ix.def.pred.is_none()
                    && ix.def.fields.first().map(String::as_str) == Some(fk.from_field.as_str())
            }) {
                let from_rows = self.store.collection(&fk.from_col);
                let mut id_slot = crate::store::row::FieldSlot::new("id");
                let mut found: Option<String> = None;
                for &v in &targets {
                    let k = crate::index::LiveIndex::single_cell_key(&Cell::from_text(v));
                    let hit = if live_idx.def.fields.len() == 1 {
                        live_idx.forward.get(&k).is_some_and(|pl| {
                            if !self_ref {
                                !pl.is_empty()
                            } else {
                                let mut non_deleted = false;
                                pl.for_each_ordered(false, |pos| {
                                    let is_del = from_rows
                                        .get(pos)
                                        .and_then(|r| id_slot.get(r))
                                        .and_then(Cell::text)
                                        .is_some_and(|id| deleted_ids.contains(id));
                                    if !is_del {
                                        non_deleted = true;
                                        return false;
                                    }
                                    true
                                });
                                non_deleted
                            }
                        })
                    } else {
                        let part = k.0[0].clone();
                        let width = live_idx.def.fields.len();
                        let mut lo = smallvec::SmallVec::with_capacity(width);
                        let mut hi = smallvec::SmallVec::with_capacity(width);
                        lo.push(part.clone());
                        hi.push(part);
                        for _ in 1..width {
                            lo.push(crate::index::IndexPart::Min);
                            hi.push(crate::index::IndexPart::Max);
                        }
                        live_idx
                            .forward
                            .range(crate::index::IndexKey(lo)..=crate::index::IndexKey(hi))
                            .any(|(_, pl)| {
                                if !self_ref {
                                    !pl.is_empty()
                                } else {
                                    let mut non_deleted = false;
                                    pl.for_each_ordered(false, |pos| {
                                        let is_del = from_rows
                                            .get(pos)
                                            .and_then(|r| id_slot.get(r))
                                            .and_then(Cell::text)
                                            .is_some_and(|id| deleted_ids.contains(id));
                                        if !is_del {
                                            non_deleted = true;
                                            return false;
                                        }
                                        true
                                    });
                                    non_deleted
                                }
                            })
                    };
                    if hit {
                        found = Some(v.to_string());
                        break;
                    }
                }
                found
            } else if !self_ref
                && fk.from_col == "orders"
                && fk.from_field == "user_id"
                && self.store.orders_soa_ready()
            {
                self.store
                    .orders_user_id()
                    .iter()
                    .find(|uid| targets.contains(uid.as_ref()))
                    .map(|uid| uid.to_string())
            } else {
                let mut from_slot = crate::store::row::FieldSlot::new(&fk.from_field);
                let mut id_slot = crate::store::row::FieldSlot::new("id");
                self.store.collection(&fk.from_col).iter().find_map(|r| {
                    let v = from_slot.get(r).and_then(Cell::text)?;
                    if !targets.contains(v) {
                        return None;
                    }
                    if self_ref
                        && id_slot
                            .get(r)
                            .and_then(Cell::text)
                            .is_some_and(|id| deleted_ids.contains(id))
                    {
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
        if to_col == "docs" && to_field == "uri" {
            return self.store.get_by_uri(val).is_some();
        }
        if let Some(live_idx) = self.store.indexes.values().find(|ix| {
            ix.def.collection == to_col
                && ix.def.pred.is_none()
                && ix.def.fields.first().map(String::as_str) == Some(to_field)
        }) {
            let k = crate::index::LiveIndex::single_cell_key(&Cell::from_text(val));
            if live_idx.def.fields.len() == 1 {
                return live_idx.forward.get(&k).is_some_and(|pl| !pl.is_empty());
            }
            let part = k.0[0].clone();
            let width = live_idx.def.fields.len();
            let mut lo = smallvec::SmallVec::with_capacity(width);
            let mut hi = smallvec::SmallVec::with_capacity(width);
            lo.push(part.clone());
            hi.push(part);
            for _ in 1..width {
                lo.push(crate::index::IndexPart::Min);
                hi.push(crate::index::IndexPart::Max);
            }
            return live_idx
                .forward
                .range(crate::index::IndexKey(lo)..=crate::index::IndexKey(hi))
                .any(|(_, pl)| !pl.is_empty());
        }
        if to_col == "orders" && to_field == "user_id" && self.store.orders_soa_ready() {
            return self.store.orders_user_id().iter().any(|u| u.as_ref() == val);
        }
        if to_col == "users" && to_field == "email" && self.store.users_soa_ready() {
            return self.store.users_email().iter().any(|e| e.as_ref() == val);
        }
        let mut slot = crate::store::row::FieldSlot::new(to_field);
        self.store
            .collection(to_col)
            .iter()
            .any(|r| slot.get(r).and_then(Cell::text) == Some(val))
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
                row.insert(k.as_str(), value_cell(v, now));
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
                self.maybe_embed_rows(collection, std::slice::from_mut(&mut row))?;
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
            if !self.store.indexes.is_empty() {
                self.store.index_insert_row(collection, start, &row)?;
            }
            if !self.store.fts.is_empty() && self.store.fts.contains_key(collection) {
                self.store.fts_insert_row(collection, start, &row);
            }
            if !self.store.hnsw.is_empty()
                && let Some(Cell::Vec(v)) = row.get("embedding")
                && let Some(hnsw) = self.store.hnsw.get_mut(collection)
            {
                hnsw.insert(start, Arc::clone(v));
            }
            self.store.row_maps_register_row(collection, start, &row);
            self.store.update_col_count(collection, 1);
            if caller_wants_rows {
                let ret = row.clone();
                self.store.collection_append_mut(collection).push(row);
                return Ok((vec![ret], new_edges, None));
            } else {
                self.store.collection_append_mut(collection).push(row);
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
        } else if records.is_none()
            && all_uniform
            && !needs_id
            && !has_fks
            && let Some(row_shape) = built.first().and_then(Row::shape_arc)
            && let Some(id_slot) = row_shape.names().iter().position(|k| k == "id")
            && (!row_shape.names().iter().any(|k| k == "body")
                || row_shape.names().iter().any(|k| k == "hash"))
            && (!is_docs
                || (row_shape.names().iter().any(|k| k == "uri")
                    && self.store.docs_moved_ids.is_empty()
                    && self.store.docs_moved_idxs.is_empty()
                    && self.store.retired_doc_ids.is_empty()))
        {
            let uri_slot_opt = if is_docs {
                row_shape.names().iter().position(|k| k == "uri")
            } else {
                None
            };
            let hash_slot_opt = if row_shape.names().iter().any(|k| k == "body") {
                row_shape.names().iter().position(|k| k == "hash")
            } else {
                None
            };
            let existing_ids = if !store_empty {
                self.store.by_id.get(collection)
            } else {
                None
            };
            check_batch_unique_rows(
                &built,
                id_slot,
                uri_slot_opt,
                hash_slot_opt,
                existing_ids,
                &self.store.docs_by_uri,
                col_len,
            )?
        } else {
            false
        };
        if !fast_checked {
            // Prepare any IDs/hashes first before immutable catalog borrow for FK watchers:
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
            }
            let mut batch_ids: FxHashSet<&str> = FxHashSet::default();
            batch_ids.reserve(n);
            let mut batch_uris: FxHashSet<&str> = FxHashSet::default();
            if is_docs {
                batch_uris.reserve(n);
            }
            let mut fk_watchers = if has_fks {
                self.batch_fk_watchers(collection)
            } else {
                smallvec::SmallVec::new()
            };
            for row in &built {
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
                if !fk_watchers.is_empty() {
                    self.check_row_fks_dedup(collection, row, &mut fk_watchers)?;
                }
            }
        }
        if !all_embedded {
            self.maybe_embed_rows(collection, &mut built)?;
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
        let staged = if allow_preencode
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
                    let wal_dict = self.persist.as_ref().and_then(|p| p.wal_dictionary);
                    crate::persist::with_dictionary_writer_opt(wal_dict, || {
                        crate::persist::prepare_insert_columns_ext(
                            generation,
                            next_id,
                            collection,
                            &shared,
                            uniform_keys,
                            Vec::new(),
                        )
                    })
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
            let wal_dict = self.persist.as_ref().and_then(|p| p.wal_dictionary);
            let mut encode_buf = self
                .persist
                .as_mut()
                .map(|p| std::mem::take(&mut p.encode_buf))
                .unwrap_or_default();
            let built_ref: &[Row] = &built;
            let (direct_res, maint_res) = crate::work_steal::steal2(
                crate::work_steal::StealLane::DirectEnc,
                move || {
                    let res = crate::persist::with_dictionary_writer_opt(wal_dict, || {
                        crate::persist::encode_insert_rows_direct_ext(
                            generation,
                            next_id,
                            collection,
                            built_ref,
                            uniform_keys,
                            &mut encode_buf,
                        )
                    });
                    (res, encode_buf)
                },
                || self.maintain_insert_slab(collection, start, built_ref),
            );
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
                .collection_append_mut(collection)
                .extend(built.iter().cloned());
            Ok((built, new_edges, staged))
        } else {
            self.store.collection_append_mut(collection).extend(built);
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
            && !self.store.is_disk_backed(collection)
        {
            if let Some(idxs) = self.interval_index_select(collection, pred, now) {
                let mut slots = super::eval::PredSlots::new(pred);
                return idxs
                    .into_iter()
                    .filter(|&i| {
                        self.store
                            .get_by_idx(collection, i)
                            .is_some_and(|row| slots.eval(row, now))
                    })
                    .collect();
            }
            if let Some((uses, covered)) =
                crate::index::pick_index_with_coverage(&self.catalog, collection, pred)
                && let Some(idxs) = self.store.index_seek(collection, &uses, now)
            {
                if covered {
                    return idxs.into_iter().collect();
                }
                let mut slots = super::eval::PredSlots::new(pred);
                return idxs
                    .into_iter()
                    .filter(|&i| {
                        self.store
                            .get_by_idx(collection, i)
                            .is_some_and(|row| slots.eval(row, now))
                    })
                    .collect();
            }
            if let Some(idxs) = self.mirror_select(collection, pred, now) {
                return idxs.into_iter().collect();
            }
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
        let has_guards = self
            .catalog
            .guards
            .iter()
            .any(|g| g.collection == collection);
        let needs_now = pred.is_some_and(pred_needs_now) || record_needs_now(record) || has_guards;
        let now = if needs_now { now_ms() } else { 0 };

        let idxs = self.mutation_positions(collection, pred, now);
        if idxs.is_empty() {
            return Err(Error::runtime("update: no matching row"));
        }
        {
            let rows = self.store.collection(collection);
            let mut hash_slot = crate::store::row::FieldSlot::new("hash");
            let mut id_slot = crate::store::row::FieldSlot::new("id");
            for i in &idxs {
                let row = &rows[*i];
                let got = hash_slot.get(row).and_then(Cell::text).unwrap_or("");
                if cas_each {
                    cas_each_ok(collection, row, &mut id_slot, got, ctx)?;
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
        let reembed = self.update_needs_reembed(&record.fields);
        let has_fks = self.catalog.fks.iter().any(|fk| fk.from_col == collection);
        let has_fts = !self.store.fts.is_empty() && self.store.fts.contains_key(collection);
        let affected = idxs.len();
        let need_materialized = ctx.want_rows || durable || ctx.track_written;
        let mut out = if need_materialized {
            Vec::with_capacity(affected)
        } else {
            Vec::new()
        };

        for i in idxs {
            let updated = {
                let before_ref = &self.store.collection(collection)[i];
                if has_guards {
                    self.check_row_update_guards(collection, before_ref, &record.fields, now)?;
                }
                self.patch_updated_row(
                    collection,
                    before_ref.clone(),
                    &record.fields,
                    now,
                    has_body,
                    reembed,
                )?
            };
            if has_fks {
                self.check_row_fks(collection, &updated)?;
            }
            if !self.store.indexes.is_empty() {
                let crate::store::Store {
                    collections,
                    indexes,
                    ..
                } = &mut self.store;
                let before_ref = &collections.get(collection).unwrap()[i];
                let mut applied = 0usize;
                let mut err_msg: Option<String> = None;
                for idx in indexes.values_mut() {
                    if idx.def.collection == collection {
                        if let Err(e) = idx.update_row(i, before_ref, &updated) {
                            err_msg = Some(e);
                            break;
                        }
                        applied += 1;
                    }
                }
                if let Some(e) = err_msg {
                    for idx in indexes.values_mut() {
                        if applied == 0 {
                            break;
                        }
                        if idx.def.collection == collection {
                            let _ = idx.update_row(i, &updated, before_ref);
                            applied -= 1;
                        }
                    }
                    return Err(Error::runtime(e));
                }
            }
            if need_materialized {
                out.push(updated.clone());
            }
            if !self.store.hnsw.is_empty() {
                let crate::store::Store {
                    collections, hnsw, ..
                } = &mut self.store;
                if let Some(h) = hnsw.get_mut(collection) {
                    if let Some(Cell::Vec(v)) = updated.get("embedding") {
                        h.insert(i, Arc::clone(v));
                    } else if collections
                        .get(collection)
                        .is_some_and(|c| c[i].contains_key("embedding"))
                    {
                        h.remove(i);
                    }
                }
            }
            // A changed `id` / `uri` or stale slot rebuilds the row maps, which must see the new row.
            let (identity_moved, stale_slot) = {
                let before_ref = &self.store.collection(collection)[i];
                let new_id = row_text(&updated, "id");
                let moved = row_text(before_ref, "id") != new_id
                    || row_text(before_ref, "uri") != row_text(&updated, "uri");
                let stale = !moved
                    && new_id.is_some_and(|key| self.store.row_index(collection, key) != Some(i));
                (moved, stale)
            };
            let before = if identity_moved || stale_slot {
                let b = std::mem::replace(&mut self.store.collection_mut(collection)[i], updated);
                self.store.rebuild_row_maps_collection(collection);
                b
            } else {
                let b = std::mem::take(&mut self.store.collection_mut(collection)[i]);
                self.store
                    .row_maps_sync_updated(collection, i, &b, &updated);
                self.store.collections.get_mut(collection).unwrap()[i] = updated;
                b
            };
            if has_fts {
                self.store.fts_sync_at(collection, i, &before);
            }
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
        durable: bool,
    ) -> Result<(Vec<Row>, usize), Error> {
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
            let mut hash_slot = crate::store::row::FieldSlot::new("hash");
            let mut id_slot = crate::store::row::FieldSlot::new("id");
            for i in &idxs {
                let row = &rows[*i];
                let got = hash_slot.get(row).and_then(Cell::text).unwrap_or("");
                if cas_each {
                    cas_each_ok(collection, row, &mut id_slot, got, ctx)?;
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
            self.ensure_not_referenced_idxs(collection, rows, &idxs)?;
        }
        let mut out = self.store.delete_rows_at(collection, &idxs);
        let affected = out.len();
        mark_written(ctx, collection, &out);

        if let Undo::Rows(events) | Undo::Mixed { events } = ctx.undo {
            let undo_rows = if durable || ctx.want_rows {
                out.clone()
            } else {
                std::mem::take(&mut out)
            };
            events.push(RowUndo::Delete(
                collection.to_string(),
                idxs.to_vec(),
                undo_rows,
            ));
        }
        Ok((out, affected))
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
                self.clear_plan_cache();
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
                self.clear_plan_cache();
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
                self.clear_plan_cache();
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
                self.clear_plan_cache();
                Ok((
                    Vec::new(),
                    Some(format!("index {}", pack_index_label(collection, fields))),
                    if durable { Some(pack) } else { None },
                    true,
                    0,
                ))
            }
            Decl::Fk { .. } | Decl::Guard { .. } => {
                self.clear_plan_cache();
                Ok((
                    Vec::new(),
                    Some("schema: session only".into()),
                    None,
                    false,
                    0,
                ))
            }
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
                self.clear_plan_cache();
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
                self.clear_plan_cache();
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

fn check_batch_unique_rows(
    rows: &[Row],
    id_slot: usize,
    uri_slot_opt: Option<usize>,
    hash_slot_opt: Option<usize>,
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
        batch_ids.reserve(rows.len());
        if uri_slot_opt.is_some() {
            batch_uris.clear();
            batch_uris.reserve(rows.len());
        }
        let mut res = Ok(true);
        for row in rows {
            let cells = row.cells_slice();
            if let Some(h_slot) = hash_slot_opt
                && cells.get(h_slot).and_then(Cell::text).is_none()
            {
                res = Ok(false);
                break;
            }
            if let Some(id) = cells.get(id_slot).and_then(Cell::text) {
                if id.is_empty() {
                    res = Ok(false);
                    break;
                }
                // SAFETY: `clear_guard` clears `batch_ids` and `batch_uris` before `rows`
                // can go out of scope, even on panic/unwind.
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
            if let Some(u_pos) = uri_slot_opt {
                if let Some(uri) = cells.get(u_pos).and_then(Cell::text) {
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

pub(crate) fn build_batch_insert_rows(
    batch: &crate::batch::RecordBatch,
    needs_embed: bool,
    hashing_dim: Option<usize>,
    now: i64,
) -> Option<(Vec<Row>, BuiltShape)> {
    use crate::batch::Column;
    let n = batch.n();
    let w = batch.cols.len();
    if n == 0 || w == 0 || w > 30 || batch.names.len() != w {
        return None;
    }
    if !(0..w).all(|i| (i + 1..w).all(|j| batch.names[i] != batch.names[j])) {
        return None;
    }
    let find_col = |target: &str| batch.names.iter().position(|k| k == target);
    let id_idx = find_col("id");
    let body_idx = find_col("body");
    let hash_idx = find_col("hash");
    let title_idx = find_col("title");
    let snippet_idx = find_col("snippet");
    let has_embedding = find_col("embedding").is_some();

    let col_all_text = |idx: usize| -> bool {
        match &batch.cols[idx] {
            Column::Text { valid, .. } | Column::Dict { valid, .. } => valid.null_count() == 0,
            Column::Cells(cs) => cs.iter().all(|c| c.text().is_some()),
            _ => false,
        }
    };
    let col_all_nonempty_text = |idx: usize| -> bool {
        match &batch.cols[idx] {
            Column::Text {
                offsets, valid, ..
            } => valid.null_count() == 0 && offsets.windows(2).all(|w| w[1] > w[0]),
            Column::Dict {
                values,
                codes,
                valid,
            } => {
                valid.null_count() == 0
                    && (values.iter().all(|s| !s.is_empty())
                        || codes
                            .iter()
                            .all(|&c| values.get(c as usize).is_some_and(|s| !s.is_empty())))
            }
            Column::Cells(cs) => cs.iter().all(|c| c.text().is_some_and(|s| !s.is_empty())),
            _ => false,
        }
    };
    if hash_idx.is_some_and(|hi| !col_all_text(hi)) && body_idx.is_some() {
        return None;
    }
    if let Some(bi) = body_idx
        && !col_all_text(bi)
        && match &batch.cols[bi] {
            Column::Text { valid, .. } | Column::Dict { valid, .. } => {
                valid.null_count() < batch.cols[bi].len()
            }
            Column::Cells(cs) => cs.iter().any(|c| c.text().is_some()),
            _ => false,
        }
    {
        return None;
    }

    let needs_hash = hash_idx.is_none() && body_idx.is_some_and(col_all_text);
    let first_embed_col = title_idx.or(body_idx).or(snippet_idx);
    let can_inline_embed = needs_embed
        && !has_embedding
        && hashing_dim.is_some_and(|d| d > 0)
        && first_embed_col.is_some_and(col_all_nonempty_text)
        && title_idx.is_none_or(col_all_text)
        && body_idx.is_none_or(col_all_text)
        && snippet_idx.is_none_or(col_all_text);
    let needs_id = id_idx.is_none_or(|ii| !col_all_text(ii));

    let mut all_names: smallvec::SmallVec<[&str; 32]> =
        batch.names.iter().map(String::as_str).collect();
    if needs_hash {
        all_names.push("hash");
    }
    if can_inline_embed {
        all_names.push("embedding");
    }
    let shape = Row::shape_of(&all_names);
    let width = shape.names().len();

    #[derive(Clone, Copy)]
    enum BatchSlot {
        Col(usize),
        Hash(usize),
        Embed,
    }
    let mut slots: smallvec::SmallVec<[BatchSlot; 32]> = smallvec::SmallVec::with_capacity(width);
    for target in shape.names() {
        if target == "hash" && needs_hash {
            slots.push(BatchSlot::Hash(body_idx.unwrap()));
        } else if target == "embedding" && can_inline_embed {
            slots.push(BatchSlot::Embed);
        } else {
            slots.push(BatchSlot::Col(find_col(target)?));
        }
    }

    let lossless_cols = batch.cols.iter().all(|col| match col {
        Column::Nulls { .. } => false,
        Column::Int { valid, .. }
        | Column::Float { valid, .. }
        | Column::Bool { valid, .. }
        | Column::Time { valid, .. }
        | Column::Text { valid, .. }
        | Column::Dict { valid, .. } => valid.null_count() == 0,
        Column::Cells(cs) => {
            let k0 = cell_lossless_kind(&cs[0]);
            k0 != 0 && cs[1..].iter().all(|c| cell_lossless_kind(c) == k0)
        }
    });

    let get_col_text = |col_opt: Option<usize>, row_i: usize| -> Option<&str> {
        let ci = col_opt?;
        batch.cols[ci].text_at(row_i)
    };

    let build_range = |start: usize, end: usize| -> Vec<Row> {
        let mut out = Vec::with_capacity(end - start);
        let mut fill_with = |mut embed_batch: Option<&mut crate::embed::HashingBatch<'_>>| {
            for i in start..end {
                let mut emb_vec = embed_batch.as_deref_mut().map(|eb| {
                    eb.embed_parts([
                        get_col_text(title_idx, i),
                        get_col_text(body_idx, i),
                        get_col_text(snippet_idx, i),
                    ])
                });
                let eval_slot = |slot: BatchSlot, emb_vec: &mut Option<Arc<[f32]>>| -> Cell {
                    match slot {
                        BatchSlot::Col(ci) => batch.cols[ci].get(i),
                        BatchSlot::Hash(bi) => {
                            let body = get_col_text(Some(bi), i).unwrap_or("");
                            Cell::SmallText(content_hash_inline(body))
                        }
                        BatchSlot::Embed => Cell::Vec(emb_vec.take().unwrap()),
                    }
                };
                if width <= 8 {
                    let mut cells: smallvec::SmallVec<[Cell; 8]> =
                        smallvec::SmallVec::with_capacity(width);
                    for &slot in &slots {
                        cells.push(eval_slot(slot, &mut emb_vec));
                    }
                    out.push(Row::from_parts(Arc::clone(&shape), cells));
                } else {
                    let mut cells = Vec::with_capacity(width);
                    for &slot in &slots {
                        cells.push(eval_slot(slot, &mut emb_vec));
                    }
                    out.push(Row::from_vec_parts(Arc::clone(&shape), cells));
                }
            }
        };
        if let Some(dim) = hashing_dim.filter(|_| can_inline_embed) {
            crate::embed::with_hashing_scratch(dim, |eb| fill_with(Some(eb)));
        } else {
            fill_with(None);
        }
        out
    };

    let rows = if n >= 256 && (can_inline_embed || needs_hash || n >= 4096) {
        let min_chunk = if can_inline_embed { 128 } else { 1024 };
        let threads = std::thread::available_parallelism()
            .map(|p| p.get())
            .unwrap_or(1)
            .min(8)
            .min((n + min_chunk - 1) / min_chunk);
        if threads > 1 {
            let chunk_size = (n + threads - 1) / threads;
            let build_range = &build_range;
            let chunks: Vec<Vec<Row>> = std::thread::scope(|scope| {
                let mut handles = Vec::with_capacity(threads);
                for c in 0..threads {
                    let start = c * chunk_size;
                    if start >= n {
                        break;
                    }
                    let end = (start + chunk_size).min(n);
                    handles.push(scope.spawn(move || build_range(start, end)));
                }
                handles.into_iter().map(|h| h.join().unwrap()).collect()
            });
            let mut out = Vec::with_capacity(n);
            for chunk in chunks {
                out.extend(chunk);
            }
            out
        } else {
            build_range(0, n)
        }
    } else {
        build_range(0, n)
    };

    Some((
        rows,
        BuiltShape {
            all_uniform: true,
            all_embedded: !needs_embed || can_inline_embed,
            needs_id,
            lossless_cols,
            hashing_dim,
            now,
        },
    ))
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
            &std::slice::from_raw_parts(task.records_ptr as *const Record, task.n)[start..end]
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

fn insert_build_workers() -> &'static [std::sync::mpsc::SyncSender<Arc<BuildTask>>] {
    static BUILD_WORKERS: std::sync::OnceLock<
        Vec<std::sync::mpsc::SyncSender<std::sync::Arc<BuildTask>>>,
    > = std::sync::OnceLock::new();
    BUILD_WORKERS.get_or_init(|| {
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
    })
}

pub(super) fn warm_insert_build_workers() {
    let _ = insert_build_workers();
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
        let workers = insert_build_workers();
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

    let mut names: smallvec::SmallVec<[&str; 32]> =
        first.fields.iter().map(|(key, _)| key.as_str()).collect();
    if needs_hash {
        names.push("hash");
    }
    if needs_embed {
        names.push("embedding");
    }
    let shape = Row::shape_of(&names);

    #[derive(Clone, Copy)]
    enum ShapeSlot {
        Source(usize),
        Body(usize),
        Hash(usize),
        Embed,
    }

    let slots = shape
        .names()
        .iter()
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
        row.insert(k.as_str(), value_cell(v, now));
    }
    row
}

impl Db {
    #[inline]
    pub(crate) fn update_needs_reembed(&self, fields: &[(String, Value)]) -> bool {
        self.embedder.is_some()
            && fields
                .iter()
                .any(|(k, _)| matches!(k.as_str(), "title" | "body" | "snippet"))
            && !fields.iter().any(|(k, _)| k == "embedding")
    }

    pub(crate) fn check_row_update_guards(
        &self,
        collection: &str,
        before: &Row,
        fields: &[(String, Value)],
        now: i64,
    ) -> Result<(), Error> {
        for (k, _) in fields {
            for g in &self.catalog.guards {
                if g.collection == collection && &g.field == k && eval_pred(&g.pred, before, now) {
                    return Err(Error::runtime(format!("immutable field: {collection}.{k}")));
                }
            }
        }
        Ok(())
    }

    pub(crate) fn patch_updated_row(
        &self,
        collection: &str,
        mut updated: Row,
        fields: &[(String, Value)],
        now: i64,
        has_body: bool,
        reembed: bool,
    ) -> Result<Row, Error> {
        for (k, v) in fields {
            updated.insert(k.as_str(), value_cell(v, now));
        }
        if has_body && let Some(body) = row_text(&updated, "body") {
            let h = content_hash_inline(body);
            updated.insert("hash", Cell::SmallText(h));
        }
        if reembed {
            updated.remove("embedding");
            self.maybe_embed_rows(collection, std::slice::from_mut(&mut updated))?;
        }
        Ok(updated)
    }
}
