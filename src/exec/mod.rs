use std::collections::BTreeMap;
use std::fs::File;
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use rustc_hash::{FxHashMap, FxHashSet, FxHasher};

mod agg;
mod cold;
mod dml;
mod gpu_ops;
mod graph_ops;
mod join;
mod mirror;
mod prepared;
mod search;
mod tx;

#[cfg(test)]
mod tests;

pub use crate::persist::{OpenMemOpts as OpenOpts, SyncMode};
pub use prepared::Prepared;
pub use tx::Transaction;

pub(crate) use agg::*;
pub(crate) use crate::eval;
pub(crate) use crate::eval::*;
pub(crate) use join::{
    JoinFieldPlan, emit_join_row, insert_prefixed, merge_join_row, order_total_cell, plan_join_fields,
    pred_total_gt, project_join_fields, rows_to_rough_batch,
};

use dml::{pack_has_schema, stmt_append_only, stmt_schema, stmt_writes};
use gpu_ops::rows_to_query_batch;
use prepared::stmt_has_param;
use tx::{Undo, dummy_dml_plan};

use crate::ast::{CmpOp, Field, Pred, Query, Source, Step, Stmt, Value};
use crate::batch::RecordBatch;
use crate::catalog::Catalog;
use crate::check;
use crate::embed::{Embedder, HashingEmbedder};
use crate::error::Error;
use crate::explain::{self, ExplainCtx, RunStats};
use crate::graph::GraphFmt;
use crate::parse;
use crate::persist::{Pack, Persist};
use crate::plan::{self, Plan};
use crate::store::{
    Cell, Row, Store, compact_row, content_hash_inline, now_ms, project_fields, row_text,
};

type StmtOut = (Vec<Row>, Option<String>, Option<Pack>);

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

impl Handle {
    #[cold]
    #[inline(never)]
    fn recycle_rows_slow(&mut self) {
        if self.rows.len() >= 1024
            && self.rows.len() <= 2_500_000
            && self
                .rows
                .first()
                .is_some_and(|r| r.len() == 2 && r.contains_key("id") && r.contains_key("title"))
        {
            crate::store::release_id_title_rows(std::mem::take(&mut self.rows));
        } else if self.rows.len() >= 1024
            && self.rows.len() <= 262_144
            && self.rows[0].len() <= 8
        {
            crate::store::release_general_rows(std::mem::take(&mut self.rows));
        } else if self.rows.len() == 1 && self.rows[0].len() == 3 {
            let is_orders_shape = {
                let mut ks = self.rows[0].keys();
                ks.next().map(String::as_str) == Some("id")
                    && ks.next().map(String::as_str) == Some("total")
                    && ks.next().map(String::as_str) == Some("user_id")
            };
            if is_orders_shape && let Some(r) = self.rows.pop() {
                crate::store::release_orders_row(r);
            }
        }
    }
}

impl Drop for Handle {
    #[inline]
    fn drop(&mut self) {
        if self.rows.is_empty() {
            return;
        }
        if self.rows.len() == 1
            && self.rows[0]
                .shape_arc()
                .is_some_and(|s| Arc::ptr_eq(s, crate::store::shape_id_title()))
        {
            crate::store::release_single_id_title_vec(std::mem::take(&mut self.rows));
            return;
        }
        if self.rows.len() >= 1024 || (self.rows.len() == 1 && self.rows[0].len() == 3) {
            self.recycle_rows_slow();
        }
    }
}

impl Handle {
    #[inline]
    pub fn into_rows(mut self) -> Vec<Row> {
        std::mem::take(&mut self.rows)
    }

    /// First cell of the first row (`ExecuteScalar`). Empty result is an error.
    pub fn scalar(&self) -> Result<Cell, Error> {
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

pub struct Db {
    pub catalog: Catalog,
    pub store: Store,
    /// Durable log handle (writer only). Absent on in-memory and read snapshots.
    pub(crate) persist: Option<Persist>,
    /// Shared flock for [`Db::open_read`] (kept alive for lock lifetime).
    reader_lock: Option<File>,
    /// Source → prepared plan (invalidated on schema change).
    pub(crate) plan_cache: FxHashMap<String, Prepared>,
    /// Queryable AST shape → prepared (no typecheck on hit).
    pub(crate) query_cache: FxHashMap<u64, Prepared>,
    /// 2-hit bloom/ring filter for promoting repeated ad-hoc DML strings into `plan_cache`.
    pub(crate) seen_dml_hashes: [u64; 16],
    /// Packs of a multi-statement cold program in progress: applied to the disk overlay
    /// under a savepoint, logged as one batch when the program ends.
    pub(crate) cold_program_packs: Option<Vec<Pack>>,
    /// Rows staged by `insert_batch` for the `InsertRows` statement it runs next.
    pub(crate) pending_insert_rows: Option<(Vec<Row>, dml::BuiltShape)>,
    /// Hard limits (local prod).
    pub(crate) quotas: Quotas,
    /// Detailed wall-time phases of last open / open_read.
    reopen: ReopenPhases,
    /// Accumulated durable append/insert row counts and wall ms (for rows/s).
    append_rows: u64,
    append_ms: f64,
    /// Named in-memory pins (`pin "x"` / `unpin "x"`; aliases snapshot/restore).
    pub(crate) pins: BTreeMap<String, crate::store::MemBackup>,
    /// Last `pull idb` payload for in-process `push idb`.
    pub(crate) pulled_wal: Vec<u8>,
    /// Durable hot-standby: [`Self::apply_wal`] OK, user writes rejected.
    pub(crate) follower: bool,
    /// Active embedder (`search vec` / hybrid / auto-embed on insert).
    pub(crate) embedder: Option<Arc<dyn Embedder>>,
    /// Cached read-only snapshot keyed by `(gen, docs_epoch, collections_epoch, collections_len, edges_len, facts_len)`.
    pub(crate) reader_cache: Option<((u64, u64, u64, usize, usize, usize), ReadDb)>,
    /// Shared read-only plan cache preserved across `reader()` snapshots while catalog is unchanged.
    #[allow(clippy::type_complexity)]
    pub(crate) reader_plan_cache: Option<
        (
            (u64, usize, usize),
            Arc<std::sync::RwLock<FxHashMap<String, Prepared>>>,
        ),
    >,
    /// Fields per collection declared with [`Db::declare_columns`].
    pub(crate) declared_cols: FxHashMap<String, Vec<String>>,
    /// Lazily built mirrors of the declared fields, keyed by collection.
    pub(crate) col_mirrors: std::sync::RwLock<FxHashMap<String, Arc<mirror::ColMirror>>>,
    #[cfg(feature = "gpu")]
    pub(crate) gpu: Option<Arc<crate::gpu::GpuCompute>>,
    #[cfg(feature = "gpu")]
    pub(crate) gpu_cache: std::sync::Mutex<crate::gpu::SearchCache>,
}

/// Shared read-only snapshot of a [`Db`] at a fixed `gen`.
///
/// Cheap to `Clone` (`Arc`). Safe to share across threads. Writes are rejected.
#[derive(Clone)]
pub struct ReadDb {
    inner: Arc<Db>,
    plan_cache: Arc<std::sync::RwLock<FxHashMap<String, Prepared>>>,
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

/// One write of a commit group (see [`Db::execute_group_sync`]).
#[cfg(feature = "async")]
pub(crate) enum GroupWrite<'a> {
    Src { src: &'a str, want_rows: bool },
    Prepared { prepared: &'a Prepared, want_rows: bool },
    Stmt(&'a Stmt),
    Group(&'a [String]),
}

const PLAN_CACHE_MAX: usize = 512;
/// Upper bound on one insert into the overlay of a cold collection. Beyond it the
/// statement falls back to the in-memory path, which pulls the whole collection into
/// RAM, so the bound is generous: uniqueness is checked with one index probe per row
/// under a single engine guard, and the commit is still one WAL record.
const COLD_OVERLAY_INSERT_MAX_ROWS: usize = 1_048_576;

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
            seen_dml_hashes: [0; 16],
            cold_program_packs: None,
            pending_insert_rows: None,
            quotas: Quotas::default(),
            reopen: ReopenPhases::default(),
            append_rows: 0,
            append_ms: 0.0,
            pins: BTreeMap::new(),
            pulled_wal: Vec::new(),
            follower: false,
            embedder: Some(embedder),
            reader_cache: None,
            reader_plan_cache: None,
            declared_cols: Default::default(),
            col_mirrors: Default::default(),
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
        self.reader_cache = None;
        self
    }

    /// Enable or disable spill-to-disk for out-of-core aggregations when memory quota is exceeded.
    pub fn with_spill(mut self, enabled: bool) -> Self {
        self.quotas.spill_enabled = enabled;
        self.reader_cache = None;
        self
    }

    /// Enable experimental GPU cosine scoring. Initialization errors are explicit.
    #[cfg(feature = "gpu")]
    pub fn with_gpu(mut self, gpu: Arc<crate::gpu::GpuCompute>) -> Self {
        self.gpu = Some(gpu);
        self.gpu_cache = Default::default();
        self.reader_cache = None;
        self
    }

    /// Attach a hardware compute backend to an existing database.
    #[cfg(feature = "gpu")]
    pub fn enable_gpu(&mut self, gpu: Arc<crate::gpu::GpuCompute>) {
        self.gpu = Some(gpu);
        self.gpu_cache = Default::default();
        self.reader_cache = None;
    }

    /// Release cached GPU data and return search to the CPU backend.
    #[cfg(feature = "gpu")]
    pub fn disable_gpu(&mut self) {
        self.gpu = None;
        self.gpu_cache = Default::default();
        self.reader_cache = None;
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

    /// Replace the active embedder. Updates `catalog.embed_id`, and updates `store.embed_id`
    /// immediately when no vector-bearing collection currently holds stored rows (otherwise
    /// `store.embed_id` remains at the stored vectors' model until [`Self::reembed_collection`]
    /// recomputes them, so vector search refuses mixed-model queries).
    pub fn with_embedder(mut self, embedder: Arc<dyn Embedder>) -> Self {
        let new_id = embedder.id().to_string();
        let has_stored_vec_rows = self
            .catalog
            .collections
            .iter()
            .any(|(name, col)| col.has_vec() && self.store.get_col_count(name) > 0);
        self.catalog.set_embed_id(new_id.clone());
        if !has_stored_vec_rows || self.store.embed_id == new_id {
            self.store.embed_id = new_id;
            if let Some(p) = self.persist.as_mut() {
                p.catalog_hash = self.catalog.hash_str().to_string();
            }
        }
        self.embedder = Some(embedder);
        self.clear_plan_cache();
        self.reader_cache = None;
        self
    }

    /// Disable embedding (vec/hybrid degrade: vec empty, hybrid→lex only).
    pub fn without_embedder(mut self) -> Self {
        self.embedder = None;
        #[cfg(feature = "gpu")]
        {
            self.gpu_cache = Default::default();
        }
        self.clear_plan_cache();
        self.reader_cache = None;
        self
    }

    pub fn with_sync_mode(mut self, sync: SyncMode) -> Self {
        if let Some(p) = self.persist.as_mut() {
            p.sync = sync;
        }
        if self.store.is_native_disk()
            && let Some(d) = self.store.disk.as_ref()
            && let Ok(mut eng) = d.engine.write()
        {
            eng.set_sync_full(sync == SyncMode::Full);
        }
        self
    }

    /// Open durable store. Default: [`SyncMode::Full`], `cold: false`.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, Error> {
        Self::open_with(path, OpenOpts::default())
    }

    pub fn open_with(path: impl AsRef<Path>, opts: OpenOpts) -> Result<Self, Error> {
        let t0 = Instant::now();
        let catalog = crate::catalog::fixture();
        let dir = path.as_ref();
        if crate::store::checkpoint::is_pure_disk_dir(dir) {
            let (store, store_phases) =
                Store::open_native_disk(dir, &catalog, opts.sync == SyncMode::Full)?;
            let mut catalog = catalog;
            store.merge_extras_into(&mut catalog);
            let embedder = Self::default_embedder(&catalog);
            let mut db = Self {
                catalog,
                store,
                persist: None,
                reader_lock: None,
                plan_cache: FxHashMap::default(),
                query_cache: FxHashMap::default(),
                seen_dml_hashes: [0; 16],
                cold_program_packs: None,
                pending_insert_rows: None,
                quotas: Quotas::default(),
                reopen: store_phases.into(),
                append_rows: 0,
                append_ms: 0.0,
                pins: BTreeMap::new(),
                pulled_wal: Vec::new(),
                follower: false,
                embedder: Some(embedder),
                reader_cache: None,
                reader_plan_cache: None,
                declared_cols: Default::default(),
                col_mirrors: Default::default(),
                #[cfg(feature = "gpu")]
                gpu: None,
                #[cfg(feature = "gpu")]
                gpu_cache: Default::default(),
            };
            db.reopen.total_ms = t0.elapsed().as_secs_f64() * 1000.0;
            return Ok(db);
        }
        let (store, persist, store_phases) = Store::open_with_profiled(dir, &catalog, opts)?;
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
            seen_dml_hashes: [0; 16],
            cold_program_packs: None,
            pending_insert_rows: None,
            quotas: Quotas::default(),
            reopen: store_phases.into(),
            append_rows: 0,
            append_ms: 0.0,
            pins: BTreeMap::new(),
            pulled_wal: Vec::new(),
            follower: false,
            embedder: Some(embedder),
            reader_cache: None,
            reader_plan_cache: None,
            declared_cols: Default::default(),
            col_mirrors: Default::default(),
            #[cfg(feature = "gpu")]
            gpu: None,
            #[cfg(feature = "gpu")]
            gpu_cache: Default::default(),
        };
        db.reopen.total_ms = t0.elapsed().as_secs_f64() * 1000.0;
        Ok(db)
    }

    /// Open a durable **read-only follower** (hot standby).
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
            seen_dml_hashes: [0; 16],
            cold_program_packs: None,
            pending_insert_rows: None,
            quotas: Quotas::default(),
            reopen: store_phases.into(),
            append_rows: 0,
            append_ms: 0.0,
            pins: BTreeMap::new(),
            pulled_wal: Vec::new(),
            follower: false,
            embedder: Some(embedder),
            reader_cache: None,
            reader_plan_cache: None,
            declared_cols: Default::default(),
            col_mirrors: Default::default(),
            #[cfg(feature = "gpu")]
            gpu: None,
            #[cfg(feature = "gpu")]
            gpu_cache: Default::default(),
        };
        inner.reopen.total_ms = t0.elapsed().as_secs_f64() * 1000.0;
        Ok(ReadDb {
            inner: Arc::new(inner),
            plan_cache: Arc::new(std::sync::RwLock::new(FxHashMap::default())),
        })
    }

    pub fn close(&mut self) -> Result<(), Error> {
        if let Some(mut p) = self.persist.take() {
            self.store.checkpoint(&mut p)?;
        } else if self.store.is_native_disk()
            && let Some(d) = self.store.disk.as_ref()
        {
            d.engine
                .write()
                .map_err(|_| Error::runtime("disk engine lock poisoned"))?
                .checkpoint()?;
        }
        self.reader_lock = None;
        Ok(())
    }

    /// Force a durable checkpoint + log compaction (no-op if in-memory).
    pub fn checkpoint(&mut self) -> Result<(), Error> {
        if let Some(p) = self.persist.as_mut() {
            self.store.checkpoint(p)?;
        } else if self.store.is_native_disk()
            && let Some(d) = self.store.disk.as_ref()
        {
            d.engine
                .write()
                .map_err(|_| Error::runtime("disk engine lock poisoned"))?
                .checkpoint()?;
        }
        Ok(())
    }

    pub fn is_durable(&self) -> bool {
        self.persist.is_some() || self.store.is_native_disk()
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
        let cache_key = (
            self.store.r#gen,
            self.store.docs_epoch,
            self.store.collections.epoch(),
            self.store.collections.len(),
            self.store.edges.len(),
            self.store.facts_len(),
        );
        if !self.store.has_deferred_doc_ops()
            && let Some((k, cached)) = &self.reader_cache
            && *k == cache_key
        {
            return cached.clone();
        }
        let mirrors = self
            .col_mirrors
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        let cat_sig = (
            self.catalog.hash_u64(),
            self.catalog.fks.len(),
            self.catalog.guards.len(),
        );
        let shared_plans = match &self.reader_plan_cache {
            Some((sig, arc)) if *sig == cat_sig => Arc::clone(arc),
            _ => {
                let arc = Arc::new(std::sync::RwLock::new(FxHashMap::default()));
                self.reader_plan_cache = Some((cat_sig, Arc::clone(&arc)));
                arc
            }
        };
        let rdb = ReadDb {
            inner: Arc::new(Self {
                catalog: self.catalog.clone(),
                store: self.store.clone_mem(),
                persist: None,
                reader_lock: None,
                plan_cache: FxHashMap::default(),
                query_cache: FxHashMap::default(),
                seen_dml_hashes: [0; 16],
                cold_program_packs: None,
                pending_insert_rows: None,
                quotas: self.quotas,
                reopen: self.reopen,
                append_rows: 0,
                append_ms: 0.0,
                pins: BTreeMap::new(),
                pulled_wal: Vec::new(),
                follower: false,
                embedder: self.embedder.clone(),
                reader_cache: None,
                reader_plan_cache: None,
                declared_cols: self.declared_cols.clone(),
                col_mirrors: std::sync::RwLock::new(mirrors),
                #[cfg(feature = "gpu")]
                gpu: self.gpu.clone(),
                #[cfg(feature = "gpu")]
                gpu_cache: Default::default(),
            }),
            plan_cache: shared_plans,
        };
        self.reader_cache = Some((cache_key, rdb.clone()));
        rdb
    }

    /// Ship WAL frames with `gen > since` (follower / network).
    pub fn export_wal_since(&self, since: u64) -> Result<Vec<u8>, Error> {
        let Some(p) = self.persist.as_ref() else {
            return Ok(Vec::new());
        };
        crate::persist::export_wal_since_indexed(&p.dir, since, &p.wal_index)
    }

    /// Apply shipped WAL frames.
    pub fn apply_wal(&mut self, frames: &[u8]) -> Result<usize, Error> {
        if self.persist.is_some() && !self.follower {
            return Err(Error::runtime(
                "apply_wal: refused on durable primary — use open_follower / bootstrap_follower",
            ));
        }

        let mut to_apply: Vec<(&[u8], crate::persist::LogRecord)> = Vec::new();
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
            to_apply.push((raw, rec));
            Ok(())
        })?;

