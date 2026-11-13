//! In-memory multi-model storage engine (`Store`): row collections, secondary/FTS/HNSW index hooks, and MVCC snapshots.

pub mod checkpoint;
pub mod columnar;
pub mod edges;
pub mod value;

#[allow(unused_imports)]
pub use crate::catalog::catalog_hash;
pub(crate) use checkpoint::StoreOpenPhases;
pub use columnar::DictColumn;
pub use edges::Edge;
pub(crate) use value::intern_wing;
#[allow(unused_imports)]
pub(crate) use value::take_recycled_id_title_vec;
#[allow(unused_imports)]
pub use value::{
    Cell, InlineStr, Row, RowExt, acquire_orders_row, acquire_row, cell_from_arc_inline,
    compact_row, content_hash_arc, fnv1a64, now_ms, project_fields, release_id_title_rows,
    release_orders_row, row_byte_size, row_text,
};

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Arc, OnceLock};

use rustc_hash::{FxHashMap, FxHashSet};

use crate::catalog::Catalog;
use crate::error::Error;
use crate::index::LiveIndex;
use crate::persist::{self, ColSnap, IndexSnap, Pack, RelSnap};

use columnar::{swap_pop_at, swap_push_at};
use edges::FactKey;
use value::{
    DocSeed, doc_row, docs_hot_field, next_docs_epoch, order_row, row_id_title, spo_key, user_row,
};

/// Minimum collection size to spill into columnar `DiskEngine` segments on `cold: true` checkpoint.
pub const COLD_MIN_ROWS: usize = 32;

pub(crate) struct DiskColLazy {
    pub row_count: usize,
    pub materialized: OnceLock<LazyRows>,
}

/// Rows of a disk-backed collection read into memory on demand, plus a primary-key map
/// built the first time a caller needs to address them by `id`. Both are dropped together
/// whenever the collection's disk state changes.
#[derive(Default)]
pub(crate) struct LazyRows {
    pub rows: Vec<Row>,
    by_id: OnceLock<FxHashMap<String, usize>>,
}

impl LazyRows {
    fn index_of(&self, id: &str) -> Option<usize> {
        self.by_id
            .get_or_init(|| {
                let mut map = FxHashMap::default();
                map.reserve(self.rows.len());
                for (i, r) in self.rows.iter().enumerate() {
                    if let Some(id) = row_text(r, "id") {
                        map.insert(id.to_string(), i);
                    }
                }
                map
            })
            .get(id)
            .copied()
    }
}

impl std::fmt::Debug for DiskColLazy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DiskColLazy")
            .field("row_count", &self.row_count)
            .field("materialized", &self.materialized.get().is_some())
            .finish()
    }
}

pub(crate) struct StoreDiskTier {
    pub engine: Arc<std::sync::RwLock<crate::disk::DiskEngine>>,
    pub tables: BTreeMap<String, DiskColLazy>,
    pub native_disk: bool,
    pub writer_lock: Option<Arc<std::fs::File>>,
}

impl std::fmt::Debug for StoreDiskTier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StoreDiskTier")
            .field("tables", &self.tables.keys().collect::<Vec<_>>())
            .field("native_disk", &self.native_disk)
            .finish()
    }
}

#[derive(Debug)]
pub struct Store {
    pub r#gen: u64,
    pub embed_id: String,
    pub next_id: u64,
    pub collections: BTreeMap<String, Vec<Row>>,
    pub edges: Vec<Edge>,
    pub extra_collections: BTreeMap<String, ColSnap>,
    pub extra_rels: BTreeMap<String, RelSnap>,
    pub extra_indexes: BTreeMap<String, IndexSnap>,
    pub extra_filters: BTreeMap<String, String>,
    pub extra_owned: BTreeMap<String, Vec<(String, String)>>,
    pub indexes: BTreeMap<String, LiveIndex>,
    /// collection → FTS postings (fields with catalog `fts`).
    pub(crate) fts: BTreeMap<String, crate::fts::FtsIndex>,
    /// collection → HNSW vector index (f32 or SQ8)
    pub(crate) hnsw: BTreeMap<String, crate::embed::VectorIndex>,
    /// collection → id → row index (O(1) Get / IndexSeek fetch).
    pub(crate) by_id: BTreeMap<String, FxHashMap<Arc<str>, usize>>,
    /// docs uri → row index.
    pub(crate) docs_by_uri: FxHashMap<Arc<str>, usize>,
    /// facts (s,p,o) → row index — O(1) append idempotency.
    pub(crate) facts_by_spo: FxHashMap<FactKey, usize>,
    /// edge (rel,from,to) set — O(1) append/delete edge.
    pub(crate) edge_keys: FxHashSet<(Arc<str>, Arc<str>, Arc<str>)>,
    /// rel → from → target node keys (zero-alloc outgoing adjacency index).
    pub(crate) edges_out: edges::AdjacencyMap,
    /// rel → to → source node keys (zero-alloc incoming adjacency index).
    pub(crate) edges_in: edges::AdjacencyMap,

    // Hot collection metadata — always accessed during scans
    /// Count of rows per collection for quick bounds checking
    pub(crate) col_counts: FxHashMap<String, usize>,

    /// Parallel Arc columns for docs — contains scans + projected materialize
    /// without cloning full `BTreeMap` rows. Organized SoA (struct-of-arrays) for cache locality.
    pub(crate) docs_id: Vec<Arc<str>>,
    docs_title: Vec<Arc<str>>,
    docs_title_packed: OnceLock<(Vec<u8>, bool)>,
    pub(crate) docs_epoch: u64,
    docs_layer: DictColumn,
    docs_wing: DictColumn,
    last_doc_id_idx: std::sync::atomic::AtomicUsize,
    pub(crate) docs_moved_ids: smallvec::SmallVec<[(Arc<str>, usize); 16]>,
    pub(crate) docs_moved_idxs: smallvec::SmallVec<[(usize, usize); 16]>,
    pub(crate) retired_doc_ids: smallvec::SmallVec<[Arc<str>; 16]>,
    pub(crate) retired_arcs: smallvec::SmallVec<[Arc<str>; 16]>,
    pub(crate) fts_deferred_doc_swaps: smallvec::SmallVec<[(usize, usize, usize); 8]>,
    pub(crate) retired_rows: smallvec::SmallVec<[Row; 2]>,
    pub(crate) retired_cells: smallvec::SmallVec<[Cell; 8]>,
    /// SoA for orders / users — FK join + OLAP project without per-row maps.
    pub(crate) orders_id: Vec<Arc<str>>,
    pub(crate) orders_user_id: Vec<Arc<str>>,
    pub(crate) orders_total: Vec<f64>,
    users_id: Vec<Arc<str>>,
    users_email: Vec<Arc<str>>,
    /// Columnar facts (append hot path). `collections["facts"]` may lag until
    /// [`Self::ensure_facts_rows`].
    facts_s: Vec<Arc<str>>,
    facts_p: Vec<Arc<str>>,
    facts_o: Vec<Arc<str>>,
    /// When true, columnar facts are ahead of `collections["facts"]`.
    facts_rows_stale: bool,
    /// Columnar mmap disk tier (`DiskEngine`) for out-of-core / cold collections.
    pub(crate) disk: Option<StoreDiskTier>,
    /// FTS index field definitions for cold tables whose in-memory `fts` entry
    /// was invalidated by a zero-promote overlay mutation (rebuilt lazily on checkpoint).
    pub(crate) dirty_fts_disk: BTreeMap<String, Vec<String>>,
    /// Stashed FTS state while a `Transaction` savepoint is open over a cold overlay.
    pub(crate) overlay_savepoint_fts:
        Option<(BTreeMap<String, crate::fts::FtsIndex>, BTreeMap<String, Vec<String>>)>,
    /// Cluster keys declared this session (`collection -> column`). The disk tier is
    /// attached read-only between checkpoints, so they are applied to the manifest by the
    /// next checkpoint; they also outlive a collection being promoted and spilled again.
    pub(crate) disk_cluster_keys: BTreeMap<String, String>,
    /// Cold collections whose segments the next checkpoint should rewrite (compaction).
    pub(crate) disk_compact_requests: Vec<String>,
}

/// Compact collection key for zero-allocation `AppendMark` snapshots.
#[derive(Debug, Clone)]
enum AppendColKey {
    Inline(InlineStr),
    Heap(Arc<str>),
}

impl AppendColKey {
    #[inline]
    fn from_name(s: &str) -> Self {
        match InlineStr::try_new(s) {
            Some(inl) => Self::Inline(inl),
            None => Self::Heap(Arc::from(s)),
        }
    }

    #[inline]
    fn as_str(&self) -> &str {
        match self {
            Self::Inline(inl) => inl.as_str(),
            Self::Heap(arc) => arc.as_ref(),
        }
    }
}

/// Cheap undo for append-only packs (insert / append) — no full store clone.
#[derive(Debug, Clone)]
pub struct AppendMark {
    next_id: u64,
    col_lens: smallvec::SmallVec<[(AppendColKey, usize); 6]>,
    edge_len: usize,
    facts_len: usize,
}

impl Store {
    pub fn empty(embed_id: impl Into<String>) -> Self {
        // ✅ Preallocate collections with expected count (4 typical tables)
        let mut collections = BTreeMap::new();
        for name in ["docs", "users", "orders", "facts"] {
            collections.insert(name.into(), Vec::new());
        }

        // Cache-friendly: track row counts per collection for fast bounds checking
        let mut col_counts = FxHashMap::default();
        col_counts.insert("docs".to_string(), 0);
        col_counts.insert("users".to_string(), 0);
        col_counts.insert("orders".to_string(), 0);
        col_counts.insert("facts".to_string(), 0);

        Self {
            r#gen: 0,
            embed_id: embed_id.into(),
            next_id: 1,
            collections,
            edges: Vec::new(),
            extra_collections: BTreeMap::new(),
            extra_rels: BTreeMap::new(),
            extra_indexes: BTreeMap::new(),
            extra_filters: BTreeMap::new(),
            extra_owned: BTreeMap::new(),
            indexes: BTreeMap::new(),
            fts: BTreeMap::new(),
            hnsw: BTreeMap::new(),
            by_id: BTreeMap::new(),
            docs_by_uri: FxHashMap::default(),
            facts_by_spo: FxHashMap::default(),
            edge_keys: FxHashSet::default(),
            edges_out: FxHashMap::default(),
            edges_in: FxHashMap::default(),
            col_counts,
            docs_id: Vec::new(),
            docs_title: Vec::new(),
            docs_title_packed: OnceLock::new(),
            docs_epoch: next_docs_epoch(),
            docs_layer: DictColumn::new(),
            docs_wing: DictColumn::new(),
            last_doc_id_idx: std::sync::atomic::AtomicUsize::new(usize::MAX),
            docs_moved_ids: smallvec::SmallVec::new(),
            docs_moved_idxs: smallvec::SmallVec::new(),
            retired_doc_ids: smallvec::SmallVec::new(),
            retired_arcs: smallvec::SmallVec::new(),
            fts_deferred_doc_swaps: smallvec::SmallVec::new(),
            retired_rows: smallvec::SmallVec::new(),
            retired_cells: smallvec::SmallVec::new(),
            orders_id: Vec::new(),
            orders_user_id: Vec::new(),
            orders_total: Vec::new(),
            users_id: Vec::new(),
            users_email: Vec::new(),
            facts_s: Vec::new(),
            facts_p: Vec::new(),
            facts_o: Vec::new(),
            facts_rows_stale: false,
            disk: None,
            dirty_fts_disk: BTreeMap::new(),
            overlay_savepoint_fts: None,
            disk_cluster_keys: BTreeMap::new(),
            disk_compact_requests: Vec::new(),
        }
    }

    #[inline]
    pub fn is_disk_backed(&self, name: &str) -> bool {
        self.disk
            .as_ref()
            .is_some_and(|d| d.tables.contains_key(name))
    }

    /// An already validated pack (WAL replay, follower apply, executor after its own checks)
    /// can be applied to the disk overlay whenever nothing tracks in-memory row positions.
    /// Scalar `LiveIndex`es of a disk-backed collection are empty placeholders, so they need
    /// no maintenance; an HNSW graph does.
    #[inline]
    pub fn can_apply_disk_overlay(&self, collection: &str) -> bool {
        self.is_disk_backed(collection) && !self.hnsw.contains_key(collection)
    }

    /// Fields covered by single-column, unconditional unique indexes of `collection`.
    /// `None` when some unique index is composite or partial: its key cannot be checked
    /// against the disk engine with a plain equality lookup.
    pub(crate) fn simple_unique_fields(&self, collection: &str) -> Option<Vec<String>> {
        let mut out: Vec<String> = Vec::new();
        let mut add = |fields: &[String], partial: bool| -> bool {
            if fields.len() != 1 || partial {
                return false;
            }
            if !out.contains(&fields[0]) {
                out.push(fields[0].clone());
            }
            true
        };
        for li in self.indexes.values() {
            if li.def.collection == collection
                && li.def.unique
                && !add(&li.def.fields, li.def.pred.is_some())
            {
                return None;
            }
        }
        for ix in self.extra_indexes.values() {
            if ix.collection == collection
                && ix.unique
                && !add(&ix.fields, ix.pred_src.is_some())
            {
                return None;
            }
        }
        Some(out)
    }

    /// True when some live row of a disk-backed collection has `field == key` (text key).
    pub(crate) fn disk_count_eq(&self, collection: &str, field: &str, key: &str) -> usize {
        self.disk_engine_read()
            .and_then(|eng| eng.count_by_eq(collection, field, &Cell::from_text(key)).ok())
            .unwrap_or(0)
    }

    /// True when `field` takes part in a unique index of `collection`.
    pub(crate) fn field_in_unique_index(&self, collection: &str, field: &str) -> bool {
        self.extra_indexes.values().any(|ix| {
            ix.collection == collection && ix.unique && ix.fields.iter().any(|f| f == field)
        }) || self.indexes.values().any(|li| {
            li.def.collection == collection
                && li.def.unique
                && li.def.fields.iter().any(|f| f == field)
        })
    }

    #[inline]
    pub fn can_mutate_disk_overlay(&self, collection: &str) -> bool {
        self.is_disk_backed(collection)
            && !self.hnsw.contains_key(collection)
            && !self
                .extra_indexes
                .values()
                .any(|ix| ix.collection == collection && ix.unique)
            && !self
                .indexes
                .values()
                .any(|li| li.def.collection == collection && li.def.unique)
    }

    /// First live row of a disk-backed collection with `field == key`, read through the
    /// engine (segments + overlay) without promoting the collection into memory.
    pub(crate) fn disk_point_row(&self, collection: &str, field: &str, key: &str) -> Option<Row> {
        let eng = self.disk_engine_read()?;
        if field == "id" {
            return eng.get_by_id(collection, key).ok().flatten();
        }
        let mut found = None;
        let _ = eng.for_each_by_eq_projected(
            collection,
            field,
            &Cell::from_text(key),
            None,
            |row| {
                found = Some(row.clone());
                Ok(false)
            },
        );
        found
    }

    pub fn disk_has_key(&self, collection: &str, field: &str, key: &str) -> bool {
        let Some(eng) = self.disk_engine_read() else {
            return false;
        };
        if field == "id" {
            return eng.exists_by_id(collection, key);
        }
        let want_fields = ["id"];
        let mut found = false;
        let _ = eng.for_each_by_eq_projected(
            collection,
            field,
            &Cell::from_text(key),
            Some(&want_fields),
            |_| {
                found = true;
                Ok(false)
            },
        );
        if found {
            return true;
        }
        false
    }

    pub(crate) fn apply_disk_overlay_ops(&mut self, ops: &[crate::disk::DiskWalOp]) -> Result<(), Error> {
        if ops.is_empty() {
            return Ok(());
        }
        let mut touched: smallvec::SmallVec<[String; 4]> = smallvec::SmallVec::new();
        for op in ops {
            let t = match op {
                crate::disk::DiskWalOp::EnsureTable { schema } => &schema.name,
                crate::disk::DiskWalOp::EnsureIndex { table, .. }
                | crate::disk::DiskWalOp::UpsertBatch { table, .. }
                | crate::disk::DiskWalOp::DeleteIds { table, .. } => table,
            };
            if !touched.iter().any(|x| x == t) {
                touched.push(t.clone());
            }
        }
        let Some(d) = self.disk.as_mut() else {
            return Ok(());
        };
        for t in &touched {
            if let Some(lazy) = d.tables.get_mut(t) {
                lazy.materialized = OnceLock::new();
            }
            if let Some(fts_idx) = self.fts.remove(t) {
                self.dirty_fts_disk.insert(t.clone(), fts_idx.fields.clone());
                if let Some((saved_fts, _)) = self.overlay_savepoint_fts.as_mut() {
                    saved_fts.entry(t.clone()).or_insert(fts_idx);
                }
            }
        }
        {
            let mut eng = d
                .engine
                .write()
                .map_err(|_| Error::runtime("disk engine lock poisoned"))?;
            for op in ops {
                eng.apply_mem_overlay_op(op.clone());
            }
            for t in &touched {
                let nrows = eng.count_table(t);
                if let Some(lazy) = d.tables.get_mut(t) {
                    lazy.row_count = nrows;
                }
                self.col_counts.insert(t.clone(), nrows);
            }
        }
        Ok(())
    }

    pub(crate) fn begin_disk_overlay_savepoint(&mut self) {
        if self.overlay_savepoint_fts.is_none() {
            self.overlay_savepoint_fts = Some((BTreeMap::new(), self.dirty_fts_disk.clone()));
        }
        if let Some(d) = self.disk.as_ref()
            && let Ok(mut eng) = d.engine.write()
        {
            eng.begin_mem_savepoint();
        }
    }

    pub(crate) fn commit_disk_overlay_savepoint(&mut self) {
        self.overlay_savepoint_fts = None;
        if let Some(d) = self.disk.as_ref()
            && let Ok(mut eng) = d.engine.write()
        {
            eng.discard_mem_savepoint();
        }
    }

    pub(crate) fn rollback_disk_overlay_savepoint(&mut self) {
        if let Some((saved_fts, saved_dirty)) = self.overlay_savepoint_fts.take() {
            for (k, v) in saved_fts {
                self.fts.insert(k, v);
            }
            self.dirty_fts_disk = saved_dirty;
        }
        let Some(d) = self.disk.as_mut() else {
            return;
        };
        if let Ok(mut eng) = d.engine.write() {
            eng.rollback_savepoint();
            for (t, lazy) in d.tables.iter_mut() {
                lazy.materialized = OnceLock::new();
                let nrows = eng.count_table(t);
                lazy.row_count = nrows;
                self.col_counts.insert(t.clone(), nrows);
            }
        }
    }

