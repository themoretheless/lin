use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap};
use std::fs::File;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use rustc_hash::FxHashMap;

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

struct PackCtx {
    snap: BTreeMap<(String, String), String>,
    written: BTreeSet<(String, String)>,
}

fn mark_written(ctx: &mut PackCtx, collection: &str, rows: &[Row]) {
    for r in rows {
        if let Some(id) = row_text(r, "id") {
            ctx.written.insert((collection.to_string(), id.to_string()));
        }
    }
}

#[derive(Debug, Clone)]
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
}

/// Shared read-only snapshot of a [`Db`] at a fixed `gen`.
///
/// Cheap to `Clone` (`Arc`). Safe to share across threads. Writes are rejected.
#[derive(Clone)]
pub struct ReadDb {
    inner: Arc<Db>,
}

/// Row / edge / log byte caps. Defaults suit embeddable single-node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Quotas {
    pub max_rows: usize,
    pub max_edges: usize,
    pub max_log_bytes: u64,
}

impl Default for Quotas {
    fn default() -> Self {
        Self {
            max_rows: 10_000_000,
            max_edges: 10_000_000,
            max_log_bytes: 512 * 1024 * 1024,
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
            quotas: Quotas::default(),
            reopen: ReopenPhases::default(),
            append_rows: 0,
            append_ms: 0.0,
            pins: BTreeMap::new(),
            pulled_wal: Vec::new(),
            follower: false,
            embedder: Some(embedder),
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

    /// Replace the active embedder. Updates `store.embed_id` / catalog to match.
    pub fn with_embedder(mut self, embedder: Arc<dyn Embedder>) -> Self {
        self.catalog.embed_id = embedder.id().to_string();
        self.store.embed_id = embedder.id().to_string();
        self.embedder = Some(embedder);
        self.plan_cache.clear();
        self
    }

    /// Disable embedding (vec/hybrid degrade: vec empty, hybrid→lex only).
    pub fn without_embedder(mut self) -> Self {
        self.embedder = None;
        self.plan_cache.clear();
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
            quotas: Quotas::default(),
            reopen: store_phases.into(),
            append_rows: 0,
            append_ms: 0.0,
            pins: BTreeMap::new(),
            pulled_wal: Vec::new(),
            follower: false,
            embedder: Some(embedder),
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
            quotas: Quotas::default(),
            reopen: store_phases.into(),
            append_rows: 0,
            append_ms: 0.0,
            pins: BTreeMap::new(),
            pulled_wal: Vec::new(),
            follower: false,
            embedder: Some(embedder),
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
                quotas: self.quotas,
                reopen: self.reopen,
                append_rows: 0,
                append_ms: 0.0,
                pins: BTreeMap::new(),
                pulled_wal: Vec::new(),
                follower: false,
                embedder: self.embedder.clone(),
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
            if let Some(p) = self.persist.as_mut() {
                if p.log_bytes + raw_len > self.quotas.max_log_bytes {
                    self.store.checkpoint(p)?;
                }
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
        if let Some(p) = self.persist.as_ref() {
            if p.log_bytes > self.quotas.max_log_bytes {
                return Err(Error::runtime(format!(
                    "quota: log_bytes {} > max_log_bytes {}",
                    p.log_bytes, self.quotas.max_log_bytes
                )));
            }
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
        let store = Store::from_snapshot(std::path::Path::new("."), &snap)?;
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
        let stmts = parse::parse_program(src)?;
        let mut prepared = self.prepare_stmts(stmts)?;
        prepared.src = src.to_string();
        Ok(prepared)
    }

    /// Typecheck + plan a program already parsed as AST (no string cache).
    pub fn prepare_stmts(&self, stmts: Vec<Stmt>) -> Result<Prepared, Error> {
        let mut check_cat = self.catalog.clone();
        check::check_program(&stmts, &mut check_cat)?;
        let plan = Arc::new(plan::plan_program(&stmts, &check_cat)?);
        let writes = stmts.iter().any(stmt_writes);
        let append_only = writes && stmts.iter().all(|s| stmt_append_only(s) || !stmt_writes(s));
        let schema = stmts.iter().any(stmt_schema);
        Ok(Prepared {
            src: String::new(),
            stmts,
            plan,
            writes,
            append_only,
            schema,
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
        let t0 = Instant::now();
        let undo = if !prepared.writes {
            Undo::None
        } else if prepared.append_only {
            Undo::Append(self.store.append_mark())
        } else {
            Undo::Full(self.store.mem_backup())
        };
        let cat_backup = if prepared.writes && !prepared.append_only {
            Some(self.catalog.clone())
        } else if prepared.schema {
            Some(self.catalog.clone())
        } else {
            None
        };

        let (rows, message, pack) = match self.exec_program(&prepared.stmts) {
            Ok(v) => v,
            Err(e) => {
                self.rollback(undo, cat_backup);
                return Err(e);
            }
        };
        let ms = t0.elapsed().as_secs_f64() * 1000.0;
        let n = if rows.is_empty() {
            pack_affect_n(pack.as_ref()).unwrap_or(0)
        } else {
            rows.len()
        };
        if let Some(pack) = pack {
            if let Err(e) = self.check_quotas() {
                self.rollback(undo, cat_backup);
                return Err(e);
            }
            // Bound WAL: compact if already over quota before appending.
            if let Some(p) = self.persist.as_mut() {
                if p.log_bytes > self.quotas.max_log_bytes {
                    if let Err(e) = self.store.checkpoint(p) {
                        self.rollback(undo, cat_backup);
                        return Err(e);
                    }
                }
            }
            if let Some(p) = self.persist.as_mut() {
                // Schema packs only — avoid hashing the catalog on every append.
                if prepared.schema {
                    p.catalog_hash = crate::store::catalog_hash(&self.catalog);
                }
                if let Err(e) = self.store.durable_commit(p, pack) {
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
        }
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
            let now = now_ms();
            let bindings = Default::default();
            if let Some(batch) = self.try_filter_project_batch(q, &bindings, now) {
                return Ok(batch);
            }
            if let Some(batch) = self.try_join_project_batch(q, &bindings, now) {
                return Ok(batch);
            }
        }
        let h = self.run_prepared(prepared)?;
        Ok(rows_to_rough_batch(&h.rows))
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
        let t0 = Instant::now();
        let (rows, message, pack) = self.exec_program_readonly(&prepared.stmts)?;
        debug_assert!(pack.is_none());
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
        }
        if let Some(cat) = cat_backup {
            self.catalog = cat;
        }
    }

    pub fn run(&mut self, src: &str) -> Result<Handle, Error> {
        let prepared = self.prepare(src)?;
        self.run_prepared(&prepared)
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
        let mut cat = self.catalog.clone();
        check::check_program(&stmts, &mut cat)?;
        let mut plan = plan::plan_program(&stmts, &cat)?;
        if let Some(fmt) = graph {
            plan.explain = match fmt {
                GraphFmt::Mermaid => crate::ast::ExplainKind::Graph,
                GraphFmt::Dot => crate::ast::ExplainKind::Dot,
            };
        }
        let mut ctx = ExplainCtx {
            r#gen: Some(self.store.r#gen),
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
            let now = now_ms();
            let bindings = Default::default();
            if let Some(batch) = self.inner.try_filter_project_batch(q, &bindings, now) {
                return Ok(batch);
            }
            if let Some(batch) = self.inner.try_join_project_batch(q, &bindings, now) {
                return Ok(batch);
            }
        }
        let h = self.run_prepared(prepared)?;
        Ok(rows_to_rough_batch(&h.rows))
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
        let mut cat = self.inner.catalog.clone();
        check::check_program(&stmts, &mut cat)?;
        let mut plan = plan::plan_program(&stmts, &cat)?;
        if let Some(fmt) = graph {
            plan.explain = match fmt {
                GraphFmt::Mermaid => crate::ast::ExplainKind::Graph,
                GraphFmt::Dot => crate::ast::ExplainKind::Dot,
            };
        }
        let mut ctx = ExplainCtx {
            r#gen: Some(self.inner.store.r#gen),
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
    fn exec_program(&mut self, stmts: &[Stmt]) -> Result<StmtOut, Error> {
        let mut bindings: BTreeMap<String, Vec<Row>> = BTreeMap::new();
        let need_snap = stmts.iter().any(|s| {
            matches!(
                s,
                Stmt::Update { cas_each: true, .. } | Stmt::Delete { cas_each: true, .. }
            )
        });
        let mut ctx = PackCtx {
            snap: if need_snap {
                self.hash_snapshot()
            } else {
                BTreeMap::new()
            },
            written: BTreeSet::new(),
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
        Ok((last_rows, last_msg, fold_packs(packs)))
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
                for record in records {
                    let mut s = String::new();
                    let mut p = String::new();
                    let mut o = String::new();
                    for (k, v) in &record.fields {
                        match k.as_str() {
                            "s" => s = value_text(v, now),
                            "p" => p = value_text(v, now),
                            "o" => o = value_text(v, now),
                            _ => {}
                        }
                    }
                    bulk_s.push(s);
                    bulk_p.push(p);
                    bulk_o.push(o);
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
                let (rows, new_edges) = self.insert_bulk(collection, records, edges.as_slice())?;
                mark_written(ctx, collection, &rows);
                let pack = if self.is_durable() {
                    Some(crate::persist::rows_to_insert_cols(
                        collection.clone(),
                        &rows,
                        new_edges,
                    ))
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
                Ok((
                    rows.clone(),
                    None,
                    Some(Pack::Update {
                        collection: collection.clone(),
                        rows,
                    }),
                ))
            }
            Stmt::Delete {
                collection,
                pred,
                cas,
                cas_each,
            } => {
                let rows = self.delete_rows(collection, pred, cas.as_deref(), *cas_each, ctx)?;
                Ok((
                    rows.clone(),
                    None,
                    Some(Pack::Delete {
                        collection: collection.clone(),
                        rows,
                    }),
                ))
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
        let now = now_ms();

        // Point Get short path: map lookup + optional project — no Scan/Filter pipeline.
        if let Some(rows) = self.try_point_get(q) {
            return Ok(rows);
        }

        // Filter → count: index-only or scan-without-clone (fair vs SQL COUNT(*)).
        if let Some(rows) = self.try_filter_count(q, bindings, now) {
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

        let mut primary = check::collection_of(&q.source).to_string();
        let mut implicit_take = true;
        let mut saw_agg = false;
        let first_filter = q.steps.iter().find_map(|s| match s {
            Step::Filter(p) => Some(p),
            _ => None,
        });
        let project_fields_step = {
            let simple = q.steps.iter().all(|s| {
                matches!(
                    s,
                    Step::Filter(_) | Step::Project(_) | Step::Skip { .. } | Step::Take { .. }
                )
            });
            if simple {
                q.steps.iter().find_map(|s| match s {
                    Step::Project(f) => Some(f.as_slice()),
                    _ => None,
                })
            } else {
                None
            }
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
                    && let Some(uses) = crate::index::pick_index(&self.catalog, name, pred)
                    && let Some(idxs) = self.store.index_seek(name, &uses, now)
                {
                    let covered = crate::index::index_covers_pred(pred, &uses);
                    self.fetch_idxs_filtered(name, &idxs, pred, now, project_fields_step, covered)
                } else if let Some(pred) = first_filter {
                    self.filter_scan(name, pred, now, project_fields_step)
                } else {
                    self.scan(name)
                }
            }
        };

        // If IndexSeek / filter_scan already applied the first filter (+ optional project),
        // skip only those steps — later filters/projects still run.
        let mut skip_first_filter = first_filter.is_some()
            && matches!(&q.source, Source::Collection(n) if bindings.get(n).is_none());
        let mut skip_first_project = skip_first_filter && project_fields_step.is_some();

        for step in &q.steps {
            match step {
                Step::Filter(pred) => {
                    if skip_first_filter {
                        skip_first_filter = false;
                        continue;
                    }
                    if let Some((field, key)) = point_key(pred)
                        && rows.len() != 1
                    {
                        rows = self.get_by(&primary, field, key);
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
                    rows = self.join_rows(&primary, rows, collection, on, *left);
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
                    rows = match by {
                        Some(by) => agg_count(&rows, by),
                        None => vec![hits_row(rows.len() as i64)],
                    };
                    primary = String::new();
                }
                Step::Sum { field, by } => {
                    implicit_take = false;
                    saw_agg = true;
                    rows = agg_sum(&rows, field, by);
                    primary = String::new();
                }
                Step::Sort { field, desc } => {
                    sort_rows(&mut rows, field, *desc);
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

    /// `docs | id == "…" | { … }` — direct map hit.
    fn try_point_get(&self, q: &Query) -> Option<Vec<Row>> {
        let name = match &q.source {
            Source::Collection(n) => n.as_str(),
            Source::Page(uri) => {
                let fields = q.steps.iter().find_map(|s| match s {
                    Step::Project(f) => Some(field_names(f)),
                    _ => None,
                });
                let row = self
                    .store
                    .project_by_key("docs", "uri", uri, fields.as_deref())?;
                // Only allow Filter/Project/Skip/Take after page get.
                if q.steps.iter().any(|s| {
                    !matches!(
                        s,
                        Step::Filter(_) | Step::Project(_) | Step::Skip { .. } | Step::Take { .. }
                    )
                }) {
                    return None;
                }
                let mut rows = vec![row];
                for step in &q.steps {
                    if let Step::Filter(pred) = step {
                        let now = now_ms();
                        rows.retain(|r| eval_pred(pred, r, now));
                    }
                }
                return Some(rows);
            }
            _ => return None,
        };
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
        let simple_eq = matches!(
            pred,
            Pred::Cmp {
                op: CmpOp::Eq,
                value: Value::String(_),
                ..
            }
        );
        let row = self
            .store
            .project_by_key(name, field, key, fields.as_deref())?;
        // Lookup already keyed by id/uri equality — skip re-eval for a lone Cmp.
        if simple_eq {
            return Some(vec![row]);
        }
        let now = now_ms();
        if !eval_pred(pred, &row, now) {
            // projected row may miss pred fields — evaluate against full row
            let full = self.store.project_by_key(name, field, key, None)?;
            if !eval_pred(pred, &full, now) {
                return Some(Vec::new());
            }
            // pred ok on full; return projected if requested
            return Some(vec![
                self.store
                    .project_by_key(name, field, key, fields.as_deref())
                    .unwrap_or(full),
            ]);
        }
        Some(vec![row])
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
        if !self.store.fts.contains_key(name) {
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
                    if mode.is_some() {
                        return Ok(None);
                    }
                    if !matches!(m, SearchMode::Lex | SearchMode::Hybrid) {
                        return Ok(None);
                    }
                    mode = Some(*m);
                    query = Some(qq.as_str());
                }
                Step::Skip { n } => {
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

        let limit = match take_n {
            Some(Some(n)) => Some(n.max(0) as usize),
            Some(None) => None,
            None => Some(50),
        };
        let rank_limit = if mode == SearchMode::Lex {
            limit.map(|n| n.saturating_add(skip_n))
        } else {
            None
        };
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
            SearchMode::Lex => Ok(self.search_fts_lex(collection, query, rank_limit)),
            SearchMode::Hybrid => {
                const RRF_K: f64 = 60.0;
                let lex = self.search_fts(collection, SearchMode::Lex, query, None)?;
                let all = self.store.collection(collection);
                let vec = self.search_rows(all, SearchMode::Vec, query)?;
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
            SearchMode::Vec => self.search_rows(self.store.collection(collection), mode, query),
        }
    }

    fn search_fts_lex(&self, collection: &str, query: &str, rank_limit: Option<usize>) -> Vec<Row> {
        if rank_limit == Some(0) {
            return Vec::new();
        }
        let fts = self
            .store
            .fts
            .get(collection)
            .expect("checked FTS collection");
        let idxs = fts.candidate_idxs(query);
        let col = self.store.collection(collection);
        let query = LexQuery::new(query);

        let mut scored = match rank_limit {
            Some(k) => {
                // The max-heap root is the worst retained result:
                // lower score first, then larger row index. This preserves the
                // old stable ordering (score desc, source row index asc).
                let mut heap: BinaryHeap<(Reverse<i64>, usize)> =
                    BinaryHeap::with_capacity(k.saturating_add(1));
                for i in idxs {
                    let Some(row) = col.get(i) else { continue };
                    let score = lex_score_prepared(row, &query);
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
                .filter_map(|i| {
                    let row = col.get(i)?;
                    let score = lex_score_prepared(row, &query);
                    (score > 0).then_some((score, i))
                })
                .collect(),
        };
        scored.sort_unstable_by(|(sa, ia), (sb, ib)| sb.cmp(sa).then_with(|| ia.cmp(ib)));
        scored
            .into_iter()
            .filter_map(|(_, i)| col.get(i).cloned())
            .collect()
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
                    if filter.is_some() {
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
                Step::Take { .. } => {}
                Step::Skip { .. } => {}
                _ => return None,
            }
        }
        let pred = filter?;
        let by = by?;

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
            if let Some(uses) = crate::index::pick_index(&self.catalog, name, pred)
                && crate::index::index_covers_pred(pred, &uses)
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
        if let Some(uses) = crate::index::pick_index(&self.catalog, name, pred)
            && crate::index::index_covers_pred(pred, &uses)
        {
            if let Some(v) = eq_value_for_field(pred, &by_key) {
                let n = self.store.index_seek_count(name, &uses, now)?;
                return Some(vec![count_row(&by_key, v, n as i64)]);
            }
            // Index covers filter but group key varies — read only the by-field.
            let idxs = self.store.index_seek(name, &uses, now)?;
            let mut map: BTreeMap<String, i64> = BTreeMap::new();
            for i in idxs {
                let Some(row) = self.store.get_by_idx(name, i) else {
                    continue;
                };
                let key = row
                    .get(&by_key)
                    .map(Cell::compact)
                    .unwrap_or_else(|| "null".into());
                *map.entry(key).or_insert(0) += 1;
            }
            return Some(count_map_to_rows(&by_key, map));
        }

        // Full scan count without cloning rows.
        let col = self.store.collection(name);
        let mut map: BTreeMap<String, i64> = BTreeMap::new();
        for r in col {
            if eval_pred(pred, r, now) {
                let key = r
                    .get(&by_key)
                    .map(Cell::compact)
                    .unwrap_or_else(|| "null".into());
                *map.entry(key).or_insert(0) += 1;
            }
        }
        Some(count_map_to_rows(&by_key, map))
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

        let mut rows = {
            // docs | title ~ needle | { hot… }
            if name == "docs"
                && let Pred::Contains { field, needle } = pred
                && field.leaf() == Some("title")
            {
                self.store
                    .scan_docs_hot_project(&names, Some(needle.as_str()))?
            } else if let Some(uses) = crate::index::pick_index(&self.catalog, name, pred)
                && let Some(idxs) = self.store.index_seek(name, &uses, now)
            {
                let covered = crate::index::index_covers_pred(pred, &uses);
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

        for step in &q.steps {
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
                    if fields.is_none() {
                        return None;
                    }
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
                if let Some(uses) = crate::index::pick_index(&self.catalog, name, pred)
                    && let Some(idxs) = self.store.index_seek(name, &uses, now)
                {
                    let covered = crate::index::index_covers_pred(pred, &uses);
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
        let probe = build_right_probe(rights, &right_fields);

        let (source, need_filter) = {
            if let Some(pred) = filter {
                if let Some(uses) = crate::index::pick_index(&self.catalog, left_name, pred)
                    && let Some(idxs) = self.store.index_seek(left_name, &uses, now)
                {
                    let covered = crate::index::index_covers_pred(pred, &uses);
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
        let names_arc: Arc<[String]> = names.clone().into();
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
            let Some(left_row) = self.store.get_by_idx(left_name, idx) else {
                continue;
            };
            if need_filter
                && let Some(pred) = filter
                && !eval_pred(pred, left_row, now)
            {
                continue;
            }

            let right_cells = left_row
                .get(on)
                .and_then(Cell::text)
                .and_then(|k| probe.get(k).map(|v| v.as_slice()));
            if right_cells.is_none() && !left_join {
                continue;
            }
            push_join_batch_row(&mut batch, left_row, right_cells, &plan);
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
        // Index into SoA columns — avoid allocating Vec<Cell> per user on every run.
        let mut probe: FxHashMap<&str, usize> = FxHashMap::default();
        probe.reserve(users_id.len());
        for i in 0..users_id.len() {
            probe.insert(users_id[i].as_ref(), i);
        }

        let orders_id = self.store.orders_id();
        let orders_uid = self.store.orders_user_id();
        let orders_total = self.store.orders_total();
        let n = orders_id.len();

        let total_gt = filter.and_then(pred_total_gt);
        let (source, need_row_filter) = if total_gt.is_some() {
            (JoinLeftSource::Scan { len: n }, false)
        } else if let Some(pred) = filter {
            if let Some(uses) = crate::index::pick_index(&self.catalog, "orders", pred)
                && let Some(idxs) = self.store.index_seek("orders", &uses, now)
            {
                let covered = crate::index::index_covers_pred(pred, &uses);
                (JoinLeftSource::Idxs(idxs), !covered)
            } else {
                (JoinLeftSource::Scan { len: n }, true)
            }
        } else {
            (JoinLeftSource::Scan { len: n }, false)
        };

        let names_arc: Arc<[String]> = names.to_vec().into();
        let mut batch = RecordBatch::with_capacity(names_arc, n);

        // Specialized: { id, users.email, total } — hottest bench / OLAP shape.
        let fast_id_email_total = plan.len() == 3
            && matches!(plan[0], JoinFieldPlan::Left(ref k) if k == "id")
            && matches!(
                plan[1],
                JoinFieldPlan::Right { ref out, idx: 0 } if out == "users.email"
                    || right_fields.first().is_some_and(|f| f == "email")
            )
            && matches!(plan[2], JoinFieldPlan::Left(ref k) if k == "total");

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
                    if !(orders_total[idx] > min) {
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
                let Some(ui) = probe.get(uid).copied() else {
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
                if !(orders_total[idx] > min) {
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
            let right_i = probe.get(uid).copied();
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
                    None => r.clone(),
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
                let mut r = BTreeMap::new();
                r.insert("id".into(), row.get("id").cloned().unwrap_or(Cell::Null));
                out.push(r);
            } else {
                out.push(match names.as_deref() {
                    Some(fs) => project_fields(row, fs),
                    None => row.clone(),
                });
            }
        }
        out
    }

    fn scan(&self, name: &str) -> Vec<Row> {
        if name == "catalog" {
            return self.scan_catalog();
        }
        self.store.collection(name).to_vec()
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

    fn get_by(&self, collection: &str, field: &str, key: &str) -> Vec<Row> {
        self.store
            .project_by_key(collection, field, key, None)
            .into_iter()
            .collect()
    }

    fn join_rows(
        &self,
        left_col: &str,
        left: Vec<Row>,
        right_col: &str,
        on: &str,
        left_join: bool,
    ) -> Vec<Row> {
        let to_field = self
            .catalog
            .find_fk(left_col, on, right_col)
            .map(|fk| fk.to_field.as_str())
            .unwrap_or("id");

        // FK → unique id: hash-build right once, then probe (tight map, no by_id indirection).
        if to_field == "id" {
            let right = self.store.collection(right_col);
            let mut by_id: FxHashMap<&str, &Row> = FxHashMap::default();
            by_id.reserve(right.len());
            for r in right {
                if let Some(id) = r.get("id").and_then(Cell::text) {
                    by_id.insert(id, r);
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
            return out;
        }

        // Non-id join key: hash probe without cloning the right collection.
        let right = self.store.collection(right_col);
        let mut idx: FxHashMap<String, Vec<&Row>> = FxHashMap::default();
        idx.reserve(right.len());
        for r in right {
            if let Some(k) = cell_key(r.get(to_field).unwrap_or(&Cell::Null)) {
                idx.entry(k).or_default().push(r);
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
                }
            } else if left_join {
                out.push(l);
            }
        }
        out
    }

    fn hop(&self, primary: &str, rows: &[Row], rel: &str, depth: i64) -> Result<Vec<Row>, Error> {
        let (edge_rel, reverse) = match self.catalog.rel(rel) {
            Some(r) if let Some(of) = &r.reverse_of => (of.as_str(), true),
            _ => (rel, false),
        };
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
            for e in &self.store.edges {
                if e.rel != edge_rel {
                    continue;
                }
                let (src, dst) = if reverse {
                    (e.to.as_str(), e.from.as_str())
                } else {
                    (e.from.as_str(), e.to.as_str())
                };
                if frontier.contains(src) && seen.insert(dst.to_string()) {
                    next.insert(dst.to_string());
                    reached.insert(dst.to_string());
                }
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
            } else if primary != "docs"
                && let Some(row) = self
                    .store
                    .collection(primary)
                    .iter()
                    .find(|r| row_text(r, "id") == Some(key.as_str()))
            {
                out.push(row.clone());
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
            for e in &self.store.edges {
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
        for start_key in keys {
            let mut stack: Vec<(
                String,
                i64,
                BTreeSet<String>,
                Option<(String, String, String)>,
            )> = vec![(start_key, 0, BTreeSet::new(), None)];
            while let Some((at, depth, path, last_edge)) = stack.pop() {
                if depth > 0 && depth >= hop.min_depth && depth <= hop.max_depth {
                    if let Some(node) = self.resolve_node(primary, &at) {
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
                }
                if depth >= hop.max_depth {
                    continue;
                }
                let mut visited = path;
                visited.insert(at.clone());
                for e in &self.store.edges {
                    if e.rel != edge_rel {
                        continue;
                    }
                    let (src, dst) = if reverse {
                        (e.to.as_str(), e.from.as_str())
                    } else {
                        (e.from.as_str(), e.to.as_str())
                    };
                    if src != at {
                        continue;
                    }
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
            }
        }
        Ok(())
    }

    fn resolve_node(&self, primary: &str, key: &str) -> Option<Row> {
        if let Some(row) = self.store.find_doc_key(key) {
            return Some(row.clone());
        }
        if primary != "docs" {
            return self
                .store
                .collection(primary)
                .iter()
                .find(|r| row_text(r, "id") == Some(key))
                .cloned();
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
                let mut scored: Vec<(i64, Row)> = rows
                    .iter()
                    .filter_map(|r| {
                        let s = lex_score_prepared(r, &query);
                        if s > 0 { Some((s, r.clone())) } else { None }
                    })
                    .collect();
                scored.sort_by_key(|a| std::cmp::Reverse(a.0));
                Ok(scored.into_iter().map(|(_, r)| r).collect())
            }
            SearchMode::Vec => {
                let Some(emb) = self.embedder.as_ref() else {
                    return Ok(Vec::new());
                };
                let qv = emb.embed(query);
                let mut scored: Vec<(f64, Row)> = rows
                    .iter()
                    .filter_map(|r| {
                        let v = r.get("embedding").and_then(Cell::as_vec)?;
                        let s = embed::cosine(qv.as_ref(), v);
                        if s > 0.01 { Some((s, r.clone())) } else { None }
                    })
                    .collect();
                scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
                Ok(scored.into_iter().map(|(_, r)| r).collect())
            }
            SearchMode::Hybrid => {
                // Reciprocal rank fusion over lex + vec lists.
                const RRF_K: f64 = 60.0;
                let lex = self.search_rows(rows, SearchMode::Lex, query)?;
                let vec = self.search_rows(rows, SearchMode::Vec, query)?;
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
        self.plan_cache.clear();
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

        let mut row_idxs = Vec::with_capacity(rows.len());
        let mut texts = Vec::with_capacity(rows.len());
        for (i, row) in rows.iter().enumerate() {
            if row.get("embedding").and_then(Cell::as_vec).is_some() {
                continue;
            }
            let text = embed::row_embed_text(row);
            if !text.is_empty() {
                row_idxs.push(i);
                texts.push(text);
            }
        }
        let refs = texts.iter().map(String::as_str).collect::<Vec<_>>();
        let mut vectors = emb.embed_batch(&refs);
        if vectors.len() != row_idxs.len() {
            // Keep third-party embedders with a broken batch implementation
            // from silently leaving a partially embedded slab.
            vectors = refs.iter().map(|text| emb.embed(text)).collect();
        }
        for (i, vector) in row_idxs.into_iter().zip(vectors) {
            rows[i].insert("embedding".into(), Cell::Vec(vector));
        }
    }

    #[allow(dead_code)]
    fn append_fact(&mut self, record: &Record) -> Result<(Row, bool), Error> {
        let now = now_ms();
        let row = record_row(record, now);
        Ok(self.store.append_fact_row(row))
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
    ) -> Result<(Vec<Row>, Vec<Edge>), Error> {
        let now = now_ms();
        let n = records.len();
        let mut built: Vec<Row> = Vec::with_capacity(n);
        let mut batch_ids: rustc_hash::FxHashSet<std::sync::Arc<str>> =
            rustc_hash::FxHashSet::default();
        batch_ids.reserve(n);
        let mut batch_uris: rustc_hash::FxHashSet<std::sync::Arc<str>> =
            rustc_hash::FxHashSet::default();
        if collection == "docs" {
            batch_uris.reserve(n);
        }

        for record in records {
            let mut row = record_row(record, now);
            if row.get("id").and_then(Cell::text).is_none() {
                row.insert("id".into(), Cell::text_arc(self.store.alloc_id()));
            }
            if row.get("hash").and_then(Cell::text).is_none()
                && let Some(body) = row.get("body").and_then(Cell::text_shared)
            {
                row.insert("hash".into(), Cell::Text(content_hash_arc(body.as_ref())));
            }
            if let Some(id) = row.get("id").and_then(Cell::text_shared) {
                if !batch_ids.insert(std::sync::Arc::clone(&id))
                    || self.store.get_by_id(collection, id.as_ref()).is_some()
                {
                    return Err(Error::runtime(format!("duplicate id: {id}")));
                }
            }
            if collection == "docs"
                && let Some(uri) = row.get("uri").and_then(Cell::text_shared)
            {
                if !batch_uris.insert(std::sync::Arc::clone(&uri))
                    || self.store.get_by_uri(uri.as_ref()).is_some()
                {
                    return Err(Error::runtime(format!("duplicate uri: {uri}")));
                }
            }
            self.check_row_fks(collection, &row)?;
            built.push(row);
        }
        self.maybe_embed_rows(collection, &mut built);

        let mut new_edges = Vec::new();
        if let Some(first) = built.first() {
            let from_id = row_text(first, "id").unwrap_or("").to_string();
            self.store.edges.reserve(edges.len());
            self.store.edge_keys.reserve(edges.len());
            for e in edges {
                let to = match &e.target {
                    EdgeTarget::Page(uri) => self
                        .store
                        .find_doc_key(uri)
                        .and_then(|r| row_text(r, "id").map(|s| s.to_string()))
                        .unwrap_or_else(|| uri.clone()),
                    EdgeTarget::Value(v) => value_text(v, now),
                };
                let _ = self.store.append_edge_parts(&e.rel, &from_id, &to);
                new_edges.push(Edge {
                    rel: e.rel.clone(),
                    from: from_id.clone(),
                    to,
                });
            }
        }

        let start = self.store.collection(collection).len();
        self.store.collection_mut(collection).reserve(n);
        self.store.row_maps_reserve(collection, n);
        self.store.index_insert_slab(collection, start, &built)?;
        self.store.fts_insert_slab(collection, start, &built);
        self.store.row_maps_register_slab(collection, start, &built);
        let want_rows = built.len() <= 128;
        if want_rows {
            self.store
                .collection_mut(collection)
                .extend(built.iter().cloned());
            Ok((built, new_edges))
        } else {
            // Large bulk: move into store, elide Handle.rows (done.n from pack).
            self.store.collection_mut(collection).extend(built);
            Ok((Vec::new(), new_edges))
        }
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
        let rows = self.store.collection(collection);
        let mut idxs = Vec::new();
        for (i, row) in rows.iter().enumerate() {
            if pred.is_none_or(|p| eval_pred(p, row, now)) {
                idxs.push(i);
            }
        }
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
        let patch = record_row(record, now);
        let mut out = Vec::new();
        for i in idxs {
            self.store.index_remove_at(collection, i);
            self.store.fts_remove_at(collection, i);
            let rows = self.store.collection_mut(collection);
            let row = &mut rows[i];
            let layer_raw = row_text(row, "layer") == Some("raw")
                || matches!(patch.get("layer"), Some(Cell::Text(s)) if s.as_ref() == "raw");
            if layer_raw && patch.contains_key("body") {
                let _ = self.store.index_insert_at(collection, i);
                self.store.fts_insert_at(collection, i);
                return Err(Error::runtime("immutable field: docs.body"));
            }
            for (k, v) in &patch {
                row.insert(k.clone(), v.clone());
            }
            if patch.contains_key("body")
                && let Some(body) = row_text(row, "body").map(str::to_string)
            {
                row.insert("hash".into(), Cell::Text(content_hash_arc(&body)));
            }
            let updated = row.clone();
            self.check_row_fks(collection, &updated)?;
            self.store.index_insert_row(collection, i, &updated)?;
            self.store.fts_insert_at(collection, i);
            out.push(updated);
        }
        self.store.rebuild_row_maps_collection(collection);
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
        let rows = self.store.collection(collection);
        let mut idxs = Vec::new();
        for (i, row) in rows.iter().enumerate() {
            if eval_pred(pred, row, now) {
                idxs.push(i);
            }
        }
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
        let mut out = Vec::new();
        for i in idxs.into_iter().rev() {
            let row = self.store.collection_mut(collection).remove(i);
            out.push(row);
        }
        out.reverse();
        self.store.rebuild_row_maps_collection(collection);
        self.store.rebuild_indexes_collection(collection);
        self.store.rebuild_fts_inplace(collection);
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
            } => {
                if *unique {
                    self.assert_unique_index(collection, fields)?;
                }
                let pack = Pack::SchemaIndex {
                    collection: collection.clone(),
                    unique: *unique,
                    fields: fields.clone(),
                };
                self.store.apply_pack(&pack);
                self.plan_cache.clear();
                Ok((
                    Vec::new(),
                    Some(format!("index {}", pack_index_label(collection, fields))),
                    Some(pack),
                ))
            }
            Decl::Fk { .. } | Decl::Guard { .. } => {
                Ok((Vec::new(), Some("schema: session only".into()), None))
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

enum Undo {
    None,
    Append(crate::store::AppendMark),
    Full(crate::store::MemBackup),
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
        left.insert(format!("{right_col}.{k}"), v.clone());
    }
    left
}

fn push_project_batch_row(batch: &mut RecordBatch, row: &Row, names: &[String]) {
    for (col_i, name) in names.iter().enumerate() {
        batch.cols[col_i].push(row.get(name).cloned().unwrap_or(Cell::Null));
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
            JoinFieldPlan::Left(k) => left.get(k).cloned().unwrap_or(Cell::Null),
            JoinFieldPlan::Right { out: _, idx } => right
                .and_then(|r| r.get(*idx).cloned())
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

pub(crate) fn build_right_probe_owned(
    rights: &[Row],
    right_fields: &[String],
) -> FxHashMap<String, Vec<Cell>> {
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
        probe.insert(id.to_string(), cells);
    }
    probe
}

pub(crate) fn emit_join_row(left: &Row, right: Option<&[Cell]>, plan: &[JoinFieldPlan]) -> Row {
    let mut out = BTreeMap::new();
    for p in plan {
        match p {
            JoinFieldPlan::Left(k) => {
                out.insert(k.clone(), left.get(k).cloned().unwrap_or(Cell::Null));
            }
            JoinFieldPlan::Right { out: k, idx } => {
                out.insert(
                    k.clone(),
                    right
                        .and_then(|r| r.get(*idx).cloned())
                        .unwrap_or(Cell::Null),
                );
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

fn record_row(record: &Record, now: i64) -> Row {
    let mut row = BTreeMap::new();
    for (k, v) in &record.fields {
        row.insert(k.clone(), value_cell(v, now));
    }
    row
}

fn value_cell(v: &Value, now: i64) -> Cell {
    match v {
        Value::String(s) => Cell::text_arc(s.as_str()),
        Value::Int(n) => Cell::Int(*n),
        Value::Float(n) => Cell::Float(*n),
        Value::Bool(b) => Cell::Bool(*b),
        Value::Now => Cell::Time(now),
        Value::NowMinus(d) => Cell::Time(now - d.as_millis()),
        Value::Duration(d) => Cell::Int(d.as_millis()),
        Value::Name(n) => Cell::text_arc(n.as_str()),
    }
}

fn value_text(v: &Value, now: i64) -> String {
    match value_cell(v, now) {
        Cell::Text(s) => s.as_ref().to_owned(),
        other => other.compact(),
    }
}

fn point_key(pred: &Pred) -> Option<(&str, &str)> {
    match pred {
        Pred::Cmp {
            field,
            op: CmpOp::Eq,
            value: Value::String(s),
        } if field.parts.len() == 1 && matches!(field.parts[0].as_str(), "id" | "uri") => {
            Some((field.parts[0].as_str(), s.as_str()))
        }
        Pred::And(a, b) => point_key(a).or_else(|| point_key(b)),
        _ => None,
    }
}

pub(crate) fn eval_pred(pred: &Pred, row: &Row, now: i64) -> bool {
    match pred {
        Pred::And(a, b) => eval_pred(a, row, now) && eval_pred(b, row, now),
        Pred::Or(a, b) => eval_pred(a, row, now) || eval_pred(b, row, now),
        Pred::Cmp { field, op, value } => cmp_cell(field_cell(row, field), value, *op, now),
        Pred::Has { field, ci, needle } => field_cell(row, field)
            .text()
            .is_some_and(|s| has_word(s, needle, *ci)),
        Pred::Contains { field, needle } => field_cell(row, field)
            .text()
            .is_some_and(|s| text_contains(s, needle)),
        Pred::Regex {
            field,
            pattern,
            flags,
        } => {
            let Some(text) = field_cell(row, field).text() else {
                return false;
            };
            let mut b = regex::RegexBuilder::new(pattern);
            if flags.contains('i') {
                b.case_insensitive(true);
            }
            b.build().is_ok_and(|re| re.is_match(text))
        }
    }
}

fn text_contains(hay: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return true;
    }
    memchr::memmem::find(hay.as_bytes(), needle.as_bytes()).is_some()
}

fn field_cell<'a>(row: &'a Row, field: &Field) -> &'a Cell {
    static NULL: Cell = Cell::Null;
    if let Some(k) = field.leaf() {
        return row.get(k).unwrap_or(&NULL);
    }
    let want = field.as_str();
    for (k, v) in row {
        if k == &want {
            return v;
        }
    }
    &NULL
}

fn cmp_cell(left: &Cell, value: &Value, op: CmpOp, now: i64) -> bool {
    let right = value_cell(value, now);
    match op {
        CmpOp::Eq => cells_eq(left, &right),
        CmpOp::Ne => !cells_eq(left, &right),
        CmpOp::Gt | CmpOp::Lt | CmpOp::Ge | CmpOp::Le => {
            let Some(l) = left.as_f64() else {
                return false;
            };
            let Some(r) = right.as_f64() else {
                return false;
            };
            match op {
                CmpOp::Gt => l > r,
                CmpOp::Lt => l < r,
                CmpOp::Ge => l >= r,
                CmpOp::Le => l <= r,
                _ => false,
            }
        }
    }
}

fn cells_eq(a: &Cell, b: &Cell) -> bool {
    match (a, b) {
        (Cell::Null, Cell::Null) => true,
        (Cell::Text(x), Cell::Text(y)) => x == y,
        (Cell::Bool(x), Cell::Bool(y)) => x == y,
        (Cell::Int(x), Cell::Int(y)) => x == y,
        (Cell::Float(x), Cell::Float(y)) => x == y,
        (Cell::Time(x), Cell::Time(y)) => x == y,
        (Cell::Vec(x), Cell::Vec(y)) => x.as_ref() == y.as_ref(),
        (Cell::Int(x), Cell::Float(y)) => *x as f64 == *y,
        (Cell::Float(x), Cell::Int(y)) => *x == *y as f64,
        _ => false,
    }
}

fn has_word(hay: &str, needle: &str, ci: bool) -> bool {
    if needle.is_empty() {
        return false;
    }
    if !ci {
        return hay
            .split(|c: char| !(c.is_alphanumeric() || c == '_'))
            .any(|w| w == needle);
    }
    // Case-insensitive without allocating per-token Vec.
    let mut nbuf = String::new();
    for ch in needle.chars() {
        for c in ch.to_lowercase() {
            nbuf.push(c);
        }
    }
    let mut wbuf = String::new();
    for tok in hay.split(|c: char| !(c.is_alphanumeric() || c == '_')) {
        if tok.is_empty() {
            continue;
        }
        wbuf.clear();
        for ch in tok.chars() {
            for c in ch.to_lowercase() {
                wbuf.push(c);
            }
        }
        if wbuf == nbuf {
            return true;
        }
    }
    false
}

struct LexQuery {
    lower: String,
    tokens: Vec<String>,
}

impl LexQuery {
    fn new(query: &str) -> Self {
        let lower = query.to_lowercase();
        let tokens = lower.split_whitespace().map(str::to_owned).collect();
        Self { lower, tokens }
    }
}

#[inline]
fn lex_score_prepared(row: &Row, query: &LexQuery) -> i64 {
    let mut blob = String::new();
    for k in ["title", "body", "snippet"] {
        if let Some(t) = row_text(row, k) {
            blob.push_str(t);
            blob.push(' ');
        }
    }
    if blob.is_empty() {
        return 0;
    }
    let blob_l = blob.to_lowercase();
    let mut score = 0i64;
    for tok in &query.tokens {
        if blob_l.contains(tok) {
            score += 1;
            if blob_l
                .split(|c: char| !(c.is_alphanumeric() || c == '_'))
                .any(|word| word == tok)
            {
                score += 2;
            }
        }
    }
    if !query.lower.is_empty() && blob_l.contains(&query.lower) {
        score += 3;
    }
    score
}

fn project(rows: &[Row], fields: &[Field]) -> Vec<Row> {
    rows.iter()
        .map(|r| {
            let mut out = BTreeMap::new();
            for f in fields {
                let k = f.as_str();
                out.insert(k.clone(), r.get(&k).cloned().unwrap_or(Cell::Null));
            }
            out
        })
        .collect()
}

fn sort_rows(rows: &mut [Row], field: &Field, desc: bool) {
    let k = field.as_str();
    rows.sort_by(|a, b| {
        let av = a.get(&k).unwrap_or(&Cell::Null);
        let bv = b.get(&k).unwrap_or(&Cell::Null);
        let ord = cmp_sort(av, bv);
        if desc { ord.reverse() } else { ord }
    });
}

fn cmp_sort(a: &Cell, b: &Cell) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    match (a, b) {
        (Cell::Null, Cell::Null) => Ordering::Equal,
        (Cell::Null, _) => Ordering::Less,
        (_, Cell::Null) => Ordering::Greater,
        (Cell::Text(x), Cell::Text(y)) => x.cmp(y),
        (Cell::Bool(x), Cell::Bool(y)) => x.cmp(y),
        _ => match (a.as_f64(), b.as_f64()) {
            (Some(x), Some(y)) => x.partial_cmp(&y).unwrap_or(Ordering::Equal),
            (Some(_), None) => Ordering::Greater,
            (None, Some(_)) => Ordering::Less,
            _ => a.compact().cmp(&b.compact()),
        },
    }
}

fn agg_count(rows: &[Row], by: &Field) -> Vec<Row> {
    let k = by.as_str();
    let mut map: BTreeMap<String, i64> = BTreeMap::new();
    for r in rows {
        let key = r
            .get(&k)
            .map(Cell::compact)
            .unwrap_or_else(|| "null".into());
        *map.entry(key).or_insert(0) += 1;
    }
    count_map_to_rows(&k, map)
}

fn count_map_to_rows(by: &str, map: BTreeMap<String, i64>) -> Vec<Row> {
    map.into_iter()
        .map(|(g, n)| count_row(by, &unquote(&g), n))
        .collect()
}

fn count_arc_map_to_rows(by: &str, map: BTreeMap<std::sync::Arc<str>, i64>) -> Vec<Row> {
    map.into_iter()
        .map(|(g, n)| {
            let mut row = BTreeMap::new();
            row.insert(by.to_string(), Cell::Text(g));
            row.insert("hits".into(), Cell::Int(n));
            row
        })
        .collect()
}

fn hits_row(n: i64) -> Row {
    let mut row = BTreeMap::new();
    row.insert("hits".into(), Cell::Int(n));
    row
}

fn count_row(by: &str, value: &str, n: i64) -> Row {
    let mut row = BTreeMap::new();
    row.insert(by.to_string(), Cell::text_arc(value));
    row.insert("hits".into(), Cell::Int(n));
    row
}

fn eq_value_for_field<'a>(pred: &'a Pred, field: &str) -> Option<&'a str> {
    match pred {
        Pred::Cmp {
            field: f,
            op: CmpOp::Eq,
            value: Value::String(s),
        } if f.as_str() == field => Some(s.as_str()),
        Pred::Cmp {
            field: f,
            op: CmpOp::Eq,
            value: Value::Name(s),
        } if f.as_str() == field => Some(s.as_str()),
        Pred::And(a, b) => eq_value_for_field(a, field).or_else(|| eq_value_for_field(b, field)),
        _ => None,
    }
}

fn agg_sum(rows: &[Row], field: &Field, by: &Field) -> Vec<Row> {
    let fk = field.as_str();
    let bk = by.as_str();
    let mut map: BTreeMap<String, f64> = BTreeMap::new();
    for r in rows {
        let key = r
            .get(&bk)
            .map(Cell::compact)
            .unwrap_or_else(|| "null".into());
        let n = r.get(&fk).and_then(Cell::as_f64).unwrap_or(0.0);
        *map.entry(key).or_insert(0.0) += n;
    }
    map.into_iter()
        .map(|(g, n)| {
            let mut row = BTreeMap::new();
            row.insert(bk.clone(), Cell::text_arc(unquote(&g)));
            row.insert(fk.clone(), Cell::Float(n));
            row
        })
        .collect()
}

fn unquote(s: &str) -> String {
    if s.len() >= 2 && s.starts_with('"') && s.ends_with('"') {
        s[1..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
}

pub(crate) fn cell_key(c: &Cell) -> Option<String> {
    match c {
        Cell::Text(s) => Some(s.as_ref().to_owned()),
        Cell::Int(n) => Some(n.to_string()),
        _ => None,
    }
}
