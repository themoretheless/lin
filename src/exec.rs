use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap};
use std::fs::File;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use rustc_hash::{FxHashMap, FxHashSet, FxHasher};
use std::hash::{Hash, Hasher};

#[path = "exec/agg.rs"]
mod agg;
#[path = "exec/eval.rs"]
mod eval;
#[path = "exec/gpu_ops.rs"]
mod gpu_ops;

pub(crate) use agg::*;
pub(crate) use eval::*;
use gpu_ops::rows_to_query_batch;
#[cfg(feature = "gpu")]
use gpu_ops::gpu_rrf_keys;

// Precomputed content hash cache: (body_str) -> precomputed_hash
// Uses lock-free RefCell inside thread_local! to reuse Arc<str> for repeated bodies
thread_local! {
    static CONTENT_HASH_CACHE: std::cell::RefCell<FxHashMap<String, Arc<str>>> =
        std::cell::RefCell::new(FxHashMap::default());
}

fn cached_content_hash(body: &str) -> Arc<str> {
    CONTENT_HASH_CACHE.with(|cache| {
        let mut guard = cache.borrow_mut();
        if let Some(cached) = guard.get(body) {
            return Arc::clone(cached);
        }
        let hash = content_hash_arc(body);
        if guard.len() >= 1024 {
            guard.clear();
        }
        guard.insert(body.to_owned(), Arc::clone(&hash));
        hash
    })
}

use crate::ast::*;
use crate::batch::RecordBatch;
use crate::catalog::Catalog;
use crate::check;
use crate::embed::{self, Embedder, HashingEmbedder};
use crate::error::Error;
use crate::explain::{self, ExplainCtx, RunStats};
use crate::graph::GraphFmt;
use crate::parse;
use crate::persist::{Pack, Persist};
use crate::plan::{self, Plan};
use crate::store::{
    Cell, Edge, Row, Store, compact_row, content_hash_arc, now_ms, project_fields, row_text,
};

pub use crate::persist::{OpenMemOpts as OpenOpts, SyncMode};

type StmtOut = (Vec<Row>, Option<String>, Option<Pack>);

#[cfg(test)]
thread_local! {
    static PREENCODE_TEST_MODE: std::cell::Cell<u8> = const { std::cell::Cell::new(0) };
    static PREENCODE_TEST_ATTEMPTS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

struct PackCtx<'a> {
    track_written: bool,
    allow_preencode: bool,
    prepared_encoding: Option<crate::persist::PreparedEncoding>,
    snap: BTreeMap<(String, String), String>,
    written: BTreeSet<(String, String)>,
    undo: &'a mut Undo,
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

#[derive(Debug, Clone, Copy)]
pub struct Done {
    pub r#gen: u64,
    pub n: usize,
}

#[derive(Debug, Clone)]
pub struct Handle {
    pub plan: Arc<Plan>,
    pub rows: Vec<Row>,
    pub done: Done,
    pub message: Option<String>,
    pub ms: f64,
}

impl Drop for Handle {
    fn drop(&mut self) {
        if self.rows.len() >= 1024
            && self.rows.len() <= 2_500_000
            && self
                .rows
                .first()
                .is_some_and(|r| r.len() == 2 && r.contains_key("id") && r.contains_key("title"))
        {
            crate::store::release_id_title_rows(std::mem::take(&mut self.rows));
        } else if self.rows.len() == 1 {
            if let Some(row) = self.rows.pop() {
                crate::store::release_single_row(row);
            }
        }
    }
}

impl Handle {
    #[inline]
    pub fn into_rows(mut self) -> Vec<Row> {
        std::mem::take(&mut self.rows)
    }

    /// First cell of the first row (`ExecuteScalar`). Empty result is an error.
    pub fn scalar(&self) -> Result<crate::store::Cell, Error> {
        let row = self.rows.first().ok_or_else(|| Error::runtime("no rows"))?;
        row.values()
            .next()
            .cloned()
            .ok_or_else(|| Error::runtime("empty row"))
    }

    pub fn scalar_as<T: crate::row::FromCell>(&self) -> Result<T, Error> {
        T::from_cell(&self.scalar()?)
    }

    /// Write verb: affected `n` and `gen` (Dapper `Execute`).
    pub fn execute(&self) -> Done {
        self.done
    }

    pub fn first(&self) -> Result<&Row, Error> {
        self.rows.first().ok_or_else(|| Error::runtime("no rows"))
    }

    pub fn first_or(&self) -> Option<&Row> {
        self.rows.first()
    }

    pub fn first_typed<T: crate::row::FromRow>(&self) -> Result<T, Error> {
        T::from_row(self.first()?)
    }

    pub fn rows_typed<T: crate::row::FromRow>(&self) -> Result<Vec<T>, Error> {
        crate::row::map_rows(&self.rows)
    }

    pub fn compact(&self) -> String {
        let mut out = String::new();
        for row in &self.rows {
            out.push_str(&compact_row(row));
            out.push('\n');
        }
        if let Some(msg) = &self.message {
            out.push_str(msg);
            out.push('\n');
        }
        out.push_str(&format!("gen={} n={}", self.done.r#gen, self.done.n));
        out
    }
}

#[derive(Debug, Clone)]
pub(crate) struct PointGetPlan {
    pub collection: String,
    pub field: String,
    pub key: String,
    pub fields: Option<Vec<String>>,
    pub is_simple_eq: bool,
    pub pred: Option<Pred>,
    pub is_docs_id_title: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct PointDeletePlan {
    pub collection: String,
    pub field: String,
    pub key: String,
    pub cas: Option<String>,
    pub append_only: bool,
    pub prepared_gen: u64,
    pub prepared_docs_epoch: u64,
    pub prepared_idx: Option<usize>,
    pub prepared_kd_kt: Option<(crate::index::IndexKey, crate::index::IndexKey)>,
}

#[derive(Debug, Clone)]
pub(crate) struct PointUpdatePlan {
    pub collection: String,
    pub field: String,
    pub key: String,
    pub cas: Option<String>,
    pub record: Record,
    pub is_docs_wing_only: bool,
}

#[derive(Debug, Clone)]
pub(crate) enum AggKind {
    Count { by: Option<Field> },
    Sum { field: Field, by: Option<Field> },
    Avg { field: Field, by: Option<Field> },
    Min { field: Field, by: Option<Field> },
    Max { field: Field, by: Option<Field> },
}

#[derive(Debug, Clone)]
pub(crate) struct AggPlan {
    pub collection: String,
    pub filter: Option<Pred>,
    pub kind: AggKind,
}

/// Compiled statement: parse + typecheck + plan once, run many times.
#[derive(Debug, Clone)]
pub struct Prepared {
    pub src: String,
    stmts: Vec<Stmt>,
    pub plan: Arc<Plan>,
    /// True if any statement may mutate catalog or store.
    writes: bool,
    /// Append-only writes (insert/append) — cheap rollback without full clone.
    append_only: bool,
    /// Schema (col/rel/index) — clears plan cache after success.
    schema: bool,
    pub(crate) point_get: Option<PointGetPlan>,
    pub(crate) agg_plan: Option<AggPlan>,
    pub(crate) point_delete: Option<PointDeletePlan>,
    pub(crate) point_update: Option<PointUpdatePlan>,
}

impl Prepared {
    pub fn run(&self, db: &mut Db) -> Result<Handle, Error> {
        db.run_prepared(self)
    }

    pub fn run_read(&self, db: &ReadDb) -> Result<Handle, Error> {
        db.run_prepared(self)
    }

    /// Columnar OLAP path when the statement is a supported batch query (FK join+project).
    /// Falls back to row materialization for other statements.
    pub fn run_batch(&self, db: &mut Db) -> Result<RecordBatch, Error> {
        db.run_prepared_batch(self)
    }

    pub fn run_batch_read(&self, db: &ReadDb) -> Result<RecordBatch, Error> {
        db.run_prepared_batch(self)
    }

    pub fn execute(&self, db: &mut Db) -> Result<Done, Error> {
        Ok(self.run(db)?.done)
    }

    pub fn scalar(&self, db: &mut Db) -> Result<crate::store::Cell, Error> {
        self.run(db)?.scalar()
    }

    pub fn scalar_read(&self, db: &ReadDb) -> Result<crate::store::Cell, Error> {
        self.run_read(db)?.scalar()
    }
}

pub struct Db {
    pub catalog: Catalog,
    pub store: Store,
    /// Durable log handle (writer only). Absent on in-memory and read snapshots.
    persist: Option<Persist>,
    /// Shared flock for [`Db::open_read`] (kept alive for lock lifetime).
    reader_lock: Option<File>,
    /// Source → prepared plan (invalidated on schema change).
    plan_cache: FxHashMap<String, Prepared>,
    /// Queryable AST shape → prepared (no typecheck on hit).
    query_cache: FxHashMap<u64, Prepared>,
    /// Hard limits (local prod).
    quotas: Quotas,
    /// Detailed wall-time phases of last open / open_read.
    reopen: ReopenPhases,
    /// Accumulated durable append/insert row counts and wall ms (for rows/s).
    append_rows: u64,
    append_ms: f64,
    /// Named in-memory pins (`pin "x"` / `unpin "x"`; aliases snapshot/restore).
    /// Not a durable checkpoint — see `checkpoint` / `export_backup`.
    pins: BTreeMap<String, crate::store::MemBackup>,
    /// Last `pull idb` payload for in-process `push idb`.
    pulled_wal: Vec<u8>,
    /// Durable hot-standby: [`Self::apply_wal`] OK, user writes rejected.
    follower: bool,
    /// Active embedder (`search vec` / hybrid / auto-embed on insert).
    embedder: Option<Arc<dyn Embedder>>,
    #[cfg(feature = "gpu")]
    gpu: Option<Arc<crate::gpu::GpuCompute>>,
    #[cfg(feature = "gpu")]
    gpu_cache: std::sync::Mutex<crate::gpu::SearchCache>,
}

/// Shared read-only snapshot of a [`Db`] at a fixed `gen`.
///
/// Cheap to `Clone` (`Arc`). Safe to share across threads. Writes are rejected.
#[derive(Clone)]
pub struct ReadDb {
    inner: Arc<Db>,
}

/// Interactive ACID transaction handle.
///
/// Ensures all mutations performed within the transaction are atomic, isolated,
/// and consistent. If dropped without calling [`Self::commit`], all mutations
/// are automatically rolled back.
pub struct Transaction<'a> {
    db: &'a mut Db,
    committed: bool,
    undo: Undo,
    cat_backup: Option<Catalog>,
    packs: Vec<Pack>,
    schema_changed: bool,
    mutation_count: usize,
    journal_bytes: usize,
}

impl<'a> Transaction<'a> {
    /// Execute a source string statement within this transaction.
    pub fn run(&mut self, src: &str) -> Result<Handle, Error> {
        let prepared = self.db.prepare(src)?;
        self.run_prepared(&prepared)
    }

    /// Prepare a query using the database catalog.
    pub fn prepare(&mut self, src: &str) -> Result<Prepared, Error> {
        self.db.prepare(src)
    }

    fn ensure_undo(&mut self, prep: &Prepared) {
        match &mut self.undo {
            Undo::None => {
                if prep.append_only && !prep.schema {
                    self.undo = Undo::Append(self.db.store.append_mark());
                } else if prep.stmts.iter().all(|s| {
                    matches!(
                        s,
                        Stmt::Update { .. }
                            | Stmt::Delete { .. }
                            | Stmt::Query(_)
                            | Stmt::Let { .. }
                    )
                }) {
                    self.undo = Undo::Rows(smallvec::SmallVec::new());
                } else {
                    self.undo = Undo::Full(self.db.store.mem_backup());
                }
            }
            Undo::Append(mark) => {
                if !prep.append_only || prep.schema {
                    if prep.stmts.iter().all(|s| {
                        matches!(
                            s,
                            Stmt::Update { .. }
                                | Stmt::Delete { .. }
                                | Stmt::Query(_)
                                | Stmt::Let { .. }
                        )
                    }) {
                        let m = mark.clone();
                        self.undo = Undo::Mixed {
                            append: m,
                            events: smallvec::SmallVec::new(),
                        };
                    } else {
                        self.undo = Undo::Full(self.db.store.mem_backup());
                    }
                }
            }
            Undo::Rows(events) => {
                if prep.append_only && !prep.schema {
                    let ev = std::mem::take(events);
                    self.undo = Undo::Mixed {
                        append: self.db.store.append_mark(),
                        events: ev,
                    };
                }
            }
            Undo::Mixed { .. } | Undo::Full(_) => {}
        }
    }

    /// Execute a pre-compiled query within this transaction.
    pub fn run_prepared(&mut self, prep: &Prepared) -> Result<Handle, Error> {
        if self.committed {
            return Err(Error::runtime("transaction already committed"));
        }
        if prep.schema {
            if self.cat_backup.is_none() {
                self.cat_backup = Some(self.db.catalog.clone());
            }
            self.schema_changed = true;
        }
        if prep.writes {
            self.ensure_undo(prep);
        }
        let (handle, pack) = self.db.run_prepared_in_tx(prep, &mut self.undo)?;
        if prep.writes {
            self.mutation_count = self.mutation_count.saturating_add(handle.done.n);
            if self.mutation_count > self.db.quotas.max_tx_mutations {
                return Err(Error::runtime(format!(
                    "transaction mutation count {} exceeds max_tx_mutations quota {}",
                    self.mutation_count, self.db.quotas.max_tx_mutations
                )));
            }
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
        Ok(handle)
    }

    /// Run and return execution summary (`Done`).
    pub fn execute(&mut self, src: &str) -> Result<Done, Error> {
        Ok(self.run(src)?.done)
    }

    /// Run and return scalar first cell.
    pub fn scalar(&mut self, src: &str) -> Result<crate::store::Cell, Error> {
        self.run(src)?.scalar()
    }

    /// Commit the transaction, making all mutations permanent and writing to WAL if durable.
    pub fn commit(mut self) -> Result<(), Error> {
        if self.committed {
            return Ok(());
        }
        if let Some(p) = self.db.persist.as_mut() {
            if self.schema_changed {
                p.catalog_hash = crate::store::catalog_hash(&self.db.catalog);
            }
            for pack in self.packs.drain(..) {
                self.db.store.durable_commit(p, pack)?;
            }
            self.db.store.maybe_checkpoint(p)?;
        }
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
        self.db.rollback(undo, cat);
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
            self.db.rollback(undo, cat);
            self.packs.clear();
        }
    }
}

/// Row / edge / log byte / query resource caps. Defaults suit embeddable single-node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Quotas {
    pub max_rows: usize,
    pub max_edges: usize,
    pub max_log_bytes: u64,
    pub max_query_bytes: usize,
    pub max_ast_depth: usize,
    pub max_ast_bytes: usize,
    pub max_output_bytes: usize,
    pub max_execution_work: usize,
    pub max_regex_bytes: usize,
    pub max_graph_expansion: usize,
    pub max_join_expansion: usize,
    pub max_aggregate_bytes: usize,
    pub max_journal_bytes: usize,
    pub max_tx_mutations: usize,
    pub spill_enabled: bool,
}

impl Default for Quotas {
    fn default() -> Self {
        Self {
            max_rows: 10_000_000,
            max_edges: 10_000_000,
            max_log_bytes: 512 * 1024 * 1024,
            max_query_bytes: 16 * 1024 * 1024,
            max_ast_depth: 128,
            max_ast_bytes: 8 * 1024 * 1024,
            max_output_bytes: 256 * 1024 * 1024,
            max_execution_work: 50_000_000,
            max_regex_bytes: 10 * 1024 * 1024,
            max_graph_expansion: 100_000,
            max_join_expansion: 1_000_000,
            max_aggregate_bytes: 64 * 1024 * 1024,
            max_journal_bytes: 128 * 1024 * 1024,
            max_tx_mutations: 100_000,
            spill_enabled: false,
        }
    }
}

/// Lightweight store counters (stable in 0.2+).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ReopenPhases {
    pub total_ms: f64,
    pub setup_ms: f64,
    pub snapshot_ms: f64,
    pub wal_ms: f64,
    pub metadata_ms: f64,
    pub indexes_ms: f64,
    pub row_maps_ms: f64,
    pub fts_ms: f64,
}

impl From<crate::store::StoreOpenPhases> for ReopenPhases {
    fn from(value: crate::store::StoreOpenPhases) -> Self {
        Self {
            setup_ms: value.setup_ms,
            snapshot_ms: value.snapshot_ms,
            wal_ms: value.wal_ms,
            metadata_ms: value.metadata_ms,
            indexes_ms: value.indexes_ms,
            row_maps_ms: value.row_maps_ms,
            fts_ms: value.fts_ms,
            ..Self::default()
        }
    }
}

/// Lightweight store counters (stable in 0.2+).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Stats {
    pub r#gen: u64,
    pub docs: usize,
    pub facts: usize,
    pub edges: usize,
    pub next_id: u64,
    pub log_bytes: u64,
    pub reopen_ms: u64,
    pub reopen: ReopenPhases,
    pub append_rows: u64,
    pub append_ms: f64,
    pub writes_since_snapshot: u32,
    /// `true` when durable and [`SyncMode::Normal`] (crash may lose uncheckpointed commits).
    pub sync_normal: bool,
    /// `true` when durable opened with cold spill enabled.
    pub cold: bool,
}

impl Stats {
    /// Durable append/insert throughput estimate (0 if no samples).
    pub fn append_rows_per_s(&self) -> f64 {
        if self.append_ms <= 0.0 {
            0.0
        } else {
            (self.append_rows as f64) / (self.append_ms / 1000.0)
        }
    }
}

impl Db {
    fn default_embedder(catalog: &Catalog) -> Arc<dyn Embedder> {
        Arc::new(HashingEmbedder::from_embed_id(&catalog.embed_id))
    }

    fn bare(catalog: Catalog, store: Store) -> Self {
        let embedder = Self::default_embedder(&catalog);
        let mut db = Self {
            catalog,
            store,
            persist: None,
            reader_lock: None,
            plan_cache: FxHashMap::default(),
            query_cache: FxHashMap::default(),
            quotas: Quotas::default(),
            reopen: ReopenPhases::default(),
            append_rows: 0,
            append_ms: 0.0,
            pins: BTreeMap::new(),
            pulled_wal: Vec::new(),
            follower: false,
            embedder: Some(embedder),
            #[cfg(feature = "gpu")]
            gpu: None,
            #[cfg(feature = "gpu")]
            gpu_cache: Default::default(),
        };
        db.store.rebuild_fts(&db.catalog);
        db
    }

    pub fn fixture() -> Self {
        let catalog = crate::catalog::fixture();
        let store = Store::fixture(&catalog);
        let mut db = Self::bare(catalog, store);
        let _ = db.reembed_collection("docs");
        db
    }

    pub fn empty() -> Self {
        let catalog = crate::catalog::fixture();
        let store = Store::empty(catalog.embed_id.clone());
        Self::bare(catalog, store)
    }

    pub fn with_quotas(mut self, quotas: Quotas) -> Self {
        self.quotas = quotas;
        self
    }

    /// Enable or disable spill-to-disk for out-of-core aggregations when memory quota is exceeded.
    pub fn with_spill(mut self, enabled: bool) -> Self {
        self.quotas.spill_enabled = enabled;
        self
    }

    /// Enable experimental GPU cosine scoring. Initialization errors are explicit.
    #[cfg(feature = "gpu")]
    pub fn with_gpu(mut self, gpu: Arc<crate::gpu::GpuCompute>) -> Self {
        self.gpu = Some(gpu);
        self.gpu_cache = Default::default();
        self
    }

    /// Attach a hardware compute backend to an existing database.
    #[cfg(feature = "gpu")]
    pub fn enable_gpu(&mut self, gpu: Arc<crate::gpu::GpuCompute>) {
        self.gpu = Some(gpu);
        self.gpu_cache = Default::default();
    }

    /// Release cached GPU data and return search to the CPU backend.
    #[cfg(feature = "gpu")]
    pub fn disable_gpu(&mut self) {
        self.gpu = None;
        self.gpu_cache = Default::default();
    }

    /// The attached backend, for custom compute pipelines and native resources.
    #[cfg(feature = "gpu")]
    pub fn gpu(&self) -> Option<&Arc<crate::gpu::GpuCompute>> {
        self.gpu.as_ref()
    }

    /// Inspect uploads/reuse of the latest vector corpus (experimental GPU API).
    #[cfg(feature = "gpu")]
    pub fn gpu_cache_stats(&self) -> Result<crate::gpu::GpuCacheStats, Error> {
        Ok(self
            .gpu_cache
            .lock()
            .map_err(|_| Error::runtime("GPU cache lock poisoned"))?
            .stats())
    }

    /// Return the active GPU adapter capabilities, memory limits, and UMA status.
    #[cfg(feature = "gpu")]
    pub fn gpu_adapter_report(&self) -> Option<crate::gpu::GpuAdapterReport> {
        self.gpu.as_ref().map(|g| g.adapter_report())
    }

    /// Replace the active embedder. Updates `store.embed_id` / catalog to match.
    pub fn with_embedder(mut self, embedder: Arc<dyn Embedder>) -> Self {
        self.catalog.embed_id = embedder.id().to_string();
        self.store.embed_id = embedder.id().to_string();
        self.embedder = Some(embedder);
        self.plan_cache.clear();
        self.query_cache.clear();
        self
    }

    /// Disable embedding (vec/hybrid degrade: vec empty, hybrid→lex only).
    pub fn without_embedder(mut self) -> Self {
        self.embedder = None;
        #[cfg(feature = "gpu")]
        {
            self.gpu_cache = Default::default();
        }
        self.plan_cache.clear();
        self.query_cache.clear();
        self
    }

    pub fn with_sync_mode(mut self, sync: SyncMode) -> Self {
        if let Some(p) = self.persist.as_mut() {
            p.sync = sync;
        }
        self
    }

