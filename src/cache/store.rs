use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::ops::{Deref, DerefMut};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};

use rustc_hash::{FxHashMap, FxHashSet};

use crate::error::Error;
use crate::exec::{Db, Prepared};
use crate::store::Cell;

use super::cluster::{
    ConsistentHashRing, NodeMember, NodeRole, PubSubEvent, RaftCommand, RaftState,
};
use super::protocol::{CacheEntry, CacheResponse, CacheValue};
use super::{CacheConfig, unix_ms};

const PUBSUB_SEEN_MAX: usize = 4096;

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
}

struct KvPrepared {
    insert: Prepared,
    update: Prepared,
    del_id: Prepared,
    del_k: Prepared,
}

pub(crate) struct StoredEntry {
    pub(crate) entry: CacheEntry,
    /// Latest access sequence updated lock-free under `RwLock::read()`.
    pub(crate) live_seq: AtomicU64,
    /// Sequence currently stored inside `lru_order`.
    pub(crate) indexed_seq: u64,
}

impl StoredEntry {
    #[inline]
    pub(crate) fn new(entry: CacheEntry) -> Self {
        let seq = entry.last_access_seq;
        Self {
            entry,
            live_seq: AtomicU64::new(seq),
            indexed_seq: seq,
        }
    }
}

impl Deref for StoredEntry {
    type Target = CacheEntry;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.entry
    }
}

impl DerefMut for StoredEntry {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.entry
    }
}

pub(crate) enum ReadLookup<'a> {
    Hit(&'a CacheEntry),
    Miss,
    Expired,
}

pub(crate) struct EngineState {
    pub(crate) entries: FxHashMap<Arc<str>, StoredEntry>,
    /// `(indexed_seq, key)` ordered ascending for O(log N) lazy-LRU eviction.
    lru_order: BTreeSet<(u64, Arc<str>)>,
    /// `(expires_at_ms, key)` ordered ascending for O(log N) TTL expiration.
    ttl_index: BTreeSet<(u64, Arc<str>)>,
    pub(crate) access_seq: AtomicU64,
    pub(crate) used_bytes: usize,
    pub(crate) hits: AtomicU64,
    pub(crate) misses: AtomicU64,
    pub(crate) evictions: u64,
    pub(crate) expired_cleaned: u64,
    /// Underlying `lin::Db` for durable persistence and pipe queries!
    pub(crate) db: Db,
    kv_prep: Option<KvPrepared>,
    lazy_lin_sync: bool,
    dirty_lin_keys: FxHashSet<Arc<str>>,
    deleted_lin_keys: FxHashSet<Arc<str>>,
    flush_all_lin_pending: bool,
    /// Ring for consistent hashing across cluster nodes.
    pub(crate) ring: ConsistentHashRing,
    /// Gossip membership table (`node_id` -> `NodeMember`).
    pub(crate) members: BTreeMap<String, NodeMember>,
    /// Local heartbeat counter.
    pub(crate) heartbeat: u64,
    /// Raft consensus state.
    pub(crate) raft: RaftState,
    /// Pub/Sub state.
    pub(crate) pubsub: PubSubHub,
}

#[inline]
fn vtype_cell(val: &CacheValue) -> Cell {
    // Inline SmallText SSO cells (<= 22 bytes): bitwise Copy/Clone with zero heap allocs!
    Cell::from_text(val.type_name())
}

