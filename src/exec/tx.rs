use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use crate::ast::{ExplainKind, Stmt};
use crate::catalog::Catalog;
use crate::check;
use crate::error::Error;
use crate::parse;
use crate::persist::{
    ColData, Pack, RollbackReasonCounters, TxPhaseDiagnostics, inc_rollback_commit_err,
    inc_rollback_dropped, inc_rollback_explicit, inc_rollback_stmt_err, reset_rollback_counters,
    rollback_counters,
};
use crate::plan::{self, Backend, Effect, Node, NodeKind, Plan};
use crate::store::{Cell, Row, Store, now_ms};

use super::dml::{stmt_append_only, stmt_schema, stmt_writes};
use super::eval::{bind_stmt_params, value_cell};
use super::prepared::{ExecPlan, Prepared};
use super::{Db, Done, Handle, pack_affect_n};

static NEXT_TX_ID: AtomicU64 = AtomicU64::new(1);

const TX_META_SHARDS: usize = 64;

#[derive(Clone, Copy)]
struct TxRuntimeState {
    tx_id: u64,
    started_at: Instant,
    statements_run: usize,
    had_stmt_error: bool,
}

static TX_RUNTIME_TABLE: [Mutex<smallvec::SmallVec<[(usize, TxRuntimeState); 2]>>; TX_META_SHARDS] =
    [const { Mutex::new(smallvec::SmallVec::new_const()) }; TX_META_SHARDS];

#[inline]
fn tx_shard_idx(db_key: usize) -> usize {
    ((db_key >> 6) ^ (db_key >> 14)) & (TX_META_SHARDS - 1)
}

fn with_tx_runtime_state<R>(db_key: usize, f: impl FnOnce(&mut TxRuntimeState) -> R) -> R {
    let mut guard = TX_RUNTIME_TABLE[tx_shard_idx(db_key)]
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    if let Some((_, st)) = guard.iter_mut().find(|(k, _)| *k == db_key) {
        return f(st);
    }
    guard.push((
        db_key,
        TxRuntimeState {
            tx_id: NEXT_TX_ID.fetch_add(1, Ordering::Relaxed),
            started_at: Instant::now(),
            statements_run: 0,
            had_stmt_error: false,
        },
    ));
    let last = guard.len() - 1;
    f(&mut guard[last].1)
}

fn take_tx_runtime_state(db_key: usize) -> Option<TxRuntimeState> {
    let mut guard = TX_RUNTIME_TABLE[tx_shard_idx(db_key)]
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let pos = guard.iter().position(|(k, _)| *k == db_key)?;
    Some(guard.swap_remove(pos).1)
}

pub(crate) enum Undo {
    None,
    Append(crate::store::AppendMark),
    Full(Box<crate::store::MemBackup>),
    Rows(smallvec::SmallVec<[RowUndo; 2]>),
    Mixed {
        events: smallvec::SmallVec<[RowUndo; 2]>,
    },
    /// A transaction whose journal had to become a full backup part way through: `full`
    /// restores the state at that point, `prior` then undoes what came before it.
    Layered {
        full: Box<crate::store::MemBackup>,
        prior: Box<Undo>,
    },
}

/// True when one of `stmts` writes a collection that lives in the disk tier. Such writes
/// must be undone with a full backup: the collection-length marks of `Undo::Append` and
/// the row positions of `Undo::Rows` describe in-memory rows only, and a cold collection
/// promoted during the statement would be cut down to its (empty) in-memory length,
/// which the next checkpoint then writes to disk. `Store::mem_restore` re-attaches the
/// untouched disk table instead.
pub(crate) fn stmts_write_disk_collection(store: &crate::store::Store, stmts: &[Stmt]) -> bool {
    if store.disk.is_none() {
        return false;
    }
    stmts.iter().any(|s| match s {
        Stmt::Insert { collection, .. }
        | Stmt::Update { collection, .. }
        | Stmt::Delete { collection, .. }
        | Stmt::Reembed { collection, .. } => store.is_disk_backed(collection),
        _ => false,
    })
}

pub(crate) enum RowUndo {
    DocsWing(usize, Option<u8>, Option<Cell>),
    Update(String, usize, Row),
    Delete(String, Vec<usize>, Vec<Row>),
    /// Collection lengths before an appending statement that follows row events. Undo
    /// replays the journal in exact reverse, so the rows appended after this point are
    /// truncated before any earlier delete is restored: restoring a swap-removed row
    /// first would move a live row to the end, where the truncation would cut it off.
    Append(crate::store::AppendMark),
}

/// Interactive ACID transaction handle.
///
/// Ensures all mutations performed within the transaction are atomic, isolated,
/// and consistent. If dropped without calling [`Self::commit`], all mutations
/// are automatically rolled back.
pub struct Transaction<'a> {
    pub(crate) db: &'a mut Db,
    pub(crate) base_gen: u64,
    pub(crate) base_next_id: u64,
    pub(crate) committed: bool,
    pub(crate) undo: Undo,
    pub(crate) cat_backup: Option<Box<Catalog>>,
    pub(crate) packs: Vec<Pack>,
    pub(crate) schema_changed: bool,
    pub(crate) disk_savepoint: bool,
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