    /// Open durable store. Default: [`SyncMode::Full`], `cold: false`.
    ///
    /// Prefer [`Self::open_with`] when setting `SyncMode::Normal` or cold spill —
    /// Normal is crash-unsafe until checkpoint; cold is a **disk** spill (still
    /// full RAM after open), not lazy mmap page-in.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, Error> {
        Self::open_with(path, OpenOpts::default())
    }

    pub fn open_with(path: impl AsRef<Path>, opts: OpenOpts) -> Result<Self, Error> {
        let t0 = Instant::now();
        let catalog = crate::catalog::fixture();
        let (store, persist, store_phases) = Store::open_with_profiled(path, &catalog, opts)?;
        let mut catalog = catalog;
        store.merge_extras_into(&mut catalog);
        let embedder = Self::default_embedder(&catalog);
        let mut db = Self {
            catalog,
            store,
            persist: Some(persist),
            reader_lock: None,
            plan_cache: FxHashMap::default(),
            query_cache: FxHashMap::default(),
            quotas: Quotas::default(),
            reopen: store_phases.into(),
            append_rows: 0,
            append_ms: 0.0,
            pins: BTreeMap::new(),
            pulled_wal: Vec::new(),
            follower: false,
            embedder: Some(embedder),
            #[cfg(feature = "gpu")]
            gpu: None,
            #[cfg(feature = "gpu")]
            gpu_cache: Default::default(),
        };
        db.reopen.total_ms = t0.elapsed().as_secs_f64() * 1000.0;
        Ok(db)
    }

    /// Open a durable **read-only follower** (hot standby).
    ///
    /// Same on-disk layout as [`Self::open`], but:
    /// - user writes (`insert` / `append` / …) are rejected;
    /// - [`Self::apply_wal`] appends shipped frames to the log and applies them;
    /// - exclusive writer lock (one applicator); use [`Self::open_read`] for
    ///   concurrent query processes against the same dir.
    ///
    /// Bootstrap: [`Self::bootstrap_follower`] from a primary backup, then
    /// periodically `primary.export_wal_since(follower.gen())` → `apply_wal`.
    /// Applying WAL alone cannot recreate history already compacted into a
    /// primary snapshot.
    pub fn open_follower(path: impl AsRef<Path>) -> Result<Self, Error> {
        Self::open_follower_with(path, OpenOpts::default())
    }

    pub fn open_follower_with(path: impl AsRef<Path>, opts: OpenOpts) -> Result<Self, Error> {
        let mut db = Self::open_with(path, opts)?;
        db.follower = true;
        Ok(db)
    }

    /// Write a portable backup into `data_dir` and open it as a follower.
    pub fn bootstrap_follower(
        backup: impl AsRef<Path>,
        data_dir: impl AsRef<Path>,
    ) -> Result<Self, Error> {
        let snap = crate::persist::read_backup(backup.as_ref())?;
        let dir = data_dir.as_ref();
        crate::persist::ensure_dir(dir)?;
        crate::persist::write_snapshot(dir, &snap)?;
        crate::persist::write_head(
            dir,
            &crate::persist::Head {
                r#gen: snap.r#gen,
                catalog_hash: snap.catalog_hash.clone(),
                embed_id: snap.embed_id.clone(),
            },
        )?;
        let log_path = dir.join(crate::persist::LOG_NAME);
        std::fs::write(&log_path, []).map_err(|e| Error::runtime(format!("persist: {e}")))?;
        Self::open_follower(dir)
    }

    /// Open a durable data dir as a read-only snapshot (no log truncate / no head write).
    ///
    /// Uses a **shared FENCE** lock: compatible with a live writer. May see a
    /// slightly stale image (commits after open are not visible). Checkpoint waits
    /// for readers. For in-process concurrent reads beside a writer, prefer
    /// [`Self::reader`].
    pub fn open_read(path: impl AsRef<Path>) -> Result<ReadDb, Error> {
        let t0 = Instant::now();
        let catalog = crate::catalog::fixture();
        let (store, lock, store_phases) = Store::open_read_with_profiled(path, &catalog)?;
        let mut catalog = catalog;
        store.merge_extras_into(&mut catalog);
        let embedder = Self::default_embedder(&catalog);
        let mut inner = Self {
            catalog,
            store,
            persist: None,
            reader_lock: Some(lock),
            plan_cache: FxHashMap::default(),
            query_cache: FxHashMap::default(),
            quotas: Quotas::default(),
            reopen: store_phases.into(),
            append_rows: 0,
            append_ms: 0.0,
            pins: BTreeMap::new(),
            pulled_wal: Vec::new(),
            follower: false,
            embedder: Some(embedder),
            #[cfg(feature = "gpu")]
            gpu: None,
            #[cfg(feature = "gpu")]
            gpu_cache: Default::default(),
        };
        inner.reopen.total_ms = t0.elapsed().as_secs_f64() * 1000.0;
        Ok(ReadDb {
            inner: Arc::new(inner),
        })
    }

    pub fn close(&mut self) -> Result<(), Error> {
        if let Some(mut p) = self.persist.take() {
            self.store.checkpoint(&mut p)?;
            // lock released when p drops
        }
        self.reader_lock = None;
        Ok(())
    }

    /// Force a durable checkpoint + log compaction (no-op if in-memory).
    pub fn checkpoint(&mut self) -> Result<(), Error> {
        if let Some(p) = self.persist.as_mut() {
            self.store.checkpoint(p)?;
        }
        Ok(())
    }

    pub fn is_durable(&self) -> bool {
        self.persist.is_some()
    }

    /// True when opened via [`Self::open_follower`] / [`Self::bootstrap_follower`].
    pub fn is_follower(&self) -> bool {
        self.follower
    }

    /// Current commit generation.
    pub fn r#gen(&self) -> u64 {
        self.store.r#gen
    }

    /// Freeze a consistent in-memory snapshot at the current `gen` for concurrent readers.
    pub fn reader(&mut self) -> ReadDb {
        ReadDb {
            inner: Arc::new(Self {
                catalog: self.catalog.clone(),
                store: self.store.clone_mem(),
                persist: None,
                reader_lock: None,
                plan_cache: FxHashMap::default(),
                query_cache: FxHashMap::default(),
                quotas: self.quotas,
                reopen: self.reopen,
                append_rows: 0,
                append_ms: 0.0,
                pins: BTreeMap::new(),
                pulled_wal: Vec::new(),
                follower: false,
                embedder: self.embedder.clone(),
                #[cfg(feature = "gpu")]
                gpu: self.gpu.clone(),
                #[cfg(feature = "gpu")]
                gpu_cache: Default::default(),
            }),
        }
    }

    /// Ship WAL frames with `gen > since` (follower / network).
    pub fn export_wal_since(&self, since: u64) -> Result<Vec<u8>, Error> {
        let Some(p) = self.persist.as_ref() else {
            return Ok(Vec::new());
        };
        crate::persist::export_wal_since(&p.dir, since)
    }

    /// Apply shipped WAL frames.
    ///
    /// - **In-memory** [`Self::empty`]: apply packs only (no log).
    /// - **Follower** ([`Self::open_follower`]): append raw frames to the durable
    ///   log (verbatim), then apply; respects [`SyncMode`].
    /// - **Primary durable**: refused — never rewrite a primary log from ships.
    pub fn apply_wal(&mut self, frames: &[u8]) -> Result<usize, Error> {
        if self.persist.is_some() && !self.follower {
            return Err(Error::runtime(
                "apply_wal: refused on durable primary — use open_follower / bootstrap_follower",
            ));
        }

        let mut to_apply: Vec<(Vec<u8>, crate::persist::LogRecord)> = Vec::new();
        let mut next_needed = self.store.r#gen + 1;
        crate::persist::for_each_wal_frame_raw(frames, |raw, rec| {
            if rec.r#gen < next_needed {
                return Ok(());
            }
            if rec.r#gen != next_needed {
                return Err(Error::runtime(format!(
                    "apply_wal: gap — need gen {}, got {} (follower at {})",
                    next_needed, rec.r#gen, self.store.r#gen
                )));
            }
            next_needed += 1;
            to_apply.push((raw.to_vec(), rec));
            Ok(())
        })?;

        if to_apply.is_empty() {
            return Ok(0);
        }

        if self.persist.is_some() {
            let raw_len: u64 = to_apply.iter().map(|(r, _)| r.len() as u64).sum();
            if let Some(p) = self.persist.as_mut()
                && p.log_bytes + raw_len > self.quotas.max_log_bytes
            {
                self.store.checkpoint(p)?;
            }
            if let Some(p) = self.persist.as_mut() {
                let mut raw_buf = Vec::with_capacity(raw_len as usize);
                for (raw, _) in &to_apply {
                    raw_buf.extend_from_slice(raw);
                }
                crate::persist::append_raw_frames(&mut p.log, &raw_buf, p.sync, &mut p.log_bytes)?;
                p.writes_since_snapshot += to_apply.len() as u32;
            }
        }

        let n = to_apply.len();
        for (_, rec) in &to_apply {
            self.store.apply_pack(&rec.pack);
            self.store.next_id = rec.next_id;
            self.store.r#gen = rec.r#gen;
        }
        self.store.rebuild_indexes();
        self.store.rebuild_row_maps();
        self.store.merge_extras_into(&mut self.catalog);
        self.store.ensure_fts(&self.catalog);
        if let Some(p) = self.persist.as_mut() {
            p.catalog_hash = crate::store::catalog_hash(&self.catalog);
            self.store.maybe_checkpoint(p)?;
        }
        self.plan_cache.clear();
        self.query_cache.clear();
        Ok(n)
    }

    pub fn stats(&self) -> Stats {
        let (log_bytes, writes_since_snapshot, sync_normal, cold) = match self.persist.as_ref() {
            Some(p) => (
                p.log_bytes,
                p.writes_since_snapshot,
                p.sync == SyncMode::Normal,
                p.cold,
            ),
            None => (0, 0, false, false),
        };
        Stats {
            r#gen: self.store.r#gen,
            docs: self.store.collection("docs").len(),
            facts: self.store.facts_len(),
            edges: self.store.edges.len(),
            next_id: self.store.next_id,
            log_bytes,
            reopen_ms: self.reopen.total_ms as u64,
            reopen: self.reopen,
            append_rows: self.append_rows,
            append_ms: self.append_ms,
            writes_since_snapshot,
            sync_normal,
            cold,
        }
    }

    fn total_rows(&self) -> usize {
        self.store.row_count()
    }

    fn check_quotas(&self) -> Result<(), Error> {
        let rows = self.total_rows();
        if rows > self.quotas.max_rows {
            return Err(Error::runtime(format!(
                "quota: rows {rows} > max_rows {}",
                self.quotas.max_rows
            )));
        }
        let edges = self.store.edges.len();
        if edges > self.quotas.max_edges {
            return Err(Error::runtime(format!(
                "quota: edges {edges} > max_edges {}",
                self.quotas.max_edges
            )));
        }
        if let Some(p) = self.persist.as_ref()
            && p.log_bytes > self.quotas.max_log_bytes
        {
            return Err(Error::runtime(format!(
                "quota: log_bytes {} > max_log_bytes {}",
                p.log_bytes, self.quotas.max_log_bytes
            )));
        }
        Ok(())
    }

    /// Write a portable JSON backup of the current memory image.
    /// If this Db is durable, checkpoints the log first.
    pub fn export_backup(&mut self, path: impl AsRef<Path>) -> Result<(), Error> {
        if let Some(p) = self.persist.as_mut() {
            self.store.checkpoint(p)?;
        }
        let hash = crate::store::catalog_hash(&self.catalog);
        let log_offset = self
            .persist
            .as_ref()
            .and_then(|p| p.log.metadata().ok().map(|m| m.len()))
            .unwrap_or(0);
        let snap = self.store.capture_snapshot_at(hash, log_offset);
        crate::persist::write_backup(path.as_ref(), &snap)
    }

    /// Load a backup file into a fresh in-memory Db (call [`Self::open`] + restore
    /// into a data dir separately if durable is needed — use `lin backup import --data`).
    pub fn import_backup(path: impl AsRef<Path>) -> Result<Self, Error> {
        let snap = crate::persist::read_backup(path.as_ref())?;
        let store = Store::from_snapshot(std::path::Path::new("."), snap)?;
        let mut catalog = crate::catalog::fixture();
        store.merge_extras_into(&mut catalog);
        Ok(Self::bare(catalog, store))
    }

    /// Import backup into a durable data directory (replaces store files).
    pub fn import_backup_into(
        backup: impl AsRef<Path>,
        data_dir: impl AsRef<Path>,
    ) -> Result<Self, Error> {
        let snap = crate::persist::read_backup(backup.as_ref())?;
        let dir = data_dir.as_ref();
        crate::persist::ensure_dir(dir)?;
        // Fresh durable dir from snapshot image.
        crate::persist::write_snapshot(dir, &snap)?;
        crate::persist::write_head(
            dir,
            &crate::persist::Head {
                r#gen: snap.r#gen,
                catalog_hash: snap.catalog_hash.clone(),
                embed_id: snap.embed_id.clone(),
            },
        )?;
        // Empty log after snapshot offset semantics: truncate/create empty log.
        let log_path = dir.join(crate::persist::LOG_NAME);
        std::fs::write(&log_path, []).map_err(|e| Error::runtime(format!("persist: {e}")))?;
        Self::open(dir)
    }

    pub fn clear_plan_cache(&mut self) {
        self.plan_cache.clear();
        self.query_cache.clear();
    }

    fn query_cache_key(&self, q: &Query) -> u64 {
        let mut h = FxHasher::default();
        crate::store::catalog_hash(&self.catalog).hash(&mut h);
        q.hash(&mut h);
        h.finish()
    }

    /// Compile a fluent [`Query`] once; cache hit skips typecheck/plan.
    pub fn prepare_query(&mut self, q: &Query) -> Result<Prepared, Error> {
        let key = self.query_cache_key(q);
        if let Some(p) = self.query_cache.get(&key) {
            return Ok(p.clone());
        }
        let prepared = self.prepare_stmt(Stmt::Query(q.clone()))?;
        self.query_cache.insert(key, prepared.clone());
        Ok(prepared)
    }

    /// Parse, typecheck, and plan once. Reuse via [`Prepared::run`] / [`Self::run_prepared`].
    pub fn prepare(&mut self, src: &str) -> Result<Prepared, Error> {
        if let Some(p) = self.plan_cache.get(src) {
            return Ok(p.clone());
        }
        let prepared = self.prepare_uncached(src)?;
        self.plan_cache.insert(src.to_string(), prepared.clone());
        Ok(prepared)
    }

    fn prepare_uncached(&self, src: &str) -> Result<Prepared, Error> {
        if src.len() > self.quotas.max_query_bytes {
            return Err(Error::new(format!(
                "query length {} exceeds max_query_bytes quota {}",
                src.len(),
                self.quotas.max_query_bytes
            )));
        }
        let stmts = parse::parse_program_with_limits(
            src,
            self.quotas.max_query_bytes,
            self.quotas.max_ast_depth,
        )?;
        let mut prepared = self.prepare_stmts(stmts)?;
        prepared.src = src.to_string();
        Ok(prepared)
    }

    /// Typecheck + plan a program already parsed as AST (no string cache).
    pub fn prepare_stmts(&self, stmts: Vec<Stmt>) -> Result<Prepared, Error> {
        let ast_bytes = crate::ast::estimate_ast_bytes(&stmts);
        if ast_bytes > self.quotas.max_ast_bytes {
            return Err(Error::new(format!(
                "prepared AST size {} exceeds max_ast_bytes quota {}",
                ast_bytes, self.quotas.max_ast_bytes
            )));
        }
        let (_, staged) = check::check_program(&stmts, &self.catalog)?;
        let plan_cat = staged.as_ref().unwrap_or(&self.catalog);
        let plan = Arc::new(plan::plan_program(&stmts, plan_cat)?);
        let writes = stmts.iter().any(stmt_writes);
        let append_only = writes && stmts.iter().all(|s| stmt_append_only(s) || !stmt_writes(s));
        let schema = stmts.iter().any(stmt_schema);
        let point_get = if stmts.len() == 1
            && let Stmt::Query(q) = &stmts[0]
            && !writes
        {
            Self::try_extract_point_get(q, plan_cat)
        } else {
            None
        };
        let agg_plan = if stmts.len() == 1
            && let Stmt::Query(q) = &stmts[0]
            && !writes
            && point_get.is_none()
        {
            Self::try_extract_agg_plan(q, plan_cat)
        } else {
            None
        };
        let point_delete = if stmts.len() == 1 {
            Self::try_extract_point_delete(&stmts[0], &self.catalog, &self.store)
        } else {
            None
        };
        let point_update = if stmts.len() == 1 {
            Self::try_extract_point_update(&stmts[0])
        } else {
            None
        };
        Ok(Prepared {
            src: String::new(),
            stmts,
            plan,
            writes,
            append_only,
            schema,
            point_get,
            agg_plan,
            point_delete,
            point_update,
        })
    }

    pub fn prepare_stmt(&self, stmt: Stmt) -> Result<Prepared, Error> {
        self.prepare_stmts(vec![stmt])
    }

    pub fn run_stmt(&mut self, stmt: Stmt) -> Result<Handle, Error> {
        let prepared = self.prepare_stmt(stmt)?;
        self.run_prepared(&prepared)
    }

    pub fn run_stmts(&mut self, stmts: Vec<Stmt>) -> Result<Handle, Error> {
        let prepared = self.prepare_stmts(stmts)?;
        self.run_prepared(&prepared)
    }

    /// Execute independent source snippets atomically as one commit.
    ///
    /// On a durable [`SyncMode::Full`] database this emits one WAL frame and
    /// performs one durability flush for the entire group. Any parse, check, or
    /// runtime error rolls back the whole group.
    pub fn run_group<I, S>(&mut self, sources: I) -> Result<Handle, Error>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut stmts = Vec::new();
        for source in sources {
            stmts.extend(parse::parse_program(source.as_ref())?);
        }
        self.run_stmts(stmts)
    }

    pub fn run_prepared(&mut self, prepared: &Prepared) -> Result<Handle, Error> {
        if self.follower && prepared.writes {
            return Err(Error::runtime(
                "follower: read-only — apply_wal for replication, open primary for writes",
            ));
        }
        if prepared.point_delete.is_none() && !self.store.fts_deferred_doc_swaps.is_empty() {
            self.store.flush_fts_deferred_docs();
        }
        if let Some(pg) = &prepared.point_get {
            let row = if pg.is_docs_id_title {
                self.store.point_get_docs_id_title(&pg.key)
            } else if pg.is_simple_eq {
                self.store
                    .project_by_key(&pg.collection, &pg.field, &pg.key, pg.fields.as_deref())
            } else {
                let full = self
                    .store
                    .project_by_key(&pg.collection, &pg.field, &pg.key, None);
                match full {
                    Some(full) => {
                        let now = now_ms();
                        if let Some(pred) = &pg.pred
                            && !eval_pred(pred, &full, now)
                        {
                            None
                        } else if let Some(fs) = &pg.fields {
                            Some(
                                self.store
                                    .project_by_key(&pg.collection, &pg.field, &pg.key, Some(fs))
                                    .unwrap_or(full),
                            )
                        } else {
                            Some(full)
                        }
                    }
                    None => None,
                }
            };
            let rows = match row {
                Some(r) => vec![r],
                None => Vec::new(),
            };
            let n = rows.len();
            return Ok(Handle {
                plan: prepared.plan.clone(),
                rows,
                done: Done {
                    r#gen: self.store.r#gen,
                    n,
                },
                message: None,
                ms: 0.0,
            });
        }
        if let Some(pd) = &prepared.point_delete {
            if !pd.append_only && pd.cas.is_none() {
                return Err(Error::runtime("delete requires cas"));
            }
            if self.persist.is_none()
                && pd.collection == "docs"
                && self.store.r#gen == pd.prepared_gen
                && self.store.docs_epoch == pd.prepared_docs_epoch
                && let Some(idx) = pd.prepared_idx
            {
                self.store
                    .point_delete_docs_at_in_memory(idx, pd.prepared_kd_kt.as_ref());
                self.store.r#gen += 1;
                return Ok(Handle {
                    plan: Arc::clone(&prepared.plan),
                    rows: Vec::new(),
                    done: Done {
                        r#gen: self.store.r#gen,
                        n: 1,
                    },
                    message: None,
                    ms: 0.0,
                });
            }
            let idx = match self.store.point_index(&pd.collection, &pd.field, &pd.key) {
                Some(i) => i,
                None => return Err(Error::runtime("delete: no matching row")),
            };
            if !pd.append_only {
                let rows = self.store.collection(&pd.collection);
                let got = row_text(&rows[idx], "hash").unwrap_or("");
                let cas = pd.cas.as_deref().unwrap_or("");
                if got != cas {
                    return Err(Error::runtime(format!(
                        "cas mismatch: expected {cas:?}, found {got:?}"
                    )));
                }
            }
            if self.persist.is_none()
                && pd.collection == "docs"
                && self.store.collection("docs").len() >= 4096
            {
                self.store.point_delete_docs_at_in_memory(idx, None);
                self.store.r#gen += 1;
                return Ok(Handle {
                    plan: Arc::clone(&prepared.plan),
                    rows: Vec::new(),
                    done: Done {
                        r#gen: self.store.r#gen,
                        n: 1,
                    },
                    message: None,
                    ms: 0.0,
                });
            }
            let out = self.store.delete_rows_at(&pd.collection, &[idx]);
            if let Some(p) = self.persist.as_mut() {
                let pack = Pack::Delete {
                    collection: pd.collection.clone(),
                    rows: out.clone(),
                };
                if let Err(e) = self.store.durable_commit(p, pack) {
                    return Err(e);
                }
            }
            self.store.r#gen += 1;
            if let Some(p) = self.persist.as_mut() {
                self.store.maybe_checkpoint(p)?;
            }
            return Ok(Handle {
                plan: Arc::clone(&prepared.plan),
                rows: out,
                done: Done {
                    r#gen: self.store.r#gen,
                    n: 1,
                },
                message: None,
                ms: 0.0,
            });
        }
        if let Some(pu) = &prepared.point_update {
            if pu.cas.is_none() {
                return Err(Error::runtime("update requires cas"));
            }
            let idx = match self.store.point_index(&pu.collection, &pu.field, &pu.key) {
                Some(i) => i,
                None => return Err(Error::runtime("update: no matching row")),
            };
            if pu.is_docs_wing_only
                && self.persist.is_none()
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
                let Store {
                    collections,
                    indexes,
                    retired_cells,
                    ..
                } = &mut self.store;
                let docs_col = collections.get_mut(&pu.collection).unwrap();
                let large_table = docs_col.len() >= 4096;
                let row = &mut docs_col[idx];
                let got = row_text(row, "hash").unwrap_or("");
                let cas = pu.cas.as_deref().unwrap_or("");
                if got != cas {
                    return Err(Error::runtime(format!(
                        "cas mismatch: expected {cas:?}, found {got:?}"
                    )));
                }
                let now = now_ms();
                let new_cell = value_cell(&pu.record.fields[0].1, now);
                let wing_arc = new_cell.text_shared().unwrap_or_default();
                let has_complex_idx = indexes.values().any(|live_idx| {
                    live_idx.def.collection == pu.collection
                        && (live_idx.def.pred.is_some()
                            || !(live_idx.def.fields.len() == 1 && live_idx.def.fields[0] == "wing")
                                && !(live_idx.def.fields.len() == 2
                                    && live_idx.def.fields[0] == "wing"
                                    && live_idx.def.fields[1] == "ts")
                                && live_idx.def.fields.iter().any(|f| f == "wing"))
                });
                let mut old_keys: smallvec::SmallVec<[crate::index::IndexKey; 4]> =
                    smallvec::SmallVec::new();
                if has_complex_idx {
                    for live_idx in indexes.values() {
                        if live_idx.def.collection == pu.collection {
                            old_keys.push(live_idx.key_of(row));
                        }
                    }
                }
                let ts_cell = if !has_complex_idx
                    && indexes.values().any(|live_idx| {
                        live_idx.def.collection == pu.collection
                            && live_idx.def.fields.len() == 2
                            && live_idx.def.fields[0] == "wing"
                            && live_idx.def.fields[1] == "ts"
                    }) {
                    row.get("ts").cloned().unwrap_or(Cell::Null)
                } else {
                    Cell::Null
                };
                let prev_cell = if let Some(slot) = row.get_mut("wing") {
                    Some(std::mem::replace(slot, new_cell))
                } else {
                    row.insert("wing".to_string(), new_cell)
                };
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
                } else {
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
                let out_rows = if large_table {
                    if let Some(prev) = prev_cell
                        && retired_cells.len() < retired_cells.inline_size()
                    {
                        retired_cells.push(prev);
                    }
                    Vec::new()
                } else {
                    vec![row.clone()]
                };
                self.store.sync_docs_wing_only(idx, wing_arc);
                self.store.r#gen += 1;
                return Ok(Handle {
                    plan: Arc::clone(&prepared.plan),
                    rows: out_rows,
                    done: Done {
                        r#gen: self.store.r#gen,
                        n: 1,
                    },
                    message: None,
                    ms: 0.0,
                });
            }
            let rows = self.store.collection(&pu.collection);
            let got = row_text(&rows[idx], "hash").unwrap_or("");
            let cas = pu.cas.as_deref().unwrap_or("");
            if got != cas {
                return Err(Error::runtime(format!(
                    "cas mismatch: expected {cas:?}, found {got:?}"
                )));
            }
            let now = now_ms();
            let before_ref = &rows[idx];
            if !self.catalog.guards.is_empty() {
                for (k, _) in &pu.record.fields {
                    for g in &self.catalog.guards {
                        if g.collection == pu.collection
                            && &g.field == k
                            && eval_pred(&g.pred, before_ref, now)
                        {
                            return Err(Error::runtime(format!("immutable field: {}.{k}", pu.collection)));
                        }
                    }
                }
            }
            let before = before_ref.clone();
            let mut updated = before.clone();
            for (k, v) in &pu.record.fields {
                updated.insert(k.clone(), value_cell(v, now));
            }
            let has_body = pu.record.fields.iter().any(|(k, _)| k == "body");
            if has_body
                && let Some(body) = row_text(&updated, "body").map(str::to_string)
            {
                updated.insert("hash".into(), Cell::Text(cached_content_hash(&body)));
            }
            self.check_row_fks(&pu.collection, &updated)?;
            if let Some(rows) = self.store.collections.get_mut(&pu.collection) {
                rows[idx] = updated.clone();
            }
            if let Err(e) = self.store.index_update_row(&pu.collection, idx, &before, &updated) {
                if let Some(rows) = self.store.collections.get_mut(&pu.collection) {
                    rows[idx] = before;
                }
                return Err(e);
            }
            self.store.fts_sync_at(&pu.collection, idx, &before);
            if let Some(hnsw) = self.store.hnsw.get_mut(&pu.collection) {
                if let Some(Cell::Vec(v)) = updated.get("embedding") {
                    hnsw.insert(idx, v.clone());
                } else if before.contains_key("embedding") {
                    hnsw.remove(idx);
                }
            }
            self.store
                .row_maps_sync_updated(&pu.collection, idx, &before, &updated);
            if let Some(p) = self.persist.as_mut() {
                let pack = Pack::Update {
                    collection: pu.collection.clone(),
                    rows: vec![updated.clone()],
                };
                if let Err(e) = self.store.durable_commit(p, pack) {
                    return Err(e);
                }
            }
            self.store.r#gen += 1;
            if let Some(p) = self.persist.as_mut() {
                self.store.maybe_checkpoint(p)?;
            }
            return Ok(Handle {
                plan: Arc::clone(&prepared.plan),
                rows: vec![updated],
                done: Done {
                    r#gen: self.store.r#gen,
                    n: 1,
                },
                message: None,
                ms: 0.0,
            });
        }
        #[cfg(feature = "gpu")]
        let has_gpu_agg = self.gpu.is_some()
            && prepared.stmts.first().map_or(false, |s| match s {
                Stmt::Query(q) => self.uses_gpu_filter(q) || self.uses_gpu_count(q),
                _ => false,
            });
        #[cfg(not(feature = "gpu"))]
        let has_gpu_agg = false;

        if !has_gpu_agg {
            if let Some(agg) = &prepared.agg_plan {
                let t0 = Instant::now();
                let now = now_ms();
                let rows = self.execute_agg_plan(agg, now)?;
                let ms = t0.elapsed().as_secs_f64() * 1000.0;
                let n = rows.len();
                return Ok(Handle {
                    plan: prepared.plan.clone(),
                    rows,
                    done: Done {
                        r#gen: self.store.r#gen,
                        n,
                    },
                    message: None,
                    ms,
                });
            }
        }
        let t0 = Instant::now();
        let mut undo = if !prepared.writes {
            Undo::None
        } else if prepared.append_only {
            Undo::Append(self.store.append_mark())
        } else if prepared.stmts.iter().all(|s| {
            matches!(
                s,
                Stmt::Update { .. } | Stmt::Delete { .. } | Stmt::Query(_) | Stmt::Let { .. }
            )
        }) {
            Undo::Rows(smallvec::SmallVec::new())
        } else {
            Undo::Full(self.store.mem_backup())
        };
        let cat_backup = if matches!(undo, Undo::Full(_)) || prepared.schema {
            Some(self.catalog.clone())
        } else {
            None
        };

        let ((rows, message, pack), prepared_encoding) =
            match self.exec_program(&prepared.stmts, &mut undo, true) {
                Ok(v) => v,
                Err(e) => {
                    self.rollback(undo, cat_backup);
                    return Err(e);
                }
            };
        let ms = t0.elapsed().as_secs_f64() * 1000.0;
        let n = if rows.is_empty() {
            match prepared.stmts.last() {
                Some(Stmt::Insert { records, .. }) => records.len(),
                Some(Stmt::AppendFacts { records }) if pack.is_some() => {
                    pack_affect_n(pack.as_ref())
                        .filter(|&c| c > 0)
                        .unwrap_or(records.len())
                }
                _ => pack_affect_n(pack.as_ref()).unwrap_or(0),
            }
        } else {
            rows.len()
        };
        if let Some(pack) = pack {
            if let Err(e) = self.check_quotas() {
                self.rollback(undo, cat_backup);
                return Err(e);
            }
            // Bound WAL: compact if already over quota before appending.
            if let Some(p) = self.persist.as_mut()
                && p.log_bytes > self.quotas.max_log_bytes
                && let Err(e) = self.store.checkpoint(p)
            {
                self.rollback(undo, cat_backup);
                return Err(e);
            }
            if let Some(p) = self.persist.as_mut() {
                // Schema packs only — avoid hashing the catalog on every append.
                if prepared.schema {
                    p.catalog_hash = crate::store::catalog_hash(&self.catalog);
                }
                if let Err(e) = self
                    .store
                    .durable_commit_prepared(p, pack, prepared_encoding)
                {
                    self.rollback(undo, cat_backup);
                    return Err(e);
                }
            }
            self.store.r#gen += 1;
            if let Some(p) = self.persist.as_mut() {
                self.store.maybe_checkpoint(p)?;
            }
            if prepared.append_only {
                self.append_rows += n as u64;
                self.append_ms += ms;
            }
        }
        if prepared.schema {
            self.plan_cache.clear();
            self.query_cache.clear();
        }
        self.check_output_quota(&rows)?;
        Ok(Handle {
            plan: prepared.plan.clone(),
            rows,
            done: Done {
                r#gen: self.store.r#gen,
                n,
            },
            message,
            ms,
        })
    }

    /// Columnar read path: FK join+project or filter+project → [`RecordBatch`].
    pub fn run_prepared_batch(&mut self, prepared: &Prepared) -> Result<RecordBatch, Error> {
        if prepared.writes {
            return Err(Error::runtime("run_batch is read-only"));
        }
        if prepared.stmts.len() == 1
            && let Stmt::Query(q) = &prepared.stmts[0]
        {
            let q = crate::catalog::with_catalog_filter(q, &self.catalog);
            let now = now_ms();
            let bindings = Default::default();
            if !self.uses_gpu_filter(&q)
                && !self.uses_gpu_join(&q)
                && let Some(batch) = self.try_filter_project_batch(&q, &bindings, now)
            {
                return Ok(batch);
            }
            if !self.uses_gpu_filter(&q)
                && !self.uses_gpu_join(&q)
                && let Some(batch) = self.try_join_project_batch(&q, &bindings, now)
            {
                return Ok(batch);
            }
        }
        let h = self.run_prepared(prepared)?;
        Ok(match prepared.stmts.as_slice() {
            [Stmt::Query(q)] => rows_to_query_batch(&h.rows, q),
            _ => rows_to_rough_batch(&h.rows),
        })
    }

    pub fn run_batch(&mut self, src: &str) -> Result<RecordBatch, Error> {
        let prepared = self.prepare(src)?;
        self.run_prepared_batch(&prepared)
    }

    /// Read-only run (`&self`): rejects mutating statements.
    pub fn run_readonly(&self, src: &str) -> Result<Handle, Error> {
        let prepared = self.prepare_uncached(src)?;
        self.run_prepared_readonly(&prepared)
    }

    pub fn run_prepared_readonly(&self, prepared: &Prepared) -> Result<Handle, Error> {
        if prepared.writes {
            return Err(Error::runtime("read-only snapshot: writes not allowed"));
        }
        if let Some(pg) = &prepared.point_get {
            let row = if pg.is_docs_id_title {
                self.store.point_get_docs_id_title(&pg.key)
            } else if pg.is_simple_eq {
                self.store
                    .project_by_key(&pg.collection, &pg.field, &pg.key, pg.fields.as_deref())
            } else {
                let full = self
                    .store
                    .project_by_key(&pg.collection, &pg.field, &pg.key, None);
                match full {
                    Some(full) => {
                        let now = now_ms();
                        if let Some(pred) = &pg.pred
                            && !eval_pred(pred, &full, now)
                        {
                            None
                        } else if let Some(fs) = &pg.fields {
                            Some(
                                self.store
                                    .project_by_key(&pg.collection, &pg.field, &pg.key, Some(fs))
                                    .unwrap_or(full),
                            )
                        } else {
                            Some(full)
                        }
                    }
                    None => None,
                }
            };
            let rows = match row {
                Some(r) => vec![r],
                None => Vec::new(),
            };
            let n = rows.len();
            return Ok(Handle {
                plan: prepared.plan.clone(),
                rows,
                done: Done {
                    r#gen: self.store.r#gen,
                    n,
                },
                message: None,
                ms: 0.0,
            });
        }
        #[cfg(feature = "gpu")]
        let has_gpu_agg = self.gpu.is_some()
            && prepared.stmts.first().map_or(false, |s| match s {
                Stmt::Query(q) => self.uses_gpu_filter(q) || self.uses_gpu_count(q),
                _ => false,
            });
        #[cfg(not(feature = "gpu"))]
        let has_gpu_agg = false;

        if !has_gpu_agg {
            if let Some(agg) = &prepared.agg_plan {
                let t0 = Instant::now();
                let now = now_ms();
                let rows = self.execute_agg_plan(agg, now)?;
                let ms = t0.elapsed().as_secs_f64() * 1000.0;
                let n = rows.len();
                return Ok(Handle {
                    plan: prepared.plan.clone(),
                    rows,
                    done: Done {
                        r#gen: self.store.r#gen,
                        n,
                    },
                    message: None,
                    ms,
                });
            }
        }
        let t0 = Instant::now();
        let (rows, message, pack) = self.exec_program_readonly(&prepared.stmts)?;
        debug_assert!(pack.is_none());
        self.check_output_quota(&rows)?;
        let ms = t0.elapsed().as_secs_f64() * 1000.0;
        let n = rows.len();
        Ok(Handle {
            plan: prepared.plan.clone(),
            rows,
            done: Done {
                r#gen: self.store.r#gen,
                n,
            },
            message,
            ms,
        })
    }

    #[inline]
    fn check_output_quota(&self, rows: &[Row]) -> Result<(), Error> {
        if rows.is_empty() {
            return Ok(());
        }
        let max_bytes = self.quotas.max_output_bytes;
        if rows.len() > 256 && max_bytes >= 64 * 1024 * 1024 {
            let sample_n = 32usize;
            let sample_bytes: usize = rows[..sample_n]
                .iter()
                .map(crate::store::row_byte_size)
                .sum();
            let est = (sample_bytes / sample_n).saturating_mul(rows.len());
            if est.saturating_mul(4) < max_bytes {
                return Ok(());
            }
        }
        let out_bytes = rows
            .iter()
            .map(crate::store::row_byte_size)
            .sum::<usize>();
        if out_bytes > max_bytes {
            return Err(Error::runtime(format!(
                "query output bytes {} exceeds max_output_bytes quota {}",
                out_bytes, max_bytes
            )));
        }
        Ok(())
    }

    fn exec_program_readonly(&self, stmts: &[Stmt]) -> Result<StmtOut, Error> {
        let mut bindings: BTreeMap<String, Vec<Row>> = BTreeMap::new();
        let mut last_rows = Vec::new();
        let mut last_msg = None;
        for stmt in stmts {
            match stmt {
                Stmt::Let { name, query } => {
                    let rows = self.exec_query(query, &bindings)?;
                    last_rows = rows.clone();
                    last_msg = None;
                    bindings.insert(name.clone(), rows);
                }
                Stmt::Query(q) => {
                    last_rows = self.exec_query(q, &bindings)?;
                    last_msg = None;
                }
                Stmt::IdbSlice { .. }
                | Stmt::IdbPull { .. }
                | Stmt::IdbPush
                | Stmt::Snapshot { .. }
                | Stmt::Restore { .. }
                | Stmt::Pin { .. }
                | Stmt::Unpin { .. } => {
                    last_rows = Vec::new();
                    last_msg = Some("no-op".into());
                }
                _ => {
                    return Err(Error::runtime("read-only snapshot: writes not allowed"));
                }
            }
        }
        Ok((last_rows, last_msg, None))
    }

    fn rollback(&mut self, undo: Undo, cat_backup: Option<Catalog>) {
        match undo {
            Undo::None => {}
            Undo::Append(mark) => self.store.append_rollback(mark),
            Undo::Full(b) => self.store.mem_restore(b),
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
        if let Some(cat) = cat_backup {
            self.catalog = cat;
            self.plan_cache.clear();
            self.query_cache.clear();
        }
    }

    fn restore_docs_wing_cell(&mut self, idx: usize, prev_cell: Option<Cell>) {
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
        let has_complex_idx = indexes.values().any(|live_idx| {
            live_idx.def.collection == "docs"
                && (live_idx.def.pred.is_some()
                    || !(live_idx.def.fields.len() == 1 && live_idx.def.fields[0] == "wing")
                        && !(live_idx.def.fields.len() == 2
                            && live_idx.def.fields[0] == "wing"
                            && live_idx.def.fields[1] == "ts")
                        && live_idx.def.fields.iter().any(|f| f == "wing"))
        });
        let mut old_keys: smallvec::SmallVec<[crate::index::IndexKey; 4]> =
            smallvec::SmallVec::new();
        if has_complex_idx {
            for live_idx in indexes.values() {
                if live_idx.def.collection == "docs" {
                    old_keys.push(live_idx.key_of(row));
                }
            }
        }
        let ts_cell = if !has_complex_idx
            && indexes.values().any(|live_idx| {
                live_idx.def.collection == "docs"
                    && live_idx.def.fields.len() == 2
                    && live_idx.def.fields[0] == "wing"
                    && live_idx.def.fields[1] == "ts"
            }) {
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
        if has_complex_idx {
            let mut key_iter = old_keys.into_iter();
            for live_idx in indexes.values_mut() {
                if live_idx.def.collection == "docs" {
                    let old_k = key_iter.next();
                    let new_k = Some(live_idx.key_of(row));
                    let _ = live_idx.update_keys(idx, old_k, new_k);
                }
            }
        } else {
            for live_idx in indexes.values_mut() {
                if live_idx.def.collection == "docs" {
                    if live_idx.def.fields.len() == 1 && live_idx.def.fields[0] == "wing" {
                        let old_k = Some(crate::index::LiveIndex::single_cell_key(
                            cur_cell.as_ref().unwrap_or(&Cell::Null),
                        ));
                        let new_k = Some(crate::index::LiveIndex::single_cell_key(
                            &Cell::Text(Arc::clone(&restored_wing_arc)),
                        ));
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
        self.store.sync_docs_wing_only(idx, restored_wing_arc);
    }

    pub fn run(&mut self, src: &str) -> Result<Handle, Error> {
        let prepared = self.prepare(src)?;
        self.run_prepared(&prepared)
    }

    /// Write / batch verb: `n` + `gen` without keeping the row buffer.
    pub fn execute(&mut self, src: &str) -> Result<Done, Error> {
        Ok(self.run(src)?.done)
    }

    /// Scalar verb: first cell of the first row (`count`, `take 1` project).
    pub fn scalar(&mut self, src: &str) -> Result<crate::store::Cell, Error> {
        self.run(src)?.scalar()
    }

    /// Build or rebuild an HNSW approximate nearest-neighbor vector index for a collection.
    pub fn build_hnsw(&mut self, collection: &str) -> Result<(), Error> {
        self.store.build_hnsw(collection, 16, 64);
        Ok(())
    }

    /// Build or rebuild a compact SQ8-quantized HNSW vector index for a collection (75% less RAM).
    pub fn build_hnsw_sq8(&mut self, collection: &str) -> Result<(), Error> {
        self.store.build_hnsw_sq8(collection, 16, 64);
        Ok(())
    }

    /// Quantize an existing collection's HNSW vector index to SQ8 in-place.
    pub fn quantize_hnsw(&mut self, collection: &str) -> Result<bool, Error> {
        Ok(self.store.quantize_hnsw(collection))
    }

    /// Resident memory in bytes of the HNSW index for `collection`, if built.
    pub fn hnsw_resident_memory(&self, collection: &str) -> Option<usize> {
        self.store.hnsw_resident_memory(collection)
    }

    /// Begin an interactive ACID transaction.
    ///
    /// All mutations executed through [`Transaction`] remain isolated until [`Transaction::commit`].
    /// If dropped without commit, all changes are automatically rolled back.
    pub fn transaction(&mut self) -> Result<Transaction<'_>, Error> {
        Ok(Transaction {
            db: self,
            committed: false,
            undo: Undo::None,
            cat_backup: None,
            packs: Vec::new(),
            schema_changed: false,
            mutation_count: 0,
            journal_bytes: 0,
        })
    }

    /// Execute a closure inside an interactive transaction, committing on success
    /// and rolling back if an error is returned or a panic occurs.
    pub fn transaction_fn<T, F>(&mut self, f: F) -> Result<T, Error>
    where
        F: FnOnce(&mut Transaction<'_>) -> Result<T, Error>,
    {
        let mut tx = self.transaction()?;
        let res = f(&mut tx)?;
        tx.commit()?;
        Ok(res)
    }

    pub(crate) fn run_prepared_in_tx(
        &mut self,
        prepared: &Prepared,
        undo: &mut Undo,
    ) -> Result<(Handle, Option<Pack>), Error> {
        if !self.store.fts_deferred_doc_swaps.is_empty() {
            self.store.flush_fts_deferred_docs();
        }
        if let Some(pu) = &prepared.point_update
            && pu.is_docs_wing_only
            && self.persist.is_none()
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
            let idx = match self.store.point_index(&pu.collection, &pu.field, &pu.key) {
                Some(i) => i,
                None => return Err(Error::runtime("update: no matching row")),
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
            let now = now_ms();
            let new_cell = value_cell(&pu.record.fields[0].1, now);
            let wing_arc = new_cell.text_shared().unwrap_or_default();
            let has_complex_idx = indexes.values().any(|live_idx| {
                live_idx.def.collection == pu.collection
                    && (live_idx.def.pred.is_some()
                        || !(live_idx.def.fields.len() == 1 && live_idx.def.fields[0] == "wing")
                            && !(live_idx.def.fields.len() == 2
                                && live_idx.def.fields[0] == "wing"
                                && live_idx.def.fields[1] == "ts")
                            && live_idx.def.fields.iter().any(|f| f == "wing"))
            });
            let mut old_keys: smallvec::SmallVec<[crate::index::IndexKey; 4]> =
                smallvec::SmallVec::new();
            if has_complex_idx {
                for live_idx in indexes.values() {
                    if live_idx.def.collection == pu.collection {
                        old_keys.push(live_idx.key_of(row));
                    }
                }
            }
            let ts_cell = if !has_complex_idx
                && indexes.values().any(|live_idx| {
                    live_idx.def.collection == pu.collection
                        && live_idx.def.fields.len() == 2
                        && live_idx.def.fields[0] == "wing"
                        && live_idx.def.fields[1] == "ts"
                }) {
                row.get("ts").cloned().unwrap_or(Cell::Null)
            } else {
                Cell::Null
            };
            let prev_cell = if let Some(slot) = row.get_mut("wing") {
                Some(std::mem::replace(slot, new_cell))
            } else {
                row.insert("wing".to_string(), new_cell)
            };
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
            } else {
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
            self.store.sync_docs_wing_only(idx, wing_arc);
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
            ));
        }
        let t0 = Instant::now();
        let ((rows, message, pack), _) = self.exec_program(&prepared.stmts, undo, false)?;
        let ms = t0.elapsed().as_secs_f64() * 1000.0;
        let n = if rows.is_empty() {
            match prepared.stmts.last() {
                Some(Stmt::Insert { records, .. }) => records.len(),
                Some(Stmt::AppendFacts { records }) if pack.is_some() => {
                    pack_affect_n(pack.as_ref())
                        .filter(|&c| c > 0)
                        .unwrap_or(records.len())
                }
                _ => pack_affect_n(pack.as_ref()).unwrap_or(0),
            }
        } else {
            rows.len()
        };
        if pack.is_some() {
            self.store.r#gen += 1;
        }
        let handle = Handle {
            plan: prepared.plan.clone(),
            done: Done {
                n,
                r#gen: self.store.r#gen,
            },
            rows,
            message,
            ms,
        };
        Ok((handle, pack))
    }

    pub fn explain_as(&mut self, src: &str, graph: Option<GraphFmt>) -> Result<String, Error> {
        let stmts = parse::parse_program(src)?;
        self.explain_stmts(stmts, graph, Some(src))
    }

    /// Explain a program already parsed as AST.
    pub fn explain_stmt(&mut self, stmt: Stmt, graph: Option<GraphFmt>) -> Result<String, Error> {
        self.explain_stmts(vec![stmt], graph, None)
    }

    pub fn explain_stmts(
        &mut self,
        stmts: Vec<Stmt>,
        graph: Option<GraphFmt>,
        run_src: Option<&str>,
    ) -> Result<String, Error> {
        let (_, staged) = check::check_program(&stmts, &self.catalog)?;
        let plan_cat = staged.as_ref().unwrap_or(&self.catalog);
        let mut plan = plan::plan_program(&stmts, plan_cat)?;
        if let Some(fmt) = graph {
            plan.explain = match fmt {
                GraphFmt::Mermaid => crate::ast::ExplainKind::Graph,
                GraphFmt::Dot => crate::ast::ExplainKind::Dot,
            };
        }
        let mut ctx = ExplainCtx {
            r#gen: Some(self.store.r#gen),
            sizes: explain::CollectionSizes::from_store(&self.store, plan_cat),
            ..ExplainCtx::default()
        };
        if plan.explain == crate::ast::ExplainKind::Run {
            let handle = if let Some(src) = run_src {
                self.run(src)?
            } else {
                self.run_stmts(stmts)?
            };
            ctx.stats = Some(RunStats {
                rows: handle.done.n,
                ms: handle.ms,
            });
            ctx.r#gen = Some(handle.done.r#gen);
            return Ok(explain::format_with(&handle.plan, &ctx));
        }
        Ok(explain::format_with(&plan, &ctx))
    }
}

impl Drop for Db {
    fn drop(&mut self) {
        if self.persist.is_some() {
            let _ = self.close();
        }
    }
}

impl ReadDb {
    #[cfg(feature = "gpu")]
    pub fn gpu_cache_stats(&self) -> Result<crate::gpu::GpuCacheStats, Error> {
        self.inner.gpu_cache_stats()
    }

    #[cfg(feature = "gpu")]
    pub fn gpu(&self) -> Option<&Arc<crate::gpu::GpuCompute>> {
        self.inner.gpu()
    }

    #[cfg(feature = "gpu")]
    pub fn gpu_adapter_report(&self) -> Option<crate::gpu::GpuAdapterReport> {
        self.inner.gpu_adapter_report()
    }

    pub fn stats(&self) -> Stats {
        self.inner.stats()
    }

    pub(crate) fn as_db(&self) -> &Db {
        &self.inner
    }

    pub fn r#gen(&self) -> u64 {
        self.inner.store.r#gen
    }

    pub fn prepare(&self, src: &str) -> Result<Prepared, Error> {
        self.inner.prepare_uncached(src)
    }

    pub fn prepare_stmt(&self, stmt: Stmt) -> Result<Prepared, Error> {
        self.inner.prepare_stmt(stmt)
    }

    pub fn prepare_stmts(&self, stmts: Vec<Stmt>) -> Result<Prepared, Error> {
        self.inner.prepare_stmts(stmts)
    }

    pub fn run(&self, src: &str) -> Result<Handle, Error> {
        self.inner.run_readonly(src)
    }

    pub fn scalar(&self, src: &str) -> Result<crate::store::Cell, Error> {
        self.run(src)?.scalar()
    }

    pub fn run_stmt(&self, stmt: Stmt) -> Result<Handle, Error> {
        let prepared = self.prepare_stmt(stmt)?;
        self.run_prepared(&prepared)
    }

    pub fn run_stmts(&self, stmts: Vec<Stmt>) -> Result<Handle, Error> {
        let prepared = self.prepare_stmts(stmts)?;
        self.run_prepared(&prepared)
    }

    pub fn run_prepared(&self, prepared: &Prepared) -> Result<Handle, Error> {
        self.inner.run_prepared_readonly(prepared)
    }

    pub fn run_prepared_batch(&self, prepared: &Prepared) -> Result<RecordBatch, Error> {
        if prepared.writes {
            return Err(Error::runtime("run_batch is read-only"));
        }
        if prepared.stmts.len() == 1
            && let Stmt::Query(q) = &prepared.stmts[0]
        {
            let q = crate::catalog::with_catalog_filter(q, &self.inner.catalog);
            let now = now_ms();
            let bindings = Default::default();
            if !self.inner.uses_gpu_filter(&q)
                && !self.inner.uses_gpu_join(&q)
                && let Some(batch) = self.inner.try_filter_project_batch(&q, &bindings, now)
            {
                return Ok(batch);
            }
            if !self.inner.uses_gpu_filter(&q)
                && !self.inner.uses_gpu_join(&q)
                && let Some(batch) = self.inner.try_join_project_batch(&q, &bindings, now)
            {
                return Ok(batch);
            }
        }
        let h = self.run_prepared(prepared)?;
        Ok(match prepared.stmts.as_slice() {
            [Stmt::Query(q)] => rows_to_query_batch(&h.rows, q),
            _ => rows_to_rough_batch(&h.rows),
        })
    }

    pub fn run_batch(&self, src: &str) -> Result<RecordBatch, Error> {
        let prepared = self.prepare(src)?;
        self.run_prepared_batch(&prepared)
    }

    pub fn explain_as(&self, src: &str, graph: Option<GraphFmt>) -> Result<String, Error> {
        let stmts = parse::parse_program(src)?;
        self.explain_stmts(stmts, graph, Some(src))
    }

    pub fn explain_stmt(&self, stmt: Stmt, graph: Option<GraphFmt>) -> Result<String, Error> {
        self.explain_stmts(vec![stmt], graph, None)
    }

    pub fn explain_stmts(
        &self,
        stmts: Vec<Stmt>,
        graph: Option<GraphFmt>,
        run_src: Option<&str>,
    ) -> Result<String, Error> {
        let (_, staged) = check::check_program(&stmts, &self.inner.catalog)?;
        let plan_cat = staged.as_ref().unwrap_or(&self.inner.catalog);
        let mut plan = plan::plan_program(&stmts, plan_cat)?;
        if let Some(fmt) = graph {
            plan.explain = match fmt {
                GraphFmt::Mermaid => crate::ast::ExplainKind::Graph,
                GraphFmt::Dot => crate::ast::ExplainKind::Dot,
            };
        }
        let mut ctx = ExplainCtx {
            r#gen: Some(self.inner.store.r#gen),
            sizes: explain::CollectionSizes::from_store(&self.inner.store, plan_cat),
            ..ExplainCtx::default()
        };
        if plan.explain == crate::ast::ExplainKind::Run {
            let handle = if let Some(src) = run_src {
                self.run(src)?
            } else {
                self.run_stmts(stmts)?
            };
            ctx.stats = Some(RunStats {
                rows: handle.done.n,
                ms: handle.ms,
            });
            ctx.r#gen = Some(handle.done.r#gen);
            return Ok(explain::format_with(&handle.plan, &ctx));
        }
        Ok(explain::format_with(&plan, &ctx))
    }
}

impl Db {
    fn exec_program(
        &mut self,
        stmts: &[Stmt],
        undo: &mut Undo,
        allow_preencode: bool,
    ) -> Result<(StmtOut, Option<crate::persist::PreparedEncoding>), Error> {
        if !self.store.fts_deferred_doc_swaps.is_empty() {
            self.store.flush_fts_deferred_docs();
        }
        // Materialize only mutated collections before snapshotting CAS hashes.
        for stmt in stmts {
            if let Stmt::Update { collection, .. } | Stmt::Delete { collection, .. } = stmt {
                self.store.collection_mut(collection);
            }
        }
        let mut bindings: BTreeMap<String, Vec<Row>> = BTreeMap::new();
        let need_snap = stmts.iter().any(|s| {
            matches!(
                s,
                Stmt::Update { cas_each: true, .. } | Stmt::Delete { cas_each: true, .. }
            )
        });
        let mut ctx = PackCtx {
            track_written: need_snap,
            allow_preencode: allow_preencode && stmts.len() == 1 && self.is_durable(),
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
        for stmt in stmts {
            if let Stmt::Let { name, query } = stmt {
                let rows = self.exec_query(query, &bindings)?;
                last_rows = rows.clone();
                last_msg = None;
                bindings.insert(name.clone(), rows);
                continue;
            }
            let (rows, message, pack) = self.exec_stmt(stmt, &bindings, &mut ctx)?;
            last_rows = rows;
            last_msg = message;
            if let Some(p) = pack {
                packs.push(p);
            }
        }
        Ok((
            (last_rows, last_msg, fold_packs(packs)),
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
    ) -> Result<StmtOut, Error> {
        match stmt {
            Stmt::Query(q) => Ok((self.exec_query(q, bindings)?, None, None)),
            Stmt::AppendFacts { records } => {
                let now = now_ms();
                let n = records.len();
                let mut bulk_s = Vec::with_capacity(n);
                let mut bulk_p = Vec::with_capacity(n);
                let mut bulk_o = Vec::with_capacity(n);
                let is_standard_spo = records.first().map_or(false, |r| {
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
                            if last_p_val == Some(pv) && let Some(pa) = &last_p_arc {
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
                let want_rows = n <= 128;
                let (rows, changed_n) = self
                    .store
                    .append_facts_spo_bulk(&bulk_s, &bulk_p, &bulk_o, want_rows);
                let msg = if changed_n == 0 {
                    Some("idempotent".into())
                } else {
                    None
                };
                let pack = if changed_n == 0 {
                    None
                } else if self.is_durable() {
                    Some(Pack::AppendFactsBulk {
                        s: bulk_s,
                        p: bulk_p,
                        o: bulk_o,
                    })
                } else {
                    Some(Pack::Batch { packs: Vec::new() })
                };
                Ok((rows, msg, pack))
            }
            Stmt::AppendEdges { edges } => {
                let n = edges.len();
                self.store.edges.reserve(n);
                self.store.edge_keys.reserve(n);
                let mut rows = Vec::with_capacity(n);
                let mut changed_n = 0usize;
                let durable = self.is_durable();
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
                    let (row, changed) = self.append_edge(&e.rel, &e.from, &e.to)?;
                    if changed {
                        changed_n += 1;
                        if durable {
                            bulk_rel.push(e.rel.clone());
                            bulk_from.push(row_text(&row, "from").unwrap_or("").to_string());
                            bulk_to.push(row_text(&row, "to").unwrap_or("").to_string());
                        }
                    }
                    rows.push(row);
                }
                let msg = if changed_n == 0 {
                    Some("idempotent".into())
                } else {
                    None
                };
                let pack = if changed_n == 0 {
                    None
                } else if durable {
                    Some(Pack::AppendEdgesBulk {
                        rel: bulk_rel,
                        from: bulk_from,
                        to: bulk_to,
                    })
                } else {
                    Some(Pack::Batch { packs: Vec::new() })
                };
                Ok((rows, msg, pack))
            }
            Stmt::Insert {
                collection,
                records,
                edges,
            } => {
                let start = self.store.collection(collection).len();
                let (rows, new_edges, staged) =
                    self.insert_bulk(collection, records, edges.as_slice(), ctx.allow_preencode)?;
                let inserted = &self.store.collection(collection)[start..];
                mark_written(ctx, collection, inserted);
                let pack = if self.is_durable() {
                    let pack = if let Some(staged) = staged {
                        ctx.prepared_encoding = Some(staged.encoding);
                        staged.pack.unwrap_or_else(|| {
                            crate::persist::rows_to_insert_cols(
                                collection.clone(),
                                inserted,
                                new_edges,
                            )
                        })
                    } else if ctx.allow_preencode
                        && edges.is_empty()
                        && new_edges.is_empty()
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
                        crate::persist::rows_to_insert_cols(collection.clone(), inserted, new_edges)
                    };
                    Some(pack)
                } else {
                    Some(Pack::Batch { packs: Vec::new() })
                };
                Ok((rows, None, pack))
            }
            Stmt::Update {
                collection,
                pred,
                cas,
                cas_each,
                record,
            } => {
                let rows = self.update_cas(
                    collection,
                    pred.as_ref(),
                    cas.as_deref(),
                    *cas_each,
                    record,
                    ctx,
                )?;
                let pack = if self.is_durable() {
                    Some(Pack::Update {
                        collection: collection.clone(),
                        rows: rows.clone(),
                    })
                } else {
                    Some(Pack::Batch { packs: Vec::new() })
                };
                Ok((rows, None, pack))
            }
            Stmt::Delete {
                collection,
                pred,
                cas,
                cas_each,
            } => {
                let rows = self.delete_rows(collection, pred, cas.as_deref(), *cas_each, ctx)?;
                let pack = if self.is_durable() {
                    Some(Pack::Delete {
                        collection: collection.clone(),
                        rows: rows.clone(),
                    })
                } else {
                    Some(Pack::Batch { packs: Vec::new() })
                };
                Ok((rows, None, pack))
            }
            Stmt::DeleteEdge { rel, from, to } => {
                let row = self.delete_edge(rel, from, to)?;
                Ok((
                    vec![row],
                    None,
                    Some(Pack::DeleteEdge {
                        rel: rel.clone(),
                        from: value_text(from, now_ms()),
                        to: value_text(to, now_ms()),
                    }),
                ))
            }
            Stmt::Let { name, query } => {
                let rows = self.exec_query(query, bindings)?;
                Ok((rows, Some(format!("let {name}")), None))
            }
            Stmt::Reembed { collection, .. } => {
                let n = self.reembed_collection(collection)?;
                let id = self
                    .embedder
                    .as_ref()
                    .map(|e| e.id().to_string())
                    .unwrap_or_else(|| self.store.embed_id.clone());
                Ok((
                    Vec::new(),
                    Some(format!("reembed {collection}: {n} rows; embed_id={id}")),
                    Some(Pack::Reembed),
                ))
            }
            Stmt::Decl(d) => self.exec_decl(d),
            Stmt::IdbSlice { query } => Ok((
                self.exec_query(query, bindings)?,
                Some("idb slice: native".into()),
                None,
            )),
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
                    row.insert("gen".into(), Cell::Int(rec.r#gen as i64));
                    row.insert("next_id".into(), Cell::Int(rec.next_id as i64));
                    row.insert(
                        "bytes".into(),
                        Cell::Int(
                            // approximate: filled after; use pack dbg size
                            0,
                        ),
                    );
                    rows.push(row);
                    n += 1;
                    Ok(())
                })?;
                // One summary row with total frame bytes for shipping.
                if rows.is_empty() {
                    let mut row = Row::new();
                    row.insert("gen".into(), Cell::Int(since as i64));
                    row.insert("frames".into(), Cell::Int(0));
                    row.insert("bytes".into(), Cell::Int(0));
                    rows.push(row);
                } else {
                    for r in &mut rows {
                        r.insert("bytes".into(), Cell::Int(frames.len() as i64));
                        r.insert("frames".into(), Cell::Int(n));
                    }
                }
                Ok((
                    rows,
                    Some(format!(
                        "pull idb since {since}: {} bytes (push idb applies)",
                        frames.len()
                    )),
                    None,
                ))
            }
            Stmt::IdbPush => {
                let frames = std::mem::take(&mut self.pulled_wal);
                if frames.is_empty() {
                    return Ok((Vec::new(), Some("push idb: nothing pulled".into()), None));
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
                ))
            }
            Stmt::Snapshot { name } | Stmt::Pin { name } => {
                self.pins.insert(name.clone(), self.store.mem_backup());
                Ok((
                    Vec::new(),
                    Some(format!(
                        "memory pin {name:?} at gen={} (not durable; use checkpoint/backup for disk)",
                        self.store.r#gen
                    )),
                    None,
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
                ))
            }
        }
    }

    pub(crate) fn exec_query(
        &self,
        q: &Query,
        bindings: &BTreeMap<String, Vec<Row>>,
    ) -> Result<Vec<Row>, Error> {
        self.exec_query_lim(q, bindings, true)
    }

    fn exec_query_lim(
        &self,
        q: &Query,
        bindings: &BTreeMap<String, Vec<Row>>,
        allow_implicit_take: bool,
    ) -> Result<Vec<Row>, Error> {
        let mut q = crate::catalog::with_catalog_filter(q, &self.catalog);
        q.to_mut().simplify_predicates();
        let q = &*q;
        let now = now_ms();

        let gpu_filter = self.uses_gpu_filter(q)
            || self.uses_gpu_count(q)
            || self.uses_gpu_join(q)
            || self.uses_gpu_sort(q)
            || self.uses_gpu_graph(q);
        if !gpu_filter {
            // Point Get short path: map lookup + optional project — no Scan/Filter pipeline.
            if let Some(rows) = self.try_point_get(q) {
                return Ok(rows);
            }

            // Filter → count: index-only or scan-without-clone (fair vs SQL COUNT(*)).
            if let Some(rows) = self.try_filter_count(q, bindings, now) {
                return Ok(rows);
            }

            // Zero-copy aggregates: sum/avg/min/max/count directly over columnar or borrowed rows.
            if let Some(rows) = self.try_agg(q, bindings, now)? {
                return Ok(rows);
            }

            // Filter → project → take: materialize projected rows without full BTreeMap clones.
            if let Some(rows) = self.try_filter_project(q, bindings, now) {
                return Ok(rows);
            }

            // collection | search lex|hybrid | … — FTS postings, no full scan.
            if let Some(rows) = self.try_fts_search(q, bindings)? {
                return Ok(rows);
            }

            // [filter?] | join | project | take — FK point-get, no full right scan / merge.
            if let Some(rows) = self.try_join_project(q, bindings, now) {
                return Ok(rows);
            }
        }

        let mut primary = check::collection_of(&q.source).to_string();
        let mut implicit_take = true;
        let mut saw_agg = false;
        // Only a leading filter can run against source rows. A filter after
        // count/join/skip/project refers to that stage's output, not the source.
        let first_filter = match q.steps.first() {
            Some(Step::Filter(p)) if !gpu_filter => Some(p),
            _ => None,
        };
        let simple_scan = q.steps.iter().all(|s| {
            matches!(
                s,
                Step::Filter(_) | Step::Project(_) | Step::Skip { .. } | Step::Take { .. }
            )
        });
        let project_fields_step = {
            let can_project_source = (first_filter.is_some()
                && matches!(q.steps.get(1), Some(Step::Project(_)))
                && q.steps
                    .iter()
                    .skip(2)
                    .all(|step| matches!(step, Step::Skip { .. } | Step::Take { .. })))
                || (matches!(q.steps.first(), Some(Step::Project(_)))
                    && q.steps
                        .iter()
                        .skip(1)
                        .all(|step| matches!(step, Step::Skip { .. } | Step::Take { .. })));
            if simple_scan && can_project_source {
                q.steps.iter().find_map(|s| match s {
                    Step::Project(f) => Some(f.as_slice()),
                    _ => None,
                })
            } else {
                None
            }
        };

        let scan_limit: Option<usize> = if !gpu_filter && simple_scan && first_filter.is_none() {
            let mut skip_n = 0usize;
            let mut take_n: Option<usize> = None;
            let mut explicit_take_all = false;
            for s in &q.steps {
                match s {
                    Step::Skip { n } => skip_n = skip_n.saturating_add((*n).max(0) as usize),
                    Step::Take { n: None } => explicit_take_all = true,
                    Step::Take { n: Some(n) } => {
                        take_n = Some((*n).max(0) as usize);
                        break;
                    }
                    _ => {}
                }
            }
            if let Some(t) = take_n {
                Some(skip_n.saturating_add(t))
            } else if allow_implicit_take && !explicit_take_all {
                Some(skip_n.saturating_add(50))
            } else {
                None
            }
        } else {
            None
        };

        let mut rows = match &q.source {
            Source::Page(uri) => self
                .store
                .project_by_key("docs", "uri", uri, None)
                .into_iter()
                .collect(),
            Source::Catalog => self.scan_catalog(),
            Source::Collection(name) => {
                if let Some(bound) = bindings.get(name) {
                    bound.clone()
                } else if let Some(pred) = first_filter
                    && let Some((uses, covered)) = crate::index::pick_index_with_coverage(&self.catalog, name, pred)
                    && let Some(idxs) = self.store.index_seek(name, &uses, now)
                {
                    self.fetch_idxs_filtered(name, &idxs, pred, now, project_fields_step, covered)
                } else if let Some(pred) = first_filter {
                    let col_len = self.store.collection(name).len();
                    if col_len > self.quotas.max_execution_work {
                        return Err(Error::runtime(format!(
                            "query execution work {} exceeds max_execution_work quota {}",
                            col_len, self.quotas.max_execution_work
                        )));
                    }
                    self.filter_scan(name, pred, now, project_fields_step)
                } else if let Some(fields) = project_fields_step {
                    self.scan_projected(name, fields, scan_limit)
                } else {
                    self.scan(name, scan_limit)
                }
            }
        };

        // If IndexSeek / filter_scan / scan_projected already applied the first filter (+ optional project),
        // skip only those steps — later filters/projects still run.
        let mut skip_first_filter = first_filter.is_some()
            && matches!(&q.source, Source::Collection(n) if bindings.get(n).is_none());
        let mut skip_first_project = project_fields_step.is_some()
            && matches!(&q.source, Source::Collection(n) if bindings.get(n).is_none());

        let mut work = rows.len();
        if work > self.quotas.max_execution_work {
            return Err(Error::runtime(format!(
                "query execution work {} exceeds max_execution_work quota {}",
                work, self.quotas.max_execution_work
            )));
        }

        for (step_idx, step) in q.steps.iter().enumerate() {
            work = work.saturating_add(rows.len());
            if work > self.quotas.max_execution_work {
                return Err(Error::runtime(format!(
                    "query execution work {} exceeds max_execution_work quota {}",
                    work, self.quotas.max_execution_work
                )));
            }
            match step {
                Step::Filter(pred) => {
                    if skip_first_filter {
                        skip_first_filter = false;
                        continue;
                    }
                    #[cfg(feature = "gpu")]
                    if let Some(mask) = self.gpu_filter_mask(pred, &rows, now)? {
                        let mut mask = mask.into_iter();
                        rows.retain(|_| mask.next().expect("one GPU mask per row"));
                        continue;
                    }
                    rows.retain(|r| eval_pred(pred, r, now));
                }
                Step::Project(fields) => {
                    if skip_first_project {
                        skip_first_project = false;
                        continue;
                    }
                    rows = project(&rows, fields);
                }
                Step::Join {
                    left,
                    collection,
                    on,
                } => {
                    #[cfg(feature = "gpu")]
                    if self.gpu.is_some() {
                        rows = self.gpu_join_rows(&primary, rows, collection, on, *left)?;
                        continue;
                    }
                    rows = self.join_rows(&primary, rows, collection, on, *left)?;
                }
                Step::Hop { rel, depth } => {
                    rows = self.hop(&primary, &rows, rel, depth.unwrap_or(1))?;
                }
                Step::Graph { rel, depth } => {
                    rows = self.graph_edges(&rows, rel, depth.unwrap_or(1))?;
                    primary = "edges".into();
                }
                Step::Match { start, hops } => {
                    rows = self.match_path(&primary, rows, start.as_deref(), hops)?;
                }
                Step::Search { mode, query } => {
                    rows = self.search_rows(&rows, *mode, query)?;
                }
                Step::Count { by } => {
                    implicit_take = false;
                    saw_agg = true;
                    #[cfg(feature = "gpu")]
                    if let Some(result) = self.gpu_count_rows(&rows, by.as_ref())? {
                        rows = result;
                        primary = String::new();
                        continue;
                    }
                    rows = match by {
                        Some(by) => agg_count(&rows, by, self.quotas.max_aggregate_bytes, self.quotas.spill_enabled)?,
                        None => vec![hits_row(rows.len() as i64)],
                    };
                    primary = String::new();
                }
                Step::Sum { field, by } => {
                    implicit_take = false;
                    saw_agg = true;
                    #[cfg(feature = "gpu")]
                    if let Some(result) = self.gpu_sum_rows(&rows, field, by.as_ref())? {
                        rows = result;
                        primary = String::new();
                        continue;
                    }
                    rows = agg_sum(&rows, field, by.as_ref(), self.quotas.max_aggregate_bytes, self.quotas.spill_enabled)?;
                    primary = String::new();
                }
                Step::Min { field, by } => {
                    implicit_take = false;
                    saw_agg = true;
                    rows = agg_min(&rows, field, by.as_ref(), self.quotas.max_aggregate_bytes, self.quotas.spill_enabled)?;
                    primary = String::new();
                }
                Step::Max { field, by } => {
                    implicit_take = false;
                    saw_agg = true;
                    rows = agg_max(&rows, field, by.as_ref(), self.quotas.max_aggregate_bytes, self.quotas.spill_enabled)?;
                    primary = String::new();
                }
                Step::Avg { field, by } => {
                    implicit_take = false;
                    saw_agg = true;
                    rows = agg_avg(&rows, field, by.as_ref(), self.quotas.max_aggregate_bytes, self.quotas.spill_enabled)?;
                    primary = String::new();
                }
                Step::Sort { field, desc } => {
                    #[cfg(feature = "gpu")]
                    if let Some(gpu) = &self.gpu {
                        let key = field.as_str();
                        let cells: Vec<&Cell> = rows
                            .iter()
                            .map(|row| row.get(&key).unwrap_or(&Cell::Null))
                            .collect();
                        let indices = gpu.sort_indices(&cells, *desc)?;
                        let mut source: Vec<Option<Row>> = rows.into_iter().map(Some).collect();
                        rows = indices
                            .into_iter()
                            .map(|i| source[i].take().expect("GPU sort permutation"))
                            .collect();
                        continue;
                    }
                    let mut look_skip = 0usize;
                    let mut look_take: Option<usize> = None;
                    let mut j = step_idx + 1;
                    while j < q.steps.len() {
                        match &q.steps[j] {
                            Step::Skip { n } => {
                                look_skip = look_skip.saturating_add((*n).max(0) as usize);
                                j += 1;
                            }
                            Step::Take { n: Some(n) } => {
                                look_take = Some((*n).max(0) as usize);
                                break;
                            }
                            _ => break,
                        }
                    }
                    let top_k = look_take.map(|t| look_skip.saturating_add(t));
                    sort_rows_top_k(&mut rows, field, *desc, top_k);
                }
                Step::Window {
                    func,
                    field,
                    by,
                    order,
                    frame,
                    as_field,
                } => {
                    rows = eval_window(
                        rows,
                        func,
                        field.as_ref(),
                        by,
                        order,
                        frame.as_ref(),
                        as_field,
                    );
                }
                Step::Skip { n } => {
                    let n = (*n).max(0) as usize;
                    if n >= rows.len() {
                        rows.clear();
                    } else if n > 0 {
                        rows.drain(0..n);
                    }
                }
                Step::Take { n } => {
                    implicit_take = false;
                    if let Some(n) = n {
                        let n = (*n).max(0) as usize;
                        if rows.len() > n {
                            rows.truncate(n);
                        }
                    }
                }
                Step::Union(rhs) => {
                    let mut right = self.exec_query_lim(rhs, bindings, false)?;
                    rows.append(&mut right);
                }
            }
        }

        if allow_implicit_take && implicit_take && !saw_agg && rows.len() > 50 {
            rows.truncate(50);
        }
        Ok(rows)
    }

    pub(crate) fn try_extract_point_get(
        q: &Query,
        cat: &crate::catalog::Catalog,
    ) -> Option<PointGetPlan> {
        let mut q = crate::catalog::with_catalog_filter(q, cat);
        q.to_mut().simplify_predicates();
        let q = &*q;
        let name = match &q.source {
            Source::Collection(n) => n.clone(),
            _ => return None,
        };
        if !matches!(q.steps.first(), Some(Step::Filter(_))) {
            return None;
        }
        let mut filter: Option<&Pred> = None;
        let mut fields: Option<Vec<String>> = None;
        for step in &q.steps {
            match step {
                Step::Filter(p) => {
                    if filter.is_some() {
                        return None;
                    }
                    filter = Some(p);
                }
                Step::Project(f) => {
                    if fields.is_some() {
                        return None;
                    }
                    fields = Some(field_names(f));
                }
                Step::Take { n: Some(n) } if *n <= 0 => return None,
                Step::Skip { n } if *n > 0 => return None,
                Step::Take { .. } | Step::Skip { .. } => {}
                _ => return None,
            }
        }
        let pred = filter?;
        let (field, key) = point_key(pred)?;
        if field == "uri" && name != "docs" {
            return None;
        }
        let simple_eq = matches!(
            pred,
            Pred::Cmp {
                op: CmpOp::Eq,
                value: Value::String(_),
                ..
            }
        );
        let is_docs_id_title = simple_eq
            && name == "docs"
            && field == "id"
            && fields
                .as_deref()
                .map(|fs| fs.len() == 2 && fs[0] == "id" && fs[1] == "title")
                .unwrap_or(false);
        Some(PointGetPlan {
            collection: name,
            field: field.to_string(),
            key: key.to_string(),
            fields,
            is_simple_eq: simple_eq,
            pred: if simple_eq { None } else { Some(pred.clone()) },
            is_docs_id_title,
        })
    }

    pub(crate) fn try_extract_point_delete(
        stmt: &Stmt,
        catalog: &Catalog,
        store: &Store,
    ) -> Option<PointDeletePlan> {
        let Stmt::Delete {
            collection,
            pred,
            cas,
            cas_each: false,
        } = stmt
        else {
            return None;
        };
        let (field, key) = point_key(pred)?;
        if field != "id" && (field != "uri" || collection != "docs") {
            return None;
        }
        if !matches!(pred, Pred::Cmp { .. }) {
            return None;
        }
        let append_only = catalog
            .collection(collection)
            .is_some_and(|c| c.append_only);
        let mut prepared_idx = None;
        let mut prepared_kd_kt = None;
        if collection == "docs" {
            let docs = store.collection("docs");
            if docs.len() >= 4096
                && let Some(idx) = store.point_index("docs", field, key)
                && idx < docs.len()
            {
                let cas_ok = if append_only {
                    true
                } else if let Some(c) = cas {
                    row_text(&docs[idx], "hash") == Some(c.as_str())
                } else {
                    false
                };
                if cas_ok {
                    prepared_idx = Some(idx);
                    let last = docs.len() - 1;
                    if idx != last && store.indexes.len() == 1 {
                        if let Some(live_idx) = store.indexes.values().next()
                            && live_idx.def.collection == "docs"
                            && live_idx.def.pred.is_none()
                        {
                            let kd = live_idx.key_of(&docs[idx]);
                            let kt = live_idx.key_of(&docs[last]);
                            prepared_kd_kt = Some((kd, kt));
                        }
                    }
                }
            }
        }
        Some(PointDeletePlan {
            collection: collection.clone(),
            field: field.to_string(),
            key: key.to_string(),
            cas: cas.clone(),
            append_only,
            prepared_gen: store.r#gen,
            prepared_docs_epoch: store.docs_epoch,
            prepared_idx,
            prepared_kd_kt,
        })
    }

    pub(crate) fn try_extract_point_update(stmt: &Stmt) -> Option<PointUpdatePlan> {
        let Stmt::Update {
            collection,
            pred: Some(pred),
            cas,
            cas_each: false,
            record,
        } = stmt else {
            return None;
        };
        let (field, key) = point_key(pred)?;
        if field != "id" && (field != "uri" || collection != "docs") {
            return None;
        }
        if !matches!(pred, Pred::Cmp { .. }) {
            return None;
        }
        let is_docs_wing_only = collection == "docs"
            && record.fields.len() == 1
            && record.fields[0].0 == "wing";
        Some(PointUpdatePlan {
            collection: collection.clone(),
            field: field.to_string(),
            key: key.to_string(),
            cas: cas.clone(),
            record: record.clone(),
            is_docs_wing_only,
        })
    }

    /// `docs | id == "…" | { … }` — direct map hit.
    fn try_point_get(&self, q: &Query) -> Option<Vec<Row>> {
        let name = match &q.source {
            Source::Collection(n) => n.as_str(),
            Source::Page(uri) => {
                if q.steps.iter().any(|step| {
                    !matches!(
                        step,
                        Step::Filter(_) | Step::Project(_) | Step::Skip { .. } | Step::Take { .. }
                    )
                }) {
                    return None;
                }
                let mut rows: Vec<Row> = self
                    .store
                    .project_by_key("docs", "uri", uri, None)
                    .into_iter()
                    .collect();
                let now = now_ms();
                for step in &q.steps {
                    match step {
                        Step::Filter(pred) => rows.retain(|row| eval_pred(pred, row, now)),
                        Step::Project(fields) => rows = project(&rows, fields),
                        Step::Skip { n } if *n > 0 => rows.clear(),
                        Step::Take { n: Some(n) } if *n <= 0 => rows.clear(),
                        _ => {}
                    }
                }
                return Some(rows);
            }
            _ => return None,
        };
        if !matches!(q.steps.first(), Some(Step::Filter(_))) {
            return None;
        }
        let mut filter: Option<&Pred> = None;
        let mut fields: Option<Vec<String>> = None;
        for step in &q.steps {
            match step {
                Step::Filter(p) => {
                    if filter.is_some() {
                        return None;
                    }
                    filter = Some(p);
                }
                Step::Project(f) => {
                    if fields.is_some() {
                        return None;
                    }
                    fields = Some(field_names(f));
                }
                Step::Take { .. } => {}
                Step::Skip { .. } => {}
                _ => return None,
            }
        }
        let pred = filter?;
        let (field, key) = point_key(pred)?;
        if q.steps.iter().any(|step| {
            matches!(step, Step::Skip { n } if *n > 0)
                || matches!(step, Step::Take { n: Some(n) } if *n <= 0)
        }) {
            return Some(Vec::new());
        }

        let simple_eq = matches!(
            pred,
            Pred::Cmp {
                op: CmpOp::Eq,
                value: Value::String(_),
                ..
            }
        );
        if simple_eq {
            let row = self
                .store
                .project_by_key(name, field, key, fields.as_deref())?;
            return Some(vec![row]);
        }
        let now = now_ms();
        let full = self.store.project_by_key(name, field, key, None)?;
        if !eval_pred(pred, &full, now) {
            return Some(Vec::new());
        }
        Some(vec![
            self.store
                .project_by_key(name, field, key, fields.as_deref())
                .unwrap_or(full),
        ])
    }

    pub(crate) fn try_extract_agg_plan(
        q: &Query,
        cat: &crate::catalog::Catalog,
    ) -> Option<AggPlan> {
        let mut q = crate::catalog::with_catalog_filter(q, cat);
        q.to_mut().simplify_predicates();
        let q = &*q;
        let name = match &q.source {
            Source::Collection(n) => n.clone(),
            _ => return None,
        };
        let mut filter: Option<Pred> = None;
        let mut kind: Option<AggKind> = None;
        for step in &q.steps {
            match step {
                Step::Filter(p) => {
                    if filter.is_some() || kind.is_some() {
                        return None;
                    }
                    filter = Some(p.clone());
                }
                Step::Count { by } => {
                    if kind.is_some() {
                        return None;
                    }
                    kind = Some(AggKind::Count { by: by.clone() });
                }
                Step::Sum { field, by } => {
                    if kind.is_some() {
                        return None;
                    }
                    kind = Some(AggKind::Sum {
                        field: field.clone(),
                        by: by.clone(),
                    });
                }
                Step::Avg { field, by } => {
                    if kind.is_some() {
                        return None;
                    }
                    kind = Some(AggKind::Avg {
                        field: field.clone(),
                        by: by.clone(),
                    });
                }
                Step::Min { field, by } => {
                    if kind.is_some() {
                        return None;
                    }
                    kind = Some(AggKind::Min {
                        field: field.clone(),
                        by: by.clone(),
                    });
                }
                Step::Max { field, by } => {
                    if kind.is_some() {
                        return None;
                    }
                    kind = Some(AggKind::Max {
                        field: field.clone(),
                        by: by.clone(),
                    });
                }
                _ => return None,
            }
        }
        let kind = kind?;
        Some(AggPlan {
            collection: name,
            filter,
            kind,
        })
    }

    fn count_arc_slice_low_cardinality<'a>(
        slice: &'a [Arc<str>],
    ) -> (Option<(Arc<str>, i64)>, Option<(Arc<str>, i64)>, Option<FxHashMap<Arc<str>, i64>>) {
        let mut slot0_ptr: *const u8 = std::ptr::null();
        let mut slot0_arc: Option<&'a Arc<str>> = None;
        let mut slot0_cnt: i64 = 0;

        let mut slot1_ptr: *const u8 = std::ptr::null();
        let mut slot1_arc: Option<&'a Arc<str>> = None;
        let mut slot1_cnt: i64 = 0;

        let mut overflow: Option<FxHashMap<Arc<str>, i64>> = None;

        for w_arc in slice {
            let w_ptr = Arc::as_ptr(w_arc) as *const u8;
            if w_ptr == slot0_ptr {
                slot0_cnt += 1;
            } else if w_ptr == slot1_ptr {
                slot1_cnt += 1;
            } else if slot0_ptr.is_null() {
                slot0_ptr = w_ptr;
                slot0_arc = Some(w_arc);
                slot0_cnt = 1;
            } else if slot1_ptr.is_null() {
                slot1_ptr = w_ptr;
                slot1_arc = Some(w_arc);
                slot1_cnt = 1;
            } else {
                let w = w_arc.as_ref();
                if slot0_arc.map_or(false, |a| a.as_ref() == w) {
                    slot0_cnt += 1;
                } else if slot1_arc.map_or(false, |a| a.as_ref() == w) {
                    slot1_cnt += 1;
                } else {
                    let map = overflow.get_or_insert_with(FxHashMap::default);
                    *map.entry(Arc::clone(w_arc)).or_insert(0) += 1;
                }
            }
        }

        let s0 = slot0_arc.map(|arc| (Arc::clone(arc), slot0_cnt));
        let s1 = slot1_arc.map(|arc| (Arc::clone(arc), slot1_cnt));
        (s0, s1, overflow)
    }

    pub(crate) fn execute_agg_plan(&self, plan: &AggPlan, now: i64) -> Result<Vec<Row>, Error> {
        // Prepared COUNT must share index/columnar paths with ordinary execution.
        if let AggKind::Count { by: None } = &plan.kind
            && let Some(pred) = &plan.filter
            && let Some(rows) = self.execute_filter_count(&plan.collection, pred, None, now)
        {
            return Ok(rows);
        }
        // Grouped prepared COUNT can also be index-only when equality fixes
        // the sole text group. Other grouped forms retain the quota-bounded
        // generic path below rather than using an unbounded grouping helper.
        if let AggKind::Count { by: Some(by) } = &plan.kind
            && let Some(pred) = &plan.filter
            && let Some(value) = eq_value_for_field(pred, &by.as_str())
            && let Some((uses, covered)) = crate::index::pick_index_with_coverage(&self.catalog, &plan.collection, pred)
            && covered
            && let Some(n) = self.store.index_seek_count(&plan.collection, &uses, now)
        {
            if n == 0 {
                return Ok(Vec::new());
            }
            // Match the generic Cell::Text key accounting, including its
            // per-group overhead, before constructing the nonempty result.
            let state_bytes = value.len().saturating_add(32).saturating_add(64);
            if state_bytes > self.quotas.max_aggregate_bytes {
                return Err(Error::runtime(format!(
                    "aggregate state bytes {state_bytes} exceeds max_aggregate_bytes quota {}",
                    self.quotas.max_aggregate_bytes
                )));
            }
            return Ok(vec![count_row(&by.as_str(), value, n as i64)]);
        }
        // 1. SoA paths for orders
        if plan.collection == "orders" && self.store.orders_soa_ready() {
            match &plan.kind {
                AggKind::Sum { field, by } if field.leaf() == Some("total") => {
                    if by.is_none() && plan.filter.is_none() {
                        let total: f64 = self.store.orders_total().iter().sum();
                        let mut row = crate::store::acquire_row();
                        row.insert("total".into(), Cell::Float(total));
                        return Ok(vec![row]);
                    }
                    if let Some(by_field) = by
                        && by_field.leaf() == Some("user_id")
                    {
                        if plan.filter.is_none() {
                            let uids = self.store.orders_user_id();
                            let totals = self.store.orders_total();
                            let total_len = totals.len();
                            let mut map: FxHashMap<Arc<str>, f64> = FxHashMap::default();
                            map.reserve((total_len / 8).clamp(64, 4096));
                            for (uid, &tot) in uids.iter().zip(totals) {
                                if let Some(acc) = map.get_mut(uid.as_ref()) {
                                    *acc += tot;
                                } else {
                                    map.insert(Arc::clone(uid), tot);
                                }
                            }
                            let mut entries: Vec<(Arc<str>, f64)> = map.into_iter().collect();
                            entries.sort_unstable_by(|(a, _), (b, _)| a.cmp(b));
                            return Ok(entries
                                .into_iter()
                                .map(|(uid, tot)| {
                                    let mut row = crate::store::acquire_row();
                                    row.insert("user_id".into(), Cell::Text(uid));
                                    row.insert("total".into(), Cell::Float(tot));
                                    row
                                })
                                .collect());
                        } else if let Some(Pred::Cmp {
                            field: filt_field,
                            op: CmpOp::Gt,
                            value,
                        }) = &plan.filter
                            && filt_field.leaf() == Some("total")
                        {
                            let thresh = match value {
                                Value::Float(f) => Some(*f),
                                Value::Int(n) => Some(*n as f64),
                                _ => None,
                            };
                            if let Some(threshold) = thresh {
                                let uids = self.store.orders_user_id();
                                let totals = self.store.orders_total();
                                let mut map: FxHashMap<Arc<str>, f64> = FxHashMap::default();
                                map.reserve((totals.len() / 8).clamp(64, 4096));
                                for (uid, &tot) in uids.iter().zip(totals) {
                                    if tot > threshold {
                                        if let Some(acc) = map.get_mut(uid.as_ref()) {
                                            *acc += tot;
                                        } else {
                                            map.insert(Arc::clone(uid), tot);
                                        }
                                    }
                                }
                                let mut entries: Vec<(Arc<str>, f64)> = map.into_iter().collect();
                                entries.sort_unstable_by(|(a, _), (b, _)| a.cmp(b));
                                return Ok(entries
                                    .into_iter()
                                    .map(|(uid, tot)| {
                                        let mut row = crate::store::acquire_row();
                                        row.insert("user_id".into(), Cell::Text(uid));
                                        row.insert("total".into(), Cell::Float(tot));
                                        row
                                    })
                                    .collect());
                            }
                        }
                    }
                }
                AggKind::Avg { field, by }
                    if field.leaf() == Some("total") && by.is_none() && plan.filter.is_none() =>
                {
                    let totals = self.store.orders_total();
                    let avg = if totals.is_empty() {
                        0.0
                    } else {
                        totals.iter().sum::<f64>() / totals.len() as f64
                    };
                    let mut row = crate::store::acquire_row();
                    row.insert("total".into(), Cell::Float(avg));
                    return Ok(vec![row]);
                }
                _ => {}
            }
        }

        // 2. SoA paths for docs
        if plan.collection == "docs"
            && self.store.docs_wing().len() == self.store.collection("docs").len()
        {
            if let AggKind::Count { by: Some(by_field) } = &plan.kind
                && by_field.leaf() == Some("wing")
                && plan.filter.is_none()
            {
                let wings = self.store.docs_wing();
                if let Some((indices, dict)) = wings.as_indices() {
                    let mut counts0 = [0i64; 256];
                    let mut counts1 = [0i64; 256];
                    let mut counts2 = [0i64; 256];
                    let mut counts3 = [0i64; 256];
                    let chunks = indices.chunks_exact(4);
                    let rem = chunks.remainder();
                    for c in chunks {
                        counts0[c[0] as usize] += 1;
                        counts1[c[1] as usize] += 1;
                        counts2[c[2] as usize] += 1;
                        counts3[c[3] as usize] += 1;
                    }
                    for &b in rem {
                        counts0[b as usize] += 1;
                    }
                    let mut entries: Vec<(Arc<str>, i64)> = Vec::with_capacity(dict.len());
                    for (idx, arc) in dict.iter().enumerate() {
                        let c = counts0[idx] + counts1[idx] + counts2[idx] + counts3[idx];
                        if c > 0 {
                            entries.push((Arc::clone(arc), c));
                        }
                    }
                    entries.sort_unstable_by(|(a, _), (b, _)| a.cmp(b));
                    return Ok(entries
                        .into_iter()
                        .map(|(w, count)| {
                            let mut row = crate::store::acquire_row();
                            row.insert("wing".into(), Cell::Text(w));
                            row.insert("hits".into(), Cell::Int(count));
                            row
                        })
                        .collect());
                } else {
                    let wings_vec = wings.to_vec();
                    let (s0, s1, overflow) = Self::count_arc_slice_low_cardinality(&wings_vec);
                    let mut combined: FxHashMap<Arc<str>, i64> = overflow.unwrap_or_default();
                    if let Some((k, v)) = s0 {
                        *combined.entry(k).or_insert(0) += v;
                    }
                    if let Some((k, v)) = s1 {
                        *combined.entry(k).or_insert(0) += v;
                    }
                    let mut entries: Vec<(Arc<str>, i64)> = combined.into_iter().collect();
                    entries.sort_unstable_by(|(a, _), (b, _)| a.cmp(b));
                    return Ok(entries
                        .into_iter()
                        .map(|(w, count)| {
                            let mut row = crate::store::acquire_row();
                            row.insert("wing".into(), Cell::Text(w));
                            row.insert("hits".into(), Cell::Int(count));
                            row
                        })
                        .collect());
                }
            }
        }

        // 3. Generic fallback: iterate `&[Row]` without cloning!
        let col = self.store.collection(&plan.collection);
        let filter = plan.filter.as_ref();
        let max_bytes = self.quotas.max_aggregate_bytes;
        let spill = self.quotas.spill_enabled;
        match &plan.kind {
            AggKind::Count { by } => {
                let Some(by) = by else {
                    let mut n = 0i64;
                    for r in col {
                        if filter.is_none_or(|p| eval_pred(p, r, now)) {
                            n += 1;
                        }
                    }
                    return Ok(vec![hits_row(n)]);
                };
                agg_count_filtered(col, filter, now, by, max_bytes, spill)
            }
            AggKind::Sum { field, by } => {
                agg_sum_filtered(col, filter, now, field, by.as_ref(), max_bytes, spill)
            }
            AggKind::Avg { field, by } => {
                agg_avg_filtered(col, filter, now, field, by.as_ref(), max_bytes, spill)
            }
            AggKind::Min { field, by } => {
                agg_extremum_filtered(col, filter, now, field, by.as_ref(), max_bytes, spill, false)
            }
            AggKind::Max { field, by } => {
                agg_extremum_filtered(col, filter, now, field, by.as_ref(), max_bytes, spill, true)
            }
        }
    }

    fn try_agg(
        &self,
        q: &Query,
        bindings: &BTreeMap<String, Vec<Row>>,
        now: i64,
    ) -> Result<Option<Vec<Row>>, Error> {
        if let Source::Collection(name) = &q.source
            && bindings.get(name).is_none()
            && let Some(plan) = Self::try_extract_agg_plan(q, &self.catalog)
        {
            Ok(Some(self.execute_agg_plan(&plan, now)?))
        } else {
            Ok(None)
        }
    }

    /// `col | search lex|hybrid …` — FTS postings → residual score (no full scan).
    fn try_fts_search(
        &self,
        q: &Query,
        bindings: &BTreeMap<String, Vec<Row>>,
    ) -> Result<Option<Vec<Row>>, Error> {
        let Source::Collection(name) = &q.source else {
            return Ok(None);
        };
        if bindings.get(name).is_some() {
            return Ok(None);
        }

        let mut mode: Option<SearchMode> = None;
        let mut query: Option<&str> = None;
        let mut skip_n: usize = 0;
        let mut take_n: Option<Option<i64>> = None;
        let mut proj: Option<&[Field]> = None;
        let mut saw_take = false;

        for step in &q.steps {
            match step {
                Step::Search { mode: m, query: qq } => {
                    if mode.is_some() || proj.is_some() || skip_n > 0 || saw_take {
                        return Ok(None);
                    }
                    mode = Some(*m);
                    query = Some(qq.as_str());
                }
                Step::Skip { n } => {
                    if mode.is_none() || saw_take {
                        return Ok(None);
                    }
                    skip_n = skip_n.saturating_add((*n).max(0) as usize);
                }
                Step::Take { n } => {
                    if saw_take {
                        return Ok(None);
                    }
                    saw_take = true;
                    take_n = Some(*n);
                }
                Step::Project(f) => {
                    if proj.is_some() {
                        return Ok(None);
                    }
                    proj = Some(f.as_slice());
                }
                _ => return Ok(None),
            }
        }

        let (Some(mode), Some(query)) = (mode, query) else {
            return Ok(None);
        };

        if mode != SearchMode::Vec && !self.store.fts.contains_key(name) {
            return Ok(None);
        }

        let limit = match take_n {
            Some(Some(n)) => Some(n.max(0) as usize),
            Some(None) => None,
            None => Some(50),
        };
        let rank_limit = limit.map(|n| n.saturating_add(skip_n));
        let mut rows = self.search_fts(name, mode, query, rank_limit)?;
        if skip_n > 0 {
            if skip_n >= rows.len() {
                rows.clear();
            } else {
                rows.drain(0..skip_n);
            }
        }
        if let Some(n) = limit
            && rows.len() > n
        {
            rows.truncate(n);
        }
        if let Some(fields) = proj {
            rows = project(&rows, fields);
        }
        Ok(Some(rows))
    }

    pub(crate) fn search_fts(
        &self,
        collection: &str,
        mode: SearchMode,
        query: &str,
        rank_limit: Option<usize>,
    ) -> Result<Vec<Row>, Error> {
        match mode {
            SearchMode::Lex => self.search_fts_lex(collection, query, rank_limit),
            SearchMode::Hybrid => {
                const RRF_K: f64 = 60.0;
                let all = self.store.collection(collection);
                let lex = self.search_fts_lex_indices(collection, query, None)?;
                let vec = self.vector_search_indices(all, query, None)?;
                #[cfg(feature = "gpu")]
                if let Some(gpu) = &self.gpu {
                    let lex_keys: Vec<usize> = lex.iter().map(|&(_, i)| i).collect();
                    let vec_keys: Vec<usize> = vec.iter().map(|&(_, i)| i).collect();
                    let ranked = gpu_rrf_keys(gpu, &lex_keys, &vec_keys)?;
                    return Ok(ranked
                        .into_iter()
                        .filter_map(|i| all.get(i).cloned())
                        .collect());
                }
                let mut scores: FxHashMap<usize, f64> = FxHashMap::default();
                scores.reserve(lex.len() + vec.len());
                for (rank, (_, i)) in lex.iter().enumerate() {
                    *scores.entry(*i).or_default() += 1.0 / (RRF_K + rank as f64 + 1.0);
                }
                for (rank, (_, i)) in vec.iter().enumerate() {
                    *scores.entry(*i).or_default() += 1.0 / (RRF_K + rank as f64 + 1.0);
                }
                let mut ranked: Vec<(f64, usize)> =
                    scores.into_iter().map(|(i, score)| (score, i)).collect();
                if let Some(limit) = rank_limit {
                    if limit == 0 {
                        return Ok(Vec::new());
                    }
                    if ranked.len() > limit {
                        ranked.select_nth_unstable_by(limit - 1, |a, b| {
                            b.0.partial_cmp(&a.0)
                                .unwrap_or(std::cmp::Ordering::Equal)
                                .then_with(|| a.1.cmp(&b.1))
                        });
                        ranked.truncate(limit);
                    }
                }
                ranked.sort_by(|a, b| {
                    b.0.partial_cmp(&a.0)
                        .unwrap_or(std::cmp::Ordering::Equal)
                        .then_with(|| a.1.cmp(&b.1))
                });
                Ok(ranked
                    .into_iter()
                    .filter_map(|(_, i)| all.get(i).cloned())
                    .collect())
            }
            SearchMode::Vec => {
                let all = self.store.collection(collection);
                let ranked = self.vector_search_indices(all, query, rank_limit)?;
                Ok(ranked
                    .into_iter()
                    .filter_map(|(_, i)| all.get(i).cloned())
                    .collect())
            }
        }
    }

    fn search_fts_lex(
        &self,
        collection: &str,
        query: &str,
        rank_limit: Option<usize>,
    ) -> Result<Vec<Row>, Error> {
        let col = self.store.collection(collection);
        Ok(self
            .search_fts_lex_indices(collection, query, rank_limit)?
            .into_iter()
            .filter_map(|(_, i)| col.get(i).cloned())
            .collect())
    }

    fn search_fts_lex_indices(
        &self,
        collection: &str,
        query: &str,
        rank_limit: Option<usize>,
    ) -> Result<Vec<(i64, usize)>, Error> {
        if rank_limit == Some(0) {
            return Ok(Vec::new());
        }
        let fts = self
            .store
            .fts
            .get(collection)
            .expect("checked FTS collection");
        let mut idxs = fts.candidate_idxs(query);
        let col = self.store.collection(collection);
        if collection == "docs" && !self.store.fts_deferred_doc_swaps.is_empty() {
            for &(p, _, _) in &self.store.fts_deferred_doc_swaps {
                if p < col.len() && !idxs.contains(&p) {
                    idxs.push(p);
                }
            }
        }
        let query = LexQuery::new(query);
        idxs.retain(|&i| col.get(i).is_some());
        let gpu_scores = self.gpu_lex_scores(idxs.iter().map(|&i| &col[i]), &query)?;
        #[cfg(feature = "gpu")]
        if let Some(scores) = &gpu_scores {
            let positions = self.gpu.as_ref().expect("GPU scores").rank_i64(scores)?;
            return Ok(positions
                .into_iter()
                .filter(|&position| scores[position] > 0)
                .take(rank_limit.unwrap_or(usize::MAX))
                .map(|position| (scores[position], idxs[position]))
                .collect());
        }

        let mut scored = match rank_limit {
            Some(k) => {
                // The max-heap root is the worst retained result:
                // lower score first, then larger row index. This preserves the
                // old stable ordering (score desc, source row index asc).
                let mut heap: BinaryHeap<(Reverse<i64>, usize)> =
                    BinaryHeap::with_capacity(k.saturating_add(1));
                for (position, i) in idxs.into_iter().enumerate() {
                    let Some(row) = col.get(i) else { continue };
                    let score = gpu_scores.as_ref().map_or_else(
                        || lex_score_prepared(row, &query),
                        |scores| scores[position],
                    );
                    if score == 0 {
                        continue;
                    }
                    heap.push((Reverse(score), i));
                    if heap.len() > k {
                        heap.pop();
                    }
                }
                heap.into_iter()
                    .map(|(Reverse(score), i)| (score, i))
                    .collect::<Vec<_>>()
            }
            None => idxs
                .into_iter()
                .enumerate()
                .filter_map(|(position, i)| {
                    let row = col.get(i)?;
                    let score = gpu_scores.as_ref().map_or_else(
                        || lex_score_prepared(row, &query),
                        |scores| scores[position],
                    );
                    (score > 0).then_some((score, i))
                })
                .collect(),
        };
        scored.sort_unstable_by(|(sa, ia), (sb, ib)| sb.cmp(sa).then_with(|| ia.cmp(ib)));
        Ok(scored)
    }

    pub(crate) fn vector_search_indices(
        &self,
        rows: &[Row],
        query: &str,
        rank_limit: Option<usize>,
    ) -> Result<Vec<(f64, usize)>, Error> {
        let Some(emb) = self.embedder.as_ref() else {
            return Ok(Vec::new());
        };
        let qv = emb.embed(query);

        // Fast-path: query maintained HNSW index if rows match collection
        for (col, hnsw) in &self.store.hnsw {
            if !hnsw.is_empty() {
                let col_rows = self.store.collection(col);
                if std::ptr::eq(rows.as_ptr(), col_rows.as_ptr())
                    && rows.len() == col_rows.len()
                    && !rows.is_empty()
                {
                    let k = rank_limit.unwrap_or(20).max(1);
                    let ef = (k * 2).max(64);
                    let mut results = hnsw.search(&qv, k, ef);
                    results.retain(|(s, _)| *s > 0.01);
                    return Ok(results);
                }
            }
        }

        let vectors: Vec<(usize, &[f32])> = rows
            .iter()
            .enumerate()
            .filter_map(|(i, r)| Some((i, r.get("embedding")?.as_vec()?)))
            .collect();
        #[cfg(feature = "gpu")]
        let gpu_scores = self
            .gpu
            .as_ref()
            .map(|gpu| {
                let shared: Vec<Arc<[f32]>> = rows
                    .iter()
                    .filter_map(|r| match r.get("embedding") {
                        Some(Cell::Vec(v)) => Some(v.clone()),
                        _ => None,
                    })
                    .collect();
                self.gpu_cache
                    .lock()
                    .map_err(|_| Error::runtime("GPU cache lock poisoned"))?
                    .score(gpu, &qv, &shared)
            })
            .transpose()?;
        let mut scored = {
            #[cfg(feature = "gpu")]
            if let Some(gpu_scores) = gpu_scores {
                vectors
                    .iter()
                    .enumerate()
                    .filter_map(|(slot, (i, _))| {
                        let score = gpu_scores[slot] as f64;
                        (score > 0.01).then_some((score, *i))
                    })
                    .collect::<Vec<_>>()
            } else {
                Self::score_vectors_cpu(&qv, &vectors)
            }
            #[cfg(not(feature = "gpu"))]
            Self::score_vectors_cpu(&qv, &vectors)
        };
        #[cfg(feature = "gpu")]
        if let Some(gpu) = &self.gpu {
            let keys: Vec<Cell> = scored
                .iter()
                .map(|(score, _)| Cell::Float(*score))
                .collect();
            let references: Vec<&Cell> = keys.iter().collect();
            let mut res: Vec<(f64, usize)> = gpu
                .sort_indices(&references, true)?
                .into_iter()
                .map(|index| scored[index])
                .collect();
            if let Some(limit) = rank_limit {
                res.truncate(limit);
            }
            return Ok(res);
        }
        if let Some(limit) = rank_limit {
            if limit == 0 {
                return Ok(Vec::new());
            }
            if scored.len() > limit {
                scored.select_nth_unstable_by(limit - 1, |a, b| {
                    b.0.partial_cmp(&a.0)
                        .unwrap_or(std::cmp::Ordering::Equal)
                        .then_with(|| a.1.cmp(&b.1))
                });
                scored.truncate(limit);
            }
        }
        scored.sort_by(|a, b| {
            b.0.partial_cmp(&a.0)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.1.cmp(&b.1))
        });
        Ok(scored)
    }

    fn score_vectors_cpu(qv: &[f32], vectors: &[(usize, &[f32])]) -> Vec<(f64, usize)> {
        let q_norm = qv.iter().map(|&x| (x * x) as f64).sum::<f64>().sqrt();
        if q_norm <= 1e-18 {
            return Vec::new();
        }
        let n = vectors.len();
        if n >= 2048 {
            let threads = std::thread::available_parallelism()
                .map(|p| p.get())
                .unwrap_or(1)
                .min(8);
            let chunk_size = (n + threads - 1) / threads;
            std::thread::scope(|scope| {
                let mut handles = Vec::with_capacity(threads);
                for chunk in vectors.chunks(chunk_size) {
                    handles.push(scope.spawn(move || {
                        let mut local = Vec::with_capacity(chunk.len());
                        for &(i, v) in chunk {
                            let score = crate::embed::cosine_with_q_norm(qv, q_norm, v);
                            if score > 0.01 {
                                local.push((score, i));
                            }
                        }
                        local
                    }));
                }
                let mut all = Vec::with_capacity(n);
                for handle in handles {
                    if let Ok(part) = handle.join() {
                        all.extend(part);
                    }
                }
                all
            })
        } else {
            let mut all = Vec::with_capacity(n);
            for &(i, v) in vectors {
                let score = crate::embed::cosine_with_q_norm(qv, q_norm, v);
                if score > 0.01 {
                    all.push((score, i));
                }
            }
            all
        }
    }

    /// `col | pred | count` / `count by f` — no row materialization when possible.
    fn try_filter_count(
        &self,
        q: &Query,
        bindings: &BTreeMap<String, Vec<Row>>,
        now: i64,
    ) -> Option<Vec<Row>> {
        let Source::Collection(name) = &q.source else {
            return None;
        };
        if bindings.get(name).is_some() {
            return None;
        }
        let mut filter: Option<&Pred> = None;
        let mut by: Option<Option<&Field>> = None;
        for step in &q.steps {
            match step {
                Step::Filter(p) => {
                    if filter.is_some() || by.is_some() {
                        return None;
                    }
                    filter = Some(p);
                }
                Step::Count { by: b } => {
                    if by.is_some() {
                        return None;
                    }
                    by = Some(b.as_ref());
                }
                _ => return None,
            }
        }
        self.execute_filter_count(name, filter?, by?, now)
    }

    fn execute_filter_count(
        &self,
        name: &str,
        pred: &Pred,
        by: Option<&Field>,
        now: i64,
    ) -> Option<Vec<Row>> {
        // docs | title ~ "…" | count — columnar title scan + memchr (fair vs SQL COUNT(*)).
        if name == "docs"
            && by.is_none()
            && let Pred::Contains { field, needle } = pred
            && field.leaf() == Some("title")
        {
            let n = self.store.docs_title_contains_count(needle);
            return Some(vec![hits_row(n)]);
        }

        // docs | title ~ "…" | count by layer — columnar title scan + memchr.
        if name == "docs"
            && let Some(by_field) = by
            && let Pred::Contains { field, needle } = pred
            && field.leaf() == Some("title")
            && by_field.leaf() == Some("layer")
        {
            let map = self.store.docs_title_contains_count_by_layer(needle);
            return Some(count_arc_map_to_rows(&by_field.as_str(), map));
        }

        let Some(by_field) = by else {
            // Full count scan (no early termination) for standard COUNT queries
            if let Some((uses, covered)) = crate::index::pick_index_with_coverage(&self.catalog, name, pred)
                && covered
            {
                let n = self.store.index_seek_count(name, &uses, now)?;
                return Some(vec![hits_row(n as i64)]);
            }
            let col = self.store.collection(name);
            let mut n = 0i64;
            for row in col {
                if eval_pred(pred, row, now) {
                    n += 1;
                }
            }
            return Some(vec![hits_row(n)]);
        };
        let by_key = by_field.as_str();

        // Index-covered + group key fixed by equality → pure seek_count.
        if let Some((uses, covered)) = crate::index::pick_index_with_coverage(&self.catalog, name, pred)
            && covered
        {
            if let Some(v) = eq_value_for_field(pred, &by_key) {
                let n = self.store.index_seek_count(name, &uses, now)?;
                if n == 0 {
                    return Some(Vec::new());
                }
                return Some(vec![count_row(&by_key, v, n as i64)]);
            }
            // Index covers filter but group key varies — read only the by-field.
            let idxs = self.store.index_seek(name, &uses, now)?;
            let mut map: FxHashMap<Cell, i64> = FxHashMap::default();
            for i in idxs {
                let Some(row) = self.store.get_by_idx(name, i) else {
                    continue;
                };
                let cell = row
                    .get(&by_key)
                    .map(Cell::clone_for_borrow)
                    .unwrap_or(Cell::Null);
                *map.entry(cell).or_insert(0) += 1;
            }
            return Some(count_cell_map_to_rows(&by_key, map));
        }

        // Full scan count without cloning rows.
        let col = self.store.collection(name);
        let mut map: FxHashMap<Cell, i64> = FxHashMap::default();
        for r in col {
            if eval_pred(pred, r, now) {
                let cell = r
                    .get(&by_key)
                    .map(Cell::clone_for_borrow)
                    .unwrap_or(Cell::Null);
                *map.entry(cell).or_insert(0) += 1;
            }
        }
        Some(count_cell_map_to_rows(&by_key, map))
    }

    /// `col | pred | { fields } | take …` — project from hot columns when possible.
    fn try_filter_project(
        &self,
        q: &Query,
        bindings: &BTreeMap<String, Vec<Row>>,
        now: i64,
    ) -> Option<Vec<Row>> {
        let Source::Collection(name) = &q.source else {
            return None;
        };
        if bindings.get(name).is_some() {
            return None;
        }
        let mut filter: Option<&Pred> = None;
        let mut fields: Option<&[Field]> = None;
        let mut explicit_take: Option<Option<i64>> = None;
        let mut skip_n: usize = 0;
        // This shortcut combines stages; only accept their canonical order.
        let mut phase = 0;
        for step in &q.steps {
            let next_phase = match step {
                Step::Filter(_) => 0,
                Step::Project(_) => 1,
                Step::Skip { .. } => 2,
                Step::Take { .. } => 3,
                _ => return None,
            };
            if next_phase < phase || (next_phase == 3 && explicit_take.is_some()) {
                return None;
            }
            phase = next_phase;
            match step {
                Step::Filter(p) => {
                    if filter.is_some() {
                        return None;
                    }
                    filter = Some(p);
                }
                Step::Project(f) => {
                    if fields.is_some() {
                        return None;
                    }
                    fields = Some(f.as_slice());
                }
                Step::Skip { n } => {
                    skip_n = skip_n.saturating_add((*n).max(0) as usize);
                }
                Step::Take { n } => {
                    explicit_take = Some(*n);
                }
                _ => return None,
            }
        }
        let pred = filter?;
        let fields = fields?;
        let names = field_names(fields);
        if names.is_empty() {
            return None;
        }

        let dict_limit = match explicit_take {
            Some(Some(n)) => Some(skip_n.saturating_add(n.max(0) as usize)),
            Some(None) => None,
            None => Some(skip_n.saturating_add(50)),
        };

        let mut rows = {
            // docs | title ~ needle | { hot… }
            if name == "docs"
                && let Pred::Contains { field, needle } = pred
                && field.leaf() == Some("title")
            {
                self.store
                    .scan_docs_hot_project(&names, Some(needle.as_str()))?
            } else if name == "docs"
                && let Pred::Cmp {
                    field,
                    op: CmpOp::Eq,
                    value: Value::String(target),
                } = pred
                && let Some(col_field @ ("wing" | "layer")) = field.leaf()
                && let Some(rows) =
                    self.store
                        .project_docs_dict_eq(col_field, target.as_ref(), &names, dict_limit)
            {
                rows
            } else if let Some((uses, covered)) = crate::index::pick_index_with_coverage(&self.catalog, name, pred)
                && let Some(idxs) = self.store.index_seek(name, &uses, now)
            {
                if name == "docs" && covered {
                    if let Some(rows) = self.store.project_docs_hot(&idxs, &names) {
                        rows
                    } else {
                        self.fetch_idxs_filtered(name, &idxs, pred, now, Some(fields), covered)
                    }
                } else {
                    self.fetch_idxs_filtered(name, &idxs, pred, now, Some(fields), covered)
                }
            } else {
                self.filter_scan(name, pred, now, Some(fields))
            }
        };

        if skip_n >= rows.len() {
            rows.clear();
        } else if skip_n > 0 {
            rows.drain(0..skip_n);
        }

        match explicit_take {
            Some(Some(n)) => {
                let n = n.max(0) as usize;
                if rows.len() > n {
                    rows.truncate(n);
                }
            }
            Some(None) => {}                              // take all
            None if rows.len() > 50 => rows.truncate(50), // implicit take
            None => {}
        }
        Some(rows)
    }

    /// `[filter?] | join | { fields } | take…` — row-API wrapper over columnar join.
    fn try_join_project(
        &self,
        q: &Query,
        bindings: &BTreeMap<String, Vec<Row>>,
        now: i64,
    ) -> Option<Vec<Row>> {
        Some(self.try_join_project_batch(q, bindings, now)?.to_rows())
    }

    /// Columnar filter? + project + skip*/take? (no join). Prefer [`Self::run_prepared_batch`].
    fn try_filter_project_batch(
        &self,
        q: &Query,
        bindings: &BTreeMap<String, Vec<Row>>,
        now: i64,
    ) -> Option<RecordBatch> {
        let Source::Collection(name) = &q.source else {
            return None;
        };
        if bindings.get(name).is_some() {
            return None;
        }

        let mut filter: Option<&Pred> = None;
        let mut fields: Option<&[Field]> = None;
        let mut explicit_take: Option<Option<i64>> = None;
        let mut skip_n: usize = 0;

        // This shortcut combines stages; only accept their canonical order.
        let mut phase = 0;
        for step in &q.steps {
            let next_phase = match step {
                Step::Filter(_) => 0,
                Step::Project(_) => 1,
                Step::Skip { .. } => 2,
                Step::Take { .. } => 3,
                _ => return None,
            };
            if next_phase < phase || (next_phase == 3 && explicit_take.is_some()) {
                return None;
            }
            phase = next_phase;
            match step {
                Step::Filter(p) => {
                    if filter.is_some() || fields.is_some() {
                        return None;
                    }
                    filter = Some(p);
                }
                Step::Project(f) => {
                    if fields.is_some() {
                        return None;
                    }
                    fields = Some(f.as_slice());
                }
                Step::Skip { n } => {
                    fields?;
                    skip_n = skip_n.saturating_add((*n).max(0) as usize);
                }
                Step::Take { n } => {
                    if fields.is_none() || explicit_take.is_some() {
                        return None;
                    }
                    explicit_take = Some(*n);
                }
                _ => return None,
            }
        }

        let fields = fields?;
        let names = field_names(fields);
        if names.is_empty() {
            return None;
        }

        let (source, need_filter) = {
            if let Some(pred) = filter {
                if let Some((uses, covered)) = crate::index::pick_index_with_coverage(&self.catalog, name, pred)
                    && let Some(idxs) = self.store.index_seek(name, &uses, now)
                {
                    (JoinLeftSource::Idxs(idxs), !covered)
                } else {
                    (
                        JoinLeftSource::Scan {
                            len: self.store.collection(name).len(),
                        },
                        true,
                    )
                }
            } else {
                (
                    JoinLeftSource::Scan {
                        len: self.store.collection(name).len(),
                    },
                    false,
                )
            }
        };

        let left_len = match &source {
            JoinLeftSource::Idxs(i) => i.len(),
            JoinLeftSource::Scan { len } => *len,
        };
        let names_arc: Arc<[String]> = names.clone().into();

        if left_len >= 131_072 {
            let threads = std::thread::available_parallelism()
                .map(|p| p.get())
                .unwrap_or(1)
                .min(8)
                .min((left_len + 16383) / 16384);
            if threads > 1 {
                let chunk_size = (left_len + threads - 1) / threads;
                let thread_batches: Vec<RecordBatch> = std::thread::scope(|scope| {
                    let mut handles = Vec::with_capacity(threads);
                    for chunk_idx in 0..threads {
                        let c_start = chunk_idx * chunk_size;
                        if c_start >= left_len {
                            break;
                        }
                        let c_end = (c_start + chunk_size).min(left_len);
                        let source_ref = &source;
                        let names_ref = &names;
                        let names_arc_clone = names_arc.clone();
                        handles.push(scope.spawn(move || {
                            let mut local_batch = RecordBatch::with_capacity(names_arc_clone, c_end - c_start);
                            for pos in c_start..c_end {
                                let idx = match source_ref {
                                    JoinLeftSource::Idxs(idxs) => idxs[pos],
                                    JoinLeftSource::Scan { .. } => pos,
                                };
                                let Some(row) = self.store.get_by_idx(name, idx) else {
                                    continue;
                                };
                                if need_filter
                                    && let Some(pred) = filter
                                    && !eval_pred(pred, row, now)
                                {
                                    continue;
                                }
                                push_project_batch_row(&mut local_batch, row, names_ref);
                            }
                            local_batch
                        }));
                    }
                    handles.into_iter().map(|h| h.join().unwrap()).collect()
                });
                let mut batch = RecordBatch::empty(names_arc);
                let total_rows: usize = thread_batches.iter().map(|b| b.n()).sum();
                for col_i in 0..batch.cols.len() {
                    batch.cols[col_i].reserve(total_rows);
                }
                for mut thread_batch in thread_batches {
                    for (col_i, col) in thread_batch.cols.drain(..).enumerate() {
                        batch.cols[col_i].extend(col);
                    }
                }
                finish_batch(&mut batch, skip_n, explicit_take);
                return Some(batch);
            }
        }

        let mut batch = RecordBatch::with_capacity(names_arc, left_len);
        let mut pos = 0usize;
        loop {
            let idx = match &source {
                JoinLeftSource::Idxs(idxs) => {
                    if pos >= idxs.len() {
                        break;
                    }
                    let i = idxs[pos];
                    pos += 1;
                    i
                }
                JoinLeftSource::Scan { len } => {
                    if pos >= *len {
                        break;
                    }
                    let i = pos;
                    pos += 1;
                    i
                }
            };
            let Some(row) = self.store.get_by_idx(name, idx) else {
                continue;
            };
            if need_filter
                && let Some(pred) = filter
                && !eval_pred(pred, row, now)
            {
                continue;
            }
            push_project_batch_row(&mut batch, row, &names);
        }

        finish_batch(&mut batch, skip_n, explicit_take);
        Some(batch)
    }

    /// Columnar FK join+project (OLAP). Prefer [`Self::run_prepared_batch`].
    fn try_join_project_batch(
        &self,
        q: &Query,
        bindings: &BTreeMap<String, Vec<Row>>,
        now: i64,
    ) -> Option<RecordBatch> {
        let Source::Collection(left_name) = &q.source else {
            return None;
        };
        if bindings.get(left_name).is_some() {
            return None;
        }

        let mut filter: Option<&Pred> = None;
        let mut join: Option<(bool, &str, &str)> = None;
        let mut fields: Option<&[Field]> = None;
        let mut explicit_take: Option<Option<i64>> = None;
        let mut skip_n: usize = 0;
        let mut saw_join = false;

        for step in &q.steps {
            match step {
                Step::Filter(p) if !saw_join => {
                    if filter.is_some() {
                        return None;
                    }
                    filter = Some(p);
                }
                Step::Join {
                    left,
                    collection,
                    on,
                } if !saw_join => {
                    saw_join = true;
                    join = Some((*left, collection.as_str(), on.as_str()));
                }
                Step::Project(f) if saw_join => {
                    if fields.is_some() {
                        return None;
                    }
                    fields = Some(f.as_slice());
                }
                Step::Skip { n } if saw_join => {
                    skip_n = skip_n.saturating_add((*n).max(0) as usize);
                }
                Step::Take { n } if saw_join => {
                    if explicit_take.is_some() {
                        return None;
                    }
                    explicit_take = Some(*n);
                }
                Step::Project(_) | Step::Skip { .. } | Step::Take { .. } if !saw_join => {
                    return None;
                }
                _ => return None,
            }
        }

        let (left_join, right_col, on) = join?;
        let fields = fields?;
        let names = field_names(fields);
        if names.is_empty() {
            return None;
        }

        let to_field = self
            .catalog
            .find_fk(left_name, on, right_col)
            .map(|fk| fk.to_field.as_str())
            .unwrap_or("id");
        if to_field != "id" {
            return None;
        }

        // Hot path: orders ⋈ users via SoA columns → RecordBatch (no per-row BTreeMap).
        if left_name == "orders"
            && right_col == "users"
            && on == "user_id"
            && self.store.orders_soa_ready()
            && self.store.users_soa_ready()
        {
            return self.join_orders_users_soa_batch(
                filter,
                left_join,
                &names,
                skip_n,
                explicit_take,
                now,
            );
        }

        let (right_fields, plan) = plan_join_fields(right_col, &names);
        let rights = self.store.collection(right_col);

        let (source, need_filter) = {
            if let Some(pred) = filter {
                if let Some((uses, covered)) = crate::index::pick_index_with_coverage(&self.catalog, left_name, pred)
                    && let Some(idxs) = self.store.index_seek(left_name, &uses, now)
                {
                    (JoinLeftSource::Idxs(idxs), !covered)
                } else {
                    (
                        JoinLeftSource::Scan {
                            len: self.store.collection(left_name).len(),
                        },
                        true,
                    )
                }
            } else {
                (
                    JoinLeftSource::Scan {
                        len: self.store.collection(left_name).len(),
                    },
                    false,
                )
            }
        };

        let left_len = match &source {
            JoinLeftSource::Idxs(i) => i.len(),
            JoinLeftSource::Scan { len } => *len,
        };

        // Cost-based join probe strategy:
        // 1. If right table has all IDs indexed in by_id and left_len < rights.len(),
        //    skip hash map allocation entirely and resolve directly in O(1) via by_id.
        // 2. If left_len >= rights.len(), build dense probe once for fast slice borrowing.
        // 3. If by_id is partial and left is significantly smaller, build an asymmetric probe.
        let store_has_all_ids = self
            .store
            .by_id
            .get(right_col)
            .map_or(false, |m| m.len() == rights.len());

        let use_direct_lookup = store_has_all_ids && left_len < rights.len();
        let probe = if use_direct_lookup {
            None
        } else if store_has_all_ids || left_len * 4 >= rights.len() {
            Some(build_right_probe(rights, &right_fields))
        } else {
            // Asymmetric probe: only hash keys needed by left rows
            let mut needed: FxHashSet<&str> = FxHashSet::default();
            for i in 0..left_len {
                let idx = match &source {
                    JoinLeftSource::Idxs(idxs) => idxs[i],
                    JoinLeftSource::Scan { .. } => i,
                };
                if let Some(row) = self.store.get_by_idx(left_name, idx) {
                    if !need_filter || filter.map_or(true, |p| eval_pred(p, row, now)) {
                        if let Some(k) = row.get(on).and_then(Cell::text) {
                            needed.insert(k);
                        }
                    }
                }
            }
            let mut p = FxHashMap::default();
            p.reserve(needed.len());
            for r in rights {
                if let Some(id) = r.get("id").and_then(Cell::text) {
                    if needed.contains(id) {
                        let cells = right_fields
                            .iter()
                            .map(|f| r.get(f).cloned().unwrap_or(Cell::Null))
                            .collect();
                        p.insert(id, cells);
                    }
                }
            }
            Some(p)
        };

        let names_arc: Arc<[String]> = names.clone().into();
        let mut batch = RecordBatch::with_capacity(names_arc, left_len);
        let mut cell_buf: smallvec::SmallVec<[Cell; 8]> = smallvec::SmallVec::new();
        let mut pos = 0usize;
        loop {
            let idx = match &source {
                JoinLeftSource::Idxs(idxs) => {
                    if pos >= idxs.len() {
                        break;
                    }
                    let i = idxs[pos];
                    pos += 1;
                    i
                }
                JoinLeftSource::Scan { len } => {
                    if pos >= *len {
                        break;
                    }
                    let i = pos;
                    pos += 1;
                    i
                }
            };
            let Some(left_row) = self.store.get_by_idx(left_name, idx) else {
                continue;
            };
            if need_filter
                && let Some(pred) = filter
                && !eval_pred(pred, left_row, now)
            {
                continue;
            }

            if let Some(ref probe_map) = probe {
                let right_cells = left_row
                    .get(on)
                    .and_then(Cell::text)
                    .and_then(|k| probe_map.get(k).map(|v| v.as_slice()));
                if right_cells.is_none() && !left_join {
                    continue;
                }
                push_join_batch_row(&mut batch, left_row, right_cells, &plan);
            } else {
                // Direct O(1) store lookup without intermediate hash map
                let right_row = left_row
                    .get(on)
                    .and_then(Cell::text)
                    .and_then(|k| self.store.get_by_id(right_col, k));
                if right_row.is_none() && !left_join {
                    continue;
                }
                if let Some(r) = right_row {
                    cell_buf.clear();
                    for f in &right_fields {
                        cell_buf.push(r.get(f).cloned().unwrap_or(Cell::Null));
                    }
                    push_join_batch_row(&mut batch, left_row, Some(&cell_buf), &plan);
                } else {
                    push_join_batch_row(&mut batch, left_row, None, &plan);
                }
            }
        }

        finish_batch(&mut batch, skip_n, explicit_take);
        Some(batch)
    }

    /// orders ⋈ users using parallel columns + hash probe on users.
    fn join_orders_users_soa_batch(
        &self,
        filter: Option<&Pred>,
        left_join: bool,
        names: &[String],
        skip_n: usize,
        explicit_take: Option<Option<i64>>,
        now: i64,
    ) -> Option<RecordBatch> {
        let (right_fields, plan) = plan_join_fields("users", names);
        let users_id = self.store.users_id();
        let users_email = self.store.users_email();
        let store_users_by_id = self
            .store
            .by_id
            .get("users")
            .filter(|m| m.len() == users_id.len());
        let mut fallback_probe: FxHashMap<&str, usize> = FxHashMap::default();
        if store_users_by_id.is_none() {
            fallback_probe.reserve(users_id.len());
            for (i, user_id) in users_id.iter().enumerate() {
                fallback_probe.insert(user_id.as_ref(), i);
            }
        }
        let lookup_user = |uid: &str| -> Option<usize> {
            if let Some(m) = store_users_by_id {
                m.get(uid).copied()
            } else {
                fallback_probe.get(uid).copied()
            }
        };

        let orders_id = self.store.orders_id();
        let orders_uid = self.store.orders_user_id();
        let orders_total = self.store.orders_total();
        let n = orders_id.len();

        let total_gt = filter.and_then(pred_total_gt);
        let (source, need_row_filter) = if total_gt.is_some() {
            (JoinLeftSource::Scan { len: n }, false)
        } else if let Some(pred) = filter {
            if let Some((uses, covered)) = crate::index::pick_index_with_coverage(&self.catalog, "orders", pred)
                && let Some(idxs) = self.store.index_seek("orders", &uses, now)
            {
                (JoinLeftSource::Idxs(idxs), !covered)
            } else {
                (JoinLeftSource::Scan { len: n }, true)
            }
        } else {
            (JoinLeftSource::Scan { len: n }, false)
        };

        let names_arc: Arc<[String]> = names.to_vec().into();
        let mut batch = RecordBatch::with_capacity(names_arc.clone(), n);

        // Specialized: { id, users.email, total } — hottest bench / OLAP shape.
        let fast_id_email_total = plan.len() == 3
            && matches!(plan[0], JoinFieldPlan::Left(ref k) if k == "id")
            && matches!(
                plan[1],
                JoinFieldPlan::Right { ref out, idx: 0 } if out == "users.email"
                    || right_fields.first().is_some_and(|f| f == "email")
            )
            && matches!(plan[2], JoinFieldPlan::Left(ref k) if k == "total");

        let left_len = match &source {
            JoinLeftSource::Idxs(idxs) => idxs.len(),
            JoinLeftSource::Scan { len } => *len,
        };

        if left_len >= 131_072 {
            let threads = std::thread::available_parallelism()
                .map(|p| p.get())
                .unwrap_or(1)
                .min(8)
                .min((left_len + 16383) / 16384);
            if threads > 1 {
                let chunk_size = (left_len + threads - 1) / threads;
                let thread_batches: Vec<RecordBatch> = std::thread::scope(|scope| {
                    let mut handles = Vec::with_capacity(threads);
                    for chunk_idx in 0..threads {
                        let c_start = chunk_idx * chunk_size;
                        if c_start >= left_len {
                            break;
                        }
                        let c_end = (c_start + chunk_size).min(left_len);
                        let source_ref = &source;
                        let lookup_user_ref = &lookup_user;
                        let plan_ref = &plan;
                        let names_arc_clone = names_arc.clone();
                        let right_fields_ref = &right_fields;
                        handles.push(scope.spawn(move || {
                            let mut local_batch = RecordBatch::with_capacity(names_arc_clone, c_end - c_start);
                            if fast_id_email_total {
                                for pos in c_start..c_end {
                                    let idx = match source_ref {
                                        JoinLeftSource::Idxs(idxs) => idxs[pos],
                                        JoinLeftSource::Scan { .. } => pos,
                                    };
                                    if idx >= n {
                                        continue;
                                    }
                                    if let Some(min) = total_gt {
                                        if orders_total[idx] <= min {
                                            continue;
                                        }
                                    } else if need_row_filter && let Some(pred) = filter {
                                        let Some(left_row) = self.store.get_by_idx("orders", idx) else {
                                            continue;
                                        };
                                        if !eval_pred(pred, left_row, now) {
                                            continue;
                                        }
                                    }
                                    let uid = orders_uid[idx].as_ref();
                                    let Some(ui) = lookup_user_ref(uid) else {
                                        if left_join {
                                            local_batch.cols[0].push(Cell::Text(Arc::clone(&orders_id[idx])));
                                            local_batch.cols[1].push(Cell::Null);
                                            local_batch.cols[2].push(Cell::Float(orders_total[idx]));
                                        }
                                        continue;
                                    };
                                    local_batch.cols[0].push(Cell::Text(Arc::clone(&orders_id[idx])));
                                    local_batch.cols[1].push(Cell::Text(Arc::clone(&users_email[ui])));
                                    local_batch.cols[2].push(Cell::Float(orders_total[idx]));
                                }
                            } else {
                                for pos in c_start..c_end {
                                    let idx = match source_ref {
                                        JoinLeftSource::Idxs(idxs) => idxs[pos],
                                        JoinLeftSource::Scan { .. } => pos,
                                    };
                                    if idx >= n {
                                        continue;
                                    }
                                    if let Some(min) = total_gt {
                                        if orders_total[idx] <= min {
                                            continue;
                                        }
                                    } else if need_row_filter && let Some(pred) = filter {
                                        let Some(left_row) = self.store.get_by_idx("orders", idx) else {
                                            continue;
                                        };
                                        if !eval_pred(pred, left_row, now) {
                                            continue;
                                        }
                                    }
                                    let uid = orders_uid[idx].as_ref();
                                    let right_i = lookup_user_ref(uid);
                                    if right_i.is_none() && !left_join {
                                        continue;
                                    }
                                    for (col_i, p) in plan_ref.iter().enumerate() {
                                        let cell = match p {
                                            JoinFieldPlan::Left(k) => match k.as_str() {
                                                "id" => Cell::Text(Arc::clone(&orders_id[idx])),
                                                "user_id" => Cell::Text(Arc::clone(&orders_uid[idx])),
                                                "total" => Cell::Float(orders_total[idx]),
                                                _ => self
                                                    .store
                                                    .get_by_idx("orders", idx)
                                                    .and_then(|r| r.get(k).cloned())
                                                    .unwrap_or(Cell::Null),
                                            },
                                            JoinFieldPlan::Right { out: _, idx: r_idx } => match right_i {
                                                Some(ui) => match right_fields_ref.get(*r_idx).map(String::as_str) {
                                                    Some("id") => Cell::Text(Arc::clone(&users_id[ui])),
                                                    Some("email") => Cell::Text(Arc::clone(&users_email[ui])),
                                                    _ => Cell::Null,
                                                },
                                                None => Cell::Null,
                                            },
                                        };
                                        local_batch.cols[col_i].push(cell);
                                    }
                                }
                            }
                            local_batch
                        }));
                    }
                    handles.into_iter().map(|h| h.join().unwrap()).collect()
                });
                let mut batch = RecordBatch::empty(names_arc);
                let total_rows: usize = thread_batches.iter().map(|b| b.n()).sum();
                for col_i in 0..batch.cols.len() {
                    batch.cols[col_i].reserve(total_rows);
                }
                for mut thread_batch in thread_batches {
                    for (col_i, col) in thread_batch.cols.drain(..).enumerate() {
                        batch.cols[col_i].extend(col);
                    }
                }
                finish_batch(&mut batch, skip_n, explicit_take);
                return Some(batch);
            }
        }

        let mut pos = 0usize;
        if fast_id_email_total {
            loop {
                let idx = match &source {
                    JoinLeftSource::Idxs(idxs) => {
                        if pos >= idxs.len() {
                            break;
                        }
                        let i = idxs[pos];
                        pos += 1;
                        i
                    }
                    JoinLeftSource::Scan { len } => {
                        if pos >= *len {
                            break;
                        }
                        let i = pos;
                        pos += 1;
                        i
                    }
                };
                if idx >= n {
                    continue;
                }
                if let Some(min) = total_gt {
                    if orders_total[idx] <= min {
                        continue;
                    }
                } else if need_row_filter && let Some(pred) = filter {
                    let Some(left_row) = self.store.get_by_idx("orders", idx) else {
                        continue;
                    };
                    if !eval_pred(pred, left_row, now) {
                        continue;
                    }
                }
                let uid = orders_uid[idx].as_ref();
                let Some(ui) = lookup_user(uid) else {
                    if left_join {
                        batch.cols[0].push(Cell::Text(Arc::clone(&orders_id[idx])));
                        batch.cols[1].push(Cell::Null);
                        batch.cols[2].push(Cell::Float(orders_total[idx]));
                    }
                    continue;
                };
                batch.cols[0].push(Cell::Text(Arc::clone(&orders_id[idx])));
                batch.cols[1].push(Cell::Text(Arc::clone(&users_email[ui])));
                batch.cols[2].push(Cell::Float(orders_total[idx]));
            }
            finish_batch(&mut batch, skip_n, explicit_take);
            return Some(batch);
        }

        loop {
            let idx = match &source {
                JoinLeftSource::Idxs(idxs) => {
                    if pos >= idxs.len() {
                        break;
                    }
                    let i = idxs[pos];
                    pos += 1;
                    i
                }
                JoinLeftSource::Scan { len } => {
                    if pos >= *len {
                        break;
                    }
                    let i = pos;
                    pos += 1;
                    i
                }
            };
            if idx >= n {
                continue;
            }
            if let Some(min) = total_gt {
                if orders_total[idx] <= min {
                    continue;
                }
            } else if need_row_filter && let Some(pred) = filter {
                let Some(left_row) = self.store.get_by_idx("orders", idx) else {
                    continue;
                };
                if !eval_pred(pred, left_row, now) {
                    continue;
                }
            }

            let uid = orders_uid[idx].as_ref();
            let right_i = lookup_user(uid);
            if right_i.is_none() && !left_join {
                continue;
            }

            for (col_i, p) in plan.iter().enumerate() {
                let cell = match p {
                    JoinFieldPlan::Left(k) => match k.as_str() {
                        "id" => Cell::Text(Arc::clone(&orders_id[idx])),
                        "user_id" => Cell::Text(Arc::clone(&orders_uid[idx])),
                        "total" => Cell::Float(orders_total[idx]),
                        _ => self
                            .store
                            .get_by_idx("orders", idx)
                            .and_then(|r| r.get(k).cloned())
                            .unwrap_or(Cell::Null),
                    },
                    JoinFieldPlan::Right { out: _, idx: ri } => {
                        let Some(ui) = right_i else {
                            batch.cols[col_i].push(Cell::Null);
                            continue;
                        };
                        let fname = right_fields.get(*ri).map(|s| s.as_str()).unwrap_or("");
                        match fname {
                            "id" => Cell::Text(Arc::clone(&users_id[ui])),
                            "email" => Cell::Text(Arc::clone(&users_email[ui])),
                            _ => Cell::Null,
                        }
                    }
                };
                batch.cols[col_i].push(cell);
            }
        }

        finish_batch(&mut batch, skip_n, explicit_take);
        Some(batch)
    }

    fn filter_scan(
        &self,
        name: &str,
        pred: &Pred,
        now: i64,
        project: Option<&[Field]>,
    ) -> Vec<Row> {
        let names = project.map(field_names);
        // docs title-contains → columnar project when fields are hot.
        if name == "docs"
            && let Pred::Contains { field, needle } = pred
            && field.leaf() == Some("title")
            && let Some(ns) = names.as_deref()
            && let Some(rows) = self.store.scan_docs_hot_project(ns, Some(needle.as_str()))
        {
            return rows;
        }
        let col = self.store.collection(name);
        let mut out = Vec::new();
        for r in col {
            if eval_pred(pred, r, now) {
                out.push(match names.as_deref() {
                    Some(fs) => project_fields(r, fs),
                    None => {
                        let mut row = crate::store::acquire_row();
                        for (k, v) in r {
                            row.insert(k.clone(), v.clone_for_borrow());
                        }
                        row
                    }
                });
            }
        }
        out
    }

    fn fetch_idxs_filtered(
        &self,
        name: &str,
        idxs: &[usize],
        pred: &Pred,
        now: i64,
        project: Option<&[Field]>,
        covered: bool,
    ) -> Vec<Row> {
        let names = project.map(field_names);
        if covered
            && name == "docs"
            && let Some(ns) = names.as_deref()
            && let Some(rows) = self.store.project_docs_hot(idxs, ns)
        {
            return rows;
        }
        let mut out = Vec::with_capacity(idxs.len());
        let id_only = names
            .as_deref()
            .is_some_and(|fs| fs.len() == 1 && fs[0] == "id");
        for &i in idxs {
            let Some(row) = self.store.get_by_idx(name, i) else {
                continue;
            };
            if !covered && !eval_pred(pred, row, now) {
                continue;
            }
            if id_only {
                let mut r = crate::store::acquire_row();
                r.insert(
                    "id".into(),
                    row.get("id")
                        .map(Cell::clone_for_borrow)
                        .unwrap_or(Cell::Null),
                );
                out.push(r);
            } else {
                out.push(match names.as_deref() {
                    Some(fs) => project_fields(row, fs),
                    None => {
                        let mut r = crate::store::acquire_row();
                        for (k, v) in row {
                            r.insert(k.clone(), v.clone_for_borrow());
                        }
                        r
                    }
                });
            }
        }
        out
    }

    fn scan_projected(&self, name: &str, fields: &[Field], limit: Option<usize>) -> Vec<Row> {
        if name == "catalog" {
            let mut r = self.scan_catalog();
            r = project(&r, fields);
            if let Some(lim) = limit {
                r.truncate(lim);
            }
            return r;
        }
        let names = field_names(fields);
        let col = self.store.collection(name);
        let count = match limit {
            Some(lim) => lim.min(col.len()),
            None => col.len(),
        };
        let mut out = Vec::with_capacity(count);
        for r in col.iter().take(count) {
            out.push(project_fields(r, &names));
        }
        out
    }

    fn scan(&self, name: &str, limit: Option<usize>) -> Vec<Row> {
        if name == "catalog" {
            let mut r = self.scan_catalog();
            if let Some(lim) = limit {
                r.truncate(lim);
            }
            return r;
        }
        let col = self.store.collection(name);
        match limit {
            Some(lim) => col.iter().take(lim).cloned().collect(),
            None => col.to_vec(),
        }
    }

    fn scan_catalog(&self) -> Vec<Row> {
        let mut rows = Vec::new();
        for name in self.catalog.collections.keys() {
            rows.push(meta_row("col", name));
        }
        for name in self.catalog.rels.keys() {
            rows.push(meta_row("rel", name));
        }
        rows
    }

    fn join_rows(
        &self,
        left_col: &str,
        left: Vec<Row>,
        right_col: &str,
        on: &str,
        left_join: bool,
    ) -> Result<Vec<Row>, Error> {
        let to_field = self
            .catalog
            .find_fk(left_col, on, right_col)
            .map(|fk| fk.to_field.as_str())
            .unwrap_or("id");

        if left.is_empty() {
            return Ok(Vec::new());
        }

        // FK → unique id: hash-build right once, then probe (tight map, no by_id indirection).
        if to_field == "id" {
            let right = self.store.collection(right_col);
            let store_has_all_ids = self
                .store
                .by_id
                .get(right_col)
                .map_or(false, |m| m.len() == right.len());
            if store_has_all_ids {
                let mut out = Vec::with_capacity(left.len());
                for l in left {
                    let right = l
                        .get(on)
                        .and_then(Cell::text)
                        .and_then(|k| self.store.get_by_id(right_col, k));
                    match right {
                        Some(r) => out.push(merge_join_row(l, r, right_col)),
                        None if left_join => out.push(l),
                        None => {}
                    }
                    if out.len() > self.quotas.max_join_expansion {
                        return Err(Error::runtime(format!(
                            "Cartesian join expansion {} exceeds max_join_expansion quota {}",
                            out.len(),
                            self.quotas.max_join_expansion
                        )));
                    }
                }
                return Ok(out);
            }

            // Asymmetric hash join: if left is smaller, only index keys needed by left
            let needed: Option<FxHashSet<&str>> = if left.len() * 4 < right.len() {
                Some(
                    left.iter()
                        .filter_map(|l| l.get(on).and_then(Cell::text))
                        .collect(),
                )
            } else {
                None
            };

            let mut by_id: FxHashMap<&str, &Row> = FxHashMap::default();
            if let Some(ref needed_set) = needed {
                by_id.reserve(needed_set.len());
                for r in right {
                    if let Some(id) = r.get("id").and_then(Cell::text) {
                        if needed_set.contains(id) {
                            by_id.insert(id, r);
                        }
                    }
                }
            } else {
                by_id.reserve(right.len());
                for r in right {
                    if let Some(id) = r.get("id").and_then(Cell::text) {
                        by_id.insert(id, r);
                    }
                }
            }

            let mut out = Vec::with_capacity(left.len());
            for l in left {
                let right = l
                    .get(on)
                    .and_then(Cell::text)
                    .and_then(|k| by_id.get(k).copied());
                match right {
                    Some(r) => out.push(merge_join_row(l, r, right_col)),
                    None if left_join => out.push(l),
                    None => {}
                }
            }
            return Ok(out);
        }

        // Non-id join key: hash probe without cloning the right collection.
        let right = self.store.collection(right_col);
        let needed_keys: FxHashSet<String> = left
            .iter()
            .filter_map(|l| l.get(on).and_then(cell_key))
            .collect();
        if needed_keys.is_empty() {
            return Ok(if left_join { left } else { Vec::new() });
        }

        let mut idx: FxHashMap<String, Vec<&Row>> = FxHashMap::default();
        idx.reserve(needed_keys.len());
        for r in right {
            if let Some(k) = cell_key(r.get(to_field).unwrap_or(&Cell::Null)) {
                if needed_keys.contains(&k) {
                    idx.entry(k).or_default().push(r);
                }
            }
        }
        let mut out = Vec::with_capacity(left.len());
        for mut l in left {
            let key = l.get(on).and_then(cell_key);
            let hits = key.as_ref().and_then(|k| idx.get(k));
            if let Some(rs) = hits {
                for (i, r) in rs.iter().enumerate() {
                    let base = if i + 1 == rs.len() {
                        std::mem::take(&mut l)
                    } else {
                        l.clone()
                    };
                    out.push(merge_join_row(base, r, right_col));
                    if out.len() > self.quotas.max_join_expansion {
                        return Err(Error::runtime(format!(
                            "Cartesian join expansion {} exceeds max_join_expansion quota {}",
                            out.len(),
                            self.quotas.max_join_expansion
                        )));
                    }
                }
            } else if left_join {
                out.push(l);
            }
        }
        Ok(out)
    }

    fn hop(&self, primary: &str, rows: &[Row], rel: &str, depth: i64) -> Result<Vec<Row>, Error> {
        let (edge_rel, reverse) = match self.catalog.rel(rel) {
            Some(r) if let Some(of) = &r.reverse_of => (of.as_str(), true),
            _ => (rel, false),
        };
        #[cfg(feature = "gpu")]
        if self.gpu.is_some() {
            return self.gpu_walk_rows(primary, rows, edge_rel, reverse, depth, false);
        }
        let mut frontier: BTreeSet<String> = BTreeSet::new();
        for r in rows {
            if let Some(id) = row_text(r, "id") {
                frontier.insert(id.to_string());
            }
            if let Some(uri) = row_text(r, "uri") {
                frontier.insert(uri.to_string());
            }
        }
        let mut seen = frontier.clone();
        let mut reached: BTreeSet<String> = BTreeSet::new();
        let depth = depth.clamp(1, 3);
        for _ in 0..depth {
            let mut next = BTreeSet::new();
            let gpu_mask = self.graph_frontier_mask(edge_rel, reverse, &frontier)?;
            if let Some(mask) = &gpu_mask {
                for (edge_index, e) in self.store.edges.iter().enumerate() {
                    if e.rel != edge_rel {
                        continue;
                    }
                    let dst = if reverse {
                        e.from.as_str()
                    } else {
                        e.to.as_str()
                    };
                    if mask[edge_index] && seen.insert(dst.to_string()) {
                        next.insert(dst.to_string());
                        reached.insert(dst.to_string());
                    }
                }
            } else {
                for src in &frontier {
                    self.store.for_each_neighbor(edge_rel, src, reverse, |dst| {
                        if seen.insert(dst.to_string()) {
                            next.insert(dst.to_string());
                            reached.insert(dst.to_string());
                        }
                    });
                }
            }
            if seen.len() > self.quotas.max_graph_expansion {
                return Err(Error::runtime(format!(
                    "graph expansion count {} exceeds max_graph_expansion quota {}",
                    seen.len(),
                    self.quotas.max_graph_expansion
                )));
            }
            if reached.len() >= 300 {
                break;
            }
            frontier = next;
            if frontier.is_empty() {
                break;
            }
        }
        let mut out = Vec::new();
        for key in reached {
            if let Some(row) = self.store.find_doc_key(&key) {
                out.push(row.clone());
            } else if primary != "docs" {
                if let Some(row) = self.store.get_by_id(primary, &key) {
                    out.push(row.clone());
                } else if let Some(row) = self
                    .store
                    .collection(primary)
                    .iter()
                    .find(|r| row_text(r, "id") == Some(key.as_str()))
                {
                    out.push(row.clone());
                }
            }
            if out.len() >= 300 {
                break;
            }
        }
        Ok(out)
    }

    /// Subgraph edges reachable from `rows` via `rel` (same walk as [`Self::hop`]).
    fn graph_edges(&self, rows: &[Row], rel: &str, depth: i64) -> Result<Vec<Row>, Error> {
        let (edge_rel, reverse) = match self.catalog.rel(rel) {
            Some(r) if let Some(of) = &r.reverse_of => (of.as_str(), true),
            _ => (rel, false),
        };
        #[cfg(feature = "gpu")]
        if self.gpu.is_some() {
            return self.gpu_walk_rows("docs", rows, edge_rel, reverse, depth, true);
        }
        let mut frontier: BTreeSet<String> = BTreeSet::new();
        for r in rows {
            if let Some(id) = row_text(r, "id") {
                frontier.insert(id.to_string());
            }
            if let Some(uri) = row_text(r, "uri") {
                frontier.insert(uri.to_string());
            }
        }
        let mut seen_nodes = frontier.clone();
        let mut seen_edges: BTreeSet<(String, String, String)> = BTreeSet::new();
        let mut out = Vec::new();
        let depth = depth.clamp(1, 3);
        for _ in 0..depth {
            let mut next = BTreeSet::new();
            let gpu_mask = self.graph_frontier_mask(edge_rel, reverse, &frontier)?;
            if let Some(mask) = &gpu_mask {
                for (edge_index, e) in self.store.edges.iter().enumerate() {
                    if e.rel != edge_rel || !mask[edge_index] {
                        continue;
                    }
                    let dst = if reverse {
                        e.from.as_str()
                    } else {
                        e.to.as_str()
                    };
                    let key = (e.rel.clone(), e.from.clone(), e.to.clone());
                    if seen_edges.insert(key) {
                        out.push(edge_row(&e.rel, &e.from, &e.to));
                        if out.len() >= 300 {
                            return Ok(out);
                        }
                    }
                    if seen_nodes.insert(dst.to_string()) {
                        next.insert(dst.to_string());
                    }
                }
            } else {
                for (edge_index, e) in self.store.edges.iter().enumerate() {
                    if e.rel != edge_rel {
                        continue;
                    }
                    let (src, dst) = if reverse {
                        (e.to.as_str(), e.from.as_str())
                    } else {
                        (e.from.as_str(), e.to.as_str())
                    };
                    if !frontier.contains(src) {
                        continue;
                    }
                    let _ = edge_index;
                    let key = (e.rel.clone(), e.from.clone(), e.to.clone());
                    if seen_edges.insert(key) {
                        out.push(edge_row(&e.rel, &e.from, &e.to));
                        if out.len() >= 300 {
                            return Ok(out);
                        }
                    }
                    if seen_nodes.insert(dst.to_string()) {
                        next.insert(dst.to_string());
                    }
                }
            }
            if seen_nodes.len() > self.quotas.max_graph_expansion {
                return Err(Error::runtime(format!(
                    "graph expansion count {} exceeds max_graph_expansion quota {}",
                    seen_nodes.len(),
                    self.quotas.max_graph_expansion
                )));
            }
            frontier = next;
            if frontier.is_empty() {
                break;
            }
        }
        Ok(out)
    }

    /// Expand `match` hops; bound nodes as `bind.field`, optional edge as `e.rel|from|to`.
    fn match_path(
        &self,
        primary: &str,
        rows: Vec<Row>,
        start: Option<&str>,
        hops: &[MatchHop],
    ) -> Result<Vec<Row>, Error> {
        let mut cur = Vec::with_capacity(rows.len());
        for row in rows {
            let mut r = row.clone();
            if let Some(alias) = start {
                for (k, v) in &row {
                    r.insert(format!("{alias}.{k}"), v.clone());
                }
            }
            cur.push(r);
        }
        let mut prev_bind = start;
        for hop in hops {
            let mut next_rows = Vec::new();
            for row in &cur {
                self.match_expand_hop(primary, row, prev_bind, hop, &mut next_rows)?;
                if next_rows.len() >= 300 {
                    return Ok(next_rows);
                }
            }
            cur = next_rows;
            prev_bind = Some(hop.bind.as_str());
            if cur.is_empty() {
                break;
            }
        }
        Ok(cur)
    }

    fn match_expand_hop(
        &self,
        primary: &str,
        row: &Row,
        prev_bind: Option<&str>,
        hop: &MatchHop,
        out: &mut Vec<Row>,
    ) -> Result<(), Error> {
        let (edge_rel, rev_catalog) = match self.catalog.rel(&hop.rel) {
            Some(r) if let Some(of) = &r.reverse_of => (of.as_str(), true),
            _ => (hop.rel.as_str(), false),
        };
        let reverse = hop.reverse ^ rev_catalog;
        let keys = match_frontier_keys(row, prev_bind);
        if keys.is_empty() {
            return Ok(());
        }
        #[cfg(feature = "gpu")]
        if self.gpu.is_some() {
            return self.gpu_match_expand(primary, row, hop, &keys, edge_rel, reverse, out);
        }
        for start_key in keys {
            type GraphSearchFrame = (
                String,
                i64,
                BTreeSet<String>,
                Option<(String, String, String)>,
            );
            let mut stack: Vec<GraphSearchFrame> = vec![(start_key, 0, BTreeSet::new(), None)];
            let mut expansions = 0;
            while let Some((at, depth, path, last_edge)) = stack.pop() {
                expansions += 1;
                if expansions > self.quotas.max_graph_expansion {
                    return Err(Error::runtime(format!(
                        "graph expansion count {} exceeds max_graph_expansion quota {}",
                        expansions, self.quotas.max_graph_expansion
                    )));
                }
                if depth > 0
                    && depth >= hop.min_depth
                    && depth <= hop.max_depth
                    && let Some(node) = self.resolve_node(primary, &at)
                {
                    let mut merged = row.clone();
                    for (k, v) in &node {
                        merged.insert(format!("{}.{k}", hop.bind), v.clone());
                    }
                    if let (Some(e_bind), Some((r, f, t))) = (&hop.edge, &last_edge) {
                        merged.insert(format!("{e_bind}.rel"), Cell::text_arc(r.as_str()));
                        merged.insert(format!("{e_bind}.from"), Cell::text_arc(f.as_str()));
                        merged.insert(format!("{e_bind}.to"), Cell::text_arc(t.as_str()));
                    }
                    out.push(merged);
                    if out.len() >= 300 {
                        return Ok(());
                    }
                }
                if depth >= hop.max_depth {
                    continue;
                }
                let mut visited = path;
                visited.insert(at.clone());
                let gpu_mask =
                    self.graph_frontier_mask(edge_rel, reverse, &BTreeSet::from([at.clone()]))?;
                if let Some(mask) = &gpu_mask {
                    for (edge_index, e) in self.store.edges.iter().enumerate() {
                        if e.rel != edge_rel || !mask[edge_index] {
                            continue;
                        }
                        let dst = if reverse {
                            e.from.as_str()
                        } else {
                            e.to.as_str()
                        };
                        if visited.contains(dst) {
                            continue;
                        }
                        stack.push((
                            dst.to_string(),
                            depth + 1,
                            visited.clone(),
                            Some((e.rel.clone(), e.from.clone(), e.to.clone())),
                        ));
                    }
                } else {
                    self.store.for_each_neighbor(edge_rel, &at, reverse, |dst| {
                        if visited.contains(dst) {
                            return;
                        }
                        let (from, to) = if reverse {
                            (dst.to_string(), at.clone())
                        } else {
                            (at.clone(), dst.to_string())
                        };
                        stack.push((
                            dst.to_string(),
                            depth + 1,
                            visited.clone(),
                            Some((edge_rel.to_string(), from, to)),
                        ));
                    });
                }
            }
        }
        Ok(())
    }

    fn resolve_node(&self, primary: &str, key: &str) -> Option<Row> {
        self.resolve_node_ref(primary, key).cloned()
    }
    fn resolve_node_ref(&self, primary: &str, key: &str) -> Option<&Row> {
        if let Some(row) = self.store.find_doc_key(key) {
            return Some(row);
        }
        if primary != "docs" {
            if let Some(row) = self.store.get_by_id(primary, key) {
                return Some(row);
            }
            return self
                .store
                .collection(primary)
                .iter()
                .find(|r| row_text(r, "id") == Some(key));
        }
        None
    }

    fn search_rows(&self, rows: &[Row], mode: SearchMode, query: &str) -> Result<Vec<Row>, Error> {
        if matches!(mode, SearchMode::Vec | SearchMode::Hybrid)
            && self.store.embed_id != self.catalog.embed_id
        {
            return Err(Error::runtime(format!(
                "embed_id mismatch: store={} catalog={}",
                self.store.embed_id, self.catalog.embed_id
            )));
        }
        match mode {
            SearchMode::Lex => {
                let query = LexQuery::new(query);
                let gpu_scores = self.gpu_lex_scores(rows.iter(), &query)?;
                #[cfg(feature = "gpu")]
                if let Some(scores) = &gpu_scores {
                    let positions = self.gpu.as_ref().expect("GPU scores").rank_i64(scores)?;
                    return Ok(positions
                        .into_iter()
                        .filter(|&i| scores[i] > 0)
                        .map(|i| rows[i].clone())
                        .collect());
                }
                let mut scored: Vec<(i64, Row)> = rows
                    .iter()
                    .enumerate()
                    .filter_map(|(i, r)| {
                        let s = gpu_scores
                            .as_ref()
                            .map_or_else(|| lex_score_prepared(r, &query), |scores| scores[i]);
                        if s > 0 { Some((s, r.clone())) } else { None }
                    })
                    .collect();
                scored.sort_by_key(|a| std::cmp::Reverse(a.0));
                Ok(scored.into_iter().map(|(_, r)| r).collect())
            }
            SearchMode::Vec => Ok(self
                .vector_search_indices(rows, query, None)?
                .into_iter()
                .map(|(_, i)| rows[i].clone())
                .collect()),

            SearchMode::Hybrid => {
                // Reciprocal rank fusion over lex + vec lists.
                const RRF_K: f64 = 60.0;
                let lex = self.search_rows(rows, SearchMode::Lex, query)?;
                let vec = self.search_rows(rows, SearchMode::Vec, query)?;
                #[cfg(feature = "gpu")]
                if let Some(gpu) = &self.gpu {
                    let key = |row: &Row| {
                        row_text(row, "id")
                            .or_else(|| row_text(row, "uri"))
                            .unwrap_or("")
                            .to_owned()
                    };
                    let lex_keys: Vec<String> = lex.iter().map(key).collect();
                    let vec_keys: Vec<String> = vec.iter().map(key).collect();
                    let mut by_id: BTreeMap<String, Row> = BTreeMap::new();
                    for (id, row) in lex_keys.iter().zip(&lex).chain(vec_keys.iter().zip(&vec)) {
                        by_id.entry(id.clone()).or_insert_with(|| row.clone());
                    }
                    return Ok(gpu_rrf_keys(gpu, &lex_keys, &vec_keys)?
                        .into_iter()
                        .filter_map(|id| by_id.remove(&id))
                        .collect());
                }
                let mut scores: BTreeMap<String, f64> = BTreeMap::new();
                let mut by_id: BTreeMap<String, Row> = BTreeMap::new();
                for (rank, r) in lex.iter().enumerate() {
                    let id = row_text(r, "id")
                        .or_else(|| row_text(r, "uri"))
                        .unwrap_or("")
                        .to_string();
                    *scores.entry(id.clone()).or_default() += 1.0 / (RRF_K + rank as f64 + 1.0);
                    by_id.entry(id).or_insert_with(|| r.clone());
                }
                for (rank, r) in vec.iter().enumerate() {
                    let id = row_text(r, "id")
                        .or_else(|| row_text(r, "uri"))
                        .unwrap_or("")
                        .to_string();
                    *scores.entry(id.clone()).or_default() += 1.0 / (RRF_K + rank as f64 + 1.0);
                    by_id.entry(id).or_insert_with(|| r.clone());
                }
                let mut ranked: Vec<(f64, Row)> = scores
                    .into_iter()
                    .filter_map(|(id, s)| by_id.remove(&id).map(|r| (s, r)))
                    .collect();
                ranked.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
                Ok(ranked.into_iter().map(|(_, r)| r).collect())
            }
        }
    }

    /// Recompute `embedding` for every row in a collection that has a vec field.
    pub fn reembed_collection(&mut self, collection: &str) -> Result<usize, Error> {
        let Some(emb) = self.embedder.clone() else {
            return Err(Error::runtime(format!("reembed {collection}: no embedder")));
        };
        if self
            .catalog
            .collection(collection)
            .is_none_or(|c| !c.has_vec())
        {
            return Err(Error::runtime(format!(
                "reembed requires embedding: {collection}"
            )));
        }
        self.store.embed_id = emb.id().to_string();
        self.catalog.embed_id = emb.id().to_string();
        let rows = self.store.collection_mut(collection);
        let mut n = 0usize;
        for row in rows.iter_mut() {
            let text = embed::row_embed_text(row);
            if text.is_empty() {
                row.insert("embedding".into(), Cell::Null);
                continue;
            }
            row.insert("embedding".into(), Cell::Vec(emb.embed(&text)));
            n += 1;
        }
        self.store.rebuild_row_maps();
        if self.store.hnsw.contains_key(collection) {
            self.store.build_hnsw(collection, 16, 64);
        }
        self.plan_cache.clear();
        self.query_cache.clear();
        Ok(n)
    }

    fn maybe_embed_rows(&self, collection: &str, rows: &mut [Row]) {
        let Some(emb) = self.embedder.as_ref() else {
            return;
        };
        if self
            .catalog
            .collection(collection)
            .is_none_or(|c| !c.has_vec())
        {
            return;
        }

        let first_needs = rows.first().is_some_and(|r| {
            r.get("embedding").and_then(Cell::as_vec).is_none()
                && (r.get("title")
                    .or_else(|| r.get("body"))
                    .or_else(|| r.get("snippet"))
                    .and_then(Cell::text)
                    .is_some_and(|s| !s.is_empty()))
        });
        let all_need = rows.len() >= 64
            && first_needs
            && rows.iter().all(|r| {
                r.get("embedding").and_then(Cell::as_vec).is_none()
                    && (r.get("title")
                        .or_else(|| r.get("body"))
                        .or_else(|| r.get("snippet"))
                        .and_then(Cell::text)
                        .is_some_and(|s| !s.is_empty()))
            });
        if all_need && emb.embed_rows_in_place(rows) {
            return;
        }
        let mut row_idxs = Vec::with_capacity(rows.len());
        let mut row_ptrs = Vec::with_capacity(rows.len());
        if all_need {
            row_idxs.extend(0..rows.len());
            row_ptrs.extend(rows.iter());
        } else {
            for (i, row) in rows.iter().enumerate() {
                if row.get("embedding").and_then(Cell::as_vec).is_some() {
                    continue;
                }
                let has_text = row
                    .get("title")
                    .or_else(|| row.get("body"))
                    .or_else(|| row.get("snippet"))
                    .and_then(Cell::text)
                    .is_some_and(|s| !s.is_empty());
                if has_text {
                    row_idxs.push(i);
                    row_ptrs.push(row);
                }
            }
        }
        if row_ptrs.is_empty() {
            return;
        }
        let mut vectors = emb.embed_rows(&row_ptrs);
        if vectors.len() != row_idxs.len() {
            // Keep third-party embedders with a broken batch implementation
            // from silently leaving a partially embedded slab.
            vectors = row_ptrs
                .iter()
                .map(|r| emb.embed(&embed::row_embed_text(r)))
                .collect();
        }
        for (i, vector) in row_idxs.into_iter().zip(vectors) {
            if let Some(cell) = rows[i].get_mut("embedding") {
                *cell = Cell::Vec(vector);
            } else {
                rows[i].insert("embedding".into(), Cell::Vec(vector));
            }
        }
    }

    fn append_edge(&mut self, rel: &str, from: &Value, to: &Value) -> Result<(Row, bool), Error> {
        let now = now_ms();
        let from = value_text(from, now);
        let to = value_text(to, now);
        let changed = self.store.append_edge_parts(rel, &from, &to);
        Ok((edge_row(rel, &from, &to), changed))
    }

    fn check_row_fks(&self, collection: &str, row: &Row) -> Result<(), Error> {
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

    fn fk_target_exists(&self, to_col: &str, to_field: &str, val: &str) -> bool {
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

    fn insert_bulk(
        &mut self,
        collection: &str,
        records: &[Record],
        edges: &[InsertEdge],
        allow_preencode: bool,
    ) -> Result<(Vec<Row>, Vec<Edge>, Option<crate::persist::PreparedInsert>), Error> {
        let now = now_ms();
        let n = records.len();
        let needs_embed = self.embedder.is_some()
            && self
                .catalog
                .collection(collection)
                .is_some_and(|c| c.has_vec());
        let hashing_dim = if needs_embed {
            self.embedder.as_ref().and_then(|e| e.hashing_dim())
        } else {
            None
        };
        let (mut built, all_uniform, all_embedded) =
            build_insert_rows(records, now, needs_embed, hashing_dim);

        let col_len = self.store.collection(collection).len();
        let store_empty = col_len == 0;
        let has_fks = self.catalog.fks.iter().any(|fk| fk.from_col == collection);
        let is_docs = collection == "docs";
        let needs_id = records
            .first()
            .map_or(true, |r| !r.fields.iter().any(|(k, _)| k == "id"));

        let first_record = records.first();
        let id_rec_pos = first_record.and_then(|r| r.fields.iter().position(|(k, _)| k == "id"));
        let uri_rec_pos = if is_docs {
            first_record.and_then(|r| r.fields.iter().position(|(k, _)| k == "uri"))
        } else {
            None
        };

        let fast_checked = if all_uniform
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
            let mut batch_ids: rustc_hash::FxHashSet<&str> = rustc_hash::FxHashSet::default();
            batch_ids.reserve(n);
            let mut batch_uris: rustc_hash::FxHashSet<&str> = rustc_hash::FxHashSet::default();
            if is_docs {
                batch_uris.reserve(n);
            }
            for row in &mut built {
                if needs_id && row.get("id").and_then(Cell::text).is_none() {
                    row.insert("id".into(), Cell::text_arc(self.store.alloc_id()));
                }
                if row.get("hash").and_then(Cell::text).is_none()
                    && let Some(body) = row.get("body").and_then(Cell::text_shared)
                {
                    // Large short-body batches bypass the cache; small batches retain reuse.
                    row.insert(
                        "hash".into(),
                        Cell::Text(if n >= 512 && body.len() <= 64 {
                            content_hash_arc(body.as_ref())
                        } else {
                            cached_content_hash(body.as_ref())
                        }),
                    );
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
        if let Some(first) = built.first() {
            let from_id = row_text(first, "id").unwrap_or("");
            self.store.edges.reserve(edges.len());
            self.store.edge_keys.reserve(edges.len());
            for e in edges {
                let to = match &e.target {
                    EdgeTarget::Page(uri) => self
                        .store
                        .find_doc_key(uri)
                        .and_then(|r| row_text(r, "id"))
                        .map(|s| s.to_string())
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
        let staged = if allow_preencode
            && (512..=16384).contains(&n)
            && edges.is_empty()
            && self.persist.is_some()
            // A conservative O(1) hint avoids scanning every small serial slab.
            // Mixed dimensions may stay serial; the exact budget below remains
            // authoritative before any worker is started.
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
            let shared = std::sync::Arc::new(std::mem::take(&mut built));
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
            built = std::sync::Arc::try_unwrap(shared).expect("encoder retained input after reply");
            maintenance?;
            Some(staged)
        } else {
            self.maintain_insert_slab(collection, start, &built)?;
            None
        };
        let want_rows = built.len() <= 128;
        if want_rows {
            self.store
                .collection_mut(collection)
                .extend(built.iter().cloned());
            Ok((built, new_edges, staged))
        } else {
            // Large bulk: move into store, elide Handle.rows (done.n from pack).
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
                return self.store.point_index(collection, field, key).into_iter().collect();
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
        self.store
            .collection(collection)
            .iter()
            .enumerate()
            .filter_map(|(i, row)| pred.is_none_or(|p| eval_pred(p, row, now)).then_some(i))
            .collect()
    }

    fn update_cas(
        &mut self,
        collection: &str,
        pred: Option<&Pred>,
        cas: Option<&str>,
        cas_each: bool,
        record: &Record,
        ctx: &mut PackCtx,
    ) -> Result<Vec<Row>, Error> {
        let now = now_ms();
        if !cas_each && cas.is_none() {
            return Err(Error::runtime("update requires cas"));
        }
        let idxs = self.mutation_positions(collection, pred, now);
        let rows = self.store.collection(collection);
        if idxs.is_empty() {
            return Err(Error::runtime("update: no matching row"));
        }
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
        let has_body = record.fields.iter().any(|(k, _)| k == "body");
        let has_guards = !self.catalog.guards.is_empty();
        let mut out = Vec::with_capacity(idxs.len());
        for i in idxs {
            let before = self.store.collection(collection)[i].clone();
            if has_guards {
                for (k, _) in &record.fields {
                    for g in &self.catalog.guards {
                        if g.collection == collection
                            && &g.field == k
                            && eval_pred(&g.pred, &before, now)
                        {
                            return Err(Error::runtime(format!("immutable field: {collection}.{k}")));
                        }
                    }
                }
            }
            let mut updated = before.clone();
            // Apply patch with zero-copy Arc clones where possible
            for (k, v) in &record.fields {
                updated.insert(k.clone(), value_cell(v, now));
            }
            if has_body
                && let Some(body) = row_text(&updated, "body").map(str::to_string)
            {
                // Use precomputed hash from cache to avoid repeated FNV-1a computations
                updated.insert("hash".into(), Cell::Text(cached_content_hash(&body)));
            }
            self.check_row_fks(collection, &updated)?;
            if let Undo::Rows(events) | Undo::Mixed { events, .. } = ctx.undo {
                events.push(RowUndo::Update(collection.to_string(), i, before.clone()));
            }
            // Clone before storing - we need both versions for sync operations
            self.store.collection_mut(collection)[i] = updated.clone();
            self.store.index_update_row(collection, i, &before, &updated)?;
            self.store.fts_sync_at(collection, i, &before);
            if let Some(hnsw) = self.store.hnsw.get_mut(collection) {
                if let Some(Cell::Vec(v)) = updated.get("embedding") {
                    hnsw.insert(i, v.clone());
                } else if before.contains_key("embedding") {
                    hnsw.remove(i);
                }
            }
            self.store
                .row_maps_sync_updated(collection, i, &before, &updated);
            out.push(updated);
        }
        mark_written(ctx, collection, &out);
        Ok(out)
    }

    fn delete_rows(
        &mut self,
        collection: &str,
        pred: &Pred,
        cas: Option<&str>,
        cas_each: bool,
        ctx: &mut PackCtx,
    ) -> Result<Vec<Row>, Error> {
        let now = now_ms();
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
        let out = self.store.delete_rows_at(collection, &idxs);

        if let Undo::Rows(events) | Undo::Mixed { events, .. } = ctx.undo {
            events.push(RowUndo::Delete(
                collection.to_string(),
                idxs.to_vec(),
                out.clone(),
            ));
        }
        mark_written(ctx, collection, &out);
        Ok(out)
    }

    fn delete_edge(&mut self, rel: &str, from: &Value, to: &Value) -> Result<Row, Error> {
        let now = now_ms();
        let from = value_text(from, now);
        let to = value_text(to, now);
        if !self.store.remove_edge(rel, &from, &to) {
            return Err(Error::runtime("delete: edge not found"));
        }
        Ok(edge_row(rel, &from, &to))
    }

    fn exec_decl(&mut self, d: &Decl) -> Result<StmtOut, Error> {
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
                Ok((Vec::new(), Some(format!("col {name}")), Some(pack)))
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
                Ok((Vec::new(), Some(format!("rel {name}")), Some(pack)))
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
                    Some(pack),
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
                    Some(pack),
                ))
            }
            Decl::Fk { .. } | Decl::Guard { .. } => {
                Ok((Vec::new(), Some("schema: session only".into()), None))
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
                self.plan_cache.clear();
                self.query_cache.clear();
                let msg = if src.is_empty() {
                    format!("unfilter {collection}")
                } else {
                    format!("filter {collection} {src}")
                };
                Ok((Vec::new(), Some(msg), Some(pack)))
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
                Ok((Vec::new(), Some(format!("owned {name}")), Some(pack)))
            }
        }
    }
}

fn cas_each_ok(collection: &str, row: &Row, got: &str, ctx: &PackCtx) -> Result<(), Error> {
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

pub(crate) enum Undo {
    None,
    Append(crate::store::AppendMark),
    Full(crate::store::MemBackup),
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

fn stmt_writes(s: &Stmt) -> bool {
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
        | Stmt::Update { .. }
        | Stmt::Delete { .. }
        | Stmt::DeleteEdge { .. }
        | Stmt::Reembed { .. }
        | Stmt::Decl(_) => true,
    }
}

fn stmt_append_only(s: &Stmt) -> bool {
    matches!(
        s,
        Stmt::AppendFacts { .. } | Stmt::AppendEdges { .. } | Stmt::Insert { .. }
    )
}

fn merge_join_row(mut left: Row, right: &Row, right_col: &str) -> Row {
    for (k, v) in right {
        left.insert(format!("{right_col}.{k}"), v.clone_for_borrow());
    }
    left
}

fn push_project_batch_row(batch: &mut RecordBatch, row: &Row, names: &[String]) {
    for (col_i, name) in names.iter().enumerate() {
        batch.cols[col_i].push(
            row.get(name)
                .map(Cell::clone_for_borrow)
                .unwrap_or(Cell::Null),
        );
    }
}

fn push_join_batch_row(
    batch: &mut RecordBatch,
    left: &Row,
    right: Option<&[Cell]>,
    plan: &[JoinFieldPlan],
) {
    for (col_i, p) in plan.iter().enumerate() {
        let cell = match p {
            JoinFieldPlan::Left(k) => left
                .get(k)
                .map(Cell::clone_for_borrow)
                .unwrap_or(Cell::Null),
            JoinFieldPlan::Right { out: _, idx } => right
                .and_then(|r| r.get(*idx).map(Cell::clone_for_borrow))
                .unwrap_or(Cell::Null),
        };
        batch.cols[col_i].push(cell);
    }
}

fn finish_batch(batch: &mut RecordBatch, skip_n: usize, explicit_take: Option<Option<i64>>) {
    let n = batch.n();
    if skip_n >= n {
        batch.truncate(0);
    } else if skip_n > 0 {
        batch.drain_prefix(skip_n);
    }
    match explicit_take {
        Some(Some(t)) => {
            let t = t.max(0) as usize;
            if batch.n() > t {
                batch.truncate(t);
            }
        }
        Some(None) => {}
        None if batch.n() > 50 => batch.truncate(50),
        None => {}
    }
}

/// `orders | total > N` — used by SoA join / cursor hot paths.
pub(crate) fn pred_total_gt(pred: &Pred) -> Option<f64> {
    match pred {
        Pred::Cmp {
            field,
            op: CmpOp::Gt,
            value,
        } if field.leaf() == Some("total") => match value {
            Value::Float(n) => Some(*n),
            Value::Int(n) => Some(*n as f64),
            _ => None,
        },
        _ => None,
    }
}

fn rows_to_rough_batch(rows: &[Row]) -> RecordBatch {
    if rows.is_empty() {
        return RecordBatch::empty(Arc::<[String]>::from(Vec::<String>::new()));
    }
    let names: Vec<String> = rows[0].keys().cloned().collect();
    let names_arc: Arc<[String]> = names.clone().into();
    let mut batch = RecordBatch::with_capacity(names_arc, rows.len());
    for row in rows {
        for (i, name) in names.iter().enumerate() {
            batch.cols[i].push(row.get(name).cloned().unwrap_or(Cell::Null));
        }
    }
    batch
}

/// Output field for a projected FK join: left column or right probe slot.
#[derive(Debug, Clone)]
pub(crate) enum JoinFieldPlan {
    Left(String),
    Right { out: String, idx: usize },
}

pub(crate) fn plan_join_fields(
    right_col: &str,
    names: &[String],
) -> (Vec<String>, Vec<JoinFieldPlan>) {
    let mut right_fields = Vec::new();
    let mut plan = Vec::with_capacity(names.len());
    for f in names {
        if let Some(rest) = f.strip_prefix(right_col).and_then(|s| s.strip_prefix('.')) {
            let idx = if let Some(i) = right_fields.iter().position(|x| x == rest) {
                i
            } else {
                right_fields.push(rest.to_string());
                right_fields.len() - 1
            };
            plan.push(JoinFieldPlan::Right {
                out: f.clone(),
                idx,
            });
        } else {
            plan.push(JoinFieldPlan::Left(f.clone()));
        }
    }
    (right_fields, plan)
}

pub(crate) fn build_right_probe<'a>(
    rights: &'a [Row],
    right_fields: &[String],
) -> FxHashMap<&'a str, Vec<Cell>> {
    let mut probe = FxHashMap::default();
    probe.reserve(rights.len());
    for r in rights {
        let Some(id) = r.get("id").and_then(Cell::text) else {
            continue;
        };
        let cells = right_fields
            .iter()
            .map(|f| r.get(f).cloned().unwrap_or(Cell::Null))
            .collect();
        probe.insert(id, cells);
    }
    probe
}

pub(crate) fn emit_join_row(left: &Row, right: Option<&[Cell]>, plan: &[JoinFieldPlan]) -> Row {
    let mut out = crate::store::acquire_row();
    for p in plan {
        match p {
            JoinFieldPlan::Left(k) => {
                // Zero-copy: use clone_for_borrow instead of full Row.clone()
                let cell = left
                    .get(k)
                    .map(|c| c.clone_for_borrow())
                    .unwrap_or(Cell::Null);
                out.insert(k.clone(), cell);
            }
            JoinFieldPlan::Right { out: k, idx } => {
                // Zero-copy borrow from right cells array
                let cell = right
                    .and_then(|r| r.get(*idx))
                    .map(|c| c.clone_for_borrow())
                    .unwrap_or(Cell::Null);
                out.insert(k.clone(), cell);
            }
        }
    }
    out
}

/// Project join output without building a full merged left∪right map.
pub(crate) fn project_join_fields(
    left: &Row,
    right: Option<&Row>,
    right_col: &str,
    fields: &[String],
) -> Row {
    let mut out = BTreeMap::new();
    for f in fields {
        let cell = if let Some(rest) = f.strip_prefix(right_col).and_then(|s| s.strip_prefix('.')) {
            right
                .and_then(|r| r.get(rest))
                .cloned()
                .unwrap_or(Cell::Null)
        } else {
            left.get(f).cloned().unwrap_or(Cell::Null)
        };
        out.insert(f.clone(), cell);
    }
    out
}

enum JoinLeftSource {
    Idxs(Vec<usize>),
    Scan { len: usize },
}

fn stmt_schema(s: &Stmt) -> bool {
    matches!(s, Stmt::Decl(_))
}

pub(crate) fn field_names(fields: &[Field]) -> Vec<String> {
    fields.iter().map(|f| f.as_str()).collect()
}

fn pack_index_label(collection: &str, fields: &[String]) -> String {
    format!("{collection}[{}]", fields.join(","))
}

fn pack_affect_n(pack: Option<&Pack>) -> Option<usize> {
    match pack? {
        Pack::AppendFactsBulk { s, .. } => Some(s.len()),
        Pack::AppendEdgesBulk { rel, .. } => Some(rel.len()),
        Pack::InsertCols { n, .. } => Some(*n as usize),
        Pack::InsertBulk { rows, .. } => Some(rows.len()),
        Pack::Insert { .. } => Some(1),
        Pack::AppendFact { .. } | Pack::AppendEdge { .. } | Pack::DeleteEdge { .. } => Some(1),
        Pack::Update { rows, .. } | Pack::Delete { rows, .. } => Some(rows.len()),
        Pack::Batch { packs } => Some(packs.iter().filter_map(|p| pack_affect_n(Some(p))).sum()),
        _ => None,
    }
}

fn fold_packs(packs: Vec<Pack>) -> Option<Pack> {
    match packs.len() {
        0 => None,
        1 => packs.into_iter().next(),
        2.. => Some(Pack::Batch { packs }),
    }
}

fn meta_row(kind: &str, name: &str) -> Row {
    let mut r = BTreeMap::new();
    r.insert("kind".into(), Cell::text_arc(kind));
    r.insert("name".into(), Cell::text_arc(name));
    r
}

fn edge_row(rel: &str, from: &str, to: &str) -> Row {
    let mut r = BTreeMap::new();
    r.insert("rel".into(), Cell::text_arc(rel));
    r.insert("from".into(), Cell::text_arc(from));
    r.insert("to".into(), Cell::text_arc(to));
    r
}

fn match_frontier_keys(row: &Row, bind: Option<&str>) -> Vec<String> {
    let mut keys = Vec::new();
    let (id_k, uri_k) = match bind {
        Some(b) => (format!("{b}.id"), format!("{b}.uri")),
        None => ("id".into(), "uri".into()),
    };
    if let Some(id) = row_text(row, &id_k) {
        keys.push(id.to_string());
    }
    if let Some(uri) = row_text(row, &uri_k) {
        keys.push(uri.to_string());
    }
    keys
}

#[cfg(test)]
fn append_bulk_worker_rows(
    rows: &mut Vec<Row>,
    records: &[Record],
    now: i64,
    worker: std::io::Result<std::thread::ScopedJoinHandle<'_, Vec<Row>>>,
) {
    match worker {
        Ok(worker) => rows.extend(worker.join().expect("row construction worker panicked")),
        Err(_) => {
            // Parallelism is optional: keep inserts available when the OS
            // cannot allocate an additional worker thread.
            rows.extend(records.iter().map(|record| record_row(record, now)));
        }
    }
}

thread_local! {
    static BATCH_DEDUP_SETS: std::cell::RefCell<(
        rustc_hash::FxHashSet<&'static str>,
        rustc_hash::FxHashSet<&'static str>,
    )> = std::cell::RefCell::new((
        rustc_hash::FxHashSet::default(),
        rustc_hash::FxHashSet::default(),
    ));
}

struct DedupSetsClearGuard<'a> {
    guard: std::cell::RefMut<
        'a,
        (
            rustc_hash::FxHashSet<&'static str>,
            rustc_hash::FxHashSet<&'static str>,
        ),
    >,
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
    existing_ids: Option<&rustc_hash::FxHashMap<Arc<str>, usize>>,
    docs_by_uri: &rustc_hash::FxHashMap<Arc<str>, usize>,
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

struct BuildRowsJob {
    records_ptr: usize,
    records_len: usize,
    out_ptr: usize,
    now: i64,
    needs_embed: bool,
    hashing_dim: Option<usize>,
    total_n: usize,
}

struct BuildRowsPool {
    workers: Vec<std::sync::mpsc::SyncSender<BuildRowsJob>>,
    done_rx: std::sync::mpsc::Receiver<(bool, bool)>,
}

fn build_insert_rows(
    records: &[Record],
    now: i64,
    needs_embed: bool,
    hashing_dim: Option<usize>,
) -> (Vec<Row>, bool, bool) {
    let n = records.len();
    if n == 0 {
        return (Vec::new(), false, !needs_embed);
    }
    if n >= 512 {
        static POOL: std::sync::OnceLock<std::sync::Mutex<Option<BuildRowsPool>>> =
            std::sync::OnceLock::new();
        let mutex = POOL.get_or_init(|| {
            let threads = std::thread::available_parallelism()
                .map(|p| p.get())
                .unwrap_or(1)
                .min(4);
            if threads <= 1 {
                return std::sync::Mutex::new(None);
            }
            let num_workers = threads - 1;
            let (done_tx, done_rx) = std::sync::mpsc::sync_channel::<(bool, bool)>(num_workers);
            let mut workers = Vec::with_capacity(num_workers);
            for idx in 0..num_workers {
                let (tx, rx) = std::sync::mpsc::sync_channel::<BuildRowsJob>(1);
                let done = done_tx.clone();
                if std::thread::Builder::new()
                    .name(format!("lin-build-{idx}"))
                    .spawn(move || {
                        while let Ok(job) = rx.recv() {
                            let recs = unsafe {
                                std::slice::from_raw_parts(
                                    job.records_ptr as *const Record,
                                    job.records_len,
                                )
                            };
                            let out = unsafe {
                                std::slice::from_raw_parts_mut(
                                    job.out_ptr as *mut std::mem::MaybeUninit<Row>,
                                    job.records_len,
                                )
                            };
                            let res = build_batch_shape_rows_into(
                                out,
                                recs,
                                job.now,
                                job.needs_embed,
                                job.hashing_dim,
                                job.total_n,
                            );
                            let _ = done.send(res);
                        }
                    })
                    .is_ok()
                {
                    workers.push(tx);
                }
            }
            if workers.is_empty() {
                std::sync::Mutex::new(None)
            } else {
                std::sync::Mutex::new(Some(BuildRowsPool { workers, done_rx }))
            }
        });

        if let Ok(guard) = mutex.try_lock()
            && let Some(pool) = guard.as_ref()
        {
            let total_ways = (pool.workers.len() + 1).min((n + 255) / 256).max(1);
            if total_ways > 1 {
                let mut rows: Vec<Row> = Vec::with_capacity(n);
                let spare = &mut rows.spare_capacity_mut()[..n];
                let chunk_size = (n + total_ways - 1) / total_ways;
                let mut rec_chunks = records.chunks(chunk_size);
                let mut out_chunks = spare.chunks_mut(chunk_size);
                let first_recs = rec_chunks.next().unwrap();
                let first_out = out_chunks.next().unwrap();
                struct DispatchedDrainGuard<'a> {
                    done_rx: &'a std::sync::mpsc::Receiver<(bool, bool)>,
                    pending: usize,
                }
                impl Drop for DispatchedDrainGuard<'_> {
                    fn drop(&mut self) {
                        for _ in 0..self.pending {
                            let _ = self.done_rx.recv();
                        }
                    }
                }
                let mut drain_guard = DispatchedDrainGuard {
                    done_rx: &pool.done_rx,
                    pending: 0,
                };
                let mut fallback_start = first_recs.len();
                for ((worker, r_chunk), o_chunk) in
                    pool.workers.iter().zip(rec_chunks).zip(out_chunks)
                {
                    let job = BuildRowsJob {
                        records_ptr: r_chunk.as_ptr() as usize,
                        records_len: r_chunk.len(),
                        out_ptr: o_chunk.as_mut_ptr() as usize,
                        now,
                        needs_embed,
                        hashing_dim,
                        total_n: n,
                    };
                    if worker.send(job).is_ok() {
                        drain_guard.pending += 1;
                        fallback_start += r_chunk.len();
                    } else {
                        break;
                    }
                }
                let (mut all_uniform, mut all_embedded) = build_batch_shape_rows_into(
                    first_out,
                    first_recs,
                    now,
                    needs_embed,
                    hashing_dim,
                    n,
                );
                if fallback_start < n {
                    let (u, e) = build_batch_shape_rows_into(
                        &mut spare[fallback_start..n],
                        &records[fallback_start..n],
                        now,
                        needs_embed,
                        hashing_dim,
                        n,
                    );
                    all_uniform &= u;
                    all_embedded &= e;
                }
                while drain_guard.pending > 0 {
                    drain_guard.pending -= 1;
                    if let Ok((u, e)) = drain_guard.done_rx.recv() {
                        all_uniform &= u;
                        all_embedded &= e;
                    }
                }
                drop(drain_guard);
                unsafe {
                    rows.set_len(n);
                }
                return (rows, all_uniform, all_embedded);
            }
        }
    }

    let mut rows: Vec<Row> = Vec::with_capacity(n);
    let spare = &mut rows.spare_capacity_mut()[..n];
    let (all_uniform, all_embedded) =
        build_batch_shape_rows_into(spare, records, now, needs_embed, hashing_dim, n);
    unsafe {
        rows.set_len(n);
    }
    (rows, all_uniform, all_embedded)
}

fn build_batch_shape_rows_into(
    out: &mut [std::mem::MaybeUninit<Row>],
    records: &[Record],
    now: i64,
    needs_embed: bool,
    hashing_dim: Option<usize>,
    total_n: usize,
) -> (bool, bool) {
    let Some(first) = records.first() else {
        return (false, !needs_embed);
    };
    if total_n < 128 || first.fields.is_empty() || first.fields.len() > 32 {
        for (slot_out, record) in out.iter_mut().zip(records) {
            slot_out.write(record_row(record, now));
        }
        return (false, !needs_embed);
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
        template.insert("hash".into(), Cell::Null);
    }
    if needs_embed {
        template.insert("embedding".into(), Cell::Null);
    }

    #[derive(Clone, Copy)]
    enum ShapeSlot {
        Source(usize),
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
                ShapeSlot::Source(
                    first
                        .fields
                        .iter()
                        .rposition(|(field, _)| field == key)
                        .unwrap(),
                )
            }
        })
        .collect::<Vec<_>>();
    let mut all_same = true;
    let mut all_embedded = !needs_embed || can_inline_embed;
    for (slot_out, record) in out.iter_mut().zip(records) {
        let same_shape = record.fields.len() == first.fields.len()
            && record
                .fields
                .iter()
                .zip(&first.fields)
                .all(|(a, b)| a.0 == b.0);
        if !same_shape {
            all_same = false;
            if needs_embed {
                all_embedded = false;
            }
            slot_out.write(record_row(record, now));
            continue;
        }
        let mut emb_vec = if let Some(dim) = hashing_dim.filter(|_| can_inline_embed) {
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
                Some(crate::embed::embed_texts_hashing(
                    dim,
                    [t_str, b_str, s_str],
                ))
            } else {
                all_embedded = false;
                None
            }
        } else {
            None
        };
        let mut row = template.clone();
        for (cell, slot) in row.values_mut().zip(&slots) {
            match slot {
                ShapeSlot::Source(src) => {
                    *cell = value_cell(&record.fields[*src].1, now);
                }
                ShapeSlot::Hash(b_src) => {
                    if let Value::String(body) = &record.fields[*b_src].1 {
                        *cell = Cell::Text(if total_n >= 512 && body.len() <= 64 {
                            crate::store::content_hash_arc(body.as_ref())
                        } else {
                            cached_content_hash(body.as_ref())
                        });
                    } else {
                        all_same = false;
                    }
                }
                ShapeSlot::Embed => {
                    if let Some(v) = emb_vec.take() {
                        *cell = Cell::Vec(v);
                    }
                }
            }
        }
        slot_out.write(row);
    }
    (all_same, all_embedded)
}

