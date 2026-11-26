use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::hash::Hasher;
use std::ops::{Deref, DerefMut};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, RwLock};

use rustc_hash::{FxHashMap, FxHashSet, FxHasher};

use crate::error::Error;
use crate::exec::{Db, Prepared};
use crate::store::Cell;

use super::cluster::{
    ConsistentHashRing, NodeMember, NodeRole, PubSubEvent, RaftCommand,
};
use super::protocol::{CacheEntry, CacheResponse, CacheValue};
use super::{CacheConfig, unix_ms};

pub(crate) const NUM_SHARDS: usize = 64;
const SHARD_MASK: usize = NUM_SHARDS - 1;
pub(crate) const NONE: u32 = u32::MAX;
const PUBSUB_SEEN_MAX: usize = 4096;

/// 128-byte cache-line padding preventing false sharing across both x86_64 and Apple Silicon.
#[repr(align(128))]
pub(crate) struct CachePadded<T>(pub(crate) T);

impl<T> Deref for CachePadded<T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> DerefMut for CachePadded<T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

/// Computes the stripe index `[0, 64)` using `FxHasher` bits independent of `hashbrown`'s
/// low bucket-mask bits (`0..26`) and top `h2` SIMD tag bits (`57..64`).
#[inline]
pub(crate) fn shard_idx(key: &str) -> usize {
    let mut hasher = FxHasher::default();
    hasher.write(key.as_bytes());
    let h = hasher.finish();
    (((h >> 26) ^ (h >> 40)) as usize) & SHARD_MASK
}

pub(crate) struct PubSubHub {
    pub(crate) next_event_id: u64,
    seen_events: FxHashSet<u64>,
    seen_order: VecDeque<u64>,
    pub(crate) recent_outbox: VecDeque<PubSubEvent>,
    /// Channel -> list of subscriber queues.
    pub(crate) subscribers: FxHashMap<String, Vec<Arc<(Mutex<VecDeque<CacheResponse>>, Condvar)>>>,
}

impl Default for PubSubHub {
    fn default() -> Self {
        Self {
            next_event_id: 1,
            seen_events: FxHashSet::default(),
            seen_order: VecDeque::new(),
            recent_outbox: VecDeque::new(),
            subscribers: FxHashMap::default(),
        }
    }
}

impl PubSubHub {
    pub(crate) fn mark_seen(&mut self, eid: u64) -> bool {
        if !self.seen_events.insert(eid) {
            return false;
        }
        self.seen_order.push_back(eid);
        while self.seen_order.len() > PUBSUB_SEEN_MAX {
            if let Some(old) = self.seen_order.pop_front() {
                self.seen_events.remove(&old);
            }
        }
        true
    }

    pub(crate) fn publish_local(
        &mut self,
        channel: &str,
        payload: &[u8],
        origin_node: &str,
    ) -> usize {
        let Some(subs) = self.subscribers.get_mut(channel) else {
            return 0;
        };
        // Prune disconnected subscribers whose client thread dropped its Arc
        subs.retain(|q| Arc::strong_count(q) > 1);
        if subs.is_empty() {
            return 0;
        }
        let msg = CacheResponse::PubSubMessage {
            channel: channel.to_string(),
            payload: payload.to_vec(),
            origin_node: origin_node.to_string(),
        };
        let mut delivered = 0;
        for q in subs.iter() {
            let (lock, cvar) = &**q;
            if let Ok(mut guard) = lock.lock() {
                guard.push_back(msg.clone());
                cvar.notify_one();
                delivered += 1;
            }
        }
        delivered
    }
}

pub(crate) struct ClusterMeta {
    /// Ring for consistent hashing across cluster nodes.
    pub(crate) ring: ConsistentHashRing,
    /// Gossip membership table (`node_id` -> `NodeMember`).
    pub(crate) members: BTreeMap<String, NodeMember>,
    /// Local heartbeat counter.
    pub(crate) heartbeat: u64,
}

impl ClusterMeta {
    pub(crate) fn new(cfg: &CacheConfig) -> Self {
        let mut ring = ConsistentHashRing::new(cfg.vnodes, cfg.replication_factor);
        ring.add_node(&cfg.node_id, &cfg.listen_addr);

        let mut members = BTreeMap::new();
        members.insert(
            cfg.node_id.clone(),
            NodeMember {
                node_id: cfg.node_id.clone(),
                addr: cfg.listen_addr.clone(),
                heartbeat: 1,
                last_seen_ms: unix_ms(),
                alive: true,
                role: NodeRole::Leader,
                term: 1,
            },
        );

        Self {
            ring,
            members,
            heartbeat: 1,
        }
    }

    pub(crate) fn merge_gossip_members(
        &mut self,
        local_id: &str,
        incoming_members: Vec<NodeMember>,
        now: u64,
    ) {
        for inc in incoming_members {
            if inc.node_id == local_id {
                continue;
            }
            self.ring.add_node(&inc.node_id, &inc.addr);
            if let Some(m) = self.members.get_mut(&inc.node_id) {
                if inc.heartbeat >= m.heartbeat {
                    m.heartbeat = inc.heartbeat;
                    if m.addr != inc.addr {
                        m.addr = inc.addr;
                    }
                    m.role = inc.role;
                    m.term = inc.term;
                    m.alive = true;
                    m.last_seen_ms = now;
                }
            } else {
                let mut m = inc;
                m.last_seen_ms = now;
                self.members.insert(m.node_id.clone(), m);
            }
        }
    }

    pub(crate) fn sync_local_member(
        &mut self,
        local_id: &str,
        bump_heartbeat: bool,
        role: NodeRole,
        term: u64,
        now: u64,
    ) {
        if bump_heartbeat {
            self.heartbeat += 1;
        }
        let hb = self.heartbeat;
        if let Some(me) = self.members.get_mut(local_id) {
            me.heartbeat = hb;
            me.last_seen_ms = now;
            me.alive = true;
            me.role = role;
            me.term = term;
        }
    }
}

struct KvPrepared {
    insert: Prepared,
    update: Prepared,
    del_id: Prepared,
    del_k: Prepared,
}