    #[inline]
    pub fn is_native_disk(&self) -> bool {
        self.disk
            .as_ref()
            .is_some_and(|d| d.native_disk && d.writer_lock.is_some())
    }

    #[inline]
    pub(crate) fn disk_engine_read(
        &self,
    ) -> Option<std::sync::RwLockReadGuard<'_, crate::disk::DiskEngine>> {
        self.disk.as_ref().and_then(|d| d.engine.read().ok())
    }

    /// Declare (or clear) the column that the disk tier keeps `collection` ordered by.
    /// Applies to segments written from now on; existing ones are reordered by compaction.
    pub fn set_disk_cluster_key(&mut self, collection: &str, field: Option<&str>) -> Result<(), Error> {
        if !self.is_disk_backed(collection) {
            return Err(Error::runtime(format!(
                "cluster key: {collection} is not in the disk tier"
            )));
        }
        let Some(d) = self.disk.as_ref() else {
            return Err(Error::runtime("cluster key needs a disk tier (open with cold: true)"));
        };
        let mut eng = d
            .engine
            .write()
            .map_err(|_| Error::runtime("disk engine lock poisoned"))?;
        if eng.is_read_only() {
            // Cold tier: the attached engine is a read-only view. Record the key in memory
            // (so compaction-free readers see it) and let the next checkpoint persist it.
            eng.set_cluster_key_in_memory(collection, field)?;
        } else {
            eng.set_cluster_key(collection, field)?;
        }
        match field {
            Some(f) => self.disk_cluster_keys.insert(collection.to_string(), f.to_string()),
            None => self.disk_cluster_keys.insert(collection.to_string(), String::new()),
        };
        Ok(())
    }

    /// Cluster key of a disk-backed collection, if one is declared.
    pub fn disk_cluster_key(&self, collection: &str) -> Option<String> {
        if let Some(k) = self.disk_cluster_keys.get(collection) {
            return (!k.is_empty()).then(|| k.clone());
        }
        self.disk_engine_read()?
            .manifest
            .tables
            .get(collection)?
            .cluster_by
            .clone()
    }

    /// Promote a disk-backed collection into `collections` (`Vec<Row>`) with full row maps & secondary indexes.
    pub fn promote_disk_col(&mut self, name: &str) {
        let Some((row_count, maybe_rows)) = (|| -> Option<(usize, Vec<Row>)> {
            let d = self.disk.as_mut()?;
            let lazy = d.tables.remove(name)?;
            let rc = lazy.row_count;
            if let Some(lazy_rows) = lazy.materialized.into_inner() {
                return Some((rc, lazy_rows.rows));
            }
            let mut out = Vec::with_capacity(rc);
            if let Ok(eng) = d.engine.read() {
                let _ = eng.scan_projected(name, None, |r| {
                    out.push(r.clone());
                    Ok(true)
                });
            }
            Some((rc, out))
        })() else {
            return;
        };
        let n = maybe_rows.len().max(row_count.min(maybe_rows.len()));
        self.col_counts.insert(name.to_string(), maybe_rows.len());
        let _ = n;
        self.collections.insert(name.to_string(), maybe_rows);
        self.rebuild_row_maps_collection(name);
        self.rebuild_indexes_collection(name);
        if let Some(fields) = self.dirty_fts_disk.remove(name) {
            let rows = self.collection(name);
            self.fts
                .insert(name.to_string(), crate::fts::FtsIndex::build(rows, &fields));
        }
    }

    #[inline]
    pub fn promote_cold(&mut self, name: &str) {
        self.promote_disk_col(name);
    }

    fn clone_rows_parallel(rows: &[Row]) -> Vec<Row> {
        let n = rows.len();
        if n < 65536 {
            return rows.to_vec();
        }
        let max_threads = if n >= 8192 { 8 } else { 4 };
        let threads = std::thread::available_parallelism()
            .map(|p| p.get())
            .unwrap_or(1)
            .min(max_threads)
            .min((n + 511) / 512);
        if threads <= 1 {
            return rows.to_vec();
        }
        let chunk_size = (n + threads - 1) / threads;
        std::thread::scope(|scope| {
            let mut handles = Vec::with_capacity(threads - 1);
            let mut all_chunks = rows.chunks(chunk_size);
            for _ in 0..threads - 1 {
                if let Some(chunk) = all_chunks.next() {
                    handles.push(scope.spawn(move || chunk.to_vec()));
                }
            }
            let my_chunk = all_chunks.next().unwrap_or(&[]);
            let my_work = my_chunk.to_vec();
            let mut out = Vec::with_capacity(n);
            for handle in handles {
                out.extend(handle.join().expect("row clone worker panicked"));
            }
            out.extend(my_work);
            out
        })
    }

    /// Deep-copy the memory image (no durable handle). Used for read snapshots.
    pub fn clone_mem(&mut self) -> Self {
        self.flush_fts_deferred_docs();
        self.ensure_facts_rows();
        let disk_snap = self.disk.as_ref().and_then(|d| {
            let eng = d.engine.read().ok()?;
            let view = eng.snapshot_view();
            let mut tables = BTreeMap::new();
            for (k, v) in &d.tables {
                tables.insert(
                    k.clone(),
                    DiskColLazy {
                        row_count: v.row_count,
                        materialized: OnceLock::new(),
                    },
                );
            }
            Some(StoreDiskTier {
                engine: Arc::new(std::sync::RwLock::new(view)),
                tables,
                native_disk: d.native_disk,
                writer_lock: None,
            })
        });
        let docs_title_packed = OnceLock::new();
        if let Some(val) = self.docs_title_packed.get() {
            let _ = docs_title_packed.set(val.clone());
        }
        let mut collections = BTreeMap::new();
        for (name, rows) in &self.collections {
            collections.insert(name.clone(), Self::clone_rows_parallel(rows));
        }
        Self {
            r#gen: self.r#gen,
            embed_id: self.embed_id.clone(),
            next_id: self.next_id,
            collections,
            edges: self.edges.clone(),
            extra_collections: self.extra_collections.clone(),
            extra_rels: self.extra_rels.clone(),
            extra_indexes: self.extra_indexes.clone(),
            extra_filters: self.extra_filters.clone(),
            extra_owned: self.extra_owned.clone(),
            indexes: self
                .indexes
                .iter()
                .map(|(k, v)| (k.clone(), v.clone_for_read()))
                .collect(),
            fts: self.fts.clone(),
            hnsw: self.hnsw.clone(),
            by_id: self.by_id.clone(),
            docs_by_uri: self.docs_by_uri.clone(),
            facts_by_spo: self.facts_by_spo.clone(),
            edge_keys: self.edge_keys.clone(),
            edges_out: self.edges_out.clone(),
            edges_in: self.edges_in.clone(),
            col_counts: self.col_counts.clone(),
            docs_id: self.docs_id.clone(),
            docs_title: self.docs_title.clone(),
            docs_title_packed,
            docs_epoch: self.docs_epoch,
            docs_layer: self.docs_layer.clone(),
            docs_wing: self.docs_wing.clone(),
            last_doc_id_idx: std::sync::atomic::AtomicUsize::new(
                self.last_doc_id_idx
                    .load(std::sync::atomic::Ordering::Relaxed),
            ),
            docs_moved_ids: self.docs_moved_ids.clone(),
            docs_moved_idxs: self.docs_moved_idxs.clone(),
            retired_doc_ids: self.retired_doc_ids.clone(),
            retired_arcs: smallvec::SmallVec::new(),
            fts_deferred_doc_swaps: smallvec::SmallVec::new(),
            retired_rows: smallvec::SmallVec::new(),
            retired_cells: smallvec::SmallVec::new(),
            orders_id: self.orders_id.clone(),
            orders_user_id: self.orders_user_id.clone(),
            orders_total: self.orders_total.clone(),
            users_id: self.users_id.clone(),
            users_email: self.users_email.clone(),
            facts_s: self.facts_s.clone(),
            facts_p: self.facts_p.clone(),
            facts_o: self.facts_o.clone(),
            facts_rows_stale: false,
            disk: disk_snap,
            dirty_fts_disk: self.dirty_fts_disk.clone(),
            overlay_savepoint_fts: None,
            disk_cluster_keys: BTreeMap::new(),
            disk_compact_requests: Vec::new(),
        }
    }

    pub fn apply_pack(&mut self, pack: &Pack) {
        match pack {
            Pack::Insert {
                collection,
                row,
                edges,
            } => {
                if self.can_apply_disk_overlay(collection) {
                    let mut ops = Vec::new();
                    checkpoint::pack_to_disk_wal_ops(pack, &mut ops);
                    let _ = self.apply_disk_overlay_ops(&ops);
                    for e in edges {
                        let _ = self.append_edge_parts(&e.rel, &e.from, &e.to);
                    }
                    return;
                }
                let emb = row.get("embedding").and_then(|c| match c {
                    Cell::Vec(v) => Some(Arc::clone(v)),
                    _ => None,
                });
                self.collection_mut(collection).push(row.clone());
                let idx = self.collection(collection).len() - 1;
                // Update cache-friendly count tracking
                self.update_col_count(collection, 1);

                self.row_maps_register(collection, idx);
                let _ = self.index_insert_at(collection, idx);
                self.fts_insert_at(collection, idx);
                if let Some(v) = emb
                    && let Some(hnsw) = self.hnsw.get_mut(collection)
                {
                    hnsw.insert(idx, v);
                }
                for e in edges {
                    let _ = self.append_edge_parts(&e.rel, &e.from, &e.to);
                }
            }
            Pack::InsertBulk {
                collection,
                rows,
                edges,
            } => {
                if self.can_apply_disk_overlay(collection) {
                    let mut ops = Vec::new();
                    checkpoint::pack_to_disk_wal_ops(pack, &mut ops);
                    let _ = self.apply_disk_overlay_ops(&ops);
                    for e in edges {
                        let _ = self.append_edge_parts(&e.rel, &e.from, &e.to);
                    }
                    return;
                }
                let start = self.collection(collection).len();
                let n = rows.len();
                self.collection_mut(collection).reserve(rows.len());
                self.row_maps_reserve(collection, rows.len());
                let _ = self.index_insert_slab(collection, start, rows);
                self.fts_insert_slab(collection, start, rows);
                self.hnsw_insert_slab(collection, start, rows);
                self.row_maps_register_slab(collection, start, rows);
                self.collection_mut(collection).extend(rows.iter().cloned());
                // Update cache-friendly count tracking
                self.update_col_count(collection, n as isize);
                for e in edges {
                    let _ = self.append_edge_parts(&e.rel, &e.from, &e.to);
                }
            }
            Pack::InsertCols {
                collection,
                fields,
                cols,
                n,
                edges,
            } => {
                if self.can_apply_disk_overlay(collection) {
                    let mut ops = Vec::new();
                    checkpoint::pack_to_disk_wal_ops(pack, &mut ops);
                    let _ = self.apply_disk_overlay_ops(&ops);
                    for e in edges {
                        let _ = self.append_edge_parts(&e.rel, &e.from, &e.to);
                    }
                    return;
                }
                let n = *n as usize;
                let rows = persist::cols_to_rows(fields, cols, n);
                let start = self.collection(collection).len();
                self.collection_mut(collection).reserve(rows.len());
                self.row_maps_reserve(collection, rows.len());
                let _ = self.index_insert_slab(collection, start, &rows);
                self.fts_insert_slab(collection, start, &rows);
                self.hnsw_insert_slab(collection, start, &rows);
                self.row_maps_register_slab(collection, start, &rows);
                self.collection_mut(collection).extend(rows);
                // Update cache-friendly count tracking
                self.update_col_count(collection, n as isize);
                for e in edges {
                    let _ = self.append_edge_parts(&e.rel, &e.from, &e.to);
                }
            }
            Pack::AppendFact { row } => {
                // Zero-copy borrow: avoid unnecessary clone from owned Row
                let (result, _) = self.append_fact_row(&row);
                drop(result); // Drop the returned row since we already updated collection
            }
            Pack::AppendFactsBulk { s, p, o } => {
                let _ = self.append_facts_spo_bulk(s, p, o, false);
            }
            Pack::AppendEdge { rel, from, to } => {
                let _ = self.append_edge_parts(rel, from, to);
            }
            Pack::AppendEdgesBulk { rel, from, to } => {
                let n = rel.len().min(from.len()).min(to.len());
                self.edges.reserve(n);
                self.edge_keys.reserve(n);
                for i in 0..n {
                    let _ = self.append_edge_parts(&rel[i], &from[i], &to[i]);
                }
            }
            Pack::Update { collection, rows } => {
                if self.can_apply_disk_overlay(collection) {
                    let mut ops = Vec::new();
                    checkpoint::pack_to_disk_wal_ops(pack, &mut ops);
                    let _ = self.apply_disk_overlay_ops(&ops);
                    return;
                }
                let mut new_rows = 0;
                for new in rows {
                    let id = row_text(new, "id").map(str::to_string);
                    let idx = id.as_ref().and_then(|id| self.row_index(collection, id));
                    let new_emb = new.get("embedding").and_then(|c| match c {
                        Cell::Vec(v) => Some(Arc::clone(v)),
                        _ => None,
                    });
                    if let Some(i) = idx {
                        let before = self.collection(collection)[i].clone();
                        let emb_changed = before.get("embedding") != new.get("embedding");
                        self.collection_mut(collection)[i] = new.clone();
                        self.row_maps_sync_updated(collection, i, &before, new);
                        let _ = self.index_update_row(collection, i, &before, new);
                        self.fts_sync_at(collection, i, &before);
                        if emb_changed && let Some(hnsw) = self.hnsw.get_mut(collection) {
                            if let Some(v) = new_emb {
                                hnsw.insert(i, v);
                            } else {
                                hnsw.remove(i);
                            }
                        }
                        continue;
                    }
                    self.collection_mut(collection).push(new.clone());
                    let i = self.collection(collection).len() - 1;
                    new_rows += 1;
                    self.row_maps_register(collection, i);
                    let _ = self.index_insert_at(collection, i);
                    self.fts_insert_at(collection, i);
                    if let Some(v) = new_emb
                        && let Some(hnsw) = self.hnsw.get_mut(collection)
                    {
                        hnsw.insert(i, v);
                    }
                }
                // Update cache-friendly count tracking
                self.update_col_count(collection, new_rows as isize);
            }
            Pack::Reembed => {}
            Pack::Delete { collection, rows } => {
                if self.can_apply_disk_overlay(collection) {
                    let mut ops = Vec::new();
                    checkpoint::pack_to_disk_wal_ops(pack, &mut ops);
                    let _ = self.apply_disk_overlay_ops(&ops);
                    return;
                }
                let dead = self.dead_positions(collection, rows);
                self.delete_rows_at(collection, &dead);
            }
            Pack::DeleteEdge { rel, from, to } => {
                self.remove_edge(rel, from, to);
            }
            Pack::SchemaCol {
                name,
                append,
                fields,
            } => {
                self.extra_collections.insert(
                    name.clone(),
                    ColSnap {
                        name: name.clone(),
                        append_only: *append,
                        fields: fields.clone(),
                    },
                );
                self.collections.entry(name.clone()).or_default();
                self.col_counts.entry(name.clone()).or_insert(0);
            }
            Pack::SchemaRel {
                name,
                stub,
                reverse_of,
            } => {
                self.extra_rels.insert(
                    name.clone(),
                    RelSnap {
                        name: name.clone(),
                        stub: *stub,
                        reverse_of: reverse_of.clone(),
                    },
                );
            }
            Pack::SchemaIndex {
                collection,
                unique,
                fields,
                pred_src,
            } => {
                let snap = IndexSnap {
                    collection: collection.clone(),
                    unique: *unique,
                    fields: fields.clone(),
                    pred_src: pred_src.clone(),
                };
                let label = format!("{}[{}]", collection, fields.join(","));
                self.extra_indexes.insert(label.clone(), snap);
                let pred = pred_src
                    .as_deref()
                    .and_then(|src| crate::parse::parse_pred_src(src).ok());
                let def = crate::catalog::IndexDef {
                    collection: collection.clone(),
                    unique: *unique,
                    fields: fields.clone(),
                    pred,
                    bitmap_card_field: None,
                };
                let mut live = LiveIndex::new(def);
                for (i, row) in self.collection(collection).iter().enumerate() {
                    let _ = live.insert_at(i, row);
                }
                self.indexes.insert(label, live);
                if fields.iter().any(|f| f == "embedding") {
                    self.build_hnsw(collection, 16, 64);
                }
            }
            Pack::SchemaFilter {
                collection,
                pred_src,
            } => match pred_src {
                Some(src) => {
                    self.extra_filters.insert(collection.clone(), src.clone());
                }
                None => {
                    self.extra_filters.remove(collection);
                }
            },
            Pack::SchemaOwned { name, fields } => {
                self.extra_owned.insert(name.clone(), fields.clone());
            }
            Pack::Batch { packs } => {
                self.apply_pack_batch(packs, 0);
            }
        }
    }

    fn apply_pack_batch(&mut self, packs: &[Pack], depth: usize) {
        if depth > 64 {
            return;
        }
        for p in packs {
            if let Pack::Batch { packs: sub } = p {
                self.apply_pack_batch(sub, depth + 1);
            } else {
                self.apply_pack(p);
            }
        }
    }

    pub fn mem_backup(&mut self) -> MemBackup {
        self.flush_fts_deferred_docs();
        self.ensure_facts_rows();
        let disk_names: Vec<String> = self
            .disk
            .as_ref()
            .map(|d| d.tables.keys().cloned().collect())
            .unwrap_or_default();
        for name in disk_names {
            self.promote_disk_col(&name);
        }
        // Deletions bitmap is not persisted (snapshots are always compacted)
        MemBackup {
            r#gen: self.r#gen,
            embed_id: self.embed_id.clone(),
            next_id: self.next_id,
            collections: self.collections.clone(),
            edges: self.edges.clone(),
            extra_collections: self.extra_collections.clone(),
            extra_rels: self.extra_rels.clone(),
            extra_indexes: self.extra_indexes.clone(),
            extra_filters: self.extra_filters.clone(),
            extra_owned: self.extra_owned.clone(),
            hnsw: self.hnsw.clone(),
        }
    }

    pub fn mem_restore(&mut self, b: MemBackup) {
        self.r#gen = b.r#gen;
        self.embed_id = b.embed_id;
        self.next_id = b.next_id;
        self.collections = b.collections;
        self.edges = b.edges;
        self.extra_collections = b.extra_collections;
        self.extra_rels = b.extra_rels;
        self.extra_indexes = b.extra_indexes;
        self.extra_filters = b.extra_filters;
        self.extra_owned = b.extra_owned;
        self.hnsw = b.hnsw;
        self.facts_rows_stale = false;
        if let Some(d) = self.disk.as_mut() {
            d.tables.clear();
        }
        self.rebuild_indexes();
        self.rebuild_row_maps();
        self.rebuild_all_fts_inplace();
        self.refresh_col_counts();
    }

    /// Mark lengths for append-only rollback (insert/append packs).
    pub fn append_mark(&self) -> AppendMark {
        let mut col_lens = smallvec::SmallVec::new();
        for (name, rows) in &self.collections {
            col_lens.push((AppendColKey::from_name(name), rows.len()));
        }
        AppendMark {
            next_id: self.next_id,
            col_lens,
            edge_len: self.edges.len(),
            facts_len: self.facts_s.len(),
        }
    }

    pub fn append_rollback(&mut self, mark: AppendMark) {
        self.next_id = mark.next_id;

        // 1. O(delta) edge rollback
        if self.edges.len() > mark.edge_len {
            if self.edge_keys.len() == self.edges.len() {
                for idx in (mark.edge_len..self.edges.len()).rev() {
                    let e = &self.edges[idx];
                    let rel_arc = Arc::<str>::from(e.rel.as_str());
                    let from_arc = Arc::<str>::from(e.from.as_str());
                    let to_arc = Arc::<str>::from(e.to.as_str());
                    self.edge_keys.remove(&(rel_arc, from_arc, to_arc));
                    if let Some(by_node) = self.edges_out.get_mut(e.rel.as_str())
                        && let Some(list) = by_node.get_mut(e.from.as_str())
                    {
                        if list.last().is_some_and(|x| x.as_ref() == e.to) {
                            list.pop();
                        } else {
                            list.retain(|x| x.as_ref() != e.to);
                        }
                    }
                    if let Some(by_node) = self.edges_in.get_mut(e.rel.as_str())
                        && let Some(list) = by_node.get_mut(e.to.as_str())
                    {
                        if list.last().is_some_and(|x| x.as_ref() == e.from) {
                            list.pop();
                        } else {
                            list.retain(|x| x.as_ref() != e.from);
                        }
                    }
                }
                self.edges.truncate(mark.edge_len);
            } else {
                self.edges.truncate(mark.edge_len);
                self.rebuild_edge_keys();
            }
        }

        // 2. O(delta) facts rollback
        if self.facts_s.len() > mark.facts_len {
            for idx in (mark.facts_len..self.facts_s.len()).rev() {
                let key = (
                    Arc::clone(&self.facts_s[idx]),
                    Arc::clone(&self.facts_p[idx]),
                    Arc::clone(&self.facts_o[idx]),
                );
                if self.facts_by_spo.get(&key) == Some(&idx) {
                    self.facts_by_spo.remove(&key);
                }
            }
            self.facts_s.truncate(mark.facts_len);
            self.facts_p.truncate(mark.facts_len);
            self.facts_o.truncate(mark.facts_len);
            if !self.facts_rows_stale
                && let Some(f_rows) = self.collections.get_mut("facts")
            {
                f_rows.truncate(mark.facts_len);
            }
            if let Some(c) = self.col_counts.get_mut("facts") {
                *c = mark.facts_len;
            } else {
                self.col_counts.insert("facts".to_string(), mark.facts_len);
            }
        } else if let Some(f_rows) = self.collections.get_mut("facts")
            && f_rows.len() > mark.facts_len
        {
            f_rows.truncate(mark.facts_len);
            if let Some(c) = self.col_counts.get_mut("facts") {
                *c = mark.facts_len;
            } else {
                self.col_counts.insert("facts".to_string(), mark.facts_len);
            }
        }

        // 3. O(delta) collection row/index/map/FTS/HNSW rollback — collect only mutated collections
        let mut dirty: smallvec::SmallVec<[(AppendColKey, usize, usize); 2]> =
            smallvec::SmallVec::new();
        for (name, rows) in &self.collections {
            if name == "facts" {
                continue;
            }
            let cur_len = rows.len();
            if cur_len == 0 {
                continue;
            }
            let prev_len = mark
                .col_lens
                .iter()
                .find(|(n, _)| n.as_str() == name.as_str())
                .map(|(_, l)| *l)
                .unwrap_or(0);
            if cur_len > prev_len {
                dirty.push((AppendColKey::from_name(name), prev_len, cur_len));
            }
        }

        for (name_key, prev_len, cur_len) in dirty {
            let name = name_key.as_str();
            let delta = cur_len - prev_len;
            if name == "docs" {
                self.flush_fts_deferred_docs();
                self.invalidate_docs_cache();
                self.last_doc_id_idx
                    .store(usize::MAX, std::sync::atomic::Ordering::Relaxed);
            }
            let can_incremental = !(prev_len > 0 && delta > prev_len && cur_len > 1024)
                && match name {
                    "docs" => {
                        self.docs_id.len() == cur_len
                            && self.docs_moved_ids.is_empty()
                            && self.docs_moved_idxs.is_empty()
                    }
                    "orders" => self.orders_id.len() == cur_len,
                    "users" => self.users_id.len() == cur_len,
                    _ => true,
                };

            if can_incremental {
                let has_indexes = self.indexes.values().any(|i| i.def.collection == name);
                let has_fts = self.fts.contains_key(name);
                let has_hnsw = self.hnsw.contains_key(name);
                let is_docs = name == "docs";
                let soa_ids: Option<&[Arc<str>]> = match name {
                    "orders" => Some(&self.orders_id),
                    "users" => Some(&self.users_id),
                    "docs" => Some(&self.docs_id),
                    _ => None,
                };

                if !has_indexes
                    && !has_fts
                    && !has_hnsw
                    && !is_docs
                    && let Some(ids) = soa_ids
                {
                    if let Some(by_id) = self.by_id.get_mut(name) {
                        if prev_len == 0 {
                            by_id.clear();
                        } else {
                            for (offset, id_arc) in ids[prev_len..cur_len].iter().enumerate().rev()
                            {
                                let idx = prev_len + offset;
                                let id = id_arc.as_ref();
                                if !id.is_empty() && by_id.get(id) == Some(&idx) {
                                    by_id.remove(id);
                                }
                            }
                        }
                    }
                    if let Some(rows) = self.collections.get_mut(name) {
                        if name == "orders" && cur_len - prev_len <= 64 {
                            while rows.len() > prev_len {
                                if let Some(row) = rows.pop() {
                                    release_orders_row(row);
                                }
                            }
                        } else {
                            rows.truncate(prev_len);
                        }
                    }
                } else {
                    for idx in (prev_len..cur_len).rev() {
                        let Some(row) = self.collections.get_mut(name).and_then(|rows| rows.pop())
                        else {
                            break;
                        };
                        if let Some(id) = row_text(&row, "id")
                            && let Some(by_id) = self.by_id.get_mut(name)
                            && by_id.get(id) == Some(&idx)
                        {
                            by_id.remove(id);
                        }
                        if is_docs
                            && let Some(uri) = row_text(&row, "uri")
                            && self.docs_by_uri.get(uri) == Some(&idx)
                        {
                            self.docs_by_uri.remove(uri);
                        }
                        if has_indexes {
                            self.index_remove_row(name, idx, &row);
                        }
                        if has_fts && let Some(fts) = self.fts.get_mut(name) {
                            fts.remove_row(idx, &row);
                        }
                        if has_hnsw && let Some(hnsw) = self.hnsw.get_mut(name) {
                            hnsw.remove(idx);
                        }
                    }
                }

                match name {
                    "docs" => {
                        self.docs_id.truncate(prev_len);
                        self.docs_title.truncate(prev_len);
                        self.docs_layer.truncate(prev_len);
                        self.docs_wing.truncate(prev_len);
                    }
                    "orders" => {
                        self.orders_id.truncate(prev_len);
                        self.orders_user_id.truncate(prev_len);
                        self.orders_total.truncate(prev_len);
                    }
                    "users" => {
                        self.users_id.truncate(prev_len);
                        self.users_email.truncate(prev_len);
                    }
                    _ => {}
                }
                if let Some(c) = self.col_counts.get_mut(name) {
                    *c = prev_len;
                } else {
                    self.col_counts.insert(name.to_string(), prev_len);
                }
            } else {
                if let Some(rows) = self.collections.get_mut(name) {
                    rows.truncate(prev_len);
                }
                if let Some(c) = self.col_counts.get_mut(name) {
                    *c = prev_len;
                } else {
                    self.col_counts.insert(name.to_string(), prev_len);
                }
                self.rebuild_row_maps_collection(name);
                self.rebuild_indexes_collection(name);
                self.rebuild_fts_inplace(name);
                if let Some(hnsw) = self.hnsw.get_mut(name) {
                    for idx in (prev_len..cur_len).rev() {
                        hnsw.remove(idx);
                    }
                }
            }
        }
    }

    fn rebuild_all_fts_inplace(&mut self) {
        let names = self.fts.keys().cloned().collect::<Vec<_>>();
        for name in names {
            self.rebuild_fts_inplace(&name);
        }
    }

    pub fn fixture(cat: &Catalog) -> Self {
        let mut s = Self::empty(cat.embed_id.clone());
        let now = now_ms();
        let overview = "e7c98d54-b4d6-4165-86e9-9b999e7ce9c3";
        let wal = "d-wal";
        let facts = "d-facts";

        s.collections.insert(
            "docs".into(),
            vec![
                doc_row(DocSeed {
                    id: overview,
                    uri: "wiki://rag-overview",
                    title: "RAG overview",
                    wing: "rag",
                    room: "wiki",
                    layer: "wiki",
                    body: "hybrid search and embedding identity",
                    snippet: "overview of rag",
                    ts: now - 86_400_000,
                }),
                doc_row(DocSeed {
                    id: wal,
                    uri: "raw://n/wal",
                    title: "write ahead wal",
                    wing: "rag",
                    room: "inbox",
                    layer: "raw",
                    body: "wal notes for the reducer",
                    snippet: "wal snippet",
                    ts: now - 7_200_000,
                }),
                doc_row(DocSeed {
                    id: facts,
                    uri: "wiki://lin-facts",
                    title: "Lin facts",
                    wing: "sys",
                    room: "meta",
                    layer: "wiki",
                    body: "catalog and facts",
                    snippet: "lin facts",
                    ts: now - 3 * 86_400_000,
                }),
            ],
        );
        s.collections.insert(
            "users".into(),
            vec![
                user_row("u1", "alice@lin.dev"),
                user_row("u2", "bob@lin.dev"),
            ],
        );
        s.collections.insert(
            "orders".into(),
            vec![
                order_row("o1", "u1", 150.0, now - 86_400_000),
                order_row("o2", "u2", 40.0, now - 2 * 86_400_000),
            ],
        );
        s.edges.push(Edge {
            rel: "wikilink".into(),
            from: wal.into(),
            to: overview.into(),
        });
        s.edges.push(Edge {
            rel: "tagged".into(),
            from: overview.into(),
            to: facts.into(),
        });
        s.next_id = 10;
        s.rebuild_row_maps();
        s.refresh_col_counts();
        s
    }

    pub fn alloc_id(&mut self) -> String {
        let id = format!("id-{}", self.next_id);
        self.next_id += 1;
        id
    }

    /// Facts row count (columnar or materialized).
    pub fn facts_len(&self) -> usize {
        self.facts_s
            .len()
            .max(self.collections.get("facts").map(|c| c.len()).unwrap_or(0))
    }

    /// Row count including columnar facts not yet materialized into `collections`.
    pub fn row_count(&self) -> usize {
        let mut n = 0usize;
        for (name, rows) in &self.collections {
            if name == "facts" && self.facts_rows_stale {
                n += self.facts_s.len();
            } else if rows.is_empty() {
                if let Some(d) = &self.disk
                    && let Some(c) = d.tables.get(name)
                {
                    n += c.row_count;
                }
            } else {
                n += rows.len();
            }
        }
        n
    }

    /// Materialize columnar facts into `collections["facts"]` (no-op if fresh).
    pub fn ensure_facts_rows(&mut self) {
        if !self.facts_rows_stale {
            return;
        }
        let n = self.facts_s.len();
        let facts = self.collections.entry("facts".into()).or_default();
        facts.clear();
        facts.reserve(n);
        for i in 0..n {
            let mut row = BTreeMap::new();
            row.insert("s".into(), Cell::Text(Arc::clone(&self.facts_s[i])));
            row.insert("p".into(), Cell::Text(Arc::clone(&self.facts_p[i])));
            row.insert("o".into(), Cell::Text(Arc::clone(&self.facts_o[i])));
            facts.push(row);
        }
        self.facts_rows_stale = false;
    }

    /// True when rows live in `collections` (not disk-only), so position-keyed
    /// journals can be replayed against it.
    pub fn has_collection(&self, name: &str) -> bool {
        self.collections.contains_key(name) && !self.is_disk_backed(name)
    }

    pub fn collection(&self, name: &str) -> &[Row] {
        if let Some(d) = &self.disk
            && let Some(lazy) = d.tables.get(name)
        {
            return Self::lazy_rows(d, lazy, name).rows.as_slice();
        }
        self.collections
            .get(name)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// Cache-friendly row count lookup (O(1) via FxHashMap / DiskColLazy, avoids materializing disk tables)
    #[inline]
    pub fn get_col_count(&self, collection: &str) -> usize {
        if collection == "facts" && self.facts_rows_stale {
            return self.facts_s.len();
        }
        if let Some(d) = &self.disk
            && let Some(lazy) = d.tables.get(collection)
        {
            return lazy.row_count;
        }
        if let Some(&c) = self.col_counts.get(collection) {
            return c;
        }
        self.collections.get(collection).map_or(0, Vec::len)
    }

    /// Update cached count after single-row insert/delete
    #[inline]
    pub(crate) fn update_col_count(&mut self, collection: &str, delta: isize) {
        if let Some(count) = self.col_counts.get_mut(collection) {
            if let Some(new_count) = count.checked_add_signed(delta) {
                *count = new_count;
            }
        } else {
            let current = self.collections.get(collection).map_or(0, Vec::len);
            if let Some(new_count) = current.checked_add_signed(delta) {
                self.col_counts.insert(collection.to_string(), new_count);
            }
        }
    }

    /// Rebuild counts from scratch (used after bulk operations or snapshot restore)
    pub fn refresh_col_counts(&mut self) {
        self.col_counts.clear();
        for name in self.collections.keys() {
            let len = if name == "facts" && self.facts_rows_stale {
                self.facts_s.len()
            } else if let Some(d) = &self.disk
                && let Some(c) = d.tables.get(name)
            {
                c.row_count
            } else {
                self.collections.get(name).map(|v| v.len()).unwrap_or(0)
            };
            self.col_counts.insert(name.clone(), len);
        }
    }

    pub fn collection_mut(&mut self, name: &str) -> &mut Vec<Row> {
        if name == "facts" {
            self.ensure_facts_rows();
        }
        self.promote_disk_col(name);
        // Keep the existing-key path allocation-free without an early returned
        // mutable borrow that older borrow checkers extend into the entry path.
        if self.collections.contains_key(name) {
            self.collections.get_mut(name).expect("existing collection")
        } else {
            self.collections.entry(name.to_string()).or_default()
        }
    }

    pub fn find_doc_key(&self, key: &str) -> Option<&Row> {
        self.get_by_id("docs", key).or_else(|| self.get_by_uri(key))
    }

    fn lazy_rows<'a>(d: &'a StoreDiskTier, lazy: &'a DiskColLazy, name: &str) -> &'a LazyRows {
        lazy.materialized.get_or_init(|| {
            let mut rows = Vec::with_capacity(lazy.row_count);
            if let Ok(eng) = d.engine.read() {
                let _ = eng.scan_projected(name, None, |r| {
                    rows.push(r.clone());
                    Ok(true)
                });
            }
            LazyRows {
                rows,
                by_id: OnceLock::new(),
            }
        })
    }

    /// Position of the row with this `id` inside the lazily materialized rows of a
    /// disk-backed collection. Valid until the collection's disk state changes.
    fn disk_row_index(&self, collection: &str, id: &str) -> Option<usize> {
        let d = self.disk.as_ref()?;
        let lazy = d.tables.get(collection)?;
        Self::lazy_rows(d, lazy, collection).index_of(id)
    }

    pub fn get_by_id(&self, collection: &str, id: &str) -> Option<&Row> {
        let idx = self.row_index(collection, id)?;
        self.get_by_idx(collection, idx)
    }

    fn doc_uri_index(&self, uri: &str) -> Option<usize> {
        if self.is_disk_backed("docs") {
            return self
                .collection("docs")
                .iter()
                .position(|r| row_text(r, "uri") == Some(uri));
        }
        if let Some(mut idx) = self.docs_by_uri.get(uri).copied() {
            for &(from, to) in &self.docs_moved_idxs {
                if idx == to {
                    return None;
                } else if idx == from {
                    idx = to;
                }
            }
            return Some(idx);
        }
        if self.docs_by_uri.is_empty() && !self.docs_id.is_empty() {
            if let Some(rows) = self.collections.get("docs") {
                for (i, r) in rows.iter().enumerate() {
                    if row_text(r, "uri") == Some(uri) {
                        return Some(i);
                    }
                }
            }
        }
        None
    }

    pub fn get_by_uri(&self, uri: &str) -> Option<&Row> {
        let idx = self.doc_uri_index(uri)?;
        self.get_by_idx("docs", idx)
    }

    pub fn row_index(&self, collection: &str, id: &str) -> Option<usize> {
        // A disk-backed collection has none of the in-memory row maps below.
        if self.is_disk_backed(collection) {
            return self.disk_row_index(collection, id);
        }
        if collection == "docs" {
            let hint = self
                .last_doc_id_idx
                .load(std::sync::atomic::Ordering::Relaxed);
            if self.fts_deferred_doc_swaps.is_empty()
                && hint < self.docs_id.len()
                && self.docs_id[hint].as_ref() == id
            {
                return Some(hint);
            }
            for del_id in &self.retired_doc_ids {
                if del_id.as_ref() == id {
                    return None;
                }
            }
            for (moved_id, idx) in self.docs_moved_ids.iter().rev() {
                if moved_id.as_ref() == id {
                    return if *idx < self.docs_id.len() && self.docs_id[*idx].as_ref() == id {
                        self.last_doc_id_idx
                            .store(*idx, std::sync::atomic::Ordering::Relaxed);
                        Some(*idx)
                    } else {
                        None
                    };
                }
            }
            let mut idx = self.by_id.get(collection)?.get(id).copied()?;
            for &(from, to) in &self.docs_moved_idxs {
                if idx == to {
                    return None;
                } else if idx == from {
                    idx = to;
                }
            }
            if self.fts_deferred_doc_swaps.is_empty()
                && (!self.docs_moved_ids.is_empty()
                    || !self.docs_moved_idxs.is_empty()
                    || !self.retired_doc_ids.is_empty())
                && (idx >= self.docs_id.len() || self.docs_id[idx].as_ref() != id)
            {
                return None;
            }
            let docs_len = self
                .collections
                .get("docs")
                .map_or(0, Vec::len)
                .max(self.docs_id.len());
            if idx >= docs_len {
                return None;
            }
            self.last_doc_id_idx
                .store(idx, std::sync::atomic::Ordering::Relaxed);
            Some(idx)
        } else {
            self.by_id.get(collection)?.get(id).copied()
        }
    }

    /// Position of a row addressed by an `id` / `uri` point key, when the row
    /// maps cover that collection and field.
    pub fn point_index(&self, collection: &str, field: &str, key: &str) -> Option<usize> {
        match field {
            "id" => self.row_index(collection, key),
            "uri" if collection == "docs" => {
                let idx = self.doc_uri_index(key)?;
                self.get_by_idx("docs", idx)?;
                Some(idx)
            }
            _ => None,
        }
    }

    pub fn get_by_idx(&self, collection: &str, idx: usize) -> Option<&Row> {
        self.collection(collection).get(idx)
    }

    pub fn rows_by_idxs(&self, collection: &str, idxs: &[usize]) -> Vec<Row> {
        let rows = self.collection(collection);
        let mut out = Vec::with_capacity(idxs.len());
        for &i in idxs {
            if let Some(row) = rows.get(i) {
                out.push(row.clone());
            }
        }
        out
    }

    pub fn rows_by_ids(&self, collection: &str, ids: &[String]) -> Vec<Row> {
        let mut out = Vec::with_capacity(ids.len());
        for id in ids {
            if let Some(row) = self.get_by_id(collection, id) {
                out.push(row.clone());
            }
        }
        out
    }

    pub fn project_by_id(&self, collection: &str, id: &str, fields: &[String]) -> Option<Row> {
        if self.is_disk_backed(collection) {
            return self.project_by_key(collection, "id", id, Some(fields));
        }
        let row = self.get_by_id(collection, id)?;
        Some(project_fields(row, fields))
    }

    pub fn project_by_key(
        &self,
        collection: &str,
        field: &str,
        key: &str,
        fields: Option<&[String]>,
    ) -> Option<Row> {
        // A disk-backed collection has no in-memory row maps, whether or not some reader has
        // lazily materialized its rows: point lookups always go to the engine.
        if let Some(d) = &self.disk
            && d.tables.contains_key(collection)
        {
            let eng = d.engine.read().ok()?;
            return match field {
                "id" => match fields {
                    Some(fs) if !fs.is_empty() => {
                        let refs: smallvec::SmallVec<[&str; 8]> =
                            fs.iter().map(String::as_str).collect();
                        eng.get_projected_by_id(collection, key, &refs)
                            .ok()
                            .flatten()
                    }
                    _ => eng.get_by_id(collection, key).ok().flatten(),
                },
                "uri" if collection == "docs" => {
                    let refs: Option<smallvec::SmallVec<[&str; 8]>> = fields
                        .filter(|fs| !fs.is_empty())
                        .map(|fs| fs.iter().map(String::as_str).collect());
                    let mut found = None;
                    let _ = eng.for_each_by_eq_projected(
                        "docs",
                        "uri",
                        &Cell::from_text(key),
                        refs.as_deref(),
                        |r| {
                            found = Some(r.clone());
                            Ok(false)
                        },
                    );
                    found
                }
                _ => None,
            };
        }
        // docs hot project: columnar Arc clones, skip full BTreeMap row.
        if collection == "docs"
            && self.fts_deferred_doc_swaps.is_empty()
            && let Some(fs) = fields
            && !fs.is_empty()
            && fs.iter().all(|f| docs_hot_field(f))
        {
            let idx = match field {
                "id" => self.row_index("docs", key)?,
                "uri" => self.doc_uri_index(key)?,
                _ => return None,
            };
            return self.project_docs_hot_idx(idx, fs);
        }
        let row = match field {
            "id" => self.get_by_id(collection, key)?,
            "uri" if collection == "docs" => self.get_by_uri(key)?,
            _ => return None,
        };
        Some(match fields {
            Some(fs) if !fs.is_empty() => project_fields(row, fs),
            _ => row.clone(),
        })
    }

    #[inline]
    pub fn point_get_docs_id_title(&self, id: &str) -> Option<Row> {
        if let Some(d) = &self.disk
            && d.tables.contains_key("docs")
        {
            let eng = d.engine.read().ok()?;
            return eng
                .get_projected_by_id("docs", id, &["id", "title"])
                .ok()
                .flatten();
        }
        let idx = self.row_index("docs", id)?;
        if self.fts_deferred_doc_swaps.is_empty() && idx < self.docs_id.len() {
            Some(row_id_title(&self.docs_id[idx], &self.docs_title[idx]))
        } else {
            let row = self.get_by_idx("docs", idx)?;
            Some(project_fields(
                row,
                &[String::from("id"), String::from("title")],
            ))
        }
    }

    /// Single-row hot projection from parallel docs columns.
    pub fn project_docs_hot_idx(&self, i: usize, fields: &[String]) -> Option<Row> {
        if i >= self.docs_id.len() {
            return None;
        }
        if fields.len() == 2 && fields[0] == "id" && fields[1] == "title" {
            return Some(row_id_title(&self.docs_id[i], &self.docs_title[i]));
        }
        let mut row = BTreeMap::new();
        for f in fields {
            let cell = match f.as_str() {
                "id" => Cell::Text(Arc::clone(&self.docs_id[i])),
                "title" => Cell::Text(Arc::clone(&self.docs_title[i])),
                "layer" => Cell::Text(Arc::clone(&self.docs_layer[i])),
                "wing" => Cell::Text(Arc::clone(&self.docs_wing[i])),
                _ => return None,
            };
            row.insert(f.clone(), cell);
        }
        Some(row)
    }

    pub fn rebuild_row_maps(&mut self) {
        self.ensure_facts_rows();
        self.by_id.retain(|k, _| self.collections.contains_key(k));
        self.docs_by_uri.clear();
        self.facts_by_spo.clear();
        self.edge_keys.clear();
        let names: smallvec::SmallVec<[String; 4]> = self.collections.keys().cloned().collect();
        for name in &names {
            self.rebuild_row_maps_collection(name);
        }
        self.rebuild_edge_keys();
        // Keep columnar facts aligned with materialized rows.
        self.facts_s.clear();
        self.facts_p.clear();
        self.facts_o.clear();
        if let Some(facts) = self.collections.get("facts")
            && !facts.is_empty()
        {
            self.facts_s.reserve(facts.len());
            self.facts_p.reserve(facts.len());
            self.facts_o.reserve(facts.len());
            for row in facts {
                let (s, p, o) =
                    spo_key(row).unwrap_or_else(|| (Arc::from(""), Arc::from(""), Arc::from("")));
                self.facts_s.push(s);
                self.facts_p.push(p);
                self.facts_o.push(o);
            }
        }
        self.facts_rows_stale = false;
        self.refresh_col_counts();
    }

    /// Physical positions in `collection` whose row key matches any of `rows`.
    /// Resolves each target through the id/uri maps for an O(D) path, mirroring
    /// the Update write; falls back to a full scan only when a row's key can't be
    /// resolved through the maps (e.g. `facts` triples, or id-less rows).
    fn dead_positions(&self, collection: &str, rows: &[Row]) -> Vec<usize> {
        let n = self.collection(collection).len();
        let mut out: Vec<usize> = Vec::with_capacity(rows.len());
        let mut seen = rustc_hash::FxHashSet::default();
        for d in rows {
            let pos = row_text(d, "id")
                .and_then(|id| self.point_index(collection, "id", id))
                .or_else(|| {
                    row_text(d, "uri").and_then(|u| self.point_index(collection, "uri", u))
                });
            match pos {
                Some(p) if p < n && seen.insert(p) => out.push(p),
                Some(_) => {}
                None => {
                    for (i, r) in self.collection(collection).iter().enumerate() {
                        if same_row_key(collection, r, d) && seen.insert(i) {
                            out.push(i);
                        }
                    }
                }
            }
        }
        out
    }

    /// Swap-remove rows and update only the removed/moved index, FTS and map slots.
    /// Returns the removed rows in ascending order of their original requested positions.
    pub fn delete_rows_at(&mut self, collection: &str, dead: &[usize]) -> Vec<Row> {
        if collection == "docs" {
            self.flush_fts_deferred_docs();
        }
        let old_n = match self.collections.get(collection) {
            Some(rows) => rows.len(),
            None => return Vec::new(),
        };
        if old_n == 0 || dead.is_empty() {
            return Vec::new();
        }

        if dead.len() == 1 {
            let p = dead[0];
            if p >= old_n {
                return Vec::new();
            }
            self.update_col_count(collection, -1);
            let doomed = self.collections.get_mut(collection).unwrap().swap_remove(p);
            let last = old_n - 1;
            let has_hnsw = !self.hnsw.is_empty();
            if p != last {
                self.index_swap_remove_rows(collection, p, &doomed, last);
                if has_hnsw && let Some(hnsw) = self.hnsw.get_mut(collection) {
                    hnsw.remove(p);
                    hnsw.move_id(last, p);
                }
                self.move_deleted_tail(collection, last, p, Some(&doomed));
            } else {
                self.index_remove_row(collection, p, &doomed);
                if has_hnsw && let Some(hnsw) = self.hnsw.get_mut(collection) {
                    hnsw.remove(p);
                }
                self.fts_remove_row(collection, p, &doomed);
                self.row_maps_drop_keys(collection, &doomed);
                self.swap_pop_columns(collection, p, last + 1);
            }
            let new_n = old_n - 1;
            let soa_len = match collection {
                "docs" => self.docs_id.len(),
                "orders" => self.orders_id.len(),
                "users" => self.users_id.len(),
                _ => new_n,
            };
            if soa_len != new_n {
                self.rebuild_row_maps_collection(collection);
            }
            return vec![doomed];
        }

        let mut dead: Vec<usize> = dead.iter().copied().filter(|&d| d < old_n).collect();
        dead.sort_unstable();
        dead.dedup();
        if dead.is_empty() {
            return Vec::new();
        }

        // Cache-friendly count update (decrement by number of deleted rows)
        self.update_col_count(collection, -(dead.len() as isize));

        // Swap-remove, highest slot first: the last live row moves into each vacated
        // slot, so the array stays dense and only the doomed row's and the moved
        // row's index / FTS / map entries change. Positions are derived (maps,
        // postings and columnar arrays are rebuilt from the rows on restore), so row
        // order is not a contract — `sort` is the explicit remedy when one is needed.

        let has_hnsw = !self.hnsw.is_empty();
        let mut removed = Vec::with_capacity(dead.len());
        for &p in dead.iter().rev() {
            let last = match self.collections.get(collection) {
                Some(rows) if p < rows.len() => rows.len() - 1,
                _ => break,
            };

            let doomed = self.collections.get_mut(collection).unwrap().swap_remove(p);

            if p != last {
                self.index_swap_remove_rows(collection, p, &doomed, last);
                if has_hnsw && let Some(hnsw) = self.hnsw.get_mut(collection) {
                    hnsw.remove(p);
                    hnsw.move_id(last, p);
                }
                self.move_deleted_tail(collection, last, p, Some(&doomed));
            } else {
                self.index_remove_row(collection, p, &doomed);
                if has_hnsw && let Some(hnsw) = self.hnsw.get_mut(collection) {
                    hnsw.remove(p);
                }
                self.fts_remove_row(collection, p, &doomed);
                self.row_maps_drop_keys(collection, &doomed);
                self.swap_pop_columns(collection, p, last + 1);
            }
            removed.push(doomed);
        }

        let new_n = old_n - dead.len();
        let soa_len = match collection {
            "docs" => self.docs_id.len(),
            "orders" => self.orders_id.len(),
            "users" => self.users_id.len(),
            _ => new_n,
        };
        if soa_len != new_n {
            self.rebuild_row_maps_collection(collection);
        }
        removed.reverse();
        removed
    }

    pub fn flush_fts_deferred_docs(&mut self) {
        if self.fts_deferred_doc_swaps.is_empty() {
            return;
        }
        let swaps = std::mem::take(&mut self.fts_deferred_doc_swaps);
        for (p, r_idx, last) in swaps {
            let old_n = last + 1;
            if let Some(doomed) = self.retired_rows.get(r_idx).cloned() {
                if p != last {
                    self.index_swap_remove_rows("docs", p, &doomed, last);
                    if let Some(fts) = self.fts.get_mut("docs")
                        && let Some(docs) = self.collections.get("docs")
                        && let Some(tail) = docs.get(p)
                    {
                        fts.swap_remove_row(p, &doomed, last, tail);
                    }
                    if !self.docs_by_uri.is_empty() {
                        if let Some(dead_uri) = row_text(&doomed, "uri") {
                            self.docs_by_uri.remove(dead_uri);
                        }
                        if let Some(docs) = self.collections.get("docs")
                            && let Some(tail) = docs.get(p)
                            && let Some(u_s) = row_text(tail, "uri")
                        {
                            if let Some(slot) = self.docs_by_uri.get_mut(u_s) {
                                *slot = p;
                            } else if let Some(uri) = tail.get("uri").and_then(Cell::text_shared) {
                                self.docs_by_uri.insert(uri, p);
                            }
                        }
                    }
                } else {
                    self.index_remove_row("docs", p, &doomed);
                    if let Some(fts) = self.fts.get_mut("docs") {
                        fts.remove_row(p, &doomed);
                    }
                    if !self.docs_by_uri.is_empty()
                        && let Some(dead_uri) = row_text(&doomed, "uri")
                    {
                        self.docs_by_uri.remove(dead_uri);
                    }
                }
            }
            if self.docs_id.len() == old_n && p < old_n {
                self.retired_doc_ids.push(self.docs_id.swap_remove(p));
            }
            if self.docs_title.len() == old_n && p < old_n {
                self.retired_arcs.push(self.docs_title.swap_remove(p));
            }
            self.docs_layer.swap_pop_at(p, old_n);
            self.docs_wing.swap_pop_at(p, old_n);
        }
    }

    pub fn point_delete_docs_at_in_memory(
        &mut self,
        p: usize,
        _precomputed_idx_keys: Option<&(crate::index::IndexKey, crate::index::IndexKey)>,
    ) {
        if !self.fts_deferred_doc_swaps.is_empty() {
            self.flush_fts_deferred_docs();
        }
        let old_n = match self.collections.get("docs") {
            Some(rows) => rows.len(),
            None => return,
        };
        if p >= old_n {
            return;
        }
        let last = old_n - 1;
        if p != last
            && self.hnsw.is_empty()
            && self.docs_id.len() == old_n
            && self.docs_title.len() == old_n
            && self.retired_rows.len() < self.retired_rows.inline_size()
            && self.fts_deferred_doc_swaps.is_empty()
            && self.docs_moved_idxs.len() < self.docs_moved_idxs.inline_size()
            && self.retired_doc_ids.len() < self.retired_doc_ids.inline_size()
            && self.retired_arcs.len() < self.retired_arcs.inline_size()
        {
            self.update_col_count("docs", -1);
            let doomed = self.collections.get_mut("docs").unwrap().swap_remove(p);
            let r_idx = self.retired_rows.len();
            self.retired_rows.push(doomed);
            self.fts_deferred_doc_swaps.push((p, r_idx, last));
            let hint = self
                .last_doc_id_idx
                .load(std::sync::atomic::Ordering::Relaxed);
            if hint == last {
                self.last_doc_id_idx
                    .store(p, std::sync::atomic::Ordering::Relaxed);
            } else if hint == p {
                self.last_doc_id_idx
                    .store(usize::MAX, std::sync::atomic::Ordering::Relaxed);
            }
            self.docs_moved_idxs.push((last, p));
            self.invalidate_docs_cache();
            return;
        }
        self.flush_fts_deferred_docs();
        self.update_col_count("docs", -1);
        let doomed = self.collections.get_mut("docs").unwrap().swap_remove(p);
        let has_hnsw = !self.hnsw.is_empty();
        if p != last {
            self.index_swap_remove_rows("docs", p, &doomed, last);
            if has_hnsw && let Some(hnsw) = self.hnsw.get_mut("docs") {
                hnsw.remove(p);
                hnsw.move_id(last, p);
            }
            if !self.fts.is_empty()
                && let Some(fts) = self.fts.get_mut("docs")
            {
                let row = &self.collections["docs"][p];
                fts.swap_remove_row(p, &doomed, last, row);
            }
            let hint = self
                .last_doc_id_idx
                .load(std::sync::atomic::Ordering::Relaxed);
            if hint == last {
                self.last_doc_id_idx
                    .store(p, std::sync::atomic::Ordering::Relaxed);
            } else if hint == p {
                self.last_doc_id_idx
                    .store(usize::MAX, std::sync::atomic::Ordering::Relaxed);
            }
            if p < self.docs_id.len() && last < self.docs_id.len() && self.docs_moved_ids.len() < 16
            {
                self.docs_moved_ids
                    .push((Arc::clone(&self.docs_id[last]), p));
            } else {
                let row = &self.collections["docs"][p];
                let (dead_id_fast, tail_id_fast) =
                    if p < self.docs_id.len() && last < self.docs_id.len() {
                        (Some(&self.docs_id[p]), Some(&self.docs_id[last]))
                    } else {
                        (None, None)
                    };
                if let Some(map) = self.by_id.get_mut("docs") {
                    if let Some(dead_id) = dead_id_fast {
                        map.remove(dead_id.as_ref());
                    } else if let Some(dead_id) = row_text(&doomed, "id") {
                        map.remove(dead_id);
                    }
                    if let Some(tail_id) = tail_id_fast {
                        if let Some(slot) = map.get_mut(tail_id.as_ref()) {
                            *slot = p;
                        } else {
                            map.insert(Arc::clone(tail_id), p);
                        }
                    } else if let Some(id_s) = row_text(row, "id") {
                        if let Some(slot) = map.get_mut(id_s) {
                            *slot = p;
                        } else if let Some(id) = row.get("id").and_then(Cell::text_shared) {
                            map.insert(id, p);
                        }
                    }
                }
            }
            if !self.docs_by_uri.is_empty() {
                if let Some(dead_uri) = row_text(&doomed, "uri") {
                    self.docs_by_uri.remove(dead_uri);
                }
                let row = &self.collections["docs"][p];
                if let Some(u_s) = row_text(row, "uri") {
                    if let Some(slot) = self.docs_by_uri.get_mut(u_s) {
                        *slot = p;
                    } else if let Some(uri) = row.get("uri").and_then(Cell::text_shared) {
                        self.docs_by_uri.insert(uri, p);
                    }
                }
            }
            self.swap_pop_columns("docs", p, last + 1);
        } else {
            self.index_remove_row("docs", p, &doomed);
            if has_hnsw && let Some(hnsw) = self.hnsw.get_mut("docs") {
                hnsw.remove(p);
            }
            self.fts_remove_row("docs", p, &doomed);
            self.last_doc_id_idx
                .store(usize::MAX, std::sync::atomic::Ordering::Relaxed);
            if p < self.docs_id.len() && self.docs_moved_ids.len() < 16 {
                self.docs_moved_ids
                    .push((Arc::clone(&self.docs_id[p]), usize::MAX));
            } else {
                self.row_maps_drop_keys("docs", &doomed);
            }
            self.swap_pop_columns("docs", p, last + 1);
        }
        let new_n = old_n - 1;
        if self.docs_id.len() != new_n {
            self.rebuild_row_maps_collection("docs");
        }
        if self.retired_rows.len() < self.retired_rows.inline_size() {
            self.retired_rows.push(doomed);
        }
    }

    /// The collection already contains the moved row at `to`. Read it in place
    /// while editing disjoint derived indexes; retain only identity keys for
    /// the column/map step, avoiding a complete Row clone.
    fn move_deleted_tail(
        &mut self,
        collection: &str,
        from: usize,
        to: usize,
        doomed: Option<&Row>,
    ) {
        let row = &self.collections[collection][to];
        if !self.fts.is_empty() {
            if let Some(fts) = self.fts.get_mut(collection) {
                if let Some(doomed) = doomed {
                    fts.swap_remove_row(to, doomed, from, row);
                } else {
                    fts.move_row(from, to, row);
                }
            }
        }
        let is_docs = collection == "docs";
        if is_docs {
            let hint = self
                .last_doc_id_idx
                .load(std::sync::atomic::Ordering::Relaxed);
            if hint == from {
                self.last_doc_id_idx
                    .store(to, std::sync::atomic::Ordering::Relaxed);
            } else if hint == to {
                self.last_doc_id_idx
                    .store(usize::MAX, std::sync::atomic::Ordering::Relaxed);
            }
        }
        let (dead_id_fast, tail_id_fast) =
            if is_docs && to < self.docs_id.len() && from < self.docs_id.len() {
                (Some(&self.docs_id[to]), Some(&self.docs_id[from]))
            } else {
                (None, None)
            };
        let id_str = if tail_id_fast.is_none() {
            row_text(row, "id")
        } else {
            None
        };
        let has_uri_map = is_docs && !self.docs_by_uri.is_empty();
        let uri_str = if has_uri_map {
            row_text(row, "uri")
        } else {
            None
        };
        let spo = (collection == "facts").then(|| spo_key(row)).flatten();
        // Keep the original remove/register semantics for duplicate identities.
        if let Some(map) = self.by_id.get_mut(collection) {
            if let Some(dead_id) = dead_id_fast {
                map.remove(dead_id.as_ref());
            } else if let Some(doomed) = doomed
                && let Some(dead_id) = row_text(doomed, "id")
            {
                map.remove(dead_id);
            }
            if let Some(tail_id) = tail_id_fast {
                if let Some(slot) = map.get_mut(tail_id.as_ref()) {
                    *slot = to;
                } else {
                    map.insert(Arc::clone(tail_id), to);
                }
            } else if let Some(id_s) = id_str {
                if let Some(slot) = map.get_mut(id_s) {
                    *slot = to;
                } else if let Some(id) = row.get("id").and_then(Cell::text_shared) {
                    map.insert(id, to);
                }
            }
        }
        if has_uri_map {
            if let Some(doomed) = doomed
                && let Some(dead_uri) = row_text(doomed, "uri")
            {
                self.docs_by_uri.remove(dead_uri);
            }
            if let Some(u_s) = uri_str {
                if let Some(slot) = self.docs_by_uri.get_mut(u_s) {
                    *slot = to;
                } else if let Some(uri) = row.get("uri").and_then(Cell::text_shared) {
                    self.docs_by_uri.insert(uri, to);
                }
            }
        }
        if collection == "facts" {
            if let Some(doomed) = doomed
                && let Some(dead_spo) = spo_key(doomed)
            {
                self.facts_by_spo.remove(&dead_spo);
            }
            if let Some(spo) = spo {
                self.facts_by_spo.insert(spo, to);
            }
        }
        // These arrays are known parallel; swap-pop already transfers every
        // mirrored value, so registering the row again would duplicate the work.
        self.swap_pop_columns(collection, to, from + 1);
    }

    #[inline]
    fn invalidate_docs_cache(&mut self) {
        self.docs_title_packed.take();
        self.docs_epoch = next_docs_epoch();
    }

    /// Mirror [`swap_pop_at`] across the collection's columnar arrays, skipping
    /// columns that aren't parallel to the rows (the caller's length guard rebuilds).
    fn swap_pop_columns(&mut self, collection: &str, p: usize, len: usize) {
        if collection == "docs" {
            self.invalidate_docs_cache();
        }
        match collection {
            "docs" => {
                swap_pop_at(&mut self.docs_id, p, len);
                swap_pop_at(&mut self.docs_title, p, len);
                self.docs_layer.swap_pop_at(p, len);
                self.docs_wing.swap_pop_at(p, len);
            }
            "orders" => {
                swap_pop_at(&mut self.orders_id, p, len);
                swap_pop_at(&mut self.orders_user_id, p, len);
                swap_pop_at(&mut self.orders_total, p, len);
            }
            "users" => {
                swap_pop_at(&mut self.users_id, p, len);
                swap_pop_at(&mut self.users_email, p, len);
            }
            "facts" => {
                swap_pop_at(&mut self.facts_s, p, len);
                swap_pop_at(&mut self.facts_p, p, len);
                swap_pop_at(&mut self.facts_o, p, len);
            }
            _ => {}
        }
    }

    /// Drop a row's `key → position` entries after its slot is vacated.
    fn row_maps_drop_keys(&mut self, collection: &str, row: &Row) {
        if collection == "docs" {
            self.last_doc_id_idx
                .store(usize::MAX, std::sync::atomic::Ordering::Relaxed);
        }
        if let Some(id) = row_text(row, "id")
            && let Some(map) = self.by_id.get_mut(collection)
        {
            map.remove(id);
        }
        if collection == "docs"
            && !self.docs_by_uri.is_empty()
            && let Some(uri) = row_text(row, "uri")
        {
            self.docs_by_uri.remove(uri);
        }
        if collection == "facts"
            && let Some(spo) = spo_key(row)
        {
            self.facts_by_spo.remove(&spo);
        }
    }

    /// [`Store::row_maps_register_row`] plus the `facts` key that helper ignores.
    fn swap_maps_register(&mut self, collection: &str, idx: usize, row: &Row) {
        // Inline: facts By_SPO lookup is only needed for triples
        if collection == "facts"
            && let Some(spo) = spo_key(row)
        {
            self.facts_by_spo.insert(spo, idx);
        } else {
            self.row_maps_register_row(collection, idx, row);
        }
    }

    /// Reverse in-place row mutations from a `Rows` undo journal. Positions never
    /// shift during an update, so each entry restores one row plus its own index /
    /// FTS / map slots — no whole-store clone, no per-collection rebuild.
    pub fn restore_rows(&mut self, journal: &[(String, usize, Row)]) {
        for (collection, idx, row) in journal.iter().rev() {
            let live = self
                .collections
                .get(collection)
                .is_some_and(|rows| *idx < rows.len());
            if !live {
                continue;
            }
            let old_row = std::mem::replace(
                &mut self.collections.get_mut(collection).unwrap()[*idx],
                row.clone(),
            );
            if let Some(fts) = self.fts.get_mut(collection) {
                fts.sync_row(*idx, &old_row, row);
            }
            self.row_maps_sync_updated(collection, *idx, &old_row, row);
            let _ = self.index_update_row(collection, *idx, &old_row, row);
        }
    }

    pub fn restore_row_owned(&mut self, collection: &str, idx: usize, row: Row) {
        let live = self
            .collections
            .get(collection)
            .is_some_and(|rows| idx < rows.len());
        if !live {
            return;
        }
        let old_row = std::mem::replace(
            &mut self.collections.get_mut(collection).unwrap()[idx],
            Row::new(),
        );
        if let Some(fts) = self.fts.get_mut(collection) {
            fts.sync_row(idx, &old_row, &row);
        }
        self.row_maps_sync_updated(collection, idx, &old_row, &row);
        let _ = self.index_update_row(collection, idx, &old_row, &row);
        self.collections.get_mut(collection).unwrap()[idx] = row;
    }

    /// Inverse of [`Self::delete_rows_at`]: drop each removed row back into its old
    /// slot and push whatever the swap parked there onto the tail. Walking `positions`
    /// ascending reproduces the forward pass's intermediate lengths exactly, so
    /// `p == rows.len()` marks the slots that were the tail and need no push.
    pub fn restore_rows_at(&mut self, collection: &str, positions: &[usize], rows: &[Row]) {
        for (&p, doomed) in positions.iter().zip(rows) {
            let Some(current) = self.collections.get(collection) else {
                return;
            };
            let len = current.len();
            if p > len {
                continue;
            }
            if p < len {
                let moved = self.collections[collection][p].clone();
                self.index_remove_row(collection, p, &moved);
                self.fts_move_row(collection, p, len, &moved);
                self.row_maps_drop_keys(collection, &moved);
                if let Some(rows) = self.collections.get_mut(collection) {
                    rows.push(moved.clone());
                }
                let _ = self.index_insert_row(collection, len, &moved);
                self.swap_maps_register(collection, len, &moved);
            }
            if let Some(rows) = self.collections.get_mut(collection) {
                if p == rows.len() {
                    rows.push(doomed.clone());
                } else {
                    rows[p] = doomed.clone();
                }
            }
            let _ = self.index_insert_row(collection, p, doomed);
            self.fts_insert_row(collection, p, doomed);
            self.swap_maps_register(collection, p, doomed);
            // The slab's columns follow the rows array: the value at `p` moves to the
            // tail and the restored row takes `p`, which one swap-push expresses.
            if collection == "facts" {
                let get = |f: &str| {
                    doomed
                        .get(f)
                        .and_then(Cell::text_shared)
                        .unwrap_or_default()
                };
                swap_push_at(&mut self.facts_s, p, len, get("s"));
                swap_push_at(&mut self.facts_p, p, len, get("p"));
                swap_push_at(&mut self.facts_o, p, len, get("o"));
            }
        }
        let n = self
            .collections
            .get(collection)
            .map_or(0, std::vec::Vec::len);
        self.col_counts.insert(collection.to_string(), n);
        let soa_len = match collection {
            "docs" => self.docs_id.len(),
            "orders" => self.orders_id.len(),
            "users" => self.users_id.len(),
            "facts" => self.facts_s.len(),
            _ => n,
        };
        if soa_len != n {
            self.rebuild_row_maps_collection(collection);
        }
    }

    pub fn rebuild_row_maps_collection(&mut self, collection: &str) {
        if collection == "docs" {
            self.invalidate_docs_cache();
        }
        self.by_id
            .entry(collection.to_string())
            .or_default()
            .clear();
        if collection == "docs" {
            self.docs_by_uri.clear();
            self.docs_id.clear();
            self.docs_title.clear();
            self.docs_layer.clear();
            self.docs_wing.clear();
            self.docs_moved_ids.clear();
            self.docs_moved_idxs.clear();
            self.retired_doc_ids.clear();
            self.fts_deferred_doc_swaps.clear();
        }
        if collection == "orders" {
            self.orders_id.clear();
            self.orders_user_id.clear();
            self.orders_total.clear();
        }
        if collection == "users" {
            self.users_id.clear();
            self.users_email.clear();
        }
        if collection == "facts" {
            self.facts_by_spo.clear();
        }
        let Some(rows) = self.collections.get(collection) else {
            return;
        };
        let n = rows.len();
        if n == 0 {
            return;
        }

        let by_id_map = self.by_id.get_mut(collection).expect("by_id entry");
        by_id_map.reserve(n);

        if collection == "docs" {
            self.docs_by_uri.reserve(n);
            self.docs_id.reserve(n);
            self.docs_title.reserve(n);
            self.docs_layer.reserve(n);
            self.docs_wing.reserve(n);
            let first = &rows[0];
            let id_pos = first.keys().position(|k| k == "id");
            let uri_pos = first.keys().position(|k| k == "uri");
            let title_pos = first.keys().position(|k| k == "title");
            let layer_pos = first.keys().position(|k| k == "layer");
            let wing_pos = first.keys().position(|k| k == "wing");
            let uniform = rows[1..].iter().all(|r| r.len() == first.len());
            if uniform && first.len() <= 32 {
                for (i, row) in rows.iter().enumerate() {
                    let mut cells: [&Cell; 32] = [&Cell::Null; 32];
                    for (p, val) in row.values().enumerate().take(32) {
                        cells[p] = val;
                    }
                    let id_arc: Arc<str> = id_pos
                        .and_then(|p| cells[p].text_shared())
                        .unwrap_or_default();
                    let uri_arc: Option<Arc<str>> = uri_pos.and_then(|p| cells[p].text_shared());
                    let title_arc: Arc<str> = title_pos
                        .and_then(|p| cells[p].text_shared())
                        .unwrap_or_default();
                    let layer_arc: Arc<str> = layer_pos
                        .and_then(|p| cells[p].text_shared())
                        .unwrap_or_default();
                    let wing_arc: Arc<str> = wing_pos
                        .and_then(|p| cells[p].text_shared())
                        .unwrap_or_default();

                    if !id_arc.is_empty() {
                        by_id_map.insert(Arc::clone(&id_arc), i);
                    }
                    if let Some(uri) = uri_arc {
                        self.docs_by_uri.insert(uri, i);
                    }
                    self.docs_id.push(id_arc);
                    self.docs_title.push(title_arc);
                    self.docs_layer.push(layer_arc);
                    self.docs_wing.push(intern_wing(wing_arc));
                }
                return;
            }
            for (i, row) in rows.iter().enumerate() {
                let mut id = None;
                let mut uri = None;
                let mut title = None;
                let mut layer = None;
                let mut wing = None;
                for (k, v) in row {
                    match k.as_str() {
                        "id" => id = v.text_shared(),
                        "uri" => uri = v.text_shared(),
                        "title" => title = v.text_shared(),
                        "layer" => layer = v.text_shared(),
                        "wing" => wing = v.text_shared(),
                        _ => {}
                    }
                }
                if let Some(ref id_val) = id {
                    by_id_map.insert(Arc::clone(id_val), i);
                }
                if let Some(ref uri_val) = uri {
                    self.docs_by_uri.insert(Arc::clone(uri_val), i);
                }
                self.docs_id.push(id.unwrap_or_default());
                self.docs_title.push(title.unwrap_or_default());
                self.docs_layer.push(layer.unwrap_or_default());
                self.docs_wing.push(intern_wing(wing.unwrap_or_default()));
            }
            return;
        }

        if collection == "orders" {
            self.orders_id.reserve(n);
            self.orders_user_id.reserve(n);
            self.orders_total.reserve(n);
            let first = &rows[0];
            let id_pos = first.keys().position(|k| k == "id");
            let uid_pos = first.keys().position(|k| k == "user_id");
            let total_pos = first.keys().position(|k| k == "total");
            let uniform = rows[1..].iter().all(|r| r.len() == first.len());
            if uniform && first.len() <= 32 {
                for (i, row) in rows.iter().enumerate() {
                    let mut cells: [&Cell; 32] = [&Cell::Null; 32];
                    for (p, val) in row.values().enumerate().take(32) {
                        cells[p] = val;
                    }
                    let id_arc: Arc<str> = id_pos
                        .and_then(|p| cells[p].text_shared())
                        .unwrap_or_default();
                    let uid_arc: Arc<str> = uid_pos
                        .and_then(|p| cells[p].text_shared())
                        .unwrap_or_default();
                    let total = total_pos.and_then(|p| cells[p].as_f64()).unwrap_or(0.0);
                    if !id_arc.is_empty() {
                        by_id_map.insert(Arc::clone(&id_arc), i);
                    }
                    self.orders_id.push(id_arc);
                    self.orders_user_id.push(uid_arc);
                    self.orders_total.push(total);
                }
                return;
            }
            for (i, row) in rows.iter().enumerate() {
                let mut id = None;
                let mut uid = None;
                let mut total = 0.0;
                for (k, v) in row {
                    match k.as_str() {
                        "id" => id = v.text_shared(),
                        "user_id" => uid = v.text_shared(),
                        "total" => total = v.as_f64().unwrap_or(0.0),
                        _ => {}
                    }
                }
                if let Some(ref id_val) = id {
                    by_id_map.insert(Arc::clone(id_val), i);
                }
                self.orders_id.push(id.unwrap_or_default());
                self.orders_user_id.push(uid.unwrap_or_default());
                self.orders_total.push(total);
            }
            return;
        }

        if collection == "users" {
            self.users_id.reserve(n);
            self.users_email.reserve(n);
            let first = &rows[0];
            let id_pos = first.keys().position(|k| k == "id");
            let email_pos = first.keys().position(|k| k == "email");
            let uniform = rows[1..].iter().all(|r| r.len() == first.len());
            if uniform && first.len() <= 32 {
                for (i, row) in rows.iter().enumerate() {
                    let mut cells: [&Cell; 32] = [&Cell::Null; 32];
                    for (p, val) in row.values().enumerate().take(32) {
                        cells[p] = val;
                    }
                    let id_arc: Arc<str> = id_pos
                        .and_then(|p| cells[p].text_shared())
                        .unwrap_or_default();
                    let email_arc: Arc<str> = email_pos
                        .and_then(|p| cells[p].text_shared())
                        .unwrap_or_default();
                    if !id_arc.is_empty() {
                        by_id_map.insert(Arc::clone(&id_arc), i);
                    }
                    self.users_id.push(id_arc);
                    self.users_email.push(email_arc);
                }
                return;
            }
            for (i, row) in rows.iter().enumerate() {
                let mut id = None;
                let mut email = None;
                for (k, v) in row {
                    match k.as_str() {
                        "id" => id = v.text_shared(),
                        "email" => email = v.text_shared(),
                        _ => {}
                    }
                }
                if let Some(ref id_val) = id {
                    by_id_map.insert(Arc::clone(id_val), i);
                }
                self.users_id.push(id.unwrap_or_default());
                self.users_email.push(email.unwrap_or_default());
            }
            return;
        }

        if collection == "facts" {
            self.facts_by_spo.reserve(n);
            for (i, row) in rows.iter().enumerate() {
                let id = row.get("id").and_then(Cell::text_shared);
                if let Some(ref id_val) = id {
                    by_id_map.insert(Arc::clone(id_val), i);
                }
                if let Some(spo) = spo_key(row) {
                    self.facts_by_spo.insert(spo, i);
                }
            }
            return;
        }

        for (i, row) in rows.iter().enumerate() {
            let id = row.get("id").and_then(Cell::text_shared);
            if let Some(ref id_val) = id {
                by_id_map.insert(Arc::clone(id_val), i);
            }
        }
    }

    pub fn row_maps_register(&mut self, collection: &str, idx: usize) {
        let Some(row) = self
            .collections
            .get(collection)
            .and_then(|c| c.get(idx))
            .cloned()
        else {
            return;
        };
        self.row_maps_register_row(collection, idx, &row);
    }

    /// Register maps/columns from an already-built row (avoids re-fetch).
    pub fn row_maps_register_row(&mut self, collection: &str, idx: usize, row: &Row) {
        if collection == "docs" {
            self.invalidate_docs_cache();
        }
        let id = row.get("id").and_then(Cell::text_shared);
        let uri = if collection == "docs" {
            row.get("uri").and_then(Cell::text_shared)
        } else {
            None
        };
        if let Some(ref id) = id {
            if collection == "docs" {
                if !self.docs_moved_ids.is_empty() {
                    self.docs_moved_ids.retain(|(k, _)| k != id);
                }
                if !self.retired_doc_ids.is_empty() {
                    self.retired_doc_ids.retain(|k| k != id);
                }
            }
            if let Some(map) = self.by_id.get_mut(collection) {
                map.insert(Arc::clone(id), idx);
            } else {
                self.by_id
                    .entry(collection.to_string())
                    .or_default()
                    .insert(Arc::clone(id), idx);
            }
        }
        if collection == "docs" {
            let title = row
                .get("title")
                .and_then(Cell::text_shared)
                .unwrap_or_default();
            let layer = row
                .get("layer")
                .and_then(Cell::text_shared)
                .unwrap_or_default();
            let wing = intern_wing(
                row.get("wing")
                    .and_then(Cell::text_shared)
                    .unwrap_or_default(),
            );
            let did = id.clone().unwrap_or_default();
            if let Some(uri) = uri {
                self.docs_by_uri.insert(uri, idx);
            }
            if idx == self.docs_id.len() {
                self.docs_id.push(did);
                self.docs_title.push(title);
                self.docs_layer.push(layer);
                self.docs_wing.push(wing);
            } else if idx < self.docs_id.len() {
                self.docs_id[idx] = did;
                self.docs_title[idx] = title;
                self.docs_layer.set(idx, layer);
                self.docs_wing.set(idx, wing);
            } else {
                self.rebuild_row_maps_collection("docs");
            }
        }
        if collection == "orders" {
            let uid = row
                .get("user_id")
                .and_then(Cell::text_shared)
                .unwrap_or_default();
            let total = row.get("total").and_then(Cell::as_f64).unwrap_or(0.0);
            let did = id.clone().unwrap_or_default();
            if idx == self.orders_id.len() {
                self.orders_id.push(did);
                self.orders_user_id.push(uid);
                self.orders_total.push(total);
            } else if idx < self.orders_id.len() {
                self.orders_id[idx] = did;
                self.orders_user_id[idx] = uid;
                self.orders_total[idx] = total;
            } else {
                self.rebuild_row_maps_collection("orders");
            }
        }
        if collection == "users" {
            let email = row
                .get("email")
                .and_then(Cell::text_shared)
                .unwrap_or_default();
            let did = id.clone().unwrap_or_default();
            if idx == self.users_id.len() {
                self.users_id.push(did);
                self.users_email.push(email);
            } else if idx < self.users_id.len() {
                self.users_id[idx] = did;
                self.users_email[idx] = email;
            } else {
                self.rebuild_row_maps_collection("users");
            }
        }
    }

    /// Append-only slab: register `[start, start+rows.len())` from the given rows.
    pub fn row_maps_register_slab(&mut self, collection: &str, start: usize, rows: &[Row]) {
        if collection == "docs" {
            self.invalidate_docs_cache();
        }
        if rows.is_empty() {
            return;
        }
        self.row_maps_reserve(collection, rows.len());
        if collection == "docs" {
            if self.docs_id.len() != start {
                self.rebuild_row_maps_collection("docs");
                if self.docs_id.len() != start {
                    for (i, row) in rows.iter().enumerate() {
                        self.row_maps_register_row(collection, start + i, row);
                    }
                    return;
                }
            }
            let by_id = if let Some(m) = self.by_id.get_mut(collection) {
                m
            } else {
                self.by_id.entry(collection.to_string()).or_default()
            };
            let first = &rows[0];
            let id_pos = first.keys().position(|k| k == "id");
            let uri_pos = first.keys().position(|k| k == "uri");
            let title_pos = first.keys().position(|k| k == "title");
            let layer_pos = first.keys().position(|k| k == "layer");
            let wing_pos = first.keys().position(|k| k == "wing");
            let uniform = rows[1..].iter().all(|r| r.len() == first.len());
            if uniform && first.len() <= 32 {
                for (i, row) in rows.iter().enumerate() {
                    let idx = start + i;
                    let mut cells: [&Cell; 32] = [&Cell::Null; 32];
                    for (p, val) in row.values().enumerate().take(32) {
                        cells[p] = val;
                    }
                    let id_arc: Arc<str> = id_pos
                        .and_then(|p| cells[p].text_shared())
                        .unwrap_or_default();
                    let uri_arc: Option<Arc<str>> = uri_pos.and_then(|p| cells[p].text_shared());
                    let title_arc: Arc<str> = title_pos
                        .and_then(|p| cells[p].text_shared())
                        .unwrap_or_default();
                    let layer_arc: Arc<str> = layer_pos
                        .and_then(|p| cells[p].text_shared())
                        .unwrap_or_default();
                    let wing_arc: Arc<str> = wing_pos
                        .and_then(|p| cells[p].text_shared())
                        .unwrap_or_default();

                    if !id_arc.is_empty() {
                        by_id.insert(Arc::clone(&id_arc), idx);
                    }
                    if let Some(uri) = uri_arc {
                        self.docs_by_uri.insert(uri, idx);
                    }
                    self.docs_id.push(id_arc);
                    self.docs_title.push(title_arc);
                    self.docs_layer.push(layer_arc);
                    self.docs_wing.push(intern_wing(wing_arc));
                }
                return;
            }
            for (i, row) in rows.iter().enumerate() {
                let idx = start + i;
                let mut texts: [Option<Arc<str>>; 5] = std::array::from_fn(|_| None);
                for (field, value) in row {
                    let slot = match field.as_str() {
                        "id" => 0,
                        "uri" => 1,
                        "title" => 2,
                        "layer" => 3,
                        "wing" => 4,
                        _ => continue,
                    };
                    texts[slot] = value.text_shared();
                }
                let [id, uri, title, layer, wing] = texts;
                if let Some(ref id) = id {
                    by_id.insert(Arc::clone(id), idx);
                }
                if let Some(uri) = uri {
                    self.docs_by_uri.insert(uri, idx);
                }
                self.docs_id.push(id.unwrap_or_default());
                self.docs_title.push(title.unwrap_or_default());
                self.docs_layer.push(layer.unwrap_or_default());
                self.docs_wing.push(intern_wing(wing.unwrap_or_default()));
            }
            return;
        }
        if collection == "orders" && self.orders_id.len() == start {
            let by_id = self.by_id.entry(collection.to_string()).or_default();
            for (i, row) in rows.iter().enumerate() {
                let idx = start + i;
                let id = row
                    .get("id")
                    .and_then(Cell::text_shared)
                    .unwrap_or_default();
                let uid = row
                    .get("user_id")
                    .and_then(Cell::text_shared)
                    .unwrap_or_default();
                let total = row.get("total").and_then(Cell::as_f64).unwrap_or(0.0);
                if !id.is_empty() {
                    by_id.insert(Arc::clone(&id), idx);
                }
                self.orders_id.push(id);
                self.orders_user_id.push(uid);
                self.orders_total.push(total);
            }
            return;
        }
        if collection == "users" && self.users_id.len() == start {
            let by_id = self.by_id.entry(collection.to_string()).or_default();
            for (i, row) in rows.iter().enumerate() {
                let idx = start + i;
                let id = row
                    .get("id")
                    .and_then(Cell::text_shared)
                    .unwrap_or_default();
                let email = row
                    .get("email")
                    .and_then(Cell::text_shared)
                    .unwrap_or_default();
                if !id.is_empty() {
                    by_id.insert(Arc::clone(&id), idx);
                }
                self.users_id.push(id);
                self.users_email.push(email);
            }
            return;
        }
        if !matches!(collection, "docs" | "orders" | "users" | "facts") {
            let by_id = self.by_id.entry(collection.to_string()).or_default();
            for (i, row) in rows.iter().enumerate() {
                if let Some(id) = row.get("id").and_then(Cell::text_shared) {
                    by_id.insert(id, start + i);
                }
            }
            return;
        }
        for (i, row) in rows.iter().enumerate() {
            self.row_maps_register_row(collection, start + i, row);
        }
    }

    /// Reserve row-map capacity before a bulk insert.
    pub fn row_maps_reserve(&mut self, collection: &str, additional: usize) {
        self.by_id
            .entry(collection.to_string())
            .or_default()
            .reserve(additional);
        if collection == "docs" {
            if !self.docs_by_uri.is_empty() || additional < 512 {
                self.docs_by_uri.reserve(additional);
            }
            self.docs_id.reserve(additional);
            self.docs_title.reserve(additional);
            self.docs_layer.reserve(additional);
            self.docs_wing.reserve(additional);
        }
        if collection == "orders" {
            self.orders_id.reserve(additional);
            self.orders_user_id.reserve(additional);
            self.orders_total.reserve(additional);
        }
        if collection == "users" {
            self.users_id.reserve(additional);
            self.users_email.reserve(additional);
        }
    }

    /// Sync maps / SoA after an **in-place** row update. Identity fields are immutable,
    /// so the normal path only rewrites the slots and keys of this one row; a moved
    /// `id`/`uri` (owned collections aside) falls back to rebuilding the collection.
    pub fn row_maps_sync_updated(
        &mut self,
        collection: &str,
        idx: usize,
        before: &Row,
        after: &Row,
    ) {
        if collection == "docs" {
            if before.get("title") != after.get("title") {
                self.invalidate_docs_cache();
            } else {
                self.docs_epoch = next_docs_epoch();
            }
        }
        let id = row_text(after, "id");
        let identity_moved =
            row_text(before, "id") != id || row_text(before, "uri") != row_text(after, "uri");
        let stale_slot = id.is_some_and(|key| self.row_index(collection, key) != Some(idx));
        if identity_moved || stale_slot {
            self.rebuild_row_maps_collection(collection);
            return;
        }
        // Identity keys already address this slot. Re-inserting them would
        // allocate fresh id/uri strings for every non-identity field update.
        let text = |field: &str| {
            after
                .get(field)
                .and_then(Cell::text_shared)
                .unwrap_or_default()
        };
        match collection {
            "docs"
                if idx < self.docs_title.len()
                    && idx < self.docs_layer.len()
                    && idx < self.docs_wing.len() =>
            {
                self.docs_title[idx] = text("title");
                self.docs_layer.set(idx, text("layer"));
                self.docs_wing.set(idx, text("wing"));
            }
            "orders" if idx < self.orders_user_id.len() && idx < self.orders_total.len() => {
                self.orders_user_id[idx] = text("user_id");
                self.orders_total[idx] = after.get("total").and_then(Cell::as_f64).unwrap_or(0.0);
            }
            "users" if idx < self.users_email.len() => self.users_email[idx] = text("email"),
            "docs" | "orders" | "users" => self.rebuild_row_maps_collection(collection),
            _ => {}
        }
    }

    #[inline]
    pub fn sync_docs_wing_only(&mut self, idx: usize, wing: Arc<str>) {
        if idx < self.docs_wing.len() {
            self.docs_wing.set(idx, intern_wing(wing));
        }
    }

    pub fn index_insert_at(&mut self, collection: &str, row_idx: usize) -> Result<(), Error> {
        let Some(row) = self
            .collections
            .get(collection)
            .and_then(|c| c.get(row_idx))
            .cloned()
        else {
            return Ok(());
        };
        self.index_insert_row(collection, row_idx, &row)
    }

    pub fn index_insert_row(
        &mut self,
        collection: &str,
        row_idx: usize,
        row: &Row,
    ) -> Result<(), Error> {
        for idx in self.indexes.values_mut() {
            if idx.def.collection == collection
                && let Err(e) = idx.insert_at(row_idx, row)
            {
                return Err(Error::runtime(e));
            }
        }
        Ok(())
    }

    /// Batch secondary-index inserts for a freshly appended slab `[start, start+rows.len())`.
    pub fn index_insert_slab(
        &mut self,
        collection: &str,
        start: usize,
        rows: &[Row],
    ) -> Result<(), Error> {
        if rows.is_empty() {
            return Ok(());
        }
        for idx in self.indexes.values_mut() {
            if idx.def.collection == collection {
                if rows.len() > 1 {
                    if let Err(e) = idx.insert_slab_batch(start, rows) {
                        return Err(Error::runtime(e));
                    }
                } else if let Err(e) = idx.insert_at_new(start, &rows[0]) {
                    return Err(Error::runtime(e));
                }
            }
        }
        Ok(())
    }

    pub fn maintain_insert_slab(
        &mut self,
        collection: &str,
        start: usize,
        built: &[Row],
    ) -> Result<(), Error> {
        if collection == "docs" {
            self.flush_fts_deferred_docs();
        }
        self.collection_mut(collection).reserve(built.len());
        self.row_maps_reserve(collection, built.len());
        let has_unique = self
            .indexes
            .values()
            .any(|idx| idx.def.collection == collection && idx.def.unique);
        if collection == "docs"
            && built.len() >= 512
            && !has_unique
            && self.docs_id.len() == start
            && self.fts.contains_key(collection)
        {
            self.invalidate_docs_cache();
            let indexes = &mut self.indexes;
            let fts_opt = self.fts.get_mut(collection);
            let hnsw_opt = self.hnsw.get_mut(collection);
            let by_id = self.by_id.entry(collection.to_string()).or_default();
            let docs_by_uri = &mut self.docs_by_uri;
            let docs_id = &mut self.docs_id;
            let docs_title = &mut self.docs_title;
            let docs_layer = &mut self.docs_layer;
            let docs_wing = &mut self.docs_wing;

            struct FtsJob {
                fts_ptr: usize,
                start: usize,
                rows_ptr: usize,
                rows_len: usize,
            }
            struct IdxJob {
                indexes_ptr: usize,
                start: usize,
                rows_ptr: usize,
                rows_len: usize,
            }
            struct MaintPool {
                fts_tx: std::sync::mpsc::SyncSender<FtsJob>,
                fts_done: std::sync::mpsc::Receiver<()>,
                idx_tx: std::sync::mpsc::SyncSender<IdxJob>,
                idx_done: std::sync::mpsc::Receiver<Result<(), Error>>,
            }
            static MAINT_POOL: std::sync::OnceLock<std::sync::Mutex<Option<MaintPool>>> =
                std::sync::OnceLock::new();
            let pool_mutex = MAINT_POOL.get_or_init(|| {
                let (fts_tx, fts_rx) = std::sync::mpsc::sync_channel::<FtsJob>(1);
                let (fts_done_tx, fts_done) = std::sync::mpsc::sync_channel::<()>(1);
                let (idx_tx, idx_rx) = std::sync::mpsc::sync_channel::<IdxJob>(1);
                let (idx_done_tx, idx_done) = std::sync::mpsc::sync_channel::<Result<(), Error>>(1);
                let ok1 = std::thread::Builder::new()
                    .name("lin-maint-fts".into())
                    .spawn(move || {
                        while let Ok(job) = fts_rx.recv() {
                            let fts = unsafe { &mut *(job.fts_ptr as *mut crate::fts::FtsIndex) };
                            let rows = unsafe {
                                std::slice::from_raw_parts(job.rows_ptr as *const Row, job.rows_len)
                            };
                            fts.append_slab(job.start, rows);
                            let _ = fts_done_tx.send(());
                        }
                    })
                    .is_ok();
                let ok2 = std::thread::Builder::new()
                    .name("lin-maint-idx".into())
                    .spawn(move || {
                        while let Ok(job) = idx_rx.recv() {
                            let idxs = unsafe {
                                &mut *(job.indexes_ptr as *mut BTreeMap<String, LiveIndex>)
                            };
                            let rows = unsafe {
                                std::slice::from_raw_parts(job.rows_ptr as *const Row, job.rows_len)
                            };
                            let mut res = Ok(());
                            for idx in idxs.values_mut() {
                                if idx.def.collection == "docs" {
                                    if let Err(e) =
                                        idx.insert_slab_batch_no_reverse(job.start, rows)
                                    {
                                        res = Err(Error::runtime(e));
                                        break;
                                    }
                                }
                            }
                            let _ = idx_done_tx.send(res);
                        }
                    })
                    .is_ok();
                if ok1 && ok2 {
                    std::sync::Mutex::new(Some(MaintPool {
                        fts_tx,
                        fts_done,
                        idx_tx,
                        idx_done,
                    }))
                } else {
                    std::sync::Mutex::new(None)
                }
            });

            let mut fts_opt = fts_opt;
            let pool_guard = pool_mutex.try_lock().ok();
            struct MaintDrainGuard<'a> {
                fts_done: Option<&'a std::sync::mpsc::Receiver<()>>,
                idx_done: Option<&'a std::sync::mpsc::Receiver<Result<(), Error>>>,
                last_idx_res: Option<Result<(), Error>>,
            }
            impl Drop for MaintDrainGuard<'_> {
                fn drop(&mut self) {
                    if let Some(rx) = self.fts_done.take() {
                        let _ = rx.recv();
                    }
                    if let Some(rx) = self.idx_done.take() {
                        self.last_idx_res = rx.recv().ok();
                    }
                }
            }
            let mut maint_guard = MaintDrainGuard {
                fts_done: None,
                idx_done: None,
                last_idx_res: None,
            };
            if let Some(ref guard) = pool_guard
                && let Some(ref pool) = **guard
            {
                if let Some(fts) = fts_opt.as_deref_mut() {
                    if pool
                        .fts_tx
                        .send(FtsJob {
                            fts_ptr: fts as *mut crate::fts::FtsIndex as usize,
                            start,
                            rows_ptr: built.as_ptr() as usize,
                            rows_len: built.len(),
                        })
                        .is_ok()
                    {
                        maint_guard.fts_done = Some(&pool.fts_done);
                    }
                }
                if pool
                    .idx_tx
                    .send(IdxJob {
                        indexes_ptr: indexes as *mut BTreeMap<String, LiveIndex> as usize,
                        start,
                        rows_ptr: built.as_ptr() as usize,
                        rows_len: built.len(),
                    })
                    .is_ok()
                {
                    maint_guard.idx_done = Some(&pool.idx_done);
                }
            }

            if let Some(hnsw) = hnsw_opt {
                for (i, row) in built.iter().enumerate() {
                    if let Some(Cell::Vec(v)) = row.get("embedding") {
                        hnsw.insert(start + i, v.clone());
                    }
                }
            }

            let keep_uri = !docs_by_uri.is_empty();
            let first = &built[0];
            let id_pos = first.keys().position(|k| k == "id");
            let uri_pos = if keep_uri {
                first.keys().position(|k| k == "uri")
            } else {
                None
            };
            let title_pos = first.keys().position(|k| k == "title");
            let layer_pos = first.keys().position(|k| k == "layer");
            let wing_pos = first.keys().position(|k| k == "wing");
            let uniform = built[1..].iter().all(|r| r.len() == first.len());
            if uniform && first.len() <= 32 {
                for (i, row) in built.iter().enumerate() {
                    let idx = start + i;
                    let mut cells: [&Cell; 32] = [&Cell::Null; 32];
                    for (p, val) in row.values().enumerate().take(32) {
                        cells[p] = val;
                    }
                    let id_arc: Arc<str> = id_pos
                        .and_then(|p| cells[p].text_shared())
                        .unwrap_or_default();
                    let uri_arc: Option<Arc<str>> = uri_pos.and_then(|p| cells[p].text_shared());
                    let title_arc: Arc<str> = title_pos
                        .and_then(|p| cells[p].text_shared())
                        .unwrap_or_default();
                    let layer_arc: Arc<str> = layer_pos
                        .and_then(|p| cells[p].text_shared())
                        .unwrap_or_default();
                    let wing_arc: Arc<str> = wing_pos
                        .and_then(|p| cells[p].text_shared())
                        .unwrap_or_default();

                    if !id_arc.is_empty() {
                        by_id.insert(Arc::clone(&id_arc), idx);
                    }
                    if let Some(uri) = uri_arc {
                        docs_by_uri.insert(uri, idx);
                    }
                    docs_id.push(id_arc);
                    docs_title.push(title_arc);
                    docs_layer.push(layer_arc);
                    docs_wing.push(intern_wing(wing_arc));
                }
            } else {
                for (i, row) in built.iter().enumerate() {
                    let idx = start + i;
                    let mut texts: [Option<Arc<str>>; 5] = std::array::from_fn(|_| None);
                    for (field, value) in row {
                        let slot = match field.as_str() {
                            "id" => 0,
                            "uri" if keep_uri => 1,
                            "title" => 2,
                            "layer" => 3,
                            "wing" => 4,
                            _ => continue,
                        };
                        texts[slot] = value.text_shared();
                    }
                    let [id, uri, title, layer, wing] = texts;
                    if let Some(ref id) = id {
                        by_id.insert(Arc::clone(id), idx);
                    }
                    if let Some(uri) = uri {
                        docs_by_uri.insert(uri, idx);
                    }
                    docs_id.push(id.unwrap_or_default());
                    docs_title.push(title.unwrap_or_default());
                    docs_layer.push(layer.unwrap_or_default());
                    docs_wing.push(intern_wing(wing.unwrap_or_default()));
                }
            }

            if maint_guard.fts_done.is_none() {
                if let Some(fts) = fts_opt {
                    fts.append_slab(start, built);
                }
            }
            let mut idx_res = Ok(());
            if maint_guard.idx_done.is_none() {
                for idx in indexes.values_mut() {
                    if idx.def.collection == collection {
                        if let Err(e) = idx.insert_slab_batch_no_reverse(start, built) {
                            idx_res = Err(Error::runtime(e));
                            break;
                        }
                    }
                }
            }
            if let Some(rx) = maint_guard.fts_done.take() {
                let _ = rx.recv();
            }
            if let Some(rx) = maint_guard.idx_done.take() {
                if let Ok(r) = rx.recv() {
                    idx_res = r;
                }
            }
            return idx_res;
        }
        self.index_insert_slab(collection, start, built)?;
        self.fts_insert_slab(collection, start, built);
        self.hnsw_insert_slab(collection, start, built);
        self.row_maps_register_slab(collection, start, built);
        Ok(())
    }

    /// Swap-remove one row's index entries, moving `last` into `p`.
    pub fn index_swap_remove(&mut self, collection: &str, p: usize, last: usize) {
        if self.indexes.is_empty() {
            return;
        }
        for idx in self.indexes.values_mut() {
            if idx.def.collection == collection {
                idx.swap_remove_at(p, last);
            }
        }
    }

    /// Swap-remove one row's index entries given `doomed` and the moved `tail` row now at `p`.
    pub fn index_swap_remove_rows(
        &mut self,
        collection: &str,
        p: usize,
        doomed: &Row,
        last: usize,
    ) {
        if self.indexes.is_empty() {
            return;
        }
        let Some(tail) = self.collections.get(collection).and_then(|c| c.get(p)) else {
            return;
        };
        for idx in self.indexes.values_mut() {
            if idx.def.collection == collection {
                idx.swap_remove_rows(p, doomed, last, tail);
            }
        }
    }

    /// Update one row's index entries given `before` and `after`.
    pub fn index_update_row(
        &mut self,
        collection: &str,
        row_idx: usize,
        before: &Row,
        after: &Row,
    ) -> Result<(), Error> {
        if self.indexes.is_empty() {
            return Ok(());
        }
        for idx in self.indexes.values_mut() {
            if idx.def.collection == collection
                && let Err(e) = idx.update_row(row_idx, before, after)
            {
                return Err(Error::runtime(e));
            }
        }
        Ok(())
    }

    /// Drop a position from every index of `collection`.
    pub fn index_remove_at(&mut self, collection: &str, row_idx: usize) {
        if self.indexes.is_empty() {
            return;
        }
        let row = self
            .collections
            .get(collection)
            .and_then(|c| c.get(row_idx));
        for idx in self.indexes.values_mut() {
            if idx.def.collection == collection {
                if let Some(r) = row {
                    idx.remove_row(row_idx, r);
                } else {
                    idx.remove_at(row_idx);
                }
            }
        }
    }

    /// Drop one row's index entries, given the row itself.
    pub fn index_remove_row(&mut self, collection: &str, row_idx: usize, row: &Row) {
        if self.indexes.is_empty() {
            return;
        }
        for idx in self.indexes.values_mut() {
            if idx.def.collection == collection {
                idx.remove_row(row_idx, row);
            }
        }
    }

    pub fn rebuild_indexes(&mut self) {
        self.indexes.clear();
        let labels: smallvec::SmallVec<[String; 4]> = self.extra_indexes.keys().cloned().collect();
        if labels.len() >= 2 && self.collections.values().map(Vec::len).sum::<usize>() >= 2048 {
            let built: Vec<Option<(String, LiveIndex)>> = std::thread::scope(|s| {
                let store_ref = &*self;
                let handles: Vec<_> = labels
                    .iter()
                    .map(|label| {
                        let label = label.clone();
                        s.spawn(move || {
                            let snap = store_ref.extra_indexes.get(&label)?;
                            let pred = snap
                                .pred_src
                                .as_deref()
                                .and_then(|src| crate::parse::parse_pred_src(src).ok());
                            let def = crate::catalog::IndexDef {
                                collection: snap.collection.clone(),
                                unique: snap.unique,
                                fields: snap.fields.clone(),
                                pred,
                                bitmap_card_field: None,
                            };
                            let mut live = LiveIndex::new(def);
                            if !store_ref.is_disk_backed(&snap.collection) {
                                let rows = store_ref.collection(&snap.collection);
                                if rows.len() > 1 && live.def.pred.is_none() {
                                    let _ = live.insert_slab_batch_no_reverse(0, rows);
                                } else {
                                    for (i, row) in rows.iter().enumerate() {
                                        let _ = live.insert_at_new(i, row);
                                    }
                                }
                                #[cfg(feature = "parallel")]
                                live.build_bloom_filter();
                            }
                            Some((label, live))
                        })
                    })
                    .collect();
                handles
                    .into_iter()
                    .map(|h| h.join().ok().flatten())
                    .collect()
            });
            for (label, live) in built.into_iter().flatten() {
                self.indexes.insert(label, live);
            }
            return;
        }
        for label in labels {
            let Some(snap) = self.extra_indexes.get(&label) else {
                continue;
            };
            let pred = snap
                .pred_src
                .as_deref()
                .and_then(|src| crate::parse::parse_pred_src(src).ok());
            let def = crate::catalog::IndexDef {
                collection: snap.collection.clone(),
                unique: snap.unique,
                fields: snap.fields.clone(),
                pred,
                bitmap_card_field: None,
            };
            let mut live = LiveIndex::new(def);
            if !self.is_disk_backed(&snap.collection) {
                let rows = self.collection(&snap.collection);
                if rows.len() > 1 && live.def.pred.is_none() {
                    let _ = live.insert_slab_batch_no_reverse(0, rows);
                } else {
                    for (i, row) in rows.iter().enumerate() {
                        let _ = live.insert_at_new(i, row);
                    }
                }

                // Build bloom filter for fast negative lookups
                #[cfg(feature = "parallel")]
                live.build_bloom_filter();
            }

            self.indexes.insert(label, live);
        }
    }

    /// Load checkpointed posting lists when gen/nrows/fields match; ignore stale files.
    pub(super) fn try_load_fts(&mut self, dir: &Path, catalog: &crate::catalog::Catalog) {
        for cname in catalog.collections.keys() {
            let fields = crate::fts::fts_fields(catalog, cname);
            if fields.is_empty() {
                continue;
            }
            let nrows = self.get_col_count(cname) as u64;
            match crate::fts::read_fts(dir, cname) {
                Ok((generation, file_rows, idx))
                    if generation == self.r#gen && file_rows == nrows && idx.fields == fields =>
                {
                    self.fts.insert(cname.clone(), idx);
                }
                _ => {}
            }
        }
    }

    pub(super) fn write_fts_files(&mut self, dir: &Path) -> Result<(), Error> {
        self.flush_fts_deferred_docs();
        let mut keep = Vec::new();
        let rows_per_collection: Vec<(String, u64)> = self
            .fts
            .keys()
            .map(|name| (name.clone(), self.get_col_count(name) as u64))
            .collect();
        let generation = self.r#gen;
        for (name, nrows) in rows_per_collection {
            if let Some(idx) = self.fts.get_mut(&name) {
                crate::fts::write_fts_ext(dir, &name, idx, generation, nrows, false)?;
            }
            keep.push(name);
        }
        crate::fts::prune_fts(dir, &keep)?;
        if !keep.is_empty() {
            persist::sync_dir_checked(&crate::fts::fts_dir(dir))?;
        }
        Ok(())
    }

    /// Rebuild FTS postings from catalog `fts` fields (memory / missing durable blob).
    pub fn rebuild_fts(&mut self, catalog: &crate::catalog::Catalog) {
        self.fts.retain(|k, _| catalog.collections.contains_key(k));
        let names: smallvec::SmallVec<[String; 4]> = catalog.collections.keys().cloned().collect();
        for cname in names {
            let Some(cdef) = catalog.collections.get(&cname) else {
                continue;
            };
            let fields: Vec<String> = cdef
                .fields
                .iter()
                .filter(|(_, f)| f.fts)
                .map(|(n, _)| n.clone())
                .collect();
            if fields.is_empty() {
                self.fts.remove(&cname);
                continue;
            }
            let mut existing = self.fts.remove(&cname);
            let rows = self.collection(&cname);
            let idx = if let Some(mut fts) = existing.take() {
                fts.rebuild_in_place(rows, &fields);
                fts
            } else {
                crate::fts::FtsIndex::build(rows, &fields)
            };
            self.fts.insert(cname, idx);
        }
    }

    /// Rebuild only collections that have no live posting list.
    pub fn ensure_fts(&mut self, catalog: &crate::catalog::Catalog) {
        for (cname, cdef) in &catalog.collections {
            let fields: Vec<String> = cdef
                .fields
                .iter()
                .filter(|(_, f)| f.fts)
                .map(|(n, _)| n.clone())
                .collect();
            if fields.is_empty() {
                self.fts.remove(cname);
                continue;
            }
            if self.fts.contains_key(cname) {
                continue;
            }
            if self.is_disk_backed(cname)
                && (self.get_col_count(cname) == 0 || self.is_native_disk())
            {
                if self.get_col_count(cname) == 0 {
                    self.fts
                        .insert(cname.clone(), crate::fts::FtsIndex::build(&[], &fields));
                }
                continue;
            }
            let rows = self.collection(cname);
            self.fts
                .insert(cname.clone(), crate::fts::FtsIndex::build(rows, &fields));
        }
    }

    pub fn rebuild_fts_collection(&mut self, catalog: &crate::catalog::Catalog, collection: &str) {
        let Some(cdef) = catalog.collections.get(collection) else {
            self.fts.remove(collection);
            return;
        };
        let fields: Vec<String> = cdef
            .fields
            .iter()
            .filter(|(_, f)| f.fts)
            .map(|(n, _)| n.clone())
            .collect();
        if fields.is_empty() {
            self.fts.remove(collection);
            return;
        }
        let rows = self.collection(collection);
        self.fts.insert(
            collection.to_string(),
            crate::fts::FtsIndex::build(rows, &fields),
        );
    }

    pub fn fts_insert_at(&mut self, collection: &str, row_idx: usize) {
        if collection == "docs" {
            self.flush_fts_deferred_docs();
        }
        if !self.fts.contains_key(collection) {
            return;
        }
        let Some(row) = self
            .collections
            .get(collection)
            .and_then(|c| c.get(row_idx))
            .cloned()
        else {
            return;
        };
        if let Some(fts) = self.fts.get_mut(collection) {
            fts.insert_row(row_idx, &row);
        }
    }

    /// Register one row's postings at `row_idx`, given the row itself.
    pub fn fts_insert_row(&mut self, collection: &str, row_idx: usize, row: &Row) {
        if collection == "docs" {
            self.flush_fts_deferred_docs();
        }
        if let Some(fts) = self.fts.get_mut(collection) {
            fts.insert_row(row_idx, row);
        }
    }

    pub fn fts_insert_slab(&mut self, collection: &str, start: usize, rows: &[Row]) {
        if collection == "docs" {
            self.flush_fts_deferred_docs();
        }
        let Some(fts) = self.fts.get_mut(collection) else {
            return;
        };
        fts.append_slab(start, rows);
    }

    pub fn hnsw_insert_slab(&mut self, collection: &str, start: usize, rows: &[Row]) {
        let Some(hnsw) = self.hnsw.get_mut(collection) else {
            return;
        };
        for (i, row) in rows.iter().enumerate() {
            if let Some(Cell::Vec(v)) = row.get("embedding") {
                hnsw.insert(start + i, v.clone());
            }
        }
    }

    pub fn build_hnsw(&mut self, collection: &str, m: usize, ef: usize) {
        let mut index = crate::embed::VectorIndex::new_f32(m, ef);
        if let Some(rows) = self.collections.get(collection) {
            for (i, row) in rows.iter().enumerate() {
                if let Some(Cell::Vec(v)) = row.get("embedding") {
                    index.insert(i, v.clone());
                }
            }
        }
        self.hnsw.insert(collection.to_string(), index);
    }

    pub fn build_hnsw_sq8(&mut self, collection: &str, m: usize, ef: usize) {
        let mut index = crate::embed::VectorIndex::new_sq8(m, ef);
        if let Some(rows) = self.collections.get(collection) {
            for (i, row) in rows.iter().enumerate() {
                if let Some(Cell::Vec(v)) = row.get("embedding") {
                    index.insert(i, v.clone());
                }
            }
        }
        self.hnsw.insert(collection.to_string(), index);
    }

    pub fn quantize_hnsw(&mut self, collection: &str) -> bool {
        if let Some(idx) = self.hnsw.get_mut(collection) {
            *idx = idx.to_sq8();
            true
        } else {
            false
        }
    }

    /// Resident memory in bytes of the HNSW index for `collection`, if built.
    pub fn hnsw_resident_memory(&self, collection: &str) -> Option<usize> {
        self.hnsw.get(collection).map(|idx| idx.resident_memory())
    }

    /// Drop the postings of the row currently at `row_idx`.
    pub fn fts_remove_at(&mut self, collection: &str, row_idx: usize) {
        if !self.fts.contains_key(collection) {
            return;
        }
        let Some(row) = self
            .collections
            .get(collection)
            .and_then(|c| c.get(row_idx))
            .cloned()
        else {
            return;
        };
        self.fts_remove_row(collection, row_idx, &row);
    }

    /// Drop one row's postings, given the row itself: only that row's tokens are
    /// visited, so the cost is per-row rather than per-vocabulary.
    pub fn fts_remove_row(&mut self, collection: &str, row_idx: usize, row: &Row) {
        if collection == "docs" {
            self.flush_fts_deferred_docs();
        }
        if let Some(fts) = self.fts.get_mut(collection) {
            fts.remove_row(row_idx, row);
        }
    }

    /// Re-point a row's postings between slots after a swap-move, tokenizing it
    /// once rather than once per edit.
    pub fn fts_move_row(&mut self, collection: &str, from: usize, to: usize, row: &Row) {
        if collection == "docs" {
            self.flush_fts_deferred_docs();
        }
        if let Some(fts) = self.fts.get_mut(collection) {
            fts.move_row(from, to, row);
        }
        if let Some(hnsw) = self.hnsw.get_mut(collection) {
            hnsw.move_id(from, to);
        }
    }

    /// Re-point a single row's postings after an in-place field update, given
    /// the row as it was before the write.
    pub fn fts_sync_at(&mut self, collection: &str, row_idx: usize, before: &Row) {
        if collection == "docs" {
            self.flush_fts_deferred_docs();
        }
        let Some(row) = self
            .collections
            .get(collection)
            .and_then(|c| c.get(row_idx))
            .cloned()
        else {
            return;
        };
        if let Some(fts) = self.fts.get_mut(collection) {
            fts.sync_row(row_idx, before, &row);
        }
    }

    /// Rebuild one collection's FTS using existing field list (after delete / compact).
    pub fn rebuild_fts_inplace(&mut self, collection: &str) {
        let Some(fields) = self.fts.get(collection).map(|f| f.fields.clone()) else {
            return;
        };
        let rows = self.collection(collection);
        self.fts.insert(
            collection.to_string(),
            crate::fts::FtsIndex::build(rows, &fields),
        );
    }

    pub fn rebuild_indexes_collection(&mut self, collection: &str) {
        let labels: Vec<String> = self
            .indexes
            .iter()
            .filter(|(_, idx)| idx.def.collection == collection)
            .map(|(k, _)| k.clone())
            .collect();
        let rows = self.collection(collection).to_vec();
        for label in labels {
            let def = if let Some(existing) = self.indexes.get(&label) {
                existing.def.clone()
            } else if let Some(snap) = self.extra_indexes.get(&label) {
                let pred = snap
                    .pred_src
                    .as_deref()
                    .and_then(|src| crate::parse::parse_pred_src(src).ok());
                crate::catalog::IndexDef {
                    collection: snap.collection.clone(),
                    unique: snap.unique,
                    fields: snap.fields.clone(),
                    pred,
                    bitmap_card_field: None,
                }
            } else {
                continue;
            };
            let mut live = LiveIndex::new(def);
            if live.def.pred.is_none() {
                let _ = live.insert_slab_batch_no_reverse(0, &rows);
            } else {
                for (i, row) in rows.iter().enumerate() {
                    let _ = live.insert_at(i, row);
                }
            }
            self.indexes.insert(label, live);
        }
    }

    pub fn index_seek(
        &self,
        collection: &str,
        uses: &[crate::index::IndexUse],
        now: i64,
    ) -> Option<Vec<usize>> {
        // A disk-backed collection keeps only an empty placeholder `LiveIndex`; its secondary
        // indexes live in the disk engine. Report "no usable index" so callers scan instead
        // of trusting an empty seek result.
        if self.is_disk_backed(collection) {
            return None;
        }
        if uses.is_empty() {
            return None;
        }
        if uses.len() == 1 {
            let use_ = &uses[0];
            let idx = self.indexes.get(&use_.def.label())?;
            if idx.def.collection != collection {
                return None;
            }
            return Some(idx.seek_idxs(use_, now));
        }
        let mut out = Vec::new();
        let mut seen = FxHashSet::default();
        for use_ in uses {
            let idx = self.indexes.get(&use_.def.label())?;
            if idx.def.collection != collection {
                return None;
            }
            idx.seek_each(use_, now, |i| {
                if seen.insert(i) {
                    out.push(i);
                }
            });
        }
        Some(out)
    }

    pub fn index_seek_count(
        &self,
        collection: &str,
        uses: &[crate::index::IndexUse],
        now: i64,
    ) -> Option<usize> {
        // A disk-backed collection keeps only an empty placeholder `LiveIndex`; its secondary
        // indexes live in the disk engine. Report "no usable index" so callers scan instead
        // of trusting an empty seek result.
        if self.is_disk_backed(collection) {
            return None;
        }
        if uses.is_empty() {
            return None;
        }
        if uses.len() == 1 {
            let use_ = &uses[0];
            let idx = self.indexes.get(&use_.def.label())?;
            if idx.def.collection != collection {
                return None;
            }
            return Some(idx.seek_count(use_, now));
        }
        // OR branches may overlap — dedupe via seek.
        Some(self.index_seek(collection, uses, now)?.len())
    }

    /// Count with early termination - returns None if count exceeds threshold
    pub fn index_seek_count_with_threshold(
        &self,
        collection: &str,
        uses: &[crate::index::IndexUse],
        now: i64,
        threshold: usize,
    ) -> Option<usize> {
        // A disk-backed collection keeps only an empty placeholder `LiveIndex`; its secondary
        // indexes live in the disk engine. Report "no usable index" so callers scan instead
        // of trusting an empty seek result.
        if self.is_disk_backed(collection) {
            return None;
        }
        if uses.is_empty() {
            return None;
        }
        if uses.len() == 1 {
            let use_ = &uses[0];
            let idx = self.indexes.get(&use_.def.label())?;
            if idx.def.collection != collection {
                return None;
            }
            return idx.seek_count_with_early_termination(use_, now, threshold);
        }
        // For multiple uses, fall back to full seek
        Some(self.index_seek(collection, uses, now)?.len())
    }
}