impl EngineState {
    pub(crate) fn new(mut db: Db, cfg: &CacheConfig) -> Result<Self, Error> {
        // Ensure `cache_kv` collection exists in `lin::Db`.
        // Use `var` instead of `text` so `val_hex` blobs are not tokenized into FTS postings!
        if db.catalog.collection("cache_kv").is_none() {
            let _ = db.execute(
                "col cache_kv { k: var, vtype: var, val_hex: var, exp_ms: i64, ver: i64 }",
            );
        }

        let kv_prep = Self::build_kv_prepared(&mut db);
        let lazy_lin_sync = !db.is_durable();

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

        let mut raft = RaftState::default();
        raft.leader_id = Some(cfg.node_id.clone());
        raft.leader_addr = Some(cfg.listen_addr.clone());

        let mut state = Self {
            entries: FxHashMap::default(),
            lru_order: BTreeSet::new(),
            ttl_index: BTreeSet::new(),
            access_seq: AtomicU64::new(1),
            used_bytes: 0,
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
            evictions: 0,
            expired_cleaned: 0,
            db,
            kv_prep,
            lazy_lin_sync,
            dirty_lin_keys: FxHashSet::default(),
            deleted_lin_keys: FxHashSet::default(),
            flush_all_lin_pending: false,
            ring,
            members,
            heartbeat: 1,
            raft,
            pubsub: PubSubHub::default(),
        };

        // Hydrate cache entries persisted in `lin::Db` (`cache_kv` collection)
        state.hydrate_from_lin_db();
        Ok(state)
    }

    #[inline]
    pub(crate) fn next_seq(&self) -> u64 {
        self.access_seq.fetch_add(1, Ordering::Relaxed) + 1
    }

