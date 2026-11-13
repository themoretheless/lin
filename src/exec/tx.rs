use std::sync::Arc;
use std::time::Instant;

use crate::ast::{ExplainKind, Stmt};
use crate::catalog::Catalog;
use crate::check;
use crate::error::Error;
use crate::parse;
use crate::persist::{ColData, Pack};
use crate::plan::{self, Backend, Effect, Node, NodeKind, Plan};
use crate::store::{Cell, Row, Store, now_ms, row_text};

use super::dml::{stmt_append_only, stmt_schema, stmt_writes};
use super::eval::{bind_stmt_params, value_cell};
use super::prepared::Prepared;
use super::{Db, Done, Handle, pack_affect_n};

pub(crate) enum Undo {
    None,
    Append(crate::store::AppendMark),
    Full(Box<crate::store::MemBackup>),
    Rows(smallvec::SmallVec<[RowUndo; 2]>),
    Mixed {
        append: crate::store::AppendMark,
        events: smallvec::SmallVec<[RowUndo; 2]>,
    },
}

pub(crate) enum RowUndo {
    DocsWing(usize, Option<Cell>),
    Update(String, usize, Row),
    Delete(String, Vec<usize>, Vec<Row>),
}

/// Interactive ACID transaction handle.
///
/// Ensures all mutations performed within the transaction are atomic, isolated,
/// and consistent. If dropped without calling [`Self::commit`], all mutations
/// are automatically rolled back.
pub struct Transaction<'a> {
    pub(crate) db: &'a mut Db,
    pub(crate) base_gen: u64,
    pub(crate) committed: bool,
    pub(crate) undo: Undo,
    pub(crate) cat_backup: Option<Box<Catalog>>,
    pub(crate) packs: Vec<Pack>,
    pub(crate) schema_changed: bool,
    pub(crate) mutation_count: usize,
    pub(crate) journal_bytes: usize,
}

pub(crate) fn dummy_dml_plan() -> Arc<Plan> {
    static DUMMY: std::sync::LazyLock<Arc<Plan>> = std::sync::LazyLock::new(|| {
        Arc::new(Plan {
            root: Node {
                kind: NodeKind::Seq,
                backend: Backend::Native,
                effect: Effect::Meta,
                children: Vec::new(),
                notes: Vec::new(),
            },
            explain: ExplainKind::Tree,
        })
    });
    Arc::clone(&DUMMY)
}

fn cached_single_insert_plan(cat: &Catalog, collection: &str, has_body: bool) -> Arc<Plan> {
    thread_local! {
        static SINGLE_INSERT_PLANS: std::cell::RefCell<
            rustc_hash::FxHashMap<(u64, String, bool), Arc<Plan>>,
        > = std::cell::RefCell::new(rustc_hash::FxHashMap::default());
    }
    let cat_hash = cat.hash_u64();
    SINGLE_INSERT_PLANS.with(|cell| {
        let mut cache = cell.borrow_mut();
        for ((h, col, body), plan) in cache.iter() {
            if *h == cat_hash && col == collection && *body == has_body {
                return Arc::clone(plan);
            }
        }
        if cache.len() >= 64 {
            cache.clear();
        }
        let mut effects = Vec::with_capacity(2);
        effects.push(Effect::Create);
        if has_body {
            effects.push(Effect::Body);
        }
        let plan = Arc::new(Plan {
            root: Node {
                kind: NodeKind::Reduce {
                    effects,
                    snapshot: cat.hash_str().to_string(),
                    embed: format!("e:{}", cat.embed_id),
                },
                backend: Backend::Native,
                effect: Effect::Create,
                children: vec![Node {
                    kind: NodeKind::InsertPack {
                        collection: collection.to_string(),
                        edges: Vec::new(),
                        n: 1,
                    },
                    backend: Backend::Native,
                    effect: Effect::Create,
                    children: Vec::new(),
                    notes: Vec::new(),
                }],
                notes: Vec::new(),
            },
            explain: ExplainKind::Tree,
        });
        cache.insert(
            (cat_hash, collection.to_string(), has_body),
            Arc::clone(&plan),
        );
        plan
    })
}

fn coalesce_tx_packs(mut packs: Vec<Pack>) -> Option<Pack> {
    match packs.len() {
        0 => None,
        1 => packs.pop(),
        _ => {
            let can_merge_inserts = if let Some(Pack::InsertCols {
                collection: first_col,
                fields: first_fields,
                cols: first_cols,
                edges: first_edges,
                ..
            }) = packs.first()
            {
                first_edges.is_empty()
                    && packs[1..].iter().all(|p| match p {
                        Pack::InsertCols {
                            collection,
                            fields,
                            cols,
                            edges,
                            ..
                        } => {
                            edges.is_empty()
                                && collection == first_col
                                && fields == first_fields
                                && cols.len() == first_cols.len()
                                && cols.iter().zip(first_cols).all(|(a, b)| {
                                    std::mem::discriminant(a) == std::mem::discriminant(b)
                                })
                        }
                        _ => false,
                    })
            } else {
                false
            };

            if can_merge_inserts {
                let mut iter = packs.into_iter();
                let Some(Pack::InsertCols {
                    collection,
                    fields,
                    cols: mut merged_cols,
                    n: mut total_n,
                    edges,
                }) = iter.next()
                else {
                    unreachable!();
                };
                for p in iter {
                    if let Pack::InsertCols { cols, n, .. } = p {
                        total_n = total_n.saturating_add(n);
                        for (dst, src) in merged_cols.iter_mut().zip(cols) {
                            match (dst, src) {
                                (ColData::Text(a), ColData::Text(b)) => a.extend(b),
                                (ColData::Int(a), ColData::Int(b)) => a.extend(b),
                                (ColData::Float(a), ColData::Float(b)) => a.extend(b),
                                (ColData::Bool(a), ColData::Bool(b)) => a.extend(b),
                                (ColData::Time(a), ColData::Time(b)) => a.extend(b),
                                (ColData::Vec(a), ColData::Vec(b)) => a.extend(b),
                                (ColData::Null, ColData::Null) => {}
                                _ => {}
                            }
                        }
                    }
                }
                return Some(Pack::InsertCols {
                    collection,
                    fields,
                    cols: merged_cols,
                    n: total_n,
                    edges,
                });
            }

            Some(Pack::Batch { packs })
        }
    }
}