#[allow(dead_code)]
fn append_batch_shape_rows(
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
    let (all_same, _) = build_batch_shape_rows_into(spare, records, now, needs_embed, None, n);
    unsafe {
        rows.set_len(old_len + n);
    }
    all_same
}

fn record_row(record: &Record, now: i64) -> Row {
    let mut row = BTreeMap::new();
    for (k, v) in &record.fields {
        row.insert(k.clone(), value_cell(v, now));
    }
    row
}

#[cfg(test)]
mod record_tests {
    use super::*;

    #[test]
    fn bulk_short_body_hash_preserves_values_at_batch_and_byte_boundaries() {
        for n in [127, 128, 4095, 4096, 8192] {
            let records = (0..n)
                .map(|i| {
                    let body = match i % 4 {
                        0 => format!("body-{i}"),
                        1 => "repeated body".to_owned(),
                        2 => "λ".repeat(32),
                        _ => "λ".repeat(33),
                    };
                    let mut fields = vec![
                        ("id".into(), Value::String(format!("d-{i}").into())),
                        ("uri".into(), Value::String(format!("bench://{i}").into())),
                        ("title".into(), Value::String("title".into())),
                        ("body".into(), Value::String(body.into())),
                    ];
                    if i % 257 == 0 {
                        fields.push(("hash".into(), Value::String("caller-hash".into())));
                    }
                    Record { fields }
                })
                .collect::<Vec<_>>();
            let mut db = Db::empty();
            db.insert_bulk("docs", &records, &[], false).unwrap();
            let rows = db.store.collection("docs");
            assert_eq!(rows.len(), n);
            for (i, row) in rows.iter().enumerate() {
                let expected = if i % 257 == 0 {
                    Arc::<str>::from("caller-hash")
                } else {
                    content_hash_arc(row["body"].text().unwrap())
                };
                assert_eq!(row["hash"].text(), Some(expected.as_ref()));
            }
        }
    }

