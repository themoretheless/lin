//! Disk-backed out-of-core columnar LSM / Segment-Mmap Storage Engine (`DiskEngine`).
//!
//! Designed to scale seamlessly from megabytes to **500+ GiB** databases while keeping:
//! - **`open()` under ~5–15 ms** (pure `mmap` of segment files, zero eager row deserialization).
//! - **Constant low RAM footprint** (OS page cache manages cold/hot pages; no `Vec<Row>` full-corpus heap resident image).
//! - **O(1) Point Lookups & Secondary Index Seeks** directly on memory-mapped hash + sorted-key posting tables.
//! - **Two-Stage Out-of-Core Vector Search**: SIMD (`NEON` / scalar) + `rayon` scan over flat `mmap` `SQ8` (`i8`)
//!   quantized vectors for 4x SSD/memory-bandwidth reduction, followed by exact `mmap` `f32` cosine reranking
//!   of the top candidates — zero `VECTOR_SNAPSHOT` heap copies.
//! - **Append-Only MemTable + WAL + Incremental Segment Flush** (`checkpoint` writes only the new delta segment,
//!   never rewriting existing multi-gigabyte segments; deletes mark a durable tombstone bitset).

mod cache;
mod columns;
mod index;
mod segment;
#[cfg(test)]
mod tests;

use std::cmp::{Ordering, Reverse};
use std::collections::{BTreeMap, BinaryHeap};
use std::fs::{self, File, OpenOptions};
use std::io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use rustc_hash::{FxHashMap, FxHashSet};
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

pub use cache::{HotCacheConfig, HotCacheStats, HotTierCache};
pub use columns::{MmapVecCol, MmapVecIvf};
pub use index::{MmapBm25Builder, MmapBm25Index};
pub use segment::DiskSegment;

pub(crate) use columns::MmapCol;
use columns::{DICT_MIN_SEGMENT_ROWS, DICT_NULL_CODE, IVF_MIN_SEGMENT_VECS};
use index::{encode_index_cell_key, with_text_index_key};
use segment::write_segment_to_disk;

use crate::embed::{cosine_with_q_norm, dot_sq8, l2_norm_f64, quantize_sq8};
use crate::error::Error;
use crate::persist::io_err;
use crate::store::{Cell, InlineStr, Row};

pub const DISK_DIR_MAGIC: &[u8; 8] = b"LINDISK\x01";
pub const MANIFEST_FILE: &str = "manifest.json";
pub const WAL_FILE: &str = "wal.log";
pub const TOMBSTONES_FILE: &str = "tombstones.bin";
pub const SEGMENTS_DIR: &str = "segments";
pub const LOCK_FILE: &str = "LOCK";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CompactionReport {
    pub tables_compacted: usize,
    pub old_segments_removed: usize,
    pub new_segments_written: usize,
    pub dead_rows_reclaimed: usize,
}

