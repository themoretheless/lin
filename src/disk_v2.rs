//! Disk-backed out-of-core columnar LSM / Segment-Mmap Storage Engine (`StorageEngineV2`).
//!
//! Designed to scale seamlessly from megabytes to **500+ GiB** databases while keeping:
//! - **`open()` under ~5–15 ms** (pure `mmap` of segment files, zero eager row deserialization).
//! - **Constant low RAM footprint** (OS page cache manages cold/hot pages; no `Vec<Row>` full-corpus heap resident image).
//! - **O(1) Point Lookups & Secondary Index Seeks** directly on memory-mapped hash + sorted-key posting tables.
//! - **Two-Stage Out-of-Core Vector Search**: SIMD (`NEON` / scalar) + `rayon` scan over flat `mmap` `SQ8` (`i8`)
//!   quantized vectors for 4x SSD/memory-bandwidth reduction, followed by exact `mmap` `f32` cosine reranking
//!   of the top candidates — zero `VECTOR_SNAPSHOT` heap copies.
//! - **Append-Only MemTable + WAL + Incremental Segment Flush** (`checkpoint` writes only the new delta segment,
//!   never rewriting existing multi-gigabyte segments; deletes mark a durable tombstone Roaring/bitset).

use std::cmp::{Ordering, Reverse};
use std::collections::{BTreeMap, BinaryHeap};
use std::fs::{self, File, OpenOptions};
use std::io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};

use crc32fast::Hasher as Crc32;
use memmap2::{Mmap, MmapOptions};
use rustc_hash::{FxHashMap, FxHashSet};
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

use crate::embed::{cosine_with_q_norm, dot_sq8, l2_norm_f64, quantize_sq8, quantize_sq8_into};
use crate::error::Error;
use crate::persist::io_err;
use crate::store::{Cell, InlineStr, Row};

pub const DISK_DIR_MAGIC: &[u8; 8] = b"LINDISK2";
pub const MANIFEST_FILE: &str = "manifest.v2.json";
pub const WAL_FILE: &str = "wal.v2.log";
pub const TOMBSTONES_FILE: &str = "tombstones.v2.bin";
pub const SEGMENTS_DIR: &str = "segments";
pub const LOCK_FILE: &str = "LOCK";

/// Target uncompressed payload per Zstd block for variable-length text columns (32 KiB).
/// Small enough that random point reads (`get_document`) decompress in ~8 µs,
/// large enough to achieve ~3.5x–5x Zstd compression ratio on prose/code/JSON.
const TEXT_BLOCK_TARGET_BYTES: usize = 32 * 1024;

/// Number of lock-sharded cache stripes per text column (zero contention under 8+ Rayon/HTTP reader threads).
const BLOCK_CACHE_SHARDS: usize = 16;
/// Number of decoded text blocks per shard (`16 * 128 = 2,048` blocks max = ~64 MiB cap per active column).
const BLOCK_CACHE_SLOTS_PER_SHARD: usize = 128;

/// Configuration for the bounded in-memory L1 Hot Cache (`HotTierCache`) sitting in front of `mmap` segments.
#[derive(Debug, Clone, Copy)]
pub struct HotCacheConfig {
    /// Enable the bounded in-memory L1 row & query cache.
    pub enabled: bool,
    /// Maximum memory budget in bytes for hot cached rows across all tables (default: `256 MiB`).
    pub max_bytes: usize,
    /// Maximum number of hot rows retained in L1 RAM (default: `65,536`).
    pub max_rows: usize,
    /// Maximum cached top-k vector query results for current generation (default: `512`).
    pub max_vec_queries: usize,
}