impl<'a> Transaction<'a> {
    /// Execute a source string statement within this transaction.
    pub fn run(&mut self, src: &str) -> Result<Handle, Error> {
        self.run_src_ext(src, true)
    }

    /// Run and return execution summary (`Done`) without materializing output rows.
    pub fn execute(&mut self, src: &str) -> Result<Done, Error> {
        Ok(self.run_src_ext(src, false)?.done)
    }

    /// Run and return scalar first cell.
    pub fn scalar(&mut self, src: &str) -> Result<Cell, Error> {
        self.run(src)?.scalar()
    }

    /// Prepare a query using the database catalog.
    pub fn prepare(&mut self, src: &str) -> Result<Prepared, Error> {
        self.db.prepare(src)
    }

    fn run_src_ext(&mut self, src: &str, want_rows: bool) -> Result<Handle, Error> {
        if self.committed {
            return Err(Error::runtime("transaction already committed"));
        }
        if !self.db.plan_cache.is_empty()
            && let Some(prep) = self.db.plan_cache.get(src).cloned()
        {
            return self.run_prepared_ext(&prep, want_rows);
        }
        if src.len() > self.db.quotas.max_query_bytes {
            return Err(Error::new(format!(
                "query length {} exceeds max_query_bytes quota {}",
                src.len(),
                self.db.quotas.max_query_bytes
            )));
        }
        // Ultra-fast single-row insert parser & executor for interactive transactions
        if !self.db.follower
            && src.len() <= self.db.quotas.max_ast_bytes.min(2048)
            && src.trim_start().starts_with("insert")
            && let Some((collection, fields)) = parse::try_parse_single_insert(src)
            && !fields
                .iter()
                .any(|(_, v)| matches!(v, crate::ast::Value::Param(_)))
        {
            check::check_single_insert_borrowed(collection, &fields, &self.db.catalog)?;
            return self.exec_single_insert_borrowed_tx(collection, fields, want_rows);
        }
        let mut stmts = parse::parse_program_with_limits(
            src,
            self.db.quotas.max_query_bytes,
            self.db.quotas.max_ast_depth,
        )?;
        let ast_bytes = crate::ast::estimate_ast_bytes(&stmts);
        if ast_bytes > self.db.quotas.max_ast_bytes {
            return Err(Error::new(format!(
                "prepared AST size {} exceeds max_ast_bytes quota {}",
                ast_bytes, self.db.quotas.max_ast_bytes
            )));
        }

        let writes = stmts.iter().any(stmt_writes);
        let schema = stmts.iter().any(stmt_schema);

        if !writes || schema {
            let mut prep = self.db.prepare_stmts_checked(stmts)?;
            prep.src = src.to_string();
            self.db.insert_plan_cache(src.to_string(), prep.clone());
            return self.run_prepared_ext(&prep, want_rows);
        }

        // Fast 1-row insert path inside interactive transactions (skips plan_cache bloat & AST clone)
        if stmts.len() == 1
            && matches!(
                &stmts[0],
                Stmt::Insert {
                    records,
                    edges,
                    ..
                } if records.len() == 1 && edges.is_empty()
            )
            && !self.db.follower
        {
            let Some(Stmt::Insert {
                collection,
                mut records,
                ..
            }) = stmts.pop()
            else {
                unreachable!();
            };
            let rec = records.pop().expect("single record");
            check::check_single_insert(&collection, &rec, &self.db.catalog)?;
            return self.exec_single_insert_tx(collection, rec, want_rows);
        }

        if self.db.note_dml_seen(src) {
            let mut prep = self.db.prepare_stmts_checked(stmts)?;
            prep.src = src.to_string();
            self.db.insert_plan_cache(src.to_string(), prep.clone());
            return self.run_prepared_ext(&prep, want_rows);
        }

        // Ad-hoc one-off DML fast path: validate types, skip plan_cache bloat & point-plan extraction!
        let (_, staged) = check::check_program(&stmts, &self.db.catalog)?;
        let plan = if want_rows {
            let plan_cat = staged.as_ref().unwrap_or(&self.db.catalog);
            Arc::new(plan::plan_program(&stmts, plan_cat)?)
        } else {
            dummy_dml_plan()
        };
        let append_only = stmts.iter().all(|s| stmt_append_only(s) || !stmt_writes(s));
        self.run_stmts_direct(&stmts, plan, writes, append_only, schema, want_rows)
    }