/// Column storage type tag on disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiskColKind {
    Int64,
    Float64,
    Bool,
    Time64,
    /// Block-compressed UTF-8 strings (`col_<name>.loc` + `.boff` + `.bdat`).
    TextZstd,
    /// Adaptive dictionary-encoded UTF-8 strings (`col_<name>.dstr` + `.doff` + `.dord` `u16` ordinals).
    TextDict,
    /// Flat aligned `f32` vectors (`col_<name>.vf32`) + pre-quantized `i8` SQ8 vectors (`col_<name>.vsq8`).
    VecF32Sq8 {
        dim: u32,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskColSchema {
    pub name: String,
    pub kind: DiskColKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskTableSchema {
    pub name: String,
    pub columns: Vec<DiskColSchema>,
    pub indexed_fields: Vec<String>,
    /// Column whose order compaction imposes on the table's rows (see
    /// [`DiskEngine::set_cluster_key`]). Absent in manifests written before this field.
    #[serde(default)]
    pub cluster_by: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SegmentMeta {
    pub seg_id: u64,
    pub table: String,
    pub row_count: u32,
    pub columns: Vec<DiskColSchema>,
    pub indexed_fields: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DiskManifest {
    pub version: u32,
    pub generation: u64,
    pub next_seg_id: u64,
    pub embed_id: String,
    pub tables: BTreeMap<String, DiskTableSchema>,
    pub segments: Vec<SegmentMeta>,
}

/// Durable WAL entry for uncommitted/unflushed MemTable mutations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DiskWalOp {
    EnsureTable { schema: DiskTableSchema },
    EnsureIndex { table: String, field: String },
    UpsertBatch { table: String, rows: Vec<Row> },
    DeleteIds { table: String, ids: Vec<String> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DiskWalFrame {
    generation: u64,
    ops: Vec<DiskWalOp>,
}

#[derive(Serialize)]
struct DiskWalFrameRef<'a> {
    generation: u64,
    ops: &'a [DiskWalOp],
}

/// Decode one cell of a segment column, reusing the last decoded text block when consecutive
/// rows fall into the same compressed block.
fn read_seg_cell(
    seg: &DiskSegment,
    pos: usize,
    rid: u32,
    last_block: &mut Option<(u32, Arc<[u8]>)>,
) -> Result<Cell, Error> {
    Ok(match &seg.cols[pos].1 {
        MmapCol::Text(tc) => match tc.get_loc(rid) {
            None => Cell::Null,
            Some((_, _, 0)) => Cell::SmallText(InlineStr::new_truncate("")),
            Some((bidx, in_off, len)) => {
                let block = match last_block {
                    Some((lb, arc)) if *lb == bidx => Arc::clone(arc),
                    _ => {
                        let decoded = tc.decode_block(bidx)?;
                        *last_block = Some((bidx, Arc::clone(&decoded)));
                        decoded
                    }
                };
                let s_off = in_off as usize;
                let e_off = s_off + (len as usize);
                if e_off > block.len() {
                    return Err(io_err("corrupt in-block text slice"));
                }
                let s = std::str::from_utf8(&block[s_off..e_off]).map_err(io_err)?;
                Cell::from_text(s)
            }
        },
        other => other.get_cell(rid)?,
    })
}

/// Top-level `AND` conjuncts of a predicate that can be evaluated on raw column data.
struct PredTerms<'a> {
    /// `leaf <range-op> value`: a zone-map block whose min/max cannot satisfy one of them
    /// cannot contain a matching row.
    ranges: Vec<(&'a str, crate::ast::CmpOp, Cell)>,
    /// `leaf == "text"`: a segment whose dictionary lacks the text cannot match.
    text_eqs: Vec<(&'a str, &'a str)>,
    /// `leaf in ["a", "b"]` and `leaf ~ "needle"`: on a dictionary column the set of
    /// matching codes is computed once per segment; an empty set rules the segment out.
    text_sets: Vec<(&'a str, TextSet<'a>)>,
    /// True when these conjuncts are the whole predicate, i.e. a row matches iff every
    /// term holds.
    exact: bool,
}

enum TextSet<'a> {
    AnyOf(Vec<&'a str>),
    Contains(&'a str),
}

impl TextSet<'_> {
    /// Which dictionary codes satisfy the term.
    fn allowed_codes(&self, dc: &columns::MmapDictCol) -> Vec<bool> {
        match self {
            TextSet::AnyOf(values) => dc
                .strings
                .iter()
                .map(|s| values.iter().any(|v| *v == s.as_ref()))
                .collect(),
            TextSet::Contains(needle) => dc
                .strings
                .iter()
                .map(|s| crate::exec::text_contains(s.as_ref(), needle))
                .collect(),
        }
    }
}

fn zone_prunable_terms(pred: &crate::ast::Pred, now_ms: i64) -> PredTerms<'_> {
    use crate::ast::{CmpOp, Pred, Value};
    fn walk<'a>(p: &'a Pred, now_ms: i64, out: &mut PredTerms<'a>) {
        match p {
            Pred::And(a, b) => {
                walk(a, now_ms, out);
                walk(b, now_ms, out);
            }
            Pred::Cmp { field, op, value } => match (field.leaf(), op, value) {
                (Some(name), CmpOp::Gt | CmpOp::Ge | CmpOp::Lt | CmpOp::Le, _) => {
                    out.ranges
                        .push((name, *op, crate::exec::value_cell(value, now_ms)));
                }
                (Some(name), CmpOp::Eq, Value::String(s)) => {
                    out.text_eqs.push((name, s.as_ref()));
                }
                _ => out.exact = false,
            },
            Pred::In { field, values } => {
                let texts: Option<Vec<&str>> = values
                    .iter()
                    .map(|v| match v {
                        Value::String(s) => Some(s.as_ref()),
                        _ => None,
                    })
                    .collect();
                match (field.leaf(), texts) {
                    (Some(name), Some(texts)) => out.text_sets.push((name, TextSet::AnyOf(texts))),
                    _ => out.exact = false,
                }
            }
            Pred::Contains { field, needle } => match field.leaf() {
                Some(name) => out.text_sets.push((name, TextSet::Contains(needle.as_str()))),
                None => out.exact = false,
            },
            _ => out.exact = false,
        }
    }
    let mut out = PredTerms {
        ranges: Vec::new(),
        text_eqs: Vec::new(),
        text_sets: Vec::new(),
        exact: true,
    };
    walk(pred, now_ms, &mut out);
    out
}

/// Below this many rows a per-segment parallel scan costs more than it saves.
#[cfg(feature = "parallel")]
const PAR_SEGMENT_MIN_ROWS: u64 = 65_536;

// ============================================================================
// Mutable Delta MemTable (per collection) + Tombstone Bitsets
// ============================================================================

#[derive(Default, Clone)]
struct MemTable {
    /// Latest uncommitted/unflushed row version per primary key `id`.
    rows: Vec<Row>,
    by_id: FxHashMap<String, usize>,
    /// Deleted row indices inside `rows` (when overwritten/deleted in MemTable).
    dead_mem: FxHashSet<usize>,
    /// Secondary index in MemTable: `field -> encoded_key -> Vec<usize>`.
    sec: FxHashMap<String, FxHashMap<Vec<u8>, Vec<usize>>>,
    /// IDs explicitly deleted since last segment flush (masks any older segments).
    deleted_ids: FxHashSet<String>,
}

impl MemTable {
    fn rebuild_sec_for_field(&mut self, field: &str) {
        let mut fmap: FxHashMap<Vec<u8>, Vec<usize>> = FxHashMap::default();
        let mut kbuf = Vec::with_capacity(64);
        for (idx, row) in self.rows.iter().enumerate() {
            if self.dead_mem.contains(&idx) {
                continue;
            }
            if let Some(cell) = row.get(field)
                && encode_index_cell_key(cell, &mut kbuf)
            {
                fmap.entry(kbuf.clone()).or_default().push(idx);
            }
        }
        self.sec.insert(field.to_string(), fmap);
    }

    fn upsert_row(
        &mut self,
        row: Row,
        indexed_fields: &[String],
        mut undo: Option<&mut MemTableUndo>,
    ) {
        let Some(id) = row.get("id").and_then(Cell::text).map(str::to_string) else {
            return;
        };
        if let Some(u) = undo.as_deref_mut() {
            if !u.prev_by_id.contains_key(&id) {
                u.prev_by_id
                    .insert(id.clone(), self.by_id.get(&id).copied());
            }
            if !u.prev_deleted_ids.contains_key(&id) {
                u.prev_deleted_ids
                    .insert(id.clone(), self.deleted_ids.contains(&id));
            }
        }
        self.deleted_ids.remove(&id);
        if let Some(&old_idx) = self.by_id.get(&id)
            && self.dead_mem.insert(old_idx)
            && let Some(u) = undo.as_deref_mut()
            && old_idx < u.prev_rows_len
        {
            u.added_dead_mem.push(old_idx);
        }
        let new_idx = self.rows.len();
        let mut kbuf = Vec::with_capacity(64);
        for field in indexed_fields {
            if field == "id" {
                continue;
            }
            if let Some(cell) = row.get(field)
                && encode_index_cell_key(cell, &mut kbuf)
            {
                self.sec
                    .entry(field.clone())
                    .or_default()
                    .entry(kbuf.clone())
                    .or_default()
                    .push(new_idx);
            }
        }
        self.by_id.insert(id, new_idx);
        self.rows.push(row);
    }

    fn delete_id(&mut self, id: &str, mut undo: Option<&mut MemTableUndo>) {
        if let Some(u) = undo.as_deref_mut() {
            if !u.prev_by_id.contains_key(id) {
                u.prev_by_id
                    .insert(id.to_string(), self.by_id.get(id).copied());
            }
            if !u.prev_deleted_ids.contains_key(id) {
                u.prev_deleted_ids
                    .insert(id.to_string(), self.deleted_ids.contains(id));
            }
        }
        if let Some(old_idx) = self.by_id.remove(id)
            && self.dead_mem.insert(old_idx)
            && let Some(u) = undo.as_deref_mut()
            && old_idx < u.prev_rows_len
        {
            u.added_dead_mem.push(old_idx);
        }
        self.deleted_ids.insert(id.to_string());
    }

    fn live_count(&self) -> usize {
        self.by_id.len()
    }

    fn drain_live_rows(&mut self) -> Vec<Row> {
        let mut out = Vec::with_capacity(self.by_id.len());
        let mut idxs: Vec<usize> = self.by_id.values().copied().collect();
        idxs.sort_unstable();
        let mut taken = std::mem::take(&mut self.rows);
        for idx in idxs {
            if !self.dead_mem.contains(&idx)
                && let Some(r) = taken.get_mut(idx)
            {
                out.push(std::mem::take(r));
            }
        }
        self.by_id.clear();
        self.dead_mem.clear();
        self.sec.clear();
        self.deleted_ids.clear();
        out
    }
}

/// Bitset of tombstoned `row_id`s per `seg_id` (`u64` words).
#[derive(Default, Clone, Serialize, Deserialize)]
pub(crate) struct SegmentTombstones {
    /// `seg_id -> bitset words` (`1` = deleted).
    words_by_seg: BTreeMap<u64, Vec<u64>>,
    /// Cached count of set bits per segment.
    dead_counts: BTreeMap<u64, u32>,
}

impl SegmentTombstones {
    #[inline]
    fn is_dead(&self, seg_id: u64, row_id: u32) -> bool {
        let Some(words) = self.words_by_seg.get(&seg_id) else {
            return false;
        };
        let w = (row_id as usize) >> 6;
        let b = row_id & 63;
        w < words.len() && (words[w] & (1u64 << b)) != 0
    }

    #[inline]
    fn mark_dead(&mut self, seg_id: u64, row_id: u32, seg_row_count: u32) -> bool {
        let n_words = ((seg_row_count as usize) + 63) >> 6;
        let words = self
            .words_by_seg
            .entry(seg_id)
            .or_insert_with(|| vec![0u64; n_words]);
        let w = (row_id as usize) >> 6;
        if w >= words.len() {
            words.resize(w + 1, 0);
        }
        let mask = 1u64 << (row_id & 63);
        if (words[w] & mask) == 0 {
            words[w] |= mask;
            *self.dead_counts.entry(seg_id).or_insert(0) += 1;
            true
        } else {
            false
        }
    }

    #[inline]
    fn dead_count(&self, seg_id: u64) -> u32 {
        self.dead_counts.get(&seg_id).copied().unwrap_or(0)
    }

    #[inline]
    fn unmark_dead(&mut self, seg_id: u64, row_id: u32) {
        let Some(words) = self.words_by_seg.get_mut(&seg_id) else {
            return;
        };
        let w = (row_id as usize) >> 6;
        let mask = 1u64 << (row_id & 63);
        if w < words.len() && (words[w] & mask) != 0 {
            words[w] &= !mask;
            if let Some(cnt) = self.dead_counts.get_mut(&seg_id) {
                *cnt = cnt.saturating_sub(1);
            }
        }
    }

    pub(crate) fn merge_from(&mut self, other: &SegmentTombstones) {
        for (&seg_id, other_words) in &other.words_by_seg {
            let words = self.words_by_seg.entry(seg_id).or_default();
            if words.len() < other_words.len() {
                words.resize(other_words.len(), 0);
            }
            for (w, &ow) in words.iter_mut().zip(other_words.iter()) {
                *w |= ow;
            }
            let count: u32 = words.iter().map(|w| w.count_ones()).sum();
            self.dead_counts.insert(seg_id, count);
        }
    }
}

#[derive(Clone)]
struct MemTableUndo {
    existed_before: bool,
    prev_rows_len: usize,
    prev_by_id: FxHashMap<String, Option<usize>>,
    added_dead_mem: Vec<usize>,
    prev_deleted_ids: FxHashMap<String, bool>,
}

#[derive(Clone)]
pub struct DiskSavepoint {
    saved_tables: Option<BTreeMap<String, DiskTableSchema>>,
    memtable_undos: FxHashMap<String, MemTableUndo>,
    added_tombstones: Vec<(u64, u32)>,
    pending_wal_ops: Vec<DiskWalOp>,
    saved_dirty_overlay: FxHashSet<String>,
}

// ============================================================================
// Out-of-Core DiskEngine + L1 In-Memory HotTierCache
// ============================================================================

pub struct DiskEngine {
    dir: PathBuf,
    pub manifest: DiskManifest,
    /// `table_name -> ordered Vec<Arc<DiskSegment>>` (oldest first, newest last).
    segments: BTreeMap<String, Vec<Arc<DiskSegment>>>,
    memtables: BTreeMap<String, MemTable>,
    tombstones: SegmentTombstones,
    hot_cache: Arc<HotTierCache>,
    active_savepoint: Option<DiskSavepoint>,
    /// Bumped on every in-memory overlay mutation and savepoint rollback.
    overlay_epoch: u64,
    dirty_overlay: FxHashSet<String>,
    wal_file: Option<File>,
    wal_buf: Vec<u8>,
    pub(crate) wal_bytes: u64,
    sync_full: bool,
    read_only: bool,
}

#[derive(Debug, Clone)]
pub struct DiskVecHit {
    pub seg_id: u64,
    pub row_id: u32,
    pub score: f32,
}

/// Detect whether `dir` is an initialized `DiskEngine` directory.
pub fn is_disk_dir(dir: &Path) -> bool {
    dir.is_dir() && dir.join(MANIFEST_FILE).is_file()
}

impl DiskEngine {
    /// Open or create a read-write `DiskEngine` directory in ~2–10 ms (`mmap` only, zero bulk RAM load).
    pub fn open(dir: impl AsRef<Path>, sync_full: bool) -> Result<Self, Error> {
        Self::open_with_hot_cache(dir, sync_full, HotCacheConfig::default())
    }

    pub fn open_with_hot_cache(
        dir: impl AsRef<Path>,
        sync_full: bool,
        hot_cfg: HotCacheConfig,
    ) -> Result<Self, Error> {
        let dir = dir.as_ref().to_path_buf();
        fs::create_dir_all(&dir).map_err(io_err)?;
        fs::create_dir_all(dir.join(SEGMENTS_DIR)).map_err(io_err)?;

        let manifest_path = dir.join(MANIFEST_FILE);
        let manifest: DiskManifest = if manifest_path.exists() {
            let bytes = fs::read(&manifest_path).map_err(io_err)?;
            serde_json::from_slice(&bytes).map_err(io_err)?
        } else {
            let m = DiskManifest {
                version: 2,
                generation: 0,
                next_seg_id: 1,
                embed_id: "nomic-embed-text/768".into(),
                tables: BTreeMap::new(),
                segments: Vec::new(),
            };
            let bytes = serde_json::to_vec_pretty(&m).map_err(io_err)?;
            fs::write(&manifest_path, bytes).map_err(io_err)?;
            m
        };

        let tomb_path = dir.join(TOMBSTONES_FILE);
        let tombstones: SegmentTombstones = if tomb_path.exists() {
            let bytes = fs::read(&tomb_path).map_err(io_err)?;
            rmp_serde::from_slice(&bytes).unwrap_or_default()
        } else {
            SegmentTombstones::default()
        };

        let mut segments: BTreeMap<String, Vec<Arc<DiskSegment>>> = BTreeMap::new();
        for seg_meta in &manifest.segments {
            let seg_dir = dir
                .join(SEGMENTS_DIR)
                .join(format!("seg_{:08}", seg_meta.seg_id));
            let opened = DiskSegment::open(&seg_dir, seg_meta.clone())?;
            segments
                .entry(seg_meta.table.clone())
                .or_default()
                .push(Arc::new(opened));
        }

        let wal_path = dir.join(WAL_FILE);
        let wal_file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(&wal_path)
            .map_err(io_err)?;

        let mut engine = Self {
            dir,
            manifest,
            segments,
            memtables: BTreeMap::new(),
            tombstones,
            hot_cache: Arc::new(HotTierCache::new(hot_cfg)),
            active_savepoint: None,
            overlay_epoch: 0,
            dirty_overlay: FxHashSet::default(),
            wal_file: Some(wal_file),
            wal_buf: Vec::with_capacity(16 * 1024),
            wal_bytes: 0,
            sync_full,
            read_only: false,
        };
        engine.replay_wal()?;
        Ok(engine)
    }

    /// Open a read-only `DiskEngine` handle (`mmap` shared OS pages + bounded L1 Hot Cache).
    pub fn open_read_only(dir: impl AsRef<Path>) -> Result<Self, Error> {
        Self::open_read_only_with_hot_cache(dir, HotCacheConfig::default())
    }

    pub fn open_read_only_with_hot_cache(
        dir: impl AsRef<Path>,
        hot_cfg: HotCacheConfig,
    ) -> Result<Self, Error> {
        let dir = dir.as_ref().to_path_buf();
        let manifest_path = dir.join(MANIFEST_FILE);
        if !manifest_path.exists() {
            return Err(Error::runtime(format!(
                "disk manifest not found in '{}'",
                dir.display()
            )));
        }
        let manifest: DiskManifest =
            serde_json::from_slice(&fs::read(&manifest_path).map_err(io_err)?).map_err(io_err)?;
        let tomb_path = dir.join(TOMBSTONES_FILE);
        let tombstones: SegmentTombstones = if tomb_path.exists() {
            rmp_serde::from_slice(&fs::read(&tomb_path).map_err(io_err)?).unwrap_or_default()
        } else {
            SegmentTombstones::default()
        };
        let mut segments: BTreeMap<String, Vec<Arc<DiskSegment>>> = BTreeMap::new();
        for seg_meta in &manifest.segments {
            let seg_dir = dir
                .join(SEGMENTS_DIR)
                .join(format!("seg_{:08}", seg_meta.seg_id));
            let opened = DiskSegment::open(&seg_dir, seg_meta.clone())?;
            segments
                .entry(seg_meta.table.clone())
                .or_default()
                .push(Arc::new(opened));
        }
        let mut engine = Self {
            dir: dir.clone(),
            manifest,
            segments,
            memtables: BTreeMap::new(),
            tombstones,
            hot_cache: Arc::new(HotTierCache::new(hot_cfg)),
            active_savepoint: None,
            overlay_epoch: 0,
            dirty_overlay: FxHashSet::default(),
            wal_file: None,
            wal_buf: Vec::new(),
            wal_bytes: 0,
            sync_full: false,
            read_only: true,
        };
        let wal_path = dir.join(WAL_FILE);
        if wal_path.exists() {
            let mut f = File::open(&wal_path).map_err(io_err)?;
            let _ = engine.replay_wal_from_file(&mut f, false)?;
        }
        Ok(engine)
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn is_read_only(&self) -> bool {
        self.read_only
    }

    /// Capture a zero-I/O point-in-time MVCC view of `self` (`O(#segments + MemTable)` in microseconds).
    pub fn snapshot_view(&self) -> Self {
        Self {
            dir: self.dir.clone(),
            manifest: self.manifest.clone(),
            segments: self.segments.clone(),
            memtables: self.memtables.clone(),
            tombstones: self.tombstones.clone(),
            hot_cache: Arc::new(HotTierCache::new(HotCacheConfig {
                enabled: false,
                max_bytes: 0,
                max_rows: 0,
                max_vec_queries: 0,
            })),
            active_savepoint: None,
            overlay_epoch: 0,
            dirty_overlay: self.dirty_overlay.clone(),
            wal_file: None,
            wal_buf: Vec::new(),
            wal_bytes: 0,
            sync_full: false,
            read_only: true,
        }
    }

    pub fn hot_cache_stats(&self) -> HotCacheStats {
        self.hot_cache.stats()
    }

    pub fn clear_hot_cache(&self) {
        self.hot_cache.clear_all();
    }

    pub fn set_sync_full(&mut self, sync_full: bool) {
        self.sync_full = sync_full;
    }

    pub fn flush_wal(&mut self) -> Result<(), Error> {
        if let Some(f) = self.wal_file.as_mut() {
            crate::persist::durable_sync(f)
                .or_else(|_| f.sync_data())
                .map_err(io_err)?;
        }
        Ok(())
    }

    /// Pre-warm up to `limit` live rows of `table` into the L1 in-memory HotTierCache.
    pub fn warm_table_into_hot_cache(
        &self,
        table: &str,
        limit: Option<usize>,
    ) -> Result<usize, Error> {
        let mut count = 0usize;
        self.scan_projected(table, None, |row| {
            if let Some(id) = row.get("id").and_then(Cell::text) {
                self.hot_cache.put_row(table, id, Arc::new(row.clone()));
                count += 1;
            }
            if let Some(max_n) = limit
                && count >= max_n
            {
                return Ok(false);
            }
            Ok(true)
        })?;
        Ok(count)
    }

    fn replay_wal(&mut self) -> Result<(), Error> {
        let Some(mut f) = self.wal_file.take() else {
            return Ok(());
        };
        let valid_end = self.replay_wal_from_file(&mut f, true)?;
        f.set_len(valid_end).map_err(io_err)?;
        f.seek(SeekFrom::End(0)).map_err(io_err)?;
        self.wal_bytes = valid_end;
        self.wal_file = Some(f);
        Ok(())
    }

    fn replay_wal_from_file(&mut self, f: &mut File, writable: bool) -> Result<u64, Error> {
        const MAX_WAL_FRAME_BYTES: usize = 256 * 1024 * 1024;
        f.seek(SeekFrom::Start(0)).map_err(io_err)?;
        let mut reader = BufReader::new(f);
        let mut valid_pos = 0u64;
        let mut header = [0u8; 12]; // b"LWAL" (4B) + len: u32 (4B) + crc32: u32 (4B)
        let mut payload = Vec::new();
        loop {
            match reader.read_exact(&mut header) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
                Err(e) => return Err(io_err(e)),
            }
            if &header[0..4] != b"LWAL" {
                break;
            }
            let payload_len = u32::from_le_bytes(header[4..8].try_into().unwrap()) as usize;
            if payload_len > MAX_WAL_FRAME_BYTES {
                break;
            }
            let expected_crc = u32::from_le_bytes(header[8..12].try_into().unwrap());
            payload.resize(payload_len, 0);
            if reader.read_exact(&mut payload).is_err() {
                break;
            }
            if crate::crc::hardware_crc32c(&payload) != expected_crc
                && crc32fast::hash(&payload) != expected_crc
            {
                break;
            }
            let Ok(frame) = rmp_serde::from_slice::<DiskWalFrame>(&payload) else {
                break;
            };
            if frame.generation > self.manifest.generation {
                for op in frame.ops {
                    self.apply_op_in_memory(op);
                }
                self.manifest.generation = frame.generation;
            }
            valid_pos += 12 + (payload_len as u64);
        }
        let _ = writable;
        Ok(valid_pos)
    }

    fn append_wal_ops(&mut self, generation: u64, ops: &[DiskWalOp]) -> Result<(), Error> {
        let Some(f) = self.wal_file.as_mut() else {
            return Ok(());
        };
        let frame = DiskWalFrameRef { generation, ops };
        self.wal_buf.clear();
        rmp_serde::encode::write(&mut self.wal_buf, &frame).map_err(io_err)?;
        let crc = crate::crc::hardware_crc32c(&self.wal_buf);

        let mut header = [0u8; 12];
        header[0..4].copy_from_slice(b"LWAL");
        header[4..8].copy_from_slice(&(self.wal_buf.len() as u32).to_le_bytes());
        header[8..12].copy_from_slice(&crc.to_le_bytes());

        f.write_all(&header).map_err(io_err)?;
        f.write_all(&self.wal_buf).map_err(io_err)?;
        if self.sync_full {
            crate::persist::durable_sync(f)
                .or_else(|_| f.sync_data())
                .map_err(io_err)?;
        }
        self.wal_bytes += 12 + (self.wal_buf.len() as u64);
        Ok(())
    }

    fn mark_segment_key_dead(&mut self, table: &str, id: &str) {
        let Some(segs) = self.segments.get(table) else {
            return;
        };
        let mut newly_dead = SmallVec::<[(u64, u32); 2]>::new();
        with_text_index_key(id, |kbuf| {
            for seg in segs {
                let sid = seg.meta.seg_id;
                let nrows = seg.meta.row_count;
                seg.pk_index.for_each_key(kbuf, |rid| {
                    if self.tombstones.mark_dead(sid, rid, nrows) {
                        newly_dead.push((sid, rid));
                    }
                    true
                });
            }
        });
        if let Some(sp) = self.active_savepoint.as_mut() {
            sp.added_tombstones.extend(newly_dead);
        }
    }

    #[inline]
    fn snapshot_tables_for_savepoint(&mut self) {
        if let Some(sp) = self.active_savepoint.as_mut()
            && sp.saved_tables.is_none()
        {
            sp.saved_tables = Some(self.manifest.tables.clone());
        }
    }

    #[inline]
    fn ensure_memtable_undo(&mut self, table: &str) {
        if let Some(sp) = self.active_savepoint.as_mut()
            && !sp.memtable_undos.contains_key(table)
        {
            let (existed_before, prev_rows_len) = match self.memtables.get(table) {
                Some(mt) => (true, mt.rows.len()),
                None => (false, 0),
            };
            sp.memtable_undos.insert(
                table.to_string(),
                MemTableUndo {
                    existed_before,
                    prev_rows_len,
                    prev_by_id: FxHashMap::default(),
                    added_dead_mem: Vec::new(),
                    prev_deleted_ids: FxHashMap::default(),
                },
            );
        }
    }

    fn ensure_schema_columns_for_rows(
        &mut self,
        table: &str,
        rows: &[Row],
    ) -> SmallVec<[String; 8]> {
        let needs_schema_mut = match self.manifest.tables.get(table) {
            None => true,
            Some(t) => rows.iter().any(|row| {
                row.iter().any(|(k, cell)| {
                    if matches!(cell, Cell::Null) {
                        return false;
                    }
                    match t.columns.iter().find(|c| c.name == *k) {
                        None => true,
                        Some(existing) => {
                            if let Cell::Vec(v) = cell
                                && !v.is_empty()
                            {
                                !matches!(
                                    existing.kind,
                                    DiskColKind::VecF32Sq8 { dim } if dim == v.len() as u32
                                )
                            } else {
                                false
                            }
                        }
                    }
                })
            }),
        };
        if needs_schema_mut {
            self.snapshot_tables_for_savepoint();
            if let Some(t) = self.manifest.tables.get_mut(table) {
                for row in rows {
                    for (k, cell) in row {
                        if matches!(cell, Cell::Null) {
                            continue;
                        }
                        if let Some(existing) = t.columns.iter_mut().find(|c| c.name == *k) {
                            if let Cell::Vec(v) = cell
                                && !v.is_empty()
                            {
                                existing.kind = DiskColKind::VecF32Sq8 {
                                    dim: v.len() as u32,
                                };
                            }
                        } else {
                            let kind = match cell {
                                Cell::Int(_) => DiskColKind::Int64,
                                Cell::Float(_) => DiskColKind::Float64,
                                Cell::Bool(_) => DiskColKind::Bool,
                                Cell::Time(_) => DiskColKind::Time64,
                                Cell::Vec(v) => DiskColKind::VecF32Sq8 {
                                    dim: v.len() as u32,
                                },
                                Cell::Text(_) | Cell::SmallText(_) | Cell::Null => {
                                    DiskColKind::TextZstd
                                }
                            };
                            t.columns.push(DiskColSchema {
                                name: k.clone(),
                                kind,
                            });
                        }
                    }
                }
                t.indexed_fields.iter().cloned().collect()
            } else {
                let inferred = infer_schema_from_rows(table, rows);
                let ixs: SmallVec<[String; 8]> = inferred.indexed_fields.iter().cloned().collect();
                self.manifest.tables.insert(table.to_string(), inferred);
                ixs
            }
        } else {
            self.manifest.tables[table]
                .indexed_fields
                .iter()
                .cloned()
                .collect()
        }
    }

    fn apply_op_in_memory(&mut self, op: DiskWalOp) {
        match op {
            DiskWalOp::EnsureTable { schema } => {
                self.snapshot_tables_for_savepoint();
                self.manifest
                    .tables
                    .entry(schema.name.clone())
                    .and_modify(|existing| {
                        for col in &schema.columns {
                            if !existing.columns.iter().any(|c| c.name == col.name) {
                                existing.columns.push(col.clone());
                            }
                        }
                        for idx in &schema.indexed_fields {
                            if !existing.indexed_fields.contains(idx) {
                                existing.indexed_fields.push(idx.clone());
                            }
                        }
                    })
                    .or_insert(schema);
            }
            DiskWalOp::EnsureIndex { table, field } => {
                self.ensure_memtable_undo(&table);
                if self
                    .manifest
                    .tables
                    .get(&table)
                    .is_some_and(|t| !t.indexed_fields.contains(&field))
                {
                    self.snapshot_tables_for_savepoint();
                    if let Some(t) = self.manifest.tables.get_mut(&table) {
                        t.indexed_fields.push(field.clone());
                    }
                }
                if let Some(mt) = self.memtables.get_mut(&table) {
                    mt.rebuild_sec_for_field(&field);
                }
            }
            DiskWalOp::UpsertBatch { table, rows } => {
                self.ensure_memtable_undo(&table);
                let indexed_fields = self.ensure_schema_columns_for_rows(&table, &rows);
                for row in &rows {
                    if let Some(id) = row.get("id").and_then(Cell::text) {
                        self.mark_segment_key_dead(&table, id);
                        self.hot_cache.invalidate_row(&table, id);
                    }
                }
                let mt = self.memtables.entry(table.clone()).or_default();
                let mut undo = self
                    .active_savepoint
                    .as_mut()
                    .and_then(|sp| sp.memtable_undos.get_mut(&table));
                for row in rows {
                    mt.upsert_row(row, &indexed_fields, undo.as_deref_mut());
                }
            }
            DiskWalOp::DeleteIds { table, ids } => {
                self.ensure_memtable_undo(&table);
                for id in &ids {
                    self.mark_segment_key_dead(&table, id);
                    self.hot_cache.invalidate_row(&table, id);
                }
                let mt = self.memtables.entry(table.clone()).or_default();
                let mut undo = self
                    .active_savepoint
                    .as_mut()
                    .and_then(|sp| sp.memtable_undos.get_mut(&table));
                for id in ids {
                    mt.delete_id(&id, undo.as_deref_mut());
                }
            }
        }
    }

    /// Begin an interactive ACID transaction savepoint.
    pub fn begin_savepoint(&mut self) -> Result<(), Error> {
        if self.read_only {
            return Err(Error::runtime(
                "cannot begin transaction on read-only DiskEngine",
            ));
        }
        if self.active_savepoint.is_some() {
            return Err(Error::runtime(
                "nested DiskEngine savepoints are not supported",
            ));
        }
        self.active_savepoint = Some(DiskSavepoint {
            saved_tables: None,
            memtable_undos: FxHashMap::default(),
            added_tombstones: Vec::new(),
            pending_wal_ops: Vec::new(),
            saved_dirty_overlay: self.dirty_overlay.clone(),
        });
        Ok(())
    }

    /// Start an in-memory overlay savepoint even when `self.read_only` (`cold: true` tier).
    pub fn begin_mem_savepoint(&mut self) {
        if self.active_savepoint.is_some() {
            return;
        }
        self.active_savepoint = Some(DiskSavepoint {
            saved_tables: None,
            memtable_undos: FxHashMap::default(),
            added_tombstones: Vec::new(),
            pending_wal_ops: Vec::new(),
            saved_dirty_overlay: self.dirty_overlay.clone(),
        });
    }

    pub fn discard_mem_savepoint(&mut self) {
        self.active_savepoint = None;
    }

    pub fn apply_mem_overlay_op(&mut self, op: DiskWalOp) {
        let tbl = match &op {
            DiskWalOp::EnsureTable { schema } => Some(schema.name.clone()),
            DiskWalOp::EnsureIndex { table, .. }
            | DiskWalOp::UpsertBatch { table, .. }
            | DiskWalOp::DeleteIds { table, .. } => Some(table.clone()),
        };
        if let Some(t) = tbl {
            self.dirty_overlay.insert(t);
        }
        // Overlay writes do not advance `manifest.generation`, so cached query results
        // need their own epoch to go stale.
        self.overlay_epoch = self.overlay_epoch.wrapping_add(1);
        self.apply_op_in_memory(op);
    }

    /// Generation key for cached query results: durable generation mixed with the overlay epoch.
    #[inline]
    fn query_cache_gen(&self) -> u64 {
        self.manifest
            .generation
            .wrapping_add(self.overlay_epoch.wrapping_mul(0x9E37_79B9_7F4A_7C15))
    }

    #[inline]
    pub fn has_dirty_overlay(&self) -> bool {
        !self.dirty_overlay.is_empty()
    }

    pub(crate) fn drain_dirty_overlays(
        &mut self,
    ) -> (BTreeMap<String, Vec<Row>>, SegmentTombstones) {
        let mut out = BTreeMap::new();
        let tables: Vec<String> = self.dirty_overlay.drain().collect();
        for t in tables {
            if let Some(mt) = self.memtables.get_mut(&t) {
                let rows = mt.drain_live_rows();
                if !rows.is_empty() {
                    out.insert(t, rows);
                }
            }
        }
        (out, self.tombstones.clone())
    }

    pub(crate) fn merge_overlay_and_flush(
        &mut self,
        mem_rows: BTreeMap<String, Vec<Row>>,
        tombstones: &SegmentTombstones,
    ) -> Result<(), Error> {
        self.tombstones.merge_from(tombstones);
        for (table, rows) in mem_rows {
            if rows.is_empty() {
                continue;
            }
            let _ = self.ensure_schema_columns_for_rows(&table, &rows);
            for chunk in rows.chunks(65_536) {
                self.write_bulk_segment(&table, chunk)?;
            }
        }
        self.dirty_overlay.clear();
        Ok(())
    }

    pub fn has_active_savepoint(&self) -> bool {
        self.active_savepoint.is_some()
    }

    pub fn commit_savepoint(&mut self) -> Result<u64, Error> {
        let Some(sp) = self.active_savepoint.take() else {
            return Ok(self.manifest.generation);
        };
        if sp.pending_wal_ops.is_empty() {
            return Ok(self.manifest.generation);
        }
        let next_gen = self.manifest.generation + 1;
        self.append_wal_ops(next_gen, &sp.pending_wal_ops)?;
        self.manifest.generation = next_gen;

        let total_mem_rows: usize = self.memtables.values().map(MemTable::live_count).sum();
        if total_mem_rows >= 32_768 || self.wal_bytes >= 64 * 1024 * 1024 {
            self.checkpoint()?;
        }
        Ok(next_gen)
    }

    pub fn rollback_savepoint(&mut self) {
        let Some(sp) = self.active_savepoint.take() else {
            return;
        };
        self.overlay_epoch = self.overlay_epoch.wrapping_add(1);
        self.dirty_overlay = sp.saved_dirty_overlay;
        if let Some(saved_tables) = sp.saved_tables {
            self.manifest.tables = saved_tables;
        }
        for (tbl, undo) in sp.memtable_undos {
            if !undo.existed_before {
                self.memtables.remove(&tbl);
                continue;
            }
            let remove_empty = if let Some(mt) = self.memtables.get_mut(&tbl) {
                mt.rows.truncate(undo.prev_rows_len);
                mt.dead_mem.retain(|&idx| idx < undo.prev_rows_len);
                for idx in undo.added_dead_mem {
                    mt.dead_mem.remove(&idx);
                }
                for (id, prev_idx) in undo.prev_by_id {
                    match prev_idx {
                        Some(i) => {
                            mt.by_id.insert(id, i);
                        }
                        None => {
                            mt.by_id.remove(&id);
                        }
                    }
                }
                for (id, was_del) in undo.prev_deleted_ids {
                    if was_del {
                        mt.deleted_ids.insert(id);
                    } else {
                        mt.deleted_ids.remove(&id);
                    }
                }
                mt.sec.clear();
                if let Some(schema) = self.manifest.tables.get(&tbl) {
                    for field in &schema.indexed_fields {
                        if field != "id" {
                            mt.rebuild_sec_for_field(field);
                        }
                    }
                }
                mt.rows.is_empty() && mt.deleted_ids.is_empty()
            } else {
                false
            };
            if remove_empty {
                self.memtables.remove(&tbl);
            }
        }
        for (sid, rid) in sp.added_tombstones {
            self.tombstones.unmark_dead(sid, rid);
        }
        self.hot_cache.clear_all();
    }

    /// Commit a batch of ACID operations atomically to WAL + MemTable (or stage into active savepoint).
    pub fn commit_ops(&mut self, ops: Vec<DiskWalOp>) -> Result<u64, Error> {
        if self.read_only {
            return Err(Error::runtime("cannot mutate read-only DiskEngine"));
        }
        if ops.is_empty() {
            return Ok(self.manifest.generation);
        }
        if self.active_savepoint.is_some() {
            for op in ops {
                if let Some(sp) = self.active_savepoint.as_mut() {
                    sp.pending_wal_ops.push(op.clone());
                }
                self.apply_op_in_memory(op);
            }
            return Ok(self.manifest.generation + 1);
        }
        let next_gen = self.manifest.generation + 1;
        self.append_wal_ops(next_gen, &ops)?;
        for op in ops {
            self.apply_op_in_memory(op);
        }
        self.manifest.generation = next_gen;

        let total_mem_rows: usize = self.memtables.values().map(MemTable::live_count).sum();
        if total_mem_rows >= 32_768 || self.wal_bytes >= 64 * 1024 * 1024 {
            self.checkpoint()?;
        }
        Ok(next_gen)
    }

    /// Incremental checkpoint: flushes ONLY dirty MemTable rows into new immutable mmap segments
    /// and persists updated tombstones + manifest, then truncates WAL to 0 bytes.
    pub fn checkpoint(&mut self) -> Result<(), Error> {
        if self.read_only {
            return Ok(());
        }
        let mut new_metas = Vec::new();
        let table_names: Vec<String> = self.memtables.keys().cloned().collect();
        for tname in table_names {
            let Some(mt) = self.memtables.get_mut(&tname) else {
                continue;
            };
            let mut rows = mt.drain_live_rows();
            if rows.is_empty() {
                continue;
            }
            let schema = self
                .manifest
                .tables
                .get(&tname)
                .cloned()
                .unwrap_or_else(|| infer_schema_from_rows(&tname, &rows));
            // A clustered table gets its delta segment written in key order right away.
            if let Some(key) = schema.cluster_by.as_deref() {
                sort_rows_by_cluster_key(&mut rows, key);
            }
            self.manifest
                .tables
                .entry(tname.clone())
                .or_insert_with(|| schema.clone());

            let seg_id = self.manifest.next_seg_id;
            self.manifest.next_seg_id += 1;
            let meta = write_segment_to_disk(&self.dir, seg_id, &schema, &rows)?;
            let seg_dir = self.dir.join(SEGMENTS_DIR).join(format!("seg_{seg_id:08}"));
            let opened = DiskSegment::open(&seg_dir, meta.clone())?;
            self.segments
                .entry(tname)
                .or_default()
                .push(Arc::new(opened));
            new_metas.push(meta);
        }

        self.manifest.segments.extend(new_metas);
        self.dirty_overlay.clear();
        self.save_tombstones_and_manifest()?;

        if let Some(f) = self.wal_file.as_mut() {
            f.set_len(0).map_err(io_err)?;
            f.seek(SeekFrom::Start(0)).map_err(io_err)?;
            if self.sync_full {
                crate::persist::durable_sync(f)
                    .or_else(|_| f.sync_all())
                    .map_err(io_err)?;
            }
        }
        self.wal_bytes = 0;
        if self.active_savepoint.is_none() {
            let _ = self.maybe_merge_small_delta_segments();
        }
        Ok(())
    }

    fn maybe_merge_small_delta_segments(&mut self) -> Result<CompactionReport, Error> {
        let mut total = CompactionReport::default();
        if self.read_only || self.active_savepoint.is_some() {
            return Ok(total);
        }
        let table_names: Vec<String> = self.segments.keys().cloned().collect();
        for tname in table_names {
            let Some(segs) = self.segments.get(&tname) else {
                continue;
            };
            let small_indices: Vec<usize> = segs
                .iter()
                .enumerate()
                .filter(|(_, s)| s.meta.row_count < 8_192)
                .map(|(i, _)| i)
                .collect();
            if small_indices.len() < 10 {
                continue;
            }
            let Some(schema) = self.manifest.tables.get(&tname).cloned() else {
                continue;
            };

            let mut old_seg_ids: FxHashSet<u64> = FxHashSet::default();
            let mut dead_reclaimed = 0usize;
            let mut merged_rows: Vec<Row> = Vec::new();
            for &idx in &small_indices {
                let seg = &segs[idx];
                let sid = seg.meta.seg_id;
                old_seg_ids.insert(sid);
                dead_reclaimed += self.tombstones.dead_count(sid) as usize;
                for rid in 0..seg.meta.row_count {
                    if self.tombstones.is_dead(sid, rid) {
                        continue;
                    }
                    merged_rows.push(seg.materialize_row(rid)?);
                }
            }

            if let Some(key) = schema.cluster_by.as_deref() {
                sort_rows_by_cluster_key(&mut merged_rows, key);
            }
            let mut new_segs: Vec<Arc<DiskSegment>> = Vec::new();
            let mut new_metas: Vec<SegmentMeta> = Vec::new();
            for chunk in merged_rows.chunks(65_536) {
                let seg_id = self.manifest.next_seg_id;
                self.manifest.next_seg_id += 1;
                let meta = write_segment_to_disk(&self.dir, seg_id, &schema, chunk)?;
                let seg_dir = self.dir.join(SEGMENTS_DIR).join(format!("seg_{seg_id:08}"));
                let opened = DiskSegment::open(&seg_dir, meta.clone())?;
                new_segs.push(Arc::new(opened));
                new_metas.push(meta);
            }

            if let Some(cur_segs) = self.segments.get_mut(&tname) {
                cur_segs.retain(|s| !old_seg_ids.contains(&s.meta.seg_id));
                cur_segs.extend(new_segs);
            }
            self.manifest
                .segments
                .retain(|m| !old_seg_ids.contains(&m.seg_id));
            self.manifest.segments.extend(new_metas.iter().cloned());
            for &sid in &old_seg_ids {
                self.tombstones.words_by_seg.remove(&sid);
                self.tombstones.dead_counts.remove(&sid);
            }
            self.manifest.generation += 1;
            self.hot_cache.clear_all();
            self.save_tombstones_and_manifest()?;
            for &sid in &old_seg_ids {
                let seg_dir = self.dir.join(SEGMENTS_DIR).join(format!("seg_{sid:08}"));
                let _ = fs::remove_dir_all(&seg_dir);
            }
            total.tables_compacted += 1;
            total.old_segments_removed += old_seg_ids.len();
            total.new_segments_written += new_metas.len();
            total.dead_rows_reclaimed += dead_reclaimed;
        }
        Ok(total)
    }

    /// Compact `table` when it is worth it (dead rows, small or under-indexed segments).
    /// A table with a cluster key is rewritten in key order whenever it is compacted.
    pub fn compact_table(&mut self, table: &str, force: bool) -> Result<CompactionReport, Error> {
        let key = self
            .manifest
            .tables
            .get(table)
            .and_then(|t| t.cluster_by.clone());
        self.compact_table_impl(table, force, key.as_deref(), false)
    }

    /// Declare (or clear) the column that compaction orders `table` by. Existing segments
    /// are left as they are until the next compaction or [`Self::compact_table_clustered`].
    pub fn set_cluster_key(&mut self, table: &str, cluster_by: Option<&str>) -> Result<(), Error> {
        if self.read_only {
            return Err(Error::runtime("cannot alter read-only DiskEngine"));
        }
        self.set_cluster_key_in_memory(table, cluster_by)?;
        self.save_tombstones_and_manifest()
    }

    /// Validate and record the cluster key in the in-memory manifest only (the owner of a
    /// read-only view persists it through its own checkpoint).
    pub(crate) fn set_cluster_key_in_memory(
        &mut self,
        table: &str,
        cluster_by: Option<&str>,
    ) -> Result<(), Error> {
        let Some(schema) = self.manifest.tables.get_mut(table) else {
            return Err(Error::runtime(format!("unknown disk table: {table}")));
        };
        if let Some(key) = cluster_by
            && !schema.columns.iter().any(|c| c.name == key)
        {
            return Err(Error::runtime(format!(
                "cluster key {key} is not a column of {table}"
            )));
        }
        schema.cluster_by = cluster_by.map(str::to_string);
        Ok(())
    }

    /// Rewrite every segment of `table` with its live rows ordered by `cluster_by`
    /// (ascending, missing values last, ties in their previous order). Clustering makes the
    /// per-block min/max ranges of that column disjoint, so range filters on it skip most
    /// blocks. Only sort keys are held in memory; rows are re-read in sorted order.
    pub fn compact_table_clustered(
        &mut self,
        table: &str,
        cluster_by: &str,
    ) -> Result<CompactionReport, Error> {
        self.compact_table_impl(table, true, Some(cluster_by), true)
    }

    /// `always` rewrites the table even when nothing else calls for compaction.
    fn compact_table_impl(
        &mut self,
        table: &str,
        force: bool,
        cluster_by: Option<&str>,
        always: bool,
    ) -> Result<CompactionReport, Error> {
        if self.read_only {
            return Err(Error::runtime("cannot compact read-only DiskEngine"));
        }
        if self.active_savepoint.is_some() {
            return Err(Error::runtime(
                "cannot compact DiskEngine while a transaction savepoint is active",
            ));
        }
        self.checkpoint()?;

        let Some(segs) = self.segments.get(table) else {
            return Ok(CompactionReport::default());
        };
        if segs.is_empty() {
            return Ok(CompactionReport::default());
        }
        let Some(schema) = self.manifest.tables.get(table).cloned() else {
            return Ok(CompactionReport::default());
        };

        let mut total_rows = 0usize;
        let mut dead_rows = 0usize;
        let mut small_segs = 0usize;
        let mut missing_ivf = false;
        let mut missing_sec = false;
        let mut missing_dict = false;

        for seg in segs.iter() {
            let rc = seg.meta.row_count as usize;
            total_rows += rc;
            dead_rows += self.tombstones.dead_count(seg.meta.seg_id) as usize;
            if rc < 16_384 {
                small_segs += 1;
            }
            for col in &schema.columns {
                if let DiskColKind::VecF32Sq8 { dim } = col.kind
                    && dim > 0
                    && rc >= IVF_MIN_SEGMENT_VECS
                    && seg.vec_col(&col.name).is_some_and(|vc| vc.ivf.is_none())
                {
                    missing_ivf = true;
                }
            }
            if force && rc >= DICT_MIN_SEGMENT_ROWS {
                let sample_n = rc.min(128);
                for (cname, mcol) in &seg.cols {
                    if cname == "id" || cname == "content" {
                        continue;
                    }
                    if let MmapCol::Text(tc) = mcol {
                        let mut distinct: FxHashSet<String> = FxHashSet::default();
                        for r in 0..sample_n {
                            let _ = tc.with_str(r as u32, |opt| {
                                if let Some(s) = opt {
                                    distinct.insert(s.to_string());
                                }
                            });
                        }
                        if distinct.len() * 2 <= sample_n {
                            missing_dict = true;
                            break;
                        }
                    }
                }
            }
            for ix in &schema.indexed_fields {
                if ix != "id" && !seg.sec_indexes.contains_key(ix) {
                    missing_sec = true;
                }
            }
        }

        let should_compact = if force {
            dead_rows > 0
                || segs.len() > 1 && small_segs > 0
                || missing_ivf
                || missing_sec
                || missing_dict
        } else {
            (total_rows > 0 && dead_rows * 5 >= total_rows) || small_segs >= 2 || missing_sec
        };

        if !should_compact && !always {
            return Ok(CompactionReport::default());
        }

        const TARGET_SEG_ROWS: usize = 65_536;
        let old_seg_ids: Vec<u64> = segs.iter().map(|s| s.meta.seg_id).collect();
        let old_segs_snapshot: Vec<Arc<DiskSegment>> = segs.clone();

        let mut new_segs: Vec<Arc<DiskSegment>> = Vec::new();
        let mut new_metas: Vec<SegmentMeta> = Vec::new();
        let mut batch: Vec<Row> = Vec::with_capacity(TARGET_SEG_ROWS.min(total_rows.max(1)));

        // Live rows in output order: segment order, or sorted by the cluster key.
        let mut order: Vec<(u32, u32)> = Vec::with_capacity(total_rows.saturating_sub(dead_rows));
        for (si, seg) in old_segs_snapshot.iter().enumerate() {
            let sid = seg.meta.seg_id;
            for rid in 0..seg.meta.row_count {
                if !self.tombstones.is_dead(sid, rid) {
                    order.push((si as u32, rid));
                }
            }
        }
        if let Some(key_field) = cluster_by {
            let mut keys: Vec<Cell> = Vec::with_capacity(order.len());
            for &(si, rid) in &order {
                keys.push(old_segs_snapshot[si as usize].get_cell(rid, key_field)?);
            }
            let mut perm: Vec<u32> = (0..order.len() as u32).collect();
            perm.sort_by(|&a, &b| {
                let (ka, kb) = (&keys[a as usize], &keys[b as usize]);
                match (matches!(ka, Cell::Null), matches!(kb, Cell::Null)) {
                    (true, true) => std::cmp::Ordering::Equal,
                    (true, false) => std::cmp::Ordering::Greater,
                    (false, true) => std::cmp::Ordering::Less,
                    (false, false) => crate::exec::cmp_sort(ka, kb),
                }
            });
            order = perm.into_iter().map(|i| order[i as usize]).collect();
        }
        for (si, rid) in order {
            batch.push(old_segs_snapshot[si as usize].materialize_row(rid)?);
            if batch.len() >= TARGET_SEG_ROWS {
                let seg_id = self.manifest.next_seg_id;
                self.manifest.next_seg_id += 1;
                let meta = write_segment_to_disk(&self.dir, seg_id, &schema, &batch)?;
                let seg_dir = self.dir.join(SEGMENTS_DIR).join(format!("seg_{seg_id:08}"));
                let opened = DiskSegment::open(&seg_dir, meta.clone())?;
                new_segs.push(Arc::new(opened));
                new_metas.push(meta);
                batch.clear();
            }
        }
        if !batch.is_empty() {
            let seg_id = self.manifest.next_seg_id;
            self.manifest.next_seg_id += 1;
            let meta = write_segment_to_disk(&self.dir, seg_id, &schema, &batch)?;
            let seg_dir = self.dir.join(SEGMENTS_DIR).join(format!("seg_{seg_id:08}"));
            let opened = DiskSegment::open(&seg_dir, meta.clone())?;
            new_segs.push(Arc::new(opened));
            new_metas.push(meta);
            batch.clear();
        }

        let old_id_set: FxHashSet<u64> = old_seg_ids.iter().copied().collect();
        let new_count = new_metas.len();
        if new_segs.is_empty() {
            self.segments.remove(table);
        } else {
            self.segments.insert(table.to_string(), new_segs);
        }
        self.manifest
            .segments
            .retain(|m| !old_id_set.contains(&m.seg_id));
        self.manifest.segments.extend(new_metas);
        for &sid in &old_seg_ids {
            self.tombstones.words_by_seg.remove(&sid);
            self.tombstones.dead_counts.remove(&sid);
        }
        self.manifest.generation += 1;
        self.hot_cache.clear_all();
        self.save_tombstones_and_manifest()?;

        drop(old_segs_snapshot);
        for sid in &old_seg_ids {
            let seg_dir = self.dir.join(SEGMENTS_DIR).join(format!("seg_{sid:08}"));
            let _ = fs::remove_dir_all(&seg_dir);
        }

        Ok(CompactionReport {
            tables_compacted: 1,
            old_segments_removed: old_seg_ids.len(),
            new_segments_written: new_count,
            dead_rows_reclaimed: dead_rows,
        })
    }

    pub fn compact_all(&mut self, force: bool) -> Result<CompactionReport, Error> {
        self.checkpoint()?;
        let tables: Vec<String> = self.manifest.tables.keys().cloned().collect();
        let mut agg = CompactionReport::default();
        for t in tables {
            let r = self.compact_table(&t, force)?;
            agg.tables_compacted += r.tables_compacted;
            agg.old_segments_removed += r.old_segments_removed;
            agg.new_segments_written += r.new_segments_written;
            agg.dead_rows_reclaimed += r.dead_rows_reclaimed;
        }
        Ok(agg)
    }

    pub fn vacuum(&mut self) -> Result<CompactionReport, Error> {
        self.compact_all(true)
    }

    pub fn write_bulk_segment(&mut self, table: &str, rows: &[Row]) -> Result<(), Error> {
        if rows.is_empty() {
            return Ok(());
        }
        let schema = self
            .manifest
            .tables
            .get(table)
            .cloned()
            .unwrap_or_else(|| infer_schema_from_rows(table, rows));
        self.manifest
            .tables
            .entry(table.to_string())
            .or_insert_with(|| schema.clone());

        let seg_id = self.manifest.next_seg_id;
        self.manifest.next_seg_id += 1;
        let meta = write_segment_to_disk(&self.dir, seg_id, &schema, rows)?;
        let seg_dir = self.dir.join(SEGMENTS_DIR).join(format!("seg_{seg_id:08}"));
        let opened = DiskSegment::open(&seg_dir, meta.clone())?;
        self.segments
            .entry(table.to_string())
            .or_default()
            .push(Arc::new(opened));
        self.manifest.segments.push(meta);
        self.manifest.generation += 1;
        Ok(())
    }

    /// Append the rows of a [`RecordBatch`](crate::batch::RecordBatch) as one new segment,
    /// encoding every column straight from the batch (no per-row maps are built).
    ///
    /// This is a bulk-load path, not an upsert: the batch needs an `id` column whose values
    /// are present, distinct, and not already live in `table`; otherwise nothing is written.
    /// Like [`Self::write_bulk_segment`], the manifest is updated in memory only; call
    /// [`Self::save_tombstones_and_manifest`] (or checkpoint) to make the segment durable.
    pub fn write_bulk_segment_batch(
        &mut self,
        table: &str,
        batch: &crate::batch::RecordBatch,
    ) -> Result<(), Error> {
        let n = batch.n();
        if n == 0 {
            return Ok(());
        }
        if batch.cols.iter().any(|c| c.len() != n) {
            return Err(Error::runtime("bulk batch: columns have different lengths"));
        }
        let Some(id_pos) = batch.names.iter().position(|c| c == "id") else {
            return Err(Error::runtime("bulk batch: missing `id` column"));
        };
        let mut seen: FxHashSet<&str> = FxHashSet::default();
        seen.reserve(n);
        let id_col = &batch.cols[id_pos];
        for i in 0..n {
            let Some(id) = id_col.cell_ref(i).and_then(Cell::text) else {
                return Err(Error::runtime("bulk batch: `id` must be text and present"));
            };
            if !seen.insert(id) || self.exists_by_id(table, id) {
                return Err(Error::runtime(format!("bulk batch: duplicate id: {id}")));
            }
        }
        let schema = match self.manifest.tables.get(table) {
            Some(existing) => {
                // Columns the table has not seen yet are added to its schema.
                let mut schema = existing.clone();
                for inferred in infer_schema_from_batch(table, batch).columns {
                    if !schema.columns.iter().any(|c| c.name == inferred.name) {
                        schema.columns.push(inferred);
                    }
                }
                schema
            }
            None => infer_schema_from_batch(table, batch),
        };
        self.manifest.tables.insert(table.to_string(), schema.clone());

        let seg_id = self.manifest.next_seg_id;
        self.manifest.next_seg_id += 1;
        let meta = segment::write_segment_from(
            &self.dir,
            seg_id,
            &schema,
            segment::SegSrc::Batch(batch),
        )?;
        let seg_dir = self.dir.join(SEGMENTS_DIR).join(format!("seg_{seg_id:08}"));
        let opened = DiskSegment::open(&seg_dir, meta.clone())?;
        self.segments
            .entry(table.to_string())
            .or_default()
            .push(Arc::new(opened));
        self.manifest.segments.push(meta);
        self.manifest.generation += 1;
        self.hot_cache.clear_all();
        Ok(())
    }

    pub fn save_tombstones_and_manifest(&self) -> Result<(), Error> {
        let tomb_tmp = self.dir.join("tombstones.bin.tmp");
        let tomb_bytes = rmp_serde::to_vec(&self.tombstones).map_err(io_err)?;
        fs::write(&tomb_tmp, tomb_bytes).map_err(io_err)?;
        fs::rename(&tomb_tmp, self.dir.join(TOMBSTONES_FILE)).map_err(io_err)?;

        let man_tmp = self.dir.join("manifest.json.tmp");
        let man_bytes = serde_json::to_vec_pretty(&self.manifest).map_err(io_err)?;
        fs::write(&man_tmp, man_bytes).map_err(io_err)?;
        fs::rename(&man_tmp, self.dir.join(MANIFEST_FILE)).map_err(io_err)?;
        Ok(())
    }

    // ========================================================================
    // O(1) Point Reads, Counts, Scans & Index Seeks
    // ========================================================================

    pub fn has_table(&self, table: &str) -> bool {
        self.manifest.tables.contains_key(table)
    }

    /// Exact live row count in O(#segments).
    pub fn count_table(&self, table: &str) -> usize {
        let mem_live = self.memtables.get(table).map_or(0, MemTable::live_count);
        let seg_live: usize = self
            .segments
            .get(table)
            .map(|segs| {
                segs.iter()
                    .map(|s| {
                        (s.meta
                            .row_count
                            .saturating_sub(self.tombstones.dead_count(s.meta.seg_id)))
                            as usize
                    })
                    .sum()
            })
            .unwrap_or(0);
        mem_live + seg_live
    }

    pub fn exists_by_id(&self, table: &str, id: &str) -> bool {
        if let Some(mt) = self.memtables.get(table) {
            if mt.deleted_ids.contains(id) {
                return false;
            }
            if mt.by_id.contains_key(id) {
                return true;
            }
        }
        let Some(segs) = self.segments.get(table) else {
            return false;
        };
        with_text_index_key(id, |kbuf| {
            for seg in segs.iter().rev() {
                let sid = seg.meta.seg_id;
                let mut found = false;
                seg.pk_index.for_each_key(kbuf, |rid| {
                    if !self.tombstones.is_dead(sid, rid) {
                        found = true;
                        return false;
                    }
                    true
                });
                if found {
                    return true;
                }
            }
            false
        })
    }

    /// Map every segment of `table` to a partial result, returned in segment order.
    /// With the `parallel` feature, tables of at least [`PAR_SEGMENT_MIN_ROWS`] rows spread
    /// over several segments are scanned on the rayon pool; merging the ordered partials
    /// afterwards keeps the result identical to the sequential scan.
    fn map_segments<T: Send>(
        &self,
        table: &str,
        f: impl Fn(&DiskSegment) -> Result<T, Error> + Sync,
    ) -> Result<Vec<T>, Error> {
        let Some(segs) = self.segments.get(table) else {
            return Ok(Vec::new());
        };
        #[cfg(feature = "parallel")]
        {
            let rows: u64 = segs.iter().map(|s| u64::from(s.meta.row_count)).sum();
            if segs.len() > 1 && rows >= PAR_SEGMENT_MIN_ROWS {
                use rayon::prelude::*;
                return segs.par_iter().map(|seg| f(seg)).collect();
            }
        }
        segs.iter().map(|seg| f(seg)).collect()
    }

    /// Count rows matching a numeric/time comparison predicate (with optional `eq_field = eq_val`
    /// dictionary filter) directly via `FixedZoneMap` block skipping without materializing `Row`s.
    pub fn count_cmp_and_optional_eq(
        &self,
        table: &str,
        cmp_field: &str,
        op: crate::ast::CmpOp,
        rhs: &Cell,
        eq_filter: Option<(&str, &str)>,
        full_pred: &crate::ast::Pred,
        now_ms: i64,
    ) -> Result<Option<usize>, Error> {
        let (rhs_i64, rhs_f64) = match rhs {
            Cell::Int(i) | Cell::Time(i) => (Some(*i), Some(*i as f64)),
            Cell::Float(f) => (None, Some(*f)),
            _ => return Ok(None),
        };
        let partials = self.map_segments(table, |seg| -> Result<Option<usize>, Error> {
            let Some(fc) = seg.fixed_col(cmp_field) else {
                return Ok(None);
            };
            let opt_dict = match eq_filter {
                None => None,
                Some((eq_field, eq_val)) => match seg.col(eq_field) {
                    Some(MmapCol::Dict(dc)) => {
                        let Some(code) = dc.find_code(eq_val) else {
                            // Segment dictionary doesn't contain eq_val: 0 rows can match.
                            return Ok(Some(0));
                        };
                        Some((dc, code))
                    }
                    _ => return Ok(None),
                },
            };
            let sid = seg.meta.seg_id;
            let seg_all_live = self.tombstones.dead_count(sid) == 0;
            Ok(fc.count_cmp_with_opt_dict_eq(
                op,
                rhs_i64,
                rhs_f64,
                opt_dict,
                seg_all_live,
                |rid| !self.tombstones.is_dead(sid, rid),
            ))
        })?;
        let mut total = 0usize;
        for p in partials {
            let Some(n) = p else {
                return Ok(None);
            };
            total += n;
        }
        if let Some(mt) = self.memtables.get(table) {
            for &idx in mt.by_id.values() {
                if !mt.dead_mem.contains(&idx) && crate::exec::eval_pred(full_pred, &mt.rows[idx], now_ms) {
                    total += 1;
                }
            }
        }
        Ok(Some(total))
    }

    /// Count live rows whose `field` string contains `needle` using block-level SIMD `memmem`.
    pub fn count_text_contains(
        &self,
        table: &str,
        field: &str,
        needle: &str,
    ) -> Result<Option<usize>, Error> {
        let partials = self.map_segments(table, |seg| -> Result<Option<usize>, Error> {
            let sid = seg.meta.seg_id;
            let has_dead = self.tombstones.dead_count(sid) > 0;
            Ok(match seg.col(field) {
                Some(MmapCol::Text(tc)) => Some(tc.count_contains_live(needle.as_bytes(), |rid| {
                    !has_dead || !self.tombstones.is_dead(sid, rid)
                })?),
                Some(MmapCol::Dict(dc)) => Some(dc.count_contains_live(needle.as_bytes(), |rid| {
                    !has_dead || !self.tombstones.is_dead(sid, rid)
                })),
                None => Some(0),
                _ => None,
            })
        })?;
        let mut total = 0usize;
        for p in partials {
            let Some(n) = p else {
                return Ok(None);
            };
            total += n;
        }
        if let Some(mt) = self.memtables.get(table) {
            for &idx in mt.by_id.values() {
                if !mt.dead_mem.contains(&idx)
                    && mt.rows[idx]
                        .get(field)
                        .and_then(Cell::text)
                        .is_some_and(|s| s.contains(needle))
                {
                    total += 1;
                }
            }
        }
        Ok(Some(total))
    }

    /// Columnar `SUM(field)` across `MmapFixedCol` segments and live `MemTable` rows.
    pub fn sum_f64_col(&self, table: &str, field: &str) -> Result<Option<f64>, Error> {
        // Per-segment sums are added in segment order, exactly as the sequential scan does.
        let partials = self.map_segments(table, |seg| -> Result<Option<f64>, Error> {
            let Some(fc) = seg.fixed_col(field) else {
                return Ok(if seg.col(field).is_none() {
                    Some(0.0)
                } else {
                    None
                });
            };
            let sid = seg.meta.seg_id;
            let seg_all_live = self.tombstones.dead_count(sid) == 0;
            Ok(fc.sum_f64_live(seg_all_live, |rid| !self.tombstones.is_dead(sid, rid)))
        })?;
        let mut total = 0.0f64;
        for p in partials {
            let Some(seg_sum) = p else {
                return Ok(None);
            };
            total += seg_sum;
        }
        if let Some(mt) = self.memtables.get(table) {
            for &idx in mt.by_id.values() {
                if !mt.dead_mem.contains(&idx) {
                    total += mt.rows[idx]
                        .get(field)
                        .and_then(Cell::as_f64)
                        .unwrap_or(0.0);
                }
            }
        }
        Ok(Some(total))
    }

    /// Columnar `SUM(val_field) GROUP BY by_field` using `MmapDictCol` ordinal indexing
    /// (zero per-row hash probes and zero `Row` allocations).
    pub fn sum_f64_by_col(
        &self,
        table: &str,
        val_field: &str,
        by_field: &str,
    ) -> Result<Option<FxHashMap<Cell, f64>>, Error> {
        // One partial map per segment (in parallel for large multi-segment tables), merged in
        // segment order so the float sums do not depend on scheduling.
        let partials = self.map_segments(table, |seg| -> Result<Option<FxHashMap<Cell, f64>>, Error> {
            let mut map: FxHashMap<Cell, f64> = FxHashMap::default();
            let Some(fc) = seg.fixed_col(val_field) else {
                if seg.col(val_field).is_none() {
                    return Ok(Some(map));
                }
                return Ok(None);
            };
            let is_int = match fc.kind() {
                DiskColKind::Float64 => false,
                DiskColKind::Int64 => true,
                _ => return Ok(None),
            };
            let sid = seg.meta.seg_id;
            let all_live = self.tombstones.dead_count(sid) == 0;
            let rc = seg.meta.row_count.min(fc.row_count());

            match seg.col(by_field) {
                Some(MmapCol::Dict(dc)) => {
                    let nd = dc.strings.len();
                    let mut ord_sums = vec![0.0f64; nd];
                    let mut ord_seen = vec![false; nd];
                    let mut null_sum = 0.0f64;
                    let mut null_seen = false;
                    for rid in 0..rc {
                        if !all_live && self.tombstones.is_dead(sid, rid) {
                            continue;
                        }
                        let val = if fc.is_null(rid) {
                            0.0
                        } else if is_int {
                            fc.read_i64(rid) as f64
                        } else {
                            fc.read_f64(rid)
                        };
                        let code = dc.code_at(rid);
                        if code == DICT_NULL_CODE {
                            null_sum += val;
                            null_seen = true;
                        } else if let Some(slot) = ord_sums.get_mut(code as usize) {
                            *slot += val;
                            ord_seen[code as usize] = true;
                        }
                    }
                    if null_seen {
                        *map.entry(Cell::Null).or_insert(0.0) += null_sum;
                    }
                    for (ci, (&sum_v, &seen)) in ord_sums.iter().zip(ord_seen.iter()).enumerate() {
                        if seen {
                            let cell = dc.get_cell_by_code(ci as u16);
                            *map.entry(cell).or_insert(0.0) += sum_v;
                        }
                    }
                }
                Some(MmapCol::Text(_)) => {
                    let by_pos = seg.col_pos(by_field);
                    let mut lb = None;
                    for rid in 0..rc {
                        if !all_live && self.tombstones.is_dead(sid, rid) {
                            continue;
                        }
                        let val = if fc.is_null(rid) {
                            0.0
                        } else if is_int {
                            fc.read_i64(rid) as f64
                        } else {
                            fc.read_f64(rid)
                        };
                        let cell = match seg.borrow_col_str(by_pos, rid, &mut lb)? {
                            Some(s) => Cell::from_text(s),
                            None => Cell::Null,
                        };
                        *map.entry(cell).or_insert(0.0) += val;
                    }
                }
                None => {
                    let seg_sum = fc
                        .sum_f64_live(all_live, |rid| !self.tombstones.is_dead(sid, rid))
                        .unwrap_or(0.0);
                    let live_cnt = seg
                        .meta
                        .row_count
                        .saturating_sub(self.tombstones.dead_count(sid));
                    if live_cnt > 0 {
                        *map.entry(Cell::Null).or_insert(0.0) += seg_sum;
                    }
                }
                _ => return Ok(None),
            }
            Ok(Some(map))
        })?;
        let mut map: FxHashMap<Cell, f64> = FxHashMap::default();
        for partial in partials {
            let Some(partial) = partial else {
                return Ok(None);
            };
            if map.is_empty() {
                map = partial;
            } else {
                for (cell, v) in partial {
                    *map.entry(cell).or_insert(0.0) += v;
                }
            }
        }
        if let Some(mt) = self.memtables.get(table) {
            for &idx in mt.by_id.values() {
                if mt.dead_mem.contains(&idx) {
                    continue;
                }
                let row = &mt.rows[idx];
                let key = row
                    .get(by_field)
                    .map(Cell::clone_for_borrow)
                    .unwrap_or(Cell::Null);
                let val = row.get(val_field).and_then(Cell::as_f64).unwrap_or(0.0);
                *map.entry(key).or_insert(0.0) += val;
            }
        }
        Ok(Some(map))
    }

    pub fn get_by_id(&self, table: &str, id: &str) -> Result<Option<Row>, Error> {
        if let Some(mt) = self.memtables.get(table) {
            if mt.deleted_ids.contains(id) {
                return Ok(None);
            }
            if let Some(&idx) = mt.by_id.get(id) {
                return Ok(Some(mt.rows[idx].clone()));
            }
        }
        if let Some(cached) = self.hot_cache.get_row(table, id) {
            return Ok(Some((*cached).clone()));
        }
        let Some(segs) = self.segments.get(table) else {
            return Ok(None);
        };
        with_text_index_key(id, |kbuf| {
            for seg in segs.iter().rev() {
                let sid = seg.meta.seg_id;
                let mut live_rid = None;
                seg.pk_index.for_each_key(kbuf, |rid| {
                    if !self.tombstones.is_dead(sid, rid) {
                        live_rid = Some(rid);
                        return false;
                    }
                    true
                });
                if let Some(rid) = live_rid {
                    let row = seg.materialize_row(rid)?;
                    self.hot_cache.put_row(table, id, Arc::new(row.clone()));
                    return Ok(Some(row));
                }
            }
            Ok(None)
        })
    }

    pub fn get_arc_by_id(&self, table: &str, id: &str) -> Result<Option<Arc<Row>>, Error> {
        if let Some(mt) = self.memtables.get(table) {
            if mt.deleted_ids.contains(id) {
                return Ok(None);
            }
            if let Some(&idx) = mt.by_id.get(id) {
                return Ok(Some(Arc::new(mt.rows[idx].clone())));
            }
        }
        if let Some(cached) = self.hot_cache.get_row(table, id) {
            return Ok(Some(cached));
        }
        let Some(segs) = self.segments.get(table) else {
            return Ok(None);
        };
        with_text_index_key(id, |kbuf| {
            for seg in segs.iter().rev() {
                let sid = seg.meta.seg_id;
                let mut live_rid = None;
                seg.pk_index.for_each_key(kbuf, |rid| {
                    if !self.tombstones.is_dead(sid, rid) {
                        live_rid = Some(rid);
                        return false;
                    }
                    true
                });
                if let Some(rid) = live_rid {
                    let arc = Arc::new(seg.materialize_row(rid)?);
                    self.hot_cache.put_row(table, id, Arc::clone(&arc));
                    return Ok(Some(arc));
                }
            }
            Ok(None)
        })
    }

    pub fn get_projected_by_id(
        &self,
        table: &str,
        id: &str,
        fields: &[&str],
    ) -> Result<Option<Row>, Error> {
        if let Some(mt) = self.memtables.get(table) {
            if mt.deleted_ids.contains(id) {
                return Ok(None);
            }
            if let Some(&idx) = mt.by_id.get(id) {
                let src = &mt.rows[idx];
                let mut out = Row::new();
                for &f in fields {
                    if let Some(c) = src.get(f) {
                        out.insert(f.to_string(), c.clone());
                    }
                }
                return Ok(Some(out));
            }
        }
        if let Some(cached) = self.hot_cache.get_row(table, id) {
            let mut out = Row::new();
            for &f in fields {
                if let Some(c) = cached.get(f) {
                    out.insert(f.to_string(), c.clone());
                }
            }
            return Ok(Some(out));
        }
        let Some(segs) = self.segments.get(table) else {
            return Ok(None);
        };
        with_text_index_key(id, |kbuf| {
            for seg in segs.iter().rev() {
                let sid = seg.meta.seg_id;
                let mut live_rid = None;
                seg.pk_index.for_each_key(kbuf, |rid| {
                    if !self.tombstones.is_dead(sid, rid) {
                        live_rid = Some(rid);
                        return false;
                    }
                    true
                });
                if let Some(rid) = live_rid {
                    return seg.materialize_projected(rid, fields).map(Some);
                }
            }
            Ok(None)
        })
    }

    pub fn count_by_eq(&self, table: &str, field: &str, key: &Cell) -> Result<usize, Error> {
        if field == "id"
            && let Some(id_str) = key.text()
        {
            return Ok(usize::from(self.exists_by_id(table, id_str)));
        }
        let mut kbuf = Vec::with_capacity(64);
        if !encode_index_cell_key(key, &mut kbuf) {
            return Ok(0);
        }
        let mut total = 0usize;
        if let Some(mt) = self.memtables.get(table) {
            if let Some(fmap) = mt.sec.get(field) {
                if let Some(idxs) = fmap.get(&kbuf) {
                    for &idx in idxs {
                        if !mt.dead_mem.contains(&idx) {
                            total += 1;
                        }
                    }
                }
            } else {
                for (idx, row) in mt.rows.iter().enumerate() {
                    if !mt.dead_mem.contains(&idx) && row.get(field) == Some(key) {
                        total += 1;
                    }
                }
            }
        }
        if let Some(segs) = self.segments.get(table) {
            for seg in segs {
                let sid = seg.meta.seg_id;
                if let Some(sec_idx) = seg.sec_indexes.get(field) {
                    sec_idx.for_each_key(&kbuf, |rid| {
                        if !self.tombstones.is_dead(sid, rid) {
                            total += 1;
                        }
                        true
                    });
                } else {
                    for rid in 0..seg.meta.row_count {
                        if !self.tombstones.is_dead(sid, rid) && seg.get_cell(rid, field)? == *key {
                            total += 1;
                        }
                    }
                }
            }
        }
        Ok(total)
    }

    pub fn for_each_by_eq_projected(
        &self,
        table: &str,
        field: &str,
        key: &Cell,
        project_fields: Option<&[&str]>,
        mut visit: impl FnMut(&Row) -> Result<bool, Error>,
    ) -> Result<(), Error> {
        if field == "id"
            && let Some(id_str) = key.text()
        {
            let maybe_row = match project_fields {
                Some(fs) => self.get_projected_by_id(table, id_str, fs)?,
                None => self.get_by_id(table, id_str)?,
            };
            if let Some(r) = maybe_row {
                let _ = visit(&r)?;
            }
            return Ok(());
        }

        let mut kbuf = Vec::with_capacity(64);
        if !encode_index_cell_key(key, &mut kbuf) {
            return Ok(());
        }

        let cacheable = self.active_savepoint.is_none();
        let qhash = if cacheable {
            let mut h = 0xcbf2_9ce4_8422_2325u64;
            for &b in table.as_bytes() {
                h ^= u64::from(b);
                h = h.wrapping_mul(0x100_0000_01b3);
            }
            h ^= 0xfe;
            for &b in field.as_bytes() {
                h ^= u64::from(b);
                h = h.wrapping_mul(0x100_0000_01b3);
            }
            h ^= 0xfd;
            for &b in &kbuf {
                h ^= u64::from(b);
                h = h.wrapping_mul(0x100_0000_01b3);
            }
            h ^= match project_fields {
                None => 0xaa,
                Some(fs) => {
                    let mut ph = 0x55u64;
                    for &f in fs {
                        for &b in f.as_bytes() {
                            ph ^= u64::from(b);
                            ph = ph.wrapping_mul(0x100_0000_01b3);
                        }
                        ph ^= 0xff;
                    }
                    ph
                }
            };
            h
        } else {
            0
        };

        if cacheable
            && let Some(cached) = self
                .hot_cache
                .get_sec_query(self.query_cache_gen(), qhash)
        {
            for row in cached.iter() {
                if !visit(row)? {
                    return Ok(());
                }
            }
            return Ok(());
        }

        let max_cached_rows: usize = if project_fields.is_some_and(|fs| fs.len() <= 4) {
            8_192
        } else {
            2_048
        };
        let mut collected: Option<Vec<Row>> = cacheable.then(Vec::new);

        if let Some(mt) = self.memtables.get(table) {
            if let Some(fmap) = mt.sec.get(field) {
                if let Some(idxs) = fmap.get(&kbuf) {
                    for &idx in idxs {
                        if mt.dead_mem.contains(&idx) {
                            continue;
                        }
                        if let Some(buf) = collected.as_mut() {
                            if buf.len() < max_cached_rows {
                                buf.push(mt.rows[idx].clone());
                            } else {
                                collected = None;
                            }
                        }
                        if !visit(&mt.rows[idx])? {
                            return Ok(());
                        }
                    }
                }
            } else {
                for (idx, row) in mt.rows.iter().enumerate() {
                    if mt.dead_mem.contains(&idx) {
                        continue;
                    }
                    if row.get(field) == Some(key) {
                        if let Some(buf) = collected.as_mut() {
                            if buf.len() < max_cached_rows {
                                buf.push(row.clone());
                            } else {
                                collected = None;
                            }
                        }
                        if !visit(row)? {
                            return Ok(());
                        }
                    }
                }
            }
        }

        if let Some(segs) = self.segments.get(table) {
            for seg in segs {
                let sid = seg.meta.seg_id;
                if let Some(sec_idx) = seg.sec_indexes.get(field) {
                    let mut matches = Vec::new();
                    sec_idx.for_each_key(&kbuf, |rid| {
                        if !self.tombstones.is_dead(sid, rid) {
                            matches.push(rid);
                        }
                        true
                    });
                    if matches.len() > 1 {
                        matches.sort_unstable();
                    }
                    if collected.as_ref().is_some_and(|buf| {
                        buf.len().saturating_add(matches.len()) > max_cached_rows
                    }) {
                        collected = None;
                    }
                    match project_fields {
                        Some(fs) => {
                            let col_indices: Vec<(&str, usize)> = fs
                                .iter()
                                .filter_map(|&f| seg.col_pos.get(f).map(|&p| (f, p)))
                                .collect();
                            let mut last_blocks: Vec<Option<(u32, Arc<[u8]>)>> =
                                vec![None; col_indices.len()];
                            let mut scratch = Row::new();
                            for rid in matches {
                                scratch.clear();
                                for (ci, &(fname, pos)) in col_indices.iter().enumerate() {
                                    let cell = match &seg.cols[pos].1 {
                                        MmapCol::Text(tc) => match tc.get_loc(rid) {
                                            None => Cell::Null,
                                            Some((_, _, 0)) => {
                                                Cell::SmallText(InlineStr::new_truncate(""))
                                            }
                                            Some((bidx, in_off, len)) => {
                                                let block = match &last_blocks[ci] {
                                                    Some((lb, arc)) if *lb == bidx => {
                                                        Arc::clone(arc)
                                                    }
                                                    _ => {
                                                        let decoded = tc.decode_block(bidx)?;
                                                        last_blocks[ci] =
                                                            Some((bidx, Arc::clone(&decoded)));
                                                        decoded
                                                    }
                                                };
                                                let s_off = in_off as usize;
                                                let e_off = s_off + (len as usize);
                                                if e_off > block.len() {
                                                    return Err(io_err(
                                                        "corrupt in-block text slice",
                                                    ));
                                                }
                                                let s = std::str::from_utf8(&block[s_off..e_off])
                                                    .map_err(io_err)?;
                                                Cell::from_text(s)
                                            }
                                        },
                                        other => other.get_cell(rid)?,
                                    };
                                    if !matches!(cell, Cell::Null) {
                                        scratch.insert(fname.to_string(), cell);
                                    }
                                }
                                if let Some(buf) = collected.as_mut() {
                                    if buf.len() < max_cached_rows {
                                        buf.push(scratch.clone());
                                    } else {
                                        collected = None;
                                    }
                                }
                                if !visit(&scratch)? {
                                    return Ok(());
                                }
                            }
                        }
                        None => {
                            for rid in matches {
                                let row = seg.materialize_row(rid)?;
                                if let Some(buf) = collected.as_mut() {
                                    if buf.len() < max_cached_rows {
                                        buf.push(row.clone());
                                    } else {
                                        collected = None;
                                    }
                                }
                                if !visit(&row)? {
                                    return Ok(());
                                }
                            }
                        }
                    }
                } else {
                    for rid in 0..seg.meta.row_count {
                        if self.tombstones.is_dead(sid, rid) {
                            continue;
                        }
                        if seg.get_cell(rid, field)? == *key {
                            let row = match project_fields {
                                Some(fs) => seg.materialize_projected(rid, fs)?,
                                None => seg.materialize_row(rid)?,
                            };
                            if let Some(buf) = collected.as_mut() {
                                if buf.len() < max_cached_rows {
                                    buf.push(row.clone());
                                } else {
                                    collected = None;
                                }
                            }
                            if !visit(&row)? {
                                return Ok(());
                            }
                        }
                    }
                }
            }
        }

        if let Some(buf) = collected {
            self.hot_cache
                .put_sec_query(self.query_cache_gen(), qhash, Arc::from(buf));
        }
        Ok(())
    }

    /// True when `pred` can be answered through a secondary or primary-key index; such
    /// predicates are better served by [`Self::query_filter`] than by a column scan.
    pub(crate) fn has_index_pushdown(&self, table: &str, pred: &crate::ast::Pred, now_ms: i64) -> bool {
        self.extract_index_pushdown(table, pred, now_ms).is_some()
    }

    /// Stream the `cols` of every live row matching `pred` as plain cells (one per column,
    /// `Cell::Null` when absent) without building a `Row` per hit. Returns `Ok(false)`
    /// without emitting anything when the predicate touches non-leaf fields, which this
    /// columnar path cannot evaluate; `emit` returns `false` to stop early.
    pub(crate) fn scan_cells(
        &self,
        table: &str,
        pred: Option<&crate::ast::Pred>,
        cols: &[&str],
        now_ms: i64,
        emit: impl FnMut(&[&str], &mut SmallVec<[Cell; 8]>) -> bool,
    ) -> Result<bool, Error> {
        let mut pred_cols: Vec<&str> = Vec::new();
        if let Some(p) = pred {
            let mut pfields = Vec::new();
            p.walk_fields(&mut pfields);
            for pf in &pfields {
                let Some(name) = pf.leaf() else {
                    return Ok(false);
                };
                if !pred_cols.contains(&name) {
                    pred_cols.push(name);
                }
            }
        }
        self.scan_filter_late(table, pred, &pred_cols, cols, now_ms, emit)?;
        Ok(true)
    }

    /// Filtered scan with late materialization: only the predicate's columns are decoded per
    /// row, zone-map blocks that cannot satisfy a top-level range conjunct are skipped whole,
    /// and the projected output columns are decoded for matching rows only.
    fn scan_filter_late(
        &self,
        table: &str,
        pred: Option<&crate::ast::Pred>,
        pred_cols: &[&str],
        out_cols: &[&str],
        now_ms: i64,
        mut emit: impl FnMut(&[&str], &mut SmallVec<[Cell; 8]>) -> bool,
    ) -> Result<(), Error> {
        // No predicate is the degenerate "exact, no terms" case: every live row matches.
        let terms = match pred {
            Some(p) => zone_prunable_terms(p, now_ms),
            None => PredTerms {
                ranges: Vec::new(),
                text_eqs: Vec::new(),
                text_sets: Vec::new(),
                exact: true,
            },
        };
        let zone_terms = &terms.ranges;
        // One cell per output column, `Cell::Null` for an absent value.
        let mut out_cells: SmallVec<[Cell; 8]> = SmallVec::new();
        if let Some(segs) = self.segments.get(table) {
            'segments: for seg in segs {
                let sid = seg.meta.seg_id;
                let n = seg.meta.row_count as usize;
                let has_dead = self.tombstones.dead_count(sid) > 0;
                // Text equalities against dictionary columns become code comparisons; a
                // segment whose dictionary (or schema) lacks the value cannot match at all.
                let mut dict_eqs: Vec<(&columns::MmapDictCol, u16)> = Vec::new();
                let mut eqs_typed = true;
                for &(f, want) in &terms.text_eqs {
                    match seg.col(f) {
                        None => continue 'segments,
                        Some(MmapCol::Dict(dc)) => match dc.find_code(want) {
                            Some(code) => dict_eqs.push((dc, code)),
                            None => continue 'segments,
                        },
                        Some(_) => eqs_typed = false,
                    }
                }
                let mut dict_sets: Vec<(&columns::MmapDictCol, Vec<bool>)> = Vec::new();
                for (f, set) in &terms.text_sets {
                    match seg.col(f) {
                        // No column: every value is null, which matches neither term.
                        None => continue 'segments,
                        Some(MmapCol::Dict(dc)) => {
                            let allowed = set.allowed_codes(dc);
                            if !allowed.iter().any(|&a| a) {
                                continue 'segments;
                            }
                            dict_sets.push((dc, allowed));
                        }
                        Some(_) => eqs_typed = false,
                    }
                }
                // When the predicate is nothing but range comparisons over fixed-width
                // columns and text equalities over dictionary columns of this segment,
                // evaluate it on the raw column values.
                let typed: Option<Vec<(&columns::MmapFixedCol, crate::ast::CmpOp, columns::FixedRhs)>> =
                    if terms.exact && eqs_typed {
                        zone_terms
                            .iter()
                            .map(|(f, op, rhs)| {
                                let fc = seg.fixed_col(f)?;
                                Some((fc, *op, fc.fixed_rhs(rhs)?))
                            })
                            .collect()
                    } else {
                        None
                    };
                let pred_pos: Vec<(&str, usize)> = if typed.is_some() {
                    Vec::new()
                } else {
                    pred_cols
                        .iter()
                        .filter_map(|&f| seg.col_pos.get(f).map(|&p| (f, p)))
                        .collect()
                };
                // (name, column position, already decoded into `scratch` for the predicate)
                let out_pos: Vec<(&str, Option<usize>, bool)> = out_cols
                    .iter()
                    .map(|&f| {
                        let pos = seg.col_pos.get(f).copied();
                        (f, pos, pred_pos.iter().any(|&(pf, _)| pf == f))
                    })
                    .collect();
                let seg_zone_terms: Vec<(&columns::MmapFixedCol, crate::ast::CmpOp, &Cell)> =
                    zone_terms
                        .iter()
                        .filter_map(|(f, op, rhs)| seg.fixed_col(f).map(|fc| (fc, *op, rhs)))
                        .collect();
                let mut scratch = Row::new();
                for &(f, _) in &pred_pos {
                    scratch.insert(f.to_string(), Cell::Null);
                }
                let mut pred_blocks: Vec<Option<(u32, Arc<[u8]>)>> = vec![None; pred_pos.len()];
                let mut out_blocks: Vec<Option<(u32, Arc<[u8]>)>> = vec![None; out_pos.len()];
                let mut start = 0usize;
                while start < n {
                    let end = (start + columns::ZONE_BLOCK_ROWS).min(n);
                    let bi = start / columns::ZONE_BLOCK_ROWS;
                    let skip = seg_zone_terms
                        .iter()
                        .any(|(fc, op, rhs)| fc.zone_block_skips(bi, *op, rhs));
                    if !skip {
                        for idx in start..end {
                            let rid = idx as u32;
                            if has_dead && self.tombstones.is_dead(sid, rid) {
                                continue;
                            }
                            let hit = match &typed {
                                Some(ranges) => {
                                    dict_eqs.iter().all(|(dc, code)| dc.code_at(rid) == *code)
                                        && dict_sets.iter().all(|(dc, allowed)| {
                                            allowed
                                                .get(dc.code_at(rid) as usize)
                                                .copied()
                                                .unwrap_or(false)
                                        })
                                        && ranges
                                            .iter()
                                            .all(|(fc, op, rhs)| fc.cmp_at(rid, *op, *rhs))
                                }
                                None => {
                                    for (ci, &(f, pos)) in pred_pos.iter().enumerate() {
                                        let cell =
                                            read_seg_cell(seg, pos, rid, &mut pred_blocks[ci])?;
                                        if let Some(slot) = scratch.get_mut(f) {
                                            *slot = cell;
                                        }
                                    }
                                    pred.is_none_or(|p| {
                                        crate::exec::eval_pred(p, &scratch, now_ms)
                                    })
                                }
                            };
                            if !hit {
                                continue;
                            }
                            out_cells.clear();
                            for (ci, &(f, pos, in_scratch)) in out_pos.iter().enumerate() {
                                let cell = if in_scratch {
                                    scratch.get(f).cloned().unwrap_or(Cell::Null)
                                } else if let Some(p) = pos {
                                    read_seg_cell(seg, p, rid, &mut out_blocks[ci])?
                                } else {
                                    Cell::Null
                                };
                                out_cells.push(cell);
                            }
                            if !emit(out_cols, &mut out_cells) {
                                return Ok(());
                            }
                        }
                    }
                    start = end;
                }
            }
        }
        if let Some(mt) = self.memtables.get(table) {
            let mut idxs: Vec<usize> = mt.by_id.values().copied().collect();
            idxs.sort_unstable();
            for idx in idxs {
                if mt.dead_mem.contains(&idx) {
                    continue;
                }
                let src = &mt.rows[idx];
                if pred.is_some_and(|p| !crate::exec::eval_pred(p, src, now_ms)) {
                    continue;
                }
                out_cells.clear();
                for &f in out_cols {
                    out_cells.push(src.get(f).cloned().unwrap_or(Cell::Null));
                }
                if !emit(out_cols, &mut out_cells) {
                    return Ok(());
                }
            }
        }
        Ok(())
    }

    pub fn scan_projected(
        &self,
        table: &str,
        project_fields: Option<&[&str]>,
        mut visit: impl FnMut(&Row) -> Result<bool, Error>,
    ) -> Result<(), Error> {
        if let Some(segs) = self.segments.get(table) {
            for seg in segs {
                let sid = seg.meta.seg_id;
                match project_fields {
                    Some(fs) => {
                        let col_indices: Vec<(&str, usize)> = fs
                            .iter()
                            .filter_map(|&f| seg.col_pos.get(f).map(|&p| (f, p)))
                            .collect();
                        let mut scratch = Row::new();
                        let mut last_blocks: Vec<Option<(u32, Arc<[u8]>)>> =
                            vec![None; col_indices.len()];
                        for rid in 0..seg.meta.row_count {
                            if self.tombstones.is_dead(sid, rid) {
                                continue;
                            }
                            scratch.clear();
                            for (ci, &(fname, pos)) in col_indices.iter().enumerate() {
                                let cell = read_seg_cell(seg, pos, rid, &mut last_blocks[ci])?;
                                if !matches!(cell, Cell::Null) {
                                    scratch.insert(fname.to_string(), cell);
                                }
                            }
                            if !visit(&scratch)? {
                                return Ok(());
                            }
                        }
                    }
                    None => {
                        for rid in 0..seg.meta.row_count {
                            if self.tombstones.is_dead(sid, rid) {
                                continue;
                            }
                            let row = seg.materialize_row(rid)?;
                            if !visit(&row)? {
                                return Ok(());
                            }
                        }
                    }
                }
            }
        }
        if let Some(mt) = self.memtables.get(table) {
            let mut idxs: Vec<usize> = mt.by_id.values().copied().collect();
            idxs.sort_unstable();
            for idx in idxs {
                if mt.dead_mem.contains(&idx) {
                    continue;
                }
                match project_fields {
                    None => {
                        if !visit(&mt.rows[idx])? {
                            return Ok(());
                        }
                    }
                    Some(fs) => {
                        let mut projected = Row::new();
                        for &f in fs {
                            if let Some(c) = mt.rows[idx].get(f) {
                                projected.insert(f.to_string(), c.clone());
                            }
                        }
                        if !visit(&projected)? {
                            return Ok(());
                        }
                    }
                }
            }
        }
        Ok(())
    }

    // ========================================================================
    // Zero-Row Columnar Aggregation, Index Visitors & Direct Segment Access
    // ========================================================================

    pub fn group_count_str1(
        &self,
        table: &str,
        col: &str,
    ) -> Result<(u64, Vec<(Option<String>, u64)>), Error> {
        let (total_live, global_null, dict_cells, mut global_map) =
            self.group_count1_parts(table, col)?;
        for (cell, c) in dict_cells {
            if let Some(s) = cell.text() {
                *global_map.entry(s.to_string()).or_insert(0) += c;
            }
        }
        let mut out: Vec<(Option<String>, u64)> = Vec::with_capacity(global_map.len() + 1);
        if global_null > 0 {
            out.push((None, global_null));
        }
        for (k, v) in global_map {
            out.push((Some(k), v));
        }
        Ok((total_live, out))
    }

    /// `COUNT(*) GROUP BY col` straight into executor-ready `Cell` keys: dictionary segments
    /// reuse their pre-built cells, so no per-group `String` is allocated and re-hashed.
    pub(crate) fn group_count_cells1(
        &self,
        table: &str,
        col: &str,
    ) -> Result<FxHashMap<Cell, i64>, Error> {
        let (_, nulls, dict_cells, strings) = self.group_count1_parts(table, col)?;
        let mut map: FxHashMap<Cell, i64> =
            FxHashMap::with_capacity_and_hasher(dict_cells.len() + strings.len() + 1, Default::default());
        for (cell, c) in dict_cells {
            *map.entry(cell).or_insert(0) += c as i64;
        }
        for (s, c) in strings {
            *map.entry(Cell::from_text(&s)).or_insert(0) += c as i64;
        }
        if nulls > 0 {
            *map.entry(Cell::Null).or_insert(0) += nulls as i64;
        }
        Ok(map)
    }

    /// Shared scan for single-column group counts: `(live rows, null rows, counts keyed by
    /// dictionary cell, counts keyed by string for non-dictionary segments and the memtable)`.
    #[allow(clippy::type_complexity)]
    fn group_count1_parts(
        &self,
        table: &str,
        col: &str,
    ) -> Result<(u64, u64, FxHashMap<Cell, u64>, FxHashMap<String, u64>), Error> {
        #[allow(clippy::type_complexity)]
        let partials = self.map_segments(
            table,
            |seg| -> Result<(u64, u64, FxHashMap<Cell, u64>, FxHashMap<String, u64>), Error> {
                let mut total_live = 0u64;
                let mut nulls = 0u64;
                let mut dict_cells: FxHashMap<Cell, u64> = FxHashMap::default();
                let mut strings: FxHashMap<String, u64> = FxHashMap::default();
            let sid = seg.meta.seg_id;
            let has_dead = self.tombstones.dead_count(sid) > 0;
            let pos = seg.col_pos.get(col).copied();
            match pos.map(|p| &seg.cols[p].1) {
                Some(MmapCol::Dict(dc)) => {
                    let mut counts = vec![0u64; dc.strings.len()];
                    let mut null_c = 0u64;
                    if !has_dead {
                        total_live += u64::from(seg.meta.row_count);
                        for rid in 0..seg.meta.row_count {
                            let code = dc.code_at(rid);
                            if code == DICT_NULL_CODE {
                                null_c += 1;
                            } else if let Some(slot) = counts.get_mut(code as usize) {
                                *slot += 1;
                            }
                        }
                    } else {
                        for rid in 0..seg.meta.row_count {
                            if self.tombstones.is_dead(sid, rid) {
                                continue;
                            }
                            total_live += 1;
                            let code = dc.code_at(rid);
                            if code == DICT_NULL_CODE {
                                null_c += 1;
                            } else if let Some(slot) = counts.get_mut(code as usize) {
                                *slot += 1;
                            }
                        }
                    }
                    nulls += null_c;
                    for (ci, c) in counts.into_iter().enumerate() {
                        if c > 0 {
                            *dict_cells.entry(dc.get_cell_by_code(ci as u16)).or_insert(0) += c;
                        }
                    }
                }
                _ => {
                    let mut lb = None;
                    for rid in 0..seg.meta.row_count {
                        if has_dead && self.tombstones.is_dead(sid, rid) {
                            continue;
                        }
                        total_live += 1;
                        match seg.borrow_col_str(pos, rid, &mut lb)? {
                            None => nulls += 1,
                            Some(s) => {
                                if let Some(slot) = strings.get_mut(s) {
                                    *slot += 1;
                                } else {
                                    strings.insert(s.to_string(), 1);
                                }
                            }
                        }
                    }
                }
            }
                Ok((total_live, nulls, dict_cells, strings))
            },
        )?;
        let mut total_live = 0u64;
        let mut global_null = 0u64;
        let mut dict_cells: FxHashMap<Cell, u64> = FxHashMap::default();
        let mut global_map: FxHashMap<String, u64> = FxHashMap::default();
        for (live, nulls, cells, strings) in partials {
            total_live += live;
            global_null += nulls;
            if dict_cells.is_empty() {
                dict_cells = cells;
            } else {
                for (cell, c) in cells {
                    *dict_cells.entry(cell).or_insert(0) += c;
                }
            }
            if global_map.is_empty() {
                global_map = strings;
            } else {
                for (k, c) in strings {
                    *global_map.entry(k).or_insert(0) += c;
                }
            }
        }

        if let Some(mt) = self.memtables.get(table) {
            for &idx in mt.by_id.values() {
                if mt.dead_mem.contains(&idx) {
                    continue;
                }
                total_live += 1;
                match mt.rows[idx].get(col).and_then(Cell::text) {
                    None => global_null += 1,
                    Some(s) => {
                        if let Some(slot) = global_map.get_mut(s) {
                            *slot += 1;
                        } else {
                            global_map.insert(s.to_string(), 1);
                        }
                    }
                }
            }
        }

        Ok((total_live, global_null, dict_cells, global_map))
    }

    pub fn group_count_str2(
        &self,
        table: &str,
        col1: &str,
        col2: &str,
    ) -> Result<(u64, Vec<((Option<String>, Option<String>), u64)>), Error> {
        let mut total_live = 0u64;
        let mut global_map: FxHashMap<(Option<String>, Option<String>), u64> = FxHashMap::default();

        if let Some(segs) = self.segments.get(table) {
            for seg in segs {
                let sid = seg.meta.seg_id;
                let has_dead = self.tombstones.dead_count(sid) > 0;
                let p1 = seg.col_pos.get(col1).copied();
                let p2 = seg.col_pos.get(col2).copied();
                match (p1.map(|p| &seg.cols[p].1), p2.map(|p| &seg.cols[p].1)) {
                    (Some(MmapCol::Dict(d1)), Some(MmapCol::Dict(d2))) => {
                        let mut pair_counts: FxHashMap<u32, u64> = FxHashMap::default();
                        if !has_dead {
                            total_live += u64::from(seg.meta.row_count);
                            for rid in 0..seg.meta.row_count {
                                let k =
                                    (u32::from(d1.code_at(rid)) << 16) | u32::from(d2.code_at(rid));
                                *pair_counts.entry(k).or_insert(0) += 1;
                            }
                        } else {
                            for rid in 0..seg.meta.row_count {
                                if self.tombstones.is_dead(sid, rid) {
                                    continue;
                                }
                                total_live += 1;
                                let k =
                                    (u32::from(d1.code_at(rid)) << 16) | u32::from(d2.code_at(rid));
                                *pair_counts.entry(k).or_insert(0) += 1;
                            }
                        }
                        for (pair_code, cnt) in pair_counts {
                            let c1 = (pair_code >> 16) as u16;
                            let c2 = (pair_code & 0xFFFF) as u16;
                            let s1 = if c1 == DICT_NULL_CODE {
                                None
                            } else {
                                d1.strings.get(c1 as usize).map(|b| b.to_string())
                            };
                            let s2 = if c2 == DICT_NULL_CODE {
                                None
                            } else {
                                d2.strings.get(c2 as usize).map(|b| b.to_string())
                            };
                            *global_map.entry((s1, s2)).or_insert(0) += cnt;
                        }
                    }
                    _ => {
                        let mut lb1 = None;
                        let mut lb2 = None;
                        for rid in 0..seg.meta.row_count {
                            if has_dead && self.tombstones.is_dead(sid, rid) {
                                continue;
                            }
                            total_live += 1;
                            let s1 = seg.borrow_col_str(p1, rid, &mut lb1)?.map(str::to_string);
                            let s2 = seg.borrow_col_str(p2, rid, &mut lb2)?.map(str::to_string);
                            *global_map.entry((s1, s2)).or_insert(0) += 1;
                        }
                    }
                }
            }
        }

        if let Some(mt) = self.memtables.get(table) {
            for &idx in mt.by_id.values() {
                if mt.dead_mem.contains(&idx) {
                    continue;
                }
                total_live += 1;
                let r = &mt.rows[idx];
                let s1 = r.get(col1).and_then(Cell::text).map(str::to_string);
                let s2 = r.get(col2).and_then(Cell::text).map(str::to_string);
                *global_map.entry((s1, s2)).or_insert(0) += 1;
            }
        }

        Ok((total_live, global_map.into_iter().collect()))
    }

    pub fn group_count_str3(
        &self,
        table: &str,
        col1: &str,
        col2: &str,
        col3: &str,
    ) -> Result<
        (
            u64,
            Vec<((Option<String>, Option<String>, Option<String>), u64)>,
        ),
        Error,
    > {
        let mut total_live = 0u64;
        let mut global_map: FxHashMap<(Option<String>, Option<String>, Option<String>), u64> =
            FxHashMap::default();

        if let Some(segs) = self.segments.get(table) {
            for seg in segs {
                let sid = seg.meta.seg_id;
                let has_dead = self.tombstones.dead_count(sid) > 0;
                let p1 = seg.col_pos.get(col1).copied();
                let p2 = seg.col_pos.get(col2).copied();
                let p3 = seg.col_pos.get(col3).copied();
                match (
                    p1.map(|p| &seg.cols[p].1),
                    p2.map(|p| &seg.cols[p].1),
                    p3.map(|p| &seg.cols[p].1),
                ) {
                    (Some(MmapCol::Dict(d1)), Some(MmapCol::Dict(d2)), Some(MmapCol::Dict(d3))) => {
                        let mut tri_counts: FxHashMap<u64, u64> = FxHashMap::default();
                        if !has_dead {
                            total_live += u64::from(seg.meta.row_count);
                            for rid in 0..seg.meta.row_count {
                                let k = (u64::from(d1.code_at(rid)) << 32)
                                    | (u64::from(d2.code_at(rid)) << 16)
                                    | u64::from(d3.code_at(rid));
                                *tri_counts.entry(k).or_insert(0) += 1;
                            }
                        } else {
                            for rid in 0..seg.meta.row_count {
                                if self.tombstones.is_dead(sid, rid) {
                                    continue;
                                }
                                total_live += 1;
                                let k = (u64::from(d1.code_at(rid)) << 32)
                                    | (u64::from(d2.code_at(rid)) << 16)
                                    | u64::from(d3.code_at(rid));
                                *tri_counts.entry(k).or_insert(0) += 1;
                            }
                        }
                        for (tc, cnt) in tri_counts {
                            let c1 = ((tc >> 32) & 0xFFFF) as u16;
                            let c2 = ((tc >> 16) & 0xFFFF) as u16;
                            let c3 = (tc & 0xFFFF) as u16;
                            let s1 = (c1 != DICT_NULL_CODE)
                                .then(|| d1.strings.get(c1 as usize).map(|b| b.to_string()))
                                .flatten();
                            let s2 = (c2 != DICT_NULL_CODE)
                                .then(|| d2.strings.get(c2 as usize).map(|b| b.to_string()))
                                .flatten();
                            let s3 = (c3 != DICT_NULL_CODE)
                                .then(|| d3.strings.get(c3 as usize).map(|b| b.to_string()))
                                .flatten();
                            *global_map.entry((s1, s2, s3)).or_insert(0) += cnt;
                        }
                    }
                    _ => {
                        let mut lb1 = None;
                        let mut lb2 = None;
                        let mut lb3 = None;
                        for rid in 0..seg.meta.row_count {
                            if has_dead && self.tombstones.is_dead(sid, rid) {
                                continue;
                            }
                            total_live += 1;
                            let s1 = seg.borrow_col_str(p1, rid, &mut lb1)?.map(str::to_string);
                            let s2 = seg.borrow_col_str(p2, rid, &mut lb2)?.map(str::to_string);
                            let s3 = seg.borrow_col_str(p3, rid, &mut lb3)?.map(str::to_string);
                            *global_map.entry((s1, s2, s3)).or_insert(0) += 1;
                        }
                    }
                }
            }
        }

        if let Some(mt) = self.memtables.get(table) {
            for &idx in mt.by_id.values() {
                if mt.dead_mem.contains(&idx) {
                    continue;
                }
                total_live += 1;
                let r = &mt.rows[idx];
                let s1 = r.get(col1).and_then(Cell::text).map(str::to_string);
                let s2 = r.get(col2).and_then(Cell::text).map(str::to_string);
                let s3 = r.get(col3).and_then(Cell::text).map(str::to_string);
                *global_map.entry((s1, s2, s3)).or_insert(0) += 1;
            }
        }

        Ok((total_live, global_map.into_iter().collect()))
    }

    #[allow(clippy::too_many_arguments)]
    pub fn for_each_by_eq_str3(
        &self,
        table: &str,
        index_field: &str,
        key_str: &str,
        col1: &str,
        col2: &str,
        col3: Option<&str>,
        mut visit: impl FnMut(Option<&str>, Option<&str>, Option<&str>) -> Result<bool, Error>,
    ) -> Result<(), Error> {
        with_text_index_key(key_str, |kbuf| {
            if let Some(mt) = self.memtables.get(table) {
                if let Some(fmap) = mt.sec.get(index_field) {
                    if let Some(idxs) = fmap.get(kbuf) {
                        for &idx in idxs {
                            if mt.dead_mem.contains(&idx) {
                                continue;
                            }
                            let r = &mt.rows[idx];
                            let s1 = r.get(col1).and_then(Cell::text);
                            let s2 = r.get(col2).and_then(Cell::text);
                            let s3 = col3.and_then(|c| r.get(c).and_then(Cell::text));
                            if !visit(s1, s2, s3)? {
                                return Ok(());
                            }
                        }
                    }
                } else {
                    for (idx, r) in mt.rows.iter().enumerate() {
                        if mt.dead_mem.contains(&idx) {
                            continue;
                        }
                        if r.get(index_field).and_then(Cell::text) == Some(key_str) {
                            let s1 = r.get(col1).and_then(Cell::text);
                            let s2 = r.get(col2).and_then(Cell::text);
                            let s3 = col3.and_then(|c| r.get(c).and_then(Cell::text));
                            if !visit(s1, s2, s3)? {
                                return Ok(());
                            }
                        }
                    }
                }
            }

            if let Some(segs) = self.segments.get(table) {
                for seg in segs {
                    let sid = seg.meta.seg_id;
                    let p1 = seg.col_pos.get(col1).copied();
                    let p2 = seg.col_pos.get(col2).copied();
                    let p3 = col3.and_then(|c| seg.col_pos.get(c).copied());
                    let mut lb1 = None;
                    let mut lb2 = None;
                    let mut lb3 = None;

                    if let Some(sec_idx) = seg.sec_indexes.get(index_field) {
                        let mut matches = SmallVec::<[u32; 32]>::new();
                        sec_idx.for_each_key(kbuf, |rid| {
                            if !self.tombstones.is_dead(sid, rid) {
                                matches.push(rid);
                            }
                            true
                        });
                        if matches.len() > 1 {
                            matches.sort_unstable();
                        }
                        for rid in matches {
                            let s1 = seg.borrow_col_str(p1, rid, &mut lb1)?;
                            let s2 = seg.borrow_col_str(p2, rid, &mut lb2)?;
                            let s3 = seg.borrow_col_str(p3, rid, &mut lb3)?;
                            if !visit(s1, s2, s3)? {
                                return Ok(());
                            }
                        }
                    } else {
                        let pf = seg.col_pos.get(index_field).copied();
                        let mut lbf = None;
                        for rid in 0..seg.meta.row_count {
                            if self.tombstones.is_dead(sid, rid) {
                                continue;
                            }
                            if seg.borrow_col_str(pf, rid, &mut lbf)? == Some(key_str) {
                                let s1 = seg.borrow_col_str(p1, rid, &mut lb1)?;
                                let s2 = seg.borrow_col_str(p2, rid, &mut lb2)?;
                                let s3 = seg.borrow_col_str(p3, rid, &mut lb3)?;
                                if !visit(s1, s2, s3)? {
                                    return Ok(());
                                }
                            }
                        }
                    }
                }
            }
            Ok(())
        })
    }

    pub fn table_segments(&self, table: &str) -> &[Arc<DiskSegment>] {
        self.segments
            .get(table)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    pub fn build_bm25_allow_bitset(
        &self,
        table: &str,
        fk_col: &str,
        bm25: &MmapBm25Index,
        allow_fk: impl Fn(&str) -> bool,
    ) -> Result<Option<Vec<u64>>, Error> {
        let segs = self.table_segments(table);
        let mt_live = self.memtables.get(table).map_or(0, MemTable::live_count);
        let seg_live: usize = segs
            .iter()
            .map(|s| {
                (s.meta.row_count as usize)
                    .saturating_sub(self.tombstones.dead_count(s.meta.seg_id) as usize)
            })
            .sum();
        let total_live = seg_live + mt_live;
        if total_live != bm25.doc_count as usize {
            return Ok(None);
        }
        if total_live == 0 {
            return Ok(Some(Vec::new()));
        }

        let mut cursor = 0u32;
        for seg in segs {
            let sid = seg.meta.seg_id;
            let nrows = seg.meta.row_count;
            let dead = self.tombstones.dead_count(sid);
            let live_in_seg = nrows.saturating_sub(dead);
            if live_in_seg == 0 {
                continue;
            }
            let p_id = seg.col_pos.get("id").copied();
            let mut lb = None;
            let first_rid = if dead == 0 {
                Some(0)
            } else {
                (0..nrows).find(|&r| !self.tombstones.is_dead(sid, r))
            };
            let last_rid = if dead == 0 {
                Some(nrows - 1)
            } else {
                (0..nrows).rev().find(|&r| !self.tombstones.is_dead(sid, r))
            };
            if let Some(frid) = first_rid {
                let seg_id_str = seg.borrow_col_str(p_id, frid, &mut lb)?;
                if seg_id_str.is_none() || seg_id_str != bm25.doc_id(cursor) {
                    return Ok(None);
                }
            }
            if let Some(lrid) = last_rid {
                let seg_id_str = seg.borrow_col_str(p_id, lrid, &mut lb)?;
                if seg_id_str.is_none() || seg_id_str != bm25.doc_id(cursor + live_in_seg - 1) {
                    return Ok(None);
                }
            }
            cursor += live_in_seg;
        }

        let mut bits = vec![0u64; total_live.div_ceil(64)];
        let mut doc_idx = 0usize;
        for seg in segs {
            let sid = seg.meta.seg_id;
            let nrows = seg.meta.row_count;
            let has_dead = self.tombstones.dead_count(sid) > 0;
            if let Some((pos, seg_dict_bits, sel)) = seg.build_dict_allow_bitset(fk_col, &allow_fk)
            {
                if sel == 0.0 {
                    if !has_dead {
                        doc_idx += nrows as usize;
                    } else {
                        for rid in 0..nrows {
                            if !self.tombstones.is_dead(sid, rid) {
                                doc_idx += 1;
                            }
                        }
                    }
                    continue;
                }
                for rid in 0..nrows {
                    if has_dead && self.tombstones.is_dead(sid, rid) {
                        continue;
                    }
                    if seg.check_dict_bitset(pos, rid, &seg_dict_bits) {
                        bits[doc_idx >> 6] |= 1u64 << (doc_idx & 63);
                    }
                    doc_idx += 1;
                }
            } else {
                let p_fk = seg.col_pos.get(fk_col).copied();
                let mut lb = None;
                for rid in 0..nrows {
                    if has_dead && self.tombstones.is_dead(sid, rid) {
                        continue;
                    }
                    if let Some(fk_val) = seg.borrow_col_str(p_fk, rid, &mut lb)?
                        && allow_fk(fk_val)
                    {
                        bits[doc_idx >> 6] |= 1u64 << (doc_idx & 63);
                    }
                    doc_idx += 1;
                }
            }
        }

        if mt_live > 0 {
            let mut ok_align = true;
            self.for_each_memtable_row(table, |_, row| {
                let Some(cid) = row.get("id").and_then(Cell::text) else {
                    return Ok(true);
                };
                if bm25.doc_id(doc_idx as u32) != Some(cid) {
                    ok_align = false;
                    return Ok(false);
                }
                if let Some(fk_val) = row.get(fk_col).and_then(Cell::text)
                    && allow_fk(fk_val)
                {
                    bits[doc_idx >> 6] |= 1u64 << (doc_idx & 63);
                }
                doc_idx += 1;
                Ok(true)
            })?;
            if !ok_align || doc_idx != total_live {
                return Ok(None);
            }
        }

        Ok(Some(bits))
    }

    #[inline]
    pub fn is_segment_row_dead(&self, seg_id: u64, row_id: u32) -> bool {
        self.tombstones.is_dead(seg_id, row_id)
    }

    pub fn for_each_memtable_row(
        &self,
        table: &str,
        mut visit: impl FnMut(usize, &Row) -> Result<bool, Error>,
    ) -> Result<(), Error> {
        if let Some(mt) = self.memtables.get(table) {
            let mut idxs: Vec<usize> = mt.by_id.values().copied().collect();
            idxs.sort_unstable();
            for idx in idxs {
                if !mt.dead_mem.contains(&idx) && !visit(idx, &mt.rows[idx])? {
                    return Ok(());
                }
            }
        }
        Ok(())
    }

    pub fn get_memtable_row(&self, table: &str, idx: usize) -> Option<&Row> {
        self.memtables.get(table).and_then(|mt| mt.rows.get(idx))
    }

    pub fn segment_matching_rids(&self, seg: &DiskSegment, field: &str, val: &str) -> Vec<u32> {
        let sid = seg.meta.seg_id;
        if let Some(sec) = seg.sec_indexes.get(field) {
            return with_text_index_key(val, |kbuf| {
                let mut out = Vec::new();
                sec.for_each_key(kbuf, |rid| {
                    if !self.tombstones.is_dead(sid, rid) {
                        out.push(rid);
                    }
                    true
                });
                if out.len() > 1 {
                    out.sort_unstable();
                }
                out
            });
        }
        let mut out = Vec::new();
        let pos = seg.col_pos.get(field).copied();
        let mut lb = None;
        for rid in 0..seg.meta.row_count {
            if !self.tombstones.is_dead(sid, rid)
                && seg.borrow_col_str(pos, rid, &mut lb).ok().flatten() == Some(val)
            {
                out.push(rid);
            }
        }
        out
    }

    // ========================================================================
    // Out-of-Core Two-Stage Vector Search (SQ8 Mmap Scan -> Exact f32 Rerank)
    // ========================================================================

    #[allow(clippy::too_many_arguments)]
    pub fn search_vectors_out_of_core<F>(
        &self,
        table: &str,
        vec_field: &str,
        doc_id_field: &str,
        query_vec: &[f32],
        top_k: usize,
        unfiltered: bool,
        only_doc_ids: Option<&[&str]>,
        scope_cache_tag: Option<u64>,
        allow_doc: F,
    ) -> Result<Vec<(f32, Row)>, Error>
    where
        F: Fn(&str) -> bool + Sync,
    {
        if top_k == 0 || query_vec.is_empty() {
            return Ok(Vec::new());
        }
        let q_norm = l2_norm_f64(query_vec);
        if q_norm <= 1e-18 || !q_norm.is_finite() {
            return Ok(Vec::new());
        }

        let cache_key = if !self.has_active_savepoint() {
            let tag = if unfiltered {
                Some(0x554e_4649_4c54_4552u64)
            } else {
                scope_cache_tag
            };
            tag.map(|stag| compute_vec_query_hash(table, vec_field, top_k, stag, query_vec))
        } else {
            None
        };
        if let Some(qhash) = cache_key
            && let Some(cached) = self
                .hot_cache
                .get_vec_query(self.query_cache_gen(), qhash)
        {
            return Ok(cached.to_vec());
        }

        #[derive(Clone, Copy)]
        struct ExactHit {
            score: f32,
            seg_idx: usize,
            row_id: u32,
        }
        impl PartialEq for ExactHit {
            fn eq(&self, other: &Self) -> bool {
                self.score.total_cmp(&other.score) == Ordering::Equal
                    && self.seg_idx == other.seg_idx
                    && self.row_id == other.row_id
            }
        }
        impl Eq for ExactHit {}
        impl PartialOrd for ExactHit {
            fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
                Some(self.cmp(other))
            }
        }
        impl Ord for ExactHit {
            fn cmp(&self, other: &Self) -> Ordering {
                self.score
                    .total_cmp(&other.score)
                    .then_with(|| other.seg_idx.cmp(&self.seg_idx))
                    .then_with(|| other.row_id.cmp(&self.row_id))
            }
        }

        let empty_segs = Vec::new();
        let segs = self.segments.get(table).unwrap_or(&empty_segs);

        if let Some(doc_ids) = only_doc_ids
            && doc_ids.len() <= 4096
        {
            if doc_ids.is_empty() {
                return Ok(Vec::new());
            }
            let all_segs_have_sec = segs.iter().all(|s| {
                s.vec_col(vec_field).is_none() || s.sec_indexes.contains_key(doc_id_field)
            });
            if all_segs_have_sec {
                let mut exact_heap: BinaryHeap<Reverse<ExactHit>> =
                    BinaryHeap::with_capacity(top_k + 1);
                let mut kbuf = Vec::with_capacity(64);
                for &did in doc_ids {
                    kbuf.clear();
                    kbuf.push(1);
                    kbuf.extend_from_slice(did.as_bytes());
                    for (s_idx, seg) in segs.iter().enumerate() {
                        let Some(vcol) = seg.vec_col(vec_field) else {
                            continue;
                        };
                        if vcol.dim != query_vec.len() {
                            continue;
                        }
                        let Some(sec_idx) = seg.sec_indexes.get(doc_id_field) else {
                            continue;
                        };
                        let sid = seg.meta.seg_id;
                        sec_idx.for_each_key(&kbuf, |rid| {
                            if self.tombstones.is_dead(sid, rid) {
                                return true;
                            }
                            let sim = if let Some(f32_slice) = vcol.f32_slice(rid) {
                                cosine_with_q_norm(query_vec, q_norm, f32_slice) as f32
                            } else if let Cell::Vec(v) = vcol.get_cell(rid) {
                                cosine_with_q_norm(query_vec, q_norm, &v) as f32
                            } else {
                                return true;
                            };
                            let hit = ExactHit {
                                score: sim,
                                seg_idx: s_idx,
                                row_id: rid,
                            };
                            if exact_heap.len() < top_k {
                                exact_heap.push(Reverse(hit));
                            } else if exact_heap.peek().is_some_and(|m| hit > m.0) {
                                exact_heap.pop();
                                exact_heap.push(Reverse(hit));
                            }
                            true
                        });
                    }
                    if let Some(mt) = self.memtables.get(table)
                        && let Some(fmap) = mt.sec.get(doc_id_field)
                        && let Some(idxs) = fmap.get(&kbuf)
                    {
                        for &idx in idxs {
                            if mt.dead_mem.contains(&idx) {
                                continue;
                            }
                            let Some(Cell::Vec(v)) = mt.rows[idx].get(vec_field) else {
                                continue;
                            };
                            if v.len() != query_vec.len() {
                                continue;
                            }
                            let sim = cosine_with_q_norm(query_vec, q_norm, v) as f32;
                            let hit = ExactHit {
                                score: sim,
                                seg_idx: usize::MAX,
                                row_id: idx as u32,
                            };
                            if exact_heap.len() < top_k {
                                exact_heap.push(Reverse(hit));
                            } else if exact_heap.peek().is_some_and(|m| hit > m.0) {
                                exact_heap.pop();
                                exact_heap.push(Reverse(hit));
                            }
                        }
                    }
                }
                let mut winners: Vec<ExactHit> = exact_heap.into_iter().map(|r| r.0).collect();
                winners.sort_by(|a, b| b.cmp(a));
                let mut out = Vec::with_capacity(winners.len());
                for w in winners {
                    let row = if w.seg_idx == usize::MAX {
                        self.memtables[table].rows[w.row_id as usize].clone()
                    } else {
                        self.materialize_segment_winner(table, &segs[w.seg_idx], w.row_id)?
                    };
                    out.push((w.score, row));
                }
                if let Some(qhash) = cache_key {
                    self.hot_cache.put_vec_query(
                        self.query_cache_gen(),
                        qhash,
                        Arc::from(out.as_slice()),
                    );
                }
                return Ok(out);
            }
        }

        let q_sq8 = quantize_sq8(query_vec);
        let oversample = (top_k.saturating_mul(8)).max(256);

        #[derive(Clone, Copy)]
        struct Cand {
            approx: i64,
            seg_idx: usize,
            row_id: u32,
        }
        impl PartialEq for Cand {
            fn eq(&self, other: &Self) -> bool {
                self.approx == other.approx
                    && self.seg_idx == other.seg_idx
                    && self.row_id == other.row_id
            }
        }
        impl Eq for Cand {}
        impl PartialOrd for Cand {
            fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
                Some(self.cmp(other))
            }
        }
        impl Ord for Cand {
            fn cmp(&self, other: &Self) -> Ordering {
                self.approx
                    .cmp(&other.approx)
                    .then_with(|| other.seg_idx.cmp(&self.seg_idx))
                    .then_with(|| other.row_id.cmp(&self.row_id))
            }
        }

        #[derive(Clone, Copy)]
        enum VecWorkUnit {
            IvfCluster { s_idx: usize, cluster_idx: usize },
            FlatStripe { s_idx: usize, start: u32, end: u32 },
        }

        const STRIPE_ROWS: u32 = 4096;
        let total_seg_rows: usize = segs.iter().map(|s| s.meta.row_count as usize).sum();
        let use_ivf = total_seg_rows >= 32_768;

        let seg_dict_filters: Vec<Option<(usize, Vec<u64>, f32)>> = if !unfiltered {
            segs.iter()
                .map(|seg| seg.build_dict_allow_bitset(doc_id_field, &allow_doc))
                .collect()
        } else {
            Vec::new()
        };

        let mut work_units: Vec<VecWorkUnit> = Vec::new();
        for (s_idx, seg) in segs.iter().enumerate() {
            let Some(vcol) = seg.vec_col(vec_field) else {
                continue;
            };
            if vcol.dim != query_vec.len() {
                continue;
            }
            if !unfiltered
                && let Some(Some((_, _, sel))) = seg_dict_filters.get(s_idx)
                && *sel <= 0.0
            {
                continue;
            }
            if use_ivf && let Some(ref ivf) = vcol.ivf {
                let nprobe = if unfiltered {
                    (ivf.k / 3).clamp(16, 64).min(ivf.k)
                } else if let Some(Some((_, _, sel))) = seg_dict_filters.get(s_idx) {
                    if *sel < 0.15 {
                        (ivf.k * 2 / 3).clamp(32, 160).min(ivf.k)
                    } else {
                        (ivf.k / 2).clamp(24, 96).min(ivf.k)
                    }
                } else {
                    (ivf.k / 2).clamp(24, 96).min(ivf.k)
                };
                for c in ivf.top_centroids(query_vec, q_norm, nprobe) {
                    work_units.push(VecWorkUnit::IvfCluster {
                        s_idx,
                        cluster_idx: c,
                    });
                }
            } else {
                let mut start = 0u32;
                while start < seg.meta.row_count {
                    let end = (start + STRIPE_ROWS).min(seg.meta.row_count);
                    work_units.push(VecWorkUnit::FlatStripe { s_idx, start, end });
                    start = end;
                }
            }
        }

        let eval_unit = |unit: &VecWorkUnit| -> BinaryHeap<Reverse<Cand>> {
            match *unit {
                VecWorkUnit::IvfCluster { s_idx, cluster_idx } => {
                    let seg = &segs[s_idx];
                    let sid = seg.meta.seg_id;
                    let Some(vcol) = seg.vec_col(vec_field) else {
                        return BinaryHeap::new();
                    };
                    let Some(ref ivf) = vcol.ivf else {
                        return BinaryHeap::new();
                    };
                    let dict_flt = if !unfiltered {
                        seg_dict_filters[s_idx].as_ref()
                    } else {
                        None
                    };
                    let (item_start, item_end) = ivf.cluster_bounds(cluster_idx);
                    let mut local_heap: BinaryHeap<Reverse<Cand>> =
                        BinaryHeap::with_capacity(oversample + 1);
                    let mut min_approx = i64::MIN;
                    for item_idx in item_start..item_end {
                        let (rid, sq8) = ivf.item_rid_and_sq8(item_idx);
                        if self.tombstones.is_dead(sid, rid) {
                            continue;
                        }
                        if let Some((pos, bits, _)) = dict_flt
                            && !seg.check_dict_bitset(*pos, rid, bits)
                        {
                            continue;
                        }
                        let dot = (dot_sq8(&q_sq8, sq8) * 1_000_000.0) as i64;
                        if local_heap.len() >= oversample && dot <= min_approx {
                            continue;
                        }
                        if !unfiltered && dict_flt.is_none() {
                            let ok_doc = seg
                                .with_text_field(rid, doc_id_field, |opt| {
                                    opt.is_none_or(&allow_doc)
                                })
                                .unwrap_or(false);
                            if !ok_doc {
                                continue;
                            }
                        }
                        let cand = Cand {
                            approx: dot,
                            seg_idx: s_idx,
                            row_id: rid,
                        };
                        if local_heap.len() < oversample {
                            local_heap.push(Reverse(cand));
                            if local_heap.len() == oversample
                                && let Some(m) = local_heap.peek()
                            {
                                min_approx = m.0.approx;
                            }
                        } else if local_heap.peek().is_some_and(|min_c| cand > min_c.0) {
                            local_heap.pop();
                            local_heap.push(Reverse(cand));
                            if let Some(m) = local_heap.peek() {
                                min_approx = m.0.approx;
                            }
                        }
                    }
                    local_heap
                }
                VecWorkUnit::FlatStripe { s_idx, start, end } => {
                    let seg = &segs[s_idx];
                    let sid = seg.meta.seg_id;
                    let Some(vcol) = seg.vec_col(vec_field) else {
                        return BinaryHeap::new();
                    };
                    if vcol.dim != query_vec.len() {
                        return BinaryHeap::new();
                    }
                    let dict_flt = if !unfiltered {
                        seg_dict_filters[s_idx].as_ref()
                    } else {
                        None
                    };
                    let mut local_heap: BinaryHeap<Reverse<Cand>> =
                        BinaryHeap::with_capacity(oversample + 1);
                    let mut min_approx = i64::MIN;
                    for rid in start..end {
                        if self.tombstones.is_dead(sid, rid) {
                            continue;
                        }
                        if let Some((pos, bits, _)) = dict_flt
                            && !seg.check_dict_bitset(*pos, rid, bits)
                        {
                            continue;
                        }
                        let Some(sq8) = vcol.sq8_slice(rid) else {
                            continue;
                        };
                        let dot = (dot_sq8(&q_sq8, sq8) * 1_000_000.0) as i64;
                        if local_heap.len() >= oversample && dot <= min_approx {
                            continue;
                        }
                        if !unfiltered && dict_flt.is_none() {
                            let ok_doc = seg
                                .with_text_field(rid, doc_id_field, |opt| {
                                    opt.is_none_or(&allow_doc)
                                })
                                .unwrap_or(false);
                            if !ok_doc {
                                continue;
                            }
                        }
                        let cand = Cand {
                            approx: dot,
                            seg_idx: s_idx,
                            row_id: rid,
                        };
                        if local_heap.len() < oversample {
                            local_heap.push(Reverse(cand));
                            if local_heap.len() == oversample
                                && let Some(m) = local_heap.peek()
                            {
                                min_approx = m.0.approx;
                            }
                        } else if local_heap.peek().is_some_and(|min_c| cand > min_c.0) {
                            local_heap.pop();
                            local_heap.push(Reverse(cand));
                            if let Some(m) = local_heap.peek() {
                                min_approx = m.0.approx;
                            }
                        }
                    }
                    local_heap
                }
            }
        };

        #[cfg(feature = "parallel")]
        let stripe_heaps: Vec<BinaryHeap<Reverse<Cand>>> = {
            use rayon::prelude::*;
            work_units.par_iter().map(eval_unit).collect()
        };
        #[cfg(not(feature = "parallel"))]
        let stripe_heaps: Vec<BinaryHeap<Reverse<Cand>>> =
            work_units.iter().map(eval_unit).collect();

        let mut merged_sq8: BinaryHeap<Reverse<Cand>> = BinaryHeap::with_capacity(oversample + 1);
        for h in stripe_heaps {
            for Reverse(c) in h {
                if merged_sq8.len() < oversample {
                    merged_sq8.push(Reverse(c));
                } else if merged_sq8.peek().is_some_and(|min_c| c > min_c.0) {
                    merged_sq8.pop();
                    merged_sq8.push(Reverse(c));
                }
            }
        }

        let mut exact_heap: BinaryHeap<Reverse<ExactHit>> = BinaryHeap::with_capacity(top_k + 1);
        for Reverse(cand) in merged_sq8 {
            let seg = &segs[cand.seg_idx];
            let Some(vcol) = seg.vec_col(vec_field) else {
                continue;
            };
            let sim = if let Some(f32_slice) = vcol.f32_slice(cand.row_id) {
                cosine_with_q_norm(query_vec, q_norm, f32_slice) as f32
            } else if let Cell::Vec(v) = vcol.get_cell(cand.row_id) {
                cosine_with_q_norm(query_vec, q_norm, &v) as f32
            } else {
                continue;
            };
            let hit = ExactHit {
                score: sim,
                seg_idx: cand.seg_idx,
                row_id: cand.row_id,
            };
            if exact_heap.len() < top_k {
                exact_heap.push(Reverse(hit));
            } else if exact_heap.peek().is_some_and(|m| hit > m.0) {
                exact_heap.pop();
                exact_heap.push(Reverse(hit));
            }
        }

        if let Some(mt) = self.memtables.get(table) {
            for &idx in mt.by_id.values() {
                if mt.dead_mem.contains(&idx) {
                    continue;
                }
                let row = &mt.rows[idx];
                if !unfiltered
                    && let Some(doc_id) = row.get(doc_id_field).and_then(Cell::text)
                    && !allow_doc(doc_id)
                {
                    continue;
                }
                let Some(Cell::Vec(v)) = row.get(vec_field) else {
                    continue;
                };
                if v.len() != query_vec.len() {
                    continue;
                }
                let sim = cosine_with_q_norm(query_vec, q_norm, v) as f32;
                let hit = ExactHit {
                    score: sim,
                    seg_idx: usize::MAX,
                    row_id: idx as u32,
                };
                if exact_heap.len() < top_k {
                    exact_heap.push(Reverse(hit));
                } else if exact_heap.peek().is_some_and(|m| hit > m.0) {
                    exact_heap.pop();
                    exact_heap.push(Reverse(hit));
                }
            }
        }

        let mut winners: Vec<ExactHit> = exact_heap.into_iter().map(|r| r.0).collect();
        winners.sort_by(|a, b| b.cmp(a));
        let mut out = Vec::with_capacity(winners.len());
        for w in winners {
            let row = if w.seg_idx == usize::MAX {
                self.memtables[table].rows[w.row_id as usize].clone()
            } else {
                self.materialize_segment_winner(table, &segs[w.seg_idx], w.row_id)?
            };
            out.push((w.score, row));
        }
        if let Some(qhash) = cache_key {
            self.hot_cache.put_vec_query(
                self.query_cache_gen(),
                qhash,
                Arc::from(out.as_slice()),
            );
        }
        Ok(out)
    }

    fn materialize_segment_winner(
        &self,
        table: &str,
        seg: &DiskSegment,
        rid: u32,
    ) -> Result<Row, Error> {
        let maybe_id = seg.with_text_field(rid, "id", |opt| opt.map(str::to_string))?;
        if let Some(ref id_str) = maybe_id
            && let Some(cached) = self.hot_cache.get_row(table, id_str)
        {
            return Ok((*cached).clone());
        }
        let row = seg.materialize_row(rid)?;
        if let Some(id_str) = maybe_id {
            self.hot_cache
                .put_row(table, &id_str, Arc::new(row.clone()));
        }
        Ok(row)
    }

    // ========================================================================
    // Predicate Pushdown, Filtered Queries & Mutations, and Backup / Migration
    // ========================================================================

    pub fn drop_table(&mut self, table: &str) -> Result<(), Error> {
        if self.read_only {
            return Err(Error::runtime("cannot drop table on read-only DiskEngine"));
        }
        self.manifest.tables.remove(table);
        self.memtables.remove(table);
        self.dirty_overlay.remove(table);
        if let Some(segs) = self.segments.remove(table) {
            for seg in segs {
                self.tombstones.words_by_seg.remove(&seg.meta.seg_id);
                self.tombstones.dead_counts.remove(&seg.meta.seg_id);
                let seg_dir = self
                    .dir
                    .join(SEGMENTS_DIR)
                    .join(format!("seg_{:08}", seg.meta.seg_id));
                let _ = fs::remove_dir_all(&seg_dir);
            }
        }
        self.manifest.segments.retain(|m| m.table != table);
        self.manifest.generation += 1;
        self.hot_cache.clear_all();
        self.save_tombstones_and_manifest()?;
        Ok(())
    }

    pub fn query_filter(
        &self,
        table: &str,
        pred: Option<&crate::ast::Pred>,
        limit: Option<usize>,
        project_fields: Option<&[&str]>,
        now_ms: i64,
    ) -> Result<Vec<Row>, Error> {
        if limit == Some(0) {
            return Ok(Vec::new());
        }
        let mut out = Vec::new();
        let Some(p) = pred else {
            self.scan_projected(table, project_fields, |row| {
                out.push(row.clone());
                if let Some(lim) = limit
                    && out.len() >= lim
                {
                    return Ok(false);
                }
                Ok(true)
            })?;
            return Ok(out);
        };

        let needed_cols: Option<Vec<&str>> = project_fields.and_then(|fs| {
            let mut pfields = Vec::new();
            p.walk_fields(&mut pfields);
            if pfields.iter().all(|f| f.leaf().is_some()) {
                let mut cols = Vec::with_capacity(1 + pfields.len() + fs.len());
                cols.push("id");
                for pf in &pfields {
                    if let Some(name) = pf.leaf()
                        && !cols.contains(&name)
                    {
                        cols.push(name);
                    }
                }
                for &f in fs {
                    if !cols.contains(&f) {
                        cols.push(f);
                    }
                }
                Some(cols)
            } else {
                None
            }
        });

        if let Some((idx_field, keys)) = self.extract_index_pushdown(table, p, now_ms) {
            if keys.len() == 1
                && matches!(
                    p,
                    crate::ast::Pred::Cmp {
                        field,
                        op: crate::ast::CmpOp::Eq,
                        ..
                    } if field.leaf() == Some(idx_field.as_str())
                )
                && self
                    .memtables
                    .get(table)
                    .is_none_or(|mt| mt.rows.is_empty() && mt.deleted_ids.is_empty())
                && self.segments.get(table).is_none_or(|s| s.len() <= 1)
            {
                if project_fields == Some(&["id", "title"]) {
                    let mut recycled =
                        crate::store::take_recycled_id_title_vec().unwrap_or_default();
                    self.for_each_by_eq_projected(
                        table,
                        &idx_field,
                        &keys[0],
                        project_fields,
                        |row| {
                            if let Some(mut slot) = recycled.pop()
                                && slot.contains_key("id")
                                && slot.contains_key("title")
                                && let Some(id_val) = row.get("id")
                                && let Some(title_val) = row.get("title")
                            {
                                *slot.get_mut("id").unwrap() = id_val.clone_for_borrow();
                                *slot.get_mut("title").unwrap() = title_val.clone_for_borrow();
                                out.push(slot);
                            } else {
                                out.push(row.clone());
                            }
                            if let Some(lim) = limit
                                && out.len() >= lim
                            {
                                return Ok(false);
                            }
                            Ok(true)
                        },
                    )?;
                } else {
                    self.for_each_by_eq_projected(
                        table,
                        &idx_field,
                        &keys[0],
                        project_fields,
                        |row| {
                            out.push(row.clone());
                            if let Some(lim) = limit
                                && out.len() >= lim
                            {
                                return Ok(false);
                            }
                            Ok(true)
                        },
                    )?;
                }
                return Ok(out);
            }

            let mut seen_ids: FxHashSet<String> = FxHashSet::default();
            'key_loop: for key_cell in keys {
                let mut stop = false;
                self.for_each_by_eq_projected(
                    table,
                    &idx_field,
                    &key_cell,
                    needed_cols.as_deref(),
                    |row| {
                        if let Some(id) = row.get("id").and_then(Cell::text)
                            && !seen_ids.insert(id.to_string())
                        {
                            return Ok(true);
                        }
                        if crate::exec::eval_pred(p, row, now_ms) {
                            let emitted = match project_fields {
                                None => row.clone(),
                                Some(fs) => {
                                    let mut pr = Row::new();
                                    for &f in fs {
                                        if let Some(c) = row.get(f) {
                                            pr.insert(f.to_string(), c.clone());
                                        }
                                    }
                                    pr
                                }
                            };
                            out.push(emitted);
                            if let Some(lim) = limit
                                && out.len() >= lim
                            {
                                stop = true;
                                return Ok(false);
                            }
                        }
                        Ok(true)
                    },
                )?;
                if stop {
                    break 'key_loop;
                }
            }
            return Ok(out);
        }

        if let (Some(fs), Some(_)) = (project_fields, needed_cols.as_deref()) {
            let mut pfields = Vec::new();
            p.walk_fields(&mut pfields);
            let mut pred_cols: Vec<&str> = Vec::with_capacity(pfields.len());
            for pf in &pfields {
                if let Some(name) = pf.leaf()
                    && !pred_cols.contains(&name)
                {
                    pred_cols.push(name);
                }
            }
            // Rows handed back by a previous result of the same shape keep their keys, so
            // refilling them costs no key or node allocation.
            let mut recycled = if fs == ["id", "title"] {
                crate::store::take_recycled_id_title_vec().unwrap_or_default()
            } else {
                Vec::new()
            };
            self.scan_filter_late(table, Some(p), &pred_cols, fs, now_ms, |cols, cells| {
                let reusable = recycled.last().is_some_and(|slot| {
                    slot.len() == cols.len()
                        && cells.iter().all(|c| !matches!(c, Cell::Null))
                        && cols.iter().all(|&f| slot.contains_key(f))
                });
                let row = if reusable && let Some(mut slot) = recycled.pop() {
                    for (&f, cell) in cols.iter().zip(cells.drain(..)) {
                        if let Some(v) = slot.get_mut(f) {
                            *v = cell;
                        }
                    }
                    slot
                } else {
                    let mut row = Row::new();
                    for (&f, cell) in cols.iter().zip(cells.drain(..)) {
                        if !matches!(cell, Cell::Null) {
                            row.insert(f.to_string(), cell);
                        }
                    }
                    row
                };
                out.push(row);
                limit.is_none_or(|lim| out.len() < lim)
            })?;
            return Ok(out);
        }

        self.scan_projected(table, needed_cols.as_deref(), |row| {
            if crate::exec::eval_pred(p, row, now_ms) {
                let emitted = match project_fields {
                    None => row.clone(),
                    Some(fs) => {
                        let mut pr = Row::new();
                        for &f in fs {
                            if let Some(c) = row.get(f) {
                                pr.insert(f.to_string(), c.clone());
                            }
                        }
                        pr
                    }
                };
                out.push(emitted);
                if let Some(lim) = limit
                    && out.len() >= lim
                {
                    return Ok(false);
                }
            }
            Ok(true)
        })?;
        Ok(out)
    }

    pub fn update_where(
        &mut self,
        table: &str,
        pred: Option<&crate::ast::Pred>,
        patch: &[(String, Cell)],
        now_ms: i64,
    ) -> Result<usize, Error> {
        let mut matched = self.query_filter(table, pred, None, None, now_ms)?;
        if matched.is_empty() {
            return Ok(0);
        }
        for row in &mut matched {
            for (k, v) in patch {
                if matches!(v, Cell::Null) {
                    row.remove(k);
                } else {
                    row.insert(k.clone(), v.clone());
                }
            }
        }
        let n = matched.len();
        self.commit_ops(vec![DiskWalOp::UpsertBatch {
            table: table.to_string(),
            rows: matched,
        }])?;
        Ok(n)
    }

    pub fn delete_where(
        &mut self,
        table: &str,
        pred: &crate::ast::Pred,
        now_ms: i64,
    ) -> Result<usize, Error> {
        let matched = self.query_filter(table, Some(pred), None, Some(&["id"]), now_ms)?;
        if matched.is_empty() {
            return Ok(0);
        }
        let ids: Vec<String> = matched
            .into_iter()
            .filter_map(|r| r.get("id").and_then(Cell::text).map(str::to_string))
            .collect();
        let n = ids.len();
        if n > 0 {
            self.commit_ops(vec![DiskWalOp::DeleteIds {
                table: table.to_string(),
                ids,
            }])?;
        }
        Ok(n)
    }

    fn extract_index_pushdown(
        &self,
        table: &str,
        pred: &crate::ast::Pred,
        now_ms: i64,
    ) -> Option<(String, Vec<Cell>)> {
        use crate::ast::{CmpOp, Pred};
        let is_indexed = |fname: &str| -> bool {
            fname == "id"
                || self
                    .manifest
                    .tables
                    .get(table)
                    .is_some_and(|t| t.indexed_fields.iter().any(|f| f == fname))
        };
        match pred {
            Pred::Cmp {
                field,
                op: CmpOp::Eq,
                value,
            } => {
                let fname = field.leaf()?;
                if is_indexed(fname) {
                    let c = crate::exec::value_cell(value, now_ms);
                    if !matches!(c, Cell::Null) {
                        return Some((fname.to_string(), vec![c]));
                    }
                }
                None
            }
            Pred::In { field, values } => {
                let fname = field.leaf()?;
                if is_indexed(fname) {
                    let mut cells = Vec::with_capacity(values.len());
                    for v in values {
                        let c = crate::exec::value_cell(v, now_ms);
                        if !matches!(c, Cell::Null) {
                            cells.push(c);
                        }
                    }
                    return Some((fname.to_string(), cells));
                }
                None
            }
            Pred::And(a, b) => {
                let left = self.extract_index_pushdown(table, a, now_ms);
                let right = self.extract_index_pushdown(table, b, now_ms);
                match (left, right) {
                    (Some((fa, ka)), Some((fb, kb))) => {
                        if fa == "id" {
                            Some((fa, ka))
                        } else if fb == "id" {
                            Some((fb, kb))
                        } else if ka.len() <= kb.len() {
                            Some((fa, ka))
                        } else {
                            Some((fb, kb))
                        }
                    }
                    (Some(x), None) | (None, Some(x)) => Some(x),
                    (None, None) => None,
                }
            }
            _ => None,
        }
    }

    pub fn export_portable_backup_file(&self, dst_file: &Path) -> Result<(), Error> {
        use crate::persist::{ColSnap, IndexSnap, SNAPSHOT_MAGIC};
        if dst_file
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            return Err(Error::runtime(
                "path traversal rejected: backup path cannot contain '..'",
            ));
        }
        if dst_file.is_symlink() {
            return Err(Error::runtime("backup symlink policy: symlinks rejected"));
        }
        if let Some(parent) = dst_file.parent()
            && !parent.as_os_str().is_empty()
        {
            fs::create_dir_all(parent).map_err(io_err)?;
        }

        let mut extra_collections: BTreeMap<String, ColSnap> = BTreeMap::new();
        let mut extra_indexes: BTreeMap<String, IndexSnap> = BTreeMap::new();
        for (tname, schema) in &self.manifest.tables {
            let fields = schema
                .columns
                .iter()
                .map(|c| {
                    let ty = if c.name == "id" {
                        "id"
                    } else {
                        match c.kind {
                            DiskColKind::Int64 => "i64",
                            DiskColKind::Float64 => "f64",
                            DiskColKind::Bool => "bool",
                            DiskColKind::Time64 => "time",
                            DiskColKind::TextZstd | DiskColKind::TextDict => "var",
                            DiskColKind::VecF32Sq8 { .. } => "vec",
                        }
                    };
                    (c.name.clone(), ty.to_string())
                })
                .collect();
            extra_collections.insert(
                tname.clone(),
                ColSnap {
                    name: tname.clone(),
                    append_only: false,
                    fields,
                },
            );
            for idx_field in &schema.indexed_fields {
                extra_indexes.insert(
                    format!("{tname}[{idx_field}]"),
                    IndexSnap {
                        collection: tname.clone(),
                        unique: false,
                        fields: vec![idx_field.clone()],
                        pred_src: None,
                    },
                );
            }
        }

        #[derive(Serialize)]
        struct PortableHeader<'a> {
            r#gen: u64,
            embed_id: &'a str,
            next_id: u64,
            catalog_hash: &'a str,
            log_offset: u64,
            edges: &'a [crate::store::Edge],
            extra_collections: &'a BTreeMap<String, ColSnap>,
            extra_rels: BTreeMap<String, crate::persist::RelSnap>,
            extra_indexes: &'a BTreeMap<String, IndexSnap>,
            extra_filters: BTreeMap<String, String>,
            extra_owned: BTreeMap<String, Vec<(String, String)>>,
            cold_collections: &'a [String],
        }

        let tmp = dst_file.with_extension("lin.tmp");
        let res = (|| -> Result<(), Error> {
            let file = OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(&tmp)
                .map_err(io_err)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = fs::set_permissions(&tmp, fs::Permissions::from_mode(0o600));
            }
            let mut writer = BufWriter::with_capacity(256 * 1024, file);
            writer.write_all(&[0x03]).map_err(io_err)?;
            writer.write_all(&SNAPSHOT_MAGIC).map_err(io_err)?;

            let header = PortableHeader {
                r#gen: self.manifest.generation,
                embed_id: &self.manifest.embed_id,
                next_id: 1,
                catalog_hash: "",
                log_offset: 0,
                edges: &[],
                extra_collections: &extra_collections,
                extra_rels: BTreeMap::new(),
                extra_indexes: &extra_indexes,
                extra_filters: BTreeMap::new(),
                extra_owned: BTreeMap::new(),
                cold_collections: &[],
            };
            let header_bytes = rmp_serde::encode::to_vec_named(&header).map_err(io_err)?;
            writer
                .write_all(&(header_bytes.len() as u32).to_le_bytes())
                .map_err(io_err)?;
            writer.write_all(&header_bytes).map_err(io_err)?;

            let count_pos = 1 + 4 + 4 + (header_bytes.len() as u64);
            writer.write_all(&0u32.to_le_bytes()).map_err(io_err)?;

            let mut chunk_count = 0u32;
            const CHUNK_ROWS: usize = 512;

            let write_chunk =
                |w: &mut BufWriter<File>, tname: &str, batch: &[Row]| -> Result<(), Error> {
                    let data = if batch.is_empty() {
                        Vec::new()
                    } else {
                        let mut col_buf = Vec::new();
                        let raw = if let Some((magic, _)) =
                            crate::persist::encode_insert_rows_direct(0, 0, "", batch, &mut col_buf)
                        {
                            let mut framed = Vec::with_capacity(8 + col_buf.len());
                            framed.extend_from_slice(b"LCOL");
                            framed.extend_from_slice(&magic);
                            framed.extend_from_slice(&col_buf);
                            framed
                        } else {
                            rmp_serde::encode::to_vec_named(batch).map_err(io_err)?
                        };
                        #[cfg(feature = "compress-zstd")]
                        {
                            zstd::encode_all(&raw[..], 1).map_err(io_err)?
                        }
                        #[cfg(not(feature = "compress-zstd"))]
                        {
                            raw
                        }
                    };
                    let nb = tname.as_bytes();
                    w.write_all(&(nb.len() as u16).to_le_bytes())
                        .map_err(io_err)?;
                    w.write_all(nb).map_err(io_err)?;
                    w.write_all(&(data.len() as u32).to_le_bytes())
                        .map_err(io_err)?;
                    if !data.is_empty() {
                        w.write_all(&data).map_err(io_err)?;
                    }
                    Ok(())
                };

            for tname in self.manifest.tables.keys() {
                let mut batch = Vec::with_capacity(CHUNK_ROWS);
                let mut emitted_any = false;
                self.scan_projected(tname, None, |row| {
                    batch.push(row.clone());
                    if batch.len() >= CHUNK_ROWS {
                        write_chunk(&mut writer, tname, &batch)?;
                        chunk_count += 1;
                        emitted_any = true;
                        batch.clear();
                    }
                    Ok(true)
                })?;
                if !batch.is_empty() || !emitted_any {
                    write_chunk(&mut writer, tname, &batch)?;
                    chunk_count += 1;
                }
            }

            writer.flush().map_err(io_err)?;
            let mut file = writer.into_inner().map_err(io_err)?;
            file.seek(SeekFrom::Start(count_pos)).map_err(io_err)?;
            file.write_all(&chunk_count.to_le_bytes()).map_err(io_err)?;
            file.sync_all().map_err(io_err)?;
            drop(file);
            fs::rename(&tmp, dst_file).map_err(io_err)?;
            Ok(())
        })();
        if res.is_err() {
            let _ = fs::remove_file(&tmp);
        }
        res
    }

    pub fn migrate_from_legacy(
        src_path: &Path,
        dst_dir: &Path,
        rows_per_segment: usize,
        extra_indexed_fields: &BTreeMap<String, Vec<String>>,
    ) -> Result<Self, Error> {
        let snap = if src_path.is_dir() {
            let mut db = crate::exec::Db::open_follower(src_path)?;
            db.store.capture_snapshot_at(String::new(), 0)
        } else {
            crate::persist::read_backup(src_path)?
        };
        Self::from_legacy_snapshot(snap, dst_dir, rows_per_segment, extra_indexed_fields)
    }

    pub fn from_legacy_snapshot(
        snap: crate::persist::Snapshot,
        dst_dir: &Path,
        rows_per_segment: usize,
        extra_indexed_fields: &BTreeMap<String, Vec<String>>,
    ) -> Result<Self, Error> {
        if dst_dir.exists() {
            fs::remove_dir_all(dst_dir).map_err(io_err)?;
        }
        let mut eng = Self::open(dst_dir, false)?;
        eng.manifest.embed_id = snap.embed_id.clone();
        let seg_cap = rows_per_segment.max(1024);

        for (tname, col_snap) in &snap.extra_collections {
            let mut columns = Vec::new();
            for (fname, fty) in &col_snap.fields {
                let kind = match fty.as_str() {
                    "int" | "i64" | "dur" => DiskColKind::Int64,
                    "float" | "f64" => DiskColKind::Float64,
                    "bool" => DiskColKind::Bool,
                    "time" => DiskColKind::Time64,
                    "vec" => DiskColKind::VecF32Sq8 { dim: 0 },
                    _ if tname == "chunks" && fname == "embedding" => {
                        DiskColKind::VecF32Sq8 { dim: 0 }
                    }
                    _ => DiskColKind::TextZstd,
                };
                columns.push(DiskColSchema {
                    name: fname.clone(),
                    kind,
                });
            }
            let mut indexed_fields = extra_indexed_fields.get(tname).cloned().unwrap_or_default();
            for idx in snap.extra_indexes.values() {
                if idx.collection == *tname {
                    for f in &idx.fields {
                        if !indexed_fields.contains(f) {
                            indexed_fields.push(f.clone());
                        }
                    }
                }
            }
            eng.manifest.tables.insert(
                tname.clone(),
                DiskTableSchema {
                    name: tname.clone(),
                    columns,
                    indexed_fields,
                    cluster_by: None,
                },
            );
        }

        for (tname, rows) in snap.collections {
            let inferred = infer_schema_from_rows(&tname, &rows);
            let entry = eng.manifest.tables.entry(tname.clone()).or_insert_with(|| {
                let mut s = inferred.clone();
                if let Some(ixs) = extra_indexed_fields.get(&tname) {
                    s.indexed_fields = ixs.clone();
                }
                s
            });
            for col in inferred.columns {
                if let Some(existing) = entry.columns.iter_mut().find(|c| c.name == col.name) {
                    if matches!(col.kind, DiskColKind::VecF32Sq8 { .. }) {
                        existing.kind = col.kind;
                    }
                } else {
                    entry.columns.push(col);
                }
            }
            if let Some(ixs) = extra_indexed_fields.get(&tname) {
                for ix in ixs {
                    if !entry.indexed_fields.contains(ix) {
                        entry.indexed_fields.push(ix.clone());
                    }
                }
            }
            for chunk in rows.chunks(seg_cap) {
                eng.write_bulk_segment(&tname, chunk)?;
            }
        }
        eng.manifest.generation = snap.r#gen.max(eng.manifest.generation).max(1);
        eng.save_tombstones_and_manifest()?;
        Ok(eng)
    }

    pub fn to_legacy_snapshot(&self) -> Result<crate::persist::Snapshot, Error> {
        let tmp = std::env::temp_dir().join(format!(
            "lin_disk_snap_{}_{}.lin",
            std::process::id(),
            self.manifest.generation
        ));
        self.export_portable_backup_file(&tmp)?;
        let res = crate::persist::read_backup(&tmp);
        let _ = fs::remove_file(&tmp);
        res
    }
}