#[derive(Clone)]
pub struct MemBackup {
    r#gen: u64,
    embed_id: String,
    next_id: u64,
    collections: BTreeMap<String, Vec<Row>>,
    edges: Vec<Edge>,
    extra_collections: BTreeMap<String, ColSnap>,
    extra_rels: BTreeMap<String, RelSnap>,
    extra_indexes: BTreeMap<String, IndexSnap>,
    extra_filters: BTreeMap<String, String>,
    extra_owned: BTreeMap<String, Vec<(String, String)>>,
    hnsw: BTreeMap<String, crate::embed::VectorIndex>,
}

fn same_row_key(collection: &str, a: &Row, b: &Row) -> bool {
    if collection == "facts" {
        return row_text(a, "s") == row_text(b, "s")
            && row_text(a, "p") == row_text(b, "p")
            && row_text(a, "o") == row_text(b, "o");
    }
    match (row_text(a, "id"), row_text(b, "id")) {
        (Some(x), Some(y)) => x == y,
        _ => row_text(a, "uri")
            .zip(row_text(b, "uri"))
            .is_some_and(|(x, y)| x == y),
    }
}

#[cfg(test)]
mod borrowed_delete_tests {
    use super::*;

    #[test]
    fn doc_slab_registration_matches_single_rows_with_missing_and_nontext_fields() {
        let rows = (0..64)
            .map(|i| {
                let mut row = Row::new();
                for field in ["id", "uri", "title", "layer", "wing", "extra"] {
                    match (i + field.len()) % 4 {
                        0 => {}
                        1 => {
                            row.insert(field.into(), Cell::Null);
                        }
                        2 => {
                            row.insert(field.into(), Cell::Int(i as i64));
                        }
                        _ => {
                            row.insert(field.into(), Cell::text_arc(format!("{field}-{}", i % 9)));
                        }
                    }
                }
                row
            })
            .collect::<Vec<_>>();
        let mut slab = Store::empty("test/8");
        let mut scalar = Store::empty("test/8");
        slab.row_maps_register_slab("docs", 0, &rows);
        for (i, row) in rows.iter().enumerate() {
            scalar.row_maps_register_row("docs", i, row);
        }
        assert_eq!(slab.by_id, scalar.by_id);
        assert_eq!(slab.docs_by_uri, scalar.docs_by_uri);
        assert_eq!(slab.docs_id, scalar.docs_id);
        assert_eq!(slab.docs_title, scalar.docs_title);
        assert_eq!(slab.docs_layer, scalar.docs_layer);
        assert_eq!(slab.docs_wing, scalar.docs_wing);
    }
    use crate::catalog::IndexDef;