/// Underlying `lin::Db` state kept separate from the 64 keyspace shards.
pub(crate) struct DbState {
    pub(crate) db: Db,
    kv_prep: Option<KvPrepared>,
}

impl DbState {
    pub(crate) fn new(mut db: Db) -> Self {
        // Ensure `cache_kv` collection exists in `lin::Db`.
        // Use `var` instead of `text` so `val_hex` blobs are not tokenized into FTS postings!
        if db.catalog.collection("cache_kv").is_none() {
            let _ = db.execute(
                "col cache_kv { k: var, vtype: var, val_hex: var, exp_ms: i64, ver: i64 }",
            );
        }
        let kv_prep = Self::build_kv_prepared(&mut db);
        Self { db, kv_prep }
    }

    fn build_kv_prepared(db: &mut Db) -> Option<KvPrepared> {
        let insert = db
            .prepare(
                "insert cache_kv [{ id: $id, k: $k, vtype: $vtype, val_hex: $val_hex, exp_ms: $exp_ms, ver: $ver }]",
            )
            .ok()?;
        let update = db
            .prepare(
                "update cache_kv [id == $id] cas \"\" { vtype: $vtype, val_hex: $val_hex, exp_ms: $exp_ms, ver: $ver }",
            )
            .ok()?;
        let del_id = db.prepare("delete cache_kv [id == $id] cas \"\"").ok()?;
        let del_k = db.prepare("delete cache_kv [k == $k] cas \"\"").ok()?;
        Some(KvPrepared {
            insert,
            update,
            del_id,
            del_k,
        })
    }

    pub(crate) fn refresh_kv_prepared_if_needed(&mut self) {
        if self.kv_prep.is_none() && self.db.catalog.collection("cache_kv").is_some() {
            self.kv_prep = Self::build_kv_prepared(&mut self.db);
        }
    }

    pub(crate) fn persist_entry(&mut self, key: &Arc<str>, entry: &CacheEntry) {
        let Ok(bytes) = rmp_serde::to_vec_named(&entry.value) else {
            return;
        };
        let hex_val = hex_encode(&bytes);
        let vt_cell = vtype_cell(&entry.value);
        let exp = entry.expires_at_ms.map(|e| e as i64).unwrap_or(0);
        let ver = entry.version as i64;

        if let Some(prep) = &self.kv_prep {
            let key_cell = Cell::from_arc_str(key);
            let hex_cell = Cell::from_text(&hex_val);
            if self.db.store.point_index("cache_kv", "id", key.as_ref()).is_some() {
                let params = [
                    ("id", key_cell),
                    ("vtype", vt_cell),
                    ("val_hex", hex_cell),
                    ("exp_ms", Cell::Int(exp)),
                    ("ver", Cell::Int(ver)),
                ];
                let _ = prep.update.execute_with(&mut self.db, &params);
            } else {
                // Clean up any legacy row where `id` differed from `k`
                if !self.db.store.collection("cache_kv").is_empty()
                    && self.db.store.by_id.get("cache_kv").map_or(0, |m| m.len())
                        < self.db.store.collection("cache_kv").len()
                {
                    let _ = prep
                        .del_k
                        .execute_with(&mut self.db, &[("k", key_cell.clone())]);
                }
                let params = [
                    ("id", key_cell.clone()),
                    ("k", key_cell),
                    ("vtype", vt_cell),
                    ("val_hex", hex_cell),
                    ("exp_ms", Cell::Int(exp)),
                    ("ver", Cell::Int(ver)),
                ];
                let _ = prep.insert.execute_with(&mut self.db, &params);
            }
            return;
        }

        let vtype = entry.value.type_name();
        let escaped_key = escape_lin_str(key);
        let _ = self.db.execute(&format!(
            "delete cache_kv [k == \"{escaped_key}\"] cas \"\""
        ));
        let _ = self.db.execute(&format!(
            "insert cache_kv [{{ id: \"{escaped_key}\", k: \"{escaped_key}\", vtype: \"{vtype}\", val_hex: \"{hex_val}\", exp_ms: {exp}, ver: {ver} }}]"
        ));
    }

    pub(crate) fn delete_entry(&mut self, key: &Arc<str>) {
        if self.db.store.collection("cache_kv").is_empty() {
            return;
        }
        if let Some(prep) = &self.kv_prep {
            if self.db.store.point_index("cache_kv", "id", key.as_ref()).is_some() {
                let _ = prep
                    .del_id
                    .execute_with(&mut self.db, &[("id", Cell::from_arc_str(key))]);
            } else if self.db.store.by_id.get("cache_kv").map_or(0, |m| m.len())
                < self.db.store.collection("cache_kv").len()
            {
                let _ = prep
                    .del_k
                    .execute_with(&mut self.db, &[("k", Cell::from_arc_str(key))]);
            }
            return;
        }
        let escaped_key = escape_lin_str(key);
        let _ = self.db.execute(&format!(
            "delete cache_kv [k == \"{escaped_key}\"] cas \"\""
        ));
    }

    pub(crate) fn clear_all_entries(&mut self) {
        if !self.db.store.collection("cache_kv").is_empty() {
            let _ = self.db.execute("delete cache_kv [k is not null] cas \"\"");
        }
    }
}

pub(crate) struct OccupiedSlot {
    pub(crate) key: Arc<str>,
    pub(crate) entry: CacheEntry,
    /// Cached `key.len() + entry.value.estimated_bytes()` for O(1) byte accounting.
    pub(crate) entry_bytes: usize,
    /// Latest access sequence updated lock-free on first unreferenced read.
    pub(crate) access_seq: AtomicU64,
    /// Second-chance Clock-LRU bit set under `RwLock::read()` with zero write-lock upgrade!
    pub(crate) referenced: AtomicBool,
}

pub(crate) enum SlotKind {
    Occupied(OccupiedSlot),
    Vacant { next_free: u32 },
}

pub(crate) struct EntrySlot {
    pub(crate) prev: u32,
    pub(crate) next: u32,
    pub(crate) kind: SlotKind,
}