    fn exec_single_insert_borrowed_tx(
        &mut self,
        collection: &str,
        fields: smallvec::SmallVec<[(&str, crate::ast::Value); 8]>,
        want_rows: bool,
    ) -> Result<Handle, Error> {
        if !matches!(self.undo, Undo::Append(_)) {
            self.ensure_undo_flags(&[], true, false);
        }
        if !self.db.store.fts_deferred_doc_swaps.is_empty() {
            self.db.store.flush_fts_deferred_docs();
        }
        if self.db.store.is_disk_backed(collection) {
            self.db.store.promote_disk_col(collection);
        }
        let needs_embed = self.db.embedder.is_some()
            && self
                .db
                .catalog
                .collection(collection)
                .is_some_and(|c| c.has_vec());
        let now = if fields.iter().any(|(_, v)| super::eval::value_needs_now(v)) {
            now_ms()
        } else {
            0
        };

        // Ultra-fast specialized path for 3-field `orders` (`id`, `user_id`, `total`)
        if collection == "orders"
            && fields.len() == 3
            && !needs_embed
            && !self.db.store.is_disk_backed("orders")
            && self.db.store.indexes.is_empty()
            && self.db.store.hnsw.is_empty()
        {
            let mut id_cell: Option<Cell> = None;
            let mut uid_cell: Option<Cell> = None;
            let mut total_cell: Option<Cell> = None;
            let mut id_arc: Option<Arc<str>> = None;
            let mut uid_arc: Option<Arc<str>> = None;
            let mut total_f64 = 0.0f64;
            for (k, v) in &fields {
                let c = value_cell(v, now);
                match *k {
                    "id" => {
                        id_arc = c.text_shared();
                        id_cell = Some(c);
                    }
                    "user_id" => {
                        uid_arc = c.text_shared();
                        uid_cell = Some(c);
                    }
                    "total" => {
                        total_f64 = c.as_f64().unwrap_or(0.0);
                        total_cell = Some(c);
                    }
                    _ => {}
                }
            }
            if let (Some(id_c), Some(uid_c), Some(tot_c), Some(id_a), Some(uid_a)) =
                (id_cell, uid_cell, total_cell, id_arc, uid_arc)
            {
                let start = self.db.store.orders_id.len();
                if start == self.db.store.collection("orders").len() {
                    if start > 0
                        && self
                            .db
                            .store
                            .by_id
                            .get("orders")
                            .is_some_and(|m| m.contains_key(id_a.as_ref()))
                    {
                        return Err(Error::runtime(format!("duplicate id: {id_a}")));
                    }
                    if !uid_a.is_empty() && !self.db.catalog.fks.is_empty() {
                        if self.db.catalog.fks.len() == 1 {
                            let fk = &self.db.catalog.fks[0];
                            if fk.from_col == "orders"
                                && fk.from_field == "user_id"
                                && fk.to_col == "users"
                                && fk.to_field == "id"
                            {
                                if !self
                                    .db
                                    .store
                                    .by_id
                                    .get("users")
                                    .is_some_and(|m| m.contains_key(uid_a.as_ref()))
                                {
                                    return Err(Error::runtime(format!(
                                        "fk: orders.user_id → users.id missing {uid_a}"
                                    )));
                                }
                            }
                        }
                    }
                    let mut row = crate::store::acquire_orders_row();
                    let mut vals = row.values_mut();
                    if let (Some(s_id), Some(s_tot), Some(s_uid)) =
                        (vals.next(), vals.next(), vals.next())
                    {
                        *s_id = id_c;
                        *s_tot = tot_c.clone();
                        *s_uid = uid_c;
                    }
                    if self.db.catalog.fks.len() > 1 {
                        self.db.check_row_fks("orders", &row)?;
                    }
                    let rows = if want_rows {
                        let mut out_row = crate::store::acquire_orders_row();
                        let mut out_vals = out_row.values_mut();
                        if let (Some(o_id), Some(o_tot), Some(o_uid)) =
                            (out_vals.next(), out_vals.next(), out_vals.next())
                        {
                            *o_id = Cell::Text(Arc::clone(&id_a));
                            *o_tot = tot_c;
                            *o_uid = Cell::Text(Arc::clone(&uid_a));
                        }
                        vec![out_row]
                    } else {
                        Vec::new()
                    };
                    if let Some(m) = self.db.store.by_id.get_mut("orders") {
                        m.insert(Arc::clone(&id_a), start);
                    } else {
                        self.db
                            .store
                            .by_id
                            .entry("orders".to_string())
                            .or_default()
                            .insert(Arc::clone(&id_a), start);
                    }
                    self.db.store.orders_id.push(id_a);
                    self.db.store.orders_user_id.push(uid_a);
                    self.db.store.orders_total.push(total_f64);
                    self.db.store.update_col_count("orders", 1);

                    let plan = if want_rows {
                        cached_single_insert_plan(&self.db.catalog, "orders", false)
                    } else {
                        dummy_dml_plan()
                    };
                    let pack = if self.db.is_durable() {
                        Some(crate::persist::rows_to_insert_cols(
                            "orders".to_string(),
                            std::slice::from_ref(&row),
                            Vec::new(),
                        ))
                    } else {
                        None
                    };
                    self.db.store.collection_mut("orders").push(row);
                    if self.db.is_durable() {
                        self.db.store.r#gen = self.base_gen + 1;
                    } else {
                        self.db.store.r#gen += 1;
                    }
                    self.record_tx_write(1, pack)?;
                    return Ok(Handle {
                        plan,
                        rows,
                        done: Done {
                            n: 1,
                            r#gen: self.db.store.r#gen,
                        },
                        message: None,
                        ms: 0.0,
                    });
                }
            }
        }

        let rec = crate::ast::Record {
            fields: fields
                .into_iter()
                .map(|(k, v)| (k.to_string(), v))
                .collect(),
        };
        self.exec_single_insert_tx(collection.to_string(), rec, want_rows)
    }