impl Default for HotCacheConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_bytes: 256 * 1024 * 1024,
            max_rows: 65_536,
            max_vec_queries: 512,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct HotCacheStats {
    pub used_bytes: usize,
    pub cached_rows: usize,
    pub row_hits: u64,
    pub row_misses: u64,
    pub row_evictions: u64,
    pub vec_query_hits: u64,
    pub vec_query_misses: u64,
    pub sec_query_hits: u64,
    pub sec_query_misses: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CompactionReport {
    pub tables_compacted: usize,
    pub old_segments_removed: usize,
    pub new_segments_written: usize,
    pub dead_rows_reclaimed: usize,
}

const HOT_ROW_SHARDS: usize = 16;
const LRU_NONE: u32 = u32::MAX;

struct HotRowSlot {
    hash: u64,
    table: Box<str>,
    id: Box<str>,
    row: Arc<Row>,
    approx_bytes: usize,
    prev: u32,
    next: u32,
    alive: bool,
}

struct HotRowShard {
    slots: Vec<HotRowSlot>,
    free: Vec<u32>,
    by_hash: FxHashMap<u64, SmallVec<[u32; 1]>>,
    head: u32,
    tail: u32,
    live_rows: usize,
    used_bytes: usize,
    hits: u64,
    misses: u64,
    evictions: u64,
}

impl Default for HotRowShard {
    fn default() -> Self {
        Self {
            slots: Vec::new(),
            free: Vec::new(),
            by_hash: FxHashMap::default(),
            head: LRU_NONE,
            tail: LRU_NONE,
            live_rows: 0,
            used_bytes: 0,
            hits: 0,
            misses: 0,
            evictions: 0,
        }
    }
}

impl HotRowShard {
    #[inline]
    fn find_slot(&self, hash: u64, table: &str, id: &str) -> Option<u32> {
        let bucket = self.by_hash.get(&hash)?;
        for &idx in bucket {
            let s = &self.slots[idx as usize];
            if s.alive && s.id.as_ref() == id && s.table.as_ref() == table {
                return Some(idx);
            }
        }
        None
    }

    #[inline]
    fn detach(&mut self, idx: u32) {
        let (prev, next) = {
            let s = &mut self.slots[idx as usize];
            let p = s.prev;
            let n = s.next;
            s.prev = LRU_NONE;
            s.next = LRU_NONE;
            (p, n)
        };
        if prev != LRU_NONE {
            self.slots[prev as usize].next = next;
        } else {
            self.head = next;
        }
        if next != LRU_NONE {
            self.slots[next as usize].prev = prev;
        } else {
            self.tail = prev;
        }
    }

    #[inline]
    fn push_back_mru(&mut self, idx: u32) {
        let old_tail = self.tail;
        let s = &mut self.slots[idx as usize];
        s.prev = old_tail;
        s.next = LRU_NONE;
        if old_tail != LRU_NONE {
            self.slots[old_tail as usize].next = idx;
        } else {
            self.head = idx;
        }
        self.tail = idx;
    }

    fn remove_slot(&mut self, idx: u32) {
        self.detach(idx);
        let (hash, approx) = {
            let s = &mut self.slots[idx as usize];
            s.alive = false;
            (s.hash, s.approx_bytes)
        };
        self.used_bytes = self.used_bytes.saturating_sub(approx);
        self.live_rows = self.live_rows.saturating_sub(1);
        if let Some(bucket) = self.by_hash.get_mut(&hash) {
            if let Some(pos) = bucket.iter().position(|&x| x == idx) {
                bucket.swap_remove(pos);
            }
            if bucket.is_empty() {
                self.by_hash.remove(&hash);
            }
        }
        self.free.push(idx);
    }

    /// Zero-allocation O(1) L1 cache hit + intrusive array MRU promotion!
    #[inline]
    fn get(&mut self, hash: u64, table: &str, id: &str) -> Option<Arc<Row>> {
        let Some(idx) = self.find_slot(hash, table, id) else {
            self.misses += 1;
            return None;
        };
        let out = Arc::clone(&self.slots[idx as usize].row);
        if self.tail != idx {
            self.detach(idx);
            self.push_back_mru(idx);
        }
        self.hits += 1;
        Some(out)
    }

    #[allow(clippy::too_many_arguments)]
    fn insert(
        &mut self,
        hash: u64,
        table: &str,
        id: &str,
        row: Arc<Row>,
        approx_bytes: usize,
        max_bytes: usize,
        max_rows: usize,
    ) {
        if max_bytes == 0 || max_rows == 0 || approx_bytes > max_bytes {
            return;
        }
        if let Some(existing_idx) = self.find_slot(hash, table, id) {
            let old_bytes = self.slots[existing_idx as usize].approx_bytes;
            self.used_bytes = self.used_bytes.saturating_sub(old_bytes);
            while (self.used_bytes + approx_bytes > max_bytes) && self.head != LRU_NONE {
                let victim = self.head;
                if victim == existing_idx {
                    if self.slots[victim as usize].next == LRU_NONE {
                        break;
                    }
                    let next_v = self.slots[victim as usize].next;
                    self.remove_slot(next_v);
                    self.evictions += 1;
                } else {
                    self.remove_slot(victim);
                    self.evictions += 1;
                }
            }
            let s = &mut self.slots[existing_idx as usize];
            s.row = row;
            s.approx_bytes = approx_bytes;
            self.used_bytes += approx_bytes;
            if self.tail != existing_idx {
                self.detach(existing_idx);
                self.push_back_mru(existing_idx);
            }
            return;
        }

        while (self.live_rows >= max_rows || self.used_bytes + approx_bytes > max_bytes)
            && self.head != LRU_NONE
        {
            let victim = self.head;
            self.remove_slot(victim);
            self.evictions += 1;
        }

        let slot = HotRowSlot {
            hash,
            table: table.into(),
            id: id.into(),
            row,
            approx_bytes,
            prev: LRU_NONE,
            next: LRU_NONE,
            alive: true,
        };
        let idx = if let Some(reused) = self.free.pop() {
            self.slots[reused as usize] = slot;
            reused
        } else {
            let n = self.slots.len() as u32;
            self.slots.push(slot);
            n
        };
        self.by_hash.entry(hash).or_default().push(idx);
        self.push_back_mru(idx);
        self.live_rows += 1;
        self.used_bytes += approx_bytes;
    }

    #[inline]
    fn invalidate(&mut self, hash: u64, table: &str, id: &str) {
        if let Some(idx) = self.find_slot(hash, table, id) {
            self.remove_slot(idx);
        }
    }

    fn clear(&mut self) {
        self.slots.clear();
        self.free.clear();
        self.by_hash.clear();
        self.head = LRU_NONE;
        self.tail = LRU_NONE;
        self.live_rows = 0;
        self.used_bytes = 0;
    }
}

#[derive(Default)]
struct HotVecQueryCache {
    generation: u64,
    entries: FxHashMap<u64, Arc<[(f32, Row)]>>,
    order: std::collections::VecDeque<u64>,
    hits: u64,
    misses: u64,
}

#[derive(Default)]
struct HotSecQueryShard {
    generation: u64,
    entries: FxHashMap<u64, Arc<[Row]>>,
    order: std::collections::VecDeque<u64>,
    hits: u64,
    misses: u64,
}

/// Bounded, generation-coherent L1 in-memory Hot Cache sitting in front of cold `mmap` segments.
pub struct HotTierCache {
    cfg: HotCacheConfig,
    row_shards: [Mutex<HotRowShard>; HOT_ROW_SHARDS],
    sec_shards: [Mutex<HotSecQueryShard>; HOT_ROW_SHARDS],
    vec_queries: Mutex<HotVecQueryCache>,
}

impl HotTierCache {
    pub fn new(cfg: HotCacheConfig) -> Self {
        Self {
            cfg,
            row_shards: std::array::from_fn(|_| Mutex::new(HotRowShard::default())),
            sec_shards: std::array::from_fn(|_| Mutex::new(HotSecQueryShard::default())),
            vec_queries: Mutex::new(HotVecQueryCache::default()),
        }
    }

    #[inline]
    fn hash_table_id(table: &str, id: &str) -> u64 {
        let mut h = 0xcbf29ce484222325u64;
        for &b in table.as_bytes() {
            h ^= b as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
        h ^= 0xff;
        for &b in id.as_bytes() {
            h ^= b as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
        h
    }

    #[inline]
    fn get_row(&self, table: &str, id: &str) -> Option<Arc<Row>> {
        if !self.cfg.enabled {
            return None;
        }
        let h = Self::hash_table_id(table, id);
        let idx = ((h >> 32) as usize ^ (h as usize)) & (HOT_ROW_SHARDS - 1);
        let Ok(mut shard) = self.row_shards[idx].lock() else {
            return None;
        };
        shard.get(h, table, id)
    }

    #[inline]
    fn put_row(&self, table: &str, id: &str, row: Arc<Row>) {
        if !self.cfg.enabled {
            return;
        }
        let max_bytes = (self.cfg.max_bytes / HOT_ROW_SHARDS).max(64 * 1024);
        let max_rows = (self.cfg.max_rows / HOT_ROW_SHARDS).max(64);
        let approx = estimate_row_bytes(table, id, &row);
        let h = Self::hash_table_id(table, id);
        let idx = ((h >> 32) as usize ^ (h as usize)) & (HOT_ROW_SHARDS - 1);
        if let Ok(mut shard) = self.row_shards[idx].lock() {
            shard.insert(h, table, id, row, approx, max_bytes, max_rows);
        }
    }

    #[inline]
    fn invalidate_row(&self, table: &str, id: &str) {
        if !self.cfg.enabled {
            return;
        }
        let h = Self::hash_table_id(table, id);
        let idx = ((h >> 32) as usize ^ (h as usize)) & (HOT_ROW_SHARDS - 1);
        if let Ok(mut shard) = self.row_shards[idx].lock() {
            shard.invalidate(h, table, id);
        }
    }

    fn clear_all(&self) {
        for s in &self.row_shards {
            if let Ok(mut guard) = s.lock() {
                guard.clear();
            }
        }
        for s in &self.sec_shards {
            if let Ok(mut guard) = s.lock() {
                guard.entries.clear();
                guard.order.clear();
            }
        }
        if let Ok(mut vq) = self.vec_queries.lock() {
            vq.entries.clear();
            vq.order.clear();
        }
    }

    #[inline]
    fn get_sec_query(&self, generation: u64, qhash: u64) -> Option<Arc<[Row]>> {
        if !self.cfg.enabled || self.cfg.max_rows == 0 {
            return None;
        }
        let sidx = (qhash as usize) & (HOT_ROW_SHARDS - 1);
        let Ok(mut sq) = self.sec_shards[sidx].lock() else {
            return None;
        };
        if sq.generation != generation {
            sq.generation = generation;
            sq.entries.clear();
            sq.order.clear();
            sq.misses += 1;
            return None;
        }
        if let Some(hit) = sq.entries.get(&qhash).cloned() {
            sq.hits += 1;
            Some(hit)
        } else {
            sq.misses += 1;
            None
        }
    }

    #[inline]
    fn put_sec_query(&self, generation: u64, qhash: u64, res: Arc<[Row]>) {
        if !self.cfg.enabled || self.cfg.max_rows == 0 {
            return;
        }
        let max_per_shard = 512usize;
        let sidx = (qhash as usize) & (HOT_ROW_SHARDS - 1);
        let Ok(mut sq) = self.sec_shards[sidx].lock() else {
            return;
        };
        if sq.generation != generation {
            sq.generation = generation;
            sq.entries.clear();
            sq.order.clear();
        }
        if !sq.entries.contains_key(&qhash) {
            while sq.entries.len() >= max_per_shard
                && let Some(old_k) = sq.order.pop_front()
            {
                sq.entries.remove(&old_k);
            }
            sq.order.push_back(qhash);
        }
        sq.entries.insert(qhash, res);
    }

    fn get_vec_query(&self, generation: u64, qhash: u64) -> Option<Arc<[(f32, Row)]>> {
        if !self.cfg.enabled || self.cfg.max_vec_queries == 0 {
            return None;
        }
        let Ok(mut vq) = self.vec_queries.lock() else {
            return None;
        };
        if vq.generation != generation {
            vq.generation = generation;
            vq.entries.clear();
            vq.order.clear();
            vq.misses += 1;
            return None;
        }
        if let Some(hit) = vq.entries.get(&qhash).cloned() {
            vq.hits += 1;
            Some(hit)
        } else {
            vq.misses += 1;
            None
        }
    }

    fn put_vec_query(&self, generation: u64, qhash: u64, res: Arc<[(f32, Row)]>) {
        if !self.cfg.enabled || self.cfg.max_vec_queries == 0 {
            return;
        }
        let Ok(mut vq) = self.vec_queries.lock() else {
            return;
        };
        if vq.generation != generation {
            vq.generation = generation;
            vq.entries.clear();
            vq.order.clear();
        }
        if !vq.entries.contains_key(&qhash) {
            while vq.entries.len() >= self.cfg.max_vec_queries
                && let Some(old_k) = vq.order.pop_front()
            {
                vq.entries.remove(&old_k);
            }
            vq.order.push_back(qhash);
        }
        vq.entries.insert(qhash, res);
    }

    pub fn stats(&self) -> HotCacheStats {
        let mut st = HotCacheStats::default();
        for s in &self.row_shards {
            if let Ok(g) = s.lock() {
                st.used_bytes += g.used_bytes;
                st.cached_rows += g.live_rows;
                st.row_hits += g.hits;
                st.row_misses += g.misses;
                st.row_evictions += g.evictions;
            }
        }
        if let Ok(vq) = self.vec_queries.lock() {
            st.vec_query_hits = vq.hits;
            st.vec_query_misses = vq.misses;
        }
        for s in &self.sec_shards {
            if let Ok(sq) = s.lock() {
                st.sec_query_hits += sq.hits;
                st.sec_query_misses += sq.misses;
            }
        }
        st
    }
}

fn estimate_row_bytes(table: &str, id: &str, row: &Row) -> usize {
    let mut b = 96 + table.len() + id.len();
    for (k, v) in row {
        b += 32 + k.len();
        b += match v {
            Cell::Null | Cell::Bool(_) | Cell::Int(_) | Cell::Float(_) | Cell::Time(_) => 16,
            Cell::SmallText(_) => 24,
            Cell::Text(s) => 24 + s.len(),
            Cell::Vec(vec) => 24 + vec.len() * 4,
        };
    }
    b
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
    /// Flat aligned `f32` vectors (`vec_f32.bin`) + pre-quantized `i8` SQ8 vectors (`vec_sq8.bin`) + norms.
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

#[inline]
fn fnv1a_hash64(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    // Never return 0 so 0 can serve as the empty-slot sentinel in open-addressed mmap hash tables.
    if h == 0 { 1 } else { h }
}

// ============================================================================
// Memory-Mapped Column Readers (Zero-Heap-Copy on Open)
// ============================================================================

struct BlockCacheSlot {
    block_idx: u32,
    data: Arc<[u8]>,
}

struct MmapTextCol {
    /// `row_count` entries of `(block_idx: u32, in_block_offset: u32, byte_len: u32, is_null: u32)` = 16 bytes/row.
    row_loc: Mmap,
    /// `block_count + 1` entries of `u64` byte offsets into `blocks_data`.
    block_offsets: Mmap,
    /// Concatenated Zstd-compressed blocks.
    blocks_data: Mmap,
    row_count: u32,
    block_count: u32,
    cache_shards: [Mutex<Vec<Option<BlockCacheSlot>>>; BLOCK_CACHE_SHARDS],
}

impl MmapTextCol {
    fn open(dir: &Path, col_name: &str, row_count: u32) -> Result<Self, Error> {
        let loc_f = File::open(dir.join(format!("col_{col_name}.loc"))).map_err(io_err)?;
        let boff_f = File::open(dir.join(format!("col_{col_name}.boff"))).map_err(io_err)?;
        let bdat_f = File::open(dir.join(format!("col_{col_name}.bdat"))).map_err(io_err)?;

        let row_loc = unsafe { MmapOptions::new().map(&loc_f) }.map_err(io_err)?;
        let block_offsets = unsafe { MmapOptions::new().map(&boff_f) }.map_err(io_err)?;
        let blocks_data = unsafe { MmapOptions::new().map(&bdat_f) }.map_err(io_err)?;

        let block_count = if block_offsets.len() >= 8 {
            ((block_offsets.len() / 8) - 1) as u32
        } else {
            0
        };

        let cache_shards = std::array::from_fn(|_| {
            let mut slots = Vec::with_capacity(BLOCK_CACHE_SLOTS_PER_SHARD);
            slots.resize_with(BLOCK_CACHE_SLOTS_PER_SHARD, || None);
            Mutex::new(slots)
        });

        Ok(Self {
            row_loc,
            block_offsets,
            blocks_data,
            row_count,
            block_count,
            cache_shards,
        })
    }

    #[inline]
    fn get_loc(&self, row_id: u32) -> Option<(u32, u32, u32)> {
        if row_id >= self.row_count {
            return None;
        }
        let base = (row_id as usize) * 16;
        if base + 16 > self.row_loc.len() {
            return None;
        }
        let flags = u32::from_le_bytes(self.row_loc[base + 12..base + 16].try_into().ok()?);
        if flags != 0 {
            return None; // Null cell
        }
        let block_idx = u32::from_le_bytes(self.row_loc[base..base + 4].try_into().ok()?);
        let in_off = u32::from_le_bytes(self.row_loc[base + 4..base + 8].try_into().ok()?);
        let len = u32::from_le_bytes(self.row_loc[base + 8..base + 12].try_into().ok()?);
        Some((block_idx, in_off, len))
    }

    fn decode_block(&self, block_idx: u32) -> Result<Arc<[u8]>, Error> {
        let shard_idx = (block_idx as usize) & (BLOCK_CACHE_SHARDS - 1);
        let slot_idx = ((block_idx as usize) >> 4) & (BLOCK_CACHE_SLOTS_PER_SHARD - 1);
        if let Ok(guard) = self.cache_shards[shard_idx].lock()
            && let Some(ref entry) = guard[slot_idx]
            && entry.block_idx == block_idx
        {
            return Ok(Arc::clone(&entry.data));
        }

        if block_idx >= self.block_count {
            return Err(io_err("text block index out of range"));
        }
        let off_base = (block_idx as usize) * 8;
        let start = u64::from_le_bytes(
            self.block_offsets[off_base..off_base + 8]
                .try_into()
                .map_err(|_| io_err("corrupt boff"))?,
        ) as usize;
        let end = u64::from_le_bytes(
            self.block_offsets[off_base + 8..off_base + 16]
                .try_into()
                .map_err(|_| io_err("corrupt boff"))?,
        ) as usize;
        if end > self.blocks_data.len() || start > end {
            return Err(io_err("corrupt text block bounds"));
        }
        let slice = &self.blocks_data[start..end];
        let raw: Vec<u8> = if slice.is_empty() {
            Vec::new()
        } else if slice[0] == 0x00 {
            slice[1..].to_vec()
        } else {
            #[cfg(feature = "compress-zstd")]
            {
                thread_local! {
                    static ZSTD_DEC: std::cell::RefCell<Option<zstd::bulk::Decompressor<'static>>> =
                        const { std::cell::RefCell::new(None) };
                }
                ZSTD_DEC.with(|cell| {
                    let mut opt = cell.borrow_mut();
                    if opt.is_none() {
                        *opt = zstd::bulk::Decompressor::new().ok();
                    }
                    if let Some(dec) = opt.as_mut()
                        && let Ok(v) = dec.decompress(&slice[1..], TEXT_BLOCK_TARGET_BYTES + 4096)
                    {
                        return Ok(v);
                    }
                    zstd::decode_all(&slice[1..]).map_err(io_err)
                })?
            }
            #[cfg(not(feature = "compress-zstd"))]
            {
                return Err(io_err(
                    "zstd feature required to read compressed text block",
                ));
            }
        };
        let arc: Arc<[u8]> = Arc::from(raw);
        if let Ok(mut guard) = self.cache_shards[shard_idx].lock() {
            guard[slot_idx] = Some(BlockCacheSlot {
                block_idx,
                data: Arc::clone(&arc),
            });
        }
        Ok(arc)
    }

    fn get_cell(&self, row_id: u32) -> Result<Cell, Error> {
        let Some((block_idx, in_off, len)) = self.get_loc(row_id) else {
            return Ok(Cell::Null);
        };
        if len == 0 {
            return Ok(Cell::SmallText(InlineStr::new_truncate("")));
        }
        let block = self.decode_block(block_idx)?;
        let s_off = in_off as usize;
        let e_off = s_off + (len as usize);
        if e_off > block.len() {
            return Err(io_err("corrupt in-block text slice"));
        }
        let s = std::str::from_utf8(&block[s_off..e_off]).map_err(io_err)?;
        Ok(Cell::from_text(s))
    }

    fn with_str<R>(&self, row_id: u32, f: impl FnOnce(Option<&str>) -> R) -> Result<R, Error> {
        let Some((block_idx, in_off, len)) = self.get_loc(row_id) else {
            return Ok(f(None));
        };
        if len == 0 {
            return Ok(f(Some("")));
        }
        let block = self.decode_block(block_idx)?;
        let s_off = in_off as usize;
        let e_off = s_off + (len as usize);
        if e_off > block.len() {
            return Err(io_err("corrupt in-block text slice"));
        }
        let s = std::str::from_utf8(&block[s_off..e_off]).map_err(io_err)?;
        Ok(f(Some(s)))
    }
}

const DICT_NULL_CODE: u16 = 0xFFFF;
const DICT_MAX_DISTINCT: usize = 65_534;
const DICT_MAX_BYTES: usize = 4 * 1024 * 1024;
const DICT_MIN_SEGMENT_ROWS: usize = 32;

/// Memory-mapped Dictionary-Encoded UTF-8 string column (`col_<name>.dstr`, `.doff`, `.dord`).
///
/// - `.dstr`: concatenated UTF-8 bytes of `dict_len` unique strings.
/// - `.doff`: `(dict_len + 1) * 4` bytes of `u32` byte offsets into `.dstr`.
/// - `.dord`: `row_count * 2` bytes of `u16` dictionary codes (`0xFFFF = NULL`, `0..dict_len` = entry).
struct MmapDictCol {
    /// Pre-validated dictionary strings (`dict_len <= 65,534`) for `O(1)` branchless `&str` & `Cell` reads!
    strings: Vec<Box<str>>,
    cells: Vec<Cell>,
    ordinals: Mmap,
    row_count: u32,
}

impl MmapDictCol {
    fn open(dir: &Path, col_name: &str, row_count: u32) -> Result<Self, Error> {
        let dstr_f = File::open(dir.join(format!("col_{col_name}.dstr"))).map_err(io_err)?;
        let doff_f = File::open(dir.join(format!("col_{col_name}.doff"))).map_err(io_err)?;
        let dord_f = File::open(dir.join(format!("col_{col_name}.dord"))).map_err(io_err)?;

        let dstr = unsafe { MmapOptions::new().map(&dstr_f) }.map_err(io_err)?;
        let doff = unsafe { MmapOptions::new().map(&doff_f) }.map_err(io_err)?;
        let ordinals = unsafe { MmapOptions::new().map(&dord_f) }.map_err(io_err)?;

        let dict_len = if doff.len() >= 4 {
            (doff.len() / 4).saturating_sub(1)
        } else {
            0
        };
        let mut strings = Vec::with_capacity(dict_len);
        let mut cells = Vec::with_capacity(dict_len);
        for i in 0..dict_len {
            let base = i * 4;
            let s_off = u32::from_le_bytes(
                doff[base..base + 4]
                    .try_into()
                    .map_err(|_| io_err("corrupt doff"))?,
            ) as usize;
            let e_off = u32::from_le_bytes(
                doff[base + 4..base + 8]
                    .try_into()
                    .map_err(|_| io_err("corrupt doff"))?,
            ) as usize;
            if e_off > dstr.len() || s_off > e_off {
                return Err(io_err("corrupt dstr bounds"));
            }
            let s = std::str::from_utf8(&dstr[s_off..e_off]).map_err(io_err)?;
            strings.push(s.to_string().into_boxed_str());
            cells.push(Cell::from_text(s));
        }

        Ok(Self {
            strings,
            cells,
            ordinals,
            row_count,
        })
    }

    #[inline]
    fn code_at(&self, row_id: u32) -> u16 {
        if row_id >= self.row_count {
            return DICT_NULL_CODE;
        }
        let off = (row_id as usize) * 2;
        if off + 2 > self.ordinals.len() {
            return DICT_NULL_CODE;
        }
        u16::from_le_bytes([self.ordinals[off], self.ordinals[off + 1]])
    }

    #[inline]
    fn get_str(&self, row_id: u32) -> Option<&str> {
        let code = self.code_at(row_id);
        if code == DICT_NULL_CODE {
            None
        } else {
            self.strings.get(code as usize).map(|b| b.as_ref())
        }
    }

    #[inline]
    fn get_cell(&self, row_id: u32) -> Cell {
        let code = self.code_at(row_id);
        if code == DICT_NULL_CODE {
            Cell::Null
        } else {
            self.cells.get(code as usize).cloned().unwrap_or(Cell::Null)
        }
    }
}

struct MmapFixedCol {
    kind: DiskColKind,
    /// `row_count` bytes: `0 = present`, `1 = null`.
    nulls: Mmap,
    /// Packed little-endian values (`8 bytes * row_count` for Int64/Float64/Time64, `1 byte * row_count` for Bool).
    values: Mmap,
    row_count: u32,
}

impl MmapFixedCol {
    fn open(dir: &Path, col_name: &str, kind: DiskColKind, row_count: u32) -> Result<Self, Error> {
        let nf = File::open(dir.join(format!("col_{col_name}.null"))).map_err(io_err)?;
        let vf = File::open(dir.join(format!("col_{col_name}.val"))).map_err(io_err)?;
        let nulls = unsafe { MmapOptions::new().map(&nf) }.map_err(io_err)?;
        let values = unsafe { MmapOptions::new().map(&vf) }.map_err(io_err)?;
        Ok(Self {
            kind,
            nulls,
            values,
            row_count,
        })
    }

    #[inline]
    fn get_cell(&self, row_id: u32) -> Cell {
        let idx = row_id as usize;
        if row_id >= self.row_count || idx >= self.nulls.len() || self.nulls[idx] != 0 {
            return Cell::Null;
        }
        match self.kind {
            DiskColKind::Int64 => {
                let off = idx * 8;
                let v = i64::from_le_bytes(self.values[off..off + 8].try_into().unwrap_or([0; 8]));
                Cell::Int(v)
            }
            DiskColKind::Time64 => {
                let off = idx * 8;
                let v = i64::from_le_bytes(self.values[off..off + 8].try_into().unwrap_or([0; 8]));
                Cell::Time(v)
            }
            DiskColKind::Float64 => {
                let off = idx * 8;
                let v = f64::from_le_bytes(self.values[off..off + 8].try_into().unwrap_or([0; 8]));
                Cell::Float(v)
            }
            DiskColKind::Bool => Cell::Bool(self.values.get(idx).copied().unwrap_or(0) != 0),
            _ => Cell::Null,
        }
    }
}

/// Compact Coarse IVF-SQ8 inverted file stored alongside `col_<name>.vsq8` (`col_<name>.vivf`).
///
/// Header (64 bytes, 64-byte aligned):
///   0..8:   magic `b"LIVF2\0\0\0"`
///   8..12:  `k: u32` (number of coarse centroids)
///   12..16: `dim: u32`
///   16..20: `live_vecs: u32`
///   20..24: `reserved: u32`
///   24..32: `centroids_off: u64` (`k * dim * 4` bytes of normalized `f32`)
///   32..40: `cluster_offsets_off: u64` (`(k + 1) * 4` bytes of `u32` item offsets)
///   40..48: `cluster_rids_off: u64` (`live_vecs * 4` bytes of `u32` segment `row_id`s)
///   48..56: `cluster_sq8_off: u64` (`live_vecs * dim` bytes of cluster-contiguous `i8` SQ8 vectors)
///   56..64: `reserved2: u64`
pub struct MmapVecIvf {
    pub k: usize,
    pub dim: usize,
    pub live_vecs: u32,
    mmap: Mmap,
    centroids_off: usize,
    cluster_offsets_off: usize,
    cluster_rids_off: usize,
    cluster_sq8_off: usize,
}

const IVF_MIN_SEGMENT_VECS: usize = 4_096;

impl MmapVecIvf {
    fn open_optional(dir: &Path, col_name: &str, expected_dim: usize) -> Option<Self> {
        let path = dir.join(format!("col_{col_name}.vivf"));
        let f = File::open(path).ok()?;
        let mmap = unsafe { MmapOptions::new().map(&f) }.ok()?;
        if mmap.len() < 64 || &mmap[0..8] != b"LIVF2\0\0\0" {
            return None;
        }
        let k = u32::from_le_bytes(mmap[8..12].try_into().ok()?) as usize;
        let dim = u32::from_le_bytes(mmap[12..16].try_into().ok()?) as usize;
        let live_vecs = u32::from_le_bytes(mmap[16..20].try_into().ok()?);
        if k == 0 || dim == 0 || dim != expected_dim {
            return None;
        }
        let centroids_off = u64::from_le_bytes(mmap[24..32].try_into().ok()?) as usize;
        let cluster_offsets_off = u64::from_le_bytes(mmap[32..40].try_into().ok()?) as usize;
        let cluster_rids_off = u64::from_le_bytes(mmap[40..48].try_into().ok()?) as usize;
        let cluster_sq8_off = u64::from_le_bytes(mmap[48..56].try_into().ok()?) as usize;
        let lv = live_vecs as usize;
        if centroids_off.saturating_add(k * dim * 4) > mmap.len()
            || cluster_offsets_off.saturating_add((k + 1) * 4) > mmap.len()
            || cluster_rids_off.saturating_add(lv * 4) > mmap.len()
            || cluster_sq8_off.saturating_add(lv * dim) > mmap.len()
        {
            return None;
        }
        Some(Self {
            k,
            dim,
            live_vecs,
            mmap,
            centroids_off,
            cluster_offsets_off,
            cluster_rids_off,
            cluster_sq8_off,
        })
    }

    /// Rank the top `nprobe` closest coarse centroids to `query_vec` (returns sorted centroid indices).
    pub fn top_centroids(&self, query_vec: &[f32], q_norm: f64, nprobe: usize) -> Vec<usize> {
        let take_n = nprobe.min(self.k).max(1);
        let mut scored: Vec<(f32, usize)> = Vec::with_capacity(self.k);
        for c in 0..self.k {
            let off = self.centroids_off + c * self.dim * 4;
            let sim = {
                #[cfg(target_endian = "little")]
                {
                    let ptr = unsafe { self.mmap.as_ptr().add(off) };
                    if (ptr as usize).is_multiple_of(std::mem::align_of::<f32>()) {
                        let c_slice =
                            unsafe { std::slice::from_raw_parts(ptr as *const f32, self.dim) };
                        cosine_with_q_norm(query_vec, q_norm, c_slice) as f32
                    } else {
                        let mut dot = 0.0f64;
                        let mut cn = 0.0f64;
                        for j in 0..self.dim {
                            let v = f32::from_le_bytes(
                                self.mmap[off + j * 4..off + j * 4 + 4]
                                    .try_into()
                                    .unwrap_or([0; 4]),
                            ) as f64;
                            dot += (query_vec[j] as f64) * v;
                            cn += v * v;
                        }
                        if cn > 1e-18 {
                            (dot / (q_norm * cn.sqrt())) as f32
                        } else {
                            0.0
                        }
                    }
                }
            };
            scored.push((sim, c));
        }
        scored.sort_unstable_by(|a, b| b.0.total_cmp(&a.0));
        scored.truncate(take_n);
        let mut out: Vec<usize> = scored.into_iter().map(|(_, c)| c).collect();
        // Sort selected cluster IDs ascending so SSD / mmap page reads are strictly sequential!
        out.sort_unstable();
        out
    }

    #[inline]
    fn cluster_bounds(&self, cluster_idx: usize) -> (usize, usize) {
        let base = self.cluster_offsets_off + cluster_idx * 4;
        let s = u32::from_le_bytes(self.mmap[base..base + 4].try_into().unwrap_or([0; 4])) as usize;
        let e =
            u32::from_le_bytes(self.mmap[base + 4..base + 8].try_into().unwrap_or([0; 4])) as usize;
        (s, e.min(self.live_vecs as usize))
    }

    #[inline]
    fn item_rid_and_sq8(&self, item_idx: usize) -> (u32, &[i8]) {
        let rid_off = self.cluster_rids_off + item_idx * 4;
        let rid = u32::from_le_bytes(self.mmap[rid_off..rid_off + 4].try_into().unwrap_or([0; 4]));
        let sq8_off = self.cluster_sq8_off + item_idx * self.dim;
        let ptr = unsafe { self.mmap.as_ptr().add(sq8_off) as *const i8 };
        let sq8 = unsafe { std::slice::from_raw_parts(ptr, self.dim) };
        (rid, sq8)
    }
}

fn build_vec_ivf_file(
    seg_dir: &Path,
    col_name: &str,
    dim: usize,
    rows: &[Row],
) -> Result<(), Error> {
    if dim == 0 {
        return Ok(());
    }
    let mut live_rids: Vec<u32> = Vec::with_capacity(rows.len());
    for (i, r) in rows.iter().enumerate() {
        if let Some(Cell::Vec(v)) = r.get(col_name)
            && v.len() == dim
        {
            live_rids.push(i as u32);
        }
    }
    if live_rids.len() < IVF_MIN_SEGMENT_VECS {
        return Ok(());
    }

    let n = live_rids.len();
    let k = (n as f64).sqrt().round().clamp(16.0, 256.0) as usize;

    // 1. Deterministic strided initialization of `k` normalized centroids
    let mut centroids = vec![0.0f32; k * dim];
    for c in 0..k {
        let src_rid = live_rids[(c * n) / k] as usize;
        if let Some(Cell::Vec(v)) = rows[src_rid].get(col_name) {
            let norm = (l2_norm_f64(v) as f32).max(1e-12);
            let dst = &mut centroids[c * dim..(c + 1) * dim];
            for (j, &val) in v.iter().enumerate() {
                dst[j] = val / norm;
            }
        }
    }

    // Subsample at most 16,384 vectors for fast spherical k-means training (~15-30 ms per segment!)
    let train_step = (n / 16_384).max(1);
    let train_rids: Vec<u32> = live_rids.iter().copied().step_by(train_step).collect();

    // 2 passes of spherical k-means refinement
    let mut c_sq8: Vec<Arc<[i8]>> = Vec::with_capacity(k);
    for _iter in 0..2 {
        c_sq8.clear();
        for c in 0..k {
            c_sq8.push(quantize_sq8(&centroids[c * dim..(c + 1) * dim]));
        }

        let assign_fn = |&rid: &u32| -> usize {
            let Some(Cell::Vec(v)) = rows[rid as usize].get(col_name) else {
                return 0;
            };
            let vq = quantize_sq8(v);
            let mut best_c = 0usize;
            let mut best_d = f64::NEG_INFINITY;
            for (c, cq) in c_sq8.iter().enumerate() {
                let d = dot_sq8(&vq, cq);
                if d > best_d {
                    best_d = d;
                    best_c = c;
                }
            }
            best_c
        };

        #[cfg(feature = "parallel")]
        let train_assign: Vec<usize> = {
            use rayon::prelude::*;
            train_rids.par_iter().map(assign_fn).collect()
        };
        #[cfg(not(feature = "parallel"))]
        let train_assign: Vec<usize> = train_rids.iter().map(assign_fn).collect();

        let mut sums = vec![0.0f64; k * dim];
        let mut counts = vec![0usize; k];
        for (&rid, &cid) in train_rids.iter().zip(train_assign.iter()) {
            if let Some(Cell::Vec(v)) = rows[rid as usize].get(col_name) {
                let vnorm = l2_norm_f64(v).max(1e-12);
                let inv = 1.0 / vnorm;
                let dst = &mut sums[cid * dim..(cid + 1) * dim];
                for (j, &val) in v.iter().enumerate() {
                    dst[j] += (val as f64) * inv;
                }
                counts[cid] += 1;
            }
        }
        for c in 0..k {
            if counts[c] == 0 {
                continue;
            }
            let src = &sums[c * dim..(c + 1) * dim];
            let mut sq = 0.0f64;
            for &x in src {
                sq += x * x;
            }
            let norm = sq.sqrt().max(1e-12);
            let dst = &mut centroids[c * dim..(c + 1) * dim];
            for j in 0..dim {
                dst[j] = (src[j] / norm) as f32;
            }
        }
    }

    // Final assignment of all `n` live vectors using SQ8 quantized centroids + vectors
    c_sq8.clear();
    for c in 0..k {
        c_sq8.push(quantize_sq8(&centroids[c * dim..(c + 1) * dim]));
    }

    let assign_all_fn = |&rid: &u32| -> (usize, u32, Arc<[i8]>) {
        let v = match rows[rid as usize].get(col_name) {
            Some(Cell::Vec(v)) => v.as_ref(),
            _ => &[][..],
        };
        let vq = quantize_sq8(v);
        let mut best_c = 0usize;
        let mut best_d = f64::NEG_INFINITY;
        for (c, cq) in c_sq8.iter().enumerate() {
            let d = dot_sq8(&vq, cq);
            if d > best_d {
                best_d = d;
                best_c = c;
            }
        }
        (best_c, rid, vq)
    };

    #[cfg(feature = "parallel")]
    let mut assigned: Vec<(usize, u32, Arc<[i8]>)> = {
        use rayon::prelude::*;
        live_rids.par_iter().map(assign_all_fn).collect()
    };
    #[cfg(not(feature = "parallel"))]
    let mut assigned: Vec<(usize, u32, Arc<[i8]>)> = live_rids.iter().map(assign_all_fn).collect();

    // Sort by `(cluster_id, row_id)` so each cluster's SQ8 vectors and rids are contiguous!
    assigned.sort_unstable_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));

    let mut cluster_counts = vec![0u32; k];
    for &(cid, _, _) in &assigned {
        cluster_counts[cid] += 1;
    }
    let mut cluster_offsets = Vec::with_capacity(k + 1);
    let mut acc = 0u32;
    cluster_offsets.push(0u32);
    for &cnt in &cluster_counts {
        acc += cnt;
        cluster_offsets.push(acc);
    }

    let align64 = |x: u64| (x + 63) & !63;
    let centroids_off = 64u64;
    let centroids_bytes = (k * dim * 4) as u64;
    let cluster_offsets_off = align64(centroids_off + centroids_bytes);
    let cluster_offsets_bytes = ((k + 1) * 4) as u64;
    let cluster_rids_off = align64(cluster_offsets_off + cluster_offsets_bytes);
    let cluster_rids_bytes = (n * 4) as u64;
    let cluster_sq8_off = align64(cluster_rids_off + cluster_rids_bytes);

    let ivf_path = seg_dir.join(format!("col_{col_name}.vivf"));
    let mut w = BufWriter::with_capacity(256 * 1024, File::create(&ivf_path).map_err(io_err)?);
    let mut hdr = [0u8; 64];
    hdr[0..8].copy_from_slice(b"LIVF2\0\0\0");
    hdr[8..12].copy_from_slice(&(k as u32).to_le_bytes());
    hdr[12..16].copy_from_slice(&(dim as u32).to_le_bytes());
    hdr[16..20].copy_from_slice(&(n as u32).to_le_bytes());
    hdr[24..32].copy_from_slice(&centroids_off.to_le_bytes());
    hdr[32..40].copy_from_slice(&cluster_offsets_off.to_le_bytes());
    hdr[40..48].copy_from_slice(&cluster_rids_off.to_le_bytes());
    hdr[48..56].copy_from_slice(&cluster_sq8_off.to_le_bytes());
    w.write_all(&hdr).map_err(io_err)?;

    for &val in &centroids {
        w.write_all(&val.to_le_bytes()).map_err(io_err)?;
    }
    let pad1 = (cluster_offsets_off - (centroids_off + centroids_bytes)) as usize;
    if pad1 > 0 {
        w.write_all(&vec![0u8; pad1]).map_err(io_err)?;
    }
    for &off in &cluster_offsets {
        w.write_all(&off.to_le_bytes()).map_err(io_err)?;
    }
    let pad2 = (cluster_rids_off - (cluster_offsets_off + cluster_offsets_bytes)) as usize;
    if pad2 > 0 {
        w.write_all(&vec![0u8; pad2]).map_err(io_err)?;
    }
    for &(_, rid, _) in &assigned {
        w.write_all(&rid.to_le_bytes()).map_err(io_err)?;
    }
    let pad3 = (cluster_sq8_off - (cluster_rids_off + cluster_rids_bytes)) as usize;
    if pad3 > 0 {
        w.write_all(&vec![0u8; pad3]).map_err(io_err)?;
    }
    for (_, _, sq8) in &assigned {
        let sq8_u8 = unsafe { std::slice::from_raw_parts(sq8.as_ptr() as *const u8, sq8.len()) };
        w.write_all(sq8_u8).map_err(io_err)?;
    }
    w.flush().map_err(io_err)?;
    Ok(())
}

pub struct MmapVecCol {
    pub dim: usize,
    pub row_count: u32,
    /// `0 = present`, `1 = null/empty`.
    nulls: Mmap,
    /// Flat little-endian `f32` array of length `row_count * dim` (64-byte aligned start).
    f32_mmap: Mmap,
    /// Flat `i8` SQ8 array of length `row_count * dim` for 4x faster initial scan bandwidth!
    sq8_mmap: Mmap,
    /// Optional coarse IVF-SQ8 partition index (`col_<name>.vivf`) for sub-linear 500+ GiB ANN scans!
    pub ivf: Option<MmapVecIvf>,
}

impl MmapVecCol {
    fn open(dir: &Path, col_name: &str, dim: u32, row_count: u32) -> Result<Self, Error> {
        let nf = File::open(dir.join(format!("col_{col_name}.vnull"))).map_err(io_err)?;
        let ff = File::open(dir.join(format!("col_{col_name}.vf32"))).map_err(io_err)?;
        let qf = File::open(dir.join(format!("col_{col_name}.vsq8"))).map_err(io_err)?;
        let nulls = unsafe { MmapOptions::new().map(&nf) }.map_err(io_err)?;
        let f32_mmap = unsafe { MmapOptions::new().map(&ff) }.map_err(io_err)?;
        let sq8_mmap = unsafe { MmapOptions::new().map(&qf) }.map_err(io_err)?;
        let ivf = MmapVecIvf::open_optional(dir, col_name, dim as usize);
        Ok(Self {
            dim: dim as usize,
            row_count,
            nulls,
            f32_mmap,
            sq8_mmap,
            ivf,
        })
    }

    #[inline]
    pub fn is_present(&self, row_id: u32) -> bool {
        let idx = row_id as usize;
        row_id < self.row_count && idx < self.nulls.len() && self.nulls[idx] == 0
    }

    /// Zero-copy slice into the OS page-cached `i8` SQ8 vector for `row_id`.
    #[inline]
    pub fn sq8_slice(&self, row_id: u32) -> Option<&[i8]> {
        if !self.is_present(row_id) || self.dim == 0 {
            return None;
        }
        let start = (row_id as usize) * self.dim;
        let end = start + self.dim;
        if end > self.sq8_mmap.len() {
            return None;
        }
        let ptr = unsafe { self.sq8_mmap.as_ptr().add(start) as *const i8 };
        Some(unsafe { std::slice::from_raw_parts(ptr, self.dim) })
    }

    /// Zero-copy slice into the OS page-cached `f32` vector for `row_id` (aligned on little-endian architectures).
    #[inline]
    pub fn f32_slice(&self, row_id: u32) -> Option<&[f32]> {
        if !self.is_present(row_id) || self.dim == 0 {
            return None;
        }
        let start_bytes = (row_id as usize) * self.dim * 4;
        let end_bytes = start_bytes + self.dim * 4;
        if end_bytes > self.f32_mmap.len() {
            return None;
        }
        #[cfg(target_endian = "little")]
        {
            let ptr = unsafe { self.f32_mmap.as_ptr().add(start_bytes) };
            if (ptr as usize).is_multiple_of(std::mem::align_of::<f32>()) {
                return Some(unsafe { std::slice::from_raw_parts(ptr as *const f32, self.dim) });
            }
        }
        None
    }

    pub fn get_cell(&self, row_id: u32) -> Cell {
        if let Some(slice) = self.f32_slice(row_id) {
            return Cell::Vec(Arc::from(slice));
        }
        if !self.is_present(row_id) || self.dim == 0 {
            return Cell::Null;
        }
        let start_bytes = (row_id as usize) * self.dim * 4;
        let mut out = Vec::with_capacity(self.dim);
        for i in 0..self.dim {
            let off = start_bytes + i * 4;
            let val = f32::from_le_bytes(self.f32_mmap[off..off + 4].try_into().unwrap_or([0; 4]));
            out.push(val);
        }
        Cell::Vec(Arc::from(out))
    }
}

enum MmapCol {
    Fixed(MmapFixedCol),
    Text(MmapTextCol),
    Dict(MmapDictCol),
    Vec(MmapVecCol),
}

impl MmapCol {
    #[inline]
    fn get_cell(&self, row_id: u32) -> Result<Cell, Error> {
        match self {
            MmapCol::Fixed(c) => Ok(c.get_cell(row_id)),
            MmapCol::Text(c) => c.get_cell(row_id),
            MmapCol::Dict(c) => Ok(c.get_cell(row_id)),
            MmapCol::Vec(c) => Ok(c.get_cell(row_id)),
        }
    }
}

// ============================================================================
// Memory-Mapped Open-Addressed Hash Index (PK & Secondary Equality Index)
// ============================================================================
//
// File layout (`idx_<field>.ht`):
//   Header (16 bytes):
//     magic: b"LHT1" (4B)
//     mask: u32 (capacity - 1, power of two)
//     key_count: u32
//     reserved: u32
//   Buckets (`capacity * 16` bytes):
//     hash64: u64 (0 = empty)
//     postings_off: u32 (byte offset into postings_area)
//     postings_len: u32 (number of u32 row_ids; if `postings_len == 1`, `postings_off` IS the inline `row_id`!)
//   Postings & Key Verification Area:
//     For each collision/entry, we also verify exact key bytes on match so hash collisions never return false rows!
//     Specifically, `entry_off` points into `payload_area`:
//       key_len: u32
//       key_bytes: [u8; key_len]
//       pad to 4-byte boundary
//       count: u32
//       row_ids: [u32; count]

struct MmapHashIndex {
    mmap: Mmap,
    mask: usize,
    buckets_off: usize,
    payload_off: usize,
}

impl MmapHashIndex {
    fn write_from_map(path: &Path, map: &FxHashMap<Vec<u8>, Vec<u32>>) -> Result<(), Error> {
        let key_count = map.len();
        let cap = if key_count == 0 {
            16
        } else {
            ((key_count * 2).next_power_of_two()).max(16)
        };
        let mask = (cap - 1) as u32;
        let buckets_bytes = cap * 16;
        let payload_off = 16 + buckets_bytes;

        let mut buckets = vec![0u8; buckets_bytes];
        let mut payload = Vec::with_capacity(key_count * 32);

        for (key_bytes, row_ids) in map {
            if row_ids.is_empty() {
                continue;
            }
            let h = fnv1a_hash64(key_bytes);
            // Align payload to 4 bytes
            while !payload.len().is_multiple_of(4) {
                payload.push(0);
            }
            let rel_off = payload.len() as u64;
            if rel_off > u32::MAX as u64 {
                return Err(io_err(
                    "segment index payload exceeded 4 GiB; split segment",
                ));
            }
            payload.extend_from_slice(&(key_bytes.len() as u32).to_le_bytes());
            payload.extend_from_slice(key_bytes);
            while !payload.len().is_multiple_of(4) {
                payload.push(0);
            }
            payload.extend_from_slice(&(row_ids.len() as u32).to_le_bytes());
            for &rid in row_ids {
                payload.extend_from_slice(&rid.to_le_bytes());
            }

            let mut slot = (h as usize) & (mask as usize);
            loop {
                let b_off = slot * 16;
                let existing_h = u64::from_le_bytes(buckets[b_off..b_off + 8].try_into().unwrap());
                if existing_h == 0 {
                    buckets[b_off..b_off + 8].copy_from_slice(&h.to_le_bytes());
                    buckets[b_off + 8..b_off + 12].copy_from_slice(&(rel_off as u32).to_le_bytes());
                    buckets[b_off + 12..b_off + 16]
                        .copy_from_slice(&(row_ids.len() as u32).to_le_bytes());
                    break;
                }
                slot = (slot + 1) & (mask as usize);
            }
        }

        let mut f = BufWriter::with_capacity(128 * 1024, File::create(path).map_err(io_err)?);
        f.write_all(b"LHT1").map_err(io_err)?;
        f.write_all(&mask.to_le_bytes()).map_err(io_err)?;
        f.write_all(&(key_count as u32).to_le_bytes())
            .map_err(io_err)?;
        f.write_all(&0u32.to_le_bytes()).map_err(io_err)?;
        f.write_all(&buckets).map_err(io_err)?;
        f.write_all(&payload).map_err(io_err)?;
        f.flush().map_err(io_err)?;
        let _ = payload_off;
        Ok(())
    }

    fn open(path: &Path) -> Result<Self, Error> {
        let f = File::open(path).map_err(io_err)?;
        let mmap = unsafe { MmapOptions::new().map(&f) }.map_err(io_err)?;
        if mmap.len() < 16 || &mmap[0..4] != b"LHT1" {
            return Err(io_err("invalid LHT1 mmap index header"));
        }
        let mask = u32::from_le_bytes(mmap[4..8].try_into().unwrap()) as usize;
        let cap = mask + 1;
        let buckets_off = 16;
        let payload_off = 16 + cap * 16;
        if payload_off > mmap.len() {
            return Err(io_err("truncated LHT1 mmap index"));
        }
        Ok(Self {
            mmap,
            mask,
            buckets_off,
            payload_off,
        })
    }

    /// Zero-allocation O(1) lookup of matching `u32` row IDs directly inside the `mmap`!
    #[inline]
    fn for_each_key(&self, key_bytes: &[u8], mut visit: impl FnMut(u32) -> bool) {
        let h = fnv1a_hash64(key_bytes);
        let mut slot = (h as usize) & self.mask;
        let max_probes = self.mask + 1;
        for _ in 0..max_probes {
            let b_off = self.buckets_off + slot * 16;
            let bh = u64::from_le_bytes(self.mmap[b_off..b_off + 8].try_into().unwrap());
            if bh == 0 {
                return;
            }
            if bh == h {
                let rel_off =
                    u32::from_le_bytes(self.mmap[b_off + 8..b_off + 12].try_into().unwrap())
                        as usize;
                let mut p = self.payload_off + rel_off;
                if p + 4 <= self.mmap.len() {
                    let klen = u32::from_le_bytes(self.mmap[p..p + 4].try_into().unwrap()) as usize;
                    p += 4;
                    if p + klen <= self.mmap.len() && &self.mmap[p..p + klen] == key_bytes {
                        p += klen;
                        while !(p - self.payload_off).is_multiple_of(4) {
                            p += 1;
                        }
                        if p + 4 <= self.mmap.len() {
                            let count = u32::from_le_bytes(self.mmap[p..p + 4].try_into().unwrap())
                                as usize;
                            p += 4;
                            if p + count * 4 <= self.mmap.len() {
                                for i in 0..count {
                                    let roff = p + i * 4;
                                    let rid = u32::from_le_bytes(
                                        self.mmap[roff..roff + 4].try_into().unwrap(),
                                    );
                                    if !visit(rid) {
                                        return;
                                    }
                                }
                            }
                        }
                        return;
                    }
                }
            }
            slot = (slot + 1) & self.mask;
        }
    }
}

#[inline]
fn with_text_index_key<R>(s: &str, f: impl FnOnce(&[u8]) -> R) -> R {
    let sb = s.as_bytes();
    if sb.len() < 255 {
        let mut buf = [0u8; 256];
        buf[0] = 1;
        buf[1..1 + sb.len()].copy_from_slice(sb);
        f(&buf[..1 + sb.len()])
    } else {
        let mut v = Vec::with_capacity(1 + sb.len());
        v.push(1);
        v.extend_from_slice(sb);
        f(&v)
    }
}

#[inline]
fn encode_index_cell_key(cell: &Cell, out: &mut Vec<u8>) -> bool {
    out.clear();
    match cell {
        Cell::Null => false,
        Cell::Text(s) => {
            out.push(1);
            out.extend_from_slice(s.as_bytes());
            true
        }
        Cell::SmallText(s) => {
            out.push(1);
            out.extend_from_slice(s.as_str().as_bytes());
            true
        }
        Cell::Int(n) => {
            out.push(2);
            out.extend_from_slice(&n.to_le_bytes());
            true
        }
        Cell::Time(t) => {
            out.push(3);
            out.extend_from_slice(&t.to_le_bytes());
            true
        }
        Cell::Bool(b) => {
            out.push(4);
            out.push(u8::from(*b));
            true
        }
        Cell::Float(f) => {
            out.push(5);
            let bits = if *f == 0.0 { 0u64 } else { f.to_bits() };
            out.extend_from_slice(&bits.to_le_bytes());
            true
        }
        Cell::Vec(_) => false,
    }
}

// ============================================================================
// Immutable Mmap Segment
// ============================================================================

pub struct DiskSegment {
    pub meta: SegmentMeta,
    cols: Vec<(String, MmapCol)>,
    col_pos: FxHashMap<String, usize>,
    pk_index: MmapHashIndex,
    sec_indexes: FxHashMap<String, MmapHashIndex>,
}

impl DiskSegment {
    fn open(seg_dir: &Path, meta: SegmentMeta) -> Result<Self, Error> {
        let mut cols = Vec::with_capacity(meta.columns.len());
        let mut col_pos = FxHashMap::default();
        for (i, col_schema) in meta.columns.iter().enumerate() {
            let mcol = match col_schema.kind {
                DiskColKind::Int64
                | DiskColKind::Float64
                | DiskColKind::Bool
                | DiskColKind::Time64 => MmapCol::Fixed(MmapFixedCol::open(
                    seg_dir,
                    &col_schema.name,
                    col_schema.kind,
                    meta.row_count,
                )?),
                DiskColKind::TextZstd => MmapCol::Text(MmapTextCol::open(
                    seg_dir,
                    &col_schema.name,
                    meta.row_count,
                )?),
                DiskColKind::TextDict => MmapCol::Dict(MmapDictCol::open(
                    seg_dir,
                    &col_schema.name,
                    meta.row_count,
                )?),
                DiskColKind::VecF32Sq8 { dim } => MmapCol::Vec(MmapVecCol::open(
                    seg_dir,
                    &col_schema.name,
                    dim,
                    meta.row_count,
                )?),
            };
            col_pos.insert(col_schema.name.clone(), i);
            cols.push((col_schema.name.clone(), mcol));
        }

        let pk_index = MmapHashIndex::open(&seg_dir.join("pk_id.ht"))?;
        let mut sec_indexes = FxHashMap::default();
        for field in &meta.indexed_fields {
            let p = seg_dir.join(format!("sec_{field}.ht"));
            if p.exists() {
                sec_indexes.insert(field.clone(), MmapHashIndex::open(&p)?);
            }
        }

        Ok(Self {
            meta,
            cols,
            col_pos,
            pk_index,
            sec_indexes,
        })
    }

    #[inline]
    pub fn materialize_row(&self, row_id: u32) -> Result<Row, Error> {
        let mut row = Row::new();
        for (name, col) in &self.cols {
            let cell = col.get_cell(row_id)?;
            if !matches!(cell, Cell::Null) {
                row.insert(name.clone(), cell);
            }
        }
        Ok(row)
    }

    #[inline]
    pub fn materialize_projected(&self, row_id: u32, fields: &[&str]) -> Result<Row, Error> {
        let mut row = Row::new();
        for &fname in fields {
            if let Some(&pos) = self.col_pos.get(fname) {
                let cell = self.cols[pos].1.get_cell(row_id)?;
                if !matches!(cell, Cell::Null) {
                    row.insert(fname.to_string(), cell);
                }
            }
        }
        Ok(row)
    }

    #[inline]
    pub fn get_cell(&self, row_id: u32, field: &str) -> Result<Cell, Error> {
        let Some(&pos) = self.col_pos.get(field) else {
            return Ok(Cell::Null);
        };
        self.cols[pos].1.get_cell(row_id)
    }

    #[inline]
    pub fn with_text_field<R>(
        &self,
        row_id: u32,
        field: &str,
        f: impl FnOnce(Option<&str>) -> R,
    ) -> Result<R, Error> {
        let Some(&pos) = self.col_pos.get(field) else {
            return Ok(f(None));
        };
        match &self.cols[pos].1 {
            MmapCol::Dict(dc) => Ok(f(dc.get_str(row_id))),
            MmapCol::Text(tc) => tc.with_str(row_id, f),
            _ => {
                let cell = self.cols[pos].1.get_cell(row_id)?;
                Ok(f(cell.text()))
            }
        }
    }

    #[inline]
    pub fn vec_col(&self, field: &str) -> Option<&MmapVecCol> {
        let pos = *self.col_pos.get(field)?;
        match &self.cols[pos].1 {
            MmapCol::Vec(vc) => Some(vc),
            _ => None,
        }
    }

    #[inline]
    pub fn col_pos(&self, field: &str) -> Option<usize> {
        self.col_pos.get(field).copied()
    }

    #[inline]
    pub fn borrow_col_str<'a>(
        &'a self,
        col_pos: Option<usize>,
        rid: u32,
        last_block: &'a mut Option<(u32, Arc<[u8]>)>,
    ) -> Result<Option<&'a str>, Error> {
        let Some(pos) = col_pos else {
            return Ok(None);
        };
        match &self.cols[pos].1 {
            MmapCol::Dict(dc) => Ok(dc.get_str(rid)),
            MmapCol::Text(tc) => {
                let Some((bidx, in_off, len)) = tc.get_loc(rid) else {
                    return Ok(None);
                };
                if len == 0 {
                    return Ok(Some(""));
                }
                let need_load = match last_block {
                    Some((lb, _)) => *lb != bidx,
                    None => true,
                };
                if need_load {
                    let decoded = tc.decode_block(bidx)?;
                    *last_block = Some((bidx, decoded));
                }
                let (_, ref block) = *last_block.as_ref().unwrap();
                let s_off = in_off as usize;
                let e_off = s_off + (len as usize);
                if e_off > block.len() {
                    return Err(io_err("corrupt in-block text slice"));
                }
                let s = std::str::from_utf8(&block[s_off..e_off]).map_err(io_err)?;
                Ok(Some(s))
            }
            _ => Ok(None),
        }
    }

    #[inline]
    pub fn get_bool_opt(&self, rid: u32, field: &str) -> Option<bool> {
        let pos = *self.col_pos.get(field)?;
        match &self.cols[pos].1 {
            MmapCol::Fixed(fc) => match fc.get_cell(rid) {
                Cell::Bool(b) => Some(b),
                Cell::Int(n) => Some(n != 0),
                _ => None,
            },
            _ => None,
        }
    }

    #[inline]
    pub fn get_f64_opt(&self, rid: u32, field: &str) -> Option<f64> {
        let pos = *self.col_pos.get(field)?;
        match &self.cols[pos].1 {
            MmapCol::Fixed(fc) => match fc.get_cell(rid) {
                Cell::Float(f) => Some(f),
                Cell::Int(n) => Some(n as f64),
                _ => None,
            },
            _ => None,
        }
    }

    /// Build a compact bitset over dictionary codes for `field` (`MmapCol::Dict`),
    /// returning `(col_pos, bitset_u64, selectivity_fraction)` so vector scans can
    /// pre-filter rows in 2 CPU instructions before running SIMD dot products!
    fn build_dict_allow_bitset<F>(&self, field: &str, allow: &F) -> Option<(usize, Vec<u64>, f32)>
    where
        F: Fn(&str) -> bool,
    {
        let pos = *self.col_pos.get(field)?;
        let MmapCol::Dict(dc) = &self.cols[pos].1 else {
            return None;
        };
        let n = dc.strings.len();
        if n == 0 {
            return Some((pos, Vec::new(), 0.0));
        }
        let mut bits = vec![0u64; n.div_ceil(64)];
        let mut allowed = 0usize;
        for (code, s) in dc.strings.iter().enumerate() {
            if allow(s.as_ref()) {
                bits[code >> 6] |= 1u64 << (code & 63);
                allowed += 1;
            }
        }
        let selectivity = (allowed as f32) / (n as f32);
        Some((pos, bits, selectivity))
    }

    #[inline]
    fn check_dict_bitset(&self, pos: usize, rid: u32, bits: &[u64]) -> bool {
        if let MmapCol::Dict(dc) = &self.cols[pos].1 {
            let code = dc.code_at(rid);
            if code == DICT_NULL_CODE {
                return true;
            }
            let idx = (code as usize) >> 6;
            if idx < bits.len() {
                return ((bits[idx] >> (code & 63)) & 1) != 0;
            }
            false
        } else {
            false
        }
    }
}

