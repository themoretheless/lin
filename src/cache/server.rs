use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::io::{BufReader, BufWriter, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, RwLock, RwLockWriteGuard};
use std::thread;
use std::time::Duration;

use crate::error::Error;
use crate::exec::{Db, OpenOpts, SyncMode};

use super::client::{evict_peer_pool_addr, send_peer_rpc};
use super::cluster::{
    ConsistentHashRing, NodeMember, NodeRole, PubSubEvent, RaftCommand, RaftState,
};
use super::protocol::{
    CacheEntry, CacheRequest, CacheResponse, CacheValue, read_frame_buf, write_frame_buf,
    write_frame_noflush_buf,
};
use super::store::{
    CacheStore, ClusterMeta, DbState, NUM_SHARDS, PubSubHub, ReadLookup, ShardStripe, shard_idx,
};
use super::{CacheConfig, cache_err, unix_ms};

/// A running or embeddable distributed cache node backed by `lin::Db`.
#[derive(Clone)]
pub struct CacheNode {
    pub(crate) cfg: Arc<CacheConfig>,
    /// 64-shard keyspace store with intrusive O(1) Slab-LRU.
    pub(crate) store: Arc<CacheStore>,
    /// Underlying `lin::Db` state kept separate from keyspace shards.
    pub(crate) state: Arc<RwLock<DbState>>,
    /// Cluster membership & consistent hash ring.
    pub(crate) cluster: Arc<RwLock<ClusterMeta>>,
    /// Raft consensus state.
    pub(crate) raft: Arc<Mutex<RaftState>>,
    /// Pub/Sub channels & subscriber queues.
    pub(crate) pubsub: Arc<Mutex<PubSubHub>>,
    pub(crate) stop: Arc<AtomicBool>,
    stop_signal: Arc<(Mutex<bool>, Condvar)>,
    /// Serializes multi-node replicated mutations so peers receive Raft entries in index order.
    pub(crate) mutation_order: Arc<Mutex<()>>,
    /// Lock-free flag (`cluster.ring.nodes().len() > 1`) skipping routing & Raft RPCs on 1-node instances.
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
        let mut db_state = DbState::new(db);
        let store = CacheStore::new(&mut db_state)?;
        let cluster_meta = ClusterMeta::new(&cfg);
        let multi = cluster_meta.ring.nodes().len() > 1;

        let mut raft = RaftState::default();
        raft.leader_id = Some(cfg.node_id.clone());
        raft.leader_addr = Some(cfg.listen_addr.clone());