pub(crate) fn cached_single_insert_plan(cat: &Catalog, collection: &str, has_body: bool) -> Arc<Plan> {
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

impl Undo {
    fn has_full_backup(&self) -> bool {
        matches!(self, Undo::Full(_) | Undo::Layered { .. })
    }

    fn entry_count(&self) -> usize {
        match self {
            Undo::None => 0,
            Undo::Append(_) | Undo::Full(_) => 1,
            Undo::Rows(events) | Undo::Mixed { events } => events.len(),
            Undo::Layered { prior, .. } => 1 + prior.entry_count(),
        }
    }
}

impl<'a> Transaction<'a> {
    #[inline]
    fn db_key(&self) -> usize {
        std::ptr::from_ref::<Db>(self.db) as usize
    }

    #[inline]
    fn track_stmt_result<T>(&self, was_committed: bool, res: Result<T, Error>) -> Result<T, Error> {
        if was_committed {
            return res;
        }
        let is_err = res.is_err();
        with_tx_runtime_state(self.db_key(), |st| {
            st.statements_run = st.statements_run.saturating_add(1);
            if is_err {
                st.had_stmt_error = true;
            }
        });
        if is_err {
            inc_rollback_stmt_err();
        }
        res
    }

    /// Unique monotonic replay/diagnostic identifier assigned to this transaction (`LIN-0240`).
    pub fn tx_id(&self) -> u64 {
        with_tx_runtime_state(self.db_key(), |st| st.tx_id)
    }

    /// Snapshot live execution, journal, backup, and wall-time diagnostics for this transaction (`LIN-0237`).
    pub fn diagnostics(&self) -> TxPhaseDiagnostics {
        let (tx_id, statements_run, elapsed_micros) =
            with_tx_runtime_state(self.db_key(), |st| {
                (
                    st.tx_id,
                    st.statements_run,
                    u64::try_from(st.started_at.elapsed().as_micros()).unwrap_or(u64::MAX),
                )
            });
        let journal_entries = self
            .undo
            .entry_count()
            .max(self.packs.len())
            .max(usize::from(self.cat_backup.is_some()))
            .max(usize::from(self.disk_savepoint));
        TxPhaseDiagnostics {
            tx_id,
            statements_run,
            journal_entries,
            has_full_backup: self.undo.has_full_backup(),
            disk_savepoint: self.disk_savepoint,
            elapsed_micros,
        }
    }

    /// Return global monotonic rollback reason counters across transactions (`LIN-0238`).
    pub fn rollback_counters() -> RollbackReasonCounters {
        rollback_counters()
    }

    /// Reset global rollback reason counters to zero (`LIN-0238`).
    pub fn reset_rollback_counters() {
        reset_rollback_counters();
    }

    /// Construct an explicit runtime rejection error for nested interactive transactions (`LIN-0220`).
    ///
    /// Note: Rust's `&mut Db` borrow rules already prevent opening two simultaneous
    /// [`Db::transaction`] handles on the same database at compile time; dynamic query routers
    /// and scripting bindings can call [`Self::begin_nested`] / [`Self::reject_nested`] to surface
    /// a deterministic runtime diagnostic.
    pub fn reject_nested(&self) -> Error {
        Error::runtime(format!(
            "nested interactive transactions are unsupported (active tx_id={}); commit or roll back the outer transaction first",
            self.tx_id()
        ))
    }

    /// Explicitly reject dynamic attempts to open a nested interactive transaction (`LIN-0220`).
    pub fn begin_nested(&mut self) -> Result<(), Error> {
        Err(self.reject_nested())
    }

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
        let was_committed = self.committed;
        let res = self.run_src_ext_inner(src, want_rows);
        self.track_stmt_result(was_committed, res)
    }

    fn run_src_ext_inner(&mut self, src: &str, want_rows: bool) -> Result<Handle, Error> {
        if self.committed {
            return Err(Error::runtime("transaction already committed"));
        }
        if !self.db.plan_cache.is_empty()
            && let Some(ep) = self.db.plan_cache.get(src).map(Prepared::exec_plan)
        {
            return self.run_exec_plan_ext(&ep, want_rows);
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
            let ep = prep.exec_plan();
            self.db.insert_plan_cache(src.to_string(), prep);
            return self.run_exec_plan_ext(&ep, want_rows);
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
            let ep = prep.exec_plan();
            self.db.insert_plan_cache(src.to_string(), prep);
            return self.run_exec_plan_ext(&ep, want_rows);
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
        if self.db.store.can_apply_disk_overlay(collection) {
            let rec = crate::ast::Record {
                fields: fields
                    .into_iter()
                    .map(|(k, v)| (k.to_string(), v))
                    .collect(),
            };
            return self.exec_single_insert_tx(collection.to_string(), rec, want_rows);
        }
        if self.db.store.is_disk_backed(collection) {
            self.ensure_undo_flags(&[], false, true);
        } else if !matches!(self.undo, Undo::Append(_)) {
            self.ensure_undo_flags(&[], true, false);
        }
        if self.db.store.has_deferred_doc_ops() {
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
                    if !uid_a.is_empty()
                        && !self.db.catalog.fks.is_empty()
                        && self.db.catalog.fks.len() == 1
                    {
                        let fk = &self.db.catalog.fks[0];
                        if fk.from_col == "orders"
                            && fk.from_field == "user_id"
                            && fk.to_col == "users"
                            && fk.to_field == "id"
                            && !self
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
                    let tot_tag = crate::store::order_total_tag(Some(&tot_c));
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
                    self.record_tx_single_insert("orders", &row)?;
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
                    self.db.store.orders_total_tag.push(tot_tag);
                    self.db.store.update_col_count("orders", 1);

                    let plan = if want_rows {
                        cached_single_insert_plan(&self.db.catalog, "orders", false)
                    } else {
                        dummy_dml_plan()
                    };
                    self.db.store.collection_append_mut("orders").push(row);
                    if self.db.is_durable() {
                        self.db.store.r#gen = self.base_gen + 1;
                    } else {
                        self.db.store.r#gen += 1;
                    }
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

        // General borrowed single-row insert for all collections (zero `Record` / key `String` allocs)
        let mut has_body = false;
        let mut has_hash = false;
        let mut id_arc: Option<Arc<str>> = None;
        let mut uri_arc: Option<Arc<str>> = None;
        let mut body_arc: Option<Arc<str>> = None;
        let mut row = crate::store::acquire_row();
        for (k, v) in fields {
            let cell = value_cell(&v, now);
            match k {
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
            row.insert("id", Cell::Text(Arc::clone(&gen_id)));
            id_arc = Some(gen_id);
        }
        if has_body
            && !has_hash
            && let Some(body) = body_arc
        {
            row.insert(
                "hash",
                Cell::SmallText(crate::store::content_hash_inline(body.as_ref())),
            );
        }
        if !self.db.store.collection(collection).is_empty() {
            if let Some(ref id) = id_arc
                && self.db.store.row_index(collection, id.as_ref()).is_some()
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
            self.db.check_row_fks(collection, &row)?;
        }
        if needs_embed {
            self.db
                .maybe_embed_rows(collection, std::slice::from_mut(&mut row))?;
        }

        let start = self.db.store.collection(collection).len();
        if !self.db.store.indexes.is_empty() {
            self.db.store.index_insert_row(collection, start, &row)?;
        }
        self.record_tx_single_insert(collection, &row)?;
        if !self.db.store.fts.is_empty() && self.db.store.fts.contains_key(collection) {
            self.db.store.fts_insert_row(collection, start, &row);
        }
        if !self.db.store.hnsw.is_empty()
            && let Some(Cell::Vec(v)) = row.get("embedding")
            && let Some(hnsw) = self.db.store.hnsw.get_mut(collection)
        {
            hnsw.insert(start, Arc::clone(v));
        }
        self.db.store.row_maps_register_row(collection, start, &row);
        self.db.store.update_col_count(collection, 1);

        let plan = if want_rows {
            cached_single_insert_plan(&self.db.catalog, collection, has_body)
        } else {
            dummy_dml_plan()
        };
        let rows = if want_rows {
            vec![row.clone()]
        } else {
            Vec::new()
        };
        self.db.store.collection_append_mut(collection).push(row);

        if self.db.is_durable() {
            self.db.store.r#gen = self.base_gen + 1;
        } else {
            self.db.store.r#gen += 1;
        }

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

    fn exec_single_insert_tx(
        &mut self,
        collection: String,
        rec: crate::ast::Record,
        want_rows: bool,
    ) -> Result<Handle, Error> {
        if self.db.store.has_deferred_doc_ops() {
            self.db.store.flush_fts_deferred_docs();
        }
        if self.db.store.can_apply_disk_overlay(&collection) {
            let had_sp = self.disk_savepoint;
            self.ensure_disk_savepoint();
            let has_body = rec.fields.iter().any(|(k, _)| k == "body");
            let plan = if want_rows {
                cached_single_insert_plan(&self.db.catalog, &collection, has_body)
            } else {
                dummy_dml_plan()
            };
            self.db.cold_program_packs = Some(Vec::with_capacity(1));
            let res = self.db.cold_overlay_insert(
                &collection,
                std::slice::from_ref(&rec),
                None,
                &plan,
                want_rows,
            );
            let mut staged = self.db.cold_program_packs.take().unwrap_or_default();
            match res {
                Ok(Some(mut handle)) => {
                    let pack = if self.db.is_durable() {
                        self.db.store.r#gen = self.base_gen + 1;
                        staged.pop()
                    } else {
                        self.db.store.r#gen += 1;
                        None
                    };
                    handle.done.r#gen = self.db.store.r#gen;
                    self.record_tx_write(1, pack)?;
                    return Ok(handle);
                }
                Ok(None) => {
                    if !had_sp {
                        self.db.store.rollback_disk_overlay_savepoint();
                        self.disk_savepoint = false;
                    }
                }
                Err(e) => {
                    if !had_sp {
                        self.db.store.rollback_disk_overlay_savepoint();
                        self.db.store.next_id = self.base_next_id;
                        self.disk_savepoint = false;
                    }
                    return Err(e);
                }
            }
        }
        if self.db.store.is_disk_backed(&collection) {
            self.ensure_undo_flags(&[], false, true);
            self.db.store.promote_disk_col(&collection);
        } else if !matches!(self.undo, Undo::Append(_)) {
            self.ensure_undo_flags(&[], true, false);
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
            row.insert(k.as_str(), cell);
        }
        if id_arc.is_none() {
            let gen_id: Arc<str> = Arc::from(self.db.store.alloc_id());
            row.insert("id", Cell::Text(Arc::clone(&gen_id)));
            id_arc = Some(gen_id);
        }
        if has_body
            && !has_hash
            && let Some(body) = body_arc
        {
            row.insert(
                "hash",
                Cell::SmallText(crate::store::content_hash_inline(body.as_ref())),
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
                .maybe_embed_rows(&collection, std::slice::from_mut(&mut row))?;
        }

        // Fast inline registration + index + FTS + HNSW without multi-slab overhead
        let start = self.db.store.collection(&collection).len();
        if !self.db.store.indexes.is_empty() {
            self.db.store.index_insert_row(&collection, start, &row)?;
        }
        self.record_tx_single_insert(&collection, &row)?;
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
        let rows = if want_rows {
            vec![row.clone()]
        } else {
            Vec::new()
        };
        self.db.store.collection_append_mut(&collection).push(row);

        if self.db.is_durable() {
            self.db.store.r#gen = self.base_gen + 1;
        } else {
            self.db.store.r#gen += 1;
        }

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

    #[inline]
    fn ensure_disk_savepoint(&mut self) {
        if !self.disk_savepoint {
            self.db.store.begin_disk_overlay_savepoint();
            self.disk_savepoint = true;
        }
    }

    fn has_cold_write_target(&self, stmts: &[Stmt]) -> bool {
        if self.db.store.disk.is_none() {
            return false;
        }
        stmts.iter().any(|s| match s {
            Stmt::Insert { collection, .. }
            | Stmt::Update { collection, .. }
            | Stmt::Delete { collection, .. } => self.db.store.can_apply_disk_overlay(collection),
            _ => false,
        })
    }

    fn abort_into_rollback(&mut self) {
        let undo = std::mem::replace(&mut self.undo, Undo::None);
        let cat = self.cat_backup.take();
        let had_disk_sp = self.disk_savepoint;
        self.db.rollback(undo, cat, self.base_gen);
        if had_disk_sp {
            self.db.store.rollback_disk_overlay_savepoint();
            self.db.store.next_id = self.base_next_id;
            self.db.store.r#gen = self.base_gen;
            self.db.reader_cache = None;
            self.disk_savepoint = false;
        }
        self.packs.clear();
    }

    fn ensure_undo_flags(&mut self, stmts: &[Stmt], append_only: bool, schema: bool) {
        // Writes to a cold collection need a full backup (see `stmts_write_disk_collection`).
        let disk = stmts_write_disk_collection(&self.db.store, stmts);
        let appends = append_only && !schema && !disk;
        let row_undo = !schema
            && !disk
            && stmts.iter().all(|s| {
                matches!(
                    s,
                    Stmt::Update { .. } | Stmt::Delete { .. } | Stmt::Query(_) | Stmt::Let { .. }
                )
            });
        enum Next {
            Keep,
            Full,
            Append,
            Rows,
            MixedFromAppend,
            MixedFromRows,
            MarkAppend,
        }
        let next = match &self.undo {
            Undo::None if appends => Next::Append,
            Undo::None if row_undo => Next::Rows,
            Undo::None => Next::Full,
            Undo::Append(_) if appends => Next::Keep,
            Undo::Append(_) if row_undo => Next::MixedFromAppend,
            Undo::Append(_) => Next::Full,
            Undo::Rows(_) if appends => Next::MixedFromRows,
            Undo::Rows(_) if row_undo => Next::Keep,
            Undo::Rows(_) => Next::Full,
            Undo::Mixed { .. } if appends => Next::MarkAppend,
            Undo::Mixed { .. } if row_undo => Next::Keep,
            Undo::Mixed { .. } => Next::Full,
            Undo::Full(_) | Undo::Layered { .. } => Next::Keep,
        };
        match next {
            Next::Keep => {}
            Next::Append => self.undo = Undo::Append(self.db.store.append_mark()),
            Next::Rows => self.undo = Undo::Rows(smallvec::SmallVec::new()),
            Next::MixedFromAppend => {
                // The appends came first: their mark opens the event journal.
                if let Undo::Append(mark) = std::mem::replace(&mut self.undo, Undo::None) {
                    let mut events = smallvec::SmallVec::new();
                    events.push(RowUndo::Append(mark));
                    self.undo = Undo::Mixed { events };
                }
            }
            Next::MixedFromRows => {
                if let Undo::Rows(mut events) = std::mem::replace(&mut self.undo, Undo::None) {
                    events.push(RowUndo::Append(self.db.store.append_mark()));
                    self.undo = Undo::Mixed { events };
                }
            }
            Next::MarkAppend => {
                // Appends after row events get their own mark (see `RowUndo::Append`).
                let mark = self.db.store.append_mark();
                if let Undo::Mixed { events } = &mut self.undo
                    && !events.is_empty()
                {
                    events.push(RowUndo::Append(mark));
                }
            }
            Next::Full => {
                // Keep what the journal already holds: it undoes the writes made before
                // this point, which the new backup does not cover.
                let full = Box::new(self.db.store.mem_backup());
                self.undo = match std::mem::replace(&mut self.undo, Undo::None) {
                    Undo::None => Undo::Full(full),
                    prior => Undo::Layered {
                        full,
                        prior: Box::new(prior),
                    },
                };
                if self.db.store.disk.is_some() {
                    self.disk_savepoint = true;
                }
            }
        }
    }

    /// Record a 1-row insert in `self.packs`, coalescing onto the trailing `Pack::InsertCols`
    /// when it targets the same collection and compatible column types so interactive loops
    /// avoid allocating 9 `Vec`/`String`s per row.
    fn record_tx_single_insert(&mut self, collection: &str, row: &Row) -> Result<(), Error> {
        let next_mutations = self.mutation_count.saturating_add(1);
        if next_mutations > self.db.quotas.max_tx_mutations {
            return Err(Error::runtime(format!(
                "transaction mutation count {} exceeds max_tx_mutations quota {}",
                next_mutations, self.db.quotas.max_tx_mutations
            )));
        }
        if !self.db.is_durable() {
            self.mutation_count = next_mutations;
            return Ok(());
        }
        if self.try_append_row_to_last_pack(collection, row, next_mutations)? {
            return Ok(());
        }
        let pack = crate::persist::rows_to_insert_pack(
            collection.to_string(),
            std::slice::from_ref(row),
            Vec::new(),
        );
        self.record_tx_write(1, Some(pack))
    }

    fn try_append_row_to_last_pack(
        &mut self,
        collection: &str,
        row: &Row,
        next_mutations: usize,
    ) -> Result<bool, Error> {
        let Some(Pack::InsertCols {
            collection: last_col,
            fields,
            cols,
            n,
            edges,
        }) = self.packs.last_mut()
        else {
            return Ok(false);
        };
        if last_col != collection || !edges.is_empty() {
            return Ok(false);
        }
        let Some(shape) = row.shape_arc() else {
            return Ok(false);
        };
        let names = shape.names();
        if names.len() != fields.len()
            || cols.len() != fields.len()
            || !names.iter().zip(fields.iter()).all(|(a, b)| a == b)
        {
            return Ok(false);
        }
        let cells = row.cells_slice();
        let mut delta_bytes = 0usize;
        for (cell, col) in cells.iter().zip(cols.iter()) {
            match (cell, col) {
                (Cell::Text(s), ColData::Text(_)) => delta_bytes += s.len(),
                (Cell::SmallText(s), ColData::Text(_)) => delta_bytes += s.len(),
                (Cell::Int(_), ColData::Int(_)) => delta_bytes += 8,
                (Cell::Float(_), ColData::Float(_)) => delta_bytes += 8,
                (Cell::Bool(_), ColData::Bool(_)) => delta_bytes += 1,
                (Cell::Time(_), ColData::Time(_)) => delta_bytes += 8,
                (Cell::Vec(v), ColData::Vec(_)) if !v.is_empty() => delta_bytes += 32,
                _ => return Ok(false),
            }
        }
        let next_journal = self.journal_bytes.saturating_add(delta_bytes);
        if next_journal > self.db.quotas.max_journal_bytes {
            return Err(Error::runtime(format!(
                "transaction journal bytes {} exceeds max_journal_bytes quota {}",
                next_journal, self.db.quotas.max_journal_bytes
            )));
        }
        for (cell, col) in cells.iter().zip(cols.iter_mut()) {
            match (cell, col) {
                (_, ColData::Text(v)) => {
                    v.push(cell.text_shared().unwrap_or_else(|| Arc::from("")));
                }
                (Cell::Int(x), ColData::Int(v)) => v.push(*x),
                (Cell::Float(x), ColData::Float(v)) => v.push(*x),
                (Cell::Bool(x), ColData::Bool(v)) => v.push(*x),
                (Cell::Time(x), ColData::Time(v)) => v.push(*x),
                (Cell::Vec(vec), ColData::Vec(v)) => v.push(Some(Arc::clone(vec))),
                _ => unreachable!("compatibility verified above"),
            }
        }
        *n = n.saturating_add(1);
        self.mutation_count = next_mutations;
        self.journal_bytes = next_journal;
        Ok(true)
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
        if writes
            && !schema
            && self.has_cold_write_target(stmts)
            && (stmts.len() == 1 || !self.disk_savepoint)
        {
            if self.db.store.has_deferred_doc_ops() {
                self.db.store.flush_fts_deferred_docs();
            }
            let had_sp = self.disk_savepoint;
            self.ensure_disk_savepoint();
            match self
                .db
                .try_cold_overlay_in_tx(stmts, &plan, self.base_gen, want_rows)
            {
                Ok(Some((handle, pack, _wrote))) => {
                    let tx_pack = if self.db.is_durable() { pack } else { None };
                    self.record_tx_write(handle.done.n, tx_pack)?;
                    return Ok(handle);
                }
                Ok(None) => {
                    if !had_sp {
                        self.db.store.rollback_disk_overlay_savepoint();
                        self.disk_savepoint = false;
                    }
                }
                Err(e) => {
                    if !had_sp {
                        self.db.store.rollback_disk_overlay_savepoint();
                        self.db.store.next_id = self.base_next_id;
                        self.disk_savepoint = false;
                    }
                    return Err(e);
                }
            }
        }
        if writes && !append_only && !schema && self.db.store.has_deferred_doc_ops() {
            self.db.store.flush_fts_deferred_docs();
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
        let was_committed = self.committed;
        let bound: smallvec::SmallVec<[Stmt; 2]> = prep
            .stmts
            .iter()
            .map(|s| bind_stmt_params(s, params))
            .collect();
        let res = self.run_stmts_direct(
            &bound,
            Arc::clone(&prep.plan),
            prep.writes,
            prep.append_only,
            prep.schema,
            true,
        );
        self.track_stmt_result(was_committed, res)
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
        let was_committed = self.committed;
        let bound: smallvec::SmallVec<[Stmt; 2]> = prep
            .stmts
            .iter()
            .map(|s| bind_stmt_params(s, params))
            .collect();
        let res = self
            .run_stmts_direct(
                &bound,
                Arc::clone(&prep.plan),
                prep.writes,
                prep.append_only,
                prep.schema,
                false,
            )
            .map(|h| h.done);
        self.track_stmt_result(was_committed, res)
    }

    #[inline]
    pub(crate) fn run_prepared_ext(
        &mut self,
        prep: &Prepared,
        want_rows: bool,
    ) -> Result<Handle, Error> {
        let was_committed = self.committed;
        let ep = prep.exec_plan();
        let res = self.run_exec_plan_ext(&ep, want_rows);
        self.track_stmt_result(was_committed, res)
    }

    pub(crate) fn run_exec_plan_ext(
        &mut self,
        prep: &ExecPlan,
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
        if prep.writes && !prep.schema && self.db.store.disk.is_some() {
            if let Some(pd) = &prep.point_delete
                && self.db.store.can_apply_disk_overlay(&pd.collection)
            {
                if !pd.append_only && !pd.cas_each && pd.cas.is_none() {
                    return Err(Error::runtime("delete requires cas"));
                }
                if self.db.store.has_deferred_doc_ops() {
                    self.db.store.flush_fts_deferred_docs();
                }
                let had_sp = self.disk_savepoint;
                self.ensure_disk_savepoint();
                self.db.cold_program_packs = Some(Vec::with_capacity(1));
                let res = self.db.exec_cold_point_delete(pd, &prep.plan, want_rows);
                let mut staged = self.db.cold_program_packs.take().unwrap_or_default();
                let mut handle = match res {
                    Ok(h) => h,
                    Err(e) => {
                        if !had_sp {
                            self.db.store.rollback_disk_overlay_savepoint();
                            self.disk_savepoint = false;
                        }
                        return Err(e);
                    }
                };
                let pack = if self.db.is_durable() {
                    self.db.store.r#gen = self.base_gen + 1;
                    staged.pop()
                } else {
                    self.db.store.r#gen += 1;
                    None
                };
                handle.done.r#gen = self.db.store.r#gen;
                self.record_tx_write(handle.done.n, pack)?;
                return Ok(handle);
            }
            if let Some(pu) = &prep.point_update
                && self.db.store.can_apply_disk_overlay(&pu.collection)
                && !pu.record.fields.iter().any(|(k, _)| {
                    self.db.store.unique_key_needs_memory(&pu.collection, k)
                })
            {
                if !pu.cas_each && pu.cas.is_none() {
                    return Err(Error::runtime("update requires cas"));
                }
                if self.db.store.has_deferred_doc_ops() {
                    self.db.store.flush_fts_deferred_docs();
                }
                let had_sp = self.disk_savepoint;
                self.ensure_disk_savepoint();
                self.db.cold_program_packs = Some(Vec::with_capacity(1));
                let res = self.db.exec_cold_point_update(pu, &prep.plan, want_rows);
                let mut staged = self.db.cold_program_packs.take().unwrap_or_default();
                let mut handle = match res {
                    Ok(h) => h,
                    Err(e) => {
                        if !had_sp {
                            self.db.store.rollback_disk_overlay_savepoint();
                            self.disk_savepoint = false;
                        }
                        return Err(e);
                    }
                };
                let pack = if self.db.is_durable() {
                    self.db.store.r#gen = self.base_gen + 1;
                    staged.pop()
                } else {
                    self.db.store.r#gen += 1;
                    None
                };
                handle.done.r#gen = self.db.store.r#gen;
                self.record_tx_write(handle.done.n, pack)?;
                return Ok(handle);
            }
            if self.has_cold_write_target(&prep.stmts)
                && (prep.stmts.len() == 1 || !self.disk_savepoint)
            {
                if self.db.store.has_deferred_doc_ops() {
                    self.db.store.flush_fts_deferred_docs();
                }
                let had_sp = self.disk_savepoint;
                self.ensure_disk_savepoint();
                match self.db.try_cold_overlay_in_tx(
                    &prep.stmts,
                    &prep.plan,
                    self.base_gen,
                    want_rows,
                ) {
                    Ok(Some((handle, pack, _wrote))) => {
                        let tx_pack = if self.db.is_durable() { pack } else { None };
                        self.record_tx_write(handle.done.n, tx_pack)?;
                        return Ok(handle);
                    }
                    Ok(None) => {
                        if !had_sp {
                            self.db.store.rollback_disk_overlay_savepoint();
                            self.disk_savepoint = false;
                        }
                    }
                    Err(e) => {
                        if !had_sp {
                            self.db.store.rollback_disk_overlay_savepoint();
                            self.db.store.next_id = self.base_next_id;
                            self.disk_savepoint = false;
                        }
                        return Err(e);
                    }
                }
            }
        }
        if prep.writes {
            if !prep.schema
                && !prep.append_only
                && self.db.store.disk.is_none()
                && (prep.point_update.is_some() || prep.point_delete.is_some())
                && matches!(self.undo, Undo::None | Undo::Rows(_))
            {
                if matches!(self.undo, Undo::None) {
                    self.undo = Undo::Rows(smallvec::SmallVec::new());
                }
            } else {
                self.ensure_undo_flags(&prep.stmts, prep.append_only, prep.schema);
            }
        }
        let (handle, pack, _wrote) =
            self.db
                .run_exec_plan_in_tx_ext(prep, &mut self.undo, self.base_gen, want_rows)?;
        if prep.writes {
            self.record_tx_write(handle.done.n, pack)?;
        }
        Ok(handle)
    }

    /// Commit the transaction, making all mutations permanent and writing to WAL if durable.
    pub fn commit(mut self) -> Result<(), Error> {
        if self.committed {
            return Ok(());
        }
        if take_tx_runtime_state(self.db_key()).is_none() {
            NEXT_TX_ID.fetch_add(1, Ordering::Relaxed);
        }
        if let Err(e) = self.db.check_quotas() {
            self.abort_into_rollback();
            self.committed = true;
            inc_rollback_commit_err();
            return Err(e);
        }
        if !self.packs.is_empty() {
            let packs = std::mem::take(&mut self.packs);
            if let Some(combined) = coalesce_tx_packs(packs) {
                if !self.disk_savepoint
                    && let Some(p) = self.db.persist.as_mut()
                    && p.log_bytes > self.db.quotas.max_log_bytes
                    && let Err(e) = self.db.store.checkpoint(p)
                {
                    self.abort_into_rollback();
                    self.committed = true;
                    inc_rollback_commit_err();
                    return Err(e);
                }
                if self.db.persist.is_some() {
                    if self.schema_changed
                        && let Some(p) = self.db.persist.as_mut()
                    {
                        p.catalog_hash = self.db.catalog.hash_str().to_string();
                    }
                    let new_gen = match self
                        .db
                        .durable_commit_batch_or_chunks(combined, self.base_gen)
                    {
                        Ok(g) => g,
                        Err(e) => {
                            self.abort_into_rollback();
                            self.committed = true;
                            inc_rollback_commit_err();
                            return Err(e);
                        }
                    };
                    self.db.store.r#gen = new_gen;
                    if self.disk_savepoint {
                        self.db.store.commit_disk_overlay_savepoint();
                        self.disk_savepoint = false;
                    }
                    if let Some(p) = self.db.persist.as_mut() {
                        if p.log_bytes > self.db.quotas.max_log_bytes {
                            self.db.store.checkpoint(p)?;
                        } else {
                            self.db.store.maybe_checkpoint(p)?;
                        }
                    }
                } else if self.db.store.is_native_disk() {
                    if self.disk_savepoint {
                        self.db.store.rollback_disk_overlay_savepoint();
                        self.disk_savepoint = false;
                    }
                    if let Err(e) = self.db.store.commit_pack_to_native_disk(&combined) {
                        self.abort_into_rollback();
                        self.committed = true;
                        inc_rollback_commit_err();
                        return Err(e);
                    }
                    self.db.store.r#gen = self.base_gen + 1;
                }
            }
        }
        if self.disk_savepoint {
            self.db.store.commit_disk_overlay_savepoint();
            self.disk_savepoint = false;
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
        if take_tx_runtime_state(self.db_key()).is_none() {
            NEXT_TX_ID.fetch_add(1, Ordering::Relaxed);
        }
        self.abort_into_rollback();
        self.committed = true;
        inc_rollback_explicit();
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
            let had_stmt_err = match take_tx_runtime_state(self.db_key()) {
                Some(st) => st.had_stmt_error,
                None => {
                    NEXT_TX_ID.fetch_add(1, Ordering::Relaxed);
                    false
                }
            };
            self.abort_into_rollback();
            if !had_stmt_err {
                inc_rollback_dropped();
            }
        }
    }
}

impl Db {
    pub(crate) fn rollback(&mut self, undo: Undo, cat_backup: Option<Box<Catalog>>, base_gen: u64) {
        let had_undo = !matches!(undo, Undo::None) || cat_backup.is_some();
        self.undo_journal(undo);
        if had_undo {
            self.store.r#gen = base_gen;
            self.reader_cache = None;
        }
        if let Some(cat) = cat_backup {
            self.catalog = *cat;
            self.clear_plan_cache();
        }
    }

    /// Undo the writes `undo` journals, newest first.
    fn undo_journal(&mut self, undo: Undo) {
        match undo {
            Undo::None => {}
            Undo::Append(mark) => self.store.append_rollback(mark),
            Undo::Full(b) => self.store.mem_restore(*b),
            Undo::Layered { full, prior } => {
                self.store.mem_restore(*full);
                self.undo_journal(*prior);
            }
            // Every append is covered by a mark inside `events`, so the journal alone,
            // replayed in reverse, undoes everything; truncating to the opening mark
            // afterwards would cut rows a restored delete moved to the end.
            Undo::Rows(events) | Undo::Mixed { events } => {
                for event in events.into_iter().rev() {
                    match event {
                        RowUndo::DocsWing(idx, wing_slot, prev_cell) => {
                            self.restore_docs_wing_cell(idx, wing_slot, prev_cell);
                        }
                        RowUndo::Update(collection, idx, row) => {
                            self.store.restore_row_owned(&collection, idx, row);
                        }
                        RowUndo::Delete(collection, positions, rows) => {
                            self.store.restore_rows_at(&collection, &positions, &rows);
                        }
                        RowUndo::Append(mark) => self.store.append_rollback(mark),
                    }
                }
            }
        }
    }

    pub(crate) fn restore_docs_wing_cell(
        &mut self,
        idx: usize,
        wing_slot: Option<u8>,
        prev_cell: Option<Cell>,
    ) {
        let valid_wing_slot = |row: &Row, wing_slot: Option<u8>| -> Option<usize> {
            let ws = wing_slot? as usize;
            if row
                .shape_arc()
                .is_some_and(|sh| sh.names().get(ws).map(String::as_str) == Some("wing"))
                && ws < row.cells_slice().len()
            {
                Some(ws)
            } else {
                None
            }
        };
        let Store {
            collections,
            docs_wing,
            deferred_doc_wing_updates,
            ..
        } = &mut self.store;
        if let Some(docs_col) = collections.get_mut("docs")
            && idx < docs_col.len()
        {
            let row = &docs_col[idx];
            let slot_idx = valid_wing_slot(row, wing_slot);
            if deferred_doc_wing_updates.last().is_some_and(|d| {
                let cur_wing = match slot_idx {
                    Some(ws) => row.cells_slice()[ws].text(),
                    None => row.get("wing").and_then(Cell::text),
                };
                d.idx == idx
                    && d.prev_wing.text() == prev_cell.as_ref().and_then(Cell::text)
                    && cur_wing == Some(d.new_wing.as_ref())
            }) {
                deferred_doc_wing_updates.pop();
                let row = &mut docs_col[idx];
                let wing_str = match prev_cell {
                    Some(prev) => {
                        if let Some(ws) = slot_idx {
                            row.cells_mut()[ws] = prev;
                            row.cells_slice()[ws].text().unwrap_or("")
                        } else if let Some(slot) = row.get_mut("wing") {
                            *slot = prev;
                            slot.text().unwrap_or("")
                        } else {
                            row.insert("wing", prev);
                            row.get("wing").and_then(Cell::text).unwrap_or("")
                        }
                    }
                    None => {
                        row.remove("wing");
                        ""
                    }
                };
                if idx < docs_wing.len() {
                    docs_wing.set_str(idx, wing_str);
                }
                return;
            }
        }
        if !deferred_doc_wing_updates.is_empty() {
            self.store.flush_fts_deferred_docs();
        }
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
        let slot_idx = valid_wing_slot(row, wing_slot);
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
                if let Some(ws) = slot_idx {
                    Some(std::mem::replace(&mut row.cells_mut()[ws], prev))
                } else if let Some(slot) = row.get_mut("wing") {
                    Some(std::mem::replace(slot, prev))
                } else {
                    row.insert("wing", prev)
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

    pub(crate) fn run_exec_plan_in_tx_ext(
        &mut self,
        prepared: &ExecPlan,
        undo: &mut Undo,
        base_gen: u64,
        want_rows: bool,
    ) -> Result<(Handle, Option<Pack>, bool), Error> {
        let can_coalesce_wing_update = prepared
            .point_update
            .as_ref()
            .is_some_and(|pu| pu.is_docs_wing_only)
            && self.store.deferred_doc_wing_updates.len()
                < self.store.deferred_doc_wing_updates.inline_size();
        if !self.store.fts_deferred_doc_swaps.is_empty()
            || (!self.store.deferred_doc_wing_updates.is_empty() && !can_coalesce_wing_update)
        {
            self.store.flush_fts_deferred_docs();
        }
        if let Some(pg) = &prepared.point_get {
            if !want_rows
                && pg.is_docs_id_title
                && self.store.disk.is_none()
                && self.store.fts_deferred_doc_swaps.is_empty()
            {
                let exists = (pg.prepared_idx.is_some_and(|i| {
                    i < self.store.docs_id.len()
                        && (self.store.docs_epoch == pg.prepared_docs_epoch
                            || self.store.docs_id[i].as_ref() == pg.key.as_str())
                })) || self.store.row_index("docs", &pg.key).is_some();
                return Ok((
                    Handle {
                        plan: Arc::clone(&prepared.plan),
                        rows: Vec::new(),
                        done: Done {
                            r#gen: self.store.r#gen,
                            n: usize::from(exists),
                        },
                        message: None,
                        ms: 0.0,
                    },
                    None,
                    false,
                ));
            }
            let (rows, n) = match self.execute_point_get(pg) {
                Some(r) => (
                    if want_rows {
                        crate::store::wrap_single_row_vec(r)
                    } else {
                        Vec::new()
                    },
                    1,
                ),
                None => (Vec::new(), 0),
            };
            return Ok((
                Handle {
                    plan: Arc::clone(&prepared.plan),
                    done: Done {
                        r#gen: self.store.r#gen,
                        n,
                    },
                    rows,
                    message: None,
                    ms: 0.0,
                },
                None,
                false,
            ));
        }
        if let Some(sp) = &prepared.scan_project {
            if sp.collection == "facts"
                && self.store.facts_rows_stale
                && !self.store.is_disk_backed("facts")
            {
                self.store.ensure_facts_rows();
            }
            let (rows, n) = self.execute_scan_project(sp, want_rows)?;
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
                    rows: if want_rows { rows } else { Vec::new() },
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
        if let Some(pd) = &prepared.point_delete {
            if !pd.append_only && !pd.cas_each && pd.cas.is_none() {
                return Err(Error::runtime("delete requires cas"));
            }
            if !self.store.fts_deferred_doc_swaps.is_empty() {
                self.store.flush_fts_deferred_docs();
            }
            if self.store.is_disk_backed(&pd.collection) {
                self.store.promote_disk_col(&pd.collection);
            }
            let idx = if pd.collection == "docs"
                && pd.field == "id"
                && self.store.fts_deferred_doc_swaps.is_empty()
                && let Some(i) = pd.prepared_idx
                && self
                    .store
                    .docs_id
                    .get(i)
                    .is_some_and(|id| id.as_ref() == pd.key.as_str())
            {
                i
            } else {
                match self.store.point_index(&pd.collection, &pd.field, &pd.key) {
                    Some(i) => i,
                    None => return Err(Error::runtime("delete: no matching row")),
                }
            };
            {
                let rows = self.store.collection(&pd.collection);
                if !pd.append_only && !pd.cas_each {
                    let got = crate::store::row_text(&rows[idx], "hash").unwrap_or("");
                    let cas = pd.cas.as_deref().unwrap_or("");
                    if got != cas {
                        return Err(Error::runtime(format!(
                            "cas mismatch: expected {cas:?}, found {got:?}"
                        )));
                    }
                }
                if self.catalog.fks.iter().any(|fk| fk.to_col == pd.collection) {
                    self.ensure_not_referenced_idxs(&pd.collection, rows, &[idx])?;
                }
            }
            let durable = self.is_durable();
            let mut out = self.store.delete_rows_at(&pd.collection, &[idx]);
            if let Undo::Rows(events) | Undo::Mixed { events } = undo {
                let undo_rows = if durable || want_rows {
                    out.clone()
                } else {
                    std::mem::take(&mut out)
                };
                events.push(RowUndo::Delete(pd.collection.clone(), vec![idx], undo_rows));
            }
            let pack = if durable {
                Some(Pack::Delete {
                    collection: pd.collection.clone(),
                    rows: if want_rows {
                        out.clone()
                    } else {
                        std::mem::take(&mut out)
                    },
                })
            } else {
                None
            };
            let ret_rows = if want_rows { out } else { Vec::new() };
            if durable {
                self.store.r#gen = base_gen + 1;
            } else {
                self.store.r#gen += 1;
            }
            return Ok((
                Handle {
                    plan: Arc::clone(&prepared.plan),
                    done: Done {
                        n: 1,
                        r#gen: self.store.r#gen,
                    },
                    rows: ret_rows,
                    message: None,
                    ms: 0.0,
                },
                pack,
                true,
            ));
        }
        if let Some(pu) = &prepared.point_update {
            if !pu.cas_each && pu.cas.is_none() {
                return Err(Error::runtime("update requires cas"));
            }
            if self.store.is_disk_backed(&pu.collection) {
                self.store.promote_disk_col(&pu.collection);
            }
            let idx = if pu.collection == "docs"
                && self.store.fts_deferred_doc_swaps.is_empty()
                && let Some(i) = pu.prepared_idx
                && ((self.store.docs_epoch == pu.prepared_docs_epoch
                    && self.store.collections.rewrite_epoch() == pu.prepared_rewrite_epoch
                    && i < self.store.docs_id.len())
                    || (pu.field == "id"
                        && self
                            .store
                            .docs_id
                            .get(i)
                            .is_some_and(|id| id.as_ref() == pu.key.as_str())))
            {
                i
            } else {
                match self.store.point_index(&pu.collection, &pu.field, &pu.key) {
                    Some(i) => i,
                    None => return Err(Error::runtime("update: no matching row")),
                }
            };
            if self.can_fast_docs_wing_update(pu) {
                let (prev_cell, wing_slot, out_rows) =
                    self.exec_docs_wing_update_core(pu, idx, want_rows, true)?;
                if let Undo::Rows(events) | Undo::Mixed { events } = undo {
                    events.push(RowUndo::DocsWing(idx, wing_slot, prev_cell));
                }
                self.store.r#gen += 1;
                return Ok((
                    Handle {
                        plan: Arc::clone(&prepared.plan),
                        done: Done {
                            n: 1,
                            r#gen: self.store.r#gen,
                        },
                        rows: out_rows,
                        message: None,
                        ms: 0.0,
                    },
                    None,
                    true,
                ));
            }
            if !self.store.deferred_doc_wing_updates.is_empty() {
                self.store.flush_fts_deferred_docs();
            }
            let has_guards = self
                .catalog
                .guards
                .iter()
                .any(|g| g.collection == pu.collection);
            let now = if has_guards || super::eval::record_needs_now(&pu.record) {
                now_ms()
            } else {
                0
            };
            let has_body = pu.record.fields.iter().any(|(k, _)| k == "body");
            let reembed = self.update_needs_reembed(&pu.record.fields);
            let updated = {
                let rows = self.store.collection(&pu.collection);
                let before_ref = &rows[idx];
                if !pu.cas_each {
                    let got = crate::store::row_text(before_ref, "hash").unwrap_or("");
                    let cas = pu.cas.as_deref().unwrap_or("");
                    if got != cas {
                        return Err(Error::runtime(format!(
                            "cas mismatch: expected {cas:?}, found {got:?}"
                        )));
                    }
                }
                if has_guards {
                    self.check_row_update_guards(&pu.collection, before_ref, &pu.record.fields, now)?;
                }
                self.check_update_id(&pu.collection, &pu.record, &[idx], now)?;
                self.patch_updated_row(
                    &pu.collection,
                    before_ref.clone(),
                    &pu.record.fields,
                    now,
                    has_body,
                    reembed,
                )?
            };
            if !self.catalog.fks.is_empty() {
                self.check_row_fks(&pu.collection, &updated)?;
            }
            if !self.store.indexes.is_empty() {
                let Store {
                    collections,
                    indexes,
                    ..
                } = &mut self.store;
                let before_ref = &collections.get(&pu.collection).unwrap()[idx];
                let mut applied = 0usize;
                let mut err_msg: Option<String> = None;
                for live_idx in indexes.values_mut() {
                    if live_idx.def.collection == pu.collection {
                        if let Err(e) = live_idx.update_row(idx, before_ref, &updated) {
                            err_msg = Some(e);
                            break;
                        }
                        applied += 1;
                    }
                }
                if let Some(e) = err_msg {
                    for live_idx in indexes.values_mut() {
                        if applied == 0 {
                            break;
                        }
                        if live_idx.def.collection == pu.collection {
                            let _ = live_idx.update_row(idx, &updated, before_ref);
                            applied -= 1;
                        }
                    }
                    return Err(Error::runtime(e));
                }
            }
            if !self.store.hnsw.is_empty() {
                let Store {
                    collections, hnsw, ..
                } = &mut self.store;
                if let Some(h) = hnsw.get_mut(&pu.collection) {
                    if let Some(Cell::Vec(v)) = updated.get("embedding") {
                        h.insert(idx, Arc::clone(v));
                    } else if collections
                        .get(&pu.collection)
                        .is_some_and(|c| c[idx].contains_key("embedding"))
                    {
                        h.remove(idx);
                    }
                }
            }
            let durable = self.is_durable();
            let ret_rows = if want_rows {
                vec![updated.clone()]
            } else {
                Vec::new()
            };
            let pack = if durable {
                Some(Pack::Update {
                    collection: pu.collection.clone(),
                    rows: vec![updated.clone()],
                })
            } else {
                None
            };
            let (identity_moved, stale_slot) = {
                let before_ref = &self.store.collection(&pu.collection)[idx];
                let new_id = crate::store::row_text(&updated, "id");
                let moved = crate::store::row_text(before_ref, "id") != new_id
                    || crate::store::row_text(before_ref, "uri")
                        != crate::store::row_text(&updated, "uri");
                let stale = !moved
                    && new_id
                        .is_some_and(|key| self.store.row_index(&pu.collection, key) != Some(idx));
                (moved, stale)
            };
            let before = if identity_moved || stale_slot {
                let b =
                    std::mem::replace(&mut self.store.collection_mut(&pu.collection)[idx], updated);
                self.store.rebuild_row_maps_collection(&pu.collection);
                b
            } else {
                let b = std::mem::take(&mut self.store.collection_mut(&pu.collection)[idx]);
                self.store
                    .row_maps_sync_updated(&pu.collection, idx, &b, &updated);
                self.store.collections.get_mut(&pu.collection).unwrap()[idx] = updated;
                b
            };
            if !self.store.fts.is_empty() && self.store.fts.contains_key(&pu.collection) {
                self.store.fts_sync_at(&pu.collection, idx, &before);
            }
            if let Undo::Rows(events) | Undo::Mixed { events } = undo {
                events.push(RowUndo::Update(pu.collection.clone(), idx, before));
            }
            if durable {
                self.store.r#gen = base_gen + 1;
            } else {
                self.store.r#gen += 1;
            }
            return Ok((
                Handle {
                    plan: Arc::clone(&prepared.plan),
                    done: Done {
                        n: 1,
                        r#gen: self.store.r#gen,
                    },
                    rows: ret_rows,
                    message: None,
                    ms: 0.0,
                },
                pack,
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
