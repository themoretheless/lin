use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::io::{BufReader, BufWriter, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, RwLock};
use std::thread;
use std::time::Duration;

use crate::error::Error;
use crate::exec::{Db, OpenOpts, SyncMode};

use super::client::{evict_peer_pool_addr, send_peer_rpc};
use super::cluster::{ConsistentHashRing, NodeMember, NodeRole, PubSubEvent, RaftCommand};
use super::protocol::{
    CacheEntry, CacheRequest, CacheResponse, CacheValue, read_frame_buf, write_frame_buf,
    write_frame_noflush_buf,
};
use super::store::{EngineState, ReadLookup};
use super::{CacheConfig, cache_err, unix_ms};

/// A running or embeddable distributed cache node backed by `lin::Db`.
#[derive(Clone)]
pub struct CacheNode {
    pub(crate) cfg: Arc<CacheConfig>,
    pub(crate) state: Arc<RwLock<EngineState>>,
    pub(crate) stop: Arc<AtomicBool>,
    stop_signal: Arc<(Mutex<bool>, Condvar)>,
    /// Serializes read-modify-write commands and keeps replication in log order.
    pub(crate) mutation_order: Arc<Mutex<()>>,
    /// Lock-free flag (`st.ring.nodes().len() > 1`) skipping shard routing on 1-node instances.
    pub(crate) has_peers: Arc<AtomicBool>,
    bound_addr: Arc<Mutex<Option<SocketAddr>>>,
}

impl CacheNode {
    /// Create a new in-memory distributed cache node (backed by `Db::fixture()`).
    pub fn new_in_memory(cfg: CacheConfig) -> Result<Self, Error> {
        let db = Db::fixture();
        Self::with_db(db, cfg)
    }

    /// Create a durable distributed cache node backed by a `lin::Db` directory.
    pub fn open_durable(path: impl AsRef<Path>, cfg: CacheConfig) -> Result<Self, Error> {
        let opts = OpenOpts {
            sync: SyncMode::Normal,
            ..OpenOpts::default()
        };
        let db = Db::open_with(path, opts)?;
        Self::with_db(db, cfg)
    }

    /// Create a `CacheNode` wrapping an existing `lin::Db`.
    pub fn with_db(db: Db, cfg: CacheConfig) -> Result<Self, Error> {
        let state = EngineState::new(db, &cfg)?;
        let multi = state.ring.nodes().len() > 1;
        Ok(Self {
            cfg: Arc::new(cfg),
            state: Arc::new(RwLock::new(state)),
            stop: Arc::new(AtomicBool::new(false)),
            stop_signal: Arc::new((Mutex::new(false), Condvar::new())),
            mutation_order: Arc::new(Mutex::new(())),
            has_peers: Arc::new(AtomicBool::new(multi)),
            bound_addr: Arc::new(Mutex::new(None)),
        })
    }

    #[inline]
    pub(crate) fn sync_has_peers(&self, st: &EngineState) {
        self.has_peers
            .store(st.ring.nodes().len() > 1, Ordering::Release);
    }

    pub fn config(&self) -> &CacheConfig {
        &self.cfg
    }