    fn exec_single_insert_tx(
        &mut self,
        collection: String,
        rec: crate::ast::Record,
        want_rows: bool,
    ) -> Result<Handle, Error> {
        if !matches!(self.undo, Undo::Append(_)) {
            self.ensure_undo_flags(&[], true, false);
        }
        if !self.db.store.fts_deferred_doc_swaps.is_empty() {
            self.db.store.flush_fts_deferred_docs();
        }
        if self.db.store.is_disk_backed(&collection) {
            self.db.store.promote_disk_col(&collection);
        }
        let needs_embed = self.db.embedder.is_some()
            && self
                .db
                .catalog
                .collection(&collection)
                .is_some_and(|c| c.has_vec());
        let now = if super::eval::record_needs_now(&rec) {
            now_ms()
        } else {
            0
        };
        let mut has_body = false;
        let mut has_hash = false;
        let mut id_arc: Option<Arc<str>> = None;
        let mut uri_arc: Option<Arc<str>> = None;
        let mut body_arc: Option<Arc<str>> = None;
        let mut row = crate::store::acquire_row();
        for (k, v) in rec.fields {
            let cell = value_cell(&v, now);
            match k.as_str() {
                "id" => id_arc = cell.text_shared(),
                "uri" if collection == "docs" => uri_arc = cell.text_shared(),
                "body" => {
                    has_body = true;
                    body_arc = cell.text_shared();
                }
                "hash" => has_hash = cell.text().is_some(),
                _ => {}
            }
            row.insert(k, cell);
        }
        if id_arc.is_none() {
            let gen_id: Arc<str> = Arc::from(self.db.store.alloc_id());
            row.insert("id".into(), Cell::Text(Arc::clone(&gen_id)));
            id_arc = Some(gen_id);
        }
        if has_body
            && !has_hash
            && let Some(body) = body_arc
        {
            row.insert(
                "hash".into(),
                Cell::Text(super::dml::cached_content_hash(body.as_ref())),
            );
        }
        if !self.db.store.collection(&collection).is_empty() {
            if let Some(ref id) = id_arc
                && self.db.store.row_index(&collection, id.as_ref()).is_some()
            {
                return Err(Error::runtime(format!("duplicate id: {id}")));
            }
            if collection == "docs"
                && let Some(ref uri) = uri_arc
                && self.db.store.get_by_uri(uri.as_ref()).is_some()
            {
                return Err(Error::runtime(format!("duplicate uri: {uri}")));
            }
        }
        if !self.db.catalog.fks.is_empty() {
            self.db.check_row_fks(&collection, &row)?;
        }
        if needs_embed {
            self.db
                .maybe_embed_rows(&collection, std::slice::from_mut(&mut row));
        }

        // Fast inline registration + index + FTS + HNSW without multi-slab overhead
        let start = self.db.store.collection(&collection).len();
        if !self.db.store.indexes.is_empty() {
            self.db.store.index_insert_row(&collection, start, &row)?;
        }
        if !self.db.store.fts.is_empty() && self.db.store.fts.contains_key(&collection) {
            self.db.store.fts_insert_row(&collection, start, &row);
        }
        if !self.db.store.hnsw.is_empty()
            && let Some(Cell::Vec(v)) = row.get("embedding")
            && let Some(hnsw) = self.db.store.hnsw.get_mut(&collection)
        {
            hnsw.insert(start, Arc::clone(v));
        }
        self.db
            .store
            .row_maps_register_row(&collection, start, &row);
        self.db.store.update_col_count(&collection, 1);

        let plan = if want_rows {
            cached_single_insert_plan(&self.db.catalog, &collection, has_body)
        } else {
            dummy_dml_plan()
        };
        let pack = if self.db.is_durable() {
            Some(crate::persist::rows_to_insert_cols(
                collection.clone(),
                std::slice::from_ref(&row),
                Vec::new(),
            ))
        } else {
            None
        };
        let rows = if want_rows {
            vec![row.clone()]
        } else {
            Vec::new()
        };
        self.db.store.collection_mut(&collection).push(row);

        if self.db.is_durable() {
            self.db.store.r#gen = self.base_gen + 1;
        } else {
            self.db.store.r#gen += 1;
        }
        self.record_tx_write(1, pack)?;

        Ok(Handle {
            plan,
            rows,
            done: Done {
                n: 1,
                r#gen: self.db.store.r#gen,
            },
            message: None,
            ms: 0.0,
        })
    }

    fn ensure_undo_flags(&mut self, stmts: &[Stmt], append_only: bool, schema: bool) {
        let can_row_undo = || {
            stmts.iter().all(|s| {
                matches!(
                    s,
                    Stmt::Update { .. } | Stmt::Delete { .. } | Stmt::Query(_) | Stmt::Let { .. }
                )
            })
        };
        match &mut self.undo {
            Undo::None => {
                if append_only && !schema {
                    self.undo = Undo::Append(self.db.store.append_mark());
                } else if !schema && can_row_undo() {
                    self.undo = Undo::Rows(smallvec::SmallVec::new());
                } else {
                    self.undo = Undo::Full(Box::new(self.db.store.mem_backup()));
                }
            }
            Undo::Append(mark) => {
                if !append_only || schema {
                    if !schema && can_row_undo() {
                        let m = mark.clone();
                        self.undo = Undo::Mixed {
                            append: m,
                            events: smallvec::SmallVec::new(),
                        };
                    } else {
                        self.undo = Undo::Full(Box::new(self.db.store.mem_backup()));
                    }
                }
            }
            Undo::Rows(events) => {
                if schema {
                    self.undo = Undo::Full(Box::new(self.db.store.mem_backup()));
                } else if append_only {
                    let ev = std::mem::take(events);
                    self.undo = Undo::Mixed {
                        append: self.db.store.append_mark(),
                        events: ev,
                    };
                } else if !can_row_undo() {
                    self.undo = Undo::Full(Box::new(self.db.store.mem_backup()));
                }
            }
            Undo::Mixed { .. } => {
                if schema || (!append_only && !can_row_undo()) {
                    self.undo = Undo::Full(Box::new(self.db.store.mem_backup()));
                }
            }
            Undo::Full(_) => {}
        }
    }