    /// Lock-shared read lookup: stamps `live_seq` atomically without mutating `lru_order`
    /// or cloning the key string, and skips `unix_ms()` when the entry has no TTL.
    #[inline]
    pub(crate) fn read_live(&self, key: &str) -> ReadLookup<'_> {
        let Some(stored) = self.entries.get(key) else {
            return ReadLookup::Miss;
        };
        if stored.expires_at_ms.is_some() && stored.is_expired(unix_ms()) {
            return ReadLookup::Expired;
        }
        let seq = self.next_seq();
        stored.live_seq.store(seq, Ordering::Relaxed);
        ReadLookup::Hit(&stored.entry)
    }

    #[inline]
    pub(crate) fn has_pending_lin_sync(&self) -> bool {
        self.flush_all_lin_pending
            || !self.dirty_lin_keys.is_empty()
            || !self.deleted_lin_keys.is_empty()
    }

    pub(crate) fn flush_dirty_to_lin(&mut self) {
        if !self.has_pending_lin_sync() {
            return;
        }
        if self.flush_all_lin_pending {
            self.flush_all_lin_pending = false;
            if !self.db.store.collection("cache_kv").is_empty() {
                let _ = self.db.execute("delete cache_kv [k is not null] cas \"\"");
            }
        }
        if !self.deleted_lin_keys.is_empty() {
            let deleted: Vec<Arc<str>> = self.deleted_lin_keys.drain().collect();
            for k in &deleted {
                Self::delete_entry_from_db(&mut self.db, &self.kv_prep, k);
            }
        }
        if !self.dirty_lin_keys.is_empty() {
            let dirty: Vec<Arc<str>> = self.dirty_lin_keys.drain().collect();
            for k in &dirty {
                if let Some(stored) = self.entries.get(k) {
                    Self::persist_entry_to_db(&mut self.db, &self.kv_prep, k, &stored.entry);
                }
            }
        }
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

    fn hydrate_from_lin_db(&mut self) {
        let now = unix_ms();
        let rows = self.db.store.collection("cache_kv");
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
                let key_arc = match self.entries.remove_entry(k) {
                    Some((existing_arc, prev)) => {
                        self.lru_order
                            .remove(&(prev.indexed_seq, Arc::clone(&existing_arc)));
                        if let Some(prev_exp) = prev.expires_at_ms {
                            self.ttl_index
                                .remove(&(prev_exp, Arc::clone(&existing_arc)));
                        }
                        self.used_bytes = self
                            .used_bytes
                            .saturating_sub(existing_arc.len() + prev.value.estimated_bytes());
                        existing_arc
                    }
                    None => Arc::from(k),
                };
                let sz = key_arc.len() + value.estimated_bytes();
                let seq = self.next_seq();
                self.lru_order.insert((seq, Arc::clone(&key_arc)));
                if let Some(exp) = exp_ms {
                    self.ttl_index.insert((exp, Arc::clone(&key_arc)));
                }
                self.entries.insert(
                    key_arc,
                    StoredEntry::new(CacheEntry {
                        value,
                        expires_at_ms: exp_ms,
                        last_access_seq: seq,
                        version: ver,
                    }),
                );
                self.used_bytes += sz;
            }
        }
    }

    fn persist_entry_to_db(
        db: &mut Db,
        kv_prep: &Option<KvPrepared>,
        key: &Arc<str>,
        entry: &CacheEntry,
    ) {
        let Ok(bytes) = rmp_serde::to_vec_named(&entry.value) else {
            return;
        };
        let hex_val = hex_encode(&bytes);
        let vt_cell = vtype_cell(&entry.value);
        let exp = entry.expires_at_ms.map(|e| e as i64).unwrap_or(0);
        let ver = entry.version as i64;

        if let Some(prep) = kv_prep {
            let key_cell = Cell::from_arc_str(key);
            let hex_cell = Cell::from_text(&hex_val);
            if db.store.point_index("cache_kv", "id", key.as_ref()).is_some() {
                let params = [
                    ("id", key_cell),
                    ("vtype", vt_cell),
                    ("val_hex", hex_cell),
                    ("exp_ms", Cell::Int(exp)),
                    ("ver", Cell::Int(ver)),
                ];
                let _ = prep.update.execute_with(db, &params);
            } else {
                // Clean up any legacy row where `id` differed from `k`
                if !db.store.collection("cache_kv").is_empty()
                    && db.store.by_id.get("cache_kv").map_or(0, |m| m.len())
                        < db.store.collection("cache_kv").len()
                {
                    let _ = prep
                        .del_k
                        .execute_with(db, &[("k", key_cell.clone())]);
                }
                let params = [
                    ("id", key_cell.clone()),
                    ("k", key_cell),
                    ("vtype", vt_cell),
                    ("val_hex", hex_cell),
                    ("exp_ms", Cell::Int(exp)),
                    ("ver", Cell::Int(ver)),
                ];
                let _ = prep.insert.execute_with(db, &params);
            }
            return;
        }

        let vtype = entry.value.type_name();
        let escaped_key = escape_lin_str(key);
        let _ = db.execute(&format!(
            "delete cache_kv [k == \"{escaped_key}\"] cas \"\""
        ));
        let _ = db.execute(&format!(
            "insert cache_kv [{{ id: \"{escaped_key}\", k: \"{escaped_key}\", vtype: \"{vtype}\", val_hex: \"{hex_val}\", exp_ms: {exp}, ver: {ver} }}]"
        ));
    }

    fn delete_entry_from_db(db: &mut Db, kv_prep: &Option<KvPrepared>, key: &Arc<str>) {
        if db.store.collection("cache_kv").is_empty() {
            return;
        }
        if let Some(prep) = kv_prep {
            if db.store.point_index("cache_kv", "id", key.as_ref()).is_some() {
                let _ = prep
                    .del_id
                    .execute_with(db, &[("id", Cell::from_arc_str(key))]);
            } else if db.store.by_id.get("cache_kv").map_or(0, |m| m.len())
                < db.store.collection("cache_kv").len()
            {
                let _ = prep
                    .del_k
                    .execute_with(db, &[("k", Cell::from_arc_str(key))]);
            }
            return;
        }
        let escaped_key = escape_lin_str(key);
        let _ = db.execute(&format!(
            "delete cache_kv [k == \"{escaped_key}\"] cas \"\""
        ));
    }

    #[inline]
    fn record_lin_put(&mut self, key: &Arc<str>, entry: &CacheEntry) {
        if self.lazy_lin_sync {
            self.deleted_lin_keys.remove(key);
            self.dirty_lin_keys.insert(Arc::clone(key));
        } else {
            Self::persist_entry_to_db(&mut self.db, &self.kv_prep, key, entry);
        }
    }

    #[inline]
    fn record_lin_del(&mut self, key: Arc<str>) {
        if self.lazy_lin_sync {
            self.dirty_lin_keys.remove(&key);
            if !self.flush_all_lin_pending {
                self.deleted_lin_keys.insert(key);
            }
        } else {
            Self::delete_entry_from_db(&mut self.db, &self.kv_prep, &key);
        }
    }

    pub(crate) fn evict_expired_and_lru(&mut self, max_entries: usize, max_bytes: usize) {
        // 1. Pop expired entries in O(log N) from `ttl_index` (O(1) peek without `unix_ms()` when empty!)
        if !self.ttl_index.is_empty() {
            let now = unix_ms();
            while let Some((exp, _)) = self.ttl_index.first() {
                if *exp > now {
                    break;
                }
                let (_, k) = self.ttl_index.pop_first().unwrap();
                if let Some(removed) = self.entries.remove(&k) {
                    self.lru_order.remove(&(removed.indexed_seq, Arc::clone(&k)));
                    self.used_bytes = self
                        .used_bytes
                        .saturating_sub(k.len() + removed.value.estimated_bytes());
                    self.expired_cleaned += 1;
                    self.record_lin_del(k);
                }
            }
        }

        // 2. Pop LRU victims in O(log N) from `lru_order`, lazily re-indexing entries stamped on read
        while (self.entries.len() > max_entries || self.used_bytes > max_bytes)
            && !self.entries.is_empty()
        {
            let Some((popped_seq, oldest_key)) = self.lru_order.pop_first() else {
                break;
            };
            if let Some(stored) = self.entries.get_mut(&oldest_key) {
                let live = stored.live_seq.load(Ordering::Relaxed);
                if live > popped_seq {
                    stored.indexed_seq = live;
                    stored.entry.last_access_seq = live;
                    self.lru_order.insert((live, oldest_key));
                    continue;
                }
            }
            if let Some(removed) = self.entries.remove(&oldest_key) {
                if let Some(exp) = removed.expires_at_ms {
                    self.ttl_index.remove(&(exp, Arc::clone(&oldest_key)));
                }
                self.used_bytes = self
                    .used_bytes
                    .saturating_sub(oldest_key.len() + removed.value.estimated_bytes());
                self.evictions += 1;
                self.record_lin_del(oldest_key);
            }
        }
    }

    /// Check whether `key` exists and is live, purging it if expired, **without** mutating
    /// `lru_order` on a hit (so RMW commands don't remove+reinsert `lru_order` twice!).
    pub(crate) fn purge_if_expired(&mut self, key: &str) -> bool {
        let Some(e) = self.entries.get(key) else {
            return false;
        };
        if e.expires_at_ms.is_some() && e.is_expired(unix_ms()) {
            if let Some((k_arc, removed)) = self.entries.remove_entry(key) {
                self.lru_order
                    .remove(&(removed.indexed_seq, Arc::clone(&k_arc)));
                if let Some(exp) = removed.expires_at_ms {
                    self.ttl_index.remove(&(exp, Arc::clone(&k_arc)));
                }
                self.used_bytes = self
                    .used_bytes
                    .saturating_sub(k_arc.len() + removed.value.estimated_bytes());
                self.expired_cleaned += 1;
                self.record_lin_del(k_arc);
            }
            return false;
        }
        true
    }

    pub(crate) fn touch_or_remove_expired(&mut self, key: &str) -> bool {
        if !self.purge_if_expired(key) {
            return false;
        }
        let new_seq = self.next_seq();
        if let Some((k_arc, e)) = self.entries.get_key_value(key) {
            let k_arc = Arc::clone(k_arc);
            let old_indexed = e.indexed_seq;
            if let Some(e_mut) = self.entries.get_mut(key) {
                e_mut.indexed_seq = new_seq;
                e_mut.entry.last_access_seq = new_seq;
                e_mut.live_seq.store(new_seq, Ordering::Relaxed);
            }
            self.lru_order.remove(&(old_indexed, Arc::clone(&k_arc)));
            self.lru_order.insert((new_seq, k_arc));
            true
        } else {
            false
        }
    }

    pub(crate) fn put_value(
        &mut self,
        key: &str,
        value: CacheValue,
        expires_at_ms: Option<u64>,
        max_entries: usize,
        max_bytes: usize,
    ) {
        let key_arc: Arc<str> = match self.entries.remove_entry(key) {
            Some((existing_key, old)) => {
                self.lru_order
                    .remove(&(old.indexed_seq, Arc::clone(&existing_key)));
                if let Some(old_exp) = old.expires_at_ms {
                    self.ttl_index.remove(&(old_exp, Arc::clone(&existing_key)));
                }
                self.used_bytes = self
                    .used_bytes
                    .saturating_sub(existing_key.len() + old.value.estimated_bytes());
                existing_key
            }
            None => Arc::from(key),
        };
        let seq = self.next_seq();
        let ver = self.raft.last_log_index() + 1;
        let entry = CacheEntry {
            value,
            expires_at_ms,
            last_access_seq: seq,
            version: ver,
        };
        self.used_bytes += key_arc.len() + entry.value.estimated_bytes();
        self.record_lin_put(&key_arc, &entry);
        self.lru_order.insert((seq, Arc::clone(&key_arc)));
        if let Some(exp) = expires_at_ms {
            self.ttl_index.insert((exp, Arc::clone(&key_arc)));
        }
        self.entries.insert(key_arc, StoredEntry::new(entry));
        self.evict_expired_and_lru(max_entries, max_bytes);
    }

    pub(crate) fn remove_key(&mut self, key: &str) -> bool {
        if let Some((k_arc, old)) = self.entries.remove_entry(key) {
            self.lru_order
                .remove(&(old.indexed_seq, Arc::clone(&k_arc)));
            if let Some(old_exp) = old.expires_at_ms {
                self.ttl_index.remove(&(old_exp, Arc::clone(&k_arc)));
            }
            self.used_bytes = self
                .used_bytes
                .saturating_sub(k_arc.len() + old.value.estimated_bytes());
            self.record_lin_del(k_arc);
            true
        } else {
            false
        }
    }

    pub(crate) fn apply_raft_command(
        &mut self,
        cmd: &RaftCommand,
        max_entries: usize,
        max_bytes: usize,
    ) {
        match cmd {
            RaftCommand::Set {
                key,
                value,
                expires_at_ms,
            } => {
                self.put_value(
                    key,
                    value.clone(),
                    *expires_at_ms,
                    max_entries,
                    max_bytes,
                );
            }
            RaftCommand::Del { keys } => {
                for k in keys {
                    self.remove_key(k);
                }
            }
            RaftCommand::FlushAll => {
                self.entries.clear();
                self.lru_order.clear();
                self.ttl_index.clear();
                self.used_bytes = 0;
                if self.lazy_lin_sync {
                    self.dirty_lin_keys.clear();
                    self.deleted_lin_keys.clear();
                    self.flush_all_lin_pending = true;
                } else {
                    let _ = self.db.execute("delete cache_kv [k is not null] cas \"\"");
                }
            }
            RaftCommand::LinMutation { query } => {
                self.flush_dirty_to_lin();
                let _ = self.db.execute(query);
                self.refresh_kv_prepared_if_needed();
            }
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

    pub(crate) fn sync_local_member(&mut self, local_id: &str, bump_heartbeat: bool, now: u64) {
        if bump_heartbeat {
            self.heartbeat += 1;
        }
        let hb = self.heartbeat;
        let role = self.raft.role.clone();
        let term = self.raft.current_term;
        if let Some(me) = self.members.get_mut(local_id) {
            me.heartbeat = hb;
            me.last_seen_ms = now;
            me.alive = true;
            me.role = role;
            me.term = term;
        }
    }

    pub(crate) fn publish_local(
        &mut self,
        channel: &str,
        payload: &[u8],
        origin_node: &str,
    ) -> usize {
        let Some(subs) = self.pubsub.subscribers.get_mut(channel) else {
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