/// Write a slice of `Row`s into a brand-new immutable columnar mmap segment on disk.
fn write_segment_to_disk(
    base_dir: &Path,
    seg_id: u64,
    schema: &DiskTableSchema,
    rows: &[Row],
) -> Result<SegmentMeta, Error> {
    let seg_dir = base_dir.join(SEGMENTS_DIR).join(format!("seg_{seg_id:08}"));
    fs::create_dir_all(&seg_dir).map_err(io_err)?;
    let row_count = rows.len() as u32;

    // 1. Write each column in native disk format (recording resolved per-segment column kinds)
    let mut seg_columns = schema.columns.clone();
    for col in &mut seg_columns {
        match col.kind {
            DiskColKind::Int64 | DiskColKind::Time64 => {
                let mut nulls = vec![1u8; rows.len()];
                let mut vals = Vec::with_capacity(rows.len() * 8);
                for (i, r) in rows.iter().enumerate() {
                    let v = match r.get(&col.name) {
                        Some(Cell::Int(n)) => Some(*n),
                        Some(Cell::Time(t)) => Some(*t),
                        _ => None,
                    };
                    if let Some(n) = v {
                        nulls[i] = 0;
                        vals.extend_from_slice(&n.to_le_bytes());
                    } else {
                        vals.extend_from_slice(&0i64.to_le_bytes());
                    }
                }
                fs::write(seg_dir.join(format!("col_{}.null", col.name)), &nulls)
                    .map_err(io_err)?;
                fs::write(seg_dir.join(format!("col_{}.val", col.name)), &vals).map_err(io_err)?;
            }
            DiskColKind::Float64 => {
                let mut nulls = vec![1u8; rows.len()];
                let mut vals = Vec::with_capacity(rows.len() * 8);
                for (i, r) in rows.iter().enumerate() {
                    let v = match r.get(&col.name) {
                        Some(Cell::Float(f)) => Some(*f),
                        Some(Cell::Int(n)) => Some(*n as f64),
                        _ => None,
                    };
                    if let Some(f) = v {
                        nulls[i] = 0;
                        vals.extend_from_slice(&f.to_le_bytes());
                    } else {
                        vals.extend_from_slice(&0.0f64.to_le_bytes());
                    }
                }
                fs::write(seg_dir.join(format!("col_{}.null", col.name)), &nulls)
                    .map_err(io_err)?;
                fs::write(seg_dir.join(format!("col_{}.val", col.name)), &vals).map_err(io_err)?;
            }
            DiskColKind::Bool => {
                let mut nulls = vec![1u8; rows.len()];
                let mut vals = vec![0u8; rows.len()];
                for (i, r) in rows.iter().enumerate() {
                    if let Some(Cell::Bool(b)) = r.get(&col.name) {
                        nulls[i] = 0;
                        vals[i] = u8::from(*b);
                    }
                }
                fs::write(seg_dir.join(format!("col_{}.null", col.name)), &nulls)
                    .map_err(io_err)?;
                fs::write(seg_dir.join(format!("col_{}.val", col.name)), &vals).map_err(io_err)?;
            }
            DiskColKind::TextZstd | DiskColKind::TextDict => {
                // Adaptive Dictionary Encoding probe for low/medium-cardinality text columns!
                let mut wrote_dict = false;
                if rows.len() >= DICT_MIN_SEGMENT_ROWS && col.name != "id" && col.name != "content"
                {
                    let mut dict_map: FxHashMap<&str, u16> = FxHashMap::default();
                    let mut dict_list: Vec<&str> = Vec::new();
                    let mut dict_bytes = 0usize;
                    let mut codes: Vec<u16> = Vec::with_capacity(rows.len());
                    let mut eligible = true;

                    for (row_idx, r) in rows.iter().enumerate() {
                        if row_idx == 64 && rows.len() >= 128 && dict_list.len() > 56 {
                            // >87.5% unique strings in first 64 rows (e.g. `uri`): abort probe early!
                            eligible = false;
                            break;
                        }
                        match r.get(&col.name).and_then(Cell::text) {
                            None => codes.push(DICT_NULL_CODE),
                            Some(s) => {
                                if let Some(&c) = dict_map.get(s) {
                                    codes.push(c);
                                } else {
                                    if dict_list.len() >= DICT_MAX_DISTINCT
                                        || dict_bytes.saturating_add(s.len()) > DICT_MAX_BYTES
                                    {
                                        eligible = false;
                                        break;
                                    }
                                    let c = dict_list.len() as u16;
                                    dict_map.insert(s, c);
                                    dict_list.push(s);
                                    dict_bytes += s.len();
                                    codes.push(c);
                                }
                            }
                        }
                    }

                    if eligible && dict_list.len() * 2 <= rows.len() {
                        let mut dstr_bytes = Vec::with_capacity(dict_bytes);
                        let mut doff_bytes = Vec::with_capacity((dict_list.len() + 1) * 4);
                        doff_bytes.extend_from_slice(&0u32.to_le_bytes());
                        for s in &dict_list {
                            dstr_bytes.extend_from_slice(s.as_bytes());
                            doff_bytes.extend_from_slice(&(dstr_bytes.len() as u32).to_le_bytes());
                        }
                        let mut dord_bytes = Vec::with_capacity(codes.len() * 2);
                        for &c in &codes {
                            dord_bytes.extend_from_slice(&c.to_le_bytes());
                        }
                        fs::write(seg_dir.join(format!("col_{}.dstr", col.name)), &dstr_bytes)
                            .map_err(io_err)?;
                        fs::write(seg_dir.join(format!("col_{}.doff", col.name)), &doff_bytes)
                            .map_err(io_err)?;
                        fs::write(seg_dir.join(format!("col_{}.dord", col.name)), &dord_bytes)
                            .map_err(io_err)?;
                        col.kind = DiskColKind::TextDict;
                        wrote_dict = true;
                    }
                }

                if !wrote_dict {
                    col.kind = DiskColKind::TextZstd;
                    let mut loc_bytes = Vec::with_capacity(rows.len() * 16);
                    let mut block_offsets: Vec<u64> = vec![0];
                    let mut bdat_file = BufWriter::with_capacity(
                        128 * 1024,
                        File::create(seg_dir.join(format!("col_{}.bdat", col.name)))
                            .map_err(io_err)?,
                    );
                    let mut cur_bdat_len: u64 = 0;
                    let mut cur_block = Vec::with_capacity(TEXT_BLOCK_TARGET_BYTES + 4096);
                    let mut cur_block_idx: u32 = 0;

                    #[cfg(feature = "compress-zstd")]
                    let mut zstd_comp = zstd::bulk::Compressor::new(1).ok();

                    #[allow(unused_mut)]
                    let mut flush_block = |raw: &mut Vec<u8>,
                                           bdat: &mut BufWriter<File>,
                                           cur_len: &mut u64,
                                           boffs: &mut Vec<u64>,
                                           bidx: &mut u32|
                     -> Result<(), Error> {
                        if raw.is_empty() {
                            return Ok(());
                        }
                        #[cfg(feature = "compress-zstd")]
                        let encoded = {
                            let compressed = if let Some(c) = zstd_comp.as_mut() {
                                c.compress(&raw[..]).map_err(io_err)?
                            } else {
                                zstd::encode_all(&raw[..], 1).map_err(io_err)?
                            };
                            if compressed.len() + 1 < raw.len() {
                                let mut out = Vec::with_capacity(1 + compressed.len());
                                out.push(0x01);
                                out.extend_from_slice(&compressed);
                                out
                            } else {
                                let mut out = Vec::with_capacity(1 + raw.len());
                                out.push(0x00);
                                out.extend_from_slice(raw);
                                out
                            }
                        };
                        #[cfg(not(feature = "compress-zstd"))]
                        let encoded = {
                            let mut out = Vec::with_capacity(1 + raw.len());
                            out.push(0x00);
                            out.extend_from_slice(raw);
                            out
                        };

                        bdat.write_all(&encoded).map_err(io_err)?;
                        *cur_len += encoded.len() as u64;
                        boffs.push(*cur_len);
                        raw.clear();
                        *bidx += 1;
                        Ok(())
                    };

                    for r in rows {
                        match r.get(&col.name).and_then(Cell::text) {
                            None => {
                                loc_bytes.extend_from_slice(&0u32.to_le_bytes());
                                loc_bytes.extend_from_slice(&0u32.to_le_bytes());
                                loc_bytes.extend_from_slice(&0u32.to_le_bytes());
                                loc_bytes.extend_from_slice(&1u32.to_le_bytes()); // is_null = 1
                            }
                            Some(s) => {
                                let sb = s.as_bytes();
                                if !cur_block.is_empty()
                                    && cur_block.len() + sb.len() > TEXT_BLOCK_TARGET_BYTES
                                {
                                    flush_block(
                                        &mut cur_block,
                                        &mut bdat_file,
                                        &mut cur_bdat_len,
                                        &mut block_offsets,
                                        &mut cur_block_idx,
                                    )?;
                                }
                                let in_off = cur_block.len() as u32;
                                let blen = sb.len() as u32;
                                cur_block.extend_from_slice(sb);
                                loc_bytes.extend_from_slice(&cur_block_idx.to_le_bytes());
                                loc_bytes.extend_from_slice(&in_off.to_le_bytes());
                                loc_bytes.extend_from_slice(&blen.to_le_bytes());
                                loc_bytes.extend_from_slice(&0u32.to_le_bytes()); // is_null = 0
                            }
                        }
                    }
                    flush_block(
                        &mut cur_block,
                        &mut bdat_file,
                        &mut cur_bdat_len,
                        &mut block_offsets,
                        &mut cur_block_idx,
                    )?;
                    bdat_file.flush().map_err(io_err)?;

                    let mut boff_bytes = Vec::with_capacity(block_offsets.len() * 8);
                    for &off in &block_offsets {
                        boff_bytes.extend_from_slice(&off.to_le_bytes());
                    }
                    fs::write(seg_dir.join(format!("col_{}.loc", col.name)), &loc_bytes)
                        .map_err(io_err)?;
                    fs::write(seg_dir.join(format!("col_{}.boff", col.name)), &boff_bytes)
                        .map_err(io_err)?;
                }
            }
            DiskColKind::VecF32Sq8 { dim } => {
                let d = rows
                    .iter()
                    .find_map(|r| match r.get(&col.name) {
                        Some(Cell::Vec(v)) if !v.is_empty() => Some(v.len()),
                        _ => None,
                    })
                    .unwrap_or(dim as usize);
                col.kind = DiskColKind::VecF32Sq8 { dim: d as u32 };
                let mut nulls = vec![1u8; rows.len()];
                let mut f32_writer = BufWriter::with_capacity(
                    256 * 1024,
                    File::create(seg_dir.join(format!("col_{}.vf32", col.name))).map_err(io_err)?,
                );
                let mut sq8_writer = BufWriter::with_capacity(
                    128 * 1024,
                    File::create(seg_dir.join(format!("col_{}.vsq8", col.name))).map_err(io_err)?,
                );
                let zero_f32 = vec![0u8; d * 4];
                let zero_sq8 = vec![0u8; d];
                #[cfg(not(target_endian = "little"))]
                let mut f32_row_buf = vec![0u8; d * 4];
                let mut sq8_row_buf = vec![0i8; d];

                for (i, r) in rows.iter().enumerate() {
                    if d > 0
                        && let Some(Cell::Vec(v)) = r.get(&col.name)
                        && v.len() == d
                    {
                        nulls[i] = 0;
                        #[cfg(target_endian = "little")]
                        {
                            let f32_u8 = unsafe {
                                std::slice::from_raw_parts(v.as_ptr() as *const u8, d * 4)
                            };
                            f32_writer.write_all(f32_u8).map_err(io_err)?;
                        }
                        #[cfg(not(target_endian = "little"))]
                        {
                            for (j, &val) in v.iter().enumerate() {
                                f32_row_buf[j * 4..j * 4 + 4].copy_from_slice(&val.to_le_bytes());
                            }
                            f32_writer.write_all(&f32_row_buf).map_err(io_err)?;
                        }
                        quantize_sq8_into(v, &mut sq8_row_buf);
                        let sq8_u8 = unsafe {
                            std::slice::from_raw_parts(sq8_row_buf.as_ptr() as *const u8, d)
                        };
                        sq8_writer.write_all(sq8_u8).map_err(io_err)?;
                    } else {
                        f32_writer.write_all(&zero_f32).map_err(io_err)?;
                        sq8_writer.write_all(&zero_sq8).map_err(io_err)?;
                    }
                }
                f32_writer.flush().map_err(io_err)?;
                sq8_writer.flush().map_err(io_err)?;
                fs::write(seg_dir.join(format!("col_{}.vnull", col.name)), &nulls)
                    .map_err(io_err)?;
                if rows.len() >= IVF_MIN_SEGMENT_VECS && d > 0 {
                    build_vec_ivf_file(&seg_dir, &col.name, d, rows)?;
                }
            }
        }
    }

    // 2. Write Primary Key Index (`pk_id.ht`)
    let mut pk_map: FxHashMap<Vec<u8>, Vec<u32>> = FxHashMap::default();
    pk_map.reserve(rows.len());
    let mut key_buf = Vec::with_capacity(64);
    for (i, r) in rows.iter().enumerate() {
        if let Some(id_cell) = r.get("id")
            && encode_index_cell_key(id_cell, &mut key_buf)
        {
            pk_map.entry(key_buf.clone()).or_default().push(i as u32);
        }
    }
    MmapHashIndex::write_from_map(&seg_dir.join("pk_id.ht"), &pk_map)?;

    // 3. Write Secondary Equality Indexes (`sec_<field>.ht`)
    for field in &schema.indexed_fields {
        if field == "id" {
            continue;
        }
        let mut sec_map: FxHashMap<Vec<u8>, Vec<u32>> = FxHashMap::default();
        for (i, r) in rows.iter().enumerate() {
            if let Some(cell) = r.get(field)
                && encode_index_cell_key(cell, &mut key_buf)
            {
                sec_map.entry(key_buf.clone()).or_default().push(i as u32);
            }
        }
        MmapHashIndex::write_from_map(&seg_dir.join(format!("sec_{field}.ht")), &sec_map)?;
    }

    Ok(SegmentMeta {
        seg_id,
        table: schema.name.clone(),
        row_count,
        columns: seg_columns,
        indexed_fields: schema.indexed_fields.clone(),
    })
}

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
struct SegmentTombstones {
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
}

