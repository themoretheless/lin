use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::{Arc, Condvar, Mutex};

use rustc_hash::{FxHashMap, FxHashSet};

use crate::error::Error;
use crate::exec::{Db, Prepared};
use crate::store::Cell;

use super::cluster::{ConsistentHashRing, NodeMember, NodeRole, PubSubEvent, RaftCommand, RaftState};
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

pub(crate) struct EngineState {
    pub(crate) entries: FxHashMap<String, CacheEntry>,
    /// `(last_access_seq, key)` ordered ascending for O(log N) LRU eviction.
    lru_order: BTreeSet<(u64, String)>,
    /// `(expires_at_ms, key)` ordered ascending for O(log N) TTL expiration.
    ttl_index: BTreeSet<(u64, String)>,
    pub(crate) access_seq: u64,
    pub(crate) used_bytes: usize,
    pub(crate) hits: u64,
    pub(crate) misses: u64,
    pub(crate) evictions: u64,
    pub(crate) expired_cleaned: u64,
    /// Underlying `lin::Db` for durable persistence and pipe queries!
    pub(crate) db: Db,
    kv_prep: Option<KvPrepared>,
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
            access_seq: 1,
            used_bytes: 0,
            hits: 0,
            misses: 0,
            evictions: 0,
            expired_cleaned: 0,
            db,
            kv_prep,
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
                let key_owned = k.to_string();
                if let Some(prev) = self.entries.remove(&key_owned) {
                    self.lru_order.remove(&(prev.last_access_seq, key_owned.clone()));
                    if let Some(prev_exp) = prev.expires_at_ms {
                        self.ttl_index.remove(&(prev_exp, key_owned.clone()));
                    }
                    self.used_bytes = self
                        .used_bytes
                        .saturating_sub(key_owned.len() + prev.value.estimated_bytes());
                }
                let sz = key_owned.len() + value.estimated_bytes();
                self.access_seq += 1;
                let seq = self.access_seq;
                self.lru_order.insert((seq, key_owned.clone()));
                if let Some(exp) = exp_ms {
                    self.ttl_index.insert((exp, key_owned.clone()));
                }
                self.entries.insert(
                    key_owned,
                    CacheEntry {
                        value,
                        expires_at_ms: exp_ms,
                        last_access_seq: seq,
                        version: ver,
                    },
                );
                self.used_bytes += sz;
            }
        }
    }

    fn persist_entry_to_lin(&mut self, key: &str, entry: &CacheEntry) {
        let Ok(bytes) = rmp_serde::to_vec_named(&entry.value) else {
            return;
        };
        let hex_val = hex_encode(&bytes);
        let vtype = entry.value.type_name();
        let exp = entry.expires_at_ms.map(|e| e as i64).unwrap_or(0);
        let ver = entry.version as i64;

        if let Some(prep) = &self.kv_prep {
            let key_arc: Arc<str> = Arc::from(key);
            let vtype_arc: Arc<str> = Arc::from(vtype);
            let hex_arc: Arc<str> = Arc::from(hex_val);
            if self.db.store.point_index("cache_kv", "id", key).is_some() {
                let params = [
                    ("id", Cell::Text(Arc::clone(&key_arc))),
                    ("vtype", Cell::Text(vtype_arc)),
                    ("val_hex", Cell::Text(hex_arc)),
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
                        .execute_with(&mut self.db, &[("k", Cell::Text(Arc::clone(&key_arc)))]);
                }
                let params = [
                    ("id", Cell::Text(Arc::clone(&key_arc))),
                    ("k", Cell::Text(key_arc)),
                    ("vtype", Cell::Text(vtype_arc)),
                    ("val_hex", Cell::Text(hex_arc)),
                    ("exp_ms", Cell::Int(exp)),
                    ("ver", Cell::Int(ver)),
                ];
                let _ = prep.insert.execute_with(&mut self.db, &params);
            }
            return;
        }

        let escaped_key = escape_lin_str(key);
        let _ = self.db.execute(&format!(
            "delete cache_kv [k == \"{escaped_key}\"] cas \"\""
        ));
        let _ = self.db.execute(&format!(
            "insert cache_kv [{{ id: \"{escaped_key}\", k: \"{escaped_key}\", vtype: \"{vtype}\", val_hex: \"{hex_val}\", exp_ms: {exp}, ver: {ver} }}]"
        ));
    }

    fn delete_entry_from_lin(&mut self, key: &str) {
        if let Some(prep) = &self.kv_prep {
            let key_cell = Cell::Text(Arc::from(key));
            if self.db.store.point_index("cache_kv", "id", key).is_some() {
                let _ = prep.del_id.execute_with(&mut self.db, &[("id", key_cell)]);
            } else {
                let _ = prep.del_k.execute_with(&mut self.db, &[("k", key_cell)]);
            }
            return;
        }
        let escaped_key = escape_lin_str(key);
        let _ = self.db.execute(&format!(
            "delete cache_kv [k == \"{escaped_key}\"] cas \"\""
        ));
    }

    pub(crate) fn evict_expired_and_lru(&mut self, max_entries: usize, max_bytes: usize) {
        let now = unix_ms();
        // 1. Pop expired entries in O(log N) from `ttl_index` (O(1) peek when none expired!)
        while let Some((exp, _)) = self.ttl_index.first() {
            if *exp > now {
                break;
            }
            let (_, k) = self.ttl_index.pop_first().unwrap();
            if let Some(removed) = self.entries.remove(&k) {
                self.lru_order.remove(&(removed.last_access_seq, k.clone()));
                self.used_bytes = self
                    .used_bytes
                    .saturating_sub(k.len() + removed.value.estimated_bytes());
                self.expired_cleaned += 1;
                self.delete_entry_from_lin(&k);
            }
        }

        // 2. Pop LRU victims in O(log N) from `lru_order`
        while (self.entries.len() > max_entries || self.used_bytes > max_bytes)
            && !self.entries.is_empty()
        {
            let Some((_, oldest_key)) = self.lru_order.pop_first() else {
                break;
            };
            if let Some(removed) = self.entries.remove(&oldest_key) {
                if let Some(exp) = removed.expires_at_ms {
                    self.ttl_index.remove(&(exp, oldest_key.clone()));
                }
                self.used_bytes = self
                    .used_bytes
                    .saturating_sub(oldest_key.len() + removed.value.estimated_bytes());
                self.evictions += 1;
                self.delete_entry_from_lin(&oldest_key);
            }
        }
    }

    pub(crate) fn touch_or_remove_expired(&mut self, key: &str) -> bool {
        let now = unix_ms();
        if let Some(e) = self.entries.get(key)
            && e.is_expired(now)
        {
            if let Some(removed) = self.entries.remove(key) {
                let k_owned = key.to_string();
                self.lru_order.remove(&(removed.last_access_seq, k_owned.clone()));
                if let Some(exp) = removed.expires_at_ms {
                    self.ttl_index.remove(&(exp, k_owned));
                }
                self.used_bytes = self
                    .used_bytes
                    .saturating_sub(key.len() + removed.value.estimated_bytes());
                self.expired_cleaned += 1;
                self.delete_entry_from_lin(key);
            }
            return false;
        }
        if let Some(e) = self.entries.get_mut(key) {
            let old_seq = e.last_access_seq;
            self.access_seq += 1;
            let new_seq = self.access_seq;
            e.last_access_seq = new_seq;
            let k_owned = key.to_string();
            self.lru_order.remove(&(old_seq, k_owned.clone()));
            self.lru_order.insert((new_seq, k_owned));
            true
        } else {
            false
        }
    }

    pub(crate) fn put_value(
        &mut self,
        key: String,
        value: CacheValue,
        expires_at_ms: Option<u64>,
        max_entries: usize,
        max_bytes: usize,
    ) {
        if let Some(old) = self.entries.remove(&key) {
            self.lru_order.remove(&(old.last_access_seq, key.clone()));
            if let Some(old_exp) = old.expires_at_ms {
                self.ttl_index.remove(&(old_exp, key.clone()));
            }
            self.used_bytes = self
                .used_bytes
                .saturating_sub(key.len() + old.value.estimated_bytes());
        }
        self.access_seq += 1;
        let seq = self.access_seq;
        let ver = self.raft.last_log_index() + 1;
        let entry = CacheEntry {
            value,
            expires_at_ms,
            last_access_seq: seq,
            version: ver,
        };
        self.used_bytes += key.len() + entry.value.estimated_bytes();
        self.persist_entry_to_lin(&key, &entry);
        self.lru_order.insert((seq, key.clone()));
        if let Some(exp) = expires_at_ms {
            self.ttl_index.insert((exp, key.clone()));
        }
        self.entries.insert(key, entry);
        self.evict_expired_and_lru(max_entries, max_bytes);
    }

    pub(crate) fn remove_key(&mut self, key: &str) -> bool {
        if let Some(old) = self.entries.remove(key) {
            let k_owned = key.to_string();
            self.lru_order.remove(&(old.last_access_seq, k_owned.clone()));
            if let Some(old_exp) = old.expires_at_ms {
                self.ttl_index.remove(&(old_exp, k_owned));
            }
            self.used_bytes = self
                .used_bytes
                .saturating_sub(key.len() + old.value.estimated_bytes());
            self.delete_entry_from_lin(key);
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
                    key.clone(),
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
                let _ = self.db.execute("delete cache_kv [k is not null] cas \"\"");
            }
            RaftCommand::LinMutation { query } => {
                let _ = self.db.execute(query);
                self.refresh_kv_prepared_if_needed();
            }
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

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}

fn hex_decode(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        return None;
    }
    let mut out = Vec::with_capacity(s.len() / 2);
    let bytes = s.as_bytes();
    for i in (0..bytes.len()).step_by(2) {
        let hi = (bytes[i] as char).to_digit(16)? as u8;
        let lo = (bytes[i + 1] as char).to_digit(16)? as u8;
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
