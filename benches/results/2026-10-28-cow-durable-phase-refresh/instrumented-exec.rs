use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap};
use std::fs::File;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use rustc_hash::{FxHashMap, FxHashSet, FxHasher};
use std::hash::{Hash, Hasher};

// Thread-local regex compilation cache: (pattern, flags) -> Regex
// Reduces per-query regex compilation overhead dramatically
thread_local! {
    static REGEX_CACHE: Arc<Mutex<FxHashMap<(String, String), Arc<regex::Regex>>>> =
        Arc::new(Mutex::new(FxHashMap::default()));
}

// Precomputed content hash cache: (body_str) -> precomputed_hash
// Avoids repeated FNV-1a computations for identical body strings across inserts
thread_local! {
    static CONTENT_HASH_CACHE: Mutex<FxHashMap<String, Arc<str>>> =
        Mutex::new(FxHashMap::default());
}

// Query result cache with LRU eviction policy
// Caches intermediate results for repeated queries to avoid recomputation
thread_local! {
    #[allow(dead_code)]
    static QUERY_CACHE: Cache = Cache::new();
}

use crate::ast::*;

/// LRU cache entry for cached query results
#[allow(dead_code)]
struct CacheEntry {
    /// Result rows stored as Vec<Row>
    rows: Vec<Row>,
    /// Last access timestamp for LRU tracking
    last_access: std::time::Instant,
}

/// Simple LRU cache implementation using FxHashMap + Vec for order tracking
#[allow(dead_code)]
pub struct Cache {
    /// Cache key → entry mapping
    entries: FxHashMap<String, CacheEntry>,
    /// LRU order: head is most recently used
    lru_order: Vec<String>,
    /// Maximum number of cache entries
    max_entries: usize,
    /// Current cache size in bytes (estimate)
    size_bytes: usize,
}

#[allow(dead_code)]
impl Cache {
    const DEFAULT_MAX_ENTRIES: usize = 256;

    fn new() -> Self {
        Self {
            entries: FxHashMap::default(),
            lru_order: Vec::with_capacity(Self::DEFAULT_MAX_ENTRIES),
            max_entries: Self::DEFAULT_MAX_ENTRIES,
            size_bytes: 0,
        }
    }

    /// Get a cached result, moving it to front of LRU list if found
    pub fn get(&mut self, key: &str) -> Option<&Vec<Row>> {
        // Extract key first to avoid double-mutable-borrow issues
        let key_string = key.to_string();

        if let Some(entry) = self.entries.get_mut(&key_string) {
            // Update access time and move to front of LRU list
            entry.last_access = std::time::Instant::now();
            self.evict_to_front(&key_string);
            return self.entries.get(&key_string).map(|e| &e.rows);
        }
        None
    }

    /// Insert a new result into the cache
    pub fn insert(&mut self, key: String, rows: Vec<Row>) {
        let estimated_size = rows.len() * 1024; // Rough estimate: ~1KB per row

        // Evict if at capacity
        while self.size_bytes + estimated_size > self.max_bytes() {
            self.evict_lru();
        }

        // Remove old entry if exists
        if let Some(old_entry) = self.entries.get(&key) {
            self.size_bytes -= old_entry.rows.len() * 1024;
            self.lru_order.retain(|k| k != &key);
        }

        // Insert new entry
        let entry = CacheEntry {
            rows: rows.clone(),
            last_access: std::time::Instant::now(),
        };

        self.entries.insert(key.clone(), entry);
        self.lru_order.push(key);
        self.size_bytes += estimated_size;
    }

    /// Check if a key exists in cache (without updating LRU)
    pub fn contains(&self, key: &str) -> bool {
        self.entries.contains_key(key)
    }

    /// Clear the entire cache
    pub fn clear(&mut self) {
        self.entries.clear();
        self.lru_order.clear();
        self.size_bytes = 0;
    }

    /// Get current cache statistics
    pub fn stats(&self) -> (usize, usize) {
        (self.entries.len(), self.size_bytes)
    }

    /// Calculate maximum allowed bytes based on entry count limit
    fn max_bytes(&self) -> usize {
        self.max_entries * 1024 * 1024 // ~1MB per entry budget
    }

    /// Evict least recently used entry
    fn evict_lru(&mut self) {
        if self.lru_order.is_empty() {
            return;
        }

        // Find oldest entry by scanning all (simple O(n) approach)
        // Could be optimized with doubly-linked list for O(1) removal
        let oldest_idx = self.find_oldest_index();
        if let Some(oldest_key) = self.lru_order.get(oldest_idx).cloned() {
            if let Some(entry) = self.entries.remove(&oldest_key) {
                self.size_bytes -= entry.rows.len() * 1024;
            }
            self.lru_order.remove(oldest_idx);
        }
    }

    /// Find index of oldest entry in LRU list
    fn find_oldest_index(&self) -> usize {
        self.lru_order
            .iter()
            .enumerate()
            .min_by_key(|(_, key)| {
                if let Some(entry) = self.entries.get(*key) {
                    entry.last_access
                } else {
                    std::time::Instant::now() // Treat missing as very old
                }
            })
            .map(|(idx, _)| idx)
            .unwrap_or(0)
    }

    /// Move key to front of LRU order (most recently used)
    fn evict_to_front(&mut self, key: &str) {
        // Remove from middle and push to end
        self.lru_order.retain(|k| k != key);
        self.lru_order.push(key.to_string());
    }
}

/// Aggregate function types for precomputed aggregation views
#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)]
pub enum AggregateType {
    Count, // COUNT(*) or COUNT(column)
    Sum,   // SUM(column) - numeric only
    Avg,   // AVG(column) - numeric only
    Min,   // MIN(column)
    Max,   // MAX(column)
}

impl std::fmt::Display for AggregateType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AggregateType::Count => write!(f, "COUNT"),
            AggregateType::Sum => write!(f, "SUM"),
            AggregateType::Avg => write!(f, "AVG"),
            AggregateType::Min => write!(f, "MIN"),
            AggregateType::Max => write!(f, "MAX"),
        }
    }
}

/// Values for different aggregate types maintained incrementally
#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
pub struct AggregateValue {
    pub count: u64,
    pub sum: f64,
    pub avg_sum: f64,   // Running sum for average calculation
    pub avg_count: u64, // Running count for average calculation
    pub min: Option<f64>,
    pub max: Option<f64>,
}

#[allow(dead_code)]
impl AggregateValue {
    /// Increment count-only metrics
    #[inline]
    pub fn inc_count(&mut self) {
        self.count += 1;
    }

    /// Add numeric value to running totals
    #[inline]
    pub fn add_value(&mut self, value: f64) {
        self.sum += value;
        self.avg_sum += value;
        self.avg_count += 1;

        if let Some(current_min) = self.min {
            self.min = Some(current_min.min(value));
        } else {
            self.min = Some(value);
        }

        if let Some(current_max) = self.max {
            self.max = Some(current_max.max(value));
        } else {
            self.max = Some(value);
        }
    }

    /// Get computed average
    #[inline]
    pub fn avg(&self) -> Option<f64> {
        if self.avg_count == 0 {
            None
        } else {
            Some(self.avg_sum / self.avg_count as f64)
        }
    }

    /// Merge two aggregate values (for parallel compute)
    #[inline]
    pub fn merge(&mut self, other: &AggregateValue) {
        self.count += other.count;
        self.sum += other.sum;
        self.avg_sum += other.avg_sum;
        self.avg_count += other.avg_count;

        if let Some(other_min) = other.min {
            if let Some(current_min) = self.min {
                self.min = Some(current_min.min(other_min));
            } else {
                self.min = Some(other_min);
            }
        }

        if let Some(other_max) = other.max {
            if let Some(current_max) = self.max {
                self.max = Some(current_max.max(other_max));
            } else {
                self.max = Some(other_max);
            }
        }
    }

    /// Clear all accumulated values
    #[inline]
    pub fn clear(&mut self) {
        *self = AggregateValue::default();
    }
}

/// Materialized aggregate view for a collection + group + agg type
/// Stores precomputed results to avoid repeated computation
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct AggregateView {
    /// Collection name this view applies to
    pub collection: String,
    /// Grouping keys (empty for total aggregate over entire collection)
    pub group_keys: Vec<String>,
    /// Aggregation types to maintain
    pub aggs: Vec<AggregateType>,
    /// Latest stored values per group
    /// Key: concatenated group key values, Value: aggregated counts/sums
    pub data: FxHashMap<String, AggregateValue>,
    /// Last update generation number (for invalidation)
    pub r#gen: u64,
}

// Thread-local storage for aggregate views
// Indexed by (collection, group_fields, agg_types) → AggregateView
thread_local! {
    static AGGREGATE_VIEWS: std::sync::Mutex<FxHashMap<String, AggregateView>> =
        std::sync::Mutex::new(FxHashMap::default());
}

/// Compute cache key from collection, group fields, and aggregation types
#[allow(dead_code)]
fn agg_view_key(collection: &str, group_keys: &[String], aggs: &[AggregateType]) -> String {
    let mut key = format!("{}:", collection);
    if !group_keys.is_empty() {
        key.push_str(&group_keys.join(","));
        key.push('|');
    }
    key.push_str(
        &aggs
            .iter()
            .map(|a| match a {
                AggregateType::Count => "C",
                AggregateType::Sum => "S",
                AggregateType::Avg => "A",
                AggregateType::Min => "M",
                AggregateType::Max => "X",
            })
            .collect::<Vec<_>>()
            .join(""),
    );
    key
}

/// Initialize or retrieve an aggregate view for the given parameters
#[allow(dead_code)]
pub fn get_or_init_aggregate_view(
    collection: &str,
    group_keys: Vec<String>,
    aggs: Vec<AggregateType>,
    r#gen: u64,
) -> AggregateView {
    let key = agg_view_key(collection, &group_keys, &aggs);

    AGGREGATE_VIEWS.with(|views| {
        let mut map = views.lock().unwrap();
        map.entry(key)
            .or_insert_with(|| AggregateView {
                collection: collection.to_string(),
                group_keys,
                aggs,
                data: FxHashMap::default(),
                r#gen,
            })
            .clone()
    })
}

/// Update aggregate view with a new row's data
#[allow(dead_code)]
pub fn update_aggregate(view: &mut AggregateView, group_values: &[String], row: &Row) {
    let group_key = group_values.join(":");
    let entry = view.data.entry(group_key).or_default();

    for agg in &view.aggs {
        match agg {
            AggregateType::Count => {
                entry.inc_count();
            }
            AggregateType::Sum | AggregateType::Avg | AggregateType::Min | AggregateType::Max => {
                if let Some(field) = view.group_keys.first() {
                    if let Some(cell) = row.get(field)
                        && let Some(value) = cell.as_f64()
                    {
                        entry.add_value(value);
                    }
                } else {
                    for (_, cell) in row.iter() {
                        if let Some(value) = cell.as_f64() {
                            entry.add_value(value);
                        }
                    }
                }
            }
        }
    }
}

/// Get computed aggregate results for a group
#[allow(dead_code)]
pub fn get_aggregate_result(entry: &AggregateValue, aggs: &[AggregateType]) -> Vec<(String, Cell)> {
    let mut result = Vec::with_capacity(aggs.len());

    for agg in aggs {
        let cell = match agg {
            AggregateType::Count => Cell::Int(entry.count as i64),
            AggregateType::Sum => Cell::Float(entry.sum),
            AggregateType::Avg => Cell::Float(entry.avg().unwrap_or(0.0)),
            AggregateType::Min => Cell::Float(entry.min.unwrap_or(0.0)),
            AggregateType::Max => Cell::Float(entry.max.unwrap_or(0.0)),
        };
        result.push((format!("{}", agg), cell));
    }

    result
}