    fn seed(collection: &str, rows: Vec<Row>) -> Store {
        let mut store = Store::empty("test");
        store.collections.insert(collection.into(), rows);
        store.rebuild_row_maps();
        for field in ["id", "group"] {
            let mut index = LiveIndex::new(IndexDef {
                collection: collection.into(),
                fields: vec![field.into()],
                unique: field == "id",
                pred: None,
                bitmap_card_field: None,
            });
            for (pos, row) in store.collection(collection).iter().enumerate() {
                index.insert_at_new(pos, row).unwrap();
            }
            store.indexes.insert(field.into(), index);
        }
        let fields = vec!["body".into(), "title".into()];
        store.fts.insert(
            collection.into(),
            crate::fts::FtsIndex::build(store.collection(collection), &fields),
        );
        store
    }

    #[test]
    fn borrowed_moves_match_rebuilt_rows_maps_columns_indexes_and_fts() {
        for collection in ["docs", "orders", "users", "facts", "custom"] {
            let rows = (0..31)
                .map(|i| {
                    Row::from([
                        ("id".into(), Cell::text_arc(format!("id{i}"))),
                        ("uri".into(), Cell::text_arc(format!("uri{i}"))),
                        ("title".into(), Cell::text_arc(format!("shared title{i}"))),
                        ("body".into(), Cell::text_arc(format!("shared unique{i}"))),
                        ("layer".into(), Cell::text_arc(format!("layer{i}"))),
                        ("wing".into(), Cell::text_arc(format!("wing{i}"))),
                        ("user_id".into(), Cell::text_arc(format!("user{i}"))),
                        ("total".into(), Cell::Float(i as f64 + 0.25)),
                        ("email".into(), Cell::text_arc(format!("mail{i}"))),
                        ("s".into(), Cell::text_arc(format!("s{i}"))),
                        ("p".into(), Cell::text_arc("tagged")),
                        ("o".into(), Cell::text_arc(format!("o{i}"))),
                        ("group".into(), Cell::Int(i % 3)),
                    ])
                })
                .collect::<Vec<_>>();
            let mut model = rows.clone();
            let mut store = seed(collection, rows);
            while !model.is_empty() {
                let dead = [0, model.len() / 3, model.len() / 3, usize::MAX];
                let mut valid = dead
                    .into_iter()
                    .filter(|&p| p < model.len())
                    .collect::<Vec<_>>();
                valid.sort_unstable();
                valid.dedup();
                for &pos in valid.iter().rev() {
                    model.swap_remove(pos);
                }
                store.delete_rows_at(collection, &dead);
                let expected = seed(collection, model.clone());
                assert_eq!(
                    store.collection(collection),
                    expected.collection(collection)
                );
                assert_eq!(store.by_id, expected.by_id);
                assert_eq!(store.docs_by_uri, expected.docs_by_uri);
                assert_eq!(store.facts_by_spo, expected.facts_by_spo);
                assert_eq!(store.docs_id, expected.docs_id);
                assert_eq!(store.docs_title, expected.docs_title);
                assert_eq!(store.docs_layer, expected.docs_layer);
                assert_eq!(store.docs_wing, expected.docs_wing);
                assert_eq!(store.orders_id, expected.orders_id);
                assert_eq!(store.orders_user_id, expected.orders_user_id);
                assert_eq!(store.orders_total, expected.orders_total);
                assert_eq!(store.users_id, expected.users_id);
                assert_eq!(store.users_email, expected.users_email);
                assert_eq!(store.facts_s, expected.facts_s);
                assert_eq!(store.facts_p, expected.facts_p);
                assert_eq!(store.facts_o, expected.facts_o);
                for (label, index) in &store.indexes {
                    let mut actual = index.forward.clone();
                    for positions in actual.values_mut() {
                        positions.sort_unstable();
                    }
                    assert_eq!(actual, expected.indexes[label].forward);
                    assert_eq!(index.reverse, expected.indexes[label].reverse);
                }
                for query in std::iter::once("shared".to_string())
                    .chain((0..31).map(|i| format!("unique{i}")))
                {
                    assert_eq!(
                        store.fts[collection].candidate_idxs(&query),
                        expected.fts[collection].candidate_idxs(&query)
                    );
                }
            }
        }
    }