pub(crate) enum ReadLookup<'a> {
    Hit(&'a CacheEntry),
    Miss,
    Expired,
}

/// Single keyspace stripe with an intrusive `O(1)` doubly-linked `Slab` LRU.
pub(crate) struct CacheShard {
    /// Maps `key -> slot_idx` inside `slots`.
    pub(crate) entries: FxHashMap<Arc<str>, u32>,
    /// Contiguous intrusive doubly-linked slab of occupied & recycled slots.
    pub(crate) slots: Vec<EntrySlot>,
    /// Oldest (LRU) slot index (`NONE` when empty).
    pub(crate) lru_head: u32,
    /// Newest (MRU) slot index (`NONE` when empty).
    pub(crate) lru_tail: u32,
    /// Singly-linked free list head of vacant slots (`NONE` when none free).
    pub(crate) free_head: u32,
    /// `(expires_at_ms, key)` ordered ascending for O(log K_ttl) TTL expiration.
    pub(crate) ttl_index: BTreeSet<(u64, Arc<str>)>,
    pub(crate) used_bytes: usize,
    pub(crate) evictions: u64,
    pub(crate) expired_cleaned: u64,
    pub(crate) dirty_lin_keys: FxHashSet<Arc<str>>,
    pub(crate) deleted_lin_keys: FxHashSet<Arc<str>>,
}

impl Default for CacheShard {
    fn default() -> Self {
        Self {
            entries: FxHashMap::default(),
            slots: Vec::new(),
            lru_head: NONE,
            lru_tail: NONE,
            free_head: NONE,
            ttl_index: BTreeSet::new(),
            used_bytes: 0,
            evictions: 0,
            expired_cleaned: 0,
            dirty_lin_keys: FxHashSet::default(),
            deleted_lin_keys: FxHashSet::default(),
        }
    }
}

impl CacheShard {
    #[inline]
    pub(crate) fn occupied(&self, idx: u32) -> &OccupiedSlot {
        match &self.slots[idx as usize].kind {
            SlotKind::Occupied(occ) => occ,
            SlotKind::Vacant { .. } => unreachable!("dangling slab slot index"),
        }
    }

    #[inline]
    pub(crate) fn occupied_mut(&mut self, idx: u32) -> &mut OccupiedSlot {
        match &mut self.slots[idx as usize].kind {
            SlotKind::Occupied(occ) => occ,
            SlotKind::Vacant { .. } => unreachable!("dangling slab slot index"),
        }
    }

    #[inline]
    pub(crate) fn push_tail(&mut self, idx: u32) {
        let old_tail = self.lru_tail;
        let slot = &mut self.slots[idx as usize];
        slot.prev = old_tail;
        slot.next = NONE;
        if old_tail != NONE {
            self.slots[old_tail as usize].next = idx;
        } else {
            self.lru_head = idx;
        }
        self.lru_tail = idx;
    }

    #[inline]
    pub(crate) fn unlink(&mut self, idx: u32) {
        let (prev, next) = {
            let slot = &mut self.slots[idx as usize];
            let p = slot.prev;
            let n = slot.next;
            slot.prev = NONE;
            slot.next = NONE;
            (p, n)
        };
        if prev != NONE {
            self.slots[prev as usize].next = next;
        } else {
            self.lru_head = next;
        }
        if next != NONE {
            self.slots[next as usize].prev = prev;
        } else {
            self.lru_tail = prev;
        }
    }

    #[inline]
    pub(crate) fn promote_to_tail(&mut self, idx: u32) {
        if idx != self.lru_tail {
            self.unlink(idx);
            self.push_tail(idx);
        }
    }

    #[inline]
    pub(crate) fn alloc_slot(&mut self, occ: OccupiedSlot) -> u32 {
        let idx = if self.free_head != NONE {
            let idx = self.free_head;
            let SlotKind::Vacant { next_free } = self.slots[idx as usize].kind else {
                unreachable!("corrupt free list");
            };
            self.free_head = next_free;
            self.slots[idx as usize] = EntrySlot {
                prev: NONE,
                next: NONE,
                kind: SlotKind::Occupied(occ),
            };
            idx
        } else {
            let idx = u32::try_from(self.slots.len()).expect("cache shard slab exceeded u32::MAX");
            self.slots.push(EntrySlot {
                prev: NONE,
                next: NONE,
                kind: SlotKind::Occupied(occ),
            });
            idx
        };
        self.push_tail(idx);
        idx
    }

    #[inline]
    pub(crate) fn free_slot(&mut self, idx: u32) -> OccupiedSlot {
        self.unlink(idx);
        let old = std::mem::replace(
            &mut self.slots[idx as usize].kind,
            SlotKind::Vacant {
                next_free: self.free_head,
            },
        );
        self.free_head = idx;
        let SlotKind::Occupied(occ) = old else {
            unreachable!("double free of vacant slab slot");
        };
        occ
    }
}

pub(crate) struct ShardStripe {
    pub(crate) lock: RwLock<CacheShard>,
    pub(crate) len: AtomicUsize,
    pub(crate) ttl_len: AtomicUsize,
    pub(crate) hits: AtomicU64,
    pub(crate) misses: AtomicU64,
}

impl Default for ShardStripe {
    fn default() -> Self {
        Self {
            lock: RwLock::new(CacheShard::default()),
            len: AtomicUsize::new(0),
            ttl_len: AtomicUsize::new(0),
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
        }
    }
}

/// 64-shard keyspace store with global atomic byte/entry budgets.
pub(crate) struct CacheStore {
    pub(crate) shards: [CachePadded<ShardStripe>; NUM_SHARDS],
    pub(crate) total_entries: AtomicUsize,
    pub(crate) used_bytes: AtomicUsize,
    pub(crate) access_seq: AtomicU64,
    pub(crate) raft_log_idx: AtomicU64,
    pub(crate) lazy_lin_sync: bool,
    pub(crate) has_pending_lin_sync: AtomicBool,
    pub(crate) flush_all_lin_pending: AtomicBool,
}