fn compute_vec_query_hash(
    table: &str,
    vec_field: &str,
    top_k: usize,
    scope_tag: u64,
    query_vec: &[f32],
) -> u64 {
    let mut h = 0xcbf29ce484222325u64;
    for &b in table.as_bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    for &b in vec_field.as_bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h ^= top_k as u64;
    h = h.wrapping_mul(0x100000001b3);
    h ^= scope_tag;
    h = h.wrapping_mul(0x100000001b3);
    for &x in query_vec {
        h ^= x.to_bits() as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

/// Thread-safe handle wrapping `DiskEngine` under an `RwLock` for multi-reader concurrency.
#[derive(Clone)]
pub struct SharedDiskEngine {
    inner: Arc<RwLock<DiskEngine>>,
}

impl SharedDiskEngine {
    pub fn new(engine: DiskEngine) -> Self {
        Self {
            inner: Arc::new(RwLock::new(engine)),
        }
    }

    pub fn read(&self) -> Result<std::sync::RwLockReadGuard<'_, DiskEngine>, Error> {
        self.inner
            .read()
            .map_err(|_| Error::runtime("DiskEngine RwLock poisoned"))
    }

    pub fn write(&self) -> Result<std::sync::RwLockWriteGuard<'_, DiskEngine>, Error> {
        self.inner
            .write()
            .map_err(|_| Error::runtime("DiskEngine RwLock poisoned"))
    }
}

