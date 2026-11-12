use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

use crate::store::{Cell, Row};

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
    order: VecDeque<u64>,
    hits: u64,
    misses: u64,
}

#[derive(Default)]
struct HotSecQueryShard {
    generation: u64,
    entries: FxHashMap<u64, Arc<[Row]>>,
    order: VecDeque<u64>,
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
    pub(super) fn get_row(&self, table: &str, id: &str) -> Option<Arc<Row>> {
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
    pub(super) fn put_row(&self, table: &str, id: &str, row: Arc<Row>) {
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
    pub(super) fn invalidate_row(&self, table: &str, id: &str) {
        if !self.cfg.enabled {
            return;
        }
        let h = Self::hash_table_id(table, id);
        let idx = ((h >> 32) as usize ^ (h as usize)) & (HOT_ROW_SHARDS - 1);
        if let Ok(mut shard) = self.row_shards[idx].lock() {
            shard.invalidate(h, table, id);
        }
    }

    pub(super) fn clear_all(&self) {
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
    pub(super) fn get_sec_query(&self, generation: u64, qhash: u64) -> Option<Arc<[Row]>> {
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
    pub(super) fn put_sec_query(&self, generation: u64, qhash: u64, res: Arc<[Row]>) {
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

    pub(super) fn get_vec_query(&self, generation: u64, qhash: u64) -> Option<Arc<[(f32, Row)]>> {
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

    pub(super) fn put_vec_query(&self, generation: u64, qhash: u64, res: Arc<[(f32, Row)]>) {
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