        Ok(Self {
            cfg: Arc::new(cfg),
            store: Arc::new(store),
            state: Arc::new(RwLock::new(db_state)),
            cluster: Arc::new(RwLock::new(cluster_meta)),
            raft: Arc::new(Mutex::new(raft)),
            pubsub: Arc::new(Mutex::new(PubSubHub::default())),
            stop: Arc::new(AtomicBool::new(false)),
            stop_signal: Arc::new((Mutex::new(false), Condvar::new())),
            mutation_order: Arc::new(Mutex::new(())),
            has_peers: Arc::new(AtomicBool::new(multi)),
            bound_addr: Arc::new(Mutex::new(None)),
        })
    }

    #[inline]
    pub(crate) fn sync_has_peers(&self, cl: &ClusterMeta) {
        self.has_peers
            .store(cl.ring.nodes().len() > 1, Ordering::Release);
    }

    #[inline]
    fn needs_raft_replication(&self) -> bool {
        self.cfg.raft_replication && self.has_peers.load(Ordering::Acquire)
    }

    #[inline]
    fn lock_db_for_mutation(&self) -> Option<RwLockWriteGuard<'_, DbState>> {
        if self.store.lazy_lin_sync {
            None
        } else {
            Some(self.state.write().unwrap())
        }
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
        let term = self
            .raft
            .lock()
            .map(|r| r.current_term)
            .unwrap_or(1);
        if let Ok(mut cl) = self.cluster.write() {
            cl.ring.add_node(&peer_id, &peer_addr);
            let now = unix_ms();
            if let Some(m) = cl.members.get_mut(&peer_id) {
                if m.addr != peer_addr {
                    m.addr = peer_addr;
                }
                m.alive = true;
                m.last_seen_ms = now;
            } else {
                cl.members.insert(
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
            self.sync_has_peers(&cl);
        }
    }

    /// Execute a `CacheRequest` directly against this node (with optional shard routing & Raft replication).
    pub fn execute(&self, req: CacheRequest) -> CacheResponse {
        self.execute_inner(req, true)
    }

    #[inline]
    fn remote_owner_addr(&self, cl: &ClusterMeta, key: &str) -> Option<String> {
        let (owner_id, addr) = cl.ring.primary_for_key_ref(key)?;
        if owner_id != self.cfg.node_id && cl.members.get(owner_id).is_some_and(|m| m.alive) {
            Some(addr.to_owned())
        } else {
            None
        }
    }

    /// Shard-local lock-shared read:
    /// - Single-node (`!has_peers`): skips `cluster` lock completely and takes **only**
    ///   `shards[shard_idx(key)].read()` with zero write-lock upgrades on live hits/misses!
    #[inline]
    fn with_routed_read(
        &self,
        key: &str,
        allow_routing: bool,
        make_forward_req: impl FnOnce() -> CacheRequest,
        on_missing: impl FnOnce(&ShardStripe) -> CacheResponse,
        on_hit: impl FnOnce(&ShardStripe, &CacheEntry) -> CacheResponse,
    ) -> CacheResponse {
        if allow_routing && self.cfg.sharding_enabled && self.has_peers.load(Ordering::Relaxed) {
            let forward_addr = {
                let cl = self.cluster.read().unwrap();
                self.remote_owner_addr(&cl, key)
            };
            if let Some(peer_addr) = forward_addr {
                let forwarded = CacheRequest::InternalShardExec {
                    inner: Box::new(make_forward_req()),
                };
                if let Ok(resp) = send_peer_rpc(&peer_addr, &forwarded) {
                    return resp;
                }
                // Peer unreachable: fall through to local shard read!
            }
        }

        let s_idx = shard_idx(key);
        let stripe = self.store.stripe(s_idx);
        {
            let sh = stripe.lock.read().unwrap();
            match self.store.read_live(&sh, key) {
                ReadLookup::Hit(entry) => return on_hit(stripe, entry),
                ReadLookup::Miss => return on_missing(stripe),
                ReadLookup::Expired => {}
            }
        }

        // Expired slow path: acquire write lock (following strict `db -> shard` order) and purge
        let mut db_guard = self.lock_db_for_mutation();
        let mut db_opt = db_guard.as_deref_mut();
        let mut sh = stripe.lock.write().unwrap();
        if let Some(slot_idx) = self
            .store
            .touch_or_remove_expired_in_shard(stripe, &mut sh, &mut db_opt, key)
        {
            on_hit(stripe, &sh.occupied(slot_idx).entry)
        } else {
            on_missing(stripe)
        }
    }

    fn execute_inner(&self, req: CacheRequest, allow_routing: bool) -> CacheResponse {
        // Unwrap InternalShardExec so routed requests execute locally without re-routing
        let (req, allow_routing) = match req {
            CacheRequest::InternalShardExec { inner } => (*inner, false),
            other => (other, allow_routing),
        };

        // 1. Fast path for read commands: single-shard `RwLock::read()` with zero write upgrades!
        match req {
            CacheRequest::Ping(msg) => {
                return CacheResponse::Pong(msg.unwrap_or_else(|| self.cfg.node_id.clone()));
            }
            CacheRequest::Get { key } => {
                return self.with_routed_read(
                    &key,
                    allow_routing,
                    || CacheRequest::Get { key: key.clone() },
                    |stripe| {
                        stripe.misses.fetch_add(1, Ordering::Relaxed);
                        CacheResponse::Nil
                    },
                    |stripe, entry| {
                        let val = match &entry.value {
                            CacheValue::String(b) => CacheResponse::Bulk(b.clone()),
                            CacheValue::Int(n) => CacheResponse::Bulk(n.to_string().into_bytes()),
                            other => CacheResponse::Text(other.as_str_lossy()),
                        };
                        stripe.hits.fetch_add(1, Ordering::Relaxed);
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
                    |stripe| {
                        stripe.misses.fetch_add(1, Ordering::Relaxed);
                        CacheResponse::Nil
                    },
                    |stripe, entry| match &entry.value {
                        CacheValue::Hash(m) => match m.get(&field) {
                            Some(v) => {
                                stripe.hits.fetch_add(1, Ordering::Relaxed);
                                CacheResponse::Bulk(v.clone())
                            }
                            None => {
                                stripe.misses.fetch_add(1, Ordering::Relaxed);
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
                    |stripe, entry| match &entry.value {
                        CacheValue::Hash(m) => {
                            let m = m.clone();
                            stripe.hits.fetch_add(1, Ordering::Relaxed);
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
                    |stripe, entry| match &entry.value {
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
                            stripe.hits.fetch_add(1, Ordering::Relaxed);
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
                    |stripe, entry| match &entry.value {
                        CacheValue::Set(s) => {
                            let out = s
                                .iter()
                                .map(|m| CacheResponse::Bulk(m.as_bytes().to_vec()))
                                .collect();
                            stripe.hits.fetch_add(1, Ordering::Relaxed);
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
                let cl = self.cluster.read().unwrap();
                self.remote_owner_addr(&cl, key)
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
                if self.needs_raft_replication() {
                    self.propose_and_apply_mutation(RaftCommand::Set {
                        key,
                        value: val,
                        expires_at_ms,
                    });
                } else {
                    self.put_single_shard(&key, val, expires_at_ms);
                }
                CacheResponse::Ok
            }
            CacheRequest::Del { keys } => {
                if self.needs_raft_replication() {
                    let order = self.mutation_order();
                    let removed_count = self.del_keys_local(&keys);
                    self.propose_raft_only_held(&order, RaftCommand::Del { keys });
                    CacheResponse::Int(removed_count)
                } else {
                    let removed_count = self.del_keys_local(&keys);
                    let _ = self.store.next_version();
                    CacheResponse::Int(removed_count)
                }
            }
            CacheRequest::IncrBy { key, delta } => self.exec_incr_by(key, delta),
            CacheRequest::Expire { key, ttl_ms } => self.exec_expire(key, ttl_ms),
            CacheRequest::Persist { key } => self.exec_persist(key),
            CacheRequest::Keys { prefix } => {
                let mut db_guard = self.lock_db_for_mutation();
                let mut db_opt = db_guard.as_deref_mut();
                self.store
                    .evict_global_overflow(&mut db_opt, self.cfg.max_entries, self.cfg.max_bytes);
                let pat = prefix.trim_end_matches('*');
                let mut matching: Vec<String> = Vec::new();
                for i in 0..NUM_SHARDS {
                    let stripe = self.store.stripe(i);
                    if stripe.len.load(Ordering::Relaxed) == 0 {
                        continue;
                    }
                    if stripe.ttl_len.load(Ordering::Relaxed) > 0 {
                        let mut sh = stripe.lock.write().unwrap();
                        self.store
                            .purge_expired_in_shard(stripe, &mut sh, &mut db_opt);
                        for k in sh.entries.keys() {
                            if pat.is_empty() || k.starts_with(pat) {
                                matching.push(k.to_string());
                            }
                        }
                    } else {
                        let sh = stripe.lock.read().unwrap();
                        for k in sh.entries.keys() {
                            if pat.is_empty() || k.starts_with(pat) {
                                matching.push(k.to_string());
                            }
                        }
                    }
                }
                matching.sort_unstable();
                CacheResponse::Array(
                    matching
                        .into_iter()
                        .map(|k| CacheResponse::Bulk(k.into_bytes()))
                        .collect(),
                )
            }
            CacheRequest::FlushAll => {
                if self.needs_raft_replication() {
                    self.propose_and_apply_mutation(RaftCommand::FlushAll);
                } else {
                    let mut db_guard = self.lock_db_for_mutation();
                    let mut db_opt = db_guard.as_deref_mut();
                    self.store.clear_all(&mut db_opt);
                    let _ = self.store.next_version();
                }
                CacheResponse::Ok
            }

            // --- Hashes ---
            CacheRequest::HSet { key, field, value } => self.exec_hset(key, field, value),
            CacheRequest::HDel { key, fields } => self.exec_hdel(key, fields),

            // --- Lists ---
            CacheRequest::LPush { key, values } => self.exec_list_push(key, values, true),
            CacheRequest::RPush { key, values } => self.exec_list_push(key, values, false),
            CacheRequest::LPop { key } => self.exec_list_pop(key, true),
            CacheRequest::RPop { key } => self.exec_list_pop(key, false),

            // --- Sets ---
            CacheRequest::SAdd { key, members } => self.exec_sadd(key, members),
            CacheRequest::SRem { key, members } => self.exec_srem(key, members),

            // --- Pub/Sub ---
            CacheRequest::Publish { channel, message } => {
                let (local_subs, event) = {
                    let mut ps = self.pubsub.lock().unwrap();
                    let eid = (crc32fast::hash(self.cfg.node_id.as_bytes()) as u64) << 32
                        | ps.next_event_id;
                    ps.next_event_id += 1;
                    ps.mark_seen(eid);
                    let count = ps.publish_local(&channel, &message, &self.cfg.node_id);
                    let ev = PubSubEvent {
                        id: eid,
                        channel,
                        payload: message,
                        origin_node: self.cfg.node_id.clone(),
                    };
                    ps.recent_outbox.push_back(ev.clone());
                    while ps.recent_outbox.len() > 64 {
                        ps.recent_outbox.pop_front();
                    }
                    (count, ev)
                };
                if self.has_peers.load(Ordering::Acquire) {
                    let (peers, members_snap) = {
                        let cl = self.cluster.read().unwrap();
                        let peers: Vec<String> = cl
                            .members
                            .values()
                            .filter(|m| m.node_id != self.cfg.node_id && m.alive)
                            .map(|m| m.addr.clone())
                            .collect();
                        let snap = if peers.is_empty() {
                            Vec::new()
                        } else {
                            cl.members.values().cloned().collect()
                        };
                        (peers, snap)
                    };
                    if !peers.is_empty() {
                        let sync_req = CacheRequest::GossipSync {
                            sender_id: self.cfg.node_id.clone(),
                            sender_addr: self.cfg.listen_addr.clone(),
                            members: members_snap,
                            pubsub_events: vec![event],
                        };
                        Self::broadcast_peer_rpc(&peers, &sync_req);
                    }
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
                if !is_write && !self.store.has_pending_lin_sync.load(Ordering::Acquire) {
                    let fast_out = {
                        let st = self.state.read().unwrap();
                        if !self.store.has_pending_lin_sync.load(Ordering::Acquire)
                            && !st.db.store.has_deferred_doc_ops()
                        {
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
                    self.store.flush_dirty_to_lin(&mut st);
                    let out = st.db.run(&query).map(|h| h.compact());
                    if out.is_ok() && is_write {
                        st.refresh_kv_prepared_if_needed();
                    }
                    out
                };
                match res {
                    Ok(out) => {
                        if is_write {
                            if self.needs_raft_replication() {
                                self.propose_raft_only(RaftCommand::LinMutation { query });
                            } else {
                                let _ = self.store.next_version();
                            }
                        }
                        CacheResponse::Text(out)
                    }
                    Err(e) => CacheResponse::Error(e.to_string()),
                }
            }

            // --- Cluster / Ring / Raft Introspection ---
            CacheRequest::ClusterInfo => {
                if self.store.has_pending_lin_sync.load(Ordering::Acquire) {
                    let mut st = self.state.write().unwrap();
                    self.store.flush_dirty_to_lin(&mut st);
                }
                let db_stats = self.state.read().unwrap().db.stats();
                let (role_str, raft_term, raft_leader, raft_log_len, raft_commit_index) = {
                    let mut raft = self.raft.lock().unwrap();
                    self.sync_raft_atomic_progress(&mut raft);
                    (
                        raft.role.as_str(),
                        raft.current_term,
                        raft.leader_id.clone(),
                        raft.log.len(),
                        raft.commit_index,
                    )
                };
                let cluster_nodes = self.cluster.read().unwrap().members.len();

                let mut hits = 0u64;
                let mut misses = 0u64;
                let mut evictions = 0u64;
                let mut expired_cleaned = 0u64;
                for i in 0..NUM_SHARDS {
                    let stripe = self.store.stripe(i);
                    hits += stripe.hits.load(Ordering::Relaxed);
                    misses += stripe.misses.load(Ordering::Relaxed);
                    let sh = stripe.lock.read().unwrap();
                    evictions += sh.evictions;
                    expired_cleaned += sh.expired_cleaned;
                }

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
                    role_str,
                    raft_term,
                    raft_leader.as_deref().unwrap_or("none"),
                    raft_log_len,
                    raft_commit_index,
                    cluster_nodes,
                    self.cfg.vnodes,
                    self.cfg.replication_factor,
                    self.store.total_entries.load(Ordering::Relaxed),
                    self.store.used_bytes.load(Ordering::Relaxed),
                    hits,
                    misses,
                    evictions,
                    expired_cleaned,
                    db_stats.r#gen,
                    db_stats.docs,
                    db_stats.facts,
                );
                CacheResponse::Text(info)
            }
            CacheRequest::ClusterNodes => {
                let cl = self.cluster.read().unwrap();
                let mut out = String::with_capacity(cl.members.len() * 64);
                for (i, m) in cl.members.values().enumerate() {
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
                let cl = self.cluster.read().unwrap();
                let hash = ConsistentHashRing::hash_key(&key);
                let replicas = cl.ring.replicas_for_key(&key);
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
                let mut raft = self.raft.lock().unwrap();
                self.sync_raft_atomic_progress(&mut raft);
                CacheResponse::Text(format!(
                    "role={} term={} leader={} voted_for={} log_len={} commit_index={} last_applied={}",
                    raft.role.as_str(),
                    raft.current_term,
                    raft.leader_id.as_deref().unwrap_or("none"),
                    raft.voted_for.as_deref().unwrap_or("none"),
                    raft.log.len(),
                    raft.commit_index,
                    raft.last_applied,
                ))
            }
            CacheRequest::RaftTriggerElection => {
                let won = self.run_leader_election();
                let mut raft = self.raft.lock().unwrap();
                self.sync_raft_atomic_progress(&mut raft);
                CacheResponse::Text(format!(
                    "election_won={} role={} term={} leader={}",
                    won,
                    raft.role.as_str(),
                    raft.current_term,
                    raft.leader_id.as_deref().unwrap_or("none")
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

    #[inline]
    fn put_single_shard(&self, key: &str, val: CacheValue, expires_at_ms: Option<u64>) {
        let mut db_guard = self.lock_db_for_mutation();
        let mut db_opt = db_guard.as_deref_mut();
        let s_idx = shard_idx(key);
        let stripe = self.store.stripe(s_idx);
        let ver = self.store.next_version();
        let overflow = {
            let mut sh = stripe.lock.write().unwrap();
            self.store.put_in_shard(
                stripe,
                &mut sh,
                &mut db_opt,
                key,
                val,
                expires_at_ms,
                ver,
                self.cfg.max_entries,
                self.cfg.max_bytes,
            )
        };
        if overflow {
            self.store
                .evict_global_overflow(&mut db_opt, self.cfg.max_entries, self.cfg.max_bytes);
        }
    }

    #[inline]
    fn del_keys_local(&self, keys: &[String]) -> i64 {
        let mut db_guard = self.lock_db_for_mutation();
        let mut db_opt = db_guard.as_deref_mut();
        let mut removed = 0i64;
        for k in keys {
            if self.store.remove_live_key(&mut db_opt, k) {
                removed += 1;
            }
        }
        removed
    }

    fn exec_incr_by(&self, key: String, delta: i64) -> CacheResponse {
        let replicate = self.needs_raft_replication();
        let order = replicate.then(|| self.mutation_order());

        let mut db_guard = self.lock_db_for_mutation();
        let mut db_opt = db_guard.as_deref_mut();
        let s_idx = shard_idx(&key);
        let stripe = self.store.stripe(s_idx);

        let overflow: bool;
        let (new_val, expires_at_ms) = {
            let mut sh = stripe.lock.write().unwrap();
            match self
                .store
                .purge_if_expired_in_shard(stripe, &mut sh, &mut db_opt, &key)
            {
                Some(slot_idx) => {
                    let cur_exp = sh.occupied(slot_idx).entry.expires_at_ms;
                    match &mut sh.occupied_mut(slot_idx).entry.value {
                        CacheValue::Int(n) => {
                            let next = n.wrapping_add(delta);
                            *n = next;
                            let ver = self.store.next_version();
                            overflow = self.store.finish_inplace_mutation(
                                stripe,
                                &mut sh,
                                &mut db_opt,
                                slot_idx,
                                0,
                                0,
                                ver,
                                self.cfg.max_entries,
                                self.cfg.max_bytes,
                            );
                            (next, cur_exp)
                        }
                        CacheValue::String(b) => {
                            let Ok(s) = std::str::from_utf8(b) else {
                                return CacheResponse::Error("ERR value is not an integer".into());
                            };
                            let Ok(parsed) = s.trim().parse::<i64>() else {
                                return CacheResponse::Error("ERR value is not an integer".into());
                            };
                            let next = parsed.wrapping_add(delta);
                            let ver = self.store.next_version();
                            overflow = self.store.put_in_shard(
                                stripe,
                                &mut sh,
                                &mut db_opt,
                                &key,
                                CacheValue::Int(next),
                                cur_exp,
                                ver,
                                self.cfg.max_entries,
                                self.cfg.max_bytes,
                            );
                            (next, cur_exp)
                        }
                        _ => {
                            return CacheResponse::Error(
                                "WRONGTYPE Operation against a key holding the wrong kind of value"
                                    .into(),
                            );
                        }
                    }
                }
                None => {
                    let ver = self.store.next_version();
                    overflow = self.store.put_in_shard(
                        stripe,
                        &mut sh,
                        &mut db_opt,
                        &key,
                        CacheValue::Int(delta),
                        None,
                        ver,
                        self.cfg.max_entries,
                        self.cfg.max_bytes,
                    );
                    (delta, None)
                }
            }
        };

        if overflow {
            self.store
                .evict_global_overflow(&mut db_opt, self.cfg.max_entries, self.cfg.max_bytes);
        }
        drop(db_guard);

        if let Some(order_guard) = order {
            self.propose_raft_only_held(
                &order_guard,
                RaftCommand::Set {
                    key,
                    value: CacheValue::Int(new_val),
                    expires_at_ms,
                },
            );
        }
        CacheResponse::Int(new_val)
    }

    fn exec_expire(&self, key: String, ttl_ms: u64) -> CacheResponse {
        let replicate = self.needs_raft_replication();
        let order = replicate.then(|| self.mutation_order());

        let mut db_guard = self.lock_db_for_mutation();
        let mut db_opt = db_guard.as_deref_mut();
        let s_idx = shard_idx(&key);
        let stripe = self.store.stripe(s_idx);
        let new_exp = Some(unix_ms() + ttl_ms);

        let replicated_val = {
            let mut sh = stripe.lock.write().unwrap();
            let Some(slot_idx) = self
                .store
                .purge_if_expired_in_shard(stripe, &mut sh, &mut db_opt, &key)
            else {
                return CacheResponse::Int(0);
            };
            let key_arc = Arc::clone(&sh.occupied(slot_idx).key);
            let old_exp = sh.occupied(slot_idx).entry.expires_at_ms;
            if old_exp != new_exp {
                if let Some(pe) = old_exp {
                    sh.ttl_index.remove(&(pe, Arc::clone(&key_arc)));
                }
                if let Some(ne) = new_exp {
                    sh.ttl_index.insert((ne, Arc::clone(&key_arc)));
                }
                stripe.ttl_len.store(sh.ttl_index.len(), Ordering::Relaxed);
                sh.occupied_mut(slot_idx).entry.expires_at_ms = new_exp;
            }
            let ver = self.store.next_version();
            let _ = self.store.finish_inplace_mutation(
                stripe,
                &mut sh,
                &mut db_opt,
                slot_idx,
                0,
                0,
                ver,
                self.cfg.max_entries,
                self.cfg.max_bytes,
            );
            order
                .is_some()
                .then(|| sh.occupied(slot_idx).entry.value.clone())
        };
        drop(db_guard);

        if let (Some(order_guard), Some(value)) = (order, replicated_val) {
            self.propose_raft_only_held(
                &order_guard,
                RaftCommand::Set {
                    key,
                    value,
                    expires_at_ms: new_exp,
                },
            );
        }
        CacheResponse::Int(1)
    }

    fn exec_persist(&self, key: String) -> CacheResponse {
        let replicate = self.needs_raft_replication();
        let order = replicate.then(|| self.mutation_order());

        let mut db_guard = self.lock_db_for_mutation();
        let mut db_opt = db_guard.as_deref_mut();
        let s_idx = shard_idx(&key);
        let stripe = self.store.stripe(s_idx);

        let replicated_val = {
            let mut sh = stripe.lock.write().unwrap();
            let Some(slot_idx) = self
                .store
                .purge_if_expired_in_shard(stripe, &mut sh, &mut db_opt, &key)
            else {
                return CacheResponse::Int(0);
            };
            let Some(old_exp) = sh.occupied(slot_idx).entry.expires_at_ms else {
                let occ = sh.occupied(slot_idx);
                if !occ.referenced.load(Ordering::Relaxed) {
                    let seq = self.store.next_seq();
                    occ.access_seq.store(seq, Ordering::Relaxed);
                    occ.referenced.store(true, Ordering::Relaxed);
                }
                return CacheResponse::Int(0);
            };
            let key_arc = Arc::clone(&sh.occupied(slot_idx).key);
            sh.ttl_index.remove(&(old_exp, key_arc));
            stripe.ttl_len.store(sh.ttl_index.len(), Ordering::Relaxed);
            sh.occupied_mut(slot_idx).entry.expires_at_ms = None;

            let ver = self.store.next_version();
            let _ = self.store.finish_inplace_mutation(
                stripe,
                &mut sh,
                &mut db_opt,
                slot_idx,
                0,
                0,
                ver,
                self.cfg.max_entries,
                self.cfg.max_bytes,
            );
            order
                .is_some()
                .then(|| sh.occupied(slot_idx).entry.value.clone())
        };
        drop(db_guard);

        if let (Some(order_guard), Some(value)) = (order, replicated_val) {
            self.propose_raft_only_held(
                &order_guard,
                RaftCommand::Set {
                    key,
                    value,
                    expires_at_ms: None,
                },
            );
        }
        CacheResponse::Int(1)
    }

    fn exec_hset(&self, key: String, field: String, value: Vec<u8>) -> CacheResponse {
        let replicate = self.needs_raft_replication();
        let order = replicate.then(|| self.mutation_order());

        let mut db_guard = self.lock_db_for_mutation();
        let mut db_opt = db_guard.as_deref_mut();
        let s_idx = shard_idx(&key);
        let stripe = self.store.stripe(s_idx);

        let overflow: bool;
        let (is_new, replicated_cmd) = {
            let mut sh = stripe.lock.write().unwrap();
            match self
                .store
                .purge_if_expired_in_shard(stripe, &mut sh, &mut db_opt, &key)
            {
                Some(slot_idx) => {
                    let exp = sh.occupied(slot_idx).entry.expires_at_ms;
                    let (is_new, added, removed) = match &mut sh.occupied_mut(slot_idx).entry.value {
                        CacheValue::Hash(m) => {
                            let new_v_len = value.len();
                            let field_len = field.len();
                            match m.insert(field, value) {
                                Some(old_v) => (false, new_v_len, old_v.len()),
                                None => (true, field_len + new_v_len + 16, 0),
                            }
                        }
                        _ => return CacheResponse::Error("WRONGTYPE Key is not a hash".into()),
                    };
                    let ver = self.store.next_version();
                    overflow = self.store.finish_inplace_mutation(
                        stripe,
                        &mut sh,
                        &mut db_opt,
                        slot_idx,
                        added,
                        removed,
                        ver,
                        self.cfg.max_entries,
                        self.cfg.max_bytes,
                    );
                    let rep = order.is_some().then(|| RaftCommand::Set {
                        key: key.clone(),
                        value: sh.occupied(slot_idx).entry.value.clone(),
                        expires_at_ms: exp,
                    });
                    (is_new, rep)
                }
                None => {
                    let mut m = BTreeMap::new();
                    m.insert(field, value);
                    let val = CacheValue::Hash(m);
                    let rep = order.is_some().then(|| RaftCommand::Set {
                        key: key.clone(),
                        value: val.clone(),
                        expires_at_ms: None,
                    });
                    let ver = self.store.next_version();
                    overflow = self.store.put_in_shard(
                        stripe,
                        &mut sh,
                        &mut db_opt,
                        &key,
                        val,
                        None,
                        ver,
                        self.cfg.max_entries,
                        self.cfg.max_bytes,
                    );
                    (true, rep)
                }
            }
        };

        if overflow {
            self.store
                .evict_global_overflow(&mut db_opt, self.cfg.max_entries, self.cfg.max_bytes);
        }
        drop(db_guard);

        if let (Some(order_guard), Some(cmd)) = (order, replicated_cmd) {
            self.propose_raft_only_held(&order_guard, cmd);
        }
        CacheResponse::Int(i64::from(is_new))
    }

    fn exec_hdel(&self, key: String, fields: Vec<String>) -> CacheResponse {
        let replicate = self.needs_raft_replication();
        let order = replicate.then(|| self.mutation_order());

        let mut db_guard = self.lock_db_for_mutation();
        let mut db_opt = db_guard.as_deref_mut();
        let s_idx = shard_idx(&key);
        let stripe = self.store.stripe(s_idx);

        let (removed_count, replicated_cmd) = {
            let mut sh = stripe.lock.write().unwrap();
            let Some(slot_idx) = self
                .store
                .purge_if_expired_in_shard(stripe, &mut sh, &mut db_opt, &key)
            else {
                return CacheResponse::Int(0);
            };
            let exp = sh.occupied(slot_idx).entry.expires_at_ms;
            let (removed, removed_bytes, became_empty) =
                match &mut sh.occupied_mut(slot_idx).entry.value {
                    CacheValue::Hash(m) => {
                        let mut removed = 0i64;
                        let mut rem_bytes = 0usize;
                        for f in &fields {
                            if let Some(old_v) = m.remove(f) {
                                removed += 1;
                                rem_bytes += f.len() + old_v.len() + 16;
                            }
                        }
                        (removed, rem_bytes, m.is_empty())
                    }
                    _ => return CacheResponse::Error("WRONGTYPE Key is not a hash".into()),
                };

            if became_empty {
                self.store
                    .remove_slot_in_shard(stripe, &mut sh, &mut db_opt, slot_idx);
                let _ = self.store.next_version();
                let rep = order.is_some().then(|| RaftCommand::Del {
                    keys: vec![key.clone()],
                });
                (removed, rep)
            } else if removed > 0 {
                let ver = self.store.next_version();
                let _ = self.store.finish_inplace_mutation(
                    stripe,
                    &mut sh,
                    &mut db_opt,
                    slot_idx,
                    0,
                    removed_bytes,
                    ver,
                    self.cfg.max_entries,
                    self.cfg.max_bytes,
                );
                let rep = order.is_some().then(|| RaftCommand::Set {
                    key: key.clone(),
                    value: sh.occupied(slot_idx).entry.value.clone(),
                    expires_at_ms: exp,
                });
                (removed, rep)
            } else {
                let occ = sh.occupied(slot_idx);
                if !occ.referenced.load(Ordering::Relaxed) {
                    let seq = self.store.next_seq();
                    occ.access_seq.store(seq, Ordering::Relaxed);
                    occ.referenced.store(true, Ordering::Relaxed);
                }
                (0, None)
            }
        };
        drop(db_guard);

        if let (Some(order_guard), Some(cmd)) = (order, replicated_cmd) {
            self.propose_raft_only_held(&order_guard, cmd);
        }
        CacheResponse::Int(removed_count)
    }

    fn exec_list_push(&self, key: String, values: Vec<Vec<u8>>, left: bool) -> CacheResponse {
        let replicate = self.needs_raft_replication();
        let order = replicate.then(|| self.mutation_order());

        let mut db_guard = self.lock_db_for_mutation();
        let mut db_opt = db_guard.as_deref_mut();
        let s_idx = shard_idx(&key);
        let stripe = self.store.stripe(s_idx);

        let overflow: bool;
        let (len, replicated_cmd) = {
            let mut sh = stripe.lock.write().unwrap();
            match self
                .store
                .purge_if_expired_in_shard(stripe, &mut sh, &mut db_opt, &key)
            {
                Some(slot_idx) => {
                    let exp = sh.occupied(slot_idx).entry.expires_at_ms;
                    let (len, added_bytes) = match &mut sh.occupied_mut(slot_idx).entry.value {
                        CacheValue::List(l) => {
                            let mut added = 0usize;
                            for v in values {
                                added += v.len() + 8;
                                if left {
                                    l.push_front(v);
                                } else {
                                    l.push_back(v);
                                }
                            }
                            (l.len() as i64, added)
                        }
                        _ => return CacheResponse::Error("WRONGTYPE Key is not a list".into()),
                    };
                    let ver = self.store.next_version();
                    overflow = self.store.finish_inplace_mutation(
                        stripe,
                        &mut sh,
                        &mut db_opt,
                        slot_idx,
                        added_bytes,
                        0,
                        ver,
                        self.cfg.max_entries,
                        self.cfg.max_bytes,
                    );
                    let rep = order.is_some().then(|| RaftCommand::Set {
                        key: key.clone(),
                        value: sh.occupied(slot_idx).entry.value.clone(),
                        expires_at_ms: exp,
                    });
                    (len, rep)
                }
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
                    let val = CacheValue::List(l);
                    let rep = order.is_some().then(|| RaftCommand::Set {
                        key: key.clone(),
                        value: val.clone(),
                        expires_at_ms: None,
                    });
                    let ver = self.store.next_version();
                    overflow = self.store.put_in_shard(
                        stripe,
                        &mut sh,
                        &mut db_opt,
                        &key,
                        val,
                        None,
                        ver,
                        self.cfg.max_entries,
                        self.cfg.max_bytes,
                    );
                    (len, rep)
                }
            }
        };

        if overflow {
            self.store
                .evict_global_overflow(&mut db_opt, self.cfg.max_entries, self.cfg.max_bytes);
        }
        drop(db_guard);

        if let (Some(order_guard), Some(cmd)) = (order, replicated_cmd) {
            self.propose_raft_only_held(&order_guard, cmd);
        }
        CacheResponse::Int(len)
    }

    fn exec_list_pop(&self, key: String, left: bool) -> CacheResponse {
        let replicate = self.needs_raft_replication();
        let order = replicate.then(|| self.mutation_order());

        let mut db_guard = self.lock_db_for_mutation();
        let mut db_opt = db_guard.as_deref_mut();
        let s_idx = shard_idx(&key);
        let stripe = self.store.stripe(s_idx);

        let (popped_opt, replicated_cmd) = {
            let mut sh = stripe.lock.write().unwrap();
            let Some(slot_idx) = self
                .store
                .purge_if_expired_in_shard(stripe, &mut sh, &mut db_opt, &key)
            else {
                return CacheResponse::Nil;
            };
            let exp = sh.occupied(slot_idx).entry.expires_at_ms;
            let (popped, became_empty) = match &mut sh.occupied_mut(slot_idx).entry.value {
                CacheValue::List(l) => {
                    let item = if left { l.pop_front() } else { l.pop_back() };
                    (item, l.is_empty())
                }
                _ => return CacheResponse::Error("WRONGTYPE Key is not a list".into()),
            };
            let Some(item) = popped else {
                return CacheResponse::Nil;
            };
            if became_empty {
                self.store
                    .remove_slot_in_shard(stripe, &mut sh, &mut db_opt, slot_idx);
                let _ = self.store.next_version();
                let rep = order.is_some().then(|| RaftCommand::Del {
                    keys: vec![key.clone()],
                });
                (item, rep)
            } else {
                let removed_bytes = item.len() + 8;
                let ver = self.store.next_version();
                let _ = self.store.finish_inplace_mutation(
                    stripe,
                    &mut sh,
                    &mut db_opt,
                    slot_idx,
                    0,
                    removed_bytes,
                    ver,
                    self.cfg.max_entries,
                    self.cfg.max_bytes,
                );
                let rep = order.is_some().then(|| RaftCommand::Set {
                    key: key.clone(),
                    value: sh.occupied(slot_idx).entry.value.clone(),
                    expires_at_ms: exp,
                });
                (item, rep)
            }
        };
        drop(db_guard);

        if let (Some(order_guard), Some(cmd)) = (order, replicated_cmd) {
            self.propose_raft_only_held(&order_guard, cmd);
        }
        CacheResponse::Bulk(popped_opt)
    }

    fn exec_sadd(&self, key: String, members: Vec<String>) -> CacheResponse {
        let replicate = self.needs_raft_replication();
        let order = replicate.then(|| self.mutation_order());

        let mut db_guard = self.lock_db_for_mutation();
        let mut db_opt = db_guard.as_deref_mut();
        let s_idx = shard_idx(&key);
        let stripe = self.store.stripe(s_idx);

        let overflow: bool;
        let (added_count, replicated_cmd) = {
            let mut sh = stripe.lock.write().unwrap();
            match self
                .store
                .purge_if_expired_in_shard(stripe, &mut sh, &mut db_opt, &key)
            {
                Some(slot_idx) => {
                    let exp = sh.occupied(slot_idx).entry.expires_at_ms;
                    let (added, added_bytes) = match &mut sh.occupied_mut(slot_idx).entry.value {
                        CacheValue::Set(s) => {
                            let mut added = 0i64;
                            let mut add_bytes = 0usize;
                            for m in members {
                                let m_len = m.len();
                                if s.insert(m) {
                                    added += 1;
                                    add_bytes += m_len + 8;
                                }
                            }
                            (added, add_bytes)
                        }
                        _ => return CacheResponse::Error("WRONGTYPE Key is not a set".into()),
                    };
                    let ver = self.store.next_version();
                    overflow = self.store.finish_inplace_mutation(
                        stripe,
                        &mut sh,
                        &mut db_opt,
                        slot_idx,
                        added_bytes,
                        0,
                        ver,
                        self.cfg.max_entries,
                        self.cfg.max_bytes,
                    );
                    let rep = order.is_some().then(|| RaftCommand::Set {
                        key: key.clone(),
                        value: sh.occupied(slot_idx).entry.value.clone(),
                        expires_at_ms: exp,
                    });
                    (added, rep)
                }
                None => {
                    let mut s = BTreeSet::new();
                    let mut added = 0i64;
                    for m in members {
                        if s.insert(m) {
                            added += 1;
                        }
                    }
                    let val = CacheValue::Set(s);
                    let rep = order.is_some().then(|| RaftCommand::Set {
                        key: key.clone(),
                        value: val.clone(),
                        expires_at_ms: None,
                    });
                    let ver = self.store.next_version();
                    overflow = self.store.put_in_shard(
                        stripe,
                        &mut sh,
                        &mut db_opt,
                        &key,
                        val,
                        None,
                        ver,
                        self.cfg.max_entries,
                        self.cfg.max_bytes,
                    );
                    (added, rep)
                }
            }
        };

        if overflow {
            self.store
                .evict_global_overflow(&mut db_opt, self.cfg.max_entries, self.cfg.max_bytes);
        }
        drop(db_guard);

        if let (Some(order_guard), Some(cmd)) = (order, replicated_cmd) {
            self.propose_raft_only_held(&order_guard, cmd);
        }
        CacheResponse::Int(added_count)
    }

    fn exec_srem(&self, key: String, members: Vec<String>) -> CacheResponse {
        let replicate = self.needs_raft_replication();
        let order = replicate.then(|| self.mutation_order());

        let mut db_guard = self.lock_db_for_mutation();
        let mut db_opt = db_guard.as_deref_mut();
        let s_idx = shard_idx(&key);
        let stripe = self.store.stripe(s_idx);

        let (removed_count, replicated_cmd) = {
            let mut sh = stripe.lock.write().unwrap();
            let Some(slot_idx) = self
                .store
                .purge_if_expired_in_shard(stripe, &mut sh, &mut db_opt, &key)
            else {
                return CacheResponse::Int(0);
            };
            let exp = sh.occupied(slot_idx).entry.expires_at_ms;
            let (removed, removed_bytes, became_empty) =
                match &mut sh.occupied_mut(slot_idx).entry.value {
                    CacheValue::Set(s) => {
                        let mut removed = 0i64;
                        let mut rem_bytes = 0usize;
                        for m in &members {
                            if s.remove(m) {
                                removed += 1;
                                rem_bytes += m.len() + 8;
                            }
                        }
                        (removed, rem_bytes, s.is_empty())
                    }
                    _ => return CacheResponse::Error("WRONGTYPE Key is not a set".into()),
                };

            if became_empty {
                self.store
                    .remove_slot_in_shard(stripe, &mut sh, &mut db_opt, slot_idx);
                let _ = self.store.next_version();
                let rep = order.is_some().then(|| RaftCommand::Del {
                    keys: vec![key.clone()],
                });
                (removed, rep)
            } else if removed > 0 {
                let ver = self.store.next_version();
                let _ = self.store.finish_inplace_mutation(
                    stripe,
                    &mut sh,
                    &mut db_opt,
                    slot_idx,
                    0,
                    removed_bytes,
                    ver,
                    self.cfg.max_entries,
                    self.cfg.max_bytes,
                );
                let rep = order.is_some().then(|| RaftCommand::Set {
                    key: key.clone(),
                    value: sh.occupied(slot_idx).entry.value.clone(),
                    expires_at_ms: exp,
                });
                (removed, rep)
            } else {
                let occ = sh.occupied(slot_idx);
                if !occ.referenced.load(Ordering::Relaxed) {
                    let seq = self.store.next_seq();
                    occ.access_seq.store(seq, Ordering::Relaxed);
                    occ.referenced.store(true, Ordering::Relaxed);
                }
                (0, None)
            }
        };
        drop(db_guard);

        if let (Some(order_guard), Some(cmd)) = (order, replicated_cmd) {
            self.propose_raft_only_held(&order_guard, cmd);
        }
        CacheResponse::Int(removed_count)
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

            // 1. Lightweight shard-striped TTL & overflow cleanup (skips shards without TTL keys!)
            {
                let any_ttl = (0..NUM_SHARDS)
                    .any(|i| self.store.stripe(i).ttl_len.load(Ordering::Relaxed) > 0);
                let over_budget = self
                    .store
                    .is_over_budget(self.cfg.max_entries, self.cfg.max_bytes);
                if any_ttl || over_budget {
                    let mut db_guard = self.lock_db_for_mutation();
                    let mut db_opt = db_guard.as_deref_mut();
                    if any_ttl {
                        for i in 0..NUM_SHARDS {
                            let stripe = self.store.stripe(i);
                            if stripe.ttl_len.load(Ordering::Relaxed) > 0 {
                                let mut sh = stripe.lock.write().unwrap();
                                self.store
                                    .purge_expired_in_shard(stripe, &mut sh, &mut db_opt);
                            }
                        }
                    }
                    if over_budget {
                        self.store.evict_global_overflow(
                            &mut db_opt,
                            self.cfg.max_entries,
                            self.cfg.max_bytes,
                        );
                    }
                }
            }

            // 2. Cluster membership liveness & Gossip rounds
            let peers: Vec<(String, String)> = {
                let mut cl = self.cluster.write().unwrap();
                let now = unix_ms();
                let timeout = self.cfg.node_timeout_ms;
                let mut dead_nodes = Vec::new();
                for m in cl.members.values_mut() {
                    if m.node_id != self.cfg.node_id && now.saturating_sub(m.last_seen_ms) > timeout
                    {
                        m.alive = false;
                        dead_nodes.push(m.node_id.clone());
                    }
                }
                if !dead_nodes.is_empty() {
                    for dn in dead_nodes {
                        cl.ring.remove_node(&dn);
                    }
                    self.sync_has_peers(&cl);
                }
                cl.members
                    .values()
                    .filter(|m| m.node_id != self.cfg.node_id)
                    .map(|m| (m.node_id.clone(), m.addr.clone()))
                    .collect()
            };

            for (peer_id, peer_addr) in peers {
                if self.gossip_once_with(&peer_addr).is_err() {
                    let mut cl = self.cluster.write().unwrap();
                    let now = unix_ms();
                    if let Some(m) = cl.members.get_mut(&peer_id)
                        && now.saturating_sub(m.last_seen_ms) > self.cfg.node_timeout_ms
                    {
                        m.alive = false;
                        cl.ring.remove_node(&peer_id);
                        self.sync_has_peers(&cl);
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
                    let mut ps = self.pubsub.lock().unwrap();
                    for ch in &channels {
                        ps.subscribers
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