/// Stable sort by the cluster key, rows without a value last (same order as
/// [`DiskEngine::compact_table_clustered`]).
pub(crate) fn sort_rows_by_cluster_key(rows: &mut [Row], key: &str) {
    rows.sort_by(|a, b| {
        let present = |r: &Row| r.get(key).filter(|c| !matches!(c, Cell::Null)).is_some();
        match (a.get(key), b.get(key)) {
            (Some(ka), Some(kb)) if present(a) && present(b) => crate::exec::cmp_sort(ka, kb),
            _ => present(b).cmp(&present(a)),
        }
    });
}

/// Column kinds of a batch: the first non-null cell of each column decides.
pub(crate) fn infer_schema_from_batch(
    table: &str,
    batch: &crate::batch::RecordBatch,
) -> DiskTableSchema {
    let mut cols: BTreeMap<String, DiskColKind> = BTreeMap::new();
    for (name, col) in batch.names.iter().zip(&batch.cols) {
        let kind = col.iter().find_map(|v| match &v {
            Cell::Null => None,
            Cell::Int(_) => Some(DiskColKind::Int64),
            Cell::Float(_) => Some(DiskColKind::Float64),
            Cell::Bool(_) => Some(DiskColKind::Bool),
            Cell::Time(_) => Some(DiskColKind::Time64),
            Cell::Text(_) | Cell::SmallText(_) => Some(DiskColKind::TextZstd),
            Cell::Vec(vec) => Some(DiskColKind::VecF32Sq8 {
                dim: vec.len() as u32,
            }),
        });
        if let Some(kind) = kind {
            cols.entry(name.clone()).or_insert(kind);
        }
    }
    DiskTableSchema {
        name: table.to_string(),
        columns: cols
            .into_iter()
            .map(|(name, kind)| DiskColSchema { name, kind })
            .collect(),
        indexed_fields: Vec::new(),
        cluster_by: None,
    }
}

pub(crate) fn infer_schema_from_rows(table: &str, rows: &[Row]) -> DiskTableSchema {
    let mut cols: BTreeMap<String, DiskColKind> = BTreeMap::new();
    for r in rows {
        for (k, v) in r {
            if cols.contains_key(k) {
                continue;
            }
            let kind = match v {
                Cell::Null => continue,
                Cell::Int(_) => DiskColKind::Int64,
                Cell::Float(_) => DiskColKind::Float64,
                Cell::Bool(_) => DiskColKind::Bool,
                Cell::Time(_) => DiskColKind::Time64,
                Cell::Text(_) | Cell::SmallText(_) => DiskColKind::TextZstd,
                Cell::Vec(vec) => DiskColKind::VecF32Sq8 {
                    dim: vec.len() as u32,
                },
            };
            cols.insert(k.clone(), kind);
        }
    }
    DiskTableSchema {
        name: table.to_string(),
        columns: cols
            .into_iter()
            .map(|(name, kind)| DiskColSchema { name, kind })
            .collect(),
        indexed_fields: Vec::new(),
        cluster_by: None,
    }
}