    #[test]
    fn nonparallel_columns_keep_the_rebuild_fallback() {
        let rows = (0..4)
            .map(|i| {
                Row::from([
                    ("id".into(), Cell::text_arc(format!("id{i}"))),
                    ("title".into(), Cell::text_arc(format!("title{i}"))),
                    ("group".into(), Cell::Int(i)),
                ])
            })
            .collect::<Vec<_>>();
        let mut store = seed("docs", rows);
        store.docs_id.clear();
        store.docs_title.clear();
        store.docs_layer.clear();
        store.docs_wing.clear();
        store.delete_rows_at("docs", &[1]);
        let expected = seed("docs", store.collection("docs").to_vec());
        assert_eq!(store.docs_id, expected.docs_id);
        assert_eq!(store.docs_title, expected.docs_title);
        assert_eq!(store.by_id, expected.by_id);
    }

    #[test]
    fn sso_inline_str_and_cell_small_text_semantics() {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        // Size check: Cell must remain 24 bytes
        assert_eq!(std::mem::size_of::<Cell>(), 24);
        assert_eq!(std::mem::size_of::<InlineStr>(), 23);

        // Strings <= 22 bytes become SmallText
        let short = "user_12345";
        let c1 = Cell::from_text(short);
        assert!(matches!(c1, Cell::SmallText(_)));
        assert_eq!(c1.text(), Some(short));

        // Strings > 22 bytes become Text(Arc<str>)
        let long = "this is a very long string that exceeds 22 bytes";
        let c2 = Cell::from_text(long);
        assert!(matches!(c2, Cell::Text(_)));
        assert_eq!(c2.text(), Some(long));

        // Equality between SmallText and Text
        let c_arc = Cell::Text(std::sync::Arc::from(short));
        assert_eq!(c1, c_arc);
        assert_eq!(c_arc, c1);

        // Hashing consistency
        let mut h1 = DefaultHasher::new();
        c1.hash(&mut h1);
        let mut h2 = DefaultHasher::new();
        c_arc.hash(&mut h2);
        assert_eq!(h1.finish(), h2.finish());

        // Zero-cost clone_for_borrow
        let borrowed = c1.clone_for_borrow();
        assert_eq!(borrowed, c1);
        assert!(matches!(borrowed, Cell::SmallText(_)));

        // Serde roundtrip
        let serialized = serde_json::to_string(&c1).unwrap();
        let deserialized: Cell = serde_json::from_str(&serialized).unwrap();
        assert_eq!(c1, deserialized);

        // Boundary tests for 22 bytes exactly
        let s22 = "1234567890123456789012";
        assert_eq!(s22.len(), 22);
        let c22 = Cell::from_text(s22);
        assert!(matches!(c22, Cell::SmallText(_)));

        let s23 = "12345678901234567890123";
        assert_eq!(s23.len(), 23);
        let c23 = Cell::from_text(s23);
        assert!(matches!(c23, Cell::Text(_)));
    }