#[inline]
fn vtype_cell(val: &CacheValue) -> Cell {
    // Inline SmallText SSO cells (<= 22 bytes): bitwise Copy/Clone with zero heap allocs!
    Cell::from_text(val.type_name())
}

impl CacheStore {
    pub(crate) fn new(db_state: &mut DbState) -> Result<Self, Error> {
        let lazy_lin_sync = !db_state.db.is_durable();
        let store = Self {
            shards: std::array::from_fn(|_| CachePadded(ShardStripe::default())),
            total_entries: AtomicUsize::new(0),
            used_bytes: AtomicUsize::new(0),
            access_seq: AtomicU64::new(1),
            raft_log_idx: AtomicU64::new(0),
            lazy_lin_sync,
            has_pending_lin_sync: AtomicBool::new(false),
            flush_all_lin_pending: AtomicBool::new(false),
        };
        store.hydrate_from_lin_db(db_state);
        Ok(store)
    }

    #[inline]
    pub(crate) fn next_seq(&self) -> u64 {
        self.access_seq.fetch_add(1, Ordering::Relaxed) + 1
    }

    #[inline]
    pub(crate) fn next_version(&self) -> u64 {
        self.raft_log_idx.fetch_add(1, Ordering::Relaxed) + 1
    }

    #[inline]
    pub(crate) fn stripe(&self, idx: usize) -> &ShardStripe {
        &self.shards[idx].0
    }

    #[inline]
    fn add_global_bytes(&self, delta: usize) {
        if delta != 0 {
            self.used_bytes.fetch_add(delta, Ordering::AcqRel);
        }
    }

    #[inline]
    fn sub_global_bytes(&self, delta: usize) {
        if delta != 0 {
            self.used_bytes.fetch_sub(delta, Ordering::AcqRel);
        }
    }