    pub fn stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
        let (lock, cvar) = &*self.stop_signal;
        if let Ok(mut stopped) = lock.lock() {
            *stopped = true;
            cvar.notify_all();
        }
        let listen_sock = self
            .bound_addr
            .lock()
            .ok()
            .and_then(|g| *g)
            .or_else(|| self.cfg.listen_addr.parse::<SocketAddr>().ok());
        if let Some(addr) = listen_sock {
            evict_peer_pool_addr(addr);
            if addr.port() != 0 {
                let _ = TcpStream::connect_timeout(&addr, Duration::from_millis(50));
            }
        }
    }

    /// Join another peer node into the cluster (updates Consistent Hash Ring + Gossip table).
    pub fn meet(&self, peer_id: impl Into<String>, peer_addr: impl Into<String>) {
        let peer_id = peer_id.into();
        let peer_addr = peer_addr.into();
        if let Ok(mut st) = self.state.write() {
            st.ring.add_node(&peer_id, &peer_addr);
            let term = st.raft.current_term;
            let now = unix_ms();
            if let Some(m) = st.members.get_mut(&peer_id) {
                if m.addr != peer_addr {
                    m.addr = peer_addr;
                }
                m.alive = true;
                m.last_seen_ms = now;
            } else {
                st.members.insert(
                    peer_id.clone(),
                    NodeMember {
                        node_id: peer_id,
                        addr: peer_addr,
                        heartbeat: 1,
                        last_seen_ms: now,
                        alive: true,
                        role: NodeRole::Follower,
                        term,
                    },
                );
            }
            self.sync_has_peers(&st);
        }
    }

    /// Execute a `CacheRequest` directly against this node (with optional shard routing & Raft replication).
    pub fn execute(&self, req: CacheRequest) -> CacheResponse {
        self.execute_inner(req, true)
    }

    #[inline]
    fn remote_owner_addr(&self, st: &EngineState, key: &str) -> Option<String> {
        let (owner_id, addr) = st.ring.primary_for_key_ref(key)?;
        if owner_id != self.cfg.node_id && st.members.get(owner_id).is_some_and(|m| m.alive) {
            Some(addr.to_owned())
        } else {
            None
        }
    }

    /// Fuses the consistent-hash shard-owner check and the lock-shared `read_live` lookup
    /// into a **single `self.state.read()`** critical section on all read hits/misses!
    #[inline]
    fn with_routed_read(
        &self,
        key: &str,
        allow_routing: bool,
        make_forward_req: impl FnOnce() -> CacheRequest,
        on_missing: impl FnOnce(&EngineState) -> CacheResponse,
        on_hit: impl FnOnce(&EngineState, &CacheEntry) -> CacheResponse,
    ) -> CacheResponse {
        let mut forward_addr: Option<String> = None;
        let mut was_expired = false;
        {
            let st = self.state.read().unwrap();
            if allow_routing
                && self.cfg.sharding_enabled
                && self.has_peers.load(Ordering::Relaxed)
                && let Some(addr) = self.remote_owner_addr(&st, key)
            {
                forward_addr = Some(addr);
            } else {
                match st.read_live(key) {
                    ReadLookup::Hit(entry) => return on_hit(&st, entry),
                    ReadLookup::Miss => return on_missing(&st),
                    ReadLookup::Expired => was_expired = true,
                }
            }
        }

        if let Some(peer_addr) = forward_addr {
            let forwarded = CacheRequest::InternalShardExec {
                inner: Box::new(make_forward_req()),
            };
            if let Ok(resp) = send_peer_rpc(&peer_addr, &forwarded) {
                return resp;
            }
            // Peer unreachable: fall back to local lookup!
            let st = self.state.read().unwrap();
            match st.read_live(key) {
                ReadLookup::Hit(entry) => return on_hit(&st, entry),
                ReadLookup::Miss => return on_missing(&st),
                ReadLookup::Expired => was_expired = true,
            }
        }

        debug_assert!(was_expired);
        let mut st = self.state.write().unwrap();
        if st.touch_or_remove_expired(key)
            && let Some(stored) = st.entries.get(key)
        {
            on_hit(&st, &stored.entry)
        } else {
            on_missing(&st)
        }
    }

    fn execute_inner(&self, req: CacheRequest, allow_routing: bool) -> CacheResponse {
        // Unwrap InternalShardExec so routed requests execute locally without re-routing
        let (req, allow_routing) = match req {
            CacheRequest::InternalShardExec { inner } => (*inner, false),
            other => (other, allow_routing),
        };

        // 1. Fast path for read commands: fuse local-owner check + read lookup in 1 `state.read()`!
        match req {
            CacheRequest::Ping(msg) => {
                return CacheResponse::Pong(msg.unwrap_or_else(|| self.cfg.node_id.clone()));
            }
            CacheRequest::Get { key } => {
                return self.with_routed_read(
                    &key,
                    allow_routing,
                    || CacheRequest::Get { key: key.clone() },
                    |st| {
                        st.misses.fetch_add(1, Ordering::Relaxed);
                        CacheResponse::Nil
                    },
                    |st, entry| {
                        let val = match &entry.value {
                            CacheValue::String(b) => CacheResponse::Bulk(b.clone()),
                            CacheValue::Int(n) => CacheResponse::Bulk(n.to_string().into_bytes()),
                            other => CacheResponse::Text(other.as_str_lossy()),
                        };
                        st.hits.fetch_add(1, Ordering::Relaxed);
                        val
                    },
                );
            }
            CacheRequest::Exists { key } => {
                return self.with_routed_read(
                    &key,
                    allow_routing,
                    || CacheRequest::Exists { key: key.clone() },
                    |_| CacheResponse::Int(0),
                    |_, _| CacheResponse::Int(1),
                );
            }
            CacheRequest::Ttl { key } => {
                return self.with_routed_read(
                    &key,
                    allow_routing,
                    || CacheRequest::Ttl { key: key.clone() },
                    |_| CacheResponse::Int(-2),
                    |_, entry| match entry.expires_at_ms {
                        Some(exp) => CacheResponse::Int(exp.saturating_sub(unix_ms()) as i64),
                        None => CacheResponse::Int(-1),
                    },
                );
            }
            CacheRequest::HGet { key, field } => {
                return self.with_routed_read(
                    &key,
                    allow_routing,
                    || CacheRequest::HGet {
                        key: key.clone(),
                        field: field.clone(),
                    },
                    |st| {
                        st.misses.fetch_add(1, Ordering::Relaxed);
                        CacheResponse::Nil
                    },
                    |st, entry| match &entry.value {
                        CacheValue::Hash(m) => match m.get(&field) {
                            Some(v) => {
                                st.hits.fetch_add(1, Ordering::Relaxed);
                                CacheResponse::Bulk(v.clone())
                            }
                            None => {
                                st.misses.fetch_add(1, Ordering::Relaxed);
                                CacheResponse::Nil
                            }
                        },
                        _ => CacheResponse::Error("WRONGTYPE Key is not a hash".into()),
                    },
                );
            }
            CacheRequest::HGetAll { key } => {
                return self.with_routed_read(
                    &key,
                    allow_routing,
                    || CacheRequest::HGetAll { key: key.clone() },
                    |_| CacheResponse::Map(BTreeMap::new()),
                    |st, entry| match &entry.value {
                        CacheValue::Hash(m) => {
                            let m = m.clone();
                            st.hits.fetch_add(1, Ordering::Relaxed);
                            CacheResponse::Map(m)
                        }
                        _ => CacheResponse::Error("WRONGTYPE Key is not a hash".into()),
                    },
                );
            }
            CacheRequest::LRange { key, start, stop } => {
                return self.with_routed_read(
                    &key,
                    allow_routing,
                    || CacheRequest::LRange {
                        key: key.clone(),
                        start,
                        stop,
                    },
                    |_| CacheResponse::Array(Vec::new()),
                    |st, entry| match &entry.value {
                        CacheValue::List(list) => {
                            let len = list.len() as isize;
                            if len == 0 {
                                return CacheResponse::Array(Vec::new());
                            }
                            let s = if start < 0 {
                                (len + start).max(0)
                            } else {
                                start.min(len)
                            };
                            let e = if stop < 0 {
                                len + stop
                            } else {
                                stop.min(len - 1)
                            };
                            if s > e || s >= len {
                                return CacheResponse::Array(Vec::new());
                            }
                            let slice: Vec<CacheResponse> = list
                                .iter()
                                .skip(s as usize)
                                .take((e - s + 1) as usize)
                                .map(|v| CacheResponse::Bulk(v.clone()))
                                .collect();
                            st.hits.fetch_add(1, Ordering::Relaxed);
                            CacheResponse::Array(slice)
                        }
                        _ => CacheResponse::Error("WRONGTYPE Key is not a list".into()),
                    },
                );
            }
            CacheRequest::SMembers { key } => {
                return self.with_routed_read(
                    &key,
                    allow_routing,
                    || CacheRequest::SMembers { key: key.clone() },
                    |_| CacheResponse::Array(Vec::new()),
                    |st, entry| match &entry.value {
                        CacheValue::Set(s) => {
                            let out = s
                                .iter()
                                .map(|m| CacheResponse::Bulk(m.as_bytes().to_vec()))
                                .collect();
                            st.hits.fetch_add(1, Ordering::Relaxed);
                            CacheResponse::Array(out)
                        }
                        _ => CacheResponse::Error("WRONGTYPE Key is not a set".into()),
                    },
                );
            }
            CacheRequest::SIsMember { key, member } => {
                return self.with_routed_read(
                    &key,
                    allow_routing,
                    || CacheRequest::SIsMember {
                        key: key.clone(),
                        member: member.clone(),
                    },
                    |_| CacheResponse::Int(0),
                    |_, entry| match &entry.value {
                        CacheValue::Set(s) => CacheResponse::Int(i64::from(s.contains(&member))),
                        _ => CacheResponse::Error("WRONGTYPE Key is not a set".into()),
                    },
                );
            }
            _ => {}
        }

        // 2. Check Consistent Hash Ring routing for write commands only when cluster has peers
        if allow_routing
            && self.cfg.sharding_enabled
            && self.has_peers.load(Ordering::Acquire)
            && let Some(key) = Self::extract_mutation_routing_key(&req)
        {
            let target_peer = {
                let st = self.state.read().unwrap();
                self.remote_owner_addr(&st, key)
            };
            if let Some(peer_addr) = target_peer {
                let forwarded = CacheRequest::InternalShardExec {
                    inner: Box::new(req.clone()),
                };
                if let Ok(resp) = send_peer_rpc(&peer_addr, &forwarded) {
                    return resp;
                }
                // Fallback to local execution if peer is unreachable
            }
        }

        // 3. Execute mutation / admin / internal command
        match req {
            CacheRequest::Set { key, value, ttl_ms } => {
                let expires_at_ms = ttl_ms.map(|t| unix_ms() + t);
                let val = CacheValue::String(value);
                self.propose_and_apply_mutation(RaftCommand::Set {
                    key,
                    value: val,
                    expires_at_ms,
                });
                CacheResponse::Ok
            }
            CacheRequest::Del { keys } => {
                let order = self.mutation_order();
                let mut removed_count = 0i64;
                {
                    let mut st = self.state.write().unwrap();
                    for k in &keys {
                        if st.purge_if_expired(k) {
                            removed_count += 1;
                        }
                    }
                }
                self.propose_and_apply_held(&order, RaftCommand::Del { keys });
                CacheResponse::Int(removed_count)
            }
            CacheRequest::IncrBy { key, delta } => {
                let order = self.mutation_order();
                let (new_val, expires_at_ms) = {
                    let mut st = self.state.write().unwrap();
                    let _ = st.purge_if_expired(&key);
                    match st.entries.get(key.as_str()) {
                        Some(entry) => {
                            let cur = match &entry.value {
                                CacheValue::Int(n) => *n,
                                CacheValue::String(b) => {
                                    let Ok(s) = std::str::from_utf8(b) else {
                                        return CacheResponse::Error(
                                            "ERR value is not an integer".into(),
                                        );
                                    };
                                    let Ok(n) = s.trim().parse::<i64>() else {
                                        return CacheResponse::Error(
                                            "ERR value is not an integer".into(),
                                        );
                                    };
                                    n
                                }
                                _ => {
                                    return CacheResponse::Error(
                                        "WRONGTYPE Operation against a key holding the wrong kind of value".into(),
                                    );
                                }
                            };
                            (cur.wrapping_add(delta), entry.expires_at_ms)
                        }
                        None => (delta, None),
                    }
                };
                self.propose_and_apply_held(
                    &order,
                    RaftCommand::Set {
                        key,
                        value: CacheValue::Int(new_val),
                        expires_at_ms,
                    },
                );
                CacheResponse::Int(new_val)
            }
            CacheRequest::Expire { key, ttl_ms } => {
                let order = self.mutation_order();
                let updated = {
                    let mut st = self.state.write().unwrap();
                    if !st.purge_if_expired(&key) {
                        None
                    } else {
                        st.entries.get(key.as_str()).map(|e| e.value.clone())
                    }
                };
                if let Some(value) = updated {
                    let expires_at_ms = Some(unix_ms() + ttl_ms);
                    self.propose_and_apply_held(
                        &order,
                        RaftCommand::Set {
                            key,
                            value,
                            expires_at_ms,
                        },
                    );
                    CacheResponse::Int(1)
                } else {
                    CacheResponse::Int(0)
                }
            }
            CacheRequest::Persist { key } => {
                let order = self.mutation_order();
                let updated = {
                    let mut st = self.state.write().unwrap();
                    if !st.purge_if_expired(&key) {
                        None
                    } else if let Some(stored) = st.entries.get(key.as_str()) {
                        if stored.expires_at_ms.is_some() {
                            Some(stored.value.clone())
                        } else {
                            let seq = st.next_seq();
                            stored.live_seq.store(seq, Ordering::Relaxed);
                            None
                        }
                    } else {
                        None
                    }
                };
                if let Some(value) = updated {
                    self.propose_and_apply_held(
                        &order,
                        RaftCommand::Set {
                            key,
                            value,
                            expires_at_ms: None,
                        },
                    );
                    CacheResponse::Int(1)
                } else {
                    CacheResponse::Int(0)
                }
            }
            CacheRequest::Keys { prefix } => {
                let mut st = self.state.write().unwrap();
                st.evict_expired_and_lru(self.cfg.max_entries, self.cfg.max_bytes);
                let pat = prefix.trim_end_matches('*');
                let mut matching: Vec<&str> = st
                    .entries
                    .keys()
                    .map(Arc::as_ref)
                    .filter(|k| pat.is_empty() || k.starts_with(pat))
                    .collect();
                matching.sort_unstable();
                CacheResponse::Array(
                    matching
                        .into_iter()
                        .map(|k| CacheResponse::Bulk(k.as_bytes().to_vec()))
                        .collect(),
                )
            }
            CacheRequest::FlushAll => {
                self.propose_and_apply_mutation(RaftCommand::FlushAll);
                CacheResponse::Ok
            }

            // --- Hashes ---
            CacheRequest::HSet { key, field, value } => {
                let order = self.mutation_order();
                let (map, is_new, exp) = {
                    let mut st = self.state.write().unwrap();
                    let _ = st.purge_if_expired(&key);
                    match st.entries.get(key.as_str()) {
                        Some(entry) => match &entry.value {
                            CacheValue::Hash(m) => {
                                let mut m = m.clone();
                                let is_new = m.insert(field, value).is_none();
                                (m, is_new, entry.expires_at_ms)
                            }
                            _ => {
                                return CacheResponse::Error("WRONGTYPE Key is not a hash".into());
                            }
                        },
                        None => {
                            let mut m = BTreeMap::new();
                            m.insert(field, value);
                            (m, true, None)
                        }
                    }
                };
                self.propose_and_apply_held(
                    &order,
                    RaftCommand::Set {
                        key,
                        value: CacheValue::Hash(map),
                        expires_at_ms: exp,
                    },
                );
                CacheResponse::Int(i64::from(is_new))
            }
            CacheRequest::HDel { key, fields } => {
                let order = self.mutation_order();
                let res = {
                    let mut st = self.state.write().unwrap();
                    if !st.purge_if_expired(&key) {
                        return CacheResponse::Int(0);
                    }
                    match st.entries.get(key.as_str()) {
                        Some(entry) => match &entry.value {
                            CacheValue::Hash(m) => {
                                let mut m = m.clone();
                                let mut removed = 0i64;
                                for f in &fields {
                                    if m.remove(f).is_some() {
                                        removed += 1;
                                    }
                                }
                                if removed == 0 {
                                    let seq = st.next_seq();
                                    entry.live_seq.store(seq, Ordering::Relaxed);
                                }
                                Some((m, removed, entry.expires_at_ms))
                            }
                            _ => None,
                        },
                        None => return CacheResponse::Int(0),
                    }
                };
                match res {
                    Some((m, removed, exp)) => {
                        if m.is_empty() {
                            self.propose_and_apply_held(
                                &order,
                                RaftCommand::Del { keys: vec![key] },
                            );
                        } else if removed > 0 {
                            self.propose_and_apply_held(
                                &order,
                                RaftCommand::Set {
                                    key,
                                    value: CacheValue::Hash(m),
                                    expires_at_ms: exp,
                                },
                            );
                        }
                        CacheResponse::Int(removed)
                    }
                    None => CacheResponse::Error("WRONGTYPE Key is not a hash".into()),
                }
            }

            // --- Lists ---
            CacheRequest::LPush { key, values } => self.exec_list_push(key, values, true),
            CacheRequest::RPush { key, values } => self.exec_list_push(key, values, false),
            CacheRequest::LPop { key } => self.exec_list_pop(key, true),
            CacheRequest::RPop { key } => self.exec_list_pop(key, false),

            // --- Sets ---
            CacheRequest::SAdd { key, members } => {
                let order = self.mutation_order();
                let (set, added, exp) = {
                    let mut st = self.state.write().unwrap();
                    let _ = st.purge_if_expired(&key);
                    match st.entries.get(key.as_str()) {
                        Some(entry) => match &entry.value {
                            CacheValue::Set(s) => {
                                let mut s = s.clone();
                                let mut added = 0i64;
                                for m in members {
                                    if s.insert(m) {
                                        added += 1;
                                    }
                                }
                                (s, added, entry.expires_at_ms)
                            }
                            _ => {
                                return CacheResponse::Error("WRONGTYPE Key is not a set".into());
                            }
                        },
                        None => {
                            let mut s = BTreeSet::new();
                            let mut added = 0i64;
                            for m in members {
                                if s.insert(m) {
                                    added += 1;
                                }
                            }
                            (s, added, None)
                        }
                    }
                };
                self.propose_and_apply_held(
                    &order,
                    RaftCommand::Set {
                        key,
                        value: CacheValue::Set(set),
                        expires_at_ms: exp,
                    },
                );
                CacheResponse::Int(added)
            }
            CacheRequest::SRem { key, members } => {
                let order = self.mutation_order();
                let res = {
                    let mut st = self.state.write().unwrap();
                    if !st.purge_if_expired(&key) {
                        return CacheResponse::Int(0);
                    }
                    match st.entries.get(key.as_str()) {
                        Some(entry) => match &entry.value {
                            CacheValue::Set(s) => {
                                let mut s = s.clone();
                                let mut removed = 0i64;
                                for m in &members {
                                    if s.remove(m) {
                                        removed += 1;
                                    }
                                }
                                if removed == 0 {
                                    let seq = st.next_seq();
                                    entry.live_seq.store(seq, Ordering::Relaxed);
                                }
                                Some((s, removed, entry.expires_at_ms))
                            }
                            _ => None,
                        },
                        None => return CacheResponse::Int(0),
                    }
                };
                match res {
                    Some((s, removed, exp)) => {
                        if s.is_empty() {
                            self.propose_and_apply_held(
                                &order,
                                RaftCommand::Del { keys: vec![key] },
                            );
                        } else if removed > 0 {
                            self.propose_and_apply_held(
                                &order,
                                RaftCommand::Set {
                                    key,
                                    value: CacheValue::Set(s),
                                    expires_at_ms: exp,
                                },
                            );
                        }
                        CacheResponse::Int(removed)
                    }
                    None => CacheResponse::Error("WRONGTYPE Key is not a set".into()),
                }
            }

            // --- Pub/Sub ---
            CacheRequest::Publish { channel, message } => {
                let (local_subs, event, peers, members_snap) = {
                    let mut st = self.state.write().unwrap();
                    let eid = (crc32fast::hash(self.cfg.node_id.as_bytes()) as u64) << 32
                        | st.pubsub.next_event_id;
                    st.pubsub.next_event_id += 1;
                    st.pubsub.mark_seen(eid);
                    let count = st.publish_local(&channel, &message, &self.cfg.node_id);
                    let ev = PubSubEvent {
                        id: eid,
                        channel,
                        payload: message,
                        origin_node: self.cfg.node_id.clone(),
                    };
                    st.pubsub.recent_outbox.push_back(ev.clone());
                    while st.pubsub.recent_outbox.len() > 64 {
                        st.pubsub.recent_outbox.pop_front();
                    }
                    let peers: Vec<String> = st
                        .members
                        .values()
                        .filter(|m| m.node_id != self.cfg.node_id && m.alive)
                        .map(|m| m.addr.clone())
                        .collect();
                    let members_snap = if peers.is_empty() {
                        Vec::new()
                    } else {
                        st.members.values().cloned().collect()
                    };
                    (count, ev, peers, members_snap)
                };
                // Propagate Pub/Sub event immediately to alive cluster peers in parallel
                if !peers.is_empty() {
                    let sync_req = CacheRequest::GossipSync {
                        sender_id: self.cfg.node_id.clone(),
                        sender_addr: self.cfg.listen_addr.clone(),
                        members: members_snap,
                        pubsub_events: vec![event],
                    };
                    Self::broadcast_peer_rpc(&peers, &sync_req);
                }
                CacheResponse::Int(local_subs as i64)
            }
            CacheRequest::Subscribe { channels } => {
                CacheResponse::Text(format!("Subscribed to: {}", channels.join(", ")))
            }

            // --- Native Lin Query ---
            CacheRequest::LinQuery { query } => {
                let is_write = {
                    let trimmed = query.trim_start();
                    trimmed.starts_with("insert ")
                        || trimmed.starts_with("append ")
                        || trimmed.starts_with("update ")
                        || trimmed.starts_with("delete ")
                        || trimmed.starts_with("col ")
                        || trimmed.starts_with("rel ")
                        || trimmed.starts_with("index ")
                };
                if !is_write {
                    let fast_out = {
                        let st = self.state.read().unwrap();
                        if !st.has_pending_lin_sync() && !st.db.store.has_deferred_doc_ops() {
                            match st.db.run_readonly(&query) {
                                Ok(h) => Some(Ok(h.compact())),
                                Err(e) if e.to_string().contains("writes not allowed") => None,
                                Err(e) => Some(Err(e)),
                            }
                        } else {
                            None
                        }
                    };
                    if let Some(res) = fast_out {
                        return match res {
                            Ok(out) => CacheResponse::Text(out),
                            Err(e) => CacheResponse::Error(e.to_string()),
                        };
                    }
                }
                let res = {
                    let mut st = self.state.write().unwrap();
                    st.flush_dirty_to_lin();
                    let out = st.db.run(&query).map(|h| h.compact());
                    if out.is_ok() && is_write {
                        st.refresh_kv_prepared_if_needed();
                    }
                    out
                };
                match res {
                    Ok(out) => {
                        if is_write {
                            self.propose_raft_only(RaftCommand::LinMutation { query });
                        }
                        CacheResponse::Text(out)
                    }
                    Err(e) => CacheResponse::Error(e.to_string()),
                }
            }

            // --- Cluster / Ring / Raft Introspection ---
            CacheRequest::ClusterInfo => {
                let needs_flush = self.state.read().unwrap().has_pending_lin_sync();
                if needs_flush {
                    self.state.write().unwrap().flush_dirty_to_lin();
                }
                let st = self.state.read().unwrap();
                let db_stats = st.db.stats();
                let info = format!(
                    "node_id:{}\n\
                     listen_addr:{}\n\
                     role:{}\n\
                     raft_term:{}\n\
                     raft_leader:{}\n\
                     raft_log_len:{}\n\
                     raft_commit_index:{}\n\
                     cluster_nodes:{}\n\
                     ring_vnodes:{}\n\
                     replication_factor:{}\n\
                     cache_keys:{}\n\
                     cache_used_bytes:{}\n\
                     cache_hits:{}\n\
                     cache_misses:{}\n\
                     cache_evictions:{}\n\
                     cache_expired_cleaned:{}\n\
                     lin_db_gen:{}\n\
                     lin_db_docs:{}\n\
                     lin_db_facts:{}",
                    self.cfg.node_id,
                    self.cfg.listen_addr,
                    st.raft.role.as_str(),
                    st.raft.current_term,
                    st.raft.leader_id.as_deref().unwrap_or("none"),
                    st.raft.log.len(),
                    st.raft.commit_index,
                    st.members.len(),
                    self.cfg.vnodes,
                    self.cfg.replication_factor,
                    st.entries.len(),
                    st.used_bytes,
                    st.hits.load(Ordering::Relaxed),
                    st.misses.load(Ordering::Relaxed),
                    st.evictions,
                    st.expired_cleaned,
                    db_stats.r#gen,
                    db_stats.docs,
                    db_stats.facts,
                );
                CacheResponse::Text(info)
            }
            CacheRequest::ClusterNodes => {
                let st = self.state.read().unwrap();
                let mut out = String::with_capacity(st.members.len() * 64);
                for (i, m) in st.members.values().enumerate() {
                    if i > 0 {
                        out.push('\n');
                    }
                    let self_tag = if m.node_id == self.cfg.node_id {
                        "myself,"
                    } else {
                        ""
                    };
                    let state_tag = if m.alive { "alive" } else { "fail" };
                    use std::fmt::Write;
                    let _ = write!(
                        &mut out,
                        "{} {} {}{},{} term={} hb={}",
                        m.node_id,
                        m.addr,
                        self_tag,
                        m.role.as_str(),
                        state_tag,
                        m.term,
                        m.heartbeat
                    );
                }
                CacheResponse::Text(out)
            }
            CacheRequest::ClusterMeet { node_id, addr } => {
                self.meet(node_id, addr.clone());
                // Immediately trigger a gossip handshake with the new peer
                let _ = self.gossip_once_with(&addr);
                CacheResponse::Ok
            }
            CacheRequest::ClusterKeySlot { key } => {
                let st = self.state.read().unwrap();
                let hash = ConsistentHashRing::hash_key(&key);
                let replicas = st.ring.replicas_for_key(&key);
                let mut out = format!("key={key} ring_hash={hash} owners=[");
                for (i, (id, addr)) in replicas.into_iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    out.push_str(&id);
                    out.push('@');
                    out.push_str(&addr);
                }
                out.push(']');
                CacheResponse::Text(out)
            }
            CacheRequest::RaftStatus => {
                let st = self.state.read().unwrap();
                CacheResponse::Text(format!(
                    "role={} term={} leader={} voted_for={} log_len={} commit_index={} last_applied={}",
                    st.raft.role.as_str(),
                    st.raft.current_term,
                    st.raft.leader_id.as_deref().unwrap_or("none"),
                    st.raft.voted_for.as_deref().unwrap_or("none"),
                    st.raft.log.len(),
                    st.raft.commit_index,
                    st.raft.last_applied,
                ))
            }
            CacheRequest::RaftTriggerElection => {
                let won = self.run_leader_election();
                let st = self.state.read().unwrap();
                CacheResponse::Text(format!(
                    "election_won={} role={} term={} leader={}",
                    won,
                    st.raft.role.as_str(),
                    st.raft.current_term,
                    st.raft.leader_id.as_deref().unwrap_or("none")
                ))
            }

            // --- Internal Gossip & Raft RPC Handlers ---
            CacheRequest::GossipSync {
                sender_id,
                sender_addr,
                members,
                pubsub_events,
            } => self.handle_gossip_sync(sender_id, sender_addr, members, pubsub_events),
            CacheRequest::RaftRequestVote {
                term,
                candidate_id,
                last_log_index,
                last_log_term,
            } => self.handle_raft_request_vote(term, candidate_id, last_log_index, last_log_term),
            CacheRequest::RaftAppendEntries {
                term,
                leader_id,
                leader_addr,
                prev_log_index,
                prev_log_term,
                entries,
                leader_commit,
            } => self.handle_raft_append_entries(
                term,
                leader_id,
                leader_addr,
                prev_log_index,
                prev_log_term,
                entries,
                leader_commit,
            ),
            // Already handled in fast read dispatch / unwrapped above
            CacheRequest::Ping(_)
            | CacheRequest::Get { .. }
            | CacheRequest::Exists { .. }
            | CacheRequest::Ttl { .. }
            | CacheRequest::HGet { .. }
            | CacheRequest::HGetAll { .. }
            | CacheRequest::LRange { .. }
            | CacheRequest::SMembers { .. }
            | CacheRequest::SIsMember { .. }
            | CacheRequest::InternalShardExec { .. } => unreachable!(),
        }
    }

    fn extract_mutation_routing_key(req: &CacheRequest) -> Option<&str> {
        match req {
            CacheRequest::Set { key, .. }
            | CacheRequest::IncrBy { key, .. }
            | CacheRequest::Expire { key, .. }
            | CacheRequest::Persist { key }
            | CacheRequest::HSet { key, .. }
            | CacheRequest::HDel { key, .. }
            | CacheRequest::LPush { key, .. }
            | CacheRequest::RPush { key, .. }
            | CacheRequest::LPop { key }
            | CacheRequest::RPop { key }
            | CacheRequest::SAdd { key, .. }
            | CacheRequest::SRem { key, .. } => Some(key.as_str()),
            CacheRequest::Del { keys } if keys.len() == 1 => Some(keys[0].as_str()),
            _ => None,
        }
    }

    fn exec_list_push(&self, key: String, values: Vec<Vec<u8>>, left: bool) -> CacheResponse {
        let order = self.mutation_order();
        let (list, len, exp) = {
            let mut st = self.state.write().unwrap();
            let _ = st.purge_if_expired(&key);
            match st.entries.get(key.as_str()) {
                Some(entry) => match &entry.value {
                    CacheValue::List(l) => {
                        let mut l = l.clone();
                        for v in values {
                            if left {
                                l.push_front(v);
                            } else {
                                l.push_back(v);
                            }
                        }
                        let len = l.len() as i64;
                        (l, len, entry.expires_at_ms)
                    }
                    _ => return CacheResponse::Error("WRONGTYPE Key is not a list".into()),
                },
                None => {
                    let mut l = VecDeque::with_capacity(values.len());
                    for v in values {
                        if left {
                            l.push_front(v);
                        } else {
                            l.push_back(v);
                        }
                    }
                    let len = l.len() as i64;
                    (l, len, None)
                }
            }
        };
        self.propose_and_apply_held(
            &order,
            RaftCommand::Set {
                key,
                value: CacheValue::List(list),
                expires_at_ms: exp,
            },
        );
        CacheResponse::Int(len)
    }

    fn exec_list_pop(&self, key: String, left: bool) -> CacheResponse {
        let order = self.mutation_order();
        let res = {
            let mut st = self.state.write().unwrap();
            if !st.purge_if_expired(&key) {
                return CacheResponse::Nil;
            }
            match st.entries.get(key.as_str()) {
                Some(entry) => match &entry.value {
                    CacheValue::List(l) => {
                        let mut l = l.clone();
                        let popped = if left { l.pop_front() } else { l.pop_back() };
                        Some((l, popped, entry.expires_at_ms))
                    }
                    _ => None,
                },
                None => return CacheResponse::Nil,
            }
        };
        match res {
            Some((l, Some(popped), exp)) => {
                if l.is_empty() {
                    self.propose_and_apply_held(&order, RaftCommand::Del { keys: vec![key] });
                } else {
                    self.propose_and_apply_held(
                        &order,
                        RaftCommand::Set {
                            key,
                            value: CacheValue::List(l),
                            expires_at_ms: exp,
                        },
                    );
                }
                CacheResponse::Bulk(popped)
            }
            Some((_, None, _)) => CacheResponse::Nil,
            None => CacheResponse::Error("WRONGTYPE Key is not a list".into()),
        }
    }

    /// Start background Gossip & TTL cleanup loop + serve TCP connections on `listener`.
    pub fn serve(&self, listener: TcpListener) -> Result<(), Error> {
        listener.set_nonblocking(false).map_err(cache_err)?;
        if let Ok(local) = listener.local_addr()
            && let Ok(mut g) = self.bound_addr.lock()
        {
            *g = Some(local);
        }

        let bg_node = self.clone();
        let gossip_handle = thread::spawn(move || {
            bg_node.background_loop();
        });

        while !self.stop.load(Ordering::Acquire) {
            match listener.accept() {
                Ok((stream, _peer)) => {
                    if self.stop.load(Ordering::Acquire) {
                        break;
                    }
                    let node = self.clone();
                    thread::spawn(move || {
                        let _ = node.handle_connection(stream);
                    });
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(_) => {
                    if self.stop.load(Ordering::Acquire) {
                        break;
                    }
                    thread::sleep(Duration::from_millis(20));
                }
            }
        }

        let _ = gossip_handle.join();
        Ok(())
    }

    fn background_loop(&self) {
        let interval = Duration::from_millis(self.cfg.gossip_interval_ms.max(50));
        let (stop_lock, stop_cvar) = &*self.stop_signal;
        while !self.stop.load(Ordering::Relaxed) {
            {
                let guard = stop_lock.lock().unwrap();
                if *guard || self.stop.load(Ordering::Relaxed) {
                    break;
                }
                let (g, _) = stop_cvar.wait_timeout(guard, interval).unwrap();
                if *g || self.stop.load(Ordering::Relaxed) {
                    break;
                }
            }
            let peers: Vec<(String, String)> = {
                let mut st = self.state.write().unwrap();
                st.evict_expired_and_lru(self.cfg.max_entries, self.cfg.max_bytes);
                let now = unix_ms();
                let timeout = self.cfg.node_timeout_ms;
                let mut dead_nodes = Vec::new();
                for m in st.members.values_mut() {
                    if m.node_id != self.cfg.node_id && now.saturating_sub(m.last_seen_ms) > timeout
                    {
                        m.alive = false;
                        dead_nodes.push(m.node_id.clone());
                    }
                }
                if !dead_nodes.is_empty() {
                    for dn in dead_nodes {
                        st.ring.remove_node(&dn);
                    }
                    self.sync_has_peers(&st);
                }
                st.members
                    .values()
                    .filter(|m| m.node_id != self.cfg.node_id)
                    .map(|m| (m.node_id.clone(), m.addr.clone()))
                    .collect()
            };

            for (peer_id, peer_addr) in peers {
                if self.gossip_once_with(&peer_addr).is_err() {
                    let mut st = self.state.write().unwrap();
                    let now = unix_ms();
                    if let Some(m) = st.members.get_mut(&peer_id)
                        && now.saturating_sub(m.last_seen_ms) > self.cfg.node_timeout_ms
                    {
                        m.alive = false;
                        st.ring.remove_node(&peer_id);
                        self.sync_has_peers(&st);
                    }
                }
            }
        }
    }

    fn handle_connection(&self, stream: TcpStream) -> Result<(), Error> {
        let _ = stream.set_nodelay(true);
        let write_sock = stream.try_clone().map_err(cache_err)?;
        let mut reader = BufReader::with_capacity(16 * 1024, stream);
        let mut writer = BufWriter::with_capacity(16 * 1024, write_sock);
        let mut read_buf = Vec::with_capacity(256);
        let mut write_buf = Vec::with_capacity(256);

        loop {
            if self.stop.load(Ordering::Relaxed) {
                break;
            }
            let req: CacheRequest = match read_frame_buf(&mut reader, &mut read_buf) {
                Ok(r) => r,
                Err(_) => break,
            };

            // Special streaming path for SUBSCRIBE
            if let CacheRequest::Subscribe { channels } = req {
                let queue = Arc::new((Mutex::new(VecDeque::new()), Condvar::new()));
                {
                    let mut st = self.state.write().unwrap();
                    for ch in &channels {
                        st.pubsub
                            .subscribers
                            .entry(ch.clone())
                            .or_default()
                            .push(Arc::clone(&queue));
                    }
                }
                // Send initial ACK
                write_frame_buf(
                    &mut writer,
                    &CacheResponse::Text(format!("Subscribed to: {}", channels.join(", "))),
                    &mut write_buf,
                )?;

                // Stream published messages to client
                let (lock, cvar) = &*queue;
                while !self.stop.load(Ordering::Relaxed) {
                    let msg = {
                        let mut guard = lock.lock().unwrap();
                        if guard.is_empty() {
                            let (mut g, _) = cvar
                                .wait_timeout(guard, Duration::from_millis(100))
                                .unwrap();
                            g.pop_front()
                        } else {
                            guard.pop_front()
                        }
                    };
                    if let Some(m) = msg
                        && write_frame_buf(&mut writer, &m, &mut write_buf).is_err()
                    {
                        break;
                    }
                }
                break;
            }

            let resp = self.execute(req);
            write_frame_noflush_buf(&mut writer, &resp, &mut write_buf)?;
            if reader.buffer().is_empty() {
                writer.flush().map_err(cache_err)?;
            }
        }
        let _ = writer.flush();
        Ok(())
    }
}