/// Invalidate aggregate views after store mutation
#[allow(dead_code)]
pub fn invalidate_aggregates(r#gen: u64) {
    AGGREGATE_VIEWS.with(|views| {
        let mut map = views.lock().unwrap();
        map.retain(|_, view| view.r#gen >= r#gen);
    });
}

/// Clear all cached aggregate views
#[allow(dead_code)]
pub fn clear_aggregate_cache() {
    AGGREGATE_VIEWS.with(|views| {
        if let Ok(mut map) = views.lock() {
            map.clear();
        }
    });
}

/// Update precomputed aggregates for an INSERT operation in-place
#[allow(dead_code)]
pub fn update_aggregate_for_insert(collection: &str, row: &Row) {
    AGGREGATE_VIEWS.with(|views| {
        let mut map = views.lock().unwrap();

        for (_, view) in map.iter_mut() {
            if view.collection != collection {
                continue;
            }

            let group_values: Vec<String> = view
                .group_keys
                .iter()
                .filter_map(|field| row.get(field).map(|c| c.compact()))
                .collect();

            update_aggregate(view, &group_values, row);
        }
    });
}

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

/// Get precomputed content hash from cache, or compute and store it
#[allow(dead_code)]
fn cached_content_hash(body: &str) -> Arc<str> {
    CONTENT_HASH_CACHE.with(|cache| {
        let mut guard = cache.lock().unwrap();

        if let Some(cached) = guard.get(body) {
            return cached.clone();
        }

        let hash = content_hash_arc(body);
        guard.insert(body.to_owned(), hash.clone());

        while guard.len() >= 1024 {
            if let Some(key) = guard.keys().next().cloned() {
                guard.remove(&key);
            } else {
                break;
            }
        }

        hash
    })
}

/// Convert statements to query source string for hashing
#[cfg(feature = "parallel")]
#[allow(dead_code)]
fn to_query_source(stmts: &[Stmt]) -> Result<String, Error> {
    if stmts.len() == 1 {
        match &stmts[0] {
            Stmt::Query(q) => Ok(format!("{q:?}")),
            _ => Ok(String::new()),
        }
    } else {
        Ok(stmts
            .iter()
            .map(|s| format!("{s:?}"))
            .collect::<Vec<_>>()
            .join("; "))
    }
}

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

impl Handle {
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
    base_generation: u64,
    committed: bool,
    aborted: bool,
    undo: Undo,
    cat_backup: Option<Catalog>,
    packs: Vec<Pack>,
    schema_changed: bool,
    mutation_count: usize,
    journal_bytes: usize,
}

impl<'a> Transaction<'a> {
    /// Execute a source string statement within this transaction.
    /// A failed run aborts the transaction; only rollback or Drop may follow.
    pub fn run(&mut self, src: &str) -> Result<Handle, Error> {
        if self.aborted {
            return Err(Error::runtime("transaction aborted: rollback required"));
        }
        let prepared = match self.db.prepare(src) {
            Ok(prepared) => prepared,
            Err(error) => {
                self.aborted = true;
                return Err(error);
            }
        };
        self.run_prepared(&prepared)
    }

    /// Prepare a query using the database catalog.
    pub fn prepare(&mut self, src: &str) -> Result<Prepared, Error> {
        if self.aborted {
            return Err(Error::runtime("transaction aborted: rollback required"));
        }
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
                    self.undo = Undo::Rows(Vec::new());
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
                            events: Vec::new(),
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
    /// The returned generation is the last committed generation until commit.
    /// Execution or quota errors abort the transaction and prevent commit.
    pub fn run_prepared(&mut self, prep: &Prepared) -> Result<Handle, Error> {
        if self.aborted {
            return Err(Error::runtime("transaction aborted: rollback required"));
        }
        let result = self.run_prepared_inner(prep);
        if result.is_err() {
            self.aborted = true;
        }
        result
    }

    fn run_prepared_inner(&mut self, prep: &Prepared) -> Result<Handle, Error> {
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

    /// Commit all mutations as one WAL frame and advance the generation once.
    ///
    /// If checkpoint maintenance fails after WAL publication, the error is
    /// returned but the transaction remains committed.
    pub fn commit(mut self) -> Result<(), Error> {
        if self.aborted {
            return Err(Error::runtime("transaction aborted: cannot commit"));
        }
        if self.committed {
            return Ok(());
        }
        let pack = fold_packs(std::mem::take(&mut self.packs));
        if let Some(pack) = pack {
            if let Some(p) = self.db.persist.as_mut() {
                self.db.store.durable_commit(p, pack)?;
                if self.schema_changed {
                    p.catalog_hash = crate::store::catalog_hash(&self.db.catalog);
                }
            }
            self.db.store.r#gen += 1;
        }
        // WAL publication is the commit point. A later maintenance failure must
        // not make Drop undo data already visible to recovery and followers.
        self.committed = true;
        if let Some(p) = self.db.persist.as_mut() {
            self.db.store.maybe_checkpoint(p)?;
        }
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
        self.db.store.r#gen = self.base_generation;
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
            self.db.store.r#gen = self.base_generation;
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
        let _profile = crate::mutation_profile::phase(1);
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
        Ok(Prepared {
            src: String::new(),
            stmts,
            plan,
            writes,
            append_only,
            schema,
            point_get,
            agg_plan,
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
        if let Some(pg) = &prepared.point_get {
            let t0 = Instant::now();
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
            let ms = t0.elapsed().as_secs_f64() * 1000.0;
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
            Undo::Rows(Vec::new())
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
                if let Err(e) = self
                    .store
                    .durable_commit_prepared(p, pack, prepared_encoding)
                {
                    self.rollback(undo, cat_backup);
                    return Err(e);
                }
                // Publish schema metadata only after WAL append succeeds. A rejected
                // record rolls back the catalog and must retain its committed hash.
                if prepared.schema {
                    p.catalog_hash = crate::store::catalog_hash(&self.catalog);
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
        let out_bytes = rows
            .iter()
            .map(|r| crate::store::row_byte_size(r))
            .sum::<usize>();
        if out_bytes > self.quotas.max_output_bytes {
            return Err(Error::runtime(format!(
                "query output bytes {} exceeds max_output_bytes quota {}",
                out_bytes, self.quotas.max_output_bytes
            )));
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
            let t0 = Instant::now();
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
            let ms = t0.elapsed().as_secs_f64() * 1000.0;
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
        let out_bytes = rows
            .iter()
            .map(|r| crate::store::row_byte_size(r))
            .sum::<usize>();
        if out_bytes > self.quotas.max_output_bytes {
            return Err(Error::runtime(format!(
                "query output bytes {} exceeds max_output_bytes quota {}",
                out_bytes, self.quotas.max_output_bytes
            )));
        }
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
        let _profile = crate::mutation_profile::phase(7);
        match undo {
            Undo::None => {}
            Undo::Append(mark) => self.store.append_rollback(mark),
            Undo::Full(b) => self.store.mem_restore(b),
            Undo::Rows(events) => {
                for event in events.into_iter().rev() {
                    match event {
                        RowUndo::Update(collection, idx, row) => {
                            if let Some(mut restored) = self.store.collection(&collection).get(idx).cloned() {
                                row.apply(&mut restored);
                                self.store.restore_row_owned(&collection, idx, restored);
                            }
                        }
                        RowUndo::Delete(collection, positions, rows) => {
                            self.store.restore_rows_at(&collection, &positions, &rows)
                        }
                    }
                }
            }
            Undo::Mixed { append, events } => {
                for event in events.into_iter().rev() {
                    match event {
                        RowUndo::Update(collection, idx, row) => {
                            if let Some(mut restored) = self.store.collection(&collection).get(idx).cloned() {
                                row.apply(&mut restored);
                                self.store.restore_row_owned(&collection, idx, restored);
                            }
                        }
                        RowUndo::Delete(collection, positions, rows) => {
                            self.store.restore_rows_at(&collection, &positions, &rows)
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

    /// Begin an interactive ACID transaction.
    ///
    /// All mutations executed through [`Transaction`] remain isolated until [`Transaction::commit`].
    /// If dropped without commit, all changes are automatically rolled back.
    pub fn transaction(&mut self) -> Result<Transaction<'_>, Error> {
        let base_generation = self.store.r#gen;
        Ok(Transaction {
            db: self,
            base_generation,
            committed: false,
            aborted: false,
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
        let _profile = crate::mutation_profile::phase(0);
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
                    && let Some(uses) = crate::index::pick_index(&self.catalog, name, pred)
                    && let Some(idxs) = self.store.index_seek(name, &uses, now)
                {
                    let covered = crate::index::index_covers_pred(pred, &uses);
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
                        Some(by) => agg_count(&rows, by, self.quotas.max_aggregate_bytes)?,
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
                    rows = agg_sum(&rows, field, by.as_ref(), self.quotas.max_aggregate_bytes)?;
                    primary = String::new();
                }
                Step::Min { field, by } => {
                    implicit_take = false;
                    saw_agg = true;
                    rows = agg_min(&rows, field, by.as_ref(), self.quotas.max_aggregate_bytes)?;
                    primary = String::new();
                }
                Step::Max { field, by } => {
                    implicit_take = false;
                    saw_agg = true;
                    rows = agg_max(&rows, field, by.as_ref(), self.quotas.max_aggregate_bytes)?;
                    primary = String::new();
                }
                Step::Avg { field, by } => {
                    implicit_take = false;
                    saw_agg = true;
                    rows = agg_avg(&rows, field, by.as_ref(), self.quotas.max_aggregate_bytes)?;
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
            && let Some(uses) = crate::index::pick_index(&self.catalog, &plan.collection, pred)
            && crate::index::index_covers_pred(pred, &uses)
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
                            let mut map: FxHashMap<Arc<str>, f64> = FxHashMap::default();
                            for (uid, &tot) in uids.iter().zip(totals) {
                                *map.entry(Arc::clone(uid)).or_insert(0.0) += tot;
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
                                for (uid, &tot) in uids.iter().zip(totals) {
                                    if tot > threshold {
                                        *map.entry(Arc::clone(uid)).or_insert(0.0) += tot;
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
                let mut map: FxHashMap<Arc<str>, i64> = FxHashMap::default();
                for w in self.store.docs_wing() {
                    *map.entry(Arc::clone(w)).or_insert(0) += 1;
                }
                let mut entries: Vec<(Arc<str>, i64)> = map.into_iter().collect();
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

        // 3. Generic fallback: iterate `&[Row]` without cloning!
        let col = self.store.collection(&plan.collection);
        let filter = plan.filter.as_ref();
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
                let bk = by.as_str();
                let mut map: FxHashMap<Cell, i64> = FxHashMap::default();
                let mut state_bytes = 0usize;
                for r in col {
                    if filter.is_none_or(|p| eval_pred(p, r, now)) {
                        let cell = r.get(&bk).map(Cell::clone_for_borrow).unwrap_or(Cell::Null);
                        if !map.contains_key(&cell) {
                            let key_size = match &cell {
                                Cell::Text(s) => s.len() + 32,
                                _ => 16,
                            };
                            state_bytes = state_bytes.saturating_add(key_size + 64);
                            if state_bytes > self.quotas.max_aggregate_bytes {
                                return Err(Error::runtime(format!(
                                    "aggregate state bytes {state_bytes} exceeds max_aggregate_bytes quota {}",
                                    self.quotas.max_aggregate_bytes
                                )));
                            }
                        }
                        *map.entry(cell).or_insert(0) += 1;
                    }
                }
                Ok(count_cell_map_to_rows(&bk, map))
            }
            AggKind::Sum { field, by } => {
                let fk = field.as_str();
                let Some(by) = by else {
                    let mut total = 0.0f64;
                    for r in col {
                        if filter.is_none_or(|p| eval_pred(p, r, now)) {
                            total += r.get(&fk).and_then(Cell::as_f64).unwrap_or(0.0);
                        }
                    }
                    let mut row = crate::store::acquire_row();
                    row.insert(fk, Cell::Float(total));
                    return Ok(vec![row]);
                };
                let bk = by.as_str();
                let mut map: FxHashMap<Cell, f64> = FxHashMap::default();
                let mut state_bytes = 0usize;
                for r in col {
                    if filter.is_none_or(|p| eval_pred(p, r, now)) {
                        let cell = r.get(&bk).map(Cell::clone_for_borrow).unwrap_or(Cell::Null);
                        if !map.contains_key(&cell) {
                            let key_size = match &cell {
                                Cell::Text(s) => s.len() + 32,
                                _ => 16,
                            };
                            state_bytes = state_bytes.saturating_add(key_size + 64);
                            if state_bytes > self.quotas.max_aggregate_bytes {
                                return Err(Error::runtime(format!(
                                    "aggregate state bytes {state_bytes} exceeds max_aggregate_bytes quota {}",
                                    self.quotas.max_aggregate_bytes
                                )));
                            }
                        }
                        let n = r.get(&fk).and_then(Cell::as_f64).unwrap_or(0.0);
                        *map.entry(cell).or_insert(0.0) += n;
                    }
                }
                let mut entries: Vec<(Cell, f64)> = map.into_iter().collect();
                entries.sort_unstable_by(|(a, _), (b, _)| cmp_sort(a, b));
                Ok(entries
                    .into_iter()
                    .map(|(cell, n)| {
                        let mut row = crate::store::acquire_row();
                        row.insert(bk.clone(), cell);
                        row.insert(fk.clone(), Cell::Float(n));
                        row
                    })
                    .collect())
            }
            AggKind::Avg { field, by } => {
                let fk = field.as_str();
                let Some(by) = by else {
                    let mut sum = 0.0f64;
                    let mut count = 0usize;
                    for r in col {
                        if filter.is_none_or(|p| eval_pred(p, r, now)) {
                            if let Some(v) = r.get(&fk).and_then(Cell::as_f64) {
                                sum += v;
                                count += 1;
                            }
                        }
                    }
                    let avg = if count == 0 {
                        0.0
                    } else {
                        sum / (count as f64)
                    };
                    let mut row = crate::store::acquire_row();
                    row.insert(fk, Cell::Float(avg));
                    return Ok(vec![row]);
                };
                let bk = by.as_str();
                let mut map: FxHashMap<Cell, (f64, usize)> = FxHashMap::default();
                let mut state_bytes = 0usize;
                for r in col {
                    if filter.is_none_or(|p| eval_pred(p, r, now)) {
                        let cell = r.get(&bk).map(Cell::clone_for_borrow).unwrap_or(Cell::Null);
                        if !map.contains_key(&cell) {
                            let key_size = match &cell {
                                Cell::Text(s) => s.len() + 32,
                                _ => 16,
                            };
                            state_bytes = state_bytes.saturating_add(key_size + 64);
                            if state_bytes > self.quotas.max_aggregate_bytes {
                                return Err(Error::runtime(format!(
                                    "aggregate state bytes {state_bytes} exceeds max_aggregate_bytes quota {}",
                                    self.quotas.max_aggregate_bytes
                                )));
                            }
                        }
                        let entry = map.entry(cell).or_insert((0.0, 0));
                        if let Some(v) = r.get(&fk).and_then(Cell::as_f64) {
                            entry.0 += v;
                            entry.1 += 1;
                        }
                    }
                }
                let mut entries: Vec<(Cell, (f64, usize))> = map.into_iter().collect();
                entries.sort_unstable_by(|(a, _), (b, _)| cmp_sort(a, b));
                Ok(entries
                    .into_iter()
                    .map(|(gcell, (sum, count))| {
                        let avg = if count == 0 {
                            0.0
                        } else {
                            sum / (count as f64)
                        };
                        let mut row = crate::store::acquire_row();
                        row.insert(bk.clone(), gcell);
                        row.insert(fk.clone(), Cell::Float(avg));
                        row
                    })
                    .collect())
            }
            AggKind::Min { field, by } => {
                let fk = field.as_str();
                let Some(by) = by else {
                    let mut best: Option<Cell> = None;
                    for r in col {
                        if filter.is_none_or(|p| eval_pred(p, r, now)) {
                            if let Some(c) = r.get(&fk).filter(|c| !matches!(c, Cell::Null)) {
                                best = Some(match best {
                                    None => c.clone_for_borrow(),
                                    Some(prev) => {
                                        if cmp_sort(c, &prev).is_lt() {
                                            c.clone_for_borrow()
                                        } else {
                                            prev
                                        }
                                    }
                                });
                            }
                        }
                    }
                    let mut row = crate::store::acquire_row();
                    row.insert(fk, best.unwrap_or(Cell::Null));
                    return Ok(vec![row]);
                };
                let bk = by.as_str();
                let mut map: FxHashMap<Cell, Option<Cell>> = FxHashMap::default();
                let mut state_bytes = 0usize;
                for r in col {
                    if filter.is_none_or(|p| eval_pred(p, r, now)) {
                        let cell = r.get(&bk).map(Cell::clone_for_borrow).unwrap_or(Cell::Null);
                        if !map.contains_key(&cell) {
                            let key_size = match &cell {
                                Cell::Text(s) => s.len() + 32,
                                _ => 16,
                            };
                            state_bytes = state_bytes.saturating_add(key_size + 64);
                            if state_bytes > self.quotas.max_aggregate_bytes {
                                return Err(Error::runtime(format!(
                                    "aggregate state bytes {state_bytes} exceeds max_aggregate_bytes quota {}",
                                    self.quotas.max_aggregate_bytes
                                )));
                            }
                        }
                        let f_val = r.get(&fk).filter(|c| !matches!(c, Cell::Null));
                        let entry = map.entry(cell).or_insert(None);
                        if let Some(c) = f_val {
                            *entry = Some(match entry.take() {
                                None => c.clone_for_borrow(),
                                Some(prev) => {
                                    if cmp_sort(c, &prev).is_lt() {
                                        c.clone_for_borrow()
                                    } else {
                                        prev
                                    }
                                }
                            });
                        }
                    }
                }
                let mut entries: Vec<(Cell, Option<Cell>)> = map.into_iter().collect();
                entries.sort_unstable_by(|(a, _), (b, _)| cmp_sort(a, b));
                Ok(entries
                    .into_iter()
                    .map(|(gcell, val)| {
                        let mut row = crate::store::acquire_row();
                        row.insert(bk.clone(), gcell);
                        row.insert(fk.clone(), val.unwrap_or(Cell::Null));
                        row
                    })
                    .collect())
            }
            AggKind::Max { field, by } => {
                let fk = field.as_str();
                let Some(by) = by else {
                    let mut best: Option<Cell> = None;
                    for r in col {
                        if filter.is_none_or(|p| eval_pred(p, r, now)) {
                            if let Some(c) = r.get(&fk).filter(|c| !matches!(c, Cell::Null)) {
                                best = Some(match best {
                                    None => c.clone_for_borrow(),
                                    Some(prev) => {
                                        if cmp_sort(c, &prev).is_gt() {
                                            c.clone_for_borrow()
                                        } else {
                                            prev
                                        }
                                    }
                                });
                            }
                        }
                    }
                    let mut row = crate::store::acquire_row();
                    row.insert(fk, best.unwrap_or(Cell::Null));
                    return Ok(vec![row]);
                };
                let bk = by.as_str();
                let mut map: FxHashMap<Cell, Option<Cell>> = FxHashMap::default();
                let mut state_bytes = 0usize;
                for r in col {
                    if filter.is_none_or(|p| eval_pred(p, r, now)) {
                        let cell = r.get(&bk).map(Cell::clone_for_borrow).unwrap_or(Cell::Null);
                        if !map.contains_key(&cell) {
                            let key_size = match &cell {
                                Cell::Text(s) => s.len() + 32,
                                _ => 16,
                            };
                            state_bytes = state_bytes.saturating_add(key_size + 64);
                            if state_bytes > self.quotas.max_aggregate_bytes {
                                return Err(Error::runtime(format!(
                                    "aggregate state bytes {state_bytes} exceeds max_aggregate_bytes quota {}",
                                    self.quotas.max_aggregate_bytes
                                )));
                            }
                        }
                        let f_val = r.get(&fk).filter(|c| !matches!(c, Cell::Null));
                        let entry = map.entry(cell).or_insert(None);
                        if let Some(c) = f_val {
                            *entry = Some(match entry.take() {
                                None => c.clone_for_borrow(),
                                Some(prev) => {
                                    if cmp_sort(c, &prev).is_gt() {
                                        c.clone_for_borrow()
                                    } else {
                                        prev
                                    }
                                }
                            });
                        }
                    }
                }
                let mut entries: Vec<(Cell, Option<Cell>)> = map.into_iter().collect();
                entries.sort_unstable_by(|(a, _), (b, _)| cmp_sort(a, b));
                Ok(entries
                    .into_iter()
                    .map(|(gcell, val)| {
                        let mut row = crate::store::acquire_row();
                        row.insert(bk.clone(), gcell);
                        row.insert(fk.clone(), val.unwrap_or(Cell::Null));
                        row
                    })
                    .collect())
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
                if std::ptr::eq(rows, col_rows)
                    || (rows.len() == col_rows.len() && !rows.is_empty())
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
        let mut scored = vectors
            .iter()
            .enumerate()
            .filter_map(|(slot, (i, v))| {
                #[cfg(feature = "gpu")]
                let score = gpu_scores
                    .as_ref()
                    .map_or_else(|| embed::cosine(&qv, v), |s| s[slot] as f64);
                #[cfg(not(feature = "gpu"))]
                let score = {
                    let _ = slot;
                    embed::cosine(&qv, v)
                };
                (score > 0.01).then_some((score, *i))
            })
            .collect::<Vec<_>>();
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
        for (i, user_id) in users_id.iter().enumerate() {
            probe.insert(user_id.as_ref(), i);
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
            let embedding = emb.embed(&text);
            row.insert("embedding".into(), Cell::Vec(embedding));
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
        if texts.is_empty() {
            return;
        }
        let refs = texts.iter().map(|text| text.as_ref()).collect::<Vec<_>>();
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
        Ok(self.store.append_fact_row(&row))
    }

    fn append_edge(&mut self, rel: &str, from: &Value, to: &Value) -> Result<(Row, bool), Error> {
        let now = now_ms();
        let from = value_text(from, now);
        let to = value_text(to, now);
        let changed = self.store.append_edge_parts(rel, &from, &to);
        Ok((edge_row(rel, &from, &to), changed))
    }

    /// Append edges in bulk with pre-allocated vectors and zero-copy Arc reuse
    #[allow(dead_code)]
    fn append_edges_batch(
        &mut self,
        edges: &[(String, Value, Value)],
    ) -> Result<Vec<(Row, bool)>, Error> {
        let now = now_ms();
        let mut results = Vec::with_capacity(edges.len());

        for (rel, from, to) in edges {
            let from_id = value_text(from, now);
            let to_id = value_text(to, now);
            let changed = self.store.append_edge_parts(rel, &from_id, &to_id);
            // Reuse Arcs from input values to avoid extra allocations
            let row = edge_row(rel, &from_id, &to_id);
            results.push((row, changed));
        }

        Ok(results)
    }

    fn check_patch_fks(&self, collection: &str, before: &Row, patch: &Row) -> Result<(), Error> {
        for fk in &self.catalog.fks {
            if fk.from_col != collection { continue; }
            let Some(val) = patch.get(&fk.from_field).or_else(||before.get(&fk.from_field)).and_then(Cell::text) else { continue; };
            if val.is_empty() { continue; }
            if !self.fk_target_exists(&fk.to_col,&fk.to_field,val) {
                return Err(Error::runtime(format!("fk: {collection}.{} → {}.{} missing {val}",fk.from_field,fk.to_col,fk.to_field)));
            }
        }
        Ok(())
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
        let mut built: Vec<Row> = if n >= 4096 {
            std::thread::scope(|scope| {
                let (left, right) = records.split_at(n / 2);
                let worker = std::thread::Builder::new().spawn_scoped(scope, || {
                    let mut rows = Vec::with_capacity(right.len());
                    append_batch_shape_rows(&mut rows, right, now);
                    rows
                });
                let mut rows = Vec::with_capacity(n);
                append_batch_shape_rows(&mut rows, left, now);
                append_bulk_worker_rows(&mut rows, right, now, worker);
                rows
            })
        } else {
            let mut rows = Vec::with_capacity(n);
            append_batch_shape_rows(&mut rows, records, now);
            rows
        };
        let mut batch_ids: rustc_hash::FxHashSet<&str> = rustc_hash::FxHashSet::default();
        batch_ids.reserve(n);
        let mut batch_uris: rustc_hash::FxHashSet<&str> = rustc_hash::FxHashSet::default();
        if collection == "docs" {
            batch_uris.reserve(n);
        }

        let store_empty = self.store.collection(collection).is_empty();
        let has_fks = self.catalog.fks.iter().any(|fk| fk.from_col == collection);

        for row in &mut built {
            if row.get("id").and_then(Cell::text).is_none() {
                row.insert("id".into(), Cell::text_arc(self.store.alloc_id()));
            }
            if row.get("hash").and_then(Cell::text).is_none()
                && let Some(body) = row.get("body").and_then(Cell::text_shared)
            {
                // Large short-body batches bypass the cache; small batches retain reuse.
                row.insert(
                    "hash".into(),
                    Cell::Text(if n >= 4096 && body.len() <= 64 {
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
            if collection == "docs"
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
        drop(batch_ids);
        drop(batch_uris);
        self.maybe_embed_rows(collection, &mut built);

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
                let floats = built
                    .iter()
                    .filter_map(|row| row.get("embedding").and_then(Cell::as_vec))
                    .fold(0usize, |total, vector| total.saturating_add(vector.len()));
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
                bytes,
                mode,
            );
            let maintenance = self.maintain_insert_slab(collection, start, &shared);
            let staged = match task {
                Ok(reply) => reply.recv().unwrap_or_else(|_| {
                    crate::persist::prepare_insert_columns(
                        generation,
                        next_id,
                        collection,
                        &shared,
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
        self.store.collection_mut(collection).reserve(built.len());
        self.store.row_maps_reserve(collection, built.len());
        self.store.index_insert_slab(collection, start, built)?;
        self.store.fts_insert_slab(collection, start, built);
        self.store.hnsw_insert_slab(collection, start, built);
        self.store.row_maps_register_slab(collection, start, built);
        Ok(())
    }

    fn mutation_positions(&self, collection: &str, pred: Option<&Pred>, now: i64) -> Vec<usize> {
        if let Some(pred) = pred
            && let Some((field, key)) = point_key(pred)
            && (field == "id" || (field == "uri" && collection == "docs"))
        {
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
        let patch = record_row(record, now);
        let mut effective_patch = patch.clone();
        if patch.contains_key("body")
            && let Some(body) = row_text(&patch, "body")
        {
            effective_patch.insert("hash".into(), Cell::Text(cached_content_hash(body)));
        }
        let mut out = Vec::new();
        for i in idxs {
            let before = &self.store.collection(collection)[i];
            for (k, _) in &patch {
                for g in &self.catalog.guards {
                    if g.collection == collection && &g.field == k && eval_pred(&g.pred, before, now) {
                        return Err(Error::runtime(format!("immutable field: {collection}.{k}")));
                    }
                }
            }
            self.check_patch_fks(collection, before, &effective_patch)?;
            let had_embedding = before.contains_key("embedding");
            let identity_before: Option<Row> = if effective_patch.contains_key("id") || effective_patch.contains_key("uri") {
                Some(before.iter().filter(|(k,_)| k.as_str()=="id" || k.as_str()=="uri").map(|(k,v)|(k.clone(),v.clone())).collect())
            } else { None };
            let fts_before: Option<Row> = self.store.fts.get(collection).and_then(|fts| {
                let touched = effective_patch.contains_key("snippet") || fts.fields.iter().any(|field|effective_patch.contains_key(field));
                touched.then(||fts.fields.iter().map(String::as_str).chain(std::iter::once("snippet")).filter_map(|field|before.get(field).map(|value|(field.to_owned(),value.clone()))).collect())
            });
            if let Undo::Rows(events) | Undo::Mixed { events, .. } = ctx.undo {
                events.push(RowUndo::Update(collection.to_string(), i, RowDelta::capture(before, &effective_patch, false)));
            }
            let live = &mut self.store.collection_mut(collection)[i];
            for (key,value) in &effective_patch {
                if let Some(slot) = live.get_mut(key.as_str()) { *slot=value.clone(); }
                else { live.insert(key.clone(),value.clone()); }
            }
            live.ensure_shared();
            let updated = live.clone();
            self.store.index_insert_row(collection, i, &updated)?;
            if let Some(before) = fts_before {
                if let Some(fts) = self.store.fts.get_mut(collection) { fts.sync_row(i, &before, &updated); }
            }
            if let Some(hnsw) = self.store.hnsw.get_mut(collection) {
                if let Some(Cell::Vec(v)) = updated.get("embedding") { hnsw.insert(i,v.clone()); }
                else if had_embedding { hnsw.remove(i); }
            }
            self.store.row_maps_sync_updated(collection,i,identity_before.as_ref().unwrap_or(&updated),&updated);
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
        let out: Vec<Row> = idxs.iter().map(|&i| rows[i].clone()).collect();

        if let Undo::Rows(events) | Undo::Mixed { events, .. } = ctx.undo {
            events.push(RowUndo::Delete(
                collection.to_string(),
                idxs.clone(),
                out.clone(),
            ));
        }
        self.store.delete_rows_at(collection, &idxs);
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
    Rows(Vec<RowUndo>),
    Mixed {
        append: crate::store::AppendMark,
        events: Vec<RowUndo>,
    },
}

pub(crate) struct RowDelta(smallvec::SmallVec<[(String, Option<Cell>); 1]>);
impl RowDelta {
    fn capture(before: &Row, patch: &Row, derived_hash: bool) -> Self {
        let mut edits = smallvec::SmallVec::new();
        for key in patch.keys() {
            edits.push((key.clone(), before.get(key).cloned()));
        }
        if derived_hash && !patch.contains_key("hash") {
            edits.push(("hash".to_owned(), before.get("hash").cloned()));
        }
        Self(edits)
    }
    fn apply(self, row: &mut Row) {
        for (key, value) in self.0 {
            match value {
                Some(value) => {
                    if let Some(slot) = row.get_mut(key.as_str()) { *slot = value; }
                    else { row.insert(key, value); }
                }
                None => { row.remove(key.as_str()); }
            }
        }
    }
}

pub(crate) enum RowUndo {
    Update(String, usize, RowDelta),
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

#[allow(dead_code)]
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
    let mut out = Row::new();
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
    let mut out = Row::new();
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
    let mut r = Row::new();
    r.insert("kind".into(), Cell::text_arc(kind));
    r.insert("name".into(), Cell::text_arc(name));
    r
}

fn edge_row(rel: &str, from: &str, to: &str) -> Row {
    let mut r = Row::new();
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

fn append_batch_shape_rows(rows: &mut Vec<Row>, records: &[Record], now: i64) {
    let Some(first) = records.first() else {
        return;
    };
    if records.len() < 128 || first.fields.is_empty() || first.fields.len() > 32 {
        rows.extend(records.iter().map(|record| record_row(record, now)));
        return;
    }
    let template = first
        .fields
        .iter()
        .map(|(key, _)| (key.clone(), Cell::Null))
        .collect::<Row>();
    let slots = template
        .keys()
        .map(|key| {
            first
                .fields
                .iter()
                .rposition(|(field, _)| field == key)
                .unwrap()
        })
        .collect::<Vec<_>>();
    for record in records {
        if record.fields.len() != first.fields.len()
            || !record
                .fields
                .iter()
                .map(|(key, _)| key.as_str())
                .eq(first.fields.iter().map(|(key, _)| key.as_str()))
        {
            rows.push(record_row(record, now));
            continue;
        }
        let mut row = template.clone();
        for (cell, &source) in row.values_mut().zip(&slots) {
            *cell = value_cell(&record.fields[source].1, now);
        }
        rows.push(row);
    }
}

fn record_row(record: &Record, now: i64) -> Row {
    let mut row = Row::new();
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
                append_batch_shape_rows(&mut actual, &records, now);
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
            append_batch_shape_rows(&mut actual, &records, 55);
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

fn value_cell(v: &Value, now: i64) -> Cell {
    match v {
        Value::String(s) => Cell::Text(std::sync::Arc::clone(s)),
        Value::Int(n) => Cell::Int(*n),
        Value::Float(n) => Cell::Float(*n),
        Value::Bool(b) => Cell::Bool(*b),
        Value::Timestamp(millis) => Cell::Time(*millis),
        Value::Now => Cell::Time(now),
        Value::NowMinus(d) => Cell::Time(now - d.as_millis()),
        Value::Duration(d) => Cell::Int(d.as_millis()),
        Value::Name(n) => Cell::text_arc(n.as_str()),
    }
}

fn value_text(v: &Value, now: i64) -> String {
    let c = value_cell(v, now);
    if let Some(s) = c.text() {
        s.to_owned()
    } else {
        c.compact()
    }
}

fn point_key(pred: &Pred) -> Option<(&str, &str)> {
    match pred {
        Pred::Cmp {
            field,
            op: CmpOp::Eq,
            value: Value::String(s),
        } if field.parts.len() == 1 && matches!(field.parts[0].as_str(), "id" | "uri") => {
            Some((field.parts[0].as_str(), s.as_ref()))
        }
        Pred::And(a, b) => point_key(a).or_else(|| point_key(b)),
        _ => None,
    }
}

pub(crate) fn eval_pred(pred: &Pred, row: &Row, now: i64) -> bool {
    match pred {
        Pred::And(a, b) => eval_pred(a, row, now) && eval_pred(b, row, now),
        Pred::Or(a, b) => eval_pred(a, row, now) || eval_pred(b, row, now),
        Pred::Not(inner) => !eval_pred(inner, row, now),
        Pred::IsNull { field, negated } => {
            let is_null = matches!(field_cell(row, field), Cell::Null);
            if *negated { !is_null } else { is_null }
        }
        Pred::In { field, values } => {
            let left = field_cell(row, field);
            values.iter().any(|v| cell_eq_val(left, v, now))
        }
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

            // Cache compiled regex instances per (pattern, flags) combo
            // First query compiles, subsequent queries reuse cached Regex
            let key = (pattern.clone(), flags.chars().collect::<String>());
            REGEX_CACHE.with(|cache| {
                let mut cache_guard = cache.lock().unwrap();
                let re = cache_guard.entry(key).or_insert_with(|| {
                    let mut b = regex::RegexBuilder::new(pattern);
                    b.size_limit(10 * 1024 * 1024);
                    if flags.contains('i') {
                        b.case_insensitive(true);
                    }
                    Arc::new(b.build().expect("invalid regex pattern"))
                });
                re.is_match(text)
            })
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
    row.get(&want).unwrap_or(&NULL)
}

#[inline]
fn int_eq_f64(i: i64, f: f64) -> bool {
    if !f.is_finite() || f.fract() != 0.0 {
        return false;
    }
    if f < -9223372036854775808.0 || f >= 9223372036854775808.0 {
        return false;
    }
    let fi = f as i64;
    (fi as f64 == f) && (i == fi)
}

#[inline]
fn cmp_i64_f64(i: i64, f: f64) -> Option<std::cmp::Ordering> {
    if f.is_nan() {
        return None;
    }
    if f.is_infinite() {
        return if f.is_sign_positive() {
            Some(std::cmp::Ordering::Less)
        } else {
            Some(std::cmp::Ordering::Greater)
        };
    }
    if f < -9223372036854775808.0 {
        return Some(std::cmp::Ordering::Greater);
    }
    if f >= 9223372036854775808.0 {
        return Some(std::cmp::Ordering::Less);
    }
    let fi = f as i64;
    let f_back = fi as f64;
    if f_back == f {
        Some(i.cmp(&fi))
    } else if f > f_back {
        if i <= fi {
            Some(std::cmp::Ordering::Less)
        } else {
            Some(std::cmp::Ordering::Greater)
        }
    } else if i >= fi {
        Some(std::cmp::Ordering::Greater)
    } else {
        Some(std::cmp::Ordering::Less)
    }
}

#[inline]
fn cell_eq_val(left: &Cell, value: &Value, now: i64) -> bool {
    match (left, value) {
        (Cell::Text(x), Value::String(y)) => x.as_ref() == y.as_ref(),
        (Cell::Text(x), Value::Name(y)) => x.as_ref() == y.as_str(),
        (Cell::SmallText(x), Value::String(y)) => x.as_str() == y.as_ref(),
        (Cell::SmallText(x), Value::Name(y)) => x.as_str() == y.as_str(),
        (Cell::Bool(x), Value::Bool(y)) => *x == *y,
        (Cell::Int(x), Value::Int(y)) => *x == *y,
        (Cell::Float(x), Value::Float(y)) => {
            if x.is_nan() || y.is_nan() {
                false
            } else {
                *x == *y
            }
        }
        (Cell::Int(x), Value::Float(y)) => int_eq_f64(*x, *y),
        (Cell::Float(x), Value::Int(y)) => int_eq_f64(*y, *x),
        (Cell::Time(x), Value::Timestamp(y)) => *x == *y,
        (Cell::Time(x), Value::Now) => *x == now,
        (Cell::Time(x), Value::NowMinus(d)) => *x == now - d.as_millis(),
        (Cell::Int(x), Value::Duration(d)) => *x == d.as_millis(),
        (Cell::Float(x), Value::Duration(d)) => int_eq_f64(d.as_millis(), *x),
        _ => false,
    }
}

fn cmp_cell(left: &Cell, value: &Value, op: CmpOp, now: i64) -> bool {
    match op {
        CmpOp::Eq => cell_eq_val(left, value, now),
        CmpOp::Ne => !cell_eq_val(left, value, now),
        CmpOp::Gt | CmpOp::Lt | CmpOp::Ge | CmpOp::Le => {
            let ord = match (left, value) {
                (Cell::Int(l), Value::Int(r)) => Some(l.cmp(r)),
                (Cell::Time(l), Value::Timestamp(r)) => Some(l.cmp(r)),
                (Cell::Time(l), Value::Now) => Some(l.cmp(&now)),
                (Cell::Time(l), Value::NowMinus(d)) => Some(l.cmp(&(now - d.as_millis()))),
                (Cell::Int(l), Value::Duration(d)) => Some(l.cmp(&d.as_millis())),
                (Cell::Float(l), Value::Float(r)) => l.partial_cmp(r),
                (Cell::Int(l), Value::Float(r)) => cmp_i64_f64(*l, *r),
                (Cell::Float(l), Value::Int(r)) => cmp_i64_f64(*r, *l).map(|o| o.reverse()),
                (Cell::Float(l), Value::Duration(d)) => {
                    cmp_i64_f64(d.as_millis(), *l).map(|o| o.reverse())
                }
                (Cell::Text(l), Value::String(r)) => Some(l.as_ref().cmp(r.as_ref())),
                (Cell::Text(l), Value::Name(r)) => Some(l.as_ref().cmp(r.as_str())),
                (Cell::SmallText(l), Value::String(r)) => Some(l.as_str().cmp(r.as_ref())),
                (Cell::SmallText(l), Value::Name(r)) => Some(l.as_str().cmp(r.as_str())),
                _ => None,
            };
            let Some(ord) = ord else {
                return false;
            };
            match op {
                CmpOp::Gt => ord == std::cmp::Ordering::Greater,
                CmpOp::Lt => ord == std::cmp::Ordering::Less,
                CmpOp::Ge => ord != std::cmp::Ordering::Less,
                CmpOp::Le => ord != std::cmp::Ordering::Greater,
                _ => false,
            }
        }
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
    let blob_l = if blob.is_ascii() && !blob.bytes().any(|b| b.is_ascii_uppercase()) {
        std::borrow::Cow::Borrowed(blob.as_str())
    } else {
        // Keep str lowercase semantics, including contextual Unicode casing.
        std::borrow::Cow::Owned(blob.to_lowercase())
    };
    let mut score = 0i64;
    for tok in &query.tokens {
        if let Some(prefix) = tok.strip_suffix('*') {
            if !prefix.is_empty() {
                if blob_l.contains(prefix) {
                    score += 1;
                    if blob_l
                        .split(|c: char| !(c.is_alphanumeric() || c == '_'))
                        .any(|word| word.starts_with(prefix))
                    {
                        score += 2;
                    }
                }
            }
        } else if blob_l.contains(tok) {
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
    let mut result = Vec::with_capacity(rows.len());
    let keys: Vec<String> = fields.iter().map(|f| f.as_str()).collect();
    for r in rows {
        let mut out = crate::store::acquire_row();
        for k in &keys {
            let cell = r.get(k).map(Cell::clone_for_borrow).unwrap_or(Cell::Null);
            out.insert(k.clone(), cell);
        }
        result.push(out);
    }
    result
}

fn sort_rows_top_k(rows: &mut Vec<Row>, field: &Field, desc: bool, top_k: Option<usize>) {
    let k = field.as_str();
    let n = rows.len();
    if n <= 1 {
        return;
    }
    if let Some(0) = top_k {
        rows.clear();
        return;
    }

    if n <= 32 {
        let cmp = |a: &Row, b: &Row| {
            let av = a.get(&k).unwrap_or(&Cell::Null);
            let bv = b.get(&k).unwrap_or(&Cell::Null);
            let ord = cmp_sort(av, bv);
            if desc { ord.reverse() } else { ord }
        };
        if let Some(limit) = top_k {
            if rows.len() > limit {
                rows.select_nth_unstable_by(limit - 1, cmp);
                rows.truncate(limit);
            }
        }
        rows.sort_by(cmp);
        return;
    }

    // Decorate: extract sort keys once in O(N) instead of O(N log N) BTree lookups
    let mut keys: Vec<(Cell, usize)> = rows
        .iter()
        .enumerate()
        .map(|(i, r)| {
            (
                r.get(&k).map(Cell::clone_for_borrow).unwrap_or(Cell::Null),
                i,
            )
        })
        .collect();

    let cmp_keys = |a: &(Cell, usize), b: &(Cell, usize)| {
        let ord = cmp_sort(&a.0, &b.0);
        let ord = if desc { ord.reverse() } else { ord };
        ord.then_with(|| a.1.cmp(&b.1))
    };

    let target_len = if let Some(limit) = top_k {
        if limit < keys.len() {
            keys.select_nth_unstable_by(limit - 1, cmp_keys);
            keys.truncate(limit);
        }
        keys.len()
    } else {
        keys.len()
    };

    keys.sort_by(cmp_keys);

    // Undecorate: permute rows into final order in O(N) without cloning
    let mut old_rows = std::mem::replace(rows, Vec::with_capacity(target_len));
    for (_, orig_idx) in keys {
        rows.push(std::mem::take(&mut old_rows[orig_idx]));
    }
}

fn cmp_sort(a: &Cell, b: &Cell) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    match (a, b) {
        (Cell::Null, Cell::Null) => Ordering::Equal,
        (Cell::Null, _) => Ordering::Less,
        (_, Cell::Null) => Ordering::Greater,
        (Cell::Text(x), Cell::Text(y)) => x.cmp(y),
        (Cell::SmallText(x), Cell::SmallText(y)) => x.cmp(y),
        (Cell::Text(x), Cell::SmallText(y)) => x.as_ref().cmp(y.as_str()),
        (Cell::SmallText(x), Cell::Text(y)) => x.as_str().cmp(y.as_ref()),
        (Cell::Bool(x), Cell::Bool(y)) => x.cmp(y),
        _ => match (a.as_f64(), b.as_f64()) {
            (Some(x), Some(y)) => match (x.is_nan(), y.is_nan()) {
                (true, true) => Ordering::Equal,
                (true, false) => Ordering::Greater,
                (false, true) => Ordering::Less,
                (false, false) => x.partial_cmp(&y).expect("ordered numeric values"),
            },
            (Some(_), None) => Ordering::Greater,
            (None, Some(_)) => Ordering::Less,
            _ => a.compact().cmp(&b.compact()),
        },
    }
}

fn get_frame_bounds(
    i: usize,
    n: usize,
    frame: Option<&crate::window::WindowFrame>,
    has_order: bool,
) -> (usize, usize) {
    match frame {
        Some(fr) => {
            let s = match fr.start {
                crate::window::FrameBound::UnboundedPreceding => 0,
                crate::window::FrameBound::Preceding(k) => i.saturating_sub(k),
                crate::window::FrameBound::CurrentRow => i,
                crate::window::FrameBound::Following(k) => (i + k).min(n.saturating_sub(1)),
                crate::window::FrameBound::UnboundedFollowing => n.saturating_sub(1),
            };
            let e = match fr.end {
                crate::window::FrameBound::UnboundedPreceding => 0,
                crate::window::FrameBound::Preceding(k) => i.saturating_sub(k),
                crate::window::FrameBound::CurrentRow => i,
                crate::window::FrameBound::Following(k) => (i + k).min(n.saturating_sub(1)),
                crate::window::FrameBound::UnboundedFollowing => n.saturating_sub(1),
            };
            (s, e)
        }
        None => {
            if has_order {
                (0, i)
            } else {
                (0, n.saturating_sub(1))
            }
        }
    }
}

pub(crate) fn eval_window(
    rows: Vec<Row>,
    func: &crate::window::WindowFunction,
    field: Option<&Field>,
    by: &[Field],
    order: &[crate::ast::WindowSort],
    frame: Option<&crate::window::WindowFrame>,
    as_field: &str,
) -> Vec<Row> {
    if rows.is_empty() {
        return rows;
    }

    let by_keys: Vec<String> = by.iter().map(|b| b.as_str()).collect();
    let order_keys: Vec<(String, bool)> =
        order.iter().map(|s| (s.field.as_str(), s.desc)).collect();
    let target_field: Option<String> = field.map(|f| f.as_str());
    let tf: &str = target_field.as_deref().unwrap_or_default();

    let mut partition_keys: Vec<Vec<Cell>> = Vec::new();
    let mut partitions: FxHashMap<Vec<Cell>, Vec<Row>> = FxHashMap::default();
    let mut key_buf: Vec<Cell> = Vec::with_capacity(by_keys.len());

    for row in rows {
        key_buf.clear();
        for bk in &by_keys {
            key_buf.push(
                row.get(bk.as_str())
                    .map(Cell::clone_for_borrow)
                    .unwrap_or(Cell::Null),
            );
        }
        if let Some(part) = partitions.get_mut(&key_buf) {
            part.push(row);
        } else {
            let key = key_buf.clone();
            partition_keys.push(key.clone());
            partitions.insert(key, vec![row]);
        }
    }

    let mut result_rows = Vec::new();

    for pkey in partition_keys {
        let mut part = partitions.remove(&pkey).unwrap_or_default();

        if !order_keys.is_empty() {
            part.sort_by(|a, b| {
                for (ok, desc) in &order_keys {
                    let av = a.get(ok.as_str()).unwrap_or(&Cell::Null);
                    let bv = b.get(ok.as_str()).unwrap_or(&Cell::Null);
                    let ord = cmp_sort(av, bv);
                    let ord = if *desc { ord.reverse() } else { ord };
                    if ord != std::cmp::Ordering::Equal {
                        return ord;
                    }
                }
                std::cmp::Ordering::Equal
            });
        }

        let n = part.len();
        let vals: Vec<Cell> = match func {
            crate::window::WindowFunction::RowNumber => {
                (0..n).map(|i| Cell::Int((i + 1) as i64)).collect()
            }
            crate::window::WindowFunction::Rank => {
                let mut ranks = Vec::with_capacity(n);
                let mut current_rank = 1i64;
                for i in 0..n {
                    if i > 0 && !order_keys.is_empty() {
                        let all_equal = order_keys.iter().all(|(ok, _)| {
                            let prev = part[i - 1].get(ok.as_str()).unwrap_or(&Cell::Null);
                            let curr = part[i].get(ok.as_str()).unwrap_or(&Cell::Null);
                            cmp_sort(prev, curr) == std::cmp::Ordering::Equal
                        });
                        if !all_equal {
                            current_rank = (i + 1) as i64;
                        }
                    }
                    ranks.push(Cell::Int(current_rank));
                }
                ranks
            }
            crate::window::WindowFunction::DenseRank => {
                let mut ranks = Vec::with_capacity(n);
                let mut current_rank = 1i64;
                for i in 0..n {
                    if i > 0 && !order_keys.is_empty() {
                        let all_equal = order_keys.iter().all(|(ok, _)| {
                            let prev = part[i - 1].get(ok.as_str()).unwrap_or(&Cell::Null);
                            let curr = part[i].get(ok.as_str()).unwrap_or(&Cell::Null);
                            cmp_sort(prev, curr) == std::cmp::Ordering::Equal
                        });
                        if !all_equal {
                            current_rank += 1;
                        }
                    }
                    ranks.push(Cell::Int(current_rank));
                }
                ranks
            }
            crate::window::WindowFunction::Lead { offset } => {
                let off = *offset;
                (0..n)
                    .map(|i| {
                        if i + off < n {
                            part[i + off].get(tf).cloned().unwrap_or(Cell::Null)
                        } else {
                            Cell::Null
                        }
                    })
                    .collect()
            }
            crate::window::WindowFunction::Lag { offset } => {
                let off = *offset;
                (0..n)
                    .map(|i| {
                        if i >= off {
                            part[i - off].get(tf).cloned().unwrap_or(Cell::Null)
                        } else {
                            Cell::Null
                        }
                    })
                    .collect()
            }
            crate::window::WindowFunction::FirstValue => {
                if let Some(fr) = frame {
                    (0..n)
                        .map(|i| {
                            let (s, e) = get_frame_bounds(i, n, Some(fr), !order_keys.is_empty());
                            if s <= e && s < n {
                                part[s].get(tf).cloned().unwrap_or(Cell::Null)
                            } else {
                                Cell::Null
                            }
                        })
                        .collect()
                } else {
                    let first_val = part
                        .first()
                        .and_then(|r| r.get(tf))
                        .cloned()
                        .unwrap_or(Cell::Null);
                    vec![first_val; n]
                }
            }
            crate::window::WindowFunction::LastValue => {
                if let Some(fr) = frame {
                    (0..n)
                        .map(|i| {
                            let (s, e) = get_frame_bounds(i, n, Some(fr), !order_keys.is_empty());
                            if s <= e && s < n {
                                let end_idx = e.min(n - 1);
                                part[end_idx].get(tf).cloned().unwrap_or(Cell::Null)
                            } else {
                                Cell::Null
                            }
                        })
                        .collect()
                } else {
                    let last_val = part
                        .last()
                        .and_then(|r| r.get(tf))
                        .cloned()
                        .unwrap_or(Cell::Null);
                    vec![last_val; n]
                }
            }
            crate::window::WindowFunction::Sum => {
                let mut has_float = false;
                for r in &part {
                    if let Some(c) = r.get(tf) {
                        if matches!(c, Cell::Float(_)) {
                            has_float = true;
                            break;
                        }
                    }
                }
                if has_float {
                    let mut prefix = Vec::with_capacity(n + 1);
                    prefix.push(0.0f64);
                    let mut acc = 0.0f64;
                    for r in &part {
                        if let Some(c) = r.get(tf)
                            && let Some(v) = c.as_f64()
                        {
                            acc += v;
                        }
                        prefix.push(acc);
                    }
                    let mut sums = Vec::with_capacity(n);
                    for i in 0..n {
                        let (s, e) = get_frame_bounds(i, n, frame, !order_keys.is_empty());
                        if s <= e && s < n {
                            let end_idx = e.min(n - 1);
                            let sum = prefix[end_idx + 1] - prefix[s];
                            sums.push(Cell::Float(sum));
                        } else {
                            sums.push(Cell::Float(0.0));
                        }
                    }
                    sums
                } else {
                    let mut prefix = Vec::with_capacity(n + 1);
                    prefix.push(0i64);
                    let mut acc = 0i64;
                    for r in &part {
                        if let Some(c) = r.get(tf)
                            && let Some(v) = c.as_int()
                        {
                            acc += v;
                        }
                        prefix.push(acc);
                    }
                    let mut sums = Vec::with_capacity(n);
                    for i in 0..n {
                        let (s, e) = get_frame_bounds(i, n, frame, !order_keys.is_empty());
                        if s <= e && s < n {
                            let end_idx = e.min(n - 1);
                            let sum = prefix[end_idx + 1] - prefix[s];
                            sums.push(Cell::Int(sum));
                        } else {
                            sums.push(Cell::Int(0));
                        }
                    }
                    sums
                }
            }
            crate::window::WindowFunction::Avg => {
                let mut prefix_sum = Vec::with_capacity(n + 1);
                let mut prefix_cnt = Vec::with_capacity(n + 1);
                prefix_sum.push(0.0f64);
                prefix_cnt.push(0usize);
                let mut sum_acc = 0.0f64;
                let mut cnt_acc = 0usize;
                for r in &part {
                    if let Some(c) = r.get(tf)
                        && let Some(v) = c.as_f64()
                    {
                        sum_acc += v;
                        cnt_acc += 1;
                    }
                    prefix_sum.push(sum_acc);
                    prefix_cnt.push(cnt_acc);
                }
                let mut avgs = Vec::with_capacity(n);
                for i in 0..n {
                    let (s, e) = get_frame_bounds(i, n, frame, !order_keys.is_empty());
                    if s <= e && s < n {
                        let end_idx = e.min(n - 1);
                        let sum = prefix_sum[end_idx + 1] - prefix_sum[s];
                        let count = prefix_cnt[end_idx + 1] - prefix_cnt[s];
                        let avg = if count == 0 { 0.0 } else { sum / count as f64 };
                        avgs.push(Cell::Float(avg));
                    } else {
                        avgs.push(Cell::Float(0.0));
                    }
                }
                avgs
            }
            crate::window::WindowFunction::Min => {
                let is_running = frame.is_none() && !order_keys.is_empty();
                let is_whole = frame.is_none() && order_keys.is_empty();
                if is_whole {
                    let mut min_cell: Option<Cell> = None;
                    for r in &part {
                        if let Some(c) = r.get(tf).filter(|c| !matches!(c, Cell::Null)) {
                            min_cell = Some(match min_cell {
                                None => c.clone_for_borrow(),
                                Some(prev) => {
                                    if cmp_sort(c, &prev).is_lt() {
                                        c.clone_for_borrow()
                                    } else {
                                        prev
                                    }
                                }
                            });
                        }
                    }
                    let cell = min_cell.unwrap_or(Cell::Null);
                    vec![cell; n]
                } else if is_running {
                    let mut mins = Vec::with_capacity(n);
                    let mut min_cell: Option<Cell> = None;
                    for r in &part {
                        if let Some(c) = r.get(tf).filter(|c| !matches!(c, Cell::Null)) {
                            min_cell = Some(match min_cell {
                                None => c.clone_for_borrow(),
                                Some(prev) => {
                                    if cmp_sort(c, &prev).is_lt() {
                                        c.clone_for_borrow()
                                    } else {
                                        prev
                                    }
                                }
                            });
                        }
                        mins.push(min_cell.clone().unwrap_or(Cell::Null));
                    }
                    mins
                } else {
                    let mut mins = Vec::with_capacity(n);
                    for i in 0..n {
                        let (s, e) = get_frame_bounds(i, n, frame, !order_keys.is_empty());
                        if s <= e && s < n {
                            let slice = &part[s..=e.min(n - 1)];
                            let mut min_cell: Option<Cell> = None;
                            for r in slice {
                                if let Some(c) = r.get(tf).filter(|c| !matches!(c, Cell::Null)) {
                                    min_cell = Some(match min_cell {
                                        None => c.clone_for_borrow(),
                                        Some(prev) => {
                                            if cmp_sort(c, &prev).is_lt() {
                                                c.clone_for_borrow()
                                            } else {
                                                prev
                                            }
                                        }
                                    });
                                }
                            }
                            mins.push(min_cell.unwrap_or(Cell::Null));
                        } else {
                            mins.push(Cell::Null);
                        }
                    }
                    mins
                }
            }
            crate::window::WindowFunction::Max => {
                let is_running = frame.is_none() && !order_keys.is_empty();
                let is_whole = frame.is_none() && order_keys.is_empty();
                if is_whole {
                    let mut max_cell: Option<Cell> = None;
                    for r in &part {
                        if let Some(c) = r.get(tf).filter(|c| !matches!(c, Cell::Null)) {
                            max_cell = Some(match max_cell {
                                None => c.clone_for_borrow(),
                                Some(prev) => {
                                    if cmp_sort(c, &prev).is_gt() {
                                        c.clone_for_borrow()
                                    } else {
                                        prev
                                    }
                                }
                            });
                        }
                    }
                    let cell = max_cell.unwrap_or(Cell::Null);
                    vec![cell; n]
                } else if is_running {
                    let mut maxs = Vec::with_capacity(n);
                    let mut max_cell: Option<Cell> = None;
                    for r in &part {
                        if let Some(c) = r.get(tf).filter(|c| !matches!(c, Cell::Null)) {
                            max_cell = Some(match max_cell {
                                None => c.clone_for_borrow(),
                                Some(prev) => {
                                    if cmp_sort(c, &prev).is_gt() {
                                        c.clone_for_borrow()
                                    } else {
                                        prev
                                    }
                                }
                            });
                        }
                        maxs.push(max_cell.clone().unwrap_or(Cell::Null));
                    }
                    maxs
                } else {
                    let mut maxs = Vec::with_capacity(n);
                    for i in 0..n {
                        let (s, e) = get_frame_bounds(i, n, frame, !order_keys.is_empty());
                        if s <= e && s < n {
                            let slice = &part[s..=e.min(n - 1)];
                            let mut max_cell: Option<Cell> = None;
                            for r in slice {
                                if let Some(c) = r.get(tf).filter(|c| !matches!(c, Cell::Null)) {
                                    max_cell = Some(match max_cell {
                                        None => c.clone_for_borrow(),
                                        Some(prev) => {
                                            if cmp_sort(c, &prev).is_gt() {
                                                c.clone_for_borrow()
                                            } else {
                                                prev
                                            }
                                        }
                                    });
                                }
                            }
                            maxs.push(max_cell.unwrap_or(Cell::Null));
                        } else {
                            maxs.push(Cell::Null);
                        }
                    }
                    maxs
                }
            }
            crate::window::WindowFunction::Count => {
                if target_field.is_some() {
                    let mut prefix_cnt = Vec::with_capacity(n + 1);
                    prefix_cnt.push(0usize);
                    let mut cnt_acc = 0usize;
                    for r in &part {
                        if r.get(tf).is_some_and(|c| !matches!(c, Cell::Null)) {
                            cnt_acc += 1;
                        }
                        prefix_cnt.push(cnt_acc);
                    }
                    let mut counts = Vec::with_capacity(n);
                    for i in 0..n {
                        let (s, e) = get_frame_bounds(i, n, frame, !order_keys.is_empty());
                        if s <= e && s < n {
                            let end_idx = e.min(n - 1);
                            let count = prefix_cnt[end_idx + 1] - prefix_cnt[s];
                            counts.push(Cell::Int(count as i64));
                        } else {
                            counts.push(Cell::Int(0));
                        }
                    }
                    counts
                } else {
                    let mut counts = Vec::with_capacity(n);
                    for i in 0..n {
                        let (s, e) = get_frame_bounds(i, n, frame, !order_keys.is_empty());
                        let count = if s <= e && s < n {
                            e.min(n - 1) - s + 1
                        } else {
                            0
                        };
                        counts.push(Cell::Int(count as i64));
                    }
                    counts
                }
            }
        };

        for (i, mut row) in part.into_iter().enumerate() {
            row.insert(as_field.to_string(), vals[i].clone());
            result_rows.push(row);
        }
    }

    result_rows
}

fn agg_count(rows: &[Row], by: &Field, max_bytes: usize) -> Result<Vec<Row>, Error> {
    let k = by.as_str();
    let mut map: FxHashMap<Cell, i64> = FxHashMap::default();
    let mut state_bytes = 0usize;
    for r in rows {
        let cell = r.get(&k).map(Cell::clone_for_borrow).unwrap_or(Cell::Null);
        if !map.contains_key(&cell) {
            let key_size = match &cell {
                Cell::Text(s) => s.len() + 32,
                _ => 16,
            };
            state_bytes = state_bytes.saturating_add(key_size + 64);
            if state_bytes > max_bytes {
                return Err(Error::runtime(format!(
                    "aggregate state bytes {state_bytes} exceeds max_aggregate_bytes quota {max_bytes}"
                )));
            }
        }
        *map.entry(cell).or_insert(0) += 1;
    }
    let mut entries: Vec<(Cell, i64)> = map.into_iter().collect();
    entries.sort_unstable_by(|(a, _), (b, _)| cmp_sort(a, b));
    Ok(entries
        .into_iter()
        .map(|(cell, n)| {
            let mut row = crate::store::acquire_row();
            row.insert(k.to_string(), cell);
            row.insert("hits".into(), Cell::Int(n));
            row
        })
        .collect())
}

fn count_cell_map_to_rows(by: &str, map: FxHashMap<Cell, i64>) -> Vec<Row> {
    let mut entries: Vec<(Cell, i64)> = map.into_iter().collect();
    entries.sort_unstable_by(|(a, _), (b, _)| cmp_sort(a, b));
    entries
        .into_iter()
        .map(|(cell, n)| {
            let mut row = crate::store::acquire_row();
            row.insert(by.to_string(), cell);
            row.insert("hits".into(), Cell::Int(n));
            row
        })
        .collect()
}

fn count_arc_map_to_rows(by: &str, map: BTreeMap<std::sync::Arc<str>, i64>) -> Vec<Row> {
    map.into_iter()
        .map(|(g, n)| {
            let mut row = crate::store::acquire_row();
            row.insert(by.to_string(), Cell::Text(g));
            row.insert("hits".into(), Cell::Int(n));
            row
        })
        .collect()
}

fn hits_row(n: i64) -> Row {
    let mut row = crate::store::acquire_row();
    row.insert("hits".into(), Cell::Int(n));
    row
}

fn count_row(by: &str, value: &str, n: i64) -> Row {
    let mut row = crate::store::acquire_row();
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
        } if f.as_str() == field => Some(s.as_ref()),
        Pred::Cmp {
            field: f,
            op: CmpOp::Eq,
            value: Value::Name(s),
        } if f.as_str() == field => Some(s.as_str()),
        Pred::And(a, b) => eq_value_for_field(a, field).or_else(|| eq_value_for_field(b, field)),
        _ => None,
    }
}

fn agg_sum(
    rows: &[Row],
    field: &Field,
    by: Option<&Field>,
    max_bytes: usize,
) -> Result<Vec<Row>, Error> {
    let fk = field.as_str();
    let Some(by) = by else {
        let total: f64 = rows
            .iter()
            .map(|r| r.get(&fk).and_then(Cell::as_f64).unwrap_or(0.0))
            .sum();
        let mut row = crate::store::acquire_row();
        row.insert(fk, Cell::Float(total));
        return Ok(vec![row]);
    };
    let bk = by.as_str();
    let mut map: FxHashMap<Cell, f64> = FxHashMap::default();
    let mut state_bytes = 0usize;
    for r in rows {
        let cell = r.get(&bk).map(Cell::clone_for_borrow).unwrap_or(Cell::Null);
        if !map.contains_key(&cell) {
            let key_size = match &cell {
                Cell::Text(s) => s.len() + 32,
                _ => 16,
            };
            state_bytes = state_bytes.saturating_add(key_size + 64);
            if state_bytes > max_bytes {
                return Err(Error::runtime(format!(
                    "aggregate state bytes {state_bytes} exceeds max_aggregate_bytes quota {max_bytes}"
                )));
            }
        }
        let n = r.get(&fk).and_then(Cell::as_f64).unwrap_or(0.0);
        *map.entry(cell).or_insert(0.0) += n;
    }
    let mut entries: Vec<(Cell, f64)> = map.into_iter().collect();
    entries.sort_unstable_by(|(a, _), (b, _)| cmp_sort(a, b));
    Ok(entries
        .into_iter()
        .map(|(cell, n)| {
            let mut row = crate::store::acquire_row();
            row.insert(bk.clone(), cell);
            row.insert(fk.clone(), Cell::Float(n));
            row
        })
        .collect())
}

fn agg_min(
    rows: &[Row],
    field: &Field,
    by: Option<&Field>,
    max_bytes: usize,
) -> Result<Vec<Row>, Error> {
    let fk = field.as_str();
    let Some(by) = by else {
        let mut best: Option<Cell> = None;
        for r in rows {
            if let Some(c) = r.get(&fk).filter(|c| !matches!(c, Cell::Null)) {
                best = Some(match best {
                    None => c.clone_for_borrow(),
                    Some(prev) => {
                        if cmp_sort(c, &prev).is_lt() {
                            c.clone_for_borrow()
                        } else {
                            prev
                        }
                    }
                });
            }
        }
        let mut row = crate::store::acquire_row();
        row.insert(fk, best.unwrap_or(Cell::Null));
        return Ok(vec![row]);
    };
    let bk = by.as_str();
    let mut map: FxHashMap<Cell, Option<Cell>> = FxHashMap::default();
    let mut state_bytes = 0usize;
    for r in rows {
        let cell = r.get(&bk).map(Cell::clone_for_borrow).unwrap_or(Cell::Null);
        if !map.contains_key(&cell) {
            let key_size = match &cell {
                Cell::Text(s) => s.len() + 32,
                _ => 16,
            };
            state_bytes = state_bytes.saturating_add(key_size + 64);
            if state_bytes > max_bytes {
                return Err(Error::runtime(format!(
                    "aggregate state bytes {state_bytes} exceeds max_aggregate_bytes quota {max_bytes}"
                )));
            }
        }
        let f_val = r.get(&fk).filter(|c| !matches!(c, Cell::Null));
        let entry = map.entry(cell).or_insert(None);
        if let Some(c) = f_val {
            *entry = Some(match entry.take() {
                None => c.clone_for_borrow(),
                Some(prev) => {
                    if cmp_sort(c, &prev).is_lt() {
                        c.clone_for_borrow()
                    } else {
                        prev
                    }
                }
            });
        }
    }
    let mut entries: Vec<(Cell, Option<Cell>)> = map.into_iter().collect();
    entries.sort_unstable_by(|(a, _), (b, _)| cmp_sort(a, b));
    Ok(entries
        .into_iter()
        .map(|(gcell, val)| {
            let mut row = crate::store::acquire_row();
            row.insert(bk.clone(), gcell);
            row.insert(fk.clone(), val.unwrap_or(Cell::Null));
            row
        })
        .collect())
}

fn agg_max(
    rows: &[Row],
    field: &Field,
    by: Option<&Field>,
    max_bytes: usize,
) -> Result<Vec<Row>, Error> {
    let fk = field.as_str();
    let Some(by) = by else {
        let mut best: Option<Cell> = None;
        for r in rows {
            if let Some(c) = r.get(&fk).filter(|c| !matches!(c, Cell::Null)) {
                best = Some(match best {
                    None => c.clone_for_borrow(),
                    Some(prev) => {
                        if cmp_sort(c, &prev).is_gt() {
                            c.clone_for_borrow()
                        } else {
                            prev
                        }
                    }
                });
            }
        }
        let mut row = crate::store::acquire_row();
        row.insert(fk, best.unwrap_or(Cell::Null));
        return Ok(vec![row]);
    };
    let bk = by.as_str();
    let mut map: FxHashMap<Cell, Option<Cell>> = FxHashMap::default();
    let mut state_bytes = 0usize;
    for r in rows {
        let cell = r.get(&bk).map(Cell::clone_for_borrow).unwrap_or(Cell::Null);
        if !map.contains_key(&cell) {
            let key_size = match &cell {
                Cell::Text(s) => s.len() + 32,
                _ => 16,
            };
            state_bytes = state_bytes.saturating_add(key_size + 64);
            if state_bytes > max_bytes {
                return Err(Error::runtime(format!(
                    "aggregate state bytes {state_bytes} exceeds max_aggregate_bytes quota {max_bytes}"
                )));
            }
        }
        let f_val = r.get(&fk).filter(|c| !matches!(c, Cell::Null));
        let entry = map.entry(cell).or_insert(None);
        if let Some(c) = f_val {
            *entry = Some(match entry.take() {
                None => c.clone_for_borrow(),
                Some(prev) => {
                    if cmp_sort(c, &prev).is_gt() {
                        c.clone_for_borrow()
                    } else {
                        prev
                    }
                }
            });
        }
    }
    let mut entries: Vec<(Cell, Option<Cell>)> = map.into_iter().collect();
    entries.sort_unstable_by(|(a, _), (b, _)| cmp_sort(a, b));
    Ok(entries
        .into_iter()
        .map(|(gcell, val)| {
            let mut row = crate::store::acquire_row();
            row.insert(bk.clone(), gcell);
            row.insert(fk.clone(), val.unwrap_or(Cell::Null));
            row
        })
        .collect())
}

fn agg_avg(
    rows: &[Row],
    field: &Field,
    by: Option<&Field>,
    max_bytes: usize,
) -> Result<Vec<Row>, Error> {
    let fk = field.as_str();
    let Some(by) = by else {
        let mut sum = 0.0f64;
        let mut count = 0usize;
        for r in rows {
            if let Some(v) = r.get(&fk).and_then(Cell::as_f64) {
                sum += v;
                count += 1;
            }
        }
        let avg = if count == 0 {
            0.0
        } else {
            sum / (count as f64)
        };
        let mut row = crate::store::acquire_row();
        row.insert(fk, Cell::Float(avg));
        return Ok(vec![row]);
    };
    let bk = by.as_str();
    let mut map: FxHashMap<Cell, (f64, usize)> = FxHashMap::default();
    let mut state_bytes = 0usize;
    for r in rows {
        let cell = r.get(&bk).map(Cell::clone_for_borrow).unwrap_or(Cell::Null);
        if !map.contains_key(&cell) {
            let key_size = match &cell {
                Cell::Text(s) => s.len() + 32,
                _ => 16,
            };
            state_bytes = state_bytes.saturating_add(key_size + 64);
            if state_bytes > max_bytes {
                return Err(Error::runtime(format!(
                    "aggregate state bytes {state_bytes} exceeds max_aggregate_bytes quota {max_bytes}"
                )));
            }
        }
        let v_opt = r.get(&fk).and_then(Cell::as_f64);
        let entry = map.entry(cell).or_insert((0.0, 0));
        if let Some(v) = v_opt {
            entry.0 += v;
            entry.1 += 1;
        }
    }
    let mut entries: Vec<(Cell, (f64, usize))> = map.into_iter().collect();
    entries.sort_unstable_by(|(a, _), (b, _)| cmp_sort(a, b));
    Ok(entries
        .into_iter()
        .map(|(gcell, (sum, count))| {
            let avg = if count == 0 {
                0.0
            } else {
                sum / (count as f64)
            };
            let mut row = crate::store::acquire_row();
            row.insert(bk.clone(), gcell);
            row.insert(fk.clone(), Cell::Float(avg));
            row
        })
        .collect())
}

pub(crate) fn cell_key(c: &Cell) -> Option<String> {
    match c {
        Cell::Text(s) => Some(s.as_ref().to_owned()),
        Cell::SmallText(s) => Some(s.as_str().to_owned()),
        Cell::Int(n) => Some(n.to_string()),
        _ => None,
    }
}

impl Db {
    fn uses_gpu_filter(&self, query: &Query) -> bool {
        #[cfg(feature = "gpu")]
        {
            self.gpu.is_some()
                && query
                    .steps
                    .iter()
                    .any(|step| matches!(step, Step::Filter(p) if gpu_supported_pred(p)))
        }
        #[cfg(not(feature = "gpu"))]
        {
            let _ = query;
            false
        }
    }
    #[cfg(feature = "gpu")]
    pub(crate) fn gpu_filter_mask(
        &self,
        pred: &Pred,
        rows: &[Row],
        now: i64,
    ) -> Result<Option<Vec<bool>>, Error> {
        let Some(gpu) = &self.gpu else {
            return Ok(None);
        };
        if !gpu_supported_pred(pred) {
            return Ok(None);
        }
        match pred {
            Pred::And(a, b) | Pred::Or(a, b) => {
                let a = self
                    .gpu_filter_mask(a, rows, now)?
                    .unwrap_or_else(|| rows.iter().map(|row| eval_pred(a, row, now)).collect());
                let b = self
                    .gpu_filter_mask(b, rows, now)?
                    .unwrap_or_else(|| rows.iter().map(|row| eval_pred(b, row, now)).collect());
                Ok(Some(gpu.combine_masks(
                    &a,
                    &b,
                    matches!(pred, Pred::Or(..)),
                )?))
            }
            Pred::Not(_) | Pred::IsNull { .. } | Pred::In { .. } => Ok(None),
            Pred::Cmp { field, op, value } => {
                let right = value_cell(value, now);
                if let Some(needle) = right.text() {
                    if !matches!(op, CmpOp::Eq | CmpOp::Ne) {
                        return Ok(Some(vec![false; rows.len()]));
                    }
                    let texts: Vec<_> = rows.iter().map(|r| field_cell(r, field).text()).collect();
                    return Ok(Some(gpu.text_mask(
                        &texts,
                        needle,
                        if *op == CmpOp::Ne { 2 } else { 1 },
                        false,
                    )?));
                }
                let records: Vec<_> = rows
                    .iter()
                    .map(|r| gpu_comparison_record(field_cell(r, field), &right, *op))
                    .collect();
                Ok(Some(gpu.comparison_mask(&records, *op)?))
            }
            Pred::Contains { field, needle } => {
                let texts: Vec<_> = rows.iter().map(|r| field_cell(r, field).text()).collect();
                Ok(Some(gpu.text_mask(&texts, needle, 0, false)?))
            }
            Pred::Has { field, needle, ci } => {
                let texts: Vec<_> = rows.iter().map(|r| field_cell(r, field).text()).collect();
                Ok(Some(gpu.text_mask(&texts, needle, 3, *ci)?))
            }
            Pred::Regex {
                field,
                pattern,
                flags,
            } => {
                let texts: Vec<_> = rows
                    .iter()
                    .map(|row| field_cell(row, field).text())
                    .collect();
                Ok(Some(gpu.regex_mask(&texts, pattern, flags)?))
            }
        }
    }
}
#[cfg(feature = "gpu")]
fn gpu_supported_pred(pred: &Pred) -> bool {
    match pred {
        Pred::And(a, b) | Pred::Or(a, b) => gpu_supported_pred(a) || gpu_supported_pred(b),
        Pred::Not(_) | Pred::IsNull { .. } | Pred::In { .. } => false,
        Pred::Cmp { .. } | Pred::Has { .. } | Pred::Contains { .. } => true,
        Pred::Regex { .. } => true,
    }
}

#[cfg(feature = "gpu")]
fn gpu_comparison_record(left: &Cell, right: &Cell, _op: CmpOp) -> [u32; 5] {
    let integer_key = |v: i64| (v as u64) ^ (1u64 << 63);
    let float_key = |v: f64| {
        let bits = if v == 0.0 { 0 } else { v.to_bits() };
        if bits >> 63 != 0 {
            !bits
        } else {
            bits ^ (1u64 << 63)
        }
    };
    let keys = match (left, right) {
        (Cell::Bool(a), Cell::Bool(b)) => Some((*a as u64, *b as u64)),
        (Cell::Int(a), Cell::Int(b)) | (Cell::Time(a), Cell::Time(b)) => {
            Some((integer_key(*a), integer_key(*b)))
        }
        (Cell::Float(_), Cell::Float(_))
        | (Cell::Float(_), Cell::Int(_))
        | (Cell::Int(_), Cell::Float(_)) => left
            .as_f64()
            .zip(right.as_f64())
            .filter(|(a, b)| !a.is_nan() && !b.is_nan())
            .map(|(a, b)| (float_key(a), float_key(b))),
        _ => None,
    };
    match keys {
        Some((a, b)) => [a as u32, (a >> 32) as u32, b as u32, (b >> 32) as u32, 1],
        None => [0; 5],
    }
}

impl Db {
    fn uses_gpu_count(&self, query: &Query) -> bool {
        #[cfg(feature = "gpu")]
        {
            self.gpu.is_some()
                && query
                    .steps
                    .iter()
                    .any(|step| matches!(step, Step::Count { .. } | Step::Sum { .. }))
        }
        #[cfg(not(feature = "gpu"))]
        {
            let _ = query;
            false
        }
    }
    #[cfg(feature = "gpu")]
    fn gpu_count_rows(&self, rows: &[Row], by: Option<&Field>) -> Result<Option<Vec<Row>>, Error> {
        let Some(gpu) = &self.gpu else {
            return Ok(None);
        };
        let Some(by) = by else {
            let counts = gpu.group_counts(&vec![0u32; rows.len()], 1)?;
            return Ok(Some(vec![hits_row(counts[0])]));
        };
        let field = by.as_str();
        let mut group_ids: FxHashMap<Cell, u32> = FxHashMap::default();
        let mut group_cells: Vec<Cell> = Vec::new();
        let mut ids: Vec<u32> = Vec::with_capacity(rows.len());
        for row in rows {
            let cell = row
                .get(&field)
                .map(Cell::clone_for_borrow)
                .unwrap_or(Cell::Null);
            let id = if let Some(&id) = group_ids.get(&cell) {
                id
            } else {
                let id = u32::try_from(group_cells.len())
                    .map_err(|_| Error::runtime("GPU count has too many groups"))?;
                group_cells.push(cell.clone());
                group_ids.insert(cell, id);
                id
            };
            ids.push(id);
        }
        let counts = gpu.group_counts(&ids, group_cells.len())?;
        let mut entries: Vec<(Cell, i64)> = group_cells.into_iter().zip(counts).collect();
        entries.sort_unstable_by(|(a, _), (b, _)| cmp_sort(a, b));
        Ok(Some(
            entries
                .into_iter()
                .map(|(cell, count)| {
                    let mut row = crate::store::acquire_row();
                    row.insert(field.clone(), cell);
                    row.insert("hits".into(), Cell::Int(count));
                    row
                })
                .collect(),
        ))
    }
}

impl Db {
    fn uses_gpu_join(&self, query: &Query) -> bool {
        #[cfg(feature = "gpu")]
        {
            self.gpu.is_some()
                && query
                    .steps
                    .iter()
                    .any(|step| matches!(step, Step::Join { .. }))
        }
        #[cfg(not(feature = "gpu"))]
        {
            let _ = query;
            false
        }
    }
    #[cfg(feature = "gpu")]
    pub(crate) fn gpu_join_rows(
        &self,
        left_col: &str,
        left: Vec<Row>,
        right_col: &str,
        on: &str,
        left_join: bool,
    ) -> Result<Vec<Row>, Error> {
        let gpu = self.gpu.as_ref().expect("GPU join selected");
        let to_field = self
            .catalog
            .find_fk(left_col, on, right_col)
            .map(|fk| fk.to_field.as_str())
            .unwrap_or("id");
        let right = self.store.collection(right_col);
        let mut dictionary: FxHashMap<String, u32> = FxHashMap::default();
        let key = |row: &Row, field: &str| {
            if to_field == "id" {
                row.get(field).and_then(Cell::text).map(str::to_owned)
            } else {
                row.get(field).and_then(cell_key)
            }
        };
        let mut encode = |value: Option<String>| -> Result<u32, Error> {
            let Some(value) = value else {
                return Ok(0);
            };
            if let Some(&id) = dictionary.get(&value) {
                return Ok(id);
            }
            let id = u32::try_from(dictionary.len() + 1)
                .map_err(|_| Error::runtime("GPU join has too many distinct keys"))?;
            dictionary.insert(value, id);
            Ok(id)
        };
        let left_keys: Vec<u32> = left
            .iter()
            .map(|row| encode(key(row, on)))
            .collect::<Result<_, _>>()?;
        let right_keys: Vec<u32> = right
            .iter()
            .map(|row| encode(key(row, to_field)))
            .collect::<Result<_, _>>()?;
        if to_field == "id" {
            let hits = gpu.join_id_matches(&left_keys, &right_keys)?;
            let mut out = Vec::new();
            for (row, hit) in left.into_iter().zip(hits) {
                if let Some(hit) = hit {
                    out.push(merge_join_row(row, &right[hit], right_col));
                } else if left_join {
                    out.push(row);
                }
            }
            return Ok(out);
        }
        let hits = gpu.join_matches(&left_keys, &right_keys)?;
        let mut out = Vec::new();
        for (row, hits) in left.into_iter().zip(hits) {
            if hits.is_empty() {
                if left_join {
                    out.push(row);
                }
            } else {
                for &hit in &hits {
                    out.push(merge_join_row(row.clone(), &right[hit], right_col));
                }
            }
        }
        Ok(out)
    }
}

fn rows_to_query_batch(rows: &[Row], query: &Query) -> RecordBatch {
    let mut projection = None;
    for step in &query.steps {
        match step {
            Step::Project(fields) => projection = Some(field_names(fields)),
            Step::Join { .. }
            | Step::Hop { .. }
            | Step::Graph { .. }
            | Step::Match { .. }
            | Step::Count { .. }
            | Step::Sum { .. }
            | Step::Min { .. }
            | Step::Max { .. }
            | Step::Avg { .. }
            | Step::Union(_) => projection = None,
            _ => {}
        }
    }
    let Some(names) = projection else {
        return rows_to_rough_batch(rows);
    };
    let mut batch = RecordBatch::with_capacity(Arc::<[String]>::from(names.clone()), rows.len());
    for row in rows {
        push_project_batch_row(&mut batch, row, &names);
    }
    batch
}

impl Db {
    fn uses_gpu_sort(&self, query: &Query) -> bool {
        #[cfg(feature = "gpu")]
        {
            self.gpu.is_some()
                && query
                    .steps
                    .iter()
                    .any(|step| matches!(step, Step::Sort { .. }))
        }
        #[cfg(not(feature = "gpu"))]
        {
            let _ = query;
            false
        }
    }
}

#[cfg(feature = "gpu")]
impl Db {
    fn gpu_sum_rows(
        &self,
        rows: &[Row],
        field: &Field,
        by: Option<&Field>,
    ) -> Result<Option<Vec<Row>>, Error> {
        let Some(gpu) = &self.gpu else {
            return Ok(None);
        };
        let fk = field.as_str();
        let Some(by) = by else {
            let values: Vec<f64> = rows
                .iter()
                .map(|row| row.get(&fk).and_then(Cell::as_f64).unwrap_or(0.0))
                .collect();
            let sums = gpu.group_sums(&vec![0u32; rows.len()], &values, 1)?;
            let mut row = Row::new();
            row.insert(fk, Cell::Float(sums[0]));
            return Ok(Some(vec![row]));
        };
        let bk = by.as_str();
        let mut group_ids: FxHashMap<Cell, u32> = FxHashMap::default();
        let mut group_cells: Vec<Cell> = Vec::new();
        let mut ids: Vec<u32> = Vec::with_capacity(rows.len());
        for row in rows {
            let cell = row
                .get(&bk)
                .map(Cell::clone_for_borrow)
                .unwrap_or(Cell::Null);
            let id = if let Some(&id) = group_ids.get(&cell) {
                id
            } else {
                let id = u32::try_from(group_cells.len())
                    .map_err(|_| Error::runtime("GPU sum has too many groups"))?;
                group_cells.push(cell.clone());
                group_ids.insert(cell, id);
                id
            };
            ids.push(id);
        }
        let values: Vec<f64> = rows
            .iter()
            .map(|row| row.get(&fk).and_then(Cell::as_f64).unwrap_or(0.0))
            .collect();
        let sums = gpu.group_sums(&ids, &values, group_cells.len())?;
        let mut entries: Vec<(Cell, f64)> = group_cells.into_iter().zip(sums).collect();
        entries.sort_unstable_by(|(a, _), (b, _)| cmp_sort(a, b));
        Ok(Some(
            entries
                .into_iter()
                .map(|(cell, sum)| {
                    let mut row = crate::store::acquire_row();
                    row.insert(bk.clone(), cell);
                    row.insert(fk.clone(), Cell::Float(sum));
                    row
                })
                .collect(),
        ))
    }
}

impl Db {
    fn uses_gpu_graph(&self, query: &Query) -> bool {
        #[cfg(feature = "gpu")]
        {
            self.gpu.is_some()
                && query.steps.iter().any(|step| {
                    matches!(
                        step,
                        Step::Hop { .. } | Step::Graph { .. } | Step::Match { .. }
                    )
                })
        }
        #[cfg(not(feature = "gpu"))]
        {
            let _ = query;
            false
        }
    }
    pub(crate) fn graph_frontier_mask(
        &self,
        relation: &str,
        reverse: bool,
        frontier: &BTreeSet<String>,
    ) -> Result<Option<Vec<bool>>, Error> {
        #[cfg(feature = "gpu")]
        {
            let Some(gpu) = &self.gpu else {
                return Ok(None);
            };
            let mut dictionary: FxHashMap<&str, u32> = FxHashMap::default();
            let mut keys: BTreeSet<&str> = frontier.iter().map(String::as_str).collect();
            for edge in &self.store.edges {
                if edge.rel == relation {
                    keys.insert(if reverse {
                        edge.to.as_str()
                    } else {
                        edge.from.as_str()
                    });
                }
            }
            for key in keys {
                let id = u32::try_from(dictionary.len() + 1)
                    .map_err(|_| Error::runtime("GPU graph has too many keys"))?;
                dictionary.insert(key, id);
            }
            let ids: Vec<u32> = frontier
                .iter()
                .map(|key| dictionary[key.as_str()])
                .collect();
            let sources: Vec<u32> = self
                .store
                .edges
                .iter()
                .map(|edge| {
                    if edge.rel != relation {
                        return 0;
                    }
                    let key = if reverse {
                        edge.to.as_str()
                    } else {
                        edge.from.as_str()
                    };
                    dictionary.get(key).copied().unwrap_or(0)
                })
                .collect();
            Ok(Some(gpu.graph_edge_mask(&sources, &ids)?))
        }
        #[cfg(not(feature = "gpu"))]
        {
            let _ = (relation, reverse, frontier);
            Ok(None)
        }
    }
}

impl Db {
    fn gpu_lex_scores<'a>(
        &self,
        rows: impl Iterator<Item = &'a Row>,
        query: &LexQuery,
    ) -> Result<Option<Vec<i64>>, Error> {
        #[cfg(feature = "gpu")]
        {
            let Some(gpu) = &self.gpu else {
                return Ok(None);
            };
            let blobs: Vec<String> = rows
                .map(|row| {
                    let mut blob = String::new();
                    for field in ["title", "body", "snippet"] {
                        if let Some(text) = row_text(row, field) {
                            blob.push_str(text);
                            blob.push(' ');
                        }
                    }
                    blob.to_lowercase()
                })
                .collect();
            let texts: Vec<Option<&str>> = blobs.iter().map(|blob| Some(blob.as_str())).collect();
            if query.tokens.is_empty() && query.lower.is_empty() {
                return Ok(Some(vec![0i64; blobs.len()]));
            }
            let mut scores = gpu.weight_accumulator(blobs.len())?;
            for token in &query.tokens {
                gpu.text_accumulate(&texts, token, 0, &mut scores, 1)?;
                // Punctuation-bearing tokens cannot equal a CPU-split word.
                if token.chars().all(|c| c.is_alphanumeric() || c == '_') {
                    gpu.text_accumulate(&texts, token, 3, &mut scores, 2)?;
                }
            }
            if !query.lower.is_empty() {
                gpu.text_accumulate(&texts, &query.lower, 0, &mut scores, 3)?;
            }
            Ok(Some(scores.finish()?))
        }
        #[cfg(not(feature = "gpu"))]
        {
            let _ = (rows, query);
            Ok(None)
        }
    }
}

#[cfg(feature = "gpu")]
fn gpu_rrf_keys<K: Ord + Clone>(
    gpu: &crate::gpu::GpuCompute,
    lex: &[K],
    vectors: &[K],
) -> Result<Vec<K>, Error> {
    let keys: Vec<K> = lex
        .iter()
        .chain(vectors)
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    if keys.len() > u32::MAX as usize {
        return Err(Error::runtime("GPU RRF has too many identities"));
    }
    let mut ids = Vec::with_capacity(lex.len() + vectors.len());
    let mut denominators = Vec::with_capacity(ids.capacity());
    for list in [lex, vectors] {
        for (rank, key) in list.iter().enumerate() {
            let denominator = u32::try_from(rank)
                .ok()
                .and_then(|rank| rank.checked_add(61))
                .ok_or_else(|| Error::runtime("GPU RRF rank exceeds integer range"))?;
            ids.push(keys.binary_search(key).expect("known identity") as u32);
            denominators.push(denominator);
        }
    }
    let scores = gpu.group_rrf(&ids, &denominators, keys.len())?;
    let cells: Vec<Cell> = scores.into_iter().map(Cell::Float).collect();
    let references: Vec<&Cell> = cells.iter().collect();
    Ok(gpu
        .sort_indices(&references, true)?
        .into_iter()
        .map(|i| keys[i].clone())
        .collect())
}

#[cfg(feature = "gpu")]
impl Db {
    fn gpu_walk_rows(
        &self,
        primary: &str,
        rows: &[Row],
        relation: &str,
        reverse: bool,
        depth: i64,
        graph: bool,
    ) -> Result<Vec<Row>, Error> {
        let mut seeds = BTreeSet::new();
        for row in rows {
            for field in ["id", "uri"] {
                if let Some(key) = row_text(row, field) {
                    seeds.insert(key.to_owned());
                }
            }
        }
        let (keys, reached) = self.gpu_walk_state(&seeds, relation, reverse, depth, graph)?;
        if graph {
            let edges: Vec<_> = self
                .store
                .edges
                .iter()
                .filter(|edge| edge.rel == relation)
                .collect();
            return Ok(reached
                .into_iter()
                .map(|i| {
                    let edge = edges[i];
                    edge_row(&edge.rel, &edge.from, &edge.to)
                })
                .collect());
        }
        Ok(reached
            .into_iter()
            .filter_map(|i| self.resolve_node(primary, &keys[i]))
            .take(300)
            .collect())
    }
    pub(crate) fn gpu_hop_keys(
        &self,
        seeds: &BTreeSet<String>,
        relation: &str,
        reverse: bool,
    ) -> Result<Vec<String>, Error> {
        let (keys, reached) = self.gpu_walk_state(seeds, relation, reverse, 1, false)?;
        Ok(reached.into_iter().map(|i| keys[i].clone()).collect())
    }
    fn gpu_walk_state(
        &self,
        seeds: &BTreeSet<String>,
        relation: &str,
        reverse: bool,
        depth: i64,
        graph: bool,
    ) -> Result<(Vec<String>, Vec<usize>), Error> {
        let edges: Vec<_> = self
            .store
            .edges
            .iter()
            .filter(|edge| edge.rel == relation)
            .collect();
        let mut keys = seeds.clone();
        for edge in &edges {
            keys.insert(edge.from.clone());
            keys.insert(edge.to.clone());
        }
        let keys: Vec<String> = keys.into_iter().collect();
        let id = |key: &str| {
            keys.binary_search_by(|v| v.as_str().cmp(key))
                .expect("known node") as u32
        };
        let mut identities = BTreeMap::new();
        let encoded: Vec<[u32; 4]> = edges
            .iter()
            .map(|edge| {
                let next = identities.len() as u32;
                let identity = *identities
                    .entry((edge.from.as_str(), edge.to.as_str()))
                    .or_insert(next);
                let (src, dst) = if reverse {
                    (&edge.to, &edge.from)
                } else {
                    (&edge.from, &edge.to)
                };
                [id(src), id(dst), identity, 0]
            })
            .collect();
        let seed_ids: Vec<u32> = seeds.iter().map(|key| id(key)).collect();
        let reached = self.gpu.as_ref().unwrap().walk_graph(
            &encoded,
            &seed_ids,
            keys.len(),
            identities.len(),
            depth.clamp(1, 3) as u32,
            graph,
        )?;
        Ok((keys, reached))
    }
}

#[cfg(feature = "gpu")]
impl Db {
    fn gpu_match_expand(
        &self,
        primary: &str,
        row: &Row,
        hop: &MatchHop,
        seeds: &[String],
        relation: &str,
        reverse: bool,
        out: &mut Vec<Row>,
    ) -> Result<(), Error> {
        let edges: Vec<_> = self
            .store
            .edges
            .iter()
            .filter(|e| e.rel == relation)
            .collect();
        let mut keys: BTreeSet<String> = seeds.iter().cloned().collect();
        for edge in &edges {
            keys.insert(edge.from.clone());
            keys.insert(edge.to.clone());
        }
        let keys: Vec<String> = keys.into_iter().collect();
        let id = |key: &str| {
            keys.binary_search_by(|s| s.as_str().cmp(key))
                .expect("known key") as u32
        };
        let encoded: Vec<[u32; 4]> = edges
            .iter()
            .map(|e| {
                let (src, dst) = if reverse {
                    (&e.to, &e.from)
                } else {
                    (&e.from, &e.to)
                };
                [id(src), id(dst), 0, 0]
            })
            .collect();
        let eligible: Vec<u32> = keys
            .iter()
            .map(|key| u32::from(self.resolve_node_ref(primary, key).is_some()))
            .collect();
        for seed in seeds {
            let hits = self.gpu.as_ref().unwrap().match_dfs(
                &encoded,
                &eligible,
                id(seed),
                hop.min_depth as u32,
                hop.max_depth as u32,
                300usize.saturating_sub(out.len()),
            )?;
            for (node_id, edge_id) in hits {
                let node = self
                    .resolve_node_ref(primary, &keys[node_id])
                    .expect("eligible node");
                let mut merged = row.clone();
                for (k, v) in node {
                    merged.insert(format!("{}.{k}", hop.bind), v.clone());
                }
                if let Some(bind) = &hop.edge {
                    let edge = edges[edge_id];
                    merged.insert(format!("{bind}.rel"), Cell::text_arc(edge.rel.as_str()));
                    merged.insert(format!("{bind}.from"), Cell::text_arc(edge.from.as_str()));
                    merged.insert(format!("{bind}.to"), Cell::text_arc(edge.to.as_str()));
                }
                out.push(merged);
            }
            if out.len() >= 300 {
                break;
            }
        }
        Ok(())
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
        let unfiltered = db.run("items | count by k").unwrap().rows;
        let filtered = db
            .run(r#"items | tag == "active" | count by k"#)
            .unwrap()
            .rows;
        assert_eq!(unfiltered, filtered);
        assert_eq!(unfiltered.len(), 5);
        assert_eq!(unfiltered[0]["k"], Cell::Null);

        // Sum by k must keep Int(1) and Float(1.0) in distinct groups
        let sums = db.run("items | sum v by k").unwrap().rows;
        assert_eq!(sums.len(), 5);

        // Window PARTITION BY must also use Cell identity without string-formatting collisions
        let win = db.run("items | window row_number by k as rn").unwrap().rows;
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
                let a = db.run(query).unwrap().rows;
                let b = expected.run(query).unwrap().rows;
                assert_eq!(a.len(), n);
                assert_eq!(a, b);
                let read = Db::open_read(&_dir.0).unwrap();
                assert_eq!(read.run(query).unwrap().rows, a);
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
}