    /// Lock-shared read lookup under `RwLock::read()`:
    /// - Zero write-lock upgrades on live hits/misses.
    /// - Sets `slot.referenced = true` (and stamps `access_seq` only on the first read after
    ///   write/sweep so repeated reads of hot keys never dirty the slot cache line!).
    #[inline]
    pub(crate) fn read_live<'a>(&self, sh: &'a CacheShard, key: &str) -> ReadLookup<'a> {
        let Some(&slot_idx) = sh.entries.get(key) else {
            return ReadLookup::Miss;
        };
        let occ = sh.occupied(slot_idx);
        if occ.entry.expires_at_ms.is_some() && occ.entry.is_expired(unix_ms()) {
            return ReadLookup::Expired;
        }
        if !occ.referenced.load(Ordering::Relaxed) {
            let seq = self.next_seq();
            occ.access_seq.store(seq, Ordering::Relaxed);
            occ.referenced.store(true, Ordering::Relaxed);
        }
        ReadLookup::Hit(&occ.entry)
    }

    #[inline]
    pub(crate) fn record_lin_put(
        &self,
        sh: &mut CacheShard,
        db_opt: &mut Option<&mut DbState>,
        key: &Arc<str>,
        entry: &CacheEntry,
    ) {
        if self.lazy_lin_sync {
            sh.deleted_lin_keys.remove(key);
            sh.dirty_lin_keys.insert(Arc::clone(key));
            if !self.has_pending_lin_sync.load(Ordering::Relaxed) {
                self.has_pending_lin_sync.store(true, Ordering::Release);
            }
        } else if let Some(db_st) = db_opt.as_deref_mut() {
            db_st.persist_entry(key, entry);
        }
    }

    #[inline]
    pub(crate) fn record_lin_del(
        &self,
        sh: &mut CacheShard,
        db_opt: &mut Option<&mut DbState>,
        key: Arc<str>,
    ) {
        if self.lazy_lin_sync {
            sh.dirty_lin_keys.remove(&key);
            if !self.flush_all_lin_pending.load(Ordering::Relaxed) {
                sh.deleted_lin_keys.insert(key);
                if !self.has_pending_lin_sync.load(Ordering::Relaxed) {
                    self.has_pending_lin_sync.store(true, Ordering::Release);
                }
            }
        } else if let Some(db_st) = db_opt.as_deref_mut() {
            db_st.delete_entry(&key);
        }
    }

    fn hydrate_from_lin_db(&self, db_state: &mut DbState) {
        let now = unix_ms();
        let rows = db_state.db.store.collection("cache_kv");
        for row in rows {
            let Some(k) = row.get("k").and_then(|c| c.text()) else {
                continue;
            };
            let Some(hex_str) = row.get("val_hex").and_then(|c| c.text()) else {
                continue;
            };
            let exp_ms = row
                .get("exp_ms")
                .and_then(|c| c.as_i64())
                .filter(|&v| v > 0)
                .map(|v| v as u64);
            if exp_ms.is_some_and(|exp| now >= exp) {
                continue;
            }
            let ver = row.get("ver").and_then(|c| c.as_i64()).unwrap_or(1) as u64;
            if let Some(bytes) = hex_decode(hex_str)
                && let Ok(value) = rmp_serde::from_slice::<CacheValue>(&bytes)
            {
                let s_idx = shard_idx(k);
                let stripe = self.stripe(s_idx);
                let mut sh = stripe.lock.write().unwrap();
                let seq = self.next_seq();
                let entry_bytes = k.len() + value.estimated_bytes();
                let entry = CacheEntry {
                    value,
                    expires_at_ms: exp_ms,
                    last_access_seq: seq,
                    version: ver,
                };
                if let Some(&existing_slot) = sh.entries.get(k) {
                    let key_arc = Arc::clone(&sh.occupied(existing_slot).key);
                    let old_exp = sh.occupied(existing_slot).entry.expires_at_ms;
                    let old_bytes = sh.occupied(existing_slot).entry_bytes;
                    if old_exp != exp_ms {
                        if let Some(pe) = old_exp {
                            sh.ttl_index.remove(&(pe, Arc::clone(&key_arc)));
                        }
                        if let Some(ne) = exp_ms {
                            sh.ttl_index.insert((ne, Arc::clone(&key_arc)));
                        }
                        stripe.ttl_len.store(sh.ttl_index.len(), Ordering::Relaxed);
                    }
                    let occ = sh.occupied_mut(existing_slot);
                    occ.entry = entry;
                    occ.entry_bytes = entry_bytes;
                    occ.access_seq.store(seq, Ordering::Relaxed);
                    occ.referenced.store(false, Ordering::Relaxed);
                    sh.promote_to_tail(existing_slot);
                    if entry_bytes >= old_bytes {
                        let diff = entry_bytes - old_bytes;
                        sh.used_bytes += diff;
                        self.add_global_bytes(diff);
                    } else {
                        let diff = old_bytes - entry_bytes;
                        sh.used_bytes = sh.used_bytes.saturating_sub(diff);
                        self.sub_global_bytes(diff);
                    }
                } else {
                    let key_arc: Arc<str> = Arc::from(k);
                    if let Some(ne) = exp_ms {
                        sh.ttl_index.insert((ne, Arc::clone(&key_arc)));
                        stripe.ttl_len.store(sh.ttl_index.len(), Ordering::Relaxed);
                    }
                    let slot_idx = sh.alloc_slot(OccupiedSlot {
                        key: Arc::clone(&key_arc),
                        entry,
                        entry_bytes,
                        access_seq: AtomicU64::new(seq),
                        referenced: AtomicBool::new(false),
                    });
                    sh.entries.insert(key_arc, slot_idx);
                    sh.used_bytes += entry_bytes;
                    stripe.len.store(sh.entries.len(), Ordering::Relaxed);
                    self.total_entries.fetch_add(1, Ordering::Relaxed);
                    self.add_global_bytes(entry_bytes);
                }
            }
        }
    }

    pub(crate) fn flush_dirty_to_lin(&self, db_state: &mut DbState) {
        if !self.has_pending_lin_sync.load(Ordering::Acquire) {
            return;
        }
        // Clear pending flag before draining shards so concurrent writes after a shard is
        // visited re-arm `has_pending_lin_sync = true`.
        self.has_pending_lin_sync.store(false, Ordering::Release);

        if self.flush_all_lin_pending.swap(false, Ordering::AcqRel) {
            db_state.clear_all_entries();
        }

        for i in 0..NUM_SHARDS {
            let mut sh = self.stripe(i).lock.write().unwrap();
            if !sh.deleted_lin_keys.is_empty() {
                let deleted: Vec<Arc<str>> = sh.deleted_lin_keys.drain().collect();
                for k in &deleted {
                    db_state.delete_entry(k);
                }
            }
            if !sh.dirty_lin_keys.is_empty() {
                let dirty: Vec<Arc<str>> = sh.dirty_lin_keys.drain().collect();
                for k in &dirty {
                    if let Some(&slot_idx) = sh.entries.get(k.as_ref()) {
                        let entry = &sh.occupied(slot_idx).entry;
                        db_state.persist_entry(k, entry);
                    }
                }
            }
        }
    }

    /// Purge expired TTL entries in `sh` in `O(log K_ttl)` (O(1) peek without `unix_ms()` when empty).
    #[inline]
    pub(crate) fn purge_expired_in_shard(
        &self,
        stripe: &ShardStripe,
        sh: &mut CacheShard,
        db_opt: &mut Option<&mut DbState>,
    ) {
        if sh.ttl_index.is_empty() {
            return;
        }
        let now = unix_ms();
        let mut removed_any = false;
        while let Some((exp, _)) = sh.ttl_index.first() {
            if *exp > now {
                break;
            }
            let (_, k) = sh.ttl_index.pop_first().unwrap();
            removed_any = true;
            if let Some(slot_idx) = sh.entries.remove(&k) {
                let occ = sh.free_slot(slot_idx);
                sh.used_bytes = sh.used_bytes.saturating_sub(occ.entry_bytes);
                self.sub_global_bytes(occ.entry_bytes);
                self.total_entries.fetch_sub(1, Ordering::AcqRel);
                sh.expired_cleaned += 1;
                self.record_lin_del(sh, db_opt, occ.key);
            }
        }
        if removed_any {
            stripe.len.store(sh.entries.len(), Ordering::Relaxed);
            stripe.ttl_len.store(sh.ttl_index.len(), Ordering::Relaxed);
        }
    }

    /// Check whether `key` exists and is live inside `sh`, purging it if expired,
    /// **without** mutating LRU links on a hit.
    #[inline]
    pub(crate) fn purge_if_expired_in_shard(
        &self,
        stripe: &ShardStripe,
        sh: &mut CacheShard,
        db_opt: &mut Option<&mut DbState>,
        key: &str,
    ) -> Option<u32> {
        let &slot_idx = sh.entries.get(key)?;
        let is_exp = {
            let occ = sh.occupied(slot_idx);
            occ.entry.expires_at_ms.is_some() && occ.entry.is_expired(unix_ms())
        };
        if is_exp {
            sh.entries.remove(key);
            let occ = sh.free_slot(slot_idx);
            if let Some(exp) = occ.entry.expires_at_ms {
                sh.ttl_index.remove(&(exp, Arc::clone(&occ.key)));
                stripe.ttl_len.store(sh.ttl_index.len(), Ordering::Relaxed);
            }
            sh.used_bytes = sh.used_bytes.saturating_sub(occ.entry_bytes);
            stripe.len.store(sh.entries.len(), Ordering::Relaxed);
            self.sub_global_bytes(occ.entry_bytes);
            self.total_entries.fetch_sub(1, Ordering::AcqRel);
            sh.expired_cleaned += 1;
            self.record_lin_del(sh, db_opt, occ.key);
            return None;
        }
        Some(slot_idx)
    }

    pub(crate) fn touch_or_remove_expired_in_shard(
        &self,
        stripe: &ShardStripe,
        sh: &mut CacheShard,
        db_opt: &mut Option<&mut DbState>,
        key: &str,
    ) -> Option<u32> {
        let slot_idx = self.purge_if_expired_in_shard(stripe, sh, db_opt, key)?;
        let seq = self.next_seq();
        {
            let occ = sh.occupied_mut(slot_idx);
            occ.entry.last_access_seq = seq;
            occ.access_seq.store(seq, Ordering::Relaxed);
            occ.referenced.store(false, Ordering::Relaxed);
        }
        sh.promote_to_tail(slot_idx);
        Some(slot_idx)
    }

    /// Evict one unreferenced victim from `sh.lru_head` using Clock-Pro / Second-Chance Slab-LRU:
    /// slots with `referenced == true` have `referenced` cleared and move to `lru_tail` in `O(1)`.
    #[inline]
    fn evict_one_from_shard(
        &self,
        stripe: &ShardStripe,
        sh: &mut CacheShard,
        db_opt: &mut Option<&mut DbState>,
    ) -> bool {
        loop {
            let head_idx = sh.lru_head;
            if head_idx == NONE {
                return false;
            }
            let was_referenced = sh
                .occupied(head_idx)
                .referenced
                .swap(false, Ordering::Relaxed);
            if was_referenced {
                let live = sh.occupied(head_idx).access_seq.load(Ordering::Relaxed);
                sh.occupied_mut(head_idx).entry.last_access_seq = live;
                sh.promote_to_tail(head_idx);
                continue;
            }
            let victim_key = Arc::clone(&sh.occupied(head_idx).key);
            sh.entries.remove(&victim_key);
            let occ = sh.free_slot(head_idx);
            if let Some(exp) = occ.entry.expires_at_ms {
                sh.ttl_index.remove(&(exp, Arc::clone(&occ.key)));
                stripe.ttl_len.store(sh.ttl_index.len(), Ordering::Relaxed);
            }
            sh.used_bytes = sh.used_bytes.saturating_sub(occ.entry_bytes);
            stripe.len.store(sh.entries.len(), Ordering::Relaxed);
            self.sub_global_bytes(occ.entry_bytes);
            self.total_entries.fetch_sub(1, Ordering::AcqRel);
            sh.evictions += 1;
            self.record_lin_del(sh, db_opt, occ.key);
            return true;
        }
    }

    #[inline]
    pub(crate) fn is_over_budget(&self, max_entries: usize, max_bytes: usize) -> bool {
        self.total_entries.load(Ordering::Relaxed) > max_entries
            || self.used_bytes.load(Ordering::Relaxed) > max_bytes
    }

    /// Fast in-shard eviction while `sh` holds more than its fair `1/64` share of `max_entries`.
    /// Returns `true` if global overflow still remains and cross-shard eviction is needed.
    #[inline]
    pub(crate) fn evict_local_if_needed(
        &self,
        stripe: &ShardStripe,
        sh: &mut CacheShard,
        db_opt: &mut Option<&mut DbState>,
        max_entries: usize,
        max_bytes: usize,
    ) -> bool {
        self.purge_expired_in_shard(stripe, sh, db_opt);
        if !self.is_over_budget(max_entries, max_bytes) {
            return false;
        }
        let fair_entries = (max_entries / NUM_SHARDS).max(1);
        while self.is_over_budget(max_entries, max_bytes) && sh.entries.len() > fair_entries {
            if !self.evict_one_from_shard(stripe, sh, db_opt) {
                break;
            }
        }
        self.is_over_budget(max_entries, max_bytes)
    }

    /// Cross-shard eviction invoked only when global budget is still exceeded after local shard
    /// checks (e.g., small `max_entries < 64` or skewed byte distribution across shards).
    pub(crate) fn evict_global_overflow(
        &self,
        db_opt: &mut Option<&mut DbState>,
        max_entries: usize,
        max_bytes: usize,
    ) {
        if !self.is_over_budget(max_entries, max_bytes) {
            return;
        }
        // 1. Purge expired TTL entries across shards that actually hold TTL keys
        for i in 0..NUM_SHARDS {
            let stripe = self.stripe(i);
            if stripe.ttl_len.load(Ordering::Relaxed) > 0 {
                let mut sh = stripe.lock.write().unwrap();
                self.purge_expired_in_shard(stripe, &mut sh, db_opt);
            }
        }

        // 2. Evict global oldest unreferenced `lru_head` across non-empty shards
        while self.is_over_budget(max_entries, max_bytes) {
            let mut best_candidate: Option<(u64, usize)> = None;
            for i in 0..NUM_SHARDS {
                let stripe = self.stripe(i);
                if stripe.len.load(Ordering::Relaxed) == 0 {
                    continue;
                }
                let mut sh = stripe.lock.write().unwrap();
                let max_promotions = sh.entries.len();
                for _ in 0..max_promotions {
                    let head_idx = sh.lru_head;
                    if head_idx == NONE {
                        break;
                    }
                    if sh
                        .occupied(head_idx)
                        .referenced
                        .swap(false, Ordering::Relaxed)
                    {
                        let live = sh.occupied(head_idx).access_seq.load(Ordering::Relaxed);
                        sh.occupied_mut(head_idx).entry.last_access_seq = live;
                        sh.promote_to_tail(head_idx);
                    } else {
                        break;
                    }
                }
                if sh.lru_head != NONE {
                    let seq = sh.occupied(sh.lru_head).entry.last_access_seq;
                    if best_candidate.is_none_or(|(best_seq, _)| seq < best_seq) {
                        best_candidate = Some((seq, i));
                    }
                }
            }

            let Some((_, victim_shard)) = best_candidate else {
                break;
            };
            let stripe = self.stripe(victim_shard);
            let mut sh = stripe.lock.write().unwrap();
            if sh.lru_head != NONE {
                let _ = self.evict_one_from_shard(stripe, &mut sh, db_opt);
            } else if self.total_entries.load(Ordering::Acquire) == 0 {
                break;
            }
        }
    }

    /// Insert or overwrite `key` inside `sh` in `O(1)` without rehashing or cloning `Arc<str>` on overwrites!
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn put_in_shard(
        &self,
        stripe: &ShardStripe,
        sh: &mut CacheShard,
        db_opt: &mut Option<&mut DbState>,
        key: &str,
        value: CacheValue,
        expires_at_ms: Option<u64>,
        version: u64,
        max_entries: usize,
        max_bytes: usize,
    ) -> bool {
        let seq = self.next_seq();
        let new_bytes = key.len() + value.estimated_bytes();

        if let Some(&slot_idx) = sh.entries.get(key) {
            let key_arc = Arc::clone(&sh.occupied(slot_idx).key);
            let old_exp = sh.occupied(slot_idx).entry.expires_at_ms;
            let old_bytes = sh.occupied(slot_idx).entry_bytes;

            if old_exp != expires_at_ms {
                if let Some(pe) = old_exp {
                    sh.ttl_index.remove(&(pe, Arc::clone(&key_arc)));
                }
                if let Some(ne) = expires_at_ms {
                    sh.ttl_index.insert((ne, Arc::clone(&key_arc)));
                }
                stripe.ttl_len.store(sh.ttl_index.len(), Ordering::Relaxed);
            }

            let entry = CacheEntry {
                value,
                expires_at_ms,
                last_access_seq: seq,
                version,
            };
            self.record_lin_put(sh, db_opt, &key_arc, &entry);

            {
                let occ = sh.occupied_mut(slot_idx);
                occ.entry = entry;
                occ.entry_bytes = new_bytes;
                occ.access_seq.store(seq, Ordering::Relaxed);
                occ.referenced.store(false, Ordering::Relaxed);
            }
            sh.promote_to_tail(slot_idx);

            if new_bytes >= old_bytes {
                let diff = new_bytes - old_bytes;
                sh.used_bytes += diff;
                self.add_global_bytes(diff);
            } else {
                let diff = old_bytes - new_bytes;
                sh.used_bytes = sh.used_bytes.saturating_sub(diff);
                self.sub_global_bytes(diff);
            }
        } else {
            let key_arc: Arc<str> = Arc::from(key);
            let entry = CacheEntry {
                value,
                expires_at_ms,
                last_access_seq: seq,
                version,
            };
            self.record_lin_put(sh, db_opt, &key_arc, &entry);

            if let Some(ne) = expires_at_ms {
                sh.ttl_index.insert((ne, Arc::clone(&key_arc)));
                stripe.ttl_len.store(sh.ttl_index.len(), Ordering::Relaxed);
            }
            let slot_idx = sh.alloc_slot(OccupiedSlot {
                key: Arc::clone(&key_arc),
                entry,
                entry_bytes: new_bytes,
                access_seq: AtomicU64::new(seq),
                referenced: AtomicBool::new(false),
            });
            sh.entries.insert(key_arc, slot_idx);
            sh.used_bytes += new_bytes;
            stripe.len.store(sh.entries.len(), Ordering::Relaxed);
            self.total_entries.fetch_add(1, Ordering::AcqRel);
            self.add_global_bytes(new_bytes);
        }

        self.evict_local_if_needed(stripe, sh, db_opt, max_entries, max_bytes)
    }

    /// Update metadata + byte delta of an existing `slot_idx` modified in-place inside `sh`.
    #[allow(clippy::too_many_arguments)]
    #[inline]
    pub(crate) fn finish_inplace_mutation(
        &self,
        stripe: &ShardStripe,
        sh: &mut CacheShard,
        db_opt: &mut Option<&mut DbState>,
        slot_idx: u32,
        added_bytes: usize,
        removed_bytes: usize,
        version: u64,
        max_entries: usize,
        max_bytes: usize,
    ) -> bool {
        let seq = self.next_seq();
        let key_arc = Arc::clone(&sh.occupied(slot_idx).key);
        {
            let occ = sh.occupied_mut(slot_idx);
            occ.entry.last_access_seq = seq;
            occ.entry.version = version;
            occ.entry_bytes = occ.entry_bytes + added_bytes - removed_bytes;
            debug_assert_eq!(
                occ.entry_bytes,
                occ.key.len() + occ.entry.value.estimated_bytes()
            );
            occ.access_seq.store(seq, Ordering::Relaxed);
            occ.referenced.store(false, Ordering::Relaxed);
        }
        if added_bytes >= removed_bytes {
            let diff = added_bytes - removed_bytes;
            sh.used_bytes += diff;
            self.add_global_bytes(diff);
        } else {
            let diff = removed_bytes - added_bytes;
            sh.used_bytes = sh.used_bytes.saturating_sub(diff);
            self.sub_global_bytes(diff);
        }
        sh.promote_to_tail(slot_idx);
        let entry_ref_clone = {
            if self.lazy_lin_sync {
                sh.deleted_lin_keys.remove(&key_arc);
                sh.dirty_lin_keys.insert(key_arc);
                if !self.has_pending_lin_sync.load(Ordering::Relaxed) {
                    self.has_pending_lin_sync.store(true, Ordering::Release);
                }
                false
            } else {
                true
            }
        };
        if entry_ref_clone && let Some(db_st) = db_opt.as_deref_mut() {
            let occ = sh.occupied(slot_idx);
            db_st.persist_entry(&occ.key, &occ.entry);
        }
        self.evict_local_if_needed(stripe, sh, db_opt, max_entries, max_bytes)
    }

    #[inline]
    pub(crate) fn remove_slot_in_shard(
        &self,
        stripe: &ShardStripe,
        sh: &mut CacheShard,
        db_opt: &mut Option<&mut DbState>,
        slot_idx: u32,
    ) {
        let key_arc = Arc::clone(&sh.occupied(slot_idx).key);
        sh.entries.remove(&key_arc);
        let occ = sh.free_slot(slot_idx);
        if let Some(exp) = occ.entry.expires_at_ms {
            sh.ttl_index.remove(&(exp, Arc::clone(&occ.key)));
            stripe.ttl_len.store(sh.ttl_index.len(), Ordering::Relaxed);
        }
        sh.used_bytes = sh.used_bytes.saturating_sub(occ.entry_bytes);
        stripe.len.store(sh.entries.len(), Ordering::Relaxed);
        self.sub_global_bytes(occ.entry_bytes);
        self.total_entries.fetch_sub(1, Ordering::AcqRel);
        self.record_lin_del(sh, db_opt, occ.key);
    }

    pub(crate) fn remove_key_unconditional(
        &self,
        db_opt: &mut Option<&mut DbState>,
        key: &str,
    ) -> bool {
        let s_idx = shard_idx(key);
        let stripe = self.stripe(s_idx);
        let mut sh = stripe.lock.write().unwrap();
        if let Some(&slot_idx) = sh.entries.get(key) {
            self.remove_slot_in_shard(stripe, &mut sh, db_opt, slot_idx);
            true
        } else {
            false
        }
    }

    pub(crate) fn remove_live_key(&self, db_opt: &mut Option<&mut DbState>, key: &str) -> bool {
        let s_idx = shard_idx(key);
        let stripe = self.stripe(s_idx);
        let mut sh = stripe.lock.write().unwrap();
        if let Some(slot_idx) = self.purge_if_expired_in_shard(stripe, &mut sh, db_opt, key) {
            self.remove_slot_in_shard(stripe, &mut sh, db_opt, slot_idx);
            true
        } else {
            false
        }
    }

    pub(crate) fn clear_all(&self, db_opt: &mut Option<&mut DbState>) {
        if self.lazy_lin_sync {
            self.flush_all_lin_pending.store(true, Ordering::Release);
            self.has_pending_lin_sync.store(true, Ordering::Release);
        } else if let Some(db_st) = db_opt.as_deref_mut() {
            db_st.clear_all_entries();
        }
        for i in 0..NUM_SHARDS {
            let stripe = self.stripe(i);
            let mut sh = stripe.lock.write().unwrap();
            let shard_len = sh.entries.len();
            let shard_bytes = sh.used_bytes;
            sh.entries.clear();
            sh.slots.clear();
            sh.lru_head = NONE;
            sh.lru_tail = NONE;
            sh.free_head = NONE;
            sh.ttl_index.clear();
            sh.used_bytes = 0;
            sh.dirty_lin_keys.clear();
            sh.deleted_lin_keys.clear();
            stripe.len.store(0, Ordering::Relaxed);
            stripe.ttl_len.store(0, Ordering::Relaxed);
            if shard_len > 0 {
                self.total_entries.fetch_sub(shard_len, Ordering::AcqRel);
            }
            self.sub_global_bytes(shard_bytes);
        }
    }

    pub(crate) fn apply_raft_command(
        &self,
        db_state: &RwLock<DbState>,
        cmd: &RaftCommand,
        version: u64,
        max_entries: usize,
        max_bytes: usize,
    ) {
        match cmd {
            RaftCommand::Set {
                key,
                value,
                expires_at_ms,
            } => {
                let mut db_guard = if self.lazy_lin_sync {
                    None
                } else {
                    Some(db_state.write().unwrap())
                };
                let mut db_opt = db_guard.as_deref_mut();
                let s_idx = shard_idx(key);
                let stripe = self.stripe(s_idx);
                let overflow = {
                    let mut sh = stripe.lock.write().unwrap();
                    self.put_in_shard(
                        stripe,
                        &mut sh,
                        &mut db_opt,
                        key,
                        value.clone(),
                        *expires_at_ms,
                        version,
                        max_entries,
                        max_bytes,
                    )
                };
                if overflow {
                    self.evict_global_overflow(&mut db_opt, max_entries, max_bytes);
                }
            }
            RaftCommand::Del { keys } => {
                let mut db_guard = if self.lazy_lin_sync {
                    None
                } else {
                    Some(db_state.write().unwrap())
                };
                let mut db_opt = db_guard.as_deref_mut();
                for k in keys {
                    let _ = self.remove_key_unconditional(&mut db_opt, k);
                }
            }
            RaftCommand::FlushAll => {
                let mut db_guard = if self.lazy_lin_sync {
                    None
                } else {
                    Some(db_state.write().unwrap())
                };
                let mut db_opt = db_guard.as_deref_mut();
                self.clear_all(&mut db_opt);
            }
            RaftCommand::LinMutation { query } => {
                let mut db_st = db_state.write().unwrap();
                self.flush_dirty_to_lin(&mut db_st);
                let _ = db_st.db.execute(query);
                db_st.refresh_kv_prepared_if_needed();
            }
        }
    }
}