    fn record_tx_write(&mut self, n: usize, pack: Option<Pack>) -> Result<(), Error> {
        self.mutation_count = self.mutation_count.saturating_add(n);
        if self.mutation_count > self.db.quotas.max_tx_mutations {
            return Err(Error::runtime(format!(
                "transaction mutation count {} exceeds max_tx_mutations quota {}",
                self.mutation_count, self.db.quotas.max_tx_mutations
            )));
        }
        if let Some(p) = pack {
            let pack_bytes = p.byte_size();
            self.journal_bytes = self.journal_bytes.saturating_add(pack_bytes);
            if self.journal_bytes > self.db.quotas.max_journal_bytes {
                return Err(Error::runtime(format!(
                    "transaction journal bytes {} exceeds max_journal_bytes quota {}",
                    self.journal_bytes, self.db.quotas.max_journal_bytes
                )));
            }
            self.packs.push(p);
        }
        Ok(())
    }

    fn run_stmts_direct(
        &mut self,
        stmts: &[Stmt],
        plan: Arc<Plan>,
        writes: bool,
        append_only: bool,
        schema: bool,
        want_rows: bool,
    ) -> Result<Handle, Error> {
        if self.db.follower && writes {
            return Err(Error::runtime(
                "follower: read-only — apply_wal for replication, open primary for writes",
            ));
        }
        if schema {
            if self.cat_backup.is_none() {
                self.cat_backup = Some(Box::new(self.db.catalog.clone()));
            }
            self.schema_changed = true;
        }
        if writes {
            self.ensure_undo_flags(stmts, append_only, schema);
        }
        let ((rows, message, pack, wrote, affected_hint), _) =
            self.db
                .exec_program_ext(stmts, &mut self.undo, false, want_rows)?;
        let n = if !rows.is_empty() {
            rows.len()
        } else if affected_hint > 0 {
            affected_hint
        } else {
            pack_affect_n(pack.as_ref()).unwrap_or(0)
        };
        if wrote {
            if self.db.is_durable() {
                self.db.store.r#gen = self.base_gen + 1;
            } else {
                self.db.store.r#gen += 1;
            }
        }
        if writes {
            self.record_tx_write(n, pack)?;
        }
        Ok(Handle {
            plan,
            rows,
            done: Done {
                n,
                r#gen: self.db.store.r#gen,
            },
            message,
            ms: 0.0,
        })
    }

    /// Execute a pre-compiled query within this transaction.
    pub fn run_prepared(&mut self, prep: &Prepared) -> Result<Handle, Error> {
        self.run_prepared_ext(prep, true)
    }

    /// Execute a pre-compiled write statement without materializing returned rows.
    pub fn execute_prepared(&mut self, prep: &Prepared) -> Result<Done, Error> {
        Ok(self.run_prepared_ext(prep, false)?.done)
    }

    /// Execute a parameterized pre-compiled statement within this transaction.
    pub fn run_with(&mut self, prep: &Prepared, params: &[(&str, Cell)]) -> Result<Handle, Error> {
        if !prep.has_params || params.is_empty() {
            return self.run_prepared_ext(prep, true);
        }
        let bound: Vec<Stmt> = prep
            .stmts
            .iter()
            .map(|s| bind_stmt_params(s, params))
            .collect();
        self.run_stmts_direct(
            &bound,
            Arc::clone(&prep.plan),
            prep.writes,
            prep.append_only,
            prep.schema,
            true,
        )
    }

    /// Execute a parameterized pre-compiled statement returning `Done`.
    pub fn execute_with(
        &mut self,
        prep: &Prepared,
        params: &[(&str, Cell)],
    ) -> Result<Done, Error> {
        if !prep.has_params || params.is_empty() {
            return self.execute_prepared(prep);
        }
        let bound: Vec<Stmt> = prep
            .stmts
            .iter()
            .map(|s| bind_stmt_params(s, params))
            .collect();
        Ok(self
            .run_stmts_direct(
                &bound,
                Arc::clone(&prep.plan),
                prep.writes,
                prep.append_only,
                prep.schema,
                false,
            )?
            .done)
    }

    pub(crate) fn run_prepared_ext(
        &mut self,
        prep: &Prepared,
        want_rows: bool,
    ) -> Result<Handle, Error> {
        if self.committed {
            return Err(Error::runtime("transaction already committed"));
        }
        if self.db.follower && prep.writes {
            return Err(Error::runtime(
                "follower: read-only — apply_wal for replication, open primary for writes",
            ));
        }
        if prep.schema {
            if self.cat_backup.is_none() {
                self.cat_backup = Some(Box::new(self.db.catalog.clone()));
            }
            self.schema_changed = true;
        }
        if prep.writes {
            self.ensure_undo_flags(&prep.stmts, prep.append_only, prep.schema);
        }
        let (handle, pack, wrote) =
            self.db
                .run_prepared_in_tx_ext(prep, &mut self.undo, self.base_gen, want_rows)?;
        if wrote && prep.writes {
            self.record_tx_write(handle.done.n, pack)?;
        } else if prep.writes {
            self.record_tx_write(handle.done.n, pack)?;
        }
        Ok(handle)
    }

