use std::collections::BTreeMap;
use std::fs::File;
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use rustc_hash::{FxHashMap, FxHasher};

mod agg;
mod dml;
mod eval;
mod gpu_ops;
mod graph_ops;
mod join;
mod prepared;
mod search;
mod tx;

#[cfg(test)]
mod tests;

pub use crate::persist::{OpenMemOpts as OpenOpts, SyncMode};
pub use prepared::Prepared;
pub use tx::Transaction;

pub(crate) use agg::*;
pub(crate) use eval::*;
pub(crate) use join::{
    JoinFieldPlan, emit_join_row, plan_join_fields, pred_total_gt, project_join_fields,
    rows_to_rough_batch,
};

use dml::{cached_content_hash, pack_has_schema, stmt_append_only, stmt_schema, stmt_writes};
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
    Cell, Row, Store, acquire_row, compact_row, now_ms, project_fields, row_text,
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
    /// Cached read-only snapshot keyed by `(gen, docs_epoch, collections_len, edges_len, facts_len)`.
    pub(crate) reader_cache: Option<((u64, u64, usize, usize, usize), ReadDb)>,
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
    plan_cache: Arc<std::sync::Mutex<FxHashMap<String, Prepared>>>,
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

const PLAN_CACHE_MAX: usize = 512;

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
            quotas: Quotas::default(),
            reopen: ReopenPhases::default(),
            append_rows: 0,
            append_ms: 0.0,
            pins: BTreeMap::new(),
            pulled_wal: Vec::new(),
            follower: false,
            embedder: Some(embedder),
            reader_cache: None,
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

    /// Replace the active embedder. Updates `store.embed_id` / catalog to match.
    pub fn with_embedder(mut self, embedder: Arc<dyn Embedder>) -> Self {
        self.catalog.set_embed_id(embedder.id().to_string());
        self.store.embed_id = embedder.id().to_string();
        self.embedder = Some(embedder);
        self.plan_cache.clear();
        self.query_cache.clear();
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
        self.plan_cache.clear();
        self.query_cache.clear();
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
                quotas: Quotas::default(),
                reopen: store_phases.into(),
                append_rows: 0,
                append_ms: 0.0,
                pins: BTreeMap::new(),
                pulled_wal: Vec::new(),
                follower: false,
                embedder: Some(embedder),
                reader_cache: None,
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
            quotas: Quotas::default(),
            reopen: store_phases.into(),
            append_rows: 0,
            append_ms: 0.0,
            pins: BTreeMap::new(),
            pulled_wal: Vec::new(),
            follower: false,
            embedder: Some(embedder),
            reader_cache: None,
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
            quotas: Quotas::default(),
            reopen: store_phases.into(),
            append_rows: 0,
            append_ms: 0.0,
            pins: BTreeMap::new(),
            pulled_wal: Vec::new(),
            follower: false,
            embedder: Some(embedder),
            reader_cache: None,
            #[cfg(feature = "gpu")]
            gpu: None,
            #[cfg(feature = "gpu")]
            gpu_cache: Default::default(),
        };
        inner.reopen.total_ms = t0.elapsed().as_secs_f64() * 1000.0;
        Ok(ReadDb {
            inner: Arc::new(inner),
            plan_cache: Arc::new(std::sync::Mutex::new(FxHashMap::default())),
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
            self.store.collections.len(),
            self.store.edges.len(),
            self.store.facts_len(),
        );
        if self.store.fts_deferred_doc_swaps.is_empty()
            && let Some((k, cached)) = &self.reader_cache
            && *k == cache_key
        {
            return cached.clone();
        }
        let rdb = ReadDb {
            inner: Arc::new(Self {
                catalog: self.catalog.clone(),
                store: self.store.clone_mem(),
                persist: None,
                reader_lock: None,
                plan_cache: FxHashMap::default(),
                query_cache: FxHashMap::default(),
                seen_dml_hashes: [0; 16],
                quotas: self.quotas,
                reopen: self.reopen,
                append_rows: 0,
                append_ms: 0.0,
                pins: BTreeMap::new(),
                pulled_wal: Vec::new(),
                follower: false,
                embedder: self.embedder.clone(),
                reader_cache: None,
                #[cfg(feature = "gpu")]
                gpu: self.gpu.clone(),
                #[cfg(feature = "gpu")]
                gpu_cache: Default::default(),
            }),
            plan_cache: Arc::new(std::sync::Mutex::new(FxHashMap::default())),
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
            self.plan_cache.clear();
            self.query_cache.clear();
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
            Self::try_extract_point_get(q, plan_cat).map(Arc::new)
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

    pub fn run_prepared(&mut self, prepared: &Prepared) -> Result<Handle, Error> {
        self.run_prepared_ext(prepared, true)
    }

    pub(crate) fn execute_prepared(&mut self, prepared: &Prepared) -> Result<Done, Error> {
        Ok(self.run_prepared_ext(prepared, false)?.done)
    }

    fn run_prepared_ext(&mut self, prepared: &Prepared, want_rows: bool) -> Result<Handle, Error> {
        if self.follower && prepared.writes {
            return Err(Error::runtime(
                "follower: read-only — apply_wal for replication, open primary for writes",
            ));
        }
        if prepared.point_delete.is_none() && !self.store.fts_deferred_doc_swaps.is_empty() {
            self.store.flush_fts_deferred_docs();
        }
        if let Some(pg) = &prepared.point_get {
            let row = self.execute_point_get(pg);
            let rows = match row {
                Some(r) => vec![r],
                None => Vec::new(),
            };
            let n = rows.len();
            return Ok(Handle {
                plan: prepared.plan.clone(),
                rows: if want_rows { rows } else { Vec::new() },
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
            if pu.cas.is_none() {
                return Err(Error::runtime("update requires cas"));
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
            if pu.is_docs_wing_only
                && !self.is_durable()
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
                let (new_cell, wing_arc) = if let Some((c, a)) = &pu.const_wing {
                    (c.clone(), Arc::clone(a))
                } else {
                    let now = if value_needs_now(&pu.record.fields[0].1) {
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
                if !same_wing {
                    self.store.sync_docs_wing_only(idx, wing_arc);
                }
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
                            return Err(Error::runtime(format!(
                                "immutable field: {}.{k}",
                                pu.collection
                            )));
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
        #[cfg(feature = "gpu")]
        let has_gpu_agg = self.gpu.is_some()
            && prepared.stmts.first().is_some_and(|s| match s {
                Stmt::Query(q) => self.uses_gpu_filter(q) || self.uses_gpu_count(q),
                _ => false,
            });
        #[cfg(not(feature = "gpu"))]
        let has_gpu_agg = false;

        if !has_gpu_agg
            && let Some(agg) = &prepared.agg_plan
        {
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
        let base_gen = self.store.r#gen;
        let t0 = Instant::now();
        let mut undo = if !writes {
            Undo::None
        } else if append_only && !schema {
            Undo::Append(self.store.append_mark())
        } else if !schema
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
                    if let Err(e) =
                        self.store
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
            if let Some(p) = self.persist.as_mut() {
                self.store.maybe_checkpoint(p)?;
            }
            if append_only {
                self.append_rows += n as u64;
                self.append_ms += ms;
            }
        }
        if schema {
            self.plan_cache.clear();
            self.query_cache.clear();
            self.seen_dml_hashes = [0; 16];
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
        if let Some(pg) = &prepared.point_get {
            let row = self.execute_point_get(pg);
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
            && prepared.stmts.first().is_some_and(|s| match s {
                Stmt::Query(q) => self.uses_gpu_filter(q) || self.uses_gpu_count(q),
                _ => false,
            });
        #[cfg(not(feature = "gpu"))]
        let has_gpu_agg = false;

        if !has_gpu_agg
            && let Some(agg) = &prepared.agg_plan
        {
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

    /// Begin an interactive ACID transaction.
    pub fn transaction(&mut self) -> Result<Transaction<'_>, Error> {
        let base_gen = self.store.r#gen;
        Ok(Transaction {
            db: self,
            base_gen,
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
            if let Some(rows) = self.try_filter_project(q, bindings, now) {
                return Ok(rows);
            }
            if let Some(rows) = self.try_fts_search(q, bindings)? {
                return Ok(rows);
            }
            if let Some(rows) = self.try_join_project(q, bindings, now) {
                return Ok(rows);
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
                        let proj_names = project_fields_step.map(field_names);
                        let want_proj = if simple_eq {
                            proj_names.as_deref()
                        } else {
                            None
                        };
                        if let Some(row) = self.store.project_by_key(name, field, key, want_proj) {
                            if !simple_eq && !eval_pred(pred, &row, now) {
                                Vec::new()
                            } else if !simple_eq
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
                            && matches!(
                                q.steps.get(1),
                                Some(Step::Graph { .. } | Step::Hop { .. })
                            )
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
                    self.filter_scan(name, pred, now, project_fields_step)
                } else if let Some(fields) = project_fields_step {
                    self.scan_projected(name, fields, scan_limit)
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
                            Step::Project(_) => {
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
                        for f in fs {
                            row.entry(f.clone()).or_insert(Cell::Null);
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
        let mut out = Vec::new();
        for r in col {
            if eval_pred(pred, r, now) {
                out.push(match names.as_deref() {
                    Some(fs) => project_fields(r, fs),
                    None => {
                        let mut row = acquire_row();
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
                let mut r = acquire_row();
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
                        let mut r = acquire_row();
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
        if self.store.is_disk_backed(name)
            && let Some(eng) = self.store.disk_engine_read()
        {
            if limit == Some(0) {
                return Vec::new();
            }
            let proj_refs: Vec<&str> = names.iter().map(String::as_str).collect();
            let mut out = Vec::new();
            let _ = eng.scan_projected(name, Some(&proj_refs), |r| {
                out.push(project_fields(r, &names));
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

    pub fn prepare(&self, src: &str) -> Result<Prepared, Error> {
        if let Ok(guard) = self.plan_cache.lock()
            && let Some(p) = guard.get(src)
        {
            return Ok(p.clone());
        }
        let prepared = self.inner.prepare_uncached(src)?;
        if let Ok(mut guard) = self.plan_cache.lock() {
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
        let sources: Vec<String> = queries.into_iter().map(|s| s.as_ref().to_string()).collect();
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
                            h.join()
                                .unwrap_or_else(|_| Err(Error::runtime("parallel query worker panicked")))
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
                            h.join()
                                .unwrap_or_else(|_| Err(Error::runtime("parallel query worker panicked")))
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
    let mut r = BTreeMap::new();
    r.insert("kind".into(), Cell::text_arc(kind));
    r.insert("name".into(), Cell::text_arc(name));
    r
}