    #[test]
    fn batch_shape_matches_reference_and_keeps_source_records() {
        for n in [0, 1, 127, 128, 129, 4095, 4096] {
            let mut records = (0..n)
                .map(|i| Record {
                    fields: vec![
                        ("z".into(), Value::String(format!("v-{i}").into())),
                        ("a".into(), Value::Int(1)),
                        ("t".into(), Value::Now),
                        ("a".into(), Value::Int(i as i64)),
                    ],
                })
                .collect::<Vec<_>>();
            if n > 2 {
                records[n / 2].fields.swap(0, 1);
                records[n - 1]
                    .fields
                    .push(("extra".into(), Value::Bool(true)));
            }
            let before = records.clone();
            for now in [123, 456] {
                let mut actual = vec![Row::from([("prefix".into(), Cell::Int(9))])];
                append_batch_shape_rows(&mut actual, &records, now, false);
                assert_eq!(actual.len(), n + 1);
                assert_eq!(actual[0]["prefix"], Cell::Int(9));
                let expected = records
                    .iter()
                    .map(|r| record_row(r, now))
                    .collect::<Vec<_>>();
                assert_eq!(&actual[1..], expected);
                assert_eq!(records, before);
            }
        }
    }

    #[test]
    fn wide_and_empty_shape_batches_use_reference_semantics() {
        for width in [0, 32, 33, 80] {
            let record = Record {
                fields: (0..width)
                    .map(|i| (format!("f{i}"), Value::Int(i)))
                    .collect(),
            };
            let records = vec![record; 129];
            let mut actual = Vec::new();
            append_batch_shape_rows(&mut actual, &records, 55, false);
            assert_eq!(
                actual,
                records
                    .iter()
                    .map(|r| record_row(r, 55))
                    .collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn bulk_worker_spawn_failure_preserves_tail_and_timestamp() {
        let left = Record {
            fields: vec![("id".into(), Value::String("left".into()))],
        };
        let right = (0..2053)
            .map(|i| Record {
                fields: vec![
                    ("id".into(), Value::String(format!("right-{i}").into())),
                    ("title".into(), Value::String("old".into())),
                    ("title".into(), Value::String(format!("ЁЖ {i}").into())),
                    ("ts".into(), Value::Now),
                ],
            })
            .collect::<Vec<_>>();
        let mut rows = vec![record_row(&left, 123)];
        append_bulk_worker_rows(
            &mut rows,
            &right,
            123,
            Err(std::io::Error::new(
                std::io::ErrorKind::WouldBlock,
                "worker unavailable",
            )),
        );
        assert_eq!(rows.len(), 2054);
        assert_eq!(row_text(&rows[0], "id"), Some("left"));
        for (i, row) in rows[1..].iter().enumerate() {
            assert_eq!(row_text(row, "id"), Some(format!("right-{i}").as_str()));
            assert_eq!(row_text(row, "title"), Some(format!("ЁЖ {i}").as_str()));
            assert_eq!(row.get("ts"), Some(&Cell::Time(123)));
        }
    }

    #[test]
    fn parallel_bulk_rows_preserve_order_generated_ids_and_shared_now() {
        for n in [4095, 4096, 4101] {
            let records = (0..n)
                .map(|i| Record {
                    fields: vec![
                        (
                            "uri".into(),
                            Value::String(format!("parallel://{i}").into()),
                        ),
                        ("title".into(), Value::String("old".into())),
                        ("title".into(), Value::String(format!("ЁЖ {i}").into())),
                        ("body".into(), Value::String(format!("body {i}").into())),
                        ("ts".into(), Value::Now),
                    ],
                })
                .collect::<Vec<_>>();
            let mut db = Db::empty().without_embedder();
            let mut reference = Db::empty().without_embedder();
            let expected_ids = (0..n)
                .map(|_| reference.store.alloc_id())
                .collect::<Vec<_>>();
            let before = now_ms();
            let (returned, edges, _) = db.insert_bulk("docs", &records, &[], false).unwrap();
            let after = now_ms();
            assert!(returned.is_empty());
            assert!(edges.is_empty());
            let rows = db.store.collection("docs");
            assert_eq!(rows.len(), n);
            let ts = rows[0].get("ts").unwrap();
            assert!(matches!(ts, Cell::Time(t) if (before..=after).contains(t)));
            for (i, row) in rows.iter().enumerate() {
                assert_eq!(row_text(row, "id"), Some(expected_ids[i].as_str()));
                assert_eq!(row_text(row, "title"), Some(format!("ЁЖ {i}").as_str()));
                assert_eq!(row.get("ts"), Some(ts));
                assert_eq!(
                    row.get("hash"),
                    Some(&Cell::Text(content_hash_arc(&format!("body {i}"))))
                );
                assert!(db.store.get_by_uri(&format!("parallel://{i}")).is_some());
            }
        }
    }

    #[test]
    fn parallel_bulk_checks_duplicate_keys_across_worker_boundary() {
        for field in ["id", "uri"] {
            let mut records = (0..4096)
                .map(|i| Record {
                    fields: vec![
                        ("id".into(), Value::String(format!("id-{i}").into())),
                        (
                            "uri".into(),
                            Value::String(format!("parallel://{i}").into()),
                        ),
                    ],
                })
                .collect::<Vec<_>>();
            let value = records[0]
                .fields
                .iter()
                .find(|(k, _)| k == field)
                .unwrap()
                .1
                .clone();
            records[2048].fields.push((field.into(), value));
            let mut db = Db::empty().without_embedder();
            let error = db.insert_bulk("docs", &records, &[], false).err().unwrap();
            assert!(error.to_string().contains(&format!("duplicate {field}")));
            assert!(db.store.collection("docs").is_empty());
        }
    }

    #[test]
    fn prepared_insert_keeps_last_duplicate_and_dynamic_time() {
        let source = r#"insert docs {title:"old",ts:now,title:"middle",uri:"prepared://duplicate",title:"new",id:"prepared-id"}"#;
        let mut db = Db::empty();
        let prepared = db.prepare(source).unwrap();
        let Stmt::Insert { records, .. } = &prepared.stmts[0] else {
            panic!("insert")
        };
        for now in [123, 456] {
            let row = record_row(&records[0], now);
            assert_eq!(row_text(&row, "title"), Some("new"));
            assert_eq!(row.get("ts"), Some(&Cell::Time(now)));
        }
        let before = now_ms();
        let handle = prepared.run(&mut db).unwrap();
        let after = now_ms();
        assert_eq!(handle.done.n, 1);
        let row = db.store.get_by_id("docs", "prepared-id").unwrap();
        assert_eq!(row_text(row, "title"), Some("new"));
        let Cell::Time(timestamp) = row["ts"] else {
            panic!("time")
        };
        assert!((before..=after).contains(&timestamp));
    }

    #[test]
    fn prepared_literal_storage_is_shared_and_outlives_source_and_plan() {
        let mut db = Db::empty();
        let source = String::from(
            r#"insert docs {id:"shared",uri:"shared://doc",title:"Shared ЁЖ 🦔",body:"body"}"#,
        );
        let prepared = db.prepare(&source).unwrap();
        drop(source);
        let Stmt::Insert { records, .. } = &prepared.stmts[0] else {
            panic!("insert")
        };
        let Value::String(literal) = &records[0]
            .fields
            .iter()
            .find(|(name, _)| name == "title")
            .unwrap()
            .1
        else {
            panic!("literal")
        };
        let literal = std::sync::Arc::clone(literal);
        assert_eq!(prepared.run(&mut db).unwrap().done.n, 1);
        let Cell::Text(stored) = &db.store.get_by_id("docs", "shared").unwrap()["title"] else {
            panic!("text")
        };
        assert!(std::sync::Arc::ptr_eq(stored, &literal));
        drop(prepared);
        drop(literal);
        assert_eq!(
            row_text(db.store.get_by_uri("shared://doc").unwrap(), "title"),
            Some("Shared ЁЖ 🦔")
        );
    }

    #[test]
    fn lexical_lowercase_preserves_ascii_unicode_context_and_field_phrases() {
        for (title, body, query, expected) in [
            ("wal note", "body", "wal", 6),
            ("WAL NOTE", "BODY", "wal", 6),
            ("ΟΣ", "BODY", "ος", 6),
            ("ΟΣ", "BODY", "οσ", 0),
            ("one", "two", "one two", 9),
        ] {
            let row = Row::from([
                ("title".into(), Cell::text_arc(title)),
                ("body".into(), Cell::text_arc(body)),
            ]);
            assert_eq!(
                lex_score_prepared(&row, &LexQuery::new(query)),
                expected,
                "{title}/{query}"
            );
        }
    }

    #[test]
    fn row_build_preserves_last_duplicate_and_execution_time() {
        let record = Record {
            fields: vec![
                ("z".into(), Value::String("old".into())),
                ("a".into(), Value::Now),
                ("z".into(), Value::String("new".into())),
                ("m".into(), Value::Int(42)),
            ],
        };
        let row = record_row(&record, 123);
        assert_eq!(row.len(), 3);
        assert_eq!(row_text(&row, "z"), Some("new"));
        assert_eq!(row.get("a"), Some(&Cell::Time(123)));
        assert_eq!(record_row(&record, 456).get("a"), Some(&Cell::Time(456)));
    }
}

#[cfg(test)]
mod grouping_tests {
    use crate::Db;
    use crate::store::Cell;

    #[test]
    fn mixed_numeric_and_text_grouping_preserves_cell_identity_across_operators() {
        let mut db = Db::empty();
        db.run("col items { k: f64, v: i64, tag: text }").unwrap();
        db.run(
            r#"insert items [
                { id: "i1", k: 1, v: 10, tag: "active" },
                { id: "i2", k: 1.0, v: 20, tag: "active" },
                { id: "i3", k: 0.0, v: 30, tag: "active" },
                { id: "i4", k: -0.0, v: 40, tag: "active" },
                { id: "i5", v: 50, tag: "active" }
            ]"#,
        )
        .unwrap();

        // Unfiltered count by k (agg_count) and filtered count by k (try_filter_count) must agree
        let unfiltered = db.run("items | count by k").unwrap().into_rows();
        let filtered = db
            .run(r#"items | tag == "active" | count by k"#)
            .unwrap()
            .into_rows();
        assert_eq!(unfiltered, filtered);
        assert_eq!(unfiltered.len(), 5);
        assert_eq!(unfiltered[0]["k"], Cell::Null);

        // Sum by k must keep Int(1) and Float(1.0) in distinct groups
        let sums = db.run("items | sum v by k").unwrap().into_rows();
        assert_eq!(sums.len(), 5);

        // Window PARTITION BY must also use Cell identity without string-formatting collisions
        let win = db.run("items | window row_number by k as rn").unwrap().into_rows();
        assert_eq!(win.len(), 5);
        for row in &win {
            assert_eq!(row["rn"], Cell::Int(1));
        }
    }
}

#[cfg(test)]
mod preencoded_insert_tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    struct Temp(std::path::PathBuf);
    impl Drop for Temp {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.0).unwrap();
        }
    }
    struct Mode(u8);
    impl Mode {
        fn new(mode: u8) -> Self {
            PREENCODE_TEST_ATTEMPTS.with(|x| x.set(0));
            Self(PREENCODE_TEST_MODE.with(|x| x.replace(mode)))
        }
    }
    impl Drop for Mode {
        fn drop(&mut self) {
            PREENCODE_TEST_MODE.with(|x| x.set(self.0));
        }
    }
    fn open() -> (Temp, Db) {
        let p = std::env::temp_dir().join(format!(
            "lin-preencoded-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let db = Db::open(&p).unwrap();
        (Temp(p), db)
    }
    fn source(n: usize) -> String {
        format!("insert docs [{}]",(0..n).map(|i|format!(r#"{{id:"i{i}",uri:"bench://{i}",title:"title {i}",body:"body {i}",wing:"rag",ts:timestamp(1700000000000)}}"#)).collect::<Vec<_>>().join(","))
    }
    #[test]
    fn shared_worker_survives_encoder_panic_and_one_database_closing() {
        let (_first, mut first) = open();
        let (_second, mut second) = open();
        first.run("index docs [wing, ts]").unwrap();
        second.run("index docs [wing, ts]").unwrap();
        {
            let _mode = Mode::new(2);
            first.quotas.max_rows = 0;
            assert!(
                first
                    .run(&source(4096))
                    .unwrap_err()
                    .to_string()
                    .contains("quota:")
            );
        }
        second.run(&source(4096)).unwrap();
        assert!(std::sync::Arc::ptr_eq(
            first.persist.as_ref().unwrap().encoder.as_ref().unwrap(),
            second.persist.as_ref().unwrap().encoder.as_ref().unwrap()
        ));
        drop(first);
        let next = source(4096)
            .replace("id:\"i", "id:\"j")
            .replace("bench://", "repeat://");
        second.run(&next).unwrap();
        assert_eq!(second.store.collection("docs").len(), 8192);
        let read = Db::open_read(&_second.0).unwrap();
        assert_eq!(
            read.run("docs | take 9000").unwrap().rows,
            second.run("docs | take 9000").unwrap().rows
        );
        drop(read);
        drop(second);
    }
    #[test]
    fn worker_spawn_fallback_matches_normal_values_and_readonly_replay() {
        for n in [683, 1000, 4096] {
            for mode in [0, 1, 4, 5] {
                let (_dir, mut db) = open();
                db.run("index docs [wing, ts]").unwrap();
                let _mode = Mode::new(mode);
                let src = source(n);
                assert_eq!(db.run(&src).unwrap().done.n, n);
                assert_eq!(PREENCODE_TEST_ATTEMPTS.with(|x| x.get()), 1);
                let mut expected = Db::empty();
                expected.run("index docs [wing, ts]").unwrap();
                expected.run(&src).unwrap();
                let query = "docs | {id, uri, title, body, wing, ts, hash, embedding} | take 5000";
                let a = db.run(query).unwrap().into_rows();
                let b = expected.run(query).unwrap().into_rows();
                assert_eq!(a.len(), n);
                assert_eq!(a, b);
                let read = Db::open_read(&_dir.0).unwrap();
                assert_eq!(read.run(query).unwrap().into_rows(), a);
                drop(read);
                drop(db);
            }
        }
    }
    #[test]
    fn exact_small_float_budget_can_start_at_512_rows() {
        let (_dir, db) = open();
        let mut db = db.with_embedder(std::sync::Arc::new(crate::embed::HashingEmbedder::new(
            "test/1024",
            1024,
        )));
        let _mode = Mode::new(0);
        db.run(&source(512)).unwrap();
        assert_eq!(PREENCODE_TEST_ATTEMPTS.with(|x| x.get()), 1);
        drop(db);
    }
    #[test]
    fn small_work_hint_is_conservative_for_mixed_vector_dimensions() {
        struct SkewEmbedder {
            first: usize,
            rest: usize,
        }
        fn vector(n: usize) -> Arc<[f32]> {
            let mut values = vec![0.0; n];
            values[0] = 1.0;
            values[1] = -0.0;
            values[2] = f32::from_bits(1);
            values.into()
        }
        impl crate::embed::Embedder for SkewEmbedder {
            fn id(&self) -> &str {
                "test/skew"
            }
            fn dim(&self) -> usize {
                self.rest
            }
            fn embed(&self, _: &str) -> Arc<[f32]> {
                vector(self.rest)
            }
            fn embed_batch(&self, texts: &[&str]) -> Vec<Arc<[f32]>> {
                let first = vector(self.first);
                let rest = vector(self.rest);
                (0..texts.len())
                    .map(|i| if i == 0 { first.clone() } else { rest.clone() })
                    .collect()
            }
        }
        // Hint true but actual total below budget; hint false despite a large
        // total; and a genuinely eligible uniform batch. The hint is routing
        // only and never replaces the exact total used by the worker guard.
        for (first, rest, attempts) in [(1024, 8, 0), (8, 1024, 0), (1024, 1024, 1)] {
            let (_dir, db) = open();
            let mut db = db.with_embedder(Arc::new(SkewEmbedder { first, rest }));
            let _mode = Mode::new(0);
            db.run(&source(683)).unwrap();
            assert_eq!(PREENCODE_TEST_ATTEMPTS.with(|x| x.get()), attempts);
            let verify = |rows: &[Row]| {
                assert_eq!(rows.len(), 683);
                for (i, row) in rows.iter().enumerate() {
                    let values = row["embedding"].as_vec().unwrap();
                    assert_eq!(values.len(), if i == 0 { first } else { rest });
                    assert_eq!(values[0].to_bits(), 1.0f32.to_bits());
                    assert_eq!(values[1].to_bits(), (-0.0f32).to_bits());
                    assert_eq!(values[2].to_bits(), 1);
                    assert!(values[3..].iter().all(|v| v.to_bits() == 0));
                }
            };
            verify(db.store.collection("docs"));
            let read = Db::open_read(&_dir.0).unwrap();
            verify(&read.run("docs | take 1000").unwrap().rows);
            drop(read);
            drop(db);
        }
    }
    #[test]
    fn encoded_errors_defer_until_quota_and_index_checks() {
        for mode in [2, 3] {
            let (_dir, mut db) = open();
            db.run("index docs [wing, ts]").unwrap();
            let _mode = Mode::new(mode);
            db.quotas.max_rows = 0;
            let error = db.run(&source(683)).err().unwrap();
            assert!(error.to_string().contains("quota:"), "{error}");
            assert!(db.store.collection("docs").is_empty());
            assert!(db.store.row_index("docs", "i0").is_none());
            drop(db);
            let (_dir, mut db) = open();
            db.run("index docs unique [wing]").unwrap();
            let _mode = Mode::new(mode);
            let error = db.run(&source(683)).err().unwrap();
            assert!(error.to_string().contains("unique index"), "{error}");
            assert!(db.store.collection("docs").is_empty());
            drop(db);
        }
        let (_dir, mut db) = open();
        db.run("index docs [wing, ts]").unwrap();
        let _mode = Mode::new(3);
        let before = std::fs::read(_dir.0.join("log")).unwrap();
        let error = db.run(&source(683)).err().unwrap();
        assert!(error.to_string().contains("injected WAL encoder failure"));
        assert!(db.store.collection("docs").is_empty());
        assert_eq!(std::fs::read(_dir.0.join("log")).unwrap(), before);
        drop(db);
    }
    #[test]
    fn noneligible_native_small_and_explicit_transactions_stay_serial() {
        let _mode = Mode::new(3);
        let mut memory = Db::empty();
        memory.run(&source(4096)).unwrap();
        assert_eq!(PREENCODE_TEST_ATTEMPTS.with(|x| x.get()), 0);
        let (_dir, mut db) = open();
        db.run(&source(127)).unwrap();
        assert_eq!(PREENCODE_TEST_ATTEMPTS.with(|x| x.get()), 0);
        drop(db);
        let (_dir, mut db) = open();
        db.run(&source(511)).unwrap();
        assert_eq!(PREENCODE_TEST_ATTEMPTS.with(|x| x.get()), 0);
        drop(db);
        for n in [512, 682] {
            let (_dir, mut db) = open();
            db.run(&source(n)).unwrap();
            assert_eq!(PREENCODE_TEST_ATTEMPTS.with(|x| x.get()), 0);
            drop(db);
        }
        let (_dir, db) = open();
        let mut db = db.with_embedder(std::sync::Arc::new(crate::embed::HashingEmbedder::new(
            "test/2048",
            2048,
        )));
        db.run(&source(511)).unwrap();
        assert_eq!(PREENCODE_TEST_ATTEMPTS.with(|x| x.get()), 0);
        drop(db);
        let (_dir, db) = open();
        let mut db = db.without_embedder();
        db.run(&source(4096)).unwrap();
        assert_eq!(PREENCODE_TEST_ATTEMPTS.with(|x| x.get()), 0);
        drop(db);
        let (_dir, db) = open();
        let mut db = db.with_embedder(std::sync::Arc::new(crate::embed::HashingEmbedder::new(
            "test/64", 64,
        )));
        db.run(&source(4096)).unwrap();
        assert_eq!(PREENCODE_TEST_ATTEMPTS.with(|x| x.get()), 0);
        drop(db);
        let (_dir, mut db) = open();
        db.run(&source(16385)).unwrap();
        assert_eq!(PREENCODE_TEST_ATTEMPTS.with(|x| x.get()), 0);
        drop(db);
        let (_dir, mut db) = open();
        let prep = db.prepare(&source(4096)).unwrap();
        {
            let mut tx = db.transaction().unwrap();
            tx.run_prepared(&prep).unwrap();
            tx.commit().unwrap();
        }
        assert_eq!(PREENCODE_TEST_ATTEMPTS.with(|x| x.get()), 0);
        drop(db);
        let (_dir, mut db) = open();
        let mut statements = crate::parse::parse_program(&source(4096)).unwrap();
        statements.extend(crate::parse::parse_program("docs | count").unwrap());
        let prepared = db.prepare_stmts(statements).unwrap();
        db.run_prepared(&prepared).unwrap();
        assert_eq!(PREENCODE_TEST_ATTEMPTS.with(|x| x.get()), 0);
        drop(db);
    }

    #[test]
    fn test_point_delete_and_update_extracted() {
        let mut db = Db::empty();
        let prep = db.prepare(r#"delete docs[id == "d-20"] cas "h:123""#).unwrap();
        assert!(prep.point_delete.is_some());
        let prep_up = db.prepare(r#"update docs[id == "d-20"] cas "h:123" { wing: "sys" }"#).unwrap();
        assert!(prep_up.point_update.is_some());
    }
}