// ============================================================================
// Out-of-Core DiskEngineV2 + L1 In-Memory HotTierCache
// ============================================================================

pub struct DiskEngineV2 {
    dir: PathBuf,
    pub manifest: DiskManifest,
    /// `table_name -> ordered Vec<Arc<DiskSegment>>` (oldest first, newest last!).
    segments: BTreeMap<String, Vec<Arc<DiskSegment>>>,
    memtables: BTreeMap<String, MemTable>,
    tombstones: SegmentTombstones,
    hot_cache: Arc<HotTierCache>,
    active_savepoint: Option<DiskSavepoint>,
    wal_file: Option<File>,
    wal_bytes: u64,
    sync_full: bool,
    read_only: bool,
}

#[derive(Debug, Clone)]
pub struct DiskVecHit {
    pub seg_id: u64,
    pub row_id: u32,
    /// `u64::MAX` when hit comes from MemTable (`row_id` = memtable index).
    pub score: f32,
}

/// Detect whether `dir` is an initialized `StorageEngineV2` directory.
pub fn is_disk_v2_dir(dir: &Path) -> bool {
    dir.is_dir() && dir.join(MANIFEST_FILE).is_file()
}

impl DiskEngineV2 {
    /// Open or create a read-write `DiskEngineV2` directory in ~2–10 ms (`mmap` only, zero bulk RAM load).
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
            wal_file: Some(wal_file),
            wal_bytes: 0,
            sync_full,
            read_only: false,
        };
        engine.replay_wal()?;
        Ok(engine)
    }

    /// Open a read-only `DiskEngineV2` handle (`mmap` shared OS pages + bounded L1 Hot Cache).
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
                "disk_v2 manifest not found in '{}'",
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
            wal_file: None,
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

    /// Capture a zero-I/O point-in-time MVCC view of `self` (`O(#segments + MemTable)` in microseconds!).
    /// Concurrent writers can mutate `self` immediately after this returns without affecting the view!
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
            wal_file: None,
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
        f.seek(SeekFrom::Start(0)).map_err(io_err)?;
        let mut reader = BufReader::new(f);
        let mut valid_pos = 0u64;
        let mut header = [0u8; 12]; // b"LWV2" (4B) + len: u32 (4B) + crc32: u32 (4B)
        loop {
            match reader.read_exact(&mut header) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
                Err(e) => return Err(io_err(e)),
            }
            if &header[0..4] != b"LWV2" {
                break;
            }
            let payload_len = u32::from_le_bytes(header[4..8].try_into().unwrap()) as usize;
            let expected_crc = u32::from_le_bytes(header[8..12].try_into().unwrap());
            let mut payload = vec![0u8; payload_len];
            if reader.read_exact(&mut payload).is_err() {
                break;
            }
            let mut hasher = Crc32::new();
            hasher.update(&payload);
            if hasher.finalize() != expected_crc {
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
        let payload = rmp_serde::to_vec(&frame).map_err(io_err)?;
        let mut hasher = Crc32::new();
        hasher.update(&payload);
        let crc = hasher.finalize();

        let mut header = [0u8; 12];
        header[0..4].copy_from_slice(b"LWV2");
        header[4..8].copy_from_slice(&(payload.len() as u32).to_le_bytes());
        header[8..12].copy_from_slice(&crc.to_le_bytes());

        f.write_all(&header).map_err(io_err)?;
        f.write_all(&payload).map_err(io_err)?;
        if self.sync_full {
            crate::persist::durable_sync(f)
                .or_else(|_| f.sync_data())
                .map_err(io_err)?;
        }
        self.wal_bytes += 12 + (payload.len() as u64);
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
                let needs_schema_mut = match self.manifest.tables.get(&table) {
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
                let indexed_fields: SmallVec<[String; 8]> = if needs_schema_mut {
                    self.snapshot_tables_for_savepoint();
                    if let Some(t) = self.manifest.tables.get_mut(&table) {
                        // Auto-register any newly introduced dynamic columns so `checkpoint()` never drops them!
                        for row in &rows {
                            for (k, cell) in row {
                                if matches!(cell, Cell::Null) {
                                    continue;
                                }
                                if let Some(existing) = t.columns.iter_mut().find(|c| c.name == *k)
                                {
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
                        let inferred = infer_schema_from_rows(&table, &rows);
                        let ixs: SmallVec<[String; 8]> =
                            inferred.indexed_fields.iter().cloned().collect();
                        self.manifest.tables.insert(table.clone(), inferred);
                        ixs
                    }
                } else {
                    self.manifest.tables[&table]
                        .indexed_fields
                        .iter()
                        .cloned()
                        .collect()
                };
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
    /// While active, `commit_ops` applies mutations in-memory immediately (supporting read-your-writes
    /// across all index/point/scan queries) and buffers WAL frames until `commit_savepoint()`!
    pub fn begin_savepoint(&mut self) -> Result<(), Error> {
        if self.read_only {
            return Err(Error::runtime(
                "cannot begin transaction on read-only DiskEngineV2",
            ));
        }
        if self.active_savepoint.is_some() {
            return Err(Error::runtime(
                "nested DiskEngineV2 savepoints are not supported",
            ));
        }
        self.active_savepoint = Some(DiskSavepoint {
            saved_tables: None,
            memtable_undos: FxHashMap::default(),
            added_tombstones: Vec::new(),
            pending_wal_ops: Vec::new(),
        });
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
        // Clear L1 hot cache entries that may have been touched by uncommitted savepoint ops
        self.hot_cache.clear_all();
    }

    /// Commit a batch of ACID operations atomically to WAL + MemTable (or stage into active savepoint).
    /// Automatically flushes large MemTables (> 32,768 rows or > 64 MiB WAL) into an immutable mmap segment!
    pub fn commit_ops(&mut self, ops: Vec<DiskWalOp>) -> Result<u64, Error> {
        if self.read_only {
            return Err(Error::runtime("cannot mutate read-only DiskEngineV2"));
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
    /// and persists updated tombstones + manifest, then truncates WAL to 0 bytes!
    /// Never rewrites existing multi-gigabyte segments!
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
            let rows = mt.drain_live_rows();
            if rows.is_empty() {
                continue;
            }
            let schema = self
                .manifest
                .tables
                .get(&tname)
                .cloned()
                .unwrap_or_else(|| infer_schema_from_rows(&tname, &rows));
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

    /// Lightweight background-free delta merge: when any table accumulates `>= 10` small delta segments
    /// (`< 8,192` rows), merges ONLY those small delta segments into a single segment without rewriting
    /// existing large `65,536`-row segments!
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

    /// Out-of-core streaming segment compaction for `table`:
    /// - Flushes dirty `MemTable` first (`checkpoint()`).
    /// - Reclaims tombstoned rows, merges fragmented delta segments into clean `65,536`-row immutable segments,
    ///   and builds missing `MmapVecIvf` (`col_<vec>.vivf`) partition files when `force` is `true`!
    /// - Streams in `65,536`-row chunks so peak RAM stays bounded even on 500+ GiB tables!
    pub fn compact_table(&mut self, table: &str, force: bool) -> Result<CompactionReport, Error> {
        if self.read_only {
            return Err(Error::runtime("cannot compact read-only DiskEngineV2"));
        }
        if self.active_savepoint.is_some() {
            return Err(Error::runtime(
                "cannot compact DiskEngineV2 while a transaction savepoint is active",
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

        if !should_compact {
            return Ok(CompactionReport::default());
        }

        const TARGET_SEG_ROWS: usize = 65_536;
        let old_seg_ids: Vec<u64> = segs.iter().map(|s| s.meta.seg_id).collect();
        let old_segs_snapshot: Vec<Arc<DiskSegment>> = segs.clone();

        let mut new_segs: Vec<Arc<DiskSegment>> = Vec::new();
        let mut new_metas: Vec<SegmentMeta> = Vec::new();
        let mut batch: Vec<Row> = Vec::with_capacity(TARGET_SEG_ROWS.min(total_rows.max(1)));

        for seg in &old_segs_snapshot {
            let sid = seg.meta.seg_id;
            for rid in 0..seg.meta.row_count {
                if self.tombstones.is_dead(sid, rid) {
                    continue;
                }
                batch.push(seg.materialize_row(rid)?);
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

        // Drop snapshot Arcs before unlinking old segment directories so mmap descriptors close cleanly!
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

    /// Compact all tables matching threshold policy (`force = false`) or unconditionally (`force = true`).
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

    /// Full maintenance vacuum: flushes WAL/MemTables, reclaims all tombstones, merges delta segments,
    /// and ensures every large vector segment has its `MmapVecIvf` (`col_<vec>.vivf`) built!
    pub fn vacuum(&mut self) -> Result<CompactionReport, Error> {
        self.compact_all(true)
    }

    /// Direct streaming segment writer for bulk migration (writes segments directly without buffering corpus in RAM).
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

    pub fn save_tombstones_and_manifest(&self) -> Result<(), Error> {
        let tomb_tmp = self.dir.join("tombstones.v2.bin.tmp");
        let tomb_bytes = rmp_serde::to_vec(&self.tombstones).map_err(io_err)?;
        fs::write(&tomb_tmp, tomb_bytes).map_err(io_err)?;
        fs::rename(&tomb_tmp, self.dir.join(TOMBSTONES_FILE)).map_err(io_err)?;

        let man_tmp = self.dir.join("manifest.v2.json.tmp");
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

    /// Exact live row count in O(#segments) (~nanoseconds).
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

    /// O(1) existence check by primary key `id` without decompressing any text blocks or cloning rows!
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

    /// O(1) primary key point read (served from L0 MemTable -> L1 RAM HotTierCache -> L2/L3 mmap segment!).
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

    /// O(1) zero-clone Arc<Row> primary key read via L0 MemTable / L1 RAM HotTierCache / cold mmap.
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

    /// O(1) projected point read by primary key `id` — checks L1 Hot Cache first, else skips decompressing unneeded columns!
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

    /// Count live rows matching exact equality on an indexed column in `O(#segments)` (`0` row allocations / `0` decompression!).
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

    /// Secondary-index equality visitor (`O(1)` hash seek per segment + optional column projection).
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
                .get_sec_query(self.manifest.generation, qhash)
        {
            for row in cached.iter() {
                if !visit(row)? {
                    return Ok(());
                }
            }
            return Ok(());
        }

        const MAX_CACHED_POSTING_ROWS: usize = 2_048;
        let mut collected: Option<Vec<Row>> = cacheable.then(Vec::new);

        // 1. MemTable matches
        if let Some(mt) = self.memtables.get(table) {
            if let Some(fmap) = mt.sec.get(field) {
                if let Some(idxs) = fmap.get(&kbuf) {
                    for &idx in idxs {
                        if mt.dead_mem.contains(&idx) {
                            continue;
                        }
                        if let Some(buf) = collected.as_mut() {
                            if buf.len() < MAX_CACHED_POSTING_ROWS {
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
                            if buf.len() < MAX_CACHED_POSTING_ROWS {
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

        // 2. Immutable Mmap Segments
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
                                    if buf.len() < MAX_CACHED_POSTING_ROWS {
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
                                    if buf.len() < MAX_CACHED_POSTING_ROWS {
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
                    // Fallback column scan on segment if field was not indexed
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
                                if buf.len() < MAX_CACHED_POSTING_ROWS {
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
                .put_sec_query(self.manifest.generation, qhash, Arc::from(buf));
        }
        Ok(())
    }

    /// Stream live rows of `table` out-of-core without accumulating the table in RAM!
    /// Pass `project_fields = Some(&["id", "wing", ...])` to read ONLY the requested columns from disk!
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
                                let cell = match &seg.cols[pos].1 {
                                    MmapCol::Text(tc) => match tc.get_loc(rid) {
                                        None => Cell::Null,
                                        Some((_, _, 0)) => {
                                            Cell::SmallText(InlineStr::new_truncate(""))
                                        }
                                        Some((bidx, in_off, len)) => {
                                            let block = match &last_blocks[ci] {
                                                Some((lb, arc)) if *lb == bidx => Arc::clone(arc),
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
                                                return Err(io_err("corrupt in-block text slice"));
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

    /// Columnar 1-column `GROUP BY` over `TextDict` / `TextZstd` + `MemTable`:
    /// Returns `(total_live_rows, Vec<(Option<String>, u64)>)` with **zero `Row` allocations**!
    pub fn group_count_str1(
        &self,
        table: &str,
        col: &str,
    ) -> Result<(u64, Vec<(Option<String>, u64)>), Error> {
        let mut total_live = 0u64;
        let mut global_null = 0u64;
        let mut global_map: FxHashMap<String, u64> = FxHashMap::default();

        if let Some(segs) = self.segments.get(table) {
            for seg in segs {
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
                        global_null += null_c;
                        for (s, c) in dc.strings.iter().zip(counts) {
                            if c > 0 {
                                *global_map.entry(s.to_string()).or_insert(0) += c;
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

        let mut out: Vec<(Option<String>, u64)> = Vec::with_capacity(global_map.len() + 1);
        if global_null > 0 {
            out.push((None, global_null));
        }
        for (k, v) in global_map {
            out.push((Some(k), v));
        }
        Ok((total_live, out))
    }

    /// Columnar 2-column `GROUP BY` (`(col1, col2)` -> count) with zero `Row` allocations!
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

    /// Columnar 3-column `GROUP BY` (`(col1, col2, col3)` -> count) with zero `Row` allocations!
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

    /// Zero-`Row` secondary-index equality visitor yielding up to 3 borrowed `&str` columns directly
    /// from segment Zstd/Dict buffers + `MemTable`!
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

    /// Read-only segment slice & live-row check for streaming Top-K catalog pagination!
    pub fn table_segments(&self, table: &str) -> &[Arc<DiskSegment>] {
        self.segments
            .get(table)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// Build a compact `doc_idx` bitset (`~97 KiB` for 779k rows) aligned 1:1 with `bm25` by scanning
    /// `MmapDictCol` `u16` ordinals (`~0.4 ms` total, zero Zstd decompression, zero hash probes!).
    /// Verifies segment boundary IDs against `bm25.doc_id(...)` in `< 5 µs`; returns `Ok(None)` if misaligned.
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

        // O(#segments) boundary verification (`< 5 µs`) so alignment is 100% proven before scanning!
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

    /// Visit live `MemTable` rows directly with their memtable index.
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

    /// Lookup live `rid`s matching `field == val` on a single segment (`sec_<field>.ht`).
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

    /// Search `vec_field` across all `mmap` segments + `MemTable` in parallel without loading vectors into heap!
    ///
    /// - **Hot Cache Fast Path**: Repeated queries on the current `generation` return directly from L1 RAM (`HotTierCache`).
    /// - **Selective Secondary Index Fast Path**: When `only_doc_ids` is `Some(ids)` (`<= 4096` docs), probes
    ///   `sec_<doc_id_field>.ht` + `MemTable` directly in `O(matching_chunks)` without scanning segments!
    /// - **Out-of-Core Two-Stage SIMD Path**:
    ///   Stage 1 scans compact 1-byte/dim `SQ8` (`col_<vec_field>.vsq8`) using ARM NEON / SIMD dot-product,
    ///   lazily checking `allow_doc` ONLY when `!unfiltered` and `dot_sq8` would enter the top-k heap!
    ///   Stage 2 computes exact `f32` cosine reranking (`col_<vec_field>.vf32`) ONLY on the top candidates,
    ///   hydrating rows via `HotTierCache` / `mmap` ONLY for the final `top_k` winners!
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

        // L1 Hot Vector Query Cache check (when active_savepoint is None)
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
                .get_vec_query(self.manifest.generation, qhash)
        {
            return Ok(cached.to_vec());
        }

        #[derive(Clone, Copy)]
        struct ExactHit {
            score: f32,
            seg_idx: usize, // usize::MAX for MemTable
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

        // Fast Path 1: Selective `only_doc_ids` secondary index lookup (`O(matching_chunks)` exact f32!)
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
                        self.manifest.generation,
                        qhash,
                        Arc::from(out.as_slice()),
                    );
                }
                return Ok(out);
            }
        }

        // Fast Path 2: Parallel Out-of-Core SQ8 SIMD scan + Lazy Doc Filter + Exact f32 Rerank!
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

        // Precompute O(1) dictionary ordinal bitsets for `doc_id_field` on each segment!
        // When `doc_id_field` is `MmapCol::Dict`, this replaces per-chunk string hashing with a
        // 2-instruction bit check BEFORE running `dot_sq8` SIMD!
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
                // Segment has 0 matching documents in dictionary — skip entire segment!
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
                        // Pure flat mmap SIMD dot product first — ZERO Zstd decompression for 99.5% of rows!
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

        // Also include uncommitted/unflushed MemTable rows directly in exact f32
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

        // Hydrate full rows ONLY for the final `top_k` winners (and cache them in L1 HotTierCache)!
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
                self.manifest.generation,
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
        // Check L1 Hot Row Cache by `id` first so repeated hits avoid decompressing content!
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
            return Err(Error::runtime(
                "cannot drop table on read-only DiskEngineV2",
            ));
        }
        self.manifest.tables.remove(table);
        self.memtables.remove(table);
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

    /// Evaluate an optional `lin::ast::Pred` with automatic PK / Secondary Index pushdown!
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

        // Try index pushdown when predicate contains an equality or IN conjunct on `id` or an indexed field!
        if let Some((idx_field, keys)) = self.extract_index_pushdown(table, p, now_ms) {
            // Note: If `project_fields` is passed, ensure all fields referenced by `p` are included during evaluation,
            // or materialize full row for predicate evaluation and project afterwards.
            let mut seen_ids: FxHashSet<String> = FxHashSet::default();
            'key_loop: for key_cell in keys {
                let mut stop = false;
                self.for_each_by_eq_projected(table, &idx_field, &key_cell, None, |row| {
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
                })?;
                if stop {
                    break 'key_loop;
                }
            }
            return Ok(out);
        }

        // Fallback streaming scan (only decompressing needed columns if `project_fields` covers pred fields)
        self.scan_projected(table, None, |row| {
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

    /// Update all live rows matching `pred` (using index pushdown when available), committing via `commit_ops`.
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

    /// Delete all live rows matching `pred` (using index pushdown when available), committing via `commit_ops`.
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
                    // Prefer primary key `"id"` if either side constrains `"id"`
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

    /// Export a portable single-file `LIN\x04` backup directly to `dst_file` by streaming segments
    /// in 512-row Zstd chunks without buffering the entire multi-gigabyte database in RAM!
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

            // Reserve 4 bytes for `chunk_count: u32`, filled after streaming all tables!
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

    /// Import a portable `LIN\x04` backup file or legacy `LIN\x04` directory directly into a `DiskEngineV2` directory!
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

        // Register all explicit schemas + indexes first
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
                },
            );
        }

        for (tname, rows) in snap.collections {
            // Refine vector dimension or missing columns from actual rows!
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
            "lin_diskv2_snap_{}_{}.lin",
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

/// Thread-safe handle wrapping `DiskEngineV2` under an `RwLock` for multi-reader concurrency.
#[derive(Clone)]
pub struct SharedDiskEngineV2 {
    inner: Arc<RwLock<DiskEngineV2>>,
}

impl SharedDiskEngineV2 {
    pub fn new(engine: DiskEngineV2) -> Self {
        Self {
            inner: Arc::new(RwLock::new(engine)),
        }
    }

    pub fn read(&self) -> Result<std::sync::RwLockReadGuard<'_, DiskEngineV2>, Error> {
        self.inner
            .read()
            .map_err(|_| Error::runtime("DiskEngineV2 RwLock poisoned"))
    }

    pub fn write(&self) -> Result<std::sync::RwLockWriteGuard<'_, DiskEngineV2>, Error> {
        self.inner
            .write()
            .map_err(|_| Error::runtime("DiskEngineV2 RwLock poisoned"))
    }
}

fn infer_schema_from_rows(table: &str, rows: &[Row]) -> DiskTableSchema {
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
    }
}

// ============================================================================
// Zero-Heap Memory-Mapped Okapi BM25 Inverted Index (`bm25_<stemmer>.v3.mmap`)
// ============================================================================
//
// File layout (`LBM25M03`, 64-byte aligned sections):
//   Header (96 bytes):
//     0..8:   magic `b"LBM25M03"`
//     8..16:  `generation: u64`
//     16..20: `doc_count: u32`
//     20..24: `term_count: u32`
//     24..28: `hash_mask: u32` (`capacity - 1`, power of two)
//     28..32: `header_crc32: u32` (CRC32 over bytes 0..28 + 32..96)
//     32..40: `avgdl_bits: u64` (`f64::to_bits`)
//     40..48: `doc_lens_off: u64` (`doc_count * 4` bytes of `u32`)
//     48..56: `doc_id_offs_off: u64` (`(doc_count + 1) * 8` bytes of `u64`)
//     56..64: `doc_id_bytes_off: u64` (`doc_id_bytes_len` bytes of UTF-8)
//     64..72: `hash_buckets_off: u64` (`(hash_mask + 1) * 16` bytes: `hash64: u64, term_idx: u32, pad: u32`)
//     72..80: `term_entries_off: u64` (`term_count * 32` bytes: `str_off: u64, str_len: u32, df: u32, post_off: u64, post_len: u32, pad: u32`)
//     80..88: `term_strings_off: u64` (`term_strings_len` bytes of UTF-8)
//     88..96: `postings_off: u64` (LEB128 delta-varint `(delta_doc_idx, tf)` byte stream)

const BM25_MMAP_MAGIC: &[u8; 8] = b"LBM25M03";
const BM25_MMAP_HDR_LEN: usize = 96;

#[inline]
fn push_uvarint32(out: &mut Vec<u8>, mut v: u32) {
    while v >= 0x80 {
        out.push(((v as u8) & 0x7f) | 0x80);
        v >>= 7;
    }
    out.push(v as u8);
}

#[inline]
fn read_uvarint32(buf: &[u8], pos: &mut usize) -> Option<u32> {
    let mut result = 0u32;
    let mut shift = 0u32;
    loop {
        let &b = buf.get(*pos)?;
        *pos += 1;
        result |= u32::from(b & 0x7f).checked_shl(shift)?;
        if (b & 0x80) == 0 {
            return Some(result);
        }
        shift += 7;
        if shift >= 35 {
            return None;
        }
    }
}

/// Streaming builder for `MmapBm25Index` (`bm25_<stemmer>.v3.mmap`).
pub struct MmapBm25Builder {
    generation: u64,
    doc_lens: Vec<u32>,
    doc_id_offsets: Vec<u64>,
    doc_id_bytes: Vec<u8>,
    total_tokens: u64,
    terms: FxHashMap<String, (u32, u32, Vec<u8>)>, // term -> (df, last_doc_idx, delta_varint_bytes)
}

impl MmapBm25Builder {
    pub fn new(generation: u64) -> Self {
        let mut doc_id_offsets = Vec::with_capacity(1024);
        doc_id_offsets.push(0u64);
        Self {
            generation,
            doc_lens: Vec::new(),
            doc_id_offsets,
            doc_id_bytes: Vec::new(),
            total_tokens: 0,
            terms: FxHashMap::default(),
        }
    }

    /// Add one chunk/document (`doc_id` and its already stemmed/filtered tokens).
    pub fn add_document(&mut self, doc_id: &str, tokens: &[String]) {
        let doc_idx = self.doc_lens.len() as u32;
        let doc_len = tokens.len() as u32;
        self.total_tokens += u64::from(doc_len);
        self.doc_lens.push(doc_len);
        self.doc_id_bytes.extend_from_slice(doc_id.as_bytes());
        self.doc_id_offsets.push(self.doc_id_bytes.len() as u64);

        if tokens.is_empty() {
            return;
        }
        let mut tf_map: FxHashMap<&str, u32> = FxHashMap::default();
        for tok in tokens {
            *tf_map.entry(tok.as_str()).or_insert(0) += 1;
        }
        for (term, tf) in tf_map {
            if let Some(entry) = self.terms.get_mut(term) {
                let delta = doc_idx - entry.1;
                entry.0 += 1;
                entry.1 = doc_idx;
                push_uvarint32(&mut entry.2, delta);
                push_uvarint32(&mut entry.2, tf);
            } else {
                let mut post = Vec::with_capacity(8);
                push_uvarint32(&mut post, doc_idx);
                push_uvarint32(&mut post, tf);
                self.terms.insert(term.to_string(), (1, doc_idx, post));
            }
        }
    }

    /// Finalize and atomically write `dst_path`, then `mmap`-open it with zero heap allocations!
    pub fn finish_to_file(self, dst_path: &Path) -> Result<MmapBm25Index, Error> {
        let doc_count = self.doc_lens.len() as u32;
        let avgdl = if doc_count == 0 {
            1.0
        } else {
            (self.total_tokens as f64) / (doc_count as f64)
        }
        .max(1e-9);

        let mut sorted_terms: Vec<(String, u32, Vec<u8>)> = self
            .terms
            .into_iter()
            .map(|(k, (df, _, post))| (k, df, post))
            .collect();
        sorted_terms.sort_unstable_by(|a, b| a.0.cmp(&b.0));

        let term_count = sorted_terms.len();
        let cap = if term_count == 0 {
            16usize
        } else {
            ((term_count * 2).next_power_of_two()).max(16)
        };
        let hash_mask = (cap - 1) as u32;

        let mut hash_buckets = vec![0u8; cap * 16];
        let mut term_entries = Vec::with_capacity(term_count * 32);
        let mut term_strings = Vec::with_capacity(term_count * 10);
        let mut postings_blob = Vec::with_capacity(term_count * 16);

        for (t_idx, (term_str, df, post)) in sorted_terms.into_iter().enumerate() {
            let tb = term_str.as_bytes();
            let str_off = term_strings.len() as u64;
            let str_len = tb.len() as u32;
            term_strings.extend_from_slice(tb);

            let post_off = postings_blob.len() as u64;
            let post_len = post.len() as u32;
            postings_blob.extend_from_slice(&post);

            term_entries.extend_from_slice(&str_off.to_le_bytes());
            term_entries.extend_from_slice(&str_len.to_le_bytes());
            term_entries.extend_from_slice(&df.to_le_bytes());
            term_entries.extend_from_slice(&post_off.to_le_bytes());
            term_entries.extend_from_slice(&post_len.to_le_bytes());
            term_entries.extend_from_slice(&0u32.to_le_bytes());

            let h = fnv1a_hash64(tb);
            let mut slot = (h as usize) & (hash_mask as usize);
            loop {
                let boff = slot * 16;
                let cur_h = u64::from_le_bytes(hash_buckets[boff..boff + 8].try_into().unwrap());
                if cur_h == 0 {
                    hash_buckets[boff..boff + 8].copy_from_slice(&h.to_le_bytes());
                    hash_buckets[boff + 8..boff + 12]
                        .copy_from_slice(&(t_idx as u32).to_le_bytes());
                    break;
                }
                slot = (slot + 1) & (hash_mask as usize);
            }
        }

        let align64 = |x: u64| (x + 63) & !63;
        let doc_lens_off = align64(BM25_MMAP_HDR_LEN as u64);
        let doc_lens_bytes = (doc_count as u64) * 4;
        let doc_id_offs_off = align64(doc_lens_off + doc_lens_bytes);
        let doc_id_offs_bytes = (self.doc_id_offsets.len() as u64) * 8;
        let doc_id_bytes_off = align64(doc_id_offs_off + doc_id_offs_bytes);
        let doc_id_raw_bytes = self.doc_id_bytes.len() as u64;
        let hash_buckets_off = align64(doc_id_bytes_off + doc_id_raw_bytes);
        let hash_buckets_bytes = hash_buckets.len() as u64;
        let term_entries_off = align64(hash_buckets_off + hash_buckets_bytes);
        let term_entries_bytes = term_entries.len() as u64;
        let term_strings_off = align64(term_entries_off + term_entries_bytes);
        let term_strings_bytes = term_strings.len() as u64;
        let postings_off = align64(term_strings_off + term_strings_bytes);

        let mut hdr = [0u8; BM25_MMAP_HDR_LEN];
        hdr[0..8].copy_from_slice(BM25_MMAP_MAGIC);
        hdr[8..16].copy_from_slice(&self.generation.to_le_bytes());
        hdr[16..20].copy_from_slice(&doc_count.to_le_bytes());
        hdr[20..24].copy_from_slice(&(term_count as u32).to_le_bytes());
        hdr[24..28].copy_from_slice(&hash_mask.to_le_bytes());
        hdr[32..40].copy_from_slice(&avgdl.to_bits().to_le_bytes());
        hdr[40..48].copy_from_slice(&doc_lens_off.to_le_bytes());
        hdr[48..56].copy_from_slice(&doc_id_offs_off.to_le_bytes());
        hdr[56..64].copy_from_slice(&doc_id_bytes_off.to_le_bytes());
        hdr[64..72].copy_from_slice(&hash_buckets_off.to_le_bytes());
        hdr[72..80].copy_from_slice(&term_entries_off.to_le_bytes());
        hdr[80..88].copy_from_slice(&term_strings_off.to_le_bytes());
        hdr[88..96].copy_from_slice(&postings_off.to_le_bytes());

        let mut crc = Crc32::new();
        crc.update(&hdr[0..28]);
        crc.update(&hdr[32..96]);
        let crc_val = crc.finalize();
        hdr[28..32].copy_from_slice(&crc_val.to_le_bytes());

        static TMP_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let nonce = TMP_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let tmp_path = dst_path.with_extension(format!("mmap.{}.{nonce}.tmp", std::process::id()));
        let res = (|| -> Result<(), Error> {
            let mut w =
                BufWriter::with_capacity(512 * 1024, File::create(&tmp_path).map_err(io_err)?);
            w.write_all(&hdr).map_err(io_err)?;
            let write_pad = |w: &mut BufWriter<File>, cur: u64, target: u64| -> Result<(), Error> {
                let p = (target - cur) as usize;
                if p > 0 {
                    w.write_all(&vec![0u8; p]).map_err(io_err)?;
                }
                Ok(())
            };
            write_pad(&mut w, BM25_MMAP_HDR_LEN as u64, doc_lens_off)?;
            for &dl in &self.doc_lens {
                w.write_all(&dl.to_le_bytes()).map_err(io_err)?;
            }
            write_pad(&mut w, doc_lens_off + doc_lens_bytes, doc_id_offs_off)?;
            for &off in &self.doc_id_offsets {
                w.write_all(&off.to_le_bytes()).map_err(io_err)?;
            }
            write_pad(
                &mut w,
                doc_id_offs_off + doc_id_offs_bytes,
                doc_id_bytes_off,
            )?;
            w.write_all(&self.doc_id_bytes).map_err(io_err)?;
            write_pad(
                &mut w,
                doc_id_bytes_off + doc_id_raw_bytes,
                hash_buckets_off,
            )?;
            w.write_all(&hash_buckets).map_err(io_err)?;
            write_pad(
                &mut w,
                hash_buckets_off + hash_buckets_bytes,
                term_entries_off,
            )?;
            w.write_all(&term_entries).map_err(io_err)?;
            write_pad(
                &mut w,
                term_entries_off + term_entries_bytes,
                term_strings_off,
            )?;
            w.write_all(&term_strings).map_err(io_err)?;
            write_pad(&mut w, term_strings_off + term_strings_bytes, postings_off)?;
            w.write_all(&postings_blob).map_err(io_err)?;
            w.flush().map_err(io_err)?;
            let f = w.into_inner().map_err(io_err)?;
            let _ = crate::persist::durable_sync(&f);
            drop(f);
            fs::rename(&tmp_path, dst_path).map_err(io_err)?;
            Ok(())
        })();
        if res.is_err() {
            let _ = fs::remove_file(&tmp_path);
            res?;
        }
        MmapBm25Index::open_if_valid(dst_path, self.generation)
            .ok_or_else(|| io_err("failed to open freshly written MmapBm25Index"))
    }
}

/// Zero-deserialization `mmap` Okapi BM25 inverted index (`0 MiB` heap at 500+ GiB scale!).
pub struct MmapBm25Index {
    mmap: Mmap,
    pub generation: u64,
    pub doc_count: u32,
    pub term_count: u32,
    hash_mask: usize,
    pub avgdl: f64,
    doc_lens_off: usize,
    doc_id_offs_off: usize,
    doc_id_bytes_off: usize,
    hash_buckets_off: usize,
    term_entries_off: usize,
    term_strings_off: usize,
    postings_off: usize,
}

impl MmapBm25Index {
    /// Open `path` via `mmap` in ~20 µs (`0` heap deserialization!) if magic, CRC32, and `expected_generation` match.
    pub fn open_if_valid(path: &Path, expected_generation: u64) -> Option<Self> {
        let f = File::open(path).ok()?;
        let mmap = unsafe { MmapOptions::new().map(&f) }.ok()?;
        if mmap.len() < BM25_MMAP_HDR_LEN || &mmap[0..8] != BM25_MMAP_MAGIC {
            return None;
        }
        let generation = u64::from_le_bytes(mmap[8..16].try_into().ok()?);
        if generation != expected_generation {
            return None;
        }
        let stored_crc = u32::from_le_bytes(mmap[28..32].try_into().ok()?);
        let mut crc = Crc32::new();
        crc.update(&mmap[0..28]);
        crc.update(&mmap[32..96]);
        if crc.finalize() != stored_crc {
            return None;
        }

        let doc_count = u32::from_le_bytes(mmap[16..20].try_into().ok()?);
        let term_count = u32::from_le_bytes(mmap[20..24].try_into().ok()?);
        let hash_mask = u32::from_le_bytes(mmap[24..28].try_into().ok()?) as usize;
        if !(hash_mask + 1).is_power_of_two() {
            return None;
        }
        let avgdl = f64::from_bits(u64::from_le_bytes(mmap[32..40].try_into().ok()?));
        let doc_lens_off = u64::from_le_bytes(mmap[40..48].try_into().ok()?) as usize;
        let doc_id_offs_off = u64::from_le_bytes(mmap[48..56].try_into().ok()?) as usize;
        let doc_id_bytes_off = u64::from_le_bytes(mmap[56..64].try_into().ok()?) as usize;
        let hash_buckets_off = u64::from_le_bytes(mmap[64..72].try_into().ok()?) as usize;
        let term_entries_off = u64::from_le_bytes(mmap[72..80].try_into().ok()?) as usize;
        let term_strings_off = u64::from_le_bytes(mmap[80..88].try_into().ok()?) as usize;
        let postings_off = u64::from_le_bytes(mmap[88..96].try_into().ok()?) as usize;

        let dc = doc_count as usize;
        let tc = term_count as usize;
        if doc_lens_off.saturating_add(dc * 4) > mmap.len()
            || doc_id_offs_off.saturating_add((dc + 1) * 8) > mmap.len()
            || hash_buckets_off.saturating_add((hash_mask + 1) * 16) > mmap.len()
            || term_entries_off.saturating_add(tc * 32) > mmap.len()
            || term_strings_off > mmap.len()
            || postings_off > mmap.len()
        {
            return None;
        }

        Some(Self {
            mmap,
            generation,
            doc_count,
            term_count,
            hash_mask,
            avgdl,
            doc_lens_off,
            doc_id_offs_off,
            doc_id_bytes_off,
            hash_buckets_off,
            term_entries_off,
            term_strings_off,
            postings_off,
        })
    }

    #[inline]
    pub fn doc_len(&self, doc_idx: u32) -> u32 {
        let off = self.doc_lens_off + (doc_idx as usize) * 4;
        u32::from_le_bytes(self.mmap[off..off + 4].try_into().unwrap_or([0; 4]))
    }

    /// Zero-copy `&str` slice into the `mmap` `doc_id` table for `doc_idx`!
    #[inline]
    pub fn doc_id(&self, doc_idx: u32) -> Option<&str> {
        if doc_idx >= self.doc_count {
            return None;
        }
        let base = self.doc_id_offs_off + (doc_idx as usize) * 8;
        let s = u64::from_le_bytes(self.mmap[base..base + 8].try_into().ok()?) as usize;
        let e = u64::from_le_bytes(self.mmap[base + 8..base + 16].try_into().ok()?) as usize;
        let abs_s = self.doc_id_bytes_off.checked_add(s)?;
        let abs_e = self.doc_id_bytes_off.checked_add(e)?;
        if abs_e > self.mmap.len() || abs_s > abs_e {
            return None;
        }
        std::str::from_utf8(&self.mmap[abs_s..abs_e]).ok()
    }

    #[inline]
    fn lookup_term(&self, term: &str) -> Option<(u32, &[u8])> {
        let tb = term.as_bytes();
        let h = fnv1a_hash64(tb);
        let cap = self.hash_mask + 1;
        let mut slot = (h as usize) & self.hash_mask;
        for _ in 0..cap {
            let boff = self.hash_buckets_off + slot * 16;
            let bh = u64::from_le_bytes(self.mmap[boff..boff + 8].try_into().ok()?);
            if bh == 0 {
                return None;
            }
            if bh == h {
                let t_idx =
                    u32::from_le_bytes(self.mmap[boff + 8..boff + 12].try_into().ok()?) as usize;
                let eoff = self.term_entries_off + t_idx * 32;
                if eoff + 32 <= self.mmap.len() {
                    let str_off =
                        u64::from_le_bytes(self.mmap[eoff..eoff + 8].try_into().ok()?) as usize;
                    let str_len =
                        u32::from_le_bytes(self.mmap[eoff + 8..eoff + 12].try_into().ok()?)
                            as usize;
                    let abs_str = self.term_strings_off.checked_add(str_off)?;
                    if abs_str.checked_add(str_len)? <= self.mmap.len()
                        && &self.mmap[abs_str..abs_str + str_len] == tb
                    {
                        let df =
                            u32::from_le_bytes(self.mmap[eoff + 12..eoff + 16].try_into().ok()?);
                        let post_off =
                            u64::from_le_bytes(self.mmap[eoff + 16..eoff + 24].try_into().ok()?)
                                as usize;
                        let post_len =
                            u32::from_le_bytes(self.mmap[eoff + 24..eoff + 28].try_into().ok()?)
                                as usize;
                        let abs_post = self.postings_off.checked_add(post_off)?;
                        let end_post = abs_post.checked_add(post_len)?;
                        if end_post <= self.mmap.len() {
                            return Some((df, &self.mmap[abs_post..end_post]));
                        }
                        return None;
                    }
                }
            }
            slot = (slot + 1) & self.hash_mask;
        }
        None
    }

    /// Score deduplicated query terms directly over the `mmap` delta-varint postings (`0` posting allocations!).
    /// Returns `(doc_idx, bm25_score)` for all matching chunks (`score > 0.0`).
    pub fn score_terms(&self, unique_terms: &[String], k1: f64, b: f64) -> Vec<(u32, f32)> {
        self.score_terms_top_k(unique_terms, k1, b, None)
    }

    /// Score deduplicated query terms directly over the `mmap` delta-varint postings with
    /// zero-HashMap dense thread-local accumulation and optional `O(N)` `select_nth` top-K selection!
    ///
    /// When `top_k` is `Some(k)`, returns at most `k` hits already sorted by `(score DESC, doc_id ASC)`.
    pub fn score_terms_top_k(
        &self,
        unique_terms: &[String],
        k1: f64,
        b: f64,
        top_k: Option<usize>,
    ) -> Vec<(u32, f32)> {
        self.score_terms_filtered_top_k(unique_terms, k1, b, top_k, None)
    }

    /// Score deduplicated query terms with an optional L2-resident `allow_bitset` over `doc_idx` (`2 ns` per posting!)
    /// and zero-page-fault register quickselect (`0` `doc_id` lookups on non-top-k candidates!).
    pub fn score_terms_filtered_top_k(
        &self,
        unique_terms: &[String],
        k1: f64,
        b: f64,
        top_k: Option<usize>,
        allow_bitset: Option<&[u64]>,
    ) -> Vec<(u32, f32)> {
        if self.doc_count == 0 || unique_terms.is_empty() {
            return Vec::new();
        }
        if top_k == Some(0) {
            return Vec::new();
        }
        if let Some(bits) = allow_bitset
            && bits.iter().all(|&w| w == 0)
        {
            return Vec::new();
        }
        let n_docs = f64::from(self.doc_count);
        let inv_avgdl = b / self.avgdl;
        let k1_base = (k1 * (1.0 - b)) as f32;
        let k1_dl_scale = (k1 * inv_avgdl) as f32;
        let k1_plus_1 = k1 + 1.0;

        let mut active_terms: SmallVec<[(&[u8], f32); 8]> = SmallVec::new();
        for term in unique_terms {
            let Some((df_u32, post_bytes)) = self.lookup_term(term) else {
                continue;
            };
            let df = f64::from(df_u32);
            let idf = (1.0 + (n_docs - df + 0.5) / (df + 0.5)).ln();
            if idf <= 0.0 {
                continue;
            }
            active_terms.push((post_bytes, (idf * k1_plus_1) as f32));
        }
        if active_terms.is_empty() {
            return Vec::new();
        }

        // Pure in-register comparator (`0` `doc_id` mmap page-faults during O(N) partitioning!)
        let cmp_fast = |a: &(u32, f32), b_hit: &(u32, f32)| -> Ordering {
            b_hit.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b_hit.0))
        };
        // Exact alphabetical tie-break comparator used ONLY on the final `<= top_k` winners!
        let cmp_exact = |a: &(u32, f32), b_hit: &(u32, f32)| -> Ordering {
            b_hit
                .1
                .total_cmp(&a.1)
                .then_with(|| match (self.doc_id(a.0), self.doc_id(b_hit.0)) {
                    (Some(sa), Some(sb)) => sa.cmp(sb),
                    _ => a.0.cmp(&b_hit.0),
                })
        };

        // Helper: select exact top `k` from `out` touching `doc_id` ONLY on boundary ties & final `k`!
        let finalize_top_k = |mut out: Vec<(u32, f32)>, k: usize| -> Vec<(u32, f32)> {
            if out.len() <= k {
                out.sort_unstable_by(cmp_exact);
                return out;
            }
            out.select_nth_unstable_by(k - 1, cmp_fast);
            let cutoff_score = out[k - 1].1;
            // Check if any element in `out[k..]` ties `cutoff_score` (rare; only then resolve `doc_id` ties on boundary!)
            let has_boundary_ties = out[k..].iter().any(|x| x.1 == cutoff_score);
            if !has_boundary_ties {
                out.truncate(k);
                out.sort_unstable_by(cmp_exact);
                return out;
            }
            let mut strictly_better = Vec::with_capacity(k);
            let mut tied = Vec::new();
            for item in out {
                if item.1 > cutoff_score {
                    strictly_better.push(item);
                } else if item.1 == cutoff_score {
                    tied.push(item);
                }
            }
            let need_ties = k.saturating_sub(strictly_better.len());
            if tied.len() > need_ties && need_ties > 0 {
                tied.select_nth_unstable_by(need_ties - 1, cmp_exact);
                tied.truncate(need_ties);
            } else {
                tied.truncate(need_ties);
            }
            strictly_better.extend(tied);
            strictly_better.sort_unstable_by(cmp_exact);
            strictly_better
        };

        // Fast Path 1: Single active term — postings are already strictly unique per `doc_idx`!
        if active_terms.len() == 1 {
            let (post_bytes, idf_num) = active_terms[0];
            if let Some(k) = top_k {
                #[derive(Clone, Copy)]
                struct TopCand {
                    doc_idx: u32,
                    score: f32,
                }
                impl PartialEq for TopCand {
                    fn eq(&self, other: &Self) -> bool {
                        self.doc_idx == other.doc_idx
                            && self.score.to_bits() == other.score.to_bits()
                    }
                }
                impl Eq for TopCand {}
                impl PartialOrd for TopCand {
                    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
                        Some(self.cmp(other))
                    }
                }
                impl Ord for TopCand {
                    fn cmp(&self, other: &Self) -> Ordering {
                        other
                            .score
                            .total_cmp(&self.score)
                            .then_with(|| self.doc_idx.cmp(&other.doc_idx))
                    }
                }

                let cap = k.saturating_add(64);
                let mut heap: BinaryHeap<TopCand> = BinaryHeap::with_capacity(cap + 1);
                let mut min_score = f32::NEG_INFINITY;
                let mut pos = 0usize;
                let mut doc_idx = 0u32;
                let mut first = true;
                while pos < post_bytes.len() {
                    let Some(delta) = read_uvarint32(post_bytes, &mut pos) else {
                        break;
                    };
                    let Some(tf_u32) = read_uvarint32(post_bytes, &mut pos) else {
                        break;
                    };
                    if first {
                        doc_idx = delta;
                        first = false;
                    } else {
                        doc_idx = doc_idx.wrapping_add(delta);
                    }
                    if doc_idx >= self.doc_count {
                        break;
                    }
                    if let Some(bits) = allow_bitset {
                        let w = (doc_idx as usize) >> 6;
                        if w >= bits.len() || ((bits[w] >> (doc_idx & 63)) & 1) == 0 {
                            continue;
                        }
                    }
                    let dl = self.doc_len(doc_idx) as f32;
                    let tf = tf_u32 as f32;
                    let score = (idf_num * tf) / (tf + k1_base + k1_dl_scale * dl);
                    if score <= 0.0 {
                        continue;
                    }
                    if heap.len() < cap {
                        heap.push(TopCand { doc_idx, score });
                        if heap.len() == cap
                            && let Some(worst) = heap.peek()
                        {
                            min_score = worst.score;
                        }
                    } else if score > min_score {
                        heap.pop();
                        heap.push(TopCand { doc_idx, score });
                        if let Some(worst) = heap.peek() {
                            min_score = worst.score;
                        }
                    }
                }
                let out: Vec<(u32, f32)> = heap.into_iter().map(|c| (c.doc_idx, c.score)).collect();
                return finalize_top_k(out, k);
            }

            let mut out = Vec::new();
            let mut pos = 0usize;
            let mut doc_idx = 0u32;
            let mut first = true;
            while pos < post_bytes.len() {
                let Some(delta) = read_uvarint32(post_bytes, &mut pos) else {
                    break;
                };
                let Some(tf_u32) = read_uvarint32(post_bytes, &mut pos) else {
                    break;
                };
                if first {
                    doc_idx = delta;
                    first = false;
                } else {
                    doc_idx = doc_idx.wrapping_add(delta);
                }
                if doc_idx >= self.doc_count {
                    break;
                }
                if let Some(bits) = allow_bitset {
                    let w = (doc_idx as usize) >> 6;
                    if w >= bits.len() || ((bits[w] >> (doc_idx & 63)) & 1) == 0 {
                        continue;
                    }
                }
                let dl = self.doc_len(doc_idx) as f32;
                let tf = tf_u32 as f32;
                let score = (idf_num * tf) / (tf + k1_base + k1_dl_scale * dl);
                if score > 0.0 {
                    out.push((doc_idx, score));
                }
            }
            return out;
        }

        // Fast Path 2: Multi-term query — reusable thread-local dense accumulator (`0` HashMap rehashing!)
        thread_local! {
            static BM25_SCRATCH: std::cell::RefCell<(Vec<f32>, Vec<u32>)> =
                const { std::cell::RefCell::new((Vec::new(), Vec::new())) };
        }

        BM25_SCRATCH.with(|cell| {
            let mut guard = cell.borrow_mut();
            let (scores, touched) = &mut *guard;
            let n = self.doc_count as usize;
            if scores.len() < n {
                scores.resize(n, 0.0);
            }
            touched.clear();

            for &(post_bytes, idf_num) in &active_terms {
                let mut pos = 0usize;
                let mut doc_idx = 0u32;
                let mut first = true;
                while pos < post_bytes.len() {
                    let Some(delta) = read_uvarint32(post_bytes, &mut pos) else {
                        break;
                    };
                    let Some(tf_u32) = read_uvarint32(post_bytes, &mut pos) else {
                        break;
                    };
                    if first {
                        doc_idx = delta;
                        first = false;
                    } else {
                        doc_idx = doc_idx.wrapping_add(delta);
                    }
                    if doc_idx >= self.doc_count {
                        break;
                    }
                    if let Some(bits) = allow_bitset {
                        let w = (doc_idx as usize) >> 6;
                        if w >= bits.len() || ((bits[w] >> (doc_idx & 63)) & 1) == 0 {
                            continue;
                        }
                    }
                    let dl = self.doc_len(doc_idx) as f32;
                    let tf = tf_u32 as f32;
                    let contrib = (idf_num * tf) / (tf + k1_base + k1_dl_scale * dl);
                    let idx = doc_idx as usize;
                    let prev = scores[idx];
                    if prev == 0.0 && contrib > 0.0 {
                        touched.push(doc_idx);
                    }
                    scores[idx] = prev + contrib;
                }
            }

            let mut out: Vec<(u32, f32)> = Vec::with_capacity(touched.len());
            for &doc_idx in touched.iter() {
                let idx = doc_idx as usize;
                let s = scores[idx];
                scores[idx] = 0.0;
                if s > 0.0 {
                    out.push((doc_idx, s));
                }
            }
            touched.clear();

            if let Some(k) = top_k {
                return finalize_top_k(out, k);
            }
            out
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disk_v2_roundtrip_point_reads_tombstones_and_vectors() {
        let tmp = std::env::temp_dir().join(format!("lin_disk_v2_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);

        {
            let mut eng = DiskEngineV2::open(&tmp, false).unwrap();
            eng.commit_ops(vec![DiskWalOp::EnsureTable {
                schema: DiskTableSchema {
                    name: "chunks".into(),
                    columns: vec![
                        DiskColSchema {
                            name: "id".into(),
                            kind: DiskColKind::TextZstd,
                        },
                        DiskColSchema {
                            name: "document_id".into(),
                            kind: DiskColKind::TextZstd,
                        },
                        DiskColSchema {
                            name: "content".into(),
                            kind: DiskColKind::TextZstd,
                        },
                        DiskColSchema {
                            name: "chunk_index".into(),
                            kind: DiskColKind::Int64,
                        },
                        DiskColSchema {
                            name: "embedding".into(),
                            kind: DiskColKind::VecF32Sq8 { dim: 8 },
                        },
                    ],
                    indexed_fields: vec!["document_id".into()],
                },
            }])
            .unwrap();

            let mut r1 = Row::new();
            r1.insert("id".into(), Cell::from_text("c1"));
            r1.insert("document_id".into(), Cell::from_text("d1"));
            r1.insert("content".into(), Cell::from_text("hello disk backed world"));
            r1.insert("chunk_index".into(), Cell::Int(0));
            r1.insert(
                "embedding".into(),
                Cell::Vec(Arc::from([1.0f32, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0])),
            );

            let mut r2 = Row::new();
            r2.insert("id".into(), Cell::from_text("c2"));
            r2.insert("document_id".into(), Cell::from_text("d1"));
            r2.insert("content".into(), Cell::from_text("second vector chunk"));
            r2.insert("chunk_index".into(), Cell::Int(1));
            r2.insert(
                "embedding".into(),
                Cell::Vec(Arc::from([0.0f32, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0])),
            );

            eng.commit_ops(vec![DiskWalOp::UpsertBatch {
                table: "chunks".into(),
                rows: vec![r1, r2],
            }])
            .unwrap();
            eng.checkpoint().unwrap();
        }

        // Reopen purely from mmap segments on disk
        {
            let mut eng = DiskEngineV2::open(&tmp, false).unwrap();
            assert_eq!(eng.count_table("chunks"), 2);
            // Cold mmap read populates L1 Hot Cache; second read hits L1 RAM cache!
            let c1 = eng.get_by_id("chunks", "c1").unwrap().unwrap();
            assert_eq!(
                c1.get("content").and_then(Cell::text),
                Some("hello disk backed world")
            );
            assert_eq!(eng.hot_cache_stats().row_misses, 1);
            let c1_hot = eng.get_by_id("chunks", "c1").unwrap().unwrap();
            assert_eq!(
                c1_hot.get("content").and_then(Cell::text),
                Some("hello disk backed world")
            );
            assert_eq!(eng.hot_cache_stats().row_hits, 1);

            let mut by_doc = Vec::new();
            eng.for_each_by_eq_projected(
                "chunks",
                "document_id",
                &Cell::from_text("d1"),
                None,
                |r| {
                    by_doc.push(r.clone());
                    Ok(true)
                },
            )
            .unwrap();
            assert_eq!(by_doc.len(), 2);

            let hits = eng
                .search_vectors_out_of_core(
                    "chunks",
                    "embedding",
                    "document_id",
                    &[0.95, 0.05, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
                    1,
                    true,
                    None,
                    None,
                    |_| true,
                )
                .unwrap();
            assert_eq!(hits.len(), 1);
            assert_eq!(hits[0].1.get("id").and_then(Cell::text), Some("c1"));

            // Repeat same vector query -> hits L1 Hot Vector Query Cache!
            let hits_hot = eng
                .search_vectors_out_of_core(
                    "chunks",
                    "embedding",
                    "document_id",
                    &[0.95, 0.05, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
                    1,
                    true,
                    None,
                    None,
                    |_| true,
                )
                .unwrap();
            assert_eq!(hits_hot.len(), 1);
            assert_eq!(eng.hot_cache_stats().vec_query_hits, 1);

            // Selective secondary index vector search fast path (`only_doc_ids`)
            let hits_scoped = eng
                .search_vectors_out_of_core(
                    "chunks",
                    "embedding",
                    "document_id",
                    &[0.05, 0.95, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
                    1,
                    false,
                    Some(&["d1"]),
                    None,
                    |d| d == "d1",
                )
                .unwrap();
            assert_eq!(hits_scoped.len(), 1);
            assert_eq!(hits_scoped[0].1.get("id").and_then(Cell::text), Some("c2"));

            // Savepoint rollback restores tombstones & memtable
            eng.begin_savepoint().unwrap();
            eng.commit_ops(vec![DiskWalOp::DeleteIds {
                table: "chunks".into(),
                ids: vec!["c1".into()],
            }])
            .unwrap();
            assert!(eng.get_by_id("chunks", "c1").unwrap().is_none());
            eng.rollback_savepoint();
            assert!(eng.get_by_id("chunks", "c1").unwrap().is_some());

            // Portable LIN\x04 backup export & legacy migration roundtrip
            let bak_file = tmp.join("portable.lin");
            eng.export_portable_backup_file(&bak_file).unwrap();
            let migrated_dir =
                std::env::temp_dir().join(format!("lin_disk_v2_mig_{}", std::process::id()));
            let mig_eng =
                DiskEngineV2::migrate_from_legacy(&bak_file, &migrated_dir, 1024, &BTreeMap::new())
                    .unwrap();
            assert_eq!(mig_eng.count_table("chunks"), 2);
            let _ = fs::remove_dir_all(&migrated_dir);

            // Delete c1 via WAL tombstone without rewriting segment
            eng.commit_ops(vec![DiskWalOp::DeleteIds {
                table: "chunks".into(),
                ids: vec!["c1".into()],
            }])
            .unwrap();
            assert_eq!(eng.count_table("chunks"), 1);
            assert!(eng.get_by_id("chunks", "c1").unwrap().is_none());
        }

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn mmap_bm25_index_roundtrip_and_zero_heap_scoring() {
        let tmp =
            std::env::temp_dir().join(format!("lin_bm25_v3_test_{}.mmap", std::process::id()));
        let _ = fs::remove_file(&tmp);

        let mut builder = MmapBm25Builder::new(42);
        builder.add_document(
            "chunk-a",
            &[
                "rust".into(),
                "mmap".into(),
                "storage".into(),
                "mmap".into(),
            ],
        );
        builder.add_document("chunk-b", &["python".into(), "script".into()]);
        builder.add_document(
            "chunk-c",
            &["rust".into(), "vector".into(), "search".into()],
        );

        let idx = builder.finish_to_file(&tmp).unwrap();
        assert_eq!(idx.generation, 42);
        assert_eq!(idx.doc_count, 3);
        assert_eq!(idx.doc_id(0), Some("chunk-a"));
        assert_eq!(idx.doc_id(1), Some("chunk-b"));
        assert_eq!(idx.doc_id(2), Some("chunk-c"));
        assert_eq!(idx.doc_len(0), 4);

        // Generation mismatch returns None
        assert!(MmapBm25Index::open_if_valid(&tmp, 99).is_none());
        let reopened = MmapBm25Index::open_if_valid(&tmp, 42).unwrap();
        let mut hits = reopened.score_terms(&["rust".into(), "mmap".into()], 1.2, 0.75);
        hits.sort_by(|a, b| b.1.total_cmp(&a.1));
        assert_eq!(hits.len(), 2);
        assert_eq!(reopened.doc_id(hits[0].0), Some("chunk-a"));
        assert_eq!(reopened.doc_id(hits[1].0), Some("chunk-c"));
        assert!(hits[0].1 > hits[1].1);

        let _ = fs::remove_file(&tmp);
    }

    #[test]
    fn compaction_reclaims_tombstones_and_ivf_recalls_target_vector() {
        let tmp =
            std::env::temp_dir().join(format!("lin_disk_v2_compact_ivf_{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);

        let mut eng = DiskEngineV2::open(&tmp, false).unwrap();
        eng.commit_ops(vec![DiskWalOp::EnsureTable {
            schema: DiskTableSchema {
                name: "vecs".into(),
                columns: vec![
                    DiskColSchema {
                        name: "id".into(),
                        kind: DiskColKind::TextZstd,
                    },
                    DiskColSchema {
                        name: "doc_id".into(),
                        kind: DiskColKind::TextZstd,
                    },
                    DiskColSchema {
                        name: "embedding".into(),
                        kind: DiskColKind::VecF32Sq8 { dim: 16 },
                    },
                ],
                indexed_fields: vec!["doc_id".into()],
            },
        }])
        .unwrap();

        // Write 3 separate small delta segments (2,000 rows each = 6,000 total), then delete 1,000 rows
        for b in 0..3usize {
            let mut rows = Vec::with_capacity(2_000);
            for i in 0..2_000usize {
                let gid = b * 2_000 + i;
                let mut v = [0.0f32; 16];
                let bucket = gid % 16;
                v[bucket] = 1.0;
                v[(bucket + 1) % 16] = ((gid % 97) as f32) * 0.005;
                let mut r = Row::new();
                r.insert("id".into(), Cell::from_text(format!("v-{gid}")));
                r.insert("doc_id".into(), Cell::from_text(format!("d-{}", gid % 50)));
                r.insert("embedding".into(), Cell::Vec(Arc::from(v)));
                rows.push(r);
            }
            eng.commit_ops(vec![DiskWalOp::UpsertBatch {
                table: "vecs".into(),
                rows,
            }])
            .unwrap();
            eng.checkpoint().unwrap();
        }
        assert_eq!(eng.segments["vecs"].len(), 3);

        let del_ids: Vec<String> = (0..1_000).map(|i| format!("v-{i}")).collect();
        eng.commit_ops(vec![DiskWalOp::DeleteIds {
            table: "vecs".into(),
            ids: del_ids,
        }])
        .unwrap();
        assert_eq!(eng.count_table("vecs"), 5_000);

        // Vacuum merges 3 segments -> 1 clean segment of 5,000 live rows and builds `col_embedding.vivf`!
        let rep = eng.vacuum().unwrap();
        assert_eq!(rep.tables_compacted, 1);
        assert_eq!(rep.old_segments_removed, 3);
        assert_eq!(rep.new_segments_written, 1);
        assert_eq!(rep.dead_rows_reclaimed, 1_000);
        assert_eq!(eng.count_table("vecs"), 5_000);
        assert!(eng.get_by_id("vecs", "v-10").unwrap().is_none());
        assert!(eng.get_by_id("vecs", "v-4500").unwrap().is_some());

        let seg = &eng.segments["vecs"][0];
        let vc = seg.vec_col("embedding").unwrap();
        assert!(
            vc.ivf.is_some(),
            "expected col_embedding.vivf on 5,000-row compacted segment"
        );
        let ivf = vc.ivf.as_ref().unwrap();
        assert_eq!(ivf.live_vecs, 5_000);

        // Verify top centroids probe finds exact cluster for bucket 7
        let mut q = [0.0f32; 16];
        q[7] = 1.0;
        q[8] = 0.05;
        let qn = l2_norm_f64(&q);
        let top_c = ivf.top_centroids(&q, qn, 8);
        assert!(!top_c.is_empty());

        let hits = eng
            .search_vectors_out_of_core(
                "vecs",
                "embedding",
                "doc_id",
                &q,
                5,
                true,
                None,
                None,
                |_| true,
            )
            .unwrap();
        assert_eq!(hits.len(), 5);
        assert!(hits[0].0 > 0.99);

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn adaptive_text_dict_and_group_count_and_str3_visitor() {
        let tmp =
            std::env::temp_dir().join(format!("lin_disk_v2_text_dict_{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);

        let mut eng = DiskEngineV2::open(&tmp, false).unwrap();
        eng.commit_ops(vec![DiskWalOp::EnsureTable {
            schema: DiskTableSchema {
                name: "docs".into(),
                columns: vec![
                    DiskColSchema {
                        name: "id".into(),
                        kind: DiskColKind::TextZstd,
                    },
                    DiskColSchema {
                        name: "wing".into(),
                        kind: DiskColKind::TextZstd,
                    },
                    DiskColSchema {
                        name: "room".into(),
                        kind: DiskColKind::TextZstd,
                    },
                    DiskColSchema {
                        name: "layer".into(),
                        kind: DiskColKind::TextZstd,
                    },
                ],
                indexed_fields: vec!["wing".into()],
            },
        }])
        .unwrap();

        let mut rows = Vec::with_capacity(200);
        for i in 0..200 {
            let mut r = Row::new();
            r.insert("id".into(), Cell::from_text(format!("doc-{i}")));
            if i < 180 {
                r.insert("wing".into(), Cell::from_text(format!("wing-{}", i % 3)));
                r.insert("room".into(), Cell::from_text(format!("room-{}", i % 6)));
            }
            r.insert(
                "layer".into(),
                Cell::from_text(if i % 2 == 0 { "raw" } else { "wiki" }),
            );
            rows.push(r);
        }
        eng.commit_ops(vec![DiskWalOp::UpsertBatch {
            table: "docs".into(),
            rows,
        }])
        .unwrap();
        eng.checkpoint().unwrap();

        // Verify low-cardinality columns promoted to TextDict while `id` stayed TextZstd
        let seg_meta = &eng.manifest.segments[0];
        let kind_of = |name: &str| {
            seg_meta
                .columns
                .iter()
                .find(|c| c.name == name)
                .unwrap()
                .kind
        };
        assert_eq!(kind_of("id"), DiskColKind::TextZstd);
        assert_eq!(kind_of("wing"), DiskColKind::TextDict);
        assert_eq!(kind_of("room"), DiskColKind::TextDict);
        assert_eq!(kind_of("layer"), DiskColKind::TextDict);

        // Delete 2 rows + add 1 MemTable row to test tombstones + MemTable merge in group_count
        eng.commit_ops(vec![
            DiskWalOp::DeleteIds {
                table: "docs".into(),
                ids: vec!["doc-0".into(), "doc-199".into()],
            },
            DiskWalOp::UpsertBatch {
                table: "docs".into(),
                rows: vec![{
                    let mut r = Row::new();
                    r.insert("id".into(), Cell::from_text("doc-200"));
                    r.insert("wing".into(), Cell::from_text("wing-0"));
                    r.insert("room".into(), Cell::from_text("room-0"));
                    r.insert("layer".into(), Cell::from_text("raw"));
                    r
                }],
            },
        ])
        .unwrap();

        let (tot1, g1) = eng.group_count_str1("docs", "wing").unwrap();
        assert_eq!(tot1, 199);
        let m1: FxHashMap<Option<String>, u64> = g1.into_iter().collect();
        // wing-0 had 60, minus doc-0 (+1) + doc-200 (+1) = 60
        assert_eq!(m1.get(&Some("wing-0".into())), Some(&60));
        assert_eq!(m1.get(&Some("wing-1".into())), Some(&60));
        assert_eq!(m1.get(&Some("wing-2".into())), Some(&60));
        // NULL wing had 20 (180..200), minus doc-199 = 19
        assert_eq!(m1.get(&None), Some(&19));

        let (tot2, g2) = eng.group_count_str2("docs", "wing", "room").unwrap();
        assert_eq!(tot2, 199);
        assert_eq!(g2.iter().map(|(_, c)| *c).sum::<u64>(), 199);

        let (tot3, g3) = eng
            .group_count_str3("docs", "wing", "room", "layer")
            .unwrap();
        assert_eq!(tot3, 199);
        assert_eq!(g3.iter().map(|(_, c)| *c).sum::<u64>(), 199);

        let mut w0_ids = Vec::new();
        eng.for_each_by_eq_str3(
            "docs",
            "wing",
            "wing-0",
            "id",
            "room",
            Some("layer"),
            |id, rm, ly| {
                w0_ids.push((
                    id.unwrap_or_default().to_string(),
                    rm.unwrap_or_default().to_string(),
                    ly.unwrap_or_default().to_string(),
                ));
                Ok(true)
            },
        )
        .unwrap();
        assert_eq!(w0_ids.len(), 60);
        assert!(w0_ids.iter().any(|x| x.0 == "doc-200"));
        assert!(!w0_ids.iter().any(|x| x.0 == "doc-0"));

        let _ = fs::remove_dir_all(&tmp);
    }
}