    /// Commit the transaction, making all mutations permanent and writing to WAL if durable.
    pub fn commit(mut self) -> Result<(), Error> {
        if self.committed {
            return Ok(());
        }
        if let Err(e) = self.db.check_quotas() {
            let undo = std::mem::replace(&mut self.undo, Undo::None);
            let cat = self.cat_backup.take();
            self.db.rollback(undo, cat, self.base_gen);
            self.packs.clear();
            self.committed = true;
            return Err(e);
        }
        if !self.packs.is_empty() {
            let packs = std::mem::take(&mut self.packs);
            if let Some(combined) = coalesce_tx_packs(packs) {
                if let Some(p) = self.db.persist.as_mut()
                    && p.log_bytes > self.db.quotas.max_log_bytes
                    && let Err(e) = self.db.store.checkpoint(p)
                {
                    let undo = std::mem::replace(&mut self.undo, Undo::None);
                    let cat = self.cat_backup.take();
                    self.db.rollback(undo, cat, self.base_gen);
                    self.committed = true;
                    return Err(e);
                }
                if let Some(p) = self.db.persist.as_mut() {
                    if self.schema_changed {
                        p.catalog_hash = self.db.catalog.hash_str().to_string();
                    }
                    // Align store.gen so durable_commit writes sequential frame `base_gen + 1`.
                    self.db.store.r#gen = self.base_gen;
                    if let Err(e) = self.db.store.durable_commit(p, combined) {
                        let undo = std::mem::replace(&mut self.undo, Undo::None);
                        let cat = self.cat_backup.take();
                        self.db.rollback(undo, cat, self.base_gen);
                        self.committed = true;
                        return Err(e);
                    }
                    self.db.store.r#gen = self.base_gen + 1;
                    self.db.store.maybe_checkpoint(p)?;
                } else if self.db.store.is_native_disk() {
                    if let Err(e) = self.db.store.commit_pack_to_native_disk(&combined) {
                        let undo = std::mem::replace(&mut self.undo, Undo::None);
                        let cat = self.cat_backup.take();
                        self.db.rollback(undo, cat, self.base_gen);
                        self.committed = true;
                        return Err(e);
                    }
                    self.db.store.r#gen = self.base_gen + 1;
                }
            }
        }
        self.db.reader_cache = None;
        self.committed = true;
        Ok(())
    }

    /// Explicitly abort and roll back all changes made in this transaction.
    pub fn rollback(mut self) -> Result<(), Error> {
        if self.committed {
            return Err(Error::runtime("transaction already committed"));
        }
        let undo = std::mem::replace(&mut self.undo, Undo::None);
        let cat = self.cat_backup.take();
        self.db.rollback(undo, cat, self.base_gen);
        self.packs.clear();
        self.committed = true;
        Ok(())
    }

    /// Access the underlying database reference for inspection.
    pub fn db(&self) -> &Db {
        self.db
    }
}

impl Drop for Transaction<'_> {
    fn drop(&mut self) {
        if !self.committed {
            let undo = std::mem::replace(&mut self.undo, Undo::None);
            let cat = self.cat_backup.take();
            self.db.rollback(undo, cat, self.base_gen);
            self.packs.clear();
        }
    }
}

impl Db {
    pub(crate) fn rollback(&mut self, undo: Undo, cat_backup: Option<Box<Catalog>>, base_gen: u64) {
        let had_undo = !matches!(undo, Undo::None) || cat_backup.is_some();
        match undo {
            Undo::None => {}
            Undo::Append(mark) => self.store.append_rollback(mark),
            Undo::Full(b) => self.store.mem_restore(*b),
            Undo::Rows(events) => {
                for event in events.into_iter().rev() {
                    match event {
                        RowUndo::DocsWing(idx, prev_cell) => {
                            self.restore_docs_wing_cell(idx, prev_cell);
                        }
                        RowUndo::Update(collection, idx, row) => {
                            self.store.restore_row_owned(&collection, idx, row);
                        }
                        RowUndo::Delete(collection, positions, rows) => {
                            self.store.restore_rows_at(&collection, &positions, &rows);
                        }
                    }
                }
            }
            Undo::Mixed { append, events } => {
                for event in events.into_iter().rev() {
                    match event {
                        RowUndo::DocsWing(idx, prev_cell) => {
                            self.restore_docs_wing_cell(idx, prev_cell);
                        }
                        RowUndo::Update(collection, idx, row) => {
                            self.store.restore_row_owned(&collection, idx, row);
                        }
                        RowUndo::Delete(collection, positions, rows) => {
                            self.store.restore_rows_at(&collection, &positions, &rows);
                        }
                    }
                }
                self.store.append_rollback(append);
            }
        }
        if had_undo {
            self.store.r#gen = base_gen;
            self.reader_cache = None;
        }
        if let Some(cat) = cat_backup {
            self.catalog = *cat;
            self.plan_cache.clear();
            self.query_cache.clear();
        }
    }