    #[test]
    fn cell_equality_and_hash_contract_across_all_variants() {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        fn hash_cell(c: &Cell) -> u64 {
            let mut h = DefaultHasher::new();
            c.hash(&mut h);
            h.finish()
        }

        let nan_q = Cell::Float(f64::from_bits(0x7ff8_0000_0000_0000));
        let nan_payload = Cell::Float(f64::from_bits(0x7ff8_0000_0000_0001));
        assert_eq!(nan_q, nan_q.clone());
        assert_eq!(hash_cell(&nan_q), hash_cell(&nan_q.clone()));
        assert_ne!(nan_q, nan_payload);

        // Int(1) and Float(1.0) are distinct Cell variants in Eq/Hash
        let int_one = Cell::Int(1);
        let float_one = Cell::Float(1.0);
        assert_ne!(int_one, float_one);
        assert_ne!(hash_cell(&int_one), hash_cell(&float_one));

        // Signed zeros have distinct bit representations in Cell::Float
        let pos_zero = Cell::Float(0.0);
        let neg_zero = Cell::Float(-0.0);
        assert_ne!(pos_zero, neg_zero);

        // SmallText and Text with identical UTF-8 content must be equal and have identical hashes
        let s_inline = Cell::from_text("group_key");
        let s_arc = Cell::Text(Arc::from("group_key"));
        assert_eq!(s_inline, s_arc);
        assert_eq!(hash_cell(&s_inline), hash_cell(&s_arc));

        // Every equal pair of cells must produce the exact same hash
        let samples = [
            Cell::Null,
            s_inline,
            s_arc,
            Cell::from_text("this string is longer than twenty-two bytes"),
            int_one,
            float_one,
            pos_zero,
            neg_zero,
            nan_q,
            Cell::Bool(true),
            Cell::Bool(false),
            Cell::Time(1_700_000_000_000),
            Cell::Vec(Arc::from([0.5_f32, -0.5_f32].as_slice())),
        ];
        for a in &samples {
            for b in &samples {
                if a == b {
                    assert_eq!(
                        hash_cell(a),
                        hash_cell(b),
                        "Hash/Eq mismatch for {a:?} and {b:?}"
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod empty_index_slab_tests {
    #[test]
    fn empty_index_slab_preserves_empty_and_populated_indexes() {
        for populated in [false, true] {
            let mut db = crate::Db::empty();
            db.run("index docs [wing, ts]").unwrap();
            if populated {
                db.run(r#"insert docs [{id:"a",uri:"a",wing:"rag",body:"first"},{id:"b",uri:"b",wing:"sys",body:"second"}]"#).unwrap();
            }
            let before = db.store.indexes.clone();
            let rows_before = db.store.collection("docs").to_vec();
            for collection in ["docs", "users", "missing"] {
                for start in [0, rows_before.len(), usize::MAX] {
                    db.store.index_insert_slab(collection, start, &[]).unwrap();
                    assert_eq!(db.store.indexes.len(), before.len());
                    for (label, index) in &before {
                        let after = &db.store.indexes[label];
                        assert_eq!(after.forward, index.forward);
                        assert_eq!(after.reverse, index.reverse);
                        assert_eq!(after.def.fields, index.def.fields);
                    }
                    assert_eq!(db.store.collection("docs"), rows_before);
                }
            }
        }
    }
}