        if to_apply.is_empty() {
            return Ok(0);
        }

        if self.persist.is_some() {
            let raw_len: usize = to_apply.iter().map(|(r, _)| r.len()).sum();
            if let Some(p) = self.persist.as_mut()
                && p.log_bytes + (raw_len as u64) > self.quotas.max_log_bytes
            {
                self.store.checkpoint(p)?;
            }
            if let Some(p) = self.persist.as_mut() {
                let mut cur_offset = p.log_bytes;
                for &(raw, ref rec) in &to_apply {
                    p.wal_index.push((rec.r#gen, cur_offset));
                    cur_offset += raw.len() as u64;
                }
                if to_apply.len() == 1 {
                    crate::persist::append_validated_frames(
                        &mut p.log,
                        to_apply[0].0,
                        p.sync,
                        &mut p.log_bytes,
                    )?;
                } else {
                    let slices: Vec<std::io::IoSlice<'_>> = to_apply
                        .iter()
                        .map(|(raw, _)| std::io::IoSlice::new(raw))
                        .collect();
                    crate::persist::append_validated_frame_slices(
                        &mut p.log,
                        &slices,
                        raw_len,
                        p.sync,
                        &mut p.log_bytes,
                    )?;
                }
                p.writes_since_snapshot += to_apply.len() as u32;
            }
        }

        let n = to_apply.len();
        let mut saw_schema = false;
        for (_, rec) in &to_apply {
            if pack_has_schema(&rec.pack) {
                saw_schema = true;
            }
            self.store.apply_pack(&rec.pack);
            self.store.next_id = rec.next_id;
            self.store.r#gen = rec.r#gen;
        }
        if saw_schema {
            self.store.merge_extras_into(&mut self.catalog);
            self.store.ensure_fts(&self.catalog);
        }
        if let Some(p) = self.persist.as_mut() {
            if saw_schema {
                p.catalog_hash = self.catalog.hash_str().to_string();
            }
            self.store.maybe_checkpoint(p)?;
        }
        if saw_schema {
            self.clear_plan_cache();
        }
        self.reader_cache = None;
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
            None if self.store.is_native_disk() => {
                let wb = self
                    .store
                    .disk_engine_read()
                    .map(|eng| eng.wal_bytes)
                    .unwrap_or(0);
                (wb, 0, false, true)
            }
            None => (0, 0, false, false),
        };
        Stats {
            r#gen: self.store.r#gen,
            docs: self.store.get_col_count("docs"),
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

    pub(crate) fn check_quotas(&self) -> Result<(), Error> {
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
    pub fn export_backup(&mut self, path: impl AsRef<Path>) -> Result<(), Error> {
        if let Some(p) = self.persist.as_mut() {
            self.store.checkpoint(p)?;
        } else if self.store.is_native_disk()
            && self.store.edges.is_empty()
            && self.store.facts_len() == 0
        {
            self.checkpoint()?;
            if let Some(eng) = self.store.disk_engine_read() {
                return eng.export_portable_backup_file(path.as_ref());
            }
        }
        let hash = self.catalog.hash_str().to_string();
        let log_offset = self
            .persist
            .as_ref()
            .and_then(|p| p.log.metadata().ok().map(|m| m.len()))
            .unwrap_or(0);
        let snap = self.store.capture_snapshot_at(hash, log_offset);
        crate::persist::write_backup(path.as_ref(), &snap)
    }

    /// Load a backup file into a fresh in-memory Db.
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
        Self::open(dir)
    }

    pub fn clear_plan_cache(&mut self) {
        self.plan_cache.clear();
        self.query_cache.clear();
        self.seen_dml_hashes = [0; 16];
        self.reader_plan_cache = None;
    }

    #[inline]
    pub(crate) fn note_dml_seen(&mut self, src: &str) -> bool {
        let mut hasher = FxHasher::default();
        src.hash(&mut hasher);
        let h = hasher.finish().max(1);
        let slot = &mut self.seen_dml_hashes[(h as usize) & 15];
        if *slot == h {
            true
        } else {
            *slot = h;
            false
        }
    }

    #[inline]
    pub(crate) fn insert_plan_cache(&mut self, key: String, prep: Prepared) {
        if self.plan_cache.len() >= PLAN_CACHE_MAX {
            self.plan_cache.clear();
        }
        self.plan_cache.insert(key, prep);
    }

    fn query_cache_key(&self, q: &Query) -> u64 {
        let mut h = FxHasher::default();
        self.catalog.hash_u64().hash(&mut h);
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
        if self.query_cache.len() >= PLAN_CACHE_MAX {
            self.query_cache.clear();
        }
        self.query_cache.insert(key, prepared.clone());
        Ok(prepared)
    }

    /// Parse, typecheck, and plan once. Reuse via [`Prepared::run`] / [`Self::run_prepared`].
    pub fn prepare(&mut self, src: &str) -> Result<Prepared, Error> {
        if let Some(p) = self.plan_cache.get(src) {
            return Ok(p.clone());
        }
        let prepared = self.prepare_uncached(src)?;
        self.insert_plan_cache(src.to_string(), prepared.clone());
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
        self.prepare_stmts_checked(stmts)
    }

    pub(crate) fn prepare_stmts_checked(&self, stmts: Vec<Stmt>) -> Result<Prepared, Error> {
        let (_, staged) = check::check_program(&stmts, &self.catalog)?;
        let plan_cat = staged.as_ref().unwrap_or(&self.catalog);
        let plan = Arc::new(plan::plan_program(&stmts, plan_cat)?);
        let writes = stmts.iter().any(stmt_writes);
        let append_only = writes && stmts.iter().all(|s| stmt_append_only(s) || !stmt_writes(s));
        let schema = stmts.iter().any(stmt_schema);
        let has_params = stmts.iter().any(stmt_has_param);
        let point_get = if !has_params
            && stmts.len() == 1
            && let Stmt::Query(q) = &stmts[0]
            && !writes
        {
            Self::try_extract_point_get(q, plan_cat, &self.store).map(Arc::new)
        } else {
            None
        };
        let agg_plan = if !has_params
            && stmts.len() == 1
            && let Stmt::Query(q) = &stmts[0]
            && !writes
            && point_get.is_none()
        {
            Self::try_extract_agg_plan(q, plan_cat).map(Arc::new)
        } else {
            None
        };
        let point_delete = if !has_params && stmts.len() == 1 {
            Self::try_extract_point_delete(&stmts[0], &self.catalog, &self.store).map(Arc::new)
        } else {
            None
        };
        let point_update = if !has_params && stmts.len() == 1 {
            Self::try_extract_point_update(&stmts[0], &self.store).map(Arc::new)
        } else {
            None
        };
        let append_facts = if !has_params && stmts.len() == 1 {
            Self::try_extract_append_facts(&stmts[0]).map(Arc::new)
        } else {
            None
        };
        Ok(Prepared {
            src: String::new(),
            stmts: Arc::from(stmts),
            plan,
            writes,
            append_only,
            schema,
            has_params,
            point_get,
            agg_plan,
            point_delete,
            point_update,
            append_facts,
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

    /// Run independent sources one after another, each atomic on its own exactly as with
    /// [`Db::execute`], but with a single WAL sync for the whole group instead of one per
    /// statement. No result may be reported to its caller before this returns: a statement
    /// is durable only once the group sync has succeeded, and if that sync fails every
    /// statement that had succeeded is reported as failed.
    ///
    /// Only [`SyncMode::Full`] changes behaviour; other modes and non-durable databases
    /// just run the sources in order.
    pub fn execute_group_sync<S: AsRef<str>>(&mut self, sources: &[S]) -> Vec<Result<Done, Error>> {
        self.group_sync(sources.len(), |db, i| db.run_src_ext(sources[i].as_ref(), false))
            .into_iter()
            .map(|r| r.map(|h| h.done))
            .collect()
    }

    /// One write of a commit group, outside any sync policy.
    #[cfg(feature = "async")]
    pub(crate) fn run_group_write(&mut self, write: &GroupWrite<'_>) -> Result<Handle, Error> {
        match write {
            GroupWrite::Src { src, want_rows } => self.run_src_ext(src, *want_rows),
            GroupWrite::Prepared { prepared, want_rows } => {
                self.run_prepared_ext(prepared, *want_rows)
            }
            GroupWrite::Stmt(stmt) => self.run_stmt((*stmt).clone()),
            GroupWrite::Group(sources) => self.run_group(*sources),
        }
    }

    /// Run `body` (a group of `n` commits) with per-commit fsync deferred, then make the
    /// whole group durable with one sync. An error means the group reached the log but
    /// was not confirmed durable: no commit of the group may be acknowledged.
    pub(crate) fn with_group_sync(
        &mut self,
        n: usize,
        body: impl FnOnce(&mut Self),
    ) -> Result<(), Error> {
        let deferred = n > 1
            && self.persist.as_mut().is_some_and(|p| {
                if p.sync == SyncMode::Full {
                    p.sync = SyncMode::Normal;
                    true
                } else {
                    false
                }
            });
        body(self);
        if deferred && let Some(p) = self.persist.as_mut() {
            p.sync = SyncMode::Full;
            crate::persist::durable_sync(&p.log)
                .map_err(|e| Error::runtime(format!("group commit sync failed: {e}")))?;
        }
        Ok(())
    }

    fn group_sync(
        &mut self,
        n: usize,
        mut run: impl FnMut(&mut Self, usize) -> Result<Handle, Error>,
    ) -> Vec<Result<Handle, Error>> {
        let mut results: Vec<Result<Handle, Error>> = Vec::with_capacity(n);
        let synced = self.with_group_sync(n, |db| {
            results.extend((0..n).map(|i| run(db, i)));
        });
        if let Err(err) = synced {
            for r in &mut results {
                if r.is_ok() {
                    *r = Err(err.clone());
                }
            }
        }
        results
    }

    #[inline]
    fn try_fast_docs_id_title_get(&self, prepared: &Prepared) -> Option<Handle> {
        let pg = prepared.point_get.as_ref()?;
        if !pg.is_docs_id_title
            || self.store.disk.is_some()
            || !self.store.fts_deferred_doc_swaps.is_empty()
        {
            return None;
        }
        let idx = match pg.prepared_idx {
            Some(i)
                if i < self.store.docs_id.len()
                    && (self.store.docs_epoch == pg.prepared_docs_epoch
                        || self.store.docs_id[i].as_ref() == pg.key.as_str()) =>
            {
                i
            }
            _ => self.store.row_index("docs", &pg.key)?,
        };
        if idx >= self.store.docs_id.len() || idx >= self.store.docs_title.len() {
            return None;
        }
        let rows = crate::store::take_single_id_title_vec_with_arcs(
            &self.store.docs_id[idx],
            &self.store.docs_title[idx],
        );
        Some(Handle {
            plan: Arc::clone(&prepared.plan),
            rows,
            done: Done {
                r#gen: self.store.r#gen,
                n: 1,
            },
            message: None,
            ms: 0.0,
        })
    }

    #[inline]
    pub fn run_prepared(&mut self, prepared: &Prepared) -> Result<Handle, Error> {
        if let Some(h) = self.try_fast_docs_id_title_get(prepared) {
            return Ok(h);
        }
        self.run_prepared_ext(prepared, true)
    }

    pub(crate) fn execute_prepared(&mut self, prepared: &Prepared) -> Result<Done, Error> {
        Ok(self.run_prepared_ext(prepared, false)?.done)
    }

    /// Point DML on a disk-backed collection can go through the engine overlay (no promotion
    /// into memory) when the WAL is the main persist log (or native disk / staged overlay) and
    /// nothing needs in-memory row positions: no HNSW graph to maintain. Callers must rule out
    /// unique-key changes themselves.
    pub(crate) fn disk_overlay_point_dml(&self, collection: &str) -> bool {
        (self.persist.is_some()
            || self.store.is_native_disk()
            || self.cold_program_packs.is_some())
            && self.store.can_apply_disk_overlay(collection)
    }

    /// `delete <collection>[pred]` against a disk-backed collection without promoting it.
    /// Same contract as the in-memory path: CAS is required unless the collection is
    /// append-only, every matched row must carry the expected hash, and no match is an error.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn cold_overlay_delete(
        &mut self,
        collection: &str,
        pred: &Pred,
        cas: Option<&str>,
        cas_each: bool,
        mut cas_written: Option<&mut FxHashSet<(String, String)>>,
        plan: &Arc<Plan>,
        want_rows: bool,
    ) -> Result<Handle, Error> {
        let t0 = Instant::now();
        let now = if pred_needs_now(pred) { now_ms() } else { 0 };
        let append_only = self
            .catalog
            .collection(collection)
            .is_some_and(|c| c.append_only);
        if !append_only && cas.is_none() && !cas_each {
            return Err(Error::runtime("delete requires cas"));
        }
        let rows = {
            let Some(eng) = self.store.disk_engine_read() else {
                return Err(Error::runtime("disk engine lock poisoned"));
            };
            eng.query_filter(collection, Some(pred), None, None, now)?
        };
        if rows.is_empty() {
            return Err(Error::runtime("delete: no matching row"));
        }
        self.ensure_not_referenced(collection, &rows)?;
        if !append_only && cas_each {
            if let Some(written) = cas_written.as_deref() {
                for row in &rows {
                    if let Some(id) = row_text(row, "id")
                        && written.contains(&(collection.to_string(), id.to_string()))
                    {
                        return Err(Error::runtime(format!(
                            "cas mismatch: row {id} changed mid-pack"
                        )));
                    }
                }
            }
        } else if !append_only
            && let Some(cas) = cas
        {
            for row in &rows {
                let got = row_text(row, "hash").unwrap_or("");
                if got != cas {
                    return Err(Error::runtime(format!(
                        "cas mismatch: expected {cas:?}, found {got:?}"
                    )));
                }
            }
        }
        if let Some(written) = cas_written.as_deref_mut() {
            for row in &rows {
                if let Some(id) = row_text(row, "id") {
                    written.insert((collection.to_string(), id.to_string()));
                }
            }
        }
        let n = rows.len();
        let pack = Pack::Delete {
            collection: collection.to_string(),
            rows,
        };
        let Pack::Delete { rows, .. } = self.commit_disk_overlay_pack(pack)? else {
            unreachable!("commit_disk_overlay_pack returns its input pack");
        };
        let rows = if want_rows { rows } else { Vec::new() };
        if !rows.is_empty() {
            self.check_output_quota(&rows)?;
        }
        Ok(Handle {
            plan: Arc::clone(plan),
            rows,
            done: Done {
                r#gen: self.store.r#gen,
                n,
            },
            message: None,
            ms: t0.elapsed().as_secs_f64() * 1000.0,
        })
    }

    /// `insert <collection> ...` into a disk-backed collection without promoting it: rows go
    /// to the engine overlay after the same checks the in-memory path performs (duplicate
    /// id, unique keys, foreign keys, row quota). Returns `None` when a unique key cannot be
    /// verified against the disk engine, so the caller falls back to the in-memory path.
    pub(crate) fn cold_overlay_insert(
        &mut self,
        collection: &str,
        records: &[crate::ast::Record],
        cas_written: Option<&mut FxHashSet<(String, String)>>,
        plan: &Arc<Plan>,
        want_rows: bool,
    ) -> Result<Option<Handle>, Error> {
        self.cold_overlay_insert_edges(collection, records, &[], cas_written, plan, want_rows)
    }

    /// [`Self::cold_overlay_insert`] with the statement's `edge` clauses: edges start at the
    /// first inserted row, exactly as in the in-memory path, and are logged in the same pack.
    /// Edges live in memory; the overlay savepoint remembers how many there were, so a
    /// rolled-back program or transaction drops the ones it added.
    fn cold_overlay_insert_edges(
        &mut self,
        collection: &str,
        records: &[crate::ast::Record],
        edges: &[crate::ast::InsertEdge],
        cas_written: Option<&mut FxHashSet<(String, String)>>,
        plan: &Arc<Plan>,
        want_rows: bool,
    ) -> Result<Option<Handle>, Error> {
        let edges_need_now = edges.iter().any(|e| match &e.target {
            crate::ast::EdgeTarget::Value(v) => value_needs_now(v),
            crate::ast::EdgeTarget::Page(_) => false,
        });
        let now = if edges_need_now || records.iter().any(record_needs_now) {
            now_ms()
        } else {
            0
        };
        let rows: Vec<Row> = records
            .iter()
            .map(|rec| {
                let mut row = crate::store::acquire_row();
                for (k, v) in &rec.fields {
                    row.insert(k.clone(), value_cell(v, now));
                }
                row
            })
            .collect();
        self.cold_overlay_insert_rows(collection, rows, edges, now, cas_written, plan, want_rows)
    }

    /// Core of [`Self::cold_overlay_insert_edges`], shared with [`Self::insert_batch`]:
    /// `rows` are the new rows with absent fields left out. Returns `None` (nothing
    /// written, no id allocated) when a key has semantics this path does not reproduce.
    #[allow(clippy::too_many_arguments)]
    fn cold_overlay_insert_rows(
        &mut self,
        collection: &str,
        mut rows: Vec<Row>,
        edges: &[crate::ast::InsertEdge],
        now: i64,
        mut cas_written: Option<&mut FxHashSet<(String, String)>>,
        plan: &Arc<Plan>,
        want_rows: bool,
    ) -> Result<Option<Handle>, Error> {
        let t0 = Instant::now();
        // Unique constraints, composite and partial ones included, are checked against the
        // disk engine.
        let Some(unique_keys) = self.store.disk_unique_keys(collection) else {
            return Ok(None);
        };
        let key_now = if unique_keys.iter().any(|k| k.pred.is_some()) {
            now_ms()
        } else {
            0
        };
        for row in &rows {
            // A key value this path does not reproduce (NaN, a vector) and `docs` without
            // a text `uri` keep the in-memory path's own rules; decide before any id is
            // allocated.
            if unique_keys.iter().any(|k| !k.supports(row))
                || (collection == "docs" && row.get("uri").and_then(Cell::text).is_none())
            {
                return Ok(None);
            }
        }
        let n = rows.len();
        let total = self.total_rows();
        if total + n > self.quotas.max_rows {
            return Err(Error::runtime(format!(
                "quota: rows {} > max_rows {}",
                total + n,
                self.quotas.max_rows
            )));
        }
        let has_fks = self.catalog.fks.iter().any(|fk| fk.from_col == collection);
        for row in &mut rows {
            if row.get("id").and_then(Cell::text).is_none() {
                row.insert("id", Cell::text_arc(self.store.alloc_id()));
            }
            if row.get("hash").and_then(Cell::text).is_none()
                && let Some(body) = row.get("body").and_then(Cell::text)
            {
                let h = content_hash_inline(body);
                row.insert("hash", Cell::SmallText(h));
            }
        }
        {
            // One engine guard for the whole batch, ids borrowed from the rows.
            let eng = self.store.disk_engine_read();
            let mut seen_ids: FxHashSet<&str> = FxHashSet::default();
            seen_ids.reserve(rows.len());
            let mut seen_keys: FxHashSet<(usize, String)> = FxHashSet::default();
            for row in &rows {
                let id = row_text(row, "id").unwrap_or("");
                if !seen_ids.insert(id)
                    || eng.as_ref().is_some_and(|e| e.exists_by_id(collection, id))
                {
                    return Err(Error::runtime(format!("duplicate id: {id}")));
                }
                for (ki, key) in unique_keys.iter().enumerate() {
                    // Outside a partial index's predicate the row is not constrained.
                    let Some(text) = key.indexed_text(row, key_now) else {
                        continue;
                    };
                    if !seen_keys.insert((ki, text))
                        || self.store.disk_unique_conflict(collection, key, row, None, key_now)
                    {
                        return Err(Error::runtime(
                            if collection == "docs" && key.fields == ["uri"] {
                                format!("duplicate uri: {}", row_text(row, "uri").unwrap_or(""))
                            } else {
                                format!("unique index {}: duplicate key", key.label(collection))
                            },
                        ));
                    }
                }
                if has_fks {
                    self.check_row_fks(collection, row)?;
                }
            }
        }
        if let Some(written) = cas_written.as_deref_mut() {
            for row in &rows {
                if let Some(id) = row_text(row, "id") {
                    written.insert((collection.to_string(), id.to_string()));
                }
            }
        }
        self.maybe_embed_rows(collection, &mut rows);
        let new_edges: Vec<crate::store::Edge> = match rows.first().and_then(|r| row_text(r, "id")) {
            Some(from_id) if !edges.is_empty() => edges
                .iter()
                .map(|e| crate::store::Edge {
                    rel: e.rel.clone(),
                    from: from_id.to_string(),
                    to: match &e.target {
                        // A page target resolves to the document's id when it exists.
                        crate::ast::EdgeTarget::Page(uri) => {
                            self.doc_id_for_key(uri).unwrap_or_else(|| uri.clone())
                        }
                        crate::ast::EdgeTarget::Value(v) => value_text(v, now),
                    },
                })
                .collect(),
            _ => Vec::new(),
        };
        let rows = if n == 1 {
            let pack = Pack::Insert {
                collection: collection.to_string(),
                row: rows.pop().unwrap_or_default(),
                edges: new_edges,
            };
            match self.commit_disk_overlay_pack(pack)? {
                Pack::Insert { row, .. } if want_rows => vec![row],
                _ => Vec::new(),
            }
        } else {
            // Same compact columnar WAL record as the in-memory insert path: a row-shaped
            // record repeats every field name per row and overflows the record limit on
            // a few thousand documents with embeddings.
            let pack = crate::persist::rows_to_insert_pack(collection, &rows, new_edges);
            self.commit_disk_overlay_pack(pack)?;
            if want_rows { rows } else { Vec::new() }
        };
        if !rows.is_empty() {
            self.check_output_quota(&rows)?;
        }
        Ok(Some(Handle {
            plan: Arc::clone(plan),
            rows,
            done: Done {
                r#gen: self.store.r#gen,
                n,
            },
            message: None,
            ms: t0.elapsed().as_secs_f64() * 1000.0,
        }))
    }

    /// `update <collection>[pred] { .. }` against a disk-backed collection without promoting
    /// it. Mirrors the in-memory path: CAS, immutability guards, content hash refresh and
    /// foreign keys are checked for every matched row before anything is written.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn cold_overlay_update(
        &mut self,
        collection: &str,
        pred: Option<&Pred>,
        cas: Option<&str>,
        cas_each: bool,
        mut cas_written: Option<&mut FxHashSet<(String, String)>>,
        record: &crate::ast::Record,
        plan: &Arc<Plan>,
        want_rows: bool,
    ) -> Result<Handle, Error> {
        let t0 = Instant::now();
        if !cas_each && cas.is_none() {
            return Err(Error::runtime("update requires cas"));
        }
        let has_guards = !self.catalog.guards.is_empty();
        let needs_now = pred.is_some_and(pred_needs_now) || record_needs_now(record) || has_guards;
        let now = if needs_now { now_ms() } else { 0 };
        let before_rows = {
            let Some(eng) = self.store.disk_engine_read() else {
                return Err(Error::runtime("disk engine lock poisoned"));
            };
            eng.query_filter(collection, pred, None, None, now)?
        };
        if before_rows.is_empty() {
            return Err(Error::runtime("update: no matching row"));
        }
        let assigns_id = record.fields.iter().any(|(k, _)| k == "id");
        let old_ids: Vec<Option<String>> = before_rows
            .iter()
            .map(|r| row_text(r, "id").map(str::to_string))
            .collect();
        if cas_each {
            if let Some(written) = cas_written.as_deref() {
                for row in &before_rows {
                    if let Some(id) = row_text(row, "id")
                        && written.contains(&(collection.to_string(), id.to_string()))
                    {
                        return Err(Error::runtime(format!(
                            "cas mismatch: row {id} changed mid-pack"
                        )));
                    }
                }
            }
        } else if let Some(cas) = cas {
            for row in &before_rows {
                let got = row_text(row, "hash").unwrap_or("");
                if got != cas {
                    return Err(Error::runtime(format!(
                        "cas mismatch: expected {cas:?}, found {got:?}"
                    )));
                }
            }
        }
        let has_body = record.fields.iter().any(|(k, _)| k == "body");
        let has_fks = self.catalog.fks.iter().any(|fk| fk.from_col == collection);
        let unique_keys = self
            .store
            .unique_keys_assigned(collection, record.fields.iter().map(|(k, _)| k.as_str()));
        let mut seen_keys: FxHashSet<(usize, String)> = FxHashSet::default();
        let mut updated_rows = Vec::with_capacity(before_rows.len());
        // Same rule as the in-memory update: a changed title / body / snippet is
        // embedded again unless the statement sets the embedding itself.
        let reembed = self.embedder.is_some()
            && record
                .fields
                .iter()
                .any(|(k, _)| matches!(k.as_str(), "title" | "body" | "snippet"))
            && !record.fields.iter().any(|(k, _)| k == "embedding");
        for before in before_rows {
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
            let old_keys: Vec<Option<String>> = unique_keys
                .iter()
                .map(|k| k.indexed_text(&before, now_ms()))
                .collect();
            let mut updated = before;
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
            self.cold_check_unique_keys(collection, &unique_keys, &old_keys, &updated, &mut seen_keys)?;
            if has_fks {
                self.check_row_fks(collection, &updated)?;
            }
            updated_rows.push(updated);
        }
        if assigns_id {
            self.cold_check_id_change(collection, &old_ids, &updated_rows)?;
        }
        if let Some(written) = cas_written.as_deref_mut() {
            for id in old_ids.iter().flatten() {
                written.insert((collection.to_string(), id.clone()));
            }
            for row in &updated_rows {
                if let Some(id) = row_text(row, "id") {
                    written.insert((collection.to_string(), id.to_string()));
                }
            }
        }
        let n = updated_rows.len();
        let rows = self.commit_cold_update(collection, &old_ids, updated_rows)?;
        let rows = if want_rows { rows } else { Vec::new() };
        if !rows.is_empty() {
            self.check_output_quota(&rows)?;
        }
        Ok(Handle {
            plan: Arc::clone(plan),
            rows,
            done: Done {
                r#gen: self.store.r#gen,
                n,
            },
            message: None,
            ms: t0.elapsed().as_secs_f64() * 1000.0,
        })
    }

    pub(crate) fn exec_cold_point_delete(
        &mut self,
        pd: &prepared::PointDeletePlan,
        plan: &Arc<Plan>,
        want_rows: bool,
    ) -> Result<Handle, Error> {
        let has_incoming_fk = self
            .catalog
            .fks
            .iter()
            .any(|fk| fk.to_col == pd.collection);
        let can_id_only = !want_rows
            && pd.field == "id"
            && pd.collection != "facts"
            && !has_incoming_fk;
        let row = if can_id_only {
            if pd.append_only || pd.cas_each {
                if !self.store.disk_has_key(&pd.collection, "id", &pd.key) {
                    return Err(Error::runtime("delete: no matching row"));
                }
            } else {
                let Some(found) = self
                    .store
                    .disk_engine_read()
                    .and_then(|eng| {
                        eng.get_projected_by_id(&pd.collection, &pd.key, &["id", "hash"])
                            .ok()
                            .flatten()
                    })
                else {
                    return Err(Error::runtime("delete: no matching row"));
                };
                let got = row_text(&found, "hash").unwrap_or("");
                let cas = pd.cas.as_deref().unwrap_or("");
                if got != cas {
                    return Err(Error::runtime(format!(
                        "cas mismatch: expected {cas:?}, found {got:?}"
                    )));
                }
            }
            Row::from([("id", Cell::from_text(&pd.key))])
        } else {
            let Some(row) = self
                .store
                .disk_point_row(&pd.collection, &pd.field, &pd.key)
            else {
                return Err(Error::runtime("delete: no matching row"));
            };
            if !pd.append_only && !pd.cas_each {
                let got = row_text(&row, "hash").unwrap_or("");
                let cas = pd.cas.as_deref().unwrap_or("");
                if got != cas {
                    return Err(Error::runtime(format!(
                        "cas mismatch: expected {cas:?}, found {got:?}"
                    )));
                }
            }
            self.ensure_not_referenced(&pd.collection, std::slice::from_ref(&row))?;
            row
        };
        let pack = Pack::Delete {
            collection: pd.collection.clone(),
            rows: vec![row],
        };
        let out = if want_rows {
            let Pack::Delete { rows: out, .. } = self.commit_disk_overlay_pack(pack)? else {
                unreachable!("commit_disk_overlay_pack returns its input pack");
            };
            out
        } else {
            self.commit_disk_overlay_pack_discard(pack)?;
            Vec::new()
        };
        Ok(Handle {
            plan: Arc::clone(plan),
            rows: out,
            done: Done {
                r#gen: self.store.r#gen,
                n: 1,
            },
            message: None,
            ms: 0.0,
        })
    }

    /// Uniqueness of single-column unique keys changed by a cold `update`, checked against
    /// the disk engine (and against the other rows of the same statement via `seen`).
    /// An `update` of a cold collection that assigns `id`: like the in-memory path, it may
    /// rename one row to an id no other live row holds.
    fn cold_check_id_change(
        &self,
        collection: &str,
        old_ids: &[Option<String>],
        updated: &[Row],
    ) -> Result<(), Error> {
        let eng = self.store.disk_engine_read();
        for (old, row) in old_ids.iter().zip(updated) {
            let Some(new_id) = row_text(row, "id") else {
                continue;
            };
            if old.as_deref() == Some(new_id) {
                continue;
            }
            if updated.len() > 1
                || eng.as_ref().is_some_and(|e| e.exists_by_id(collection, new_id))
            {
                return Err(Error::runtime(format!("duplicate id: {new_id}")));
            }
        }
        Ok(())
    }

    /// Commit updated rows of a cold collection. Rows whose `id` changed are logged as a
    /// delete of the old id plus an upsert of the new row, in one record, since an overlay
    /// update is an upsert keyed by `id`.
    fn commit_cold_update_ext(
        &mut self,
        collection: &str,
        old_ids: &[Option<String>],
        rows: Vec<Row>,
        want_rows: bool,
    ) -> Result<Vec<Row>, Error> {
        let renamed: Vec<Row> = old_ids
            .iter()
            .zip(&rows)
            .filter_map(|(old, row)| {
                let old = old.as_deref()?;
                (row_text(row, "id") != Some(old)).then(|| Row::from([("id", Cell::from_text(old))]))
            })
            .collect();
        let update = Pack::Update {
            collection: collection.to_string(),
            rows,
        };
        if renamed.is_empty() {
            if !want_rows {
                self.commit_disk_overlay_pack_discard(update)?;
                return Ok(Vec::new());
            }
            let Pack::Update { rows, .. } = self.commit_disk_overlay_pack(update)? else {
                unreachable!("commit_disk_overlay_pack returns its input pack");
            };
            return Ok(rows);
        }
        let pack = Pack::Batch {
            packs: vec![
                Pack::Delete {
                    collection: collection.to_string(),
                    rows: renamed,
                },
                update,
            ],
        };
        if !want_rows {
            self.commit_disk_overlay_pack_discard(pack)?;
            return Ok(Vec::new());
        }
        let Pack::Batch { mut packs } = self.commit_disk_overlay_pack(pack)? else {
            unreachable!("commit_disk_overlay_pack returns its input pack");
        };
        match packs.pop() {
            Some(Pack::Update { rows, .. }) => Ok(rows),
            _ => unreachable!("the update is the last pack of the batch"),
        }
    }

    fn commit_cold_update(
        &mut self,
        collection: &str,
        old_ids: &[Option<String>],
        rows: Vec<Row>,
    ) -> Result<Vec<Row>, Error> {
        self.commit_cold_update_ext(collection, old_ids, rows, true)
    }

    /// Reject an update that moves a row onto a unique key another row already holds.
    /// `old[i]` is the row's key under `keys[i]` before the update (`None` when it was
    /// not indexed): an unchanged key needs no check.
    fn cold_check_unique_keys(
        &self,
        collection: &str,
        keys: &[crate::store::DiskUniqueKey],
        old: &[Option<String>],
        updated: &Row,
        seen: &mut FxHashSet<(usize, String)>,
    ) -> Result<(), Error> {
        if keys.is_empty() {
            return Ok(());
        }
        let now = now_ms();
        for (i, key) in keys.iter().enumerate() {
            let Some(new) = key.indexed_text(updated, now) else {
                continue;
            };
            if old[i].as_deref() == Some(new.as_str()) {
                continue;
            }
            let own_id = row_text(updated, "id");
            if !seen.insert((i, new))
                || self
                    .store
                    .disk_unique_conflict(collection, key, updated, own_id, now)
            {
                return Err(Error::runtime(
                    if collection == "docs" && key.fields == ["uri"] {
                        format!("duplicate uri: {}", row_text(updated, "uri").unwrap_or(""))
                    } else {
                        format!("unique index {}: duplicate key", key.label(collection))
                    },
                ));
            }
        }
        Ok(())
    }

    pub(crate) fn exec_cold_point_update(
        &mut self,
        pu: &prepared::PointUpdatePlan,
        plan: &Arc<Plan>,
        want_rows: bool,
    ) -> Result<Handle, Error> {
        let unique_keys = self
            .store
            .unique_keys_assigned(&pu.collection, pu.record.fields.iter().map(|(k, _)| k.as_str()));
        let has_guards = self
            .catalog
            .guards
            .iter()
            .any(|g| g.collection == pu.collection);
        let will_reembed = self.embedder.is_some()
            && pu.record.fields.iter().any(|(k, _)| matches!(k.as_str(), "title" | "body" | "snippet"))
            && !pu.record.fields.iter().any(|(k, _)| k == "embedding");
        let mut skip: smallvec::SmallVec<[&str; 4]> = smallvec::SmallVec::new();
        if pu.field == "id" && !has_guards {
            for (k, _) in &pu.record.fields {
                let ks = k.as_str();
                if ks != "id"
                    && (pu.cas_each || ks != "hash")
                    && !unique_keys.iter().any(|uk| uk.fields.iter().any(|f| f == ks))
                {
                    skip.push(ks);
                }
            }
            if will_reembed
                && !unique_keys.iter().any(|uk| uk.fields.iter().any(|f| f == "embedding"))
                && !skip.contains(&"embedding")
            {
                skip.push("embedding");
            }
        }
        let Some(before) = self
            .store
            .disk_point_row_skipping(&pu.collection, &pu.field, &pu.key, &skip)
        else {
            return Err(Error::runtime("update: no matching row"));
        };
        if !pu.cas_each {
            let got = row_text(&before, "hash").unwrap_or("");
            let cas = pu.cas.as_deref().unwrap_or("");
            if got != cas {
                return Err(Error::runtime(format!(
                    "cas mismatch: expected {cas:?}, found {got:?}"
                )));
            }
        }
        let needs_now = has_guards
            || !unique_keys.is_empty()
            || pu
                .record
                .fields
                .iter()
                .any(|(_, v)| eval::value_needs_now(v));
        let now = if needs_now { now_ms() } else { 0 };
        for (k, _) in &pu.record.fields {
            for g in &self.catalog.guards {
                if g.collection == pu.collection
                    && &g.field == k
                    && eval_pred(&g.pred, &before, now)
                {
                    return Err(Error::runtime(format!(
                        "immutable field: {}.{k}",
                        pu.collection
                    )));
                }
            }
        }
        let old_keys: smallvec::SmallVec<[Option<String>; 4]> = unique_keys
            .iter()
            .map(|k| k.indexed_text(&before, now))
            .collect();
        let mut old_ids: smallvec::SmallVec<[Option<String>; 1]> = smallvec::SmallVec::new();
        old_ids.push(row_text(&before, "id").map(str::to_string));
        let mut updated = before;
        for (k, v) in &pu.record.fields {
            updated.insert(k.as_str(), value_cell(v, now));
        }
        let has_body = pu.record.fields.iter().any(|(k, _)| k == "body");
        if has_body && let Some(body) = row_text(&updated, "body") {
            let h = content_hash_inline(body);
            updated.insert("hash", Cell::SmallText(h));
        }
        // A changed title / body / snippet is embedded again (see the general update).
        if will_reembed {
            updated.remove("embedding");
            self.maybe_embed_rows(&pu.collection, std::slice::from_mut(&mut updated));
        }
        self.cold_check_unique_keys(
            &pu.collection,
            &unique_keys,
            &old_keys,
            &updated,
            &mut FxHashSet::default(),
        )?;
        self.check_row_fks(&pu.collection, &updated)?;
        let updated = vec![updated];
        if pu.record.fields.iter().any(|(k, _)| k == "id") {
            self.cold_check_id_change(&pu.collection, &old_ids, &updated)?;
        }
        let out = self.commit_cold_update_ext(&pu.collection, &old_ids, updated, want_rows)?;
        Ok(Handle {
            plan: Arc::clone(plan),
            rows: out,
            done: Done {
                r#gen: self.store.r#gen,
                n: 1,
            },
            message: None,
            ms: 0.0,
        })
    }

    fn cold_program_stmts_eligible(&self, stmts: &[Stmt], allow_non_disk_write_check: bool) -> bool {
        let _ = allow_non_disk_write_check;
        stmts.iter().any(dml::stmt_writes)
            && stmts.iter().all(|stmt| match stmt {
                Stmt::Insert {
                    collection,
                    records,
                    edges,
                } => {
                    let _ = edges;
                    !records.is_empty()
                        && records.len() <= COLD_OVERLAY_INSERT_MAX_ROWS
                        && self.disk_overlay_point_dml(collection)
                }
                Stmt::Update {
                    collection, record, ..
                } => {
                    self.disk_overlay_point_dml(collection)
                        && !record.fields.iter().any(|(k, _)| {
                            self.store.unique_key_needs_memory(collection, k)
                        })
                }
                Stmt::Delete { collection, .. } => self.disk_overlay_point_dml(collection),
                Stmt::Let { .. } | Stmt::Query(_) => true,
                _ => false,
            })
    }

    fn exec_cold_overlay_program_steps(
        &mut self,
        stmts: &[Stmt],
        plan: &Arc<Plan>,
        want_rows: bool,
    ) -> Result<Option<Handle>, Error> {
        let track_written = stmts.iter().any(|stmt| {
            matches!(
                stmt,
                Stmt::Update { cas_each: true, .. } | Stmt::Delete { cas_each: true, .. }
            )
        });
        let mut written: FxHashSet<(String, String)> = FxHashSet::default();
        let mut bindings: BTreeMap<String, Vec<Row>> = BTreeMap::new();
        let last_idx = stmts.len().saturating_sub(1);
        let mut last: Option<Handle> = None;
        for (i, stmt) in stmts.iter().enumerate() {
            let rows_wanted = want_rows && i == last_idx;
            let cw = if track_written { Some(&mut written) } else { None };
            match stmt {
                Stmt::Insert {
                    collection,
                    records,
                    edges,
                } => match self.cold_overlay_insert_edges(
                    collection,
                    records,
                    edges,
                    cw,
                    plan,
                    rows_wanted,
                )? {
                    Some(h) => last = Some(h),
                    None => return Ok(None),
                },
                Stmt::Update {
                    collection,
                    pred,
                    cas,
                    cas_each,
                    record,
                } => {
                    last = Some(self.cold_overlay_update(
                        collection,
                        pred.as_ref(),
                        cas.as_deref(),
                        *cas_each,
                        cw,
                        record,
                        plan,
                        rows_wanted,
                    )?);
                }
                Stmt::Delete {
                    collection,
                    pred,
                    cas,
                    cas_each,
                } => {
                    last = Some(self.cold_overlay_delete(
                        collection,
                        pred,
                        cas.as_deref(),
                        *cas_each,
                        cw,
                        plan,
                        rows_wanted,
                    )?);
                }
                Stmt::Let { name, query } => {
                    let rows = self.exec_query(query, &bindings)?;
                    let n = rows.len();
                    let out_rows = if rows_wanted {
                        rows.clone()
                    } else {
                        Vec::new()
                    };
                    bindings.insert(name.clone(), rows);
                    last = Some(Handle {
                        plan: Arc::clone(plan),
                        rows: out_rows,
                        done: Done {
                            r#gen: self.store.r#gen,
                            n,
                        },
                        message: None,
                        ms: 0.0,
                    });
                }
                Stmt::Query(q) => {
                    let mut rows = self.exec_query(q, &bindings)?;
                    let n = rows.len();
                    if !rows_wanted {
                        rows.clear();
                    }
                    if !rows.is_empty() {
                        self.check_output_quota(&rows)?;
                    }
                    last = Some(Handle {
                        plan: Arc::clone(plan),
                        rows,
                        done: Done {
                            r#gen: self.store.r#gen,
                            n,
                        },
                        message: None,
                        ms: 0.0,
                    });
                }
                _ => return Ok(None),
            }
        }
        Ok(last)
    }

    /// Execute cold-overlay DML inside an active `Transaction` (which owns the savepoint and
    /// defers WAL logging to `Transaction::commit`). Returns `Ok(None)` when the statements need
    /// the in-memory path.
    pub(crate) fn try_cold_overlay_in_tx(
        &mut self,
        stmts: &[Stmt],
        plan: &Arc<Plan>,
        base_gen: u64,
        want_rows: bool,
    ) -> Result<Option<(Handle, Option<Pack>, bool)>, Error> {
        if stmts.is_empty() || !stmts.iter().any(dml::stmt_writes) {
            return Ok(None);
        }
        if self.cold_program_packs.is_some() {
            return Ok(None);
        }
        let eligible = if stmts.len() == 1 {
            match &stmts[0] {
                Stmt::Delete { collection, .. } => self.store.can_apply_disk_overlay(collection),
                Stmt::Update {
                    collection, record, ..
                } => {
                    self.store.can_apply_disk_overlay(collection)
                        && !record.fields.iter().any(|(k, _)| {
                            self.store.unique_key_needs_memory(collection, k)
                        })
                }
                Stmt::Insert {
                    collection,
                    records,
                    edges,
                } => {
                    let _ = edges;
                    !records.is_empty()
                        && records.len() <= COLD_OVERLAY_INSERT_MAX_ROWS
                        && self.store.can_apply_disk_overlay(collection)
                }
                _ => false,
            }
        } else {
            stmts.iter().all(|stmt| match stmt {
                Stmt::Insert {
                    collection,
                    records,
                    edges,
                } => {
                    let _ = edges;
                    !records.is_empty()
                        && records.len() <= COLD_OVERLAY_INSERT_MAX_ROWS
                        && self.store.can_apply_disk_overlay(collection)
                }
                Stmt::Update {
                    collection, record, ..
                } => {
                    self.store.can_apply_disk_overlay(collection)
                        && !record.fields.iter().any(|(k, _)| {
                            self.store.unique_key_needs_memory(collection, k)
                        })
                }
                Stmt::Delete { collection, .. } => self.store.can_apply_disk_overlay(collection),
                Stmt::Let { .. } | Stmt::Query(_) => true,
                _ => false,
            })
        };
        if !eligible {
            return Ok(None);
        }

        let t0 = Instant::now();
        self.cold_program_packs = Some(Vec::with_capacity(stmts.len()));
        let res = if stmts.len() == 1 {
            match &stmts[0] {
                Stmt::Delete {
                    collection,
                    pred,
                    cas,
                    cas_each,
                } => self
                    .cold_overlay_delete(
                        collection,
                        pred,
                        cas.as_deref(),
                        *cas_each,
                        None,
                        plan,
                        want_rows,
                    )
                    .map(Some),
                Stmt::Update {
                    collection,
                    pred,
                    cas,
                    cas_each,
                    record,
                } => self
                    .cold_overlay_update(
                        collection,
                        pred.as_ref(),
                        cas.as_deref(),
                        *cas_each,
                        None,
                        record,
                        plan,
                        want_rows,
                    )
                    .map(Some),
                Stmt::Insert {
                    collection,
                    records,
                    edges,
                } => self.cold_overlay_insert_edges(
                    collection, records, edges, None, plan, want_rows,
                ),
                _ => Ok(None),
            }
        } else {
            self.exec_cold_overlay_program_steps(stmts, plan, want_rows)
        };
        let packs = self.cold_program_packs.take().unwrap_or_default();
        let handle = match res {
            Ok(Some(h)) => h,
            Ok(None) => return Ok(None),
            Err(e) => return Err(e),
        };
        let pack = dml::fold_packs(packs);
        let wrote = pack.is_some();
        if wrote {
            if self.is_durable() {
                self.store.r#gen = base_gen + 1;
            } else {
                self.store.r#gen += 1;
            }
        }
        let mut h = handle;
        h.done.r#gen = self.store.r#gen;
        h.ms = t0.elapsed().as_secs_f64() * 1000.0;
        Ok(Some((h, pack, wrote)))
    }

    /// A program of several statements targeting cold collections runs against the disk overlay
    /// under one savepoint: later statements see earlier ones, `cas each` tracks per-row mid-pack
    /// conflicts, `let` and queries bind/read overlay state, nothing is promoted into memory,
    /// the whole program is logged as one batch, and any failure rolls every statement back.
    /// Returns `None` (with nothing changed) when some statement needs the in-memory path.
    fn cold_overlay_program(
        &mut self,
        stmts: &[Stmt],
        plan: &Arc<Plan>,
        want_rows: bool,
    ) -> Result<Option<Handle>, Error> {
        if stmts.len() <= 1 || !self.cold_program_stmts_eligible(stmts, false) {
            return Ok(None);
        }
        let t0 = Instant::now();
        let saved_next_id = self.store.next_id;
        self.store.begin_disk_overlay_savepoint();
        self.cold_program_packs = Some(Vec::with_capacity(stmts.len()));
        let step = self.exec_cold_overlay_program_steps(stmts, plan, want_rows);
        let packs = self.cold_program_packs.take().unwrap_or_default();
        let is_native = self.store.is_native_disk();
        let committed = match step {
            Ok(Some(last)) => match dml::fold_packs(packs) {
                Some(batch) => {
                    if let Some(p) = self.persist.as_mut() {
                        self.store.durable_commit(p, batch).map(|()| Some(last))
                    } else if is_native {
                        self.store.rollback_disk_overlay_savepoint();
                        self.store
                            .commit_pack_to_native_disk(&batch)
                            .map(|()| Some(last))
                    } else {
                        Err(Error::runtime("disk overlay requires a durable log"))
                    }
                }
                None => Ok(Some(last)),
            },
            Ok(None) => Ok(None),
            Err(e) => Err(e),
        };
        match committed {
            Ok(Some(mut h)) => {
                if !is_native {
                    self.store.commit_disk_overlay_savepoint();
                }
                self.store.r#gen += 1;
                if let Some(p) = self.persist.as_mut() {
                    self.store.maybe_checkpoint(p)?;
                }
                h.done.r#gen = self.store.r#gen;
                h.ms = t0.elapsed().as_secs_f64() * 1000.0;
                Ok(Some(h))
            }
            Ok(None) => {
                self.store.rollback_disk_overlay_savepoint();
                self.store.next_id = saved_next_id;
                Ok(None)
            }
            Err(e) => {
                self.store.rollback_disk_overlay_savepoint();
                self.store.next_id = saved_next_id;
                Err(e)
            }
        }
    }

    /// Id of the document addressed by `key` (an id or a uri), whether `docs` is in memory
    /// or on disk; a cold collection is probed through the engine, not materialized.
    fn doc_id_for_key(&self, key: &str) -> Option<String> {
        if self.store.is_disk_backed("docs") {
            let row = self
                .store
                .disk_point_row("docs", "id", key)
                .or_else(|| self.store.disk_point_row("docs", "uri", key))?;
            return row_text(&row, "id").map(str::to_string);
        }
        self.store
            .find_doc_key(key)
            .and_then(|r| row_text(r, "id"))
            .map(str::to_string)
    }

    /// Durably log `pack`, then apply it to the disk overlay. Returns the pack back.
    fn commit_disk_overlay_pack(&mut self, pack: Pack) -> Result<Pack, Error> {
        // Inside a multi-statement program or interactive transaction the pack only reaches
        // the overlay now; all packs are logged as one batch on completion/commit.
        if let Some(packs) = self.cold_program_packs.as_mut() {
            packs.push(pack.clone());
            self.store.apply_pack(&pack);
            return Ok(pack);
        }
        if self.store.is_native_disk() {
            self.store.commit_pack_to_native_disk(&pack)?;
            self.store.r#gen += 1;
            return Ok(pack);
        }
        let Some(p) = self.persist.as_mut() else {
            return Err(Error::runtime("disk overlay requires a durable log"));
        };
        self.store.durable_commit(p, pack.clone())?;
        self.store.apply_pack(&pack);
        self.store.r#gen += 1;
        if let Some(p) = self.persist.as_mut() {
            self.store.maybe_checkpoint(p)?;
        }
        Ok(pack)
    }

    /// Durably log `pack` and move it into the disk overlay without cloning when rows aren't needed.
    fn commit_disk_overlay_pack_discard(&mut self, pack: Pack) -> Result<(), Error> {
        if let Some(packs) = self.cold_program_packs.as_mut() {
            self.store.apply_pack(&pack);
            packs.push(pack);
            return Ok(());
        }
        if self.store.is_native_disk() {
            self.store.commit_pack_to_native_disk(&pack)?;
            self.store.r#gen += 1;
            return Ok(());
        }
        let Some(p) = self.persist.as_mut() else {
            return Err(Error::runtime("disk overlay requires a durable log"));
        };
        let can_ref = match &pack {
            Pack::Update { .. } => true,
            Pack::Delete { collection, rows } => {
                let keys: &[&str] = if collection == "facts" {
                    &["s", "p", "o"]
                } else {
                    &["id", "uri"]
                };
                rows.iter()
                    .all(|r| r.iter().all(|(k, _)| keys.contains(&k.as_str())))
            }
            _ => false,
        };
        if can_ref {
            self.store.durable_commit_ref(p, &pack)?;
        } else {
            self.store.durable_commit(p, pack.clone())?;
        }
        let mut ops = Vec::new();
        crate::store::checkpoint::pack_into_disk_wal_ops(pack, &mut ops);
        if !ops.is_empty() {
            let _ = self.store.apply_disk_overlay_ops_owned(ops);
        }
        self.store.r#gen += 1;
        if let Some(p) = self.persist.as_mut() {
            self.store.maybe_checkpoint(p)?;
        }
        Ok(())
    }

    #[inline]
    pub(crate) fn can_fast_docs_wing_update(&self, pu: &prepared::PointUpdatePlan) -> bool {
        pu.is_docs_wing_only
            && !self.is_durable()
            && !self.store.is_disk_backed(&pu.collection)
            && (pu.cas.is_some() || pu.cas_each)
            && (self.catalog.guards.is_empty()
                || !self
                    .catalog
                    .guards
                    .iter()
                    .any(|g| g.collection == "docs" && g.field == "wing"))
            && (self.catalog.fks.is_empty()
                || !self.catalog.fks.iter().any(|fk| fk.from_col == "docs"))
            && (self.store.fts.is_empty()
                || !self
                    .store
                    .fts
                    .get("docs")
                    .is_some_and(|f| f.fields.iter().any(|fld| fld == "wing")))
    }

    #[inline]
    pub(crate) fn exec_docs_wing_update_core(
        &mut self,
        pu: &prepared::PointUpdatePlan,
        idx: usize,
        want_rows: bool,
        tx_mode: bool,
    ) -> Result<(Option<Cell>, Option<u8>, Vec<Row>), Error> {
        let Store {
            collections,
            docs_wing,
            indexes,
            retired_cells,
            deferred_doc_wing_updates,
            ..
        } = &mut self.store;
        let docs_col = collections.get_mut(&pu.collection).unwrap();
        let defer_wing_idx = docs_col.len() >= 64;
        let large_table = docs_col.len() >= 512;
        let row = &mut docs_col[idx];
        let shape_hit = row
            .shape_arc()
            .zip(pu.prepared_shape.as_ref())
            .is_some_and(|(a, b)| Arc::ptr_eq(a, b));
        let wing_slot_used = if shape_hit { pu.wing_slot } else { None };
        if !pu.cas_each {
            let got = if shape_hit {
                pu.hash_slot
                    .and_then(|s| row.cells_slice().get(s as usize))
                    .and_then(Cell::text)
                    .unwrap_or("")
            } else {
                row_text(row, "hash").unwrap_or("")
            };
            let cas = pu.cas.as_deref().unwrap_or("");
            if got != cas {
                return Err(Error::runtime(format!(
                    "cas mismatch: expected {cas:?}, found {got:?}"
                )));
            }
        }
        let mut dyn_arc: Option<Arc<str>> = None;
        let new_cell = if let Some((c, _)) = &pu.const_wing {
            c.clone()
        } else {
            let now = if value_needs_now(&pu.record.fields[0].1) {
                now_ms()
            } else {
                0
            };
            let cell = value_cell(&pu.record.fields[0].1, now);
            dyn_arc = Some(cell.text_shared().unwrap_or_default());
            cell
        };
        let wing_str: &str = if let Some((_, a)) = &pu.const_wing {
            a.as_ref()
        } else {
            dyn_arc.as_deref().unwrap_or("")
        };
        let mut has_complex_idx = false;
        let mut has_unique_idx = false;
        let mut needs_ts = false;
        for live_idx in indexes.values() {
            if live_idx.def.collection == pu.collection {
                if live_idx.def.unique {
                    has_unique_idx = true;
                }
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
            if shape_hit {
                pu.ts_slot
                    .and_then(|s| row.cells_slice().get(s as usize))
                    .cloned()
                    .unwrap_or(Cell::Null)
            } else {
                row.get("ts").cloned().unwrap_or(Cell::Null)
            }
        } else {
            Cell::Null
        };
        let prev_cell = if let Some(ws) = wing_slot_used
            && let Some(slot) = row.cells_mut().get_mut(ws as usize)
        {
            Some(std::mem::replace(slot, new_cell))
        } else if let Some(slot) = row.get_mut("wing") {
            Some(std::mem::replace(slot, new_cell))
        } else {
            row.insert("wing".to_string(), new_cell)
        };
        let same_wing = prev_cell
            .as_ref()
            .and_then(Cell::text)
            .is_some_and(|s| s == wing_str);
        if has_complex_idx {
            let mut key_iter = old_keys.into_iter();
            for live_idx in indexes.values_mut() {
                if live_idx.def.collection == pu.collection {
                    let old_k = key_iter.next();
                    let new_k = Some(live_idx.key_of(row));
                    if let Err(e) = live_idx.update_keys(idx, old_k, new_k) {
                        if let Some(prev) = prev_cell {
                            if let Some(ws) = wing_slot_used
                                && let Some(slot) = row.cells_mut().get_mut(ws as usize)
                            {
                                *slot = prev;
                            } else {
                                row.insert("wing".to_string(), prev);
                            }
                        } else {
                            row.remove("wing");
                        }
                        return Err(Error::runtime(e));
                    }
                }
            }
        } else if !same_wing {
            if defer_wing_idx
                && !has_unique_idx
                && deferred_doc_wing_updates.len() < deferred_doc_wing_updates.inline_size()
            {
                let new_wing = match &pu.const_wing {
                    Some((_, a)) => Arc::clone(a),
                    None => dyn_arc.as_ref().map(Arc::clone).unwrap_or_default(),
                };
                deferred_doc_wing_updates.push(crate::store::DeferredDocWingUpdate {
                    idx,
                    prev_wing: prev_cell.clone().unwrap_or(Cell::Null),
                    new_wing,
                    ts: ts_cell,
                });
            } else {
                let new_wing_cell = wing_slot_used
                    .and_then(|ws| row.cells_slice().get(ws as usize))
                    .unwrap_or_else(|| row.get("wing").unwrap_or(&Cell::Null));
                for live_idx in indexes.values_mut() {
                    if live_idx.def.collection == pu.collection {
                        if live_idx.def.fields.len() == 1 && live_idx.def.fields[0] == "wing" {
                            let old_k = Some(crate::index::LiveIndex::single_cell_key(
                                prev_cell.as_ref().unwrap_or(&Cell::Null),
                            ));
                            let new_k = Some(crate::index::LiveIndex::single_cell_key(new_wing_cell));
                            if let Err(e) = live_idx.update_keys(idx, old_k, new_k) {
                                if let Some(prev) = prev_cell {
                                    if let Some(ws) = wing_slot_used
                                        && let Some(slot) = row.cells_mut().get_mut(ws as usize)
                                    {
                                        *slot = prev;
                                    } else {
                                        row.insert("wing".to_string(), prev);
                                    }
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
                                new_wing_cell,
                                &ts_cell,
                            ));
                            if let Err(e) = live_idx.update_keys(idx, old_k, new_k) {
                                if let Some(prev) = prev_cell {
                                    if let Some(ws) = wing_slot_used
                                        && let Some(slot) = row.cells_mut().get_mut(ws as usize)
                                    {
                                        *slot = prev;
                                    } else {
                                        row.insert("wing".to_string(), prev);
                                    }
                                } else {
                                    row.remove("wing");
                                }
                                return Err(Error::runtime(e));
                            }
                        }
                    }
                }
            }
        }
        if !same_wing && idx < docs_wing.len() {
            docs_wing.set_str(idx, wing_str);
        }
        if tx_mode {
            return Ok((prev_cell, wing_slot_used, Vec::new()));
        }
        let out_rows = if !want_rows || large_table {
            if let Some(prev) = prev_cell
                && retired_cells.len() < retired_cells.inline_size()
            {
                retired_cells.push(prev);
            }
            Vec::new()
        } else {
            vec![row.clone()]
        };
        Ok((None, wing_slot_used, out_rows))
    }

    fn run_prepared_ext(&mut self, prepared: &Prepared, want_rows: bool) -> Result<Handle, Error> {
        if let Some(pg) = &prepared.point_get {
            if !self.store.fts_deferred_doc_swaps.is_empty()
                || (!pg.is_docs_id_title && !self.store.deferred_doc_wing_updates.is_empty())
            {
                self.store.flush_fts_deferred_docs();
            }
            if pg.is_docs_id_title
                && self.store.disk.is_none()
                && self.store.fts_deferred_doc_swaps.is_empty()
            {
                let idx_opt = pg
                    .prepared_idx
                    .filter(|&i| {
                        i < self.store.docs_id.len()
                            && (self.store.docs_epoch == pg.prepared_docs_epoch
                                || self.store.docs_id[i].as_ref() == pg.key.as_str())
                    })
                    .or_else(|| self.store.row_index("docs", &pg.key));
                if let Some(idx) = idx_opt
                    && idx < self.store.docs_id.len()
                    && idx < self.store.docs_title.len()
                {
                    let rows = if want_rows {
                        crate::store::take_single_id_title_vec_with_arcs(
                            &self.store.docs_id[idx],
                            &self.store.docs_title[idx],
                        )
                    } else {
                        Vec::new()
                    };
                    return Ok(Handle {
                        plan: prepared.plan.clone(),
                        rows,
                        done: Done {
                            r#gen: self.store.r#gen,
                            n: 1,
                        },
                        message: None,
                        ms: 0.0,
                    });
                }
            }
            let row = self.execute_point_get(pg);
            let (rows, n) = match row {
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
        if self.follower && prepared.writes {
            return Err(Error::runtime(
                "follower: read-only — apply_wal for replication, open primary for writes",
            ));
        }
        let can_coalesce_wing_update = prepared
            .point_update
            .as_ref()
            .is_some_and(|pu| pu.is_docs_wing_only)
            && self.store.deferred_doc_wing_updates.len()
                < self.store.deferred_doc_wing_updates.inline_size();
        if (prepared.point_delete.is_none() && !self.store.fts_deferred_doc_swaps.is_empty())
            || (!self.store.deferred_doc_wing_updates.is_empty() && !can_coalesce_wing_update)
        {
            self.store.flush_fts_deferred_docs();
        }
        // A search over a cold collection whose full-text index an earlier write dropped:
        // rebuild it from the engine first, so the search does not fall back to loading
        // the collection.
        if !self.store.dirty_fts_disk.is_empty() && self.cold_program_packs.is_none() {
            for stmt in prepared.stmts.iter() {
                if let Stmt::Query(q) = stmt
                    && let Source::Collection(name) = &q.source
                    && q.steps.iter().any(|s| matches!(s, Step::Search { .. }))
                    && self.store.dirty_fts_disk.contains_key(name)
                {
                    self.store.rebuild_dirty_disk_fts(name)?;
                }
            }
        }
        if let Some(pd) = &prepared.point_delete {
            if !pd.append_only && !pd.cas_each && pd.cas.is_none() {
                return Err(Error::runtime("delete requires cas"));
            }
            if self.disk_overlay_point_dml(&pd.collection) {
                return self.exec_cold_point_delete(pd, &prepared.plan, want_rows);
            }
            if self.store.is_disk_backed(&pd.collection) {
                self.store.promote_disk_col(&pd.collection);
            }
            if !self.is_durable()
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
            if !pd.append_only && !pd.cas_each {
                let rows = self.store.collection(&pd.collection);
                let got = row_text(&rows[idx], "hash").unwrap_or("");
                let cas = pd.cas.as_deref().unwrap_or("");
                if got != cas {
                    return Err(Error::runtime(format!(
                        "cas mismatch: expected {cas:?}, found {got:?}"
                    )));
                }
            }
            if !self.is_durable()
                && pd.collection == "docs"
                && (!want_rows || self.store.collection("docs").len() >= 64)
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
                self.store.durable_commit(p, pack)?;
            } else if self.store.is_native_disk() {
                let pack = Pack::Delete {
                    collection: pd.collection.clone(),
                    rows: out.clone(),
                };
                self.store.commit_pack_to_native_disk(&pack)?;
            }
            self.store.r#gen += 1;
            if let Some(p) = self.persist.as_mut() {
                self.store.maybe_checkpoint(p)?;
            }
            return Ok(Handle {
                plan: Arc::clone(&prepared.plan),
                rows: if want_rows { out } else { Vec::new() },
                done: Done {
                    r#gen: self.store.r#gen,
                    n: 1,
                },
                message: None,
                ms: 0.0,
            });
        }
        if let Some(pu) = &prepared.point_update {
            if !pu.cas_each && pu.cas.is_none() {
                return Err(Error::runtime("update requires cas"));
            }
            // Assigning a unique-indexed field needs a uniqueness check against the whole
            // collection, which only the promoted in-memory index provides.
            if self.disk_overlay_point_dml(&pu.collection)
                && !pu.record.fields.iter().any(|(k, _)| {
                    self.store.unique_key_needs_memory(&pu.collection, k)
                })
            {
                return self.exec_cold_point_update(pu, &prepared.plan, want_rows);
            }
            if self.store.is_disk_backed(&pu.collection) {
                self.store.promote_disk_col(&pu.collection);
            }
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
            if self.can_fast_docs_wing_update(pu) {
                let (_, _, out_rows) =
                    self.exec_docs_wing_update_core(pu, idx, want_rows, false)?;
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
            if !pu.cas_each {
                let got = row_text(&rows[idx], "hash").unwrap_or("");
                let cas = pu.cas.as_deref().unwrap_or("");
                if got != cas {
                    return Err(Error::runtime(format!(
                        "cas mismatch: expected {cas:?}, found {got:?}"
                    )));
                }
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
                            return Err(Error::runtime(format!(
                                "immutable field: {}.{k}",
                                pu.collection
                            )));
                        }
                    }
                }
            }
            self.check_update_id(&pu.collection, &pu.record, &[idx], now)?;
            let before = before_ref.clone();
            let mut updated = before.clone();
            for (k, v) in &pu.record.fields {
                updated.insert(k.clone(), value_cell(v, now));
            }
            let has_body = pu.record.fields.iter().any(|(k, _)| k == "body");
            if has_body && let Some(body) = row_text(&updated, "body") {
                let h = content_hash_inline(body);
                updated.insert("hash", Cell::SmallText(h));
            }
            // A changed title / body / snippet is embedded again (see the general update).
            if self.embedder.is_some()
                && pu.record.fields.iter().any(|(k, _)| matches!(k.as_str(), "title" | "body" | "snippet"))
                && !pu.record.fields.iter().any(|(k, _)| k == "embedding")
            {
                updated.remove("embedding");
                self.maybe_embed_rows(&pu.collection, std::slice::from_mut(&mut updated));
            }
            self.check_row_fks(&pu.collection, &updated)?;
            if let Some(rows) = self.store.collections.get_mut(&pu.collection) {
                rows[idx] = updated.clone();
            }
            if let Err(e) = self
                .store
                .index_update_row(&pu.collection, idx, &before, &updated)
            {
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
                self.store.durable_commit(p, pack)?;
            } else if self.store.is_native_disk() {
                let pack = Pack::Update {
                    collection: pu.collection.clone(),
                    rows: vec![updated.clone()],
                };
                self.store.commit_pack_to_native_disk(&pack)?;
            }
            self.store.r#gen += 1;
            if let Some(p) = self.persist.as_mut() {
                self.store.maybe_checkpoint(p)?;
            }
            return Ok(Handle {
                plan: Arc::clone(&prepared.plan),
                rows: if want_rows { vec![updated] } else { Vec::new() },
                done: Done {
                    r#gen: self.store.r#gen,
                    n: 1,
                },
                message: None,
                ms: 0.0,
            });
        }
        if let Some(pf) = &prepared.append_facts
            && (!want_rows || pf.s.len() > 128)
            && self.cold_program_packs.is_none()
            && !self.store.is_disk_backed("facts")
            && !self.store.is_native_disk()
            && self.catalog.collections.contains_key("facts")
        {
            let t0 = Instant::now();
            let base_gen = self.store.r#gen;
            let prev_facts_len = self.store.facts_len();
            let changed = self.store.append_prepared_facts(&pf.s, &pf.p, &pf.o);
            if changed == 0 {
                return Ok(Handle {
                    plan: Arc::clone(&prepared.plan),
                    rows: Vec::new(),
                    done: Done {
                        r#gen: self.store.r#gen,
                        n: 0,
                    },
                    message: Some("idempotent".into()),
                    ms: t0.elapsed().as_secs_f64() * 1000.0,
                });
            }
            if let Err(e) = self.check_quotas() {
                self.store.rollback_facts_append(prev_facts_len);
                self.store.r#gen = base_gen;
                return Err(e);
            }
            if let Some(p) = self.persist.as_mut() {
                if p.log_bytes > self.quotas.max_log_bytes
                    && let Err(e) = self.store.checkpoint(p)
                {
                    self.store.rollback_facts_append(prev_facts_len);
                    self.store.r#gen = base_gen;
                    return Err(e);
                }
                if let Err(e) = self.store.durable_commit_facts_preencoded(
                    p,
                    pf.s.len() as u32,
                    &pf.encoded_spo,
                ) {
                    self.store.rollback_facts_append(prev_facts_len);
                    self.store.r#gen = base_gen;
                    return Err(e);
                }
            }
            self.store.r#gen += 1;
            if let Some(p) = self.persist.as_mut() {
                self.store.maybe_checkpoint(p)?;
            }
            let ms = t0.elapsed().as_secs_f64() * 1000.0;
            let n = pf.s.len();
            self.append_rows += n as u64;
            self.append_ms += ms;
            return Ok(Handle {
                plan: Arc::clone(&prepared.plan),
                rows: Vec::new(),
                done: Done {
                    r#gen: self.store.r#gen,
                    n,
                },
                message: None,
                ms,
            });
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
            let t0 = Instant::now();
            let now = if agg.needs_now { now_ms() } else { 0 };
            let rows = self.execute_agg_plan(agg, now)?;
            let ms = t0.elapsed().as_secs_f64() * 1000.0;
            let n = rows.len();
            return Ok(Handle {
                plan: prepared.plan.clone(),
                rows: if want_rows { rows } else { Vec::new() },
                done: Done {
                    r#gen: self.store.r#gen,
                    n,
                },
                message: None,
                ms,
            });
        }

        self.run_bound_stmts(
            &prepared.stmts,
            &prepared.plan,
            prepared.writes,
            prepared.append_only,
            prepared.schema,
            want_rows,
        )
    }

    pub(crate) fn run_bound_stmts(
        &mut self,
        stmts: &[Stmt],
        plan: &Arc<Plan>,
        writes: bool,
        append_only: bool,
        schema: bool,
        want_rows: bool,
    ) -> Result<Handle, Error> {
        if self.follower && writes {
            return Err(Error::runtime(
                "follower: read-only — apply_wal for replication, open primary for writes",
            ));
        }
        if !schema
            && self.cold_program_packs.is_none()
            && let Some(h) = self.cold_overlay_program(stmts, plan, want_rows)?
        {
            return Ok(h);
        }
        // A lone `delete` on a cold collection only needs the matching rows: read them through
        // the engine and tombstone them in the overlay instead of promoting the collection.
        if let [
            Stmt::Delete {
                collection,
                pred,
                cas,
                cas_each,
            },
        ] = stmts
            && self.disk_overlay_point_dml(collection)
        {
            return self.cold_overlay_delete(
                collection,
                pred,
                cas.as_deref(),
                *cas_each,
                None,
                plan,
                want_rows,
            );
        }
        // Same for a lone `update`, as long as it cannot change a unique key (that needs a
        // collection-wide uniqueness check, which the promoted in-memory index provides).
        if let [
            Stmt::Update {
                collection,
                pred,
                cas,
                cas_each,
                record,
            },
        ] = stmts
            && self.disk_overlay_point_dml(collection)
            && !record.fields.iter().any(|(k, _)| {
                self.store.unique_key_needs_memory(collection, k)
            })
        {
            return self.cold_overlay_update(
                collection,
                pred.as_ref(),
                cas.as_deref(),
                *cas_each,
                None,
                record,
                plan,
                want_rows,
            );
        }
        if let [
            Stmt::Insert {
                collection,
                records,
                edges,
            },
        ] = stmts
            && !records.is_empty()
            && records.len() <= COLD_OVERLAY_INSERT_MAX_ROWS
            && self.disk_overlay_point_dml(collection)
            && let Some(h) =
                self.cold_overlay_insert_edges(collection, records, edges, None, plan, want_rows)?
        {
            return Ok(h);
        }
        let base_gen = self.store.r#gen;
        let t0 = Instant::now();
        // Writes to a cold collection need a full backup (see `stmts_write_disk_collection`).
        let touches_disk = writes && tx::stmts_write_disk_collection(&self.store, stmts);
        let mut undo = if !writes {
            Undo::None
        } else if append_only && !schema && !touches_disk {
            Undo::Append(self.store.append_mark())
        } else if !schema
            && !touches_disk
            && stmts.iter().all(|s| {
                matches!(
                    s,
                    Stmt::Update { .. } | Stmt::Delete { .. } | Stmt::Query(_) | Stmt::Let { .. }
                )
            })
        {
            Undo::Rows(smallvec::SmallVec::new())
        } else {
            Undo::Full(Box::new(self.store.mem_backup()))
        };
        let cat_backup = if matches!(undo, Undo::Full(_)) || schema {
            Some(Box::new(self.catalog.clone()))
        } else {
            None
        };

        let ((rows, message, pack, wrote, affected_hint), prepared_encoding) =
            match self.exec_program_ext(stmts, &mut undo, true, want_rows) {
                Ok(v) => v,
                Err(e) => {
                    self.rollback(undo, cat_backup, base_gen);
                    return Err(e);
                }
            };
        let ms = t0.elapsed().as_secs_f64() * 1000.0;
        let n = if !rows.is_empty() {
            rows.len()
        } else if affected_hint > 0 {
            affected_hint
        } else {
            pack_affect_n(pack.as_ref()).unwrap_or(0)
        };
        if wrote {
            if let Err(e) = self.check_quotas() {
                self.rollback(undo, cat_backup, base_gen);
                return Err(e);
            }
            if let Some(pack) = pack {
                if let Some(p) = self.persist.as_mut()
                    && p.log_bytes > self.quotas.max_log_bytes
                    && let Err(e) = self.store.checkpoint(p)
                {
                    self.rollback(undo, cat_backup, base_gen);
                    return Err(e);
                }
                if let Some(p) = self.persist.as_mut() {
                    if schema {
                        p.catalog_hash = self.catalog.hash_str().to_string();
                    }
                    if let Err(e) = self
                        .store
                        .durable_commit_prepared(p, pack, prepared_encoding)
                    {
                        self.rollback(undo, cat_backup, base_gen);
                        return Err(e);
                    }
                } else if self.store.is_native_disk() {
                    if let Err(e) = self.store.commit_pack_to_native_disk(&pack) {
                        self.rollback(undo, cat_backup, base_gen);
                        return Err(e);
                    }
                }
            }
            self.store.r#gen += 1;
            if matches!(undo, Undo::Full(_)) {
                self.store.commit_disk_overlay_savepoint();
            }
            if let Some(p) = self.persist.as_mut() {
                self.store.maybe_checkpoint(p)?;
            }
            if append_only {
                self.append_rows += n as u64;
                self.append_ms += ms;
            }
        } else if matches!(undo, Undo::Full(_)) {
            self.store.commit_disk_overlay_savepoint();
        }
        if schema {
            self.clear_plan_cache();
        }
        if !rows.is_empty() {
            self.check_output_quota(&rows)?;
        }
        Ok(Handle {
            plan: Arc::clone(plan),
            rows,
            done: Done {
                r#gen: self.store.r#gen,
                n,
            },
            message,
            ms,
        })
    }

    /// Shared DRY columnar OLAP dispatcher: filter/join/window/sort + project → [`RecordBatch`].
    pub(crate) fn try_columnar_query_batch(&self, q: &Query) -> Option<RecordBatch> {
        let q = crate::catalog::with_catalog_filter(q, &self.catalog);
        let now = now_ms();
        let bindings = Default::default();
        if !self.uses_gpu_filter(&q) && !self.uses_gpu_join(&q) {
            if let Some(batch) = self.try_filter_project_batch(&q, &bindings, now) {
                return Some(batch);
            }
            if let Some(batch) = self.try_join_project_batch(&q, &bindings, now) {
                return Some(batch);
            }
            if let Some(batch) = self.try_window_project_batch(&q, &bindings, now) {
                return Some(batch);
            }
        }
        if !self.uses_gpu_filter(&q)
            && let Some(batch) = self.try_sort_project_batch(&q, &bindings, now)
        {
            return Some(batch);
        }
        None
    }

    /// Columnar read path: FK join+project, filter+project, window+project, or sort+project → [`RecordBatch`].
    pub fn run_prepared_batch(&mut self, prepared: &Prepared) -> Result<RecordBatch, Error> {
        if prepared.writes {
            return Err(Error::runtime("run_batch is read-only"));
        }
        if prepared.stmts.len() == 1
            && let Stmt::Query(q) = &prepared.stmts[0]
            && let Some(batch) = self.try_columnar_query_batch(q)
        {
            return Ok(batch);
        }
        let h = self.run_prepared(prepared)?;
        Ok(match &prepared.stmts[..] {
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
        if let Some(h) = self.try_fast_docs_id_title_get(prepared) {
            return Ok(h);
        }
        if let Some(pg) = &prepared.point_get {
            let row = self.execute_point_get(pg);
            let (rows, n) = match row {
                Some(r) => (crate::store::wrap_single_row_vec(r), 1),
                None => (Vec::new(), 0),
            };
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
            && prepared.stmts.first().is_some_and(|s| match s {
                Stmt::Query(q) => self.uses_gpu_filter(q) || self.uses_gpu_count(q),
                _ => false,
            });
        #[cfg(not(feature = "gpu"))]
        let has_gpu_agg = false;

        if !has_gpu_agg && let Some(agg) = &prepared.agg_plan {
            let t0 = Instant::now();
            let now = if agg.needs_now { now_ms() } else { 0 };
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
        self.run_bound_stmts_readonly(&prepared.stmts, &prepared.plan)
    }

    pub(crate) fn run_bound_stmts_readonly(
        &self,
        stmts: &[Stmt],
        plan: &Arc<Plan>,
    ) -> Result<Handle, Error> {
        let t0 = Instant::now();
        let (rows, message, pack) = self.exec_program_readonly(stmts)?;
        debug_assert!(pack.is_none());
        self.check_output_quota(&rows)?;
        let ms = t0.elapsed().as_secs_f64() * 1000.0;
        let n = rows.len();
        Ok(Handle {
            plan: Arc::clone(plan),
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
    pub(crate) fn check_output_quota(&self, rows: &[Row]) -> Result<(), Error> {
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
        let out_bytes = rows.iter().map(crate::store::row_byte_size).sum::<usize>();
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

    pub fn run(&mut self, src: &str) -> Result<Handle, Error> {
        self.run_src_ext(src, true)
    }

    /// Write / batch verb: `n` + `gen` without keeping the row buffer.
    pub fn execute(&mut self, src: &str) -> Result<Done, Error> {
        Ok(self.run_src_ext(src, false)?.done)
    }

    fn run_src_ext(&mut self, src: &str, want_rows: bool) -> Result<Handle, Error> {
        if let Some(prep) = self.plan_cache.get(src).cloned() {
            return self.run_prepared_ext(&prep, want_rows);
        }
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
        self.run_preparsed_ext(src, stmts, want_rows)
    }

    /// Execute `src` when `stmts` was already parsed on a caller thread (e.g., `LinServer`),
    /// consulting `plan_cache` first and avoiding redundant row/plan materialization when `!want_rows`.
    pub(crate) fn run_preparsed_ext(
        &mut self,
        src: &str,
        stmts: Vec<Stmt>,
        want_rows: bool,
    ) -> Result<Handle, Error> {
        if let Some(prep) = self.plan_cache.get(src).cloned() {
            return self.run_prepared_ext(&prep, want_rows);
        }
        if src.len() > self.quotas.max_query_bytes {
            return Err(Error::new(format!(
                "query length {} exceeds max_query_bytes quota {}",
                src.len(),
                self.quotas.max_query_bytes
            )));
        }
        let ast_bytes = crate::ast::estimate_ast_bytes(&stmts);
        if ast_bytes > self.quotas.max_ast_bytes {
            return Err(Error::new(format!(
                "prepared AST size {} exceeds max_ast_bytes quota {}",
                ast_bytes, self.quotas.max_ast_bytes
            )));
        }
        let writes = stmts.iter().any(stmt_writes);
        let schema = stmts.iter().any(stmt_schema);
        if !writes || schema || self.note_dml_seen(src) {
            let mut prep = self.prepare_stmts_checked(stmts)?;
            prep.src = src.to_string();
            self.insert_plan_cache(src.to_string(), prep.clone());
            return self.run_prepared_ext(&prep, want_rows);
        }
        // A single point `update` / `delete` on a cold collection has an overlay fast path
        // only in its prepared form; the generic statement executor would promote the whole
        // collection into memory first.
        if let [Stmt::Update { collection, .. } | Stmt::Delete { collection, .. }] = stmts.as_slice()
            && self.disk_overlay_point_dml(collection)
        {
            let prep = self.prepare_stmts_checked(stmts.clone())?;
            if prep.point_update.is_some() || prep.point_delete.is_some() {
                return self.run_prepared_ext(&prep, want_rows);
            }
        }
        let (_, staged) = check::check_program(&stmts, &self.catalog)?;
        let plan = if want_rows {
            let plan_cat = staged.as_ref().unwrap_or(&self.catalog);
            Arc::new(plan::plan_program(&stmts, plan_cat)?)
        } else {
            dummy_dml_plan()
        };
        let append_only = stmts.iter().all(|s| stmt_append_only(s) || !stmt_writes(s));
        self.run_bound_stmts(&stmts, &plan, writes, append_only, schema, want_rows)
    }

    /// Scalar verb: first cell of the first row (`count`, `take 1` project).
    pub fn scalar(&mut self, src: &str) -> Result<Cell, Error> {
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

    /// Insert the rows of a [`RecordBatch`] into `collection` as one atomic statement.
    ///
    /// This is the typed counterpart of `insert <collection> [...]`: no query text is built
    /// or parsed, and the rows go through exactly the same checks and paths as that
    /// statement (type check, generated ids and content hashes, uniqueness, foreign keys,
    /// embeddings, WAL, cold overlay). Null cells mean "field absent". Vector columns are
    /// rejected: embeddings are produced by the configured embedder, not supplied.
    pub fn insert_batch(&mut self, collection: &str, batch: &RecordBatch) -> Result<Done, Error> {
        let n = batch.n();
        if batch.cols.iter().any(|c| c.len() != n) {
            return Err(Error::runtime("insert_batch: columns have different lengths"));
        }
        if n == 0 {
            return Ok(Done {
                r#gen: self.store.r#gen,
                n: 0,
            });
        }
        if let Some(done) = self.insert_batch_cold(collection, batch)? {
            return Ok(done);
        }
        if let Some(done) = self.insert_batch_rows(collection, batch)? {
            return Ok(done);
        }
        let mut records: Vec<crate::ast::Record> = (0..n)
            .map(|_| crate::ast::Record {
                fields: Vec::with_capacity(batch.cols.len()),
            })
            .collect();
        for (name, col) in batch.names.iter().zip(&batch.cols) {
            for (i, record) in records.iter_mut().enumerate() {
                let value = match col.get(i) {
                    Cell::Null => continue,
                    Cell::Int(v) => Value::Int(v),
                    Cell::Float(v) => Value::Float(v),
                    Cell::Bool(v) => Value::Bool(v),
                    Cell::Time(v) => Value::Timestamp(v),
                    Cell::Vec(_) => {
                        return Err(Error::runtime(format!(
                            "insert_batch: vector column {name} is not supported"
                        )));
                    }
                    text => match text.text_shared() {
                        Some(s) => Value::String(s),
                        None => continue,
                    },
                };
                record.fields.push((name.clone(), value));
            }
        }
        // No source text is involved, so the parser's AST size quota does not apply: the
        // caller already holds the batch in memory.
        Ok(self
            .run_prepared_ext(
                &self.prepare_stmts_checked(vec![Stmt::Insert {
                    collection: collection.to_string(),
                    records,
                    edges: Vec::new(),
                }])?,
                false,
            )?
            .done)
    }

    /// Type-check a batch for `insert` into `collection` exactly as the checker would
    /// check the equivalent statement. `None` when every cell is null; otherwise whether
    /// some cell is null (an absent field).
    fn check_batch_types(
        &self,
        collection: &str,
        batch: &RecordBatch,
    ) -> Result<Option<bool>, Error> {
        // The checker's type rules depend only on the kind of each value, so one record
        // per (column, kind of cell) with a placeholder value reproduces exactly what it
        // would say about the batch.
        use crate::batch::Column;
        let mut probes: Vec<crate::ast::Record> = Vec::new();
        let mut has_null = false;
        for (name, col) in batch.names.iter().zip(&batch.cols) {
            let mut kinds: smallvec::SmallVec<[Value; 2]> = smallvec::SmallVec::new();
            match col {
                Column::Nulls { .. } => has_null = true,
                Column::Int { valid, .. } => {
                    has_null |= valid.null_count() > 0;
                    kinds.push(Value::Int(0));
                }
                Column::Float { valid, .. } => {
                    has_null |= valid.null_count() > 0;
                    kinds.push(Value::Float(0.0));
                }
                Column::Bool { valid, .. } => {
                    has_null |= valid.null_count() > 0;
                    kinds.push(Value::Bool(false));
                }
                Column::Time { valid, .. } => {
                    has_null |= valid.null_count() > 0;
                    kinds.push(Value::Timestamp(0));
                }
                Column::Cells(cells) => {
                    let mut seen = [false; 5];
                    for cell in cells {
                        let (slot, value) = match cell {
                            Cell::Null => {
                                has_null = true;
                                continue;
                            }
                            Cell::Text(_) | Cell::SmallText(_) => (0, Value::String(Arc::from(""))),
                            Cell::Int(_) => (1, Value::Int(0)),
                            Cell::Float(_) => (2, Value::Float(0.0)),
                            Cell::Bool(_) => (3, Value::Bool(false)),
                            Cell::Time(_) => (4, Value::Timestamp(0)),
                            Cell::Vec(_) => {
                                return Err(Error::runtime(format!(
                                    "insert_batch: vector column {name} is not supported"
                                )));
                            }
                        };
                        if !seen[slot] {
                            seen[slot] = true;
                            kinds.push(value);
                        }
                    }
                }
            }
            for value in kinds {
                probes.push(crate::ast::Record {
                    fields: vec![(name.clone(), value)],
                });
            }
        }
        if probes.is_empty() {
            return Ok(None);
        }
        check::check_program(
            &[Stmt::Insert {
                collection: collection.to_string(),
                records: probes,
                edges: Vec::new(),
            }],
            &self.catalog,
        )?;
        Ok(Some(has_null))
    }

    /// [`Self::insert_batch`] into a cold collection, straight from the columns: the rows
    /// are built from the batch without a `Record` per row and go to the engine overlay.
    /// `None` when the statement path must handle the batch (collection in memory, inside
    /// a program, keys this path does not reproduce).
    fn insert_batch_cold(
        &mut self,
        collection: &str,
        batch: &RecordBatch,
    ) -> Result<Option<Done>, Error> {
        let n = batch.n();
        if self.follower
            || n > COLD_OVERLAY_INSERT_MAX_ROWS
            || self.cold_program_packs.is_some()
            || !self.disk_overlay_point_dml(collection)
        {
            return Ok(None);
        }
        let Some(has_null) = self.check_batch_types(collection, batch)? else {
            // Every cell is null: a record of absent fields; let the statement path decide.
            return Ok(None);
        };
        if self.store.has_deferred_doc_ops() {
            self.store.flush_fts_deferred_docs();
        }
        let mut rows = batch.to_rows();
        if has_null {
            for row in &mut rows {
                row.retain(|_, cell| !matches!(cell, Cell::Null));
            }
        }
        let plan = tx::dummy_dml_plan();
        Ok(self
            .cold_overlay_insert_rows(collection, rows, &[], 0, None, &plan, false)?
            .map(|h| h.done))
    }

    /// [`Self::insert_batch`] into an in-memory collection: the rows are built straight
    /// from the columns and run as an `InsertRows` statement, so they go through the same
    /// insert, commit, log and undo path as `insert` without a `Record` per row.
    fn insert_batch_rows(
        &mut self,
        collection: &str,
        batch: &RecordBatch,
    ) -> Result<Option<Done>, Error> {
        if self.store.is_disk_backed(collection) || self.cold_program_packs.is_some() {
            return Ok(None);
        }
        let Some(has_null) = self.check_batch_types(collection, batch)? else {
            return Ok(None);
        };
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
        let now = now_ms();
        let (rows, shape) = if !has_null
            && let Some(built) = dml::build_batch_insert_rows(batch, needs_embed, hashing_dim, now)
        {
            built
        } else {
            let mut rows = batch.to_rows();
            if has_null {
                for row in &mut rows {
                    row.retain(|_, cell| !matches!(cell, Cell::Null));
                }
            }
            let shape = dml::BuiltShape {
                // Without nulls every row has the batch's fields, in one sorted order.
                all_uniform: !has_null,
                all_embedded: !needs_embed,
                needs_id: rows.iter().any(|r| r.get("id").and_then(Cell::text).is_none()),
                lossless_cols: false,
                hashing_dim,
                now,
            };
            (rows, shape)
        };
        let n = rows.len();
        let prepared = self.prepare_stmts_checked(vec![Stmt::InsertRows {
            collection: collection.to_string(),
            rows: n,
        }])?;
        self.pending_insert_rows = Some((rows, shape));
        let result = self.run_prepared_ext(&prepared, false);
        // Never leave rows staged for a later statement, whatever happened.
        self.pending_insert_rows = None;
        Ok(Some(result?.done))
    }

    /// Keep a cold collection physically ordered by `field` (pass `None` to stop).
    /// Range filters on the cluster key then skip most zone-map blocks. Requires the
    /// collection to be in the disk tier already.
    pub fn set_cluster_key(&mut self, collection: &str, field: Option<&str>) -> Result<(), Error> {
        self.store.set_disk_cluster_key(collection, field)
    }

    /// Compact a cold collection now: checkpoint, reclaim deleted rows, merge its segments
    /// and, when a cluster key is declared, rewrite it in key order across segments.
    pub fn compact_collection(&mut self, collection: &str) -> Result<(), Error> {
        if !self.store.is_disk_backed(collection) {
            return Err(Error::runtime(format!(
                "compact: {collection} is not in the disk tier"
            )));
        }
        self.store.disk_compact_requests.push(collection.to_string());
        self.checkpoint()
    }

    /// Cluster key declared for a cold collection, if any.
    pub fn cluster_key(&self, collection: &str) -> Option<String> {
        self.store.disk_cluster_key(collection)
    }

    /// Keep a columnar mirror of `fields` of an in-memory collection, so filters on
    /// them scan dense codes instead of rows. The mirror is rebuilt on the first read
    /// after any write; it is not persisted. An empty `fields` drops the declaration.
    pub fn declare_columns(&mut self, collection: &str, fields: &[&str]) -> Result<(), Error> {
        if !self.store.collections.contains_key(collection) || self.store.is_disk_backed(collection) {
            return Err(Error::runtime(format!(
                "declare columns: no in-memory collection `{collection}`"
            )));
        }
        self.reader_cache = None;
        self.col_mirrors
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .remove(collection);
        if fields.is_empty() {
            self.declared_cols.remove(collection);
        } else {
            self.declared_cols.insert(
                collection.to_string(),
                fields.iter().map(|f| (*f).to_string()).collect(),
            );
        }
        Ok(())
    }

    fn get_or_build_mirror(&self, name: &str) -> Option<Arc<mirror::ColMirror>> {
        let fields = self.declared_cols.get(name)?;
        if self.store.is_disk_backed(name)
            || (name == "facts" && self.store.facts_rows_stale)
            || (name == "docs" && self.store.has_deferred_doc_ops())
        {
            return None;
        }
        let rows = self.store.collections.get(name)?;
        let epoch = self.store.collections.epoch();
        if let Ok(cache) = self.col_mirrors.read()
            && let Some(m) = cache.get(name)
            && m.epoch == epoch
            && m.len == rows.len()
        {
            return Some(Arc::clone(m));
        }
        let mut cache = self
            .col_mirrors
            .write()
            .unwrap_or_else(|e| e.into_inner());
        match cache.get(name) {
            Some(m) if m.epoch == epoch && m.len == rows.len() => Some(Arc::clone(m)),
            _ => {
                let m = Arc::new(mirror::ColMirror::build(rows, fields, epoch));
                cache.insert(name.to_string(), Arc::clone(&m));
                Some(m)
            }
        }
    }

    /// Positions of the rows of `name` matching `pred`, read from the declared mirror.
    pub(crate) fn mirror_select(&self, name: &str, pred: &Pred, now: i64) -> Option<Vec<usize>> {
        self.get_or_build_mirror(name)?.select(pred, now)
    }

    /// Count of the rows of `name` matching `pred`, read from the declared mirror.
    pub(crate) fn mirror_count(&self, name: &str, pred: &Pred, now: i64) -> Option<usize> {
        self.get_or_build_mirror(name)?.count(pred, now)
    }

    /// Begin an interactive ACID transaction.
    pub fn transaction(&mut self) -> Result<Transaction<'_>, Error> {
        if !self.store.fts_deferred_doc_swaps.is_empty() {
            self.store.flush_fts_deferred_docs();
        }
        let base_gen = self.store.r#gen;
        let base_next_id = self.store.next_id;
        Ok(Transaction {
            db: self,
            base_gen,
            base_next_id,
            committed: false,
            undo: Undo::None,
            cat_backup: None,
            packs: Vec::new(),
            schema_changed: false,
            disk_savepoint: false,
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
        if q.needs_simplify() {
            q.to_mut().simplify_predicates();
        }
        let q = &*q;
        let now = now_ms();

        let gpu_filter = self.uses_gpu_filter(q)
            || self.uses_gpu_count(q)
            || self.uses_gpu_join(q)
            || self.uses_gpu_sort(q)
            || self.uses_gpu_graph(q);
        if !gpu_filter {
            if let Some(rows) = self.try_point_get(q) {
                return Ok(rows);
            }
            if let Some(rows) = self.try_filter_count(q, bindings, now) {
                return Ok(rows);
            }
            if let Some(rows) = self.try_agg(q, bindings, now)? {
                return Ok(rows);
            }
            if let Some(rows) = self.try_summarize_fast(q, bindings, now)? {
                return Ok(rows);
            }
            if let Some(rows) = self.try_filter_project(q, bindings, now) {
                return Ok(rows);
            }
            if let Some(rows) = self.try_fts_search(q, bindings)? {
                return Ok(rows);
            }
            if let Some(rows) = self.try_join_project(q, bindings, now) {
                return Ok(rows);
            }
            if let Some(rows) = self.try_join_agg(q, bindings, now)? {
                return Ok(rows);
            }
            if let Some(rows) = self.try_window_project(q, bindings, now) {
                return Ok(rows);
            }
            if let Some(batch) = self.try_sort_project_batch(q, bindings, now) {
                return Ok(batch.into_rows());
            }
        }

        let mut primary = check::collection_of(&q.source).to_string();
        let mut implicit_take = true;
        let mut saw_agg = false;
        let first_filter = match q.steps.first() {
            Some(Step::Filter(p)) if !gpu_filter => Some(p),
            _ => None,
        };
        let simple_scan = q.steps.iter().all(|s| {
            matches!(
                s,
                Step::Filter(_)
                    | Step::Project(_)
                    | Step::Without(_)
                    | Step::Skip { .. }
                    | Step::Take { .. }
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
        let without_fields_step = {
            let can_exclude_source = (first_filter.is_some()
                && matches!(q.steps.get(1), Some(Step::Without(_)))
                && q.steps
                    .iter()
                    .skip(2)
                    .all(|step| matches!(step, Step::Skip { .. } | Step::Take { .. })))
                || (matches!(q.steps.first(), Some(Step::Without(_)))
                    && q.steps
                        .iter()
                        .skip(1)
                        .all(|step| matches!(step, Step::Skip { .. } | Step::Take { .. })));
            if simple_scan && can_exclude_source {
                q.steps.iter().find_map(|s| match s {
                    Step::Without(f) => Some(f.as_slice()),
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
                    && let Some((field, key)) = point_key(pred)
                    && (field == "id" || (field == "uri" && name == "docs"))
                {
                    let simple_eq = matches!(
                        pred,
                        Pred::Cmp {
                            op: CmpOp::Eq,
                            value: Value::String(_),
                            ..
                        }
                    );
                    if self.store.is_disk_backed(name) {
                        let proj_names = project_fields_step.map(field_names).or_else(|| {
                            without_fields_step
                                .and_then(|wf| self.cold_without_kept(name, wf))
                                .filter(|kept| !kept.is_empty())
                        });
                        let want_proj = if simple_eq {
                            proj_names.as_deref()
                        } else {
                            None
                        };
                        if let Some(row) = self.store.project_by_key(name, field, key, want_proj) {
                            if !simple_eq && !eval_pred(pred, &row, now) {
                                Vec::new()
                            } else if !simple_eq
                                && project_fields_step.is_some()
                                && let Some(ref fs) = proj_names
                            {
                                vec![project_fields(&row, fs)]
                            } else {
                                vec![row]
                            }
                        } else {
                            Vec::new()
                        }
                    } else if let Some(idx) = self.store.point_index(name, field, key) {
                        if simple_eq
                            && project_fields_step.is_none()
                            && matches!(q.steps.get(1), Some(Step::Graph { .. } | Step::Hop { .. }))
                        {
                            static ID_URI_FIELDS: std::sync::LazyLock<[String; 2]> =
                                std::sync::LazyLock::new(|| ["id".to_string(), "uri".to_string()]);
                            self.store
                                .project_by_key(name, field, key, Some(&*ID_URI_FIELDS))
                                .into_iter()
                                .collect()
                        } else {
                            self.fetch_idxs_filtered(
                                name,
                                std::slice::from_ref(&idx),
                                pred,
                                now,
                                project_fields_step,
                                simple_eq,
                            )
                        }
                    } else {
                        Vec::new()
                    }
                } else if let Some(pred) = first_filter
                    && !self.store.is_disk_backed(name)
                    && let Some((uses, covered)) =
                        crate::index::pick_index_with_coverage(&self.catalog, name, pred)
                    && let Some(idxs) = self.store.index_seek(name, &uses, now)
                {
                    self.fetch_idxs_filtered(name, &idxs, pred, now, project_fields_step, covered)
                } else if let Some(pred) = first_filter {
                    let col_len = self.store.get_col_count(name);
                    if col_len > self.quotas.max_execution_work {
                        return Err(Error::runtime(format!(
                            "query execution work {} exceeds max_execution_work quota {}",
                            col_len, self.quotas.max_execution_work
                        )));
                    }
                    if let Some(wf) = without_fields_step
                        && let Some(rows) = self.scan_cold_without(name, wf, Some(pred), now, None)
                    {
                        rows
                    } else if let Some(idxs) = self.mirror_select(name, pred, now) {
                        self.fetch_idxs_filtered(name, &idxs, pred, now, project_fields_step, true)
                    } else {
                        self.filter_scan(name, pred, now, project_fields_step)
                    }
                } else if let Some(fields) = project_fields_step {
                    self.scan_projected(name, fields, scan_limit)
                } else if let Some(wf) = without_fields_step
                    && let Some(rows) = self.scan_cold_without(name, wf, None, now, scan_limit)
                {
                    rows
                } else {
                    self.scan(name, scan_limit)
                }
            }
        };

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
                    let mut slots = eval::PredSlots::new(pred);
                    rows.retain(|r| slots.eval(r, now));
                }
                Step::Project(fields) => {
                    if skip_first_project {
                        skip_first_project = false;
                        continue;
                    }
                    rows = project_owned(rows, fields);
                }
                Step::Without(fields) => {
                    rows = without_rows(rows, fields);
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
                        Some(by) => agg_count(
                            &rows,
                            by,
                            self.quotas.max_aggregate_bytes,
                            self.quotas.spill_enabled,
                        )?,
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
                    rows = agg_sum(
                        &rows,
                        field,
                        by.as_ref(),
                        self.quotas.max_aggregate_bytes,
                        self.quotas.spill_enabled,
                    )?;
                    primary = String::new();
                }
                Step::Min { field, by } => {
                    implicit_take = false;
                    saw_agg = true;
                    rows = agg_min(
                        &rows,
                        field,
                        by.as_ref(),
                        self.quotas.max_aggregate_bytes,
                        self.quotas.spill_enabled,
                    )?;
                    primary = String::new();
                }
                Step::Max { field, by } => {
                    implicit_take = false;
                    saw_agg = true;
                    rows = agg_max(
                        &rows,
                        field,
                        by.as_ref(),
                        self.quotas.max_aggregate_bytes,
                        self.quotas.spill_enabled,
                    )?;
                    primary = String::new();
                }
                Step::Avg { field, by } => {
                    implicit_take = false;
                    saw_agg = true;
                    rows = agg_avg(
                        &rows,
                        field,
                        by.as_ref(),
                        self.quotas.max_aggregate_bytes,
                        self.quotas.spill_enabled,
                    )?;
                    primary = String::new();
                }
                Step::Summarize { specs, by } => {
                    implicit_take = false;
                    saw_agg = true;
                    let sum_f64_flags =
                        agg::sum_f64_flags_for(self.catalog.collections.get(&primary), specs);
                    rows = agg::agg_summarize(
                        &rows,
                        None,
                        now,
                        specs,
                        by,
                        &sum_f64_flags,
                        self.quotas.max_aggregate_bytes,
                    )?;
                    primary = String::new();
                }
                Step::ArgExtremum { is_max, order, by } => {
                    implicit_take = false;
                    saw_agg = true;
                    rows = agg::agg_arg_extremum(
                        &rows,
                        None,
                        now,
                        *is_max,
                        order,
                        by,
                        self.quotas.max_aggregate_bytes,
                    )?;
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
                    let mut explicit_take_all = false;
                    let mut j = step_idx + 1;
                    while j < q.steps.len() {
                        match &q.steps[j] {
                            Step::Skip { n } => {
                                look_skip = look_skip.saturating_add((*n).max(0) as usize);
                                j += 1;
                            }
                            Step::Project(_) | Step::Without(_) => {
                                j += 1;
                            }
                            Step::Take { n: Some(n) } => {
                                look_take = Some((*n).max(0) as usize);
                                break;
                            }
                            Step::Take { n: None } => {
                                explicit_take_all = true;
                                break;
                            }
                            _ => break,
                        }
                    }
                    let top_k = if let Some(t) = look_take {
                        Some(look_skip.saturating_add(t))
                    } else if j == q.steps.len()
                        && allow_implicit_take
                        && implicit_take
                        && !saw_agg
                        && !explicit_take_all
                    {
                        Some(look_skip.saturating_add(50))
                    } else {
                        None
                    };
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

    pub(super) fn filter_scan(
        &self,
        name: &str,
        pred: &Pred,
        now: i64,
        project: Option<&[Field]>,
    ) -> Vec<Row> {
        let names = project.map(field_names);
        if self.store.is_disk_backed(name)
            && let Some(eng) = self.store.disk_engine_read()
        {
            let proj_refs: Option<Vec<&str>> = names
                .as_ref()
                .map(|fs| fs.iter().map(String::as_str).collect());
            if let Ok(mut rows) =
                eng.query_filter(name, Some(pred), None, proj_refs.as_deref(), now)
            {
                if let Some(ref fs) = names {
                    for row in &mut rows {
                        if row.len() < fs.len() {
                            for f in fs {
                                if !row.contains_key(f.as_str()) {
                                    row.insert(f.clone(), Cell::Null);
                                }
                            }
                        }
                    }
                }
                return rows;
            }
        }
        if name == "docs"
            && let Pred::Contains { field, needle } = pred
            && field.leaf() == Some("title")
            && let Some(ns) = names.as_deref()
            && let Some(rows) = self.store.scan_docs_hot_project(ns, Some(needle.as_str()))
        {
            return rows;
        }
        let col = self.store.collection(name);
        if col.len() >= agg::PAR_AGG_MIN_ROWS {
            let threads = std::thread::available_parallelism()
                .map(|p| p.get())
                .unwrap_or(1)
                .min(8)
                .min((col.len() + agg::PAR_AGG_CHUNK_ROWS - 1) / agg::PAR_AGG_CHUNK_ROWS);
            if threads > 1 {
                let chunk_size = (col.len() + threads - 1) / threads;
                let names_ref = names.as_deref();
                let chunks: Vec<Vec<Row>> = std::thread::scope(|scope| {
                    let mut handles = Vec::with_capacity(threads);
                    for chunk in col.chunks(chunk_size) {
                        handles.push(scope.spawn(move || {
                            let mut local = Vec::new();
                            let mut slots = eval::PredSlots::new(pred);
                            let mut proj = names_ref.map(crate::store::RowProjector::new);
                            for r in chunk {
                                if slots.eval(r, now) {
                                    local.push(match proj.as_mut() {
                                        Some(p) => p.project(r),
                                        None => r.clone(),
                                    });
                                }
                            }
                            local
                        }));
                    }
                    handles.into_iter().map(|h| h.join().unwrap()).collect()
                });
                let total: usize = chunks.iter().map(Vec::len).sum();
                let mut out = Vec::with_capacity(total);
                for chunk in chunks {
                    out.extend(chunk);
                }
                return out;
            }
        }
        let mut out = Vec::new();
        let mut slots = eval::PredSlots::new(pred);
        let mut proj = names.as_deref().map(crate::store::RowProjector::new);
        for r in col {
            if slots.eval(r, now) {
                out.push(match proj.as_mut() {
                    Some(p) => p.project(r),
                    None => r.clone(),
                });
            }
        }
        out
    }

    pub(super) fn fetch_idxs_filtered(
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
        let mut slots = eval::PredSlots::new(pred);
        let mut proj = names.as_deref().map(crate::store::RowProjector::new);
        for &i in idxs {
            let Some(row) = self.store.get_by_idx(name, i) else {
                continue;
            };
            if !covered && !slots.eval(row, now) {
                continue;
            }
            out.push(match proj.as_mut() {
                Some(p) => p.project(row),
                None => row.clone(),
            });
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
        if self.store.is_disk_backed(name)
            && let Some(eng) = self.store.disk_engine_read()
        {
            if limit == Some(0) {
                return Vec::new();
            }
            let proj_refs: Vec<&str> = names.iter().map(String::as_str).collect();
            let mut out = Vec::new();
            let mut proj = crate::store::RowProjector::new(&names);
            let _ = eng.scan_projected(name, Some(&proj_refs), |r| {
                out.push(proj.project(r));
                if let Some(lim) = limit
                    && out.len() >= lim
                {
                    return Ok(false);
                }
                Ok(true)
            });
            return out;
        }
        let col = self.store.collection(name);
        let count = match limit {
            Some(lim) => lim.min(col.len()),
            None => col.len(),
        };
        let mut out = Vec::with_capacity(count);
        let mut proj = crate::store::RowProjector::new(&names);
        for r in col.iter().take(count) {
            out.push(proj.project(r));
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
        if self.store.is_disk_backed(name)
            && let Some(eng) = self.store.disk_engine_read()
        {
            if limit == Some(0) {
                return Vec::new();
            }
            let mut out = Vec::new();
            let _ = eng.scan_projected(name, None, |r| {
                out.push(r.clone());
                if let Some(lim) = limit
                    && out.len() >= lim
                {
                    return Ok(false);
                }
                Ok(true)
            });
            return out;
        }
        let col = self.store.collection(name);
        match limit {
            Some(lim) => col.iter().take(lim).cloned().collect(),
            None => col.to_vec(),
        }
    }

    fn cold_without_kept(&self, name: &str, w_fields: &[Field]) -> Option<Vec<String>> {
        if !self.store.is_disk_backed(name) || !w_fields.iter().all(|f| f.leaf().is_some()) {
            return None;
        }
        let eng = self.store.disk_engine_read()?;
        let excl: Vec<&str> = w_fields.iter().filter_map(Field::leaf).collect();
        Some(eng.table_columns_excluding(name, &excl))
    }

    fn scan_cold_without(
        &self,
        name: &str,
        w_fields: &[Field],
        pred: Option<&Pred>,
        now: i64,
        limit: Option<usize>,
    ) -> Option<Vec<Row>> {
        let kept = self.cold_without_kept(name, w_fields)?;
        let eng = self.store.disk_engine_read()?;
        let kept_refs: Vec<&str> = kept.iter().map(String::as_str).collect();
        if let Some(p) = pred {
            eng.query_filter(name, Some(p), limit, Some(&kept_refs), now).ok()
        } else {
            if limit == Some(0) {
                return Some(Vec::new());
            }
            let mut out = Vec::new();
            eng.scan_projected(name, Some(&kept_refs), |r| {
                out.push(r.clone());
                if let Some(lim) = limit
                    && out.len() >= lim
                {
                    return Ok(false);
                }
                Ok(true)
            })
            .ok()?;
            Some(out)
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
}

impl Drop for Db {
    fn drop(&mut self) {
        if self.persist.is_some() || self.store.is_native_disk() {
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

    #[inline]
    pub(crate) fn get_cached_prepared(&self, src: &str) -> Option<Prepared> {
        self.plan_cache
            .read()
            .ok()
            .and_then(|guard| guard.get(src).cloned())
    }

    pub(crate) fn prepare_with_stmts(
        &self,
        src: &str,
        stmts: Vec<Stmt>,
    ) -> Result<Prepared, Error> {
        if let Some(p) = self.get_cached_prepared(src) {
            return Ok(p);
        }
        if src.len() > self.inner.quotas.max_query_bytes {
            return Err(Error::new(format!(
                "query length {} exceeds max_query_bytes quota {}",
                src.len(),
                self.inner.quotas.max_query_bytes
            )));
        }
        let mut prepared = self.inner.prepare_stmts(stmts)?;
        prepared.src = src.to_string();
        if let Ok(mut guard) = self.plan_cache.write() {
            if guard.len() >= 1024 {
                guard.clear();
            }
            guard.insert(src.to_string(), prepared.clone());
        }
        Ok(prepared)
    }

    pub fn prepare(&self, src: &str) -> Result<Prepared, Error> {
        if let Some(p) = self.get_cached_prepared(src) {
            return Ok(p);
        }
        let prepared = self.inner.prepare_uncached(src)?;
        if let Ok(mut guard) = self.plan_cache.write() {
            if guard.len() >= 1024 {
                guard.clear();
            }
            guard.insert(src.to_string(), prepared.clone());
        }
        Ok(prepared)
    }

    pub fn prepare_stmt(&self, stmt: Stmt) -> Result<Prepared, Error> {
        self.inner.prepare_stmt(stmt)
    }

    pub fn prepare_stmts(&self, stmts: Vec<Stmt>) -> Result<Prepared, Error> {
        self.inner.prepare_stmts(stmts)
    }

    pub fn run(&self, src: &str) -> Result<Handle, Error> {
        let prepared = self.prepare(src)?;
        self.inner.run_prepared_readonly(&prepared)
    }

    pub fn scalar(&self, src: &str) -> Result<Cell, Error> {
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
            && let Some(batch) = self.inner.try_columnar_query_batch(q)
        {
            return Ok(batch);
        }
        let h = self.run_prepared(prepared)?;
        Ok(match &prepared.stmts[..] {
            [Stmt::Query(q)] => rows_to_query_batch(&h.rows, q),
            _ => rows_to_rough_batch(&h.rows),
        })
    }

    pub fn run_batch(&self, src: &str) -> Result<RecordBatch, Error> {
        let prepared = self.prepare(src)?;
        self.run_prepared_batch(&prepared)
    }

    /// Execute multiple read-only queries concurrently in parallel across CPU cores
    /// against this shared zero-copy [`ReadDb`] snapshot.
    pub fn run_parallel<I, S>(&self, queries: I) -> Result<Vec<Handle>, Error>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let sources: Vec<String> = queries
            .into_iter()
            .map(|s| s.as_ref().to_string())
            .collect();
        match sources.len() {
            0 => Ok(Vec::new()),
            1 => Ok(vec![self.run(&sources[0])?]),
            n => {
                let threads = std::thread::available_parallelism()
                    .map(|p| p.get())
                    .unwrap_or(4)
                    .min(n)
                    .max(1);
                if threads <= 1 {
                    return sources.iter().map(|q| self.run(q)).collect();
                }
                let chunk_size = n.div_ceil(threads);
                let chunks: Vec<&[String]> = sources.chunks(chunk_size).collect();
                let results: Vec<Result<Vec<Handle>, Error>> = std::thread::scope(|s| {
                    let handles: Vec<_> = chunks
                        .into_iter()
                        .map(|chunk| {
                            let db = self;
                            s.spawn(move || -> Result<Vec<Handle>, Error> {
                                let mut out = Vec::with_capacity(chunk.len());
                                for q in chunk {
                                    out.push(db.run(q)?);
                                }
                                Ok(out)
                            })
                        })
                        .collect();
                    handles
                        .into_iter()
                        .map(|h| {
                            h.join().unwrap_or_else(|_| {
                                Err(Error::runtime("parallel query worker panicked"))
                            })
                        })
                        .collect()
                });
                let mut all = Vec::with_capacity(n);
                for r in results {
                    all.extend(r?);
                }
                Ok(all)
            }
        }
    }

    /// Execute multiple pre-prepared read-only queries concurrently across CPU cores
    /// against this shared zero-copy [`ReadDb`] snapshot.
    pub fn run_prepared_parallel(&self, prepared: &[Prepared]) -> Result<Vec<Handle>, Error> {
        match prepared.len() {
            0 => Ok(Vec::new()),
            1 => Ok(vec![self.run_prepared(&prepared[0])?]),
            n => {
                let threads = std::thread::available_parallelism()
                    .map(|p| p.get())
                    .unwrap_or(4)
                    .min(n)
                    .max(1);
                if threads <= 1 {
                    return prepared.iter().map(|p| self.run_prepared(p)).collect();
                }
                let chunk_size = n.div_ceil(threads);
                let chunks: Vec<&[Prepared]> = prepared.chunks(chunk_size).collect();
                let results: Vec<Result<Vec<Handle>, Error>> = std::thread::scope(|s| {
                    let handles: Vec<_> = chunks
                        .into_iter()
                        .map(|chunk| {
                            let db = self;
                            s.spawn(move || -> Result<Vec<Handle>, Error> {
                                let mut out = Vec::with_capacity(chunk.len());
                                for p in chunk {
                                    out.push(db.run_prepared(p)?);
                                }
                                Ok(out)
                            })
                        })
                        .collect();
                    handles
                        .into_iter()
                        .map(|h| {
                            h.join().unwrap_or_else(|_| {
                                Err(Error::runtime("parallel query worker panicked"))
                            })
                        })
                        .collect()
                });
                let mut all = Vec::with_capacity(n);
                for r in results {
                    all.extend(r?);
                }
                Ok(all)
            }
        }
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

pub(crate) fn field_names(fields: &[Field]) -> Vec<String> {
    fields.iter().map(|f| f.as_str()).collect()
}

pub(crate) fn pack_affect_n(pack: Option<&Pack>) -> Option<usize> {
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

fn meta_row(kind: &str, name: &str) -> Row {
    let mut r = Row::new();
    r.insert("kind", Cell::text_arc(kind));
    r.insert("name", Cell::text_arc(name));
    r
}