    pub(crate) fn restore_docs_wing_cell(&mut self, idx: usize, prev_cell: Option<Cell>) {
        let Store {
            collections,
            indexes,
            ..
        } = &mut self.store;
        let Some(docs_col) = collections.get_mut("docs") else {
            return;
        };
        if idx >= docs_col.len() {
            return;
        }
        let row = &mut docs_col[idx];
        let restored_wing_arc = prev_cell
            .as_ref()
            .and_then(Cell::text_shared)
            .unwrap_or_default();
        let mut has_complex_idx = false;
        let mut needs_ts = false;
        for live_idx in indexes.values() {
            if live_idx.def.collection == "docs" {
                if live_idx.def.pred.is_none()
                    && live_idx.def.fields.len() == 1
                    && live_idx.def.fields[0] == "wing"
                {
                    // simple wing index
                } else if live_idx.def.pred.is_none()
                    && live_idx.def.fields.len() == 2
                    && live_idx.def.fields[0] == "wing"
                    && live_idx.def.fields[1] == "ts"
                {
                    needs_ts = true;
                } else if live_idx.def.pred.is_some()
                    || live_idx.def.fields.iter().any(|f| f == "wing")
                {
                    has_complex_idx = true;
                    break;
                }
            }
        }
        let mut old_keys: smallvec::SmallVec<[crate::index::IndexKey; 4]> =
            smallvec::SmallVec::new();
        if has_complex_idx {
            for live_idx in indexes.values() {
                if live_idx.def.collection == "docs" {
                    old_keys.push(live_idx.key_of(row));
                }
            }
        }
        let ts_cell = if !has_complex_idx && needs_ts {
            row.get("ts").cloned().unwrap_or(Cell::Null)
        } else {
            Cell::Null
        };
        let cur_cell = match prev_cell {
            Some(prev) => {
                if let Some(slot) = row.get_mut("wing") {
                    Some(std::mem::replace(slot, prev))
                } else {
                    row.insert("wing".to_string(), prev)
                }
            }
            None => row.remove("wing"),
        };
        let same_wing = cur_cell
            .as_ref()
            .and_then(Cell::text)
            .is_some_and(|s| s == restored_wing_arc.as_ref());
        if has_complex_idx {
            let mut key_iter = old_keys.into_iter();
            for live_idx in indexes.values_mut() {
                if live_idx.def.collection == "docs" {
                    let old_k = key_iter.next();
                    let new_k = Some(live_idx.key_of(row));
                    let _ = live_idx.update_keys(idx, old_k, new_k);
                }
            }
        } else if !same_wing {
            for live_idx in indexes.values_mut() {
                if live_idx.def.collection == "docs" {
                    if live_idx.def.fields.len() == 1 && live_idx.def.fields[0] == "wing" {
                        let old_k = Some(crate::index::LiveIndex::single_cell_key(
                            cur_cell.as_ref().unwrap_or(&Cell::Null),
                        ));
                        let new_k = Some(crate::index::LiveIndex::single_cell_key(&Cell::Text(
                            Arc::clone(&restored_wing_arc),
                        )));
                        let _ = live_idx.update_keys(idx, old_k, new_k);
                    } else if live_idx.def.fields.len() == 2
                        && live_idx.def.fields[0] == "wing"
                        && live_idx.def.fields[1] == "ts"
                    {
                        let old_k = Some(crate::index::LiveIndex::two_cell_key(
                            cur_cell.as_ref().unwrap_or(&Cell::Null),
                            &ts_cell,
                        ));
                        let new_k = Some(crate::index::LiveIndex::two_cell_key(
                            &Cell::Text(Arc::clone(&restored_wing_arc)),
                            &ts_cell,
                        ));
                        let _ = live_idx.update_keys(idx, old_k, new_k);
                    }
                }
            }
        }
        if !same_wing {
            self.store.sync_docs_wing_only(idx, restored_wing_arc);
        }
    }