const HEX_ENCODE_LUT: [[u8; 2]; 256] = {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut lut = [[0u8; 2]; 256];
    let mut i = 0;
    while i < 256 {
        lut[i] = [DIGITS[i >> 4], DIGITS[i & 0x0f]];
        i += 1;
    }
    lut
};

const HEX_DECODE_LUT: [u8; 256] = {
    let mut lut = [0xffu8; 256];
    let mut c = b'0';
    while c <= b'9' {
        lut[c as usize] = c - b'0';
        c += 1;
    }
    let mut c = b'a';
    while c <= b'f' {
        lut[c as usize] = c - b'a' + 10;
        c += 1;
    }
    let mut c = b'A';
    while c <= b'F' {
        lut[c as usize] = c - b'A' + 10;
        c += 1;
    }
    lut
};

fn hex_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    let mut buf = [0u8; 512];
    for chunk in bytes.chunks(256) {
        for (i, &b) in chunk.iter().enumerate() {
            let pair = HEX_ENCODE_LUT[b as usize];
            buf[i * 2] = pair[0];
            buf[i * 2 + 1] = pair[1];
        }
        let s = std::str::from_utf8(&buf[..chunk.len() * 2]).unwrap();
        out.push_str(s);
    }
    out
}

fn hex_decode(s: &str) -> Option<Vec<u8>> {
    let bytes = s.as_bytes();
    if !bytes.len().is_multiple_of(2) {
        return None;
    }
    let mut out = Vec::with_capacity(bytes.len() / 2);
    for pair in bytes.chunks_exact(2) {
        let hi = HEX_DECODE_LUT[pair[0] as usize];
        let lo = HEX_DECODE_LUT[pair[1] as usize];
        if (hi | lo) == 0xff {
            return None;
        }
        out.push((hi << 4) | lo);
    }
    Some(out)
}

fn escape_lin_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            other => out.push(other),
        }
    }
    out
}