    pub(crate) fn run_prepared_in_tx_ext(
        &mut self,
        prepared: &Prepared,
        undo: &mut Undo,
        base_gen: u64,
        want_rows: bool,
    ) -> Result<(Handle, Option<Pack>, bool), Error> {
        if !self.store.fts_deferred_doc_swaps.is_empty() {
            self.store.flush_fts_deferred_docs();
        }
        if let Some(pg) = &prepared.point_get {
            let row = self.execute_point_get(pg);
            let rows = match row {
                Some(r) => vec![r],
                None => Vec::new(),
            };
            let n = rows.len();
            return Ok((
                Handle {
                    plan: Arc::clone(&prepared.plan),
                    rows,
                    done: Done {
                        r#gen: self.store.r#gen,
                        n,
                    },
                    message: None,
                    ms: 0.0,
                },
                None,
                false,
            ));
        }
        #[cfg(feature = "gpu")]
        let has_gpu_agg = self.gpu.is_some()
            && prepared.stmts.first().is_some_and(|s| match s {
                Stmt::Query(q) => self.uses_gpu_filter(q) || self.uses_gpu_count(q),
                _ => false,
            });
        #[cfg(not(feature = "gpu"))]
        let has_gpu_agg = false;
        if !has_gpu_agg && let Some(agg) = &prepared.agg_plan {
            let now = if agg.needs_now { now_ms() } else { 0 };
            let rows = self.execute_agg_plan(agg, now)?;
            let n = rows.len();
            return Ok((
                Handle {
                    plan: Arc::clone(&prepared.plan),
                    rows,
                    done: Done {
                        r#gen: self.store.r#gen,
                        n,
                    },
                    message: None,
                    ms: 0.0,
                },
                None,
                false,
            ));
        }
        if let Some(pu) = &prepared.point_update
            && pu.is_docs_wing_only
            && !self.is_durable()
            && !self.store.is_disk_backed(&pu.collection)
            && pu.cas.is_some()
            && !self
                .catalog
                .guards
                .iter()
                .any(|g| g.collection == "docs" && g.field == "wing")
            && !self.catalog.fks.iter().any(|fk| fk.from_col == "docs")
            && !self
                .store
                .fts
                .get("docs")
                .is_some_and(|f| f.fields.iter().any(|fld| fld == "wing"))
        {
            let idx = if pu.collection == "docs"
                && self.store.docs_epoch == pu.prepared_docs_epoch
                && self.store.fts_deferred_doc_swaps.is_empty()
                && let Some(i) = pu.prepared_idx
                && i < self.store.docs_id.len()
                && (self.store.r#gen == pu.prepared_gen
                    || (pu.field == "id" && self.store.docs_id[i].as_ref() == pu.key.as_str()))
            {
                i
            } else {
                match self.store.point_index(&pu.collection, &pu.field, &pu.key) {
                    Some(i) => i,
                    None => return Err(Error::runtime("update: no matching row")),
                }
            };
            let Store {
                collections,
                indexes,
                ..
            } = &mut self.store;
            let docs_col = collections.get_mut(&pu.collection).unwrap();
            let row = &mut docs_col[idx];
            let got = row_text(row, "hash").unwrap_or("");
            let cas = pu.cas.as_deref().unwrap_or("");
            if got != cas {
                return Err(Error::runtime(format!(
                    "cas mismatch: expected {cas:?}, found {got:?}"
                )));
            }
            let (new_cell, wing_arc) = if let Some((c, a)) = &pu.const_wing {
                (c.clone(), Arc::clone(a))
            } else {
                let now = if super::eval::value_needs_now(&pu.record.fields[0].1) {
                    now_ms()
                } else {
                    0
                };
                let cell = value_cell(&pu.record.fields[0].1, now);
                let arc = cell.text_shared().unwrap_or_default();
                (cell, arc)
            };
            let mut has_complex_idx = false;
            let mut needs_ts = false;
            for live_idx in indexes.values() {
                if live_idx.def.collection == pu.collection {
                    if live_idx.def.pred.is_none()
                        && live_idx.def.fields.len() == 1
                        && live_idx.def.fields[0] == "wing"
                    {
                        // simple wing index
                    } else if live_idx.def.pred.is_none()
                        && live_idx.def.fields.len() == 2
                        && live_idx.def.fields[0] == "wing"
                        && live_idx.def.fields[1] == "ts"
                    {
                        needs_ts = true;
                    } else if live_idx.def.pred.is_some()
                        || live_idx.def.fields.iter().any(|f| f == "wing")
                    {
                        has_complex_idx = true;
                        break;
                    }
                }
            }
            let mut old_keys: smallvec::SmallVec<[crate::index::IndexKey; 4]> =
                smallvec::SmallVec::new();
            if has_complex_idx {
                for live_idx in indexes.values() {
                    if live_idx.def.collection == pu.collection {
                        old_keys.push(live_idx.key_of(row));
                    }
                }
            }
            let ts_cell = if !has_complex_idx && needs_ts {
                row.get("ts").cloned().unwrap_or(Cell::Null)
            } else {
                Cell::Null
            };
            let prev_cell = if let Some(slot) = row.get_mut("wing") {
                Some(std::mem::replace(slot, new_cell))
            } else {
                row.insert("wing".to_string(), new_cell)
            };
            let same_wing = prev_cell
                .as_ref()
                .and_then(Cell::text)
                .is_some_and(|s| s == wing_arc.as_ref());
            if has_complex_idx {
                let mut key_iter = old_keys.into_iter();
                for live_idx in indexes.values_mut() {
                    if live_idx.def.collection == pu.collection {
                        let old_k = key_iter.next();
                        let new_k = Some(live_idx.key_of(row));
                        if let Err(e) = live_idx.update_keys(idx, old_k, new_k) {
                            if let Some(prev) = prev_cell {
                                row.insert("wing".to_string(), prev);
                            } else {
                                row.remove("wing");
                            }
                            return Err(Error::runtime(e));
                        }
                    }
                }
            } else if !same_wing {
                for live_idx in indexes.values_mut() {
                    if live_idx.def.collection == pu.collection {
                        if live_idx.def.fields.len() == 1 && live_idx.def.fields[0] == "wing" {
                            let old_k = Some(crate::index::LiveIndex::single_cell_key(
                                prev_cell.as_ref().unwrap_or(&Cell::Null),
                            ));
                            let new_k = Some(crate::index::LiveIndex::single_cell_key(
                                &Cell::Text(Arc::clone(&wing_arc)),
                            ));
                            if let Err(e) = live_idx.update_keys(idx, old_k, new_k) {
                                if let Some(prev) = prev_cell {
                                    row.insert("wing".to_string(), prev);
                                } else {
                                    row.remove("wing");
                                }
                                return Err(Error::runtime(e));
                            }
                        } else if live_idx.def.fields.len() == 2
                            && live_idx.def.fields[0] == "wing"
                            && live_idx.def.fields[1] == "ts"
                        {
                            let old_k = Some(crate::index::LiveIndex::two_cell_key(
                                prev_cell.as_ref().unwrap_or(&Cell::Null),
                                &ts_cell,
                            ));
                            let new_k = Some(crate::index::LiveIndex::two_cell_key(
                                &Cell::Text(Arc::clone(&wing_arc)),
                                &ts_cell,
                            ));
                            if let Err(e) = live_idx.update_keys(idx, old_k, new_k) {
                                if let Some(prev) = prev_cell {
                                    row.insert("wing".to_string(), prev);
                                } else {
                                    row.remove("wing");
                                }
                                return Err(Error::runtime(e));
                            }
                        }
                    }
                }
            }
            if let Undo::Rows(events) | Undo::Mixed { events, .. } = undo {
                events.push(RowUndo::DocsWing(idx, prev_cell));
            }
            if !same_wing {
                self.store.sync_docs_wing_only(idx, wing_arc);
            }
            self.store.r#gen += 1;
            return Ok((
                Handle {
                    plan: Arc::clone(&prepared.plan),
                    done: Done {
                        n: 1,
                        r#gen: self.store.r#gen,
                    },
                    rows: Vec::new(),
                    message: None,
                    ms: 0.0,
                },
                None,
                true,
            ));
        }

        let t0 = Instant::now();
        let ((rows, message, pack, wrote, affected_hint), _) =
            self.exec_program_ext(&prepared.stmts, undo, false, want_rows)?;
        let ms = t0.elapsed().as_secs_f64() * 1000.0;
        let n = if !rows.is_empty() {
            rows.len()
        } else if affected_hint > 0 {
            affected_hint
        } else {
            pack_affect_n(pack.as_ref()).unwrap_or(0)
        };
        if wrote {
            if self.is_durable() {
                self.store.r#gen = base_gen + 1;
            } else {
                self.store.r#gen += 1;
            }
        }
        let handle = Handle {
            plan: Arc::clone(&prepared.plan),
            done: Done {
                n,
                r#gen: self.store.r#gen,
            },
            rows,
            message,
            ms,
        };
        Ok((handle, pack, wrote))
    }
}
