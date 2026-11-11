use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::Duration;

use crate::error::Error;
use crate::exec::{Db, OpenOpts, SyncMode};

use super::client::send_rpc;
use super::cluster::{ConsistentHashRing, NodeMember, NodeRole, PubSubEvent, RaftCommand};
use super::protocol::{CacheRequest, CacheResponse, CacheValue, read_frame, write_frame};
use super::store::EngineState;
use super::{CacheConfig, cache_err, unix_ms};

/// A running or embeddable distributed cache node backed by `lin::Db`.
#[derive(Clone)]
pub struct CacheNode {
    pub(crate) cfg: Arc<CacheConfig>,
    pub(crate) state: Arc<Mutex<EngineState>>,
    pub(crate) stop: Arc<AtomicBool>,
    stop_signal: Arc<(Mutex<bool>, Condvar)>,
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
        Ok(Self {
            cfg: Arc::new(cfg),
            state: Arc::new(Mutex::new(state)),
            stop: Arc::new(AtomicBool::new(false)),
            stop_signal: Arc::new((Mutex::new(false), Condvar::new())),
        })
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
    }

    /// Join another peer node into the cluster (updates Consistent Hash Ring + Gossip table).
    pub fn meet(&self, peer_id: impl Into<String>, peer_addr: impl Into<String>) {
        let peer_id = peer_id.into();
        let peer_addr = peer_addr.into();
        if let Ok(mut st) = self.state.lock() {
            st.ring.add_node(&peer_id, &peer_addr);
            let term = st.raft.current_term;
            st.members
                .entry(peer_id.clone())
                .and_modify(|m| {
                    m.addr = peer_addr.clone();
                    m.alive = true;
                    m.last_seen_ms = unix_ms();
                })
                .or_insert(NodeMember {
                    node_id: peer_id,
                    addr: peer_addr,
                    heartbeat: 1,
                    last_seen_ms: unix_ms(),
                    alive: true,
                    role: NodeRole::Follower,
                    term,
                });
        }
    }

    /// Execute a `CacheRequest` directly against this node (with optional shard routing & Raft replication).
    pub fn execute(&self, req: CacheRequest) -> CacheResponse {
        self.execute_inner(req, true)
    }

    fn execute_inner(&self, req: CacheRequest, allow_routing: bool) -> CacheResponse {
        // Unwrap InternalShardExec so routed requests execute locally without re-routing
        let (req, allow_routing) = match req {
            CacheRequest::InternalShardExec { inner } => (*inner, false),
            other => (other, allow_routing),
        };

        // 1. Check Consistent Hash Ring routing if sharding is enabled and request targets a single key
        if allow_routing
            && self.cfg.sharding_enabled
            && let Some(key) = Self::extract_routing_key(&req)
        {
            let target_peer = {
                let st = self.state.lock().unwrap();
                st.ring.primary_for_key(key).and_then(|(owner_id, addr)| {
                    if owner_id != self.cfg.node_id
                        && st.members.get(&owner_id).is_some_and(|m| m.alive)
                    {
                        Some((owner_id, addr))
                    } else {
                        None
                    }
                })
            };
            if let Some((_owner_id, peer_addr)) = target_peer {
                let forwarded = CacheRequest::InternalShardExec {
                    inner: Box::new(req.clone()),
                };
                if let Ok(resp) = send_rpc(&peer_addr, &forwarded) {
                    return resp;
                }
                // Fallback to local execution if peer is unreachable
            }
        }

        // 2. Execute command
        match req {
            CacheRequest::Ping(msg) => {
                CacheResponse::Pong(msg.unwrap_or_else(|| self.cfg.node_id.clone()))
            }
            CacheRequest::Get { key } => {
                let mut st = self.state.lock().unwrap();
                if st.touch_or_remove_expired(&key)
                    && let Some(entry) = st.entries.get(&key)
                {
                    let val = match &entry.value {
                        CacheValue::String(b) => CacheResponse::Bulk(b.clone()),
                        CacheValue::Int(n) => CacheResponse::Bulk(n.to_string().into_bytes()),
                        other => CacheResponse::Text(other.as_str_lossy()),
                    };
                    st.hits += 1;
                    val
                } else {
                    st.misses += 1;
                    CacheResponse::Nil
                }
            }
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
                let mut removed_count = 0i64;
                {
                    let mut st = self.state.lock().unwrap();
                    for k in &keys {
                        if st.touch_or_remove_expired(k) {
                            removed_count += 1;
                        }
                    }
                }
                self.propose_and_apply_mutation(RaftCommand::Del { keys });
                CacheResponse::Int(removed_count)
            }
            CacheRequest::Exists { key } => {
                let mut st = self.state.lock().unwrap();
                let exists = st.touch_or_remove_expired(&key);
                CacheResponse::Int(i64::from(exists))
            }
            CacheRequest::IncrBy { key, delta } => {
                let (new_val, expires_at_ms) = {
                    let mut st = self.state.lock().unwrap();
                    let _ = st.touch_or_remove_expired(&key);
                    match st.entries.get(&key) {
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
                self.propose_and_apply_mutation(RaftCommand::Set {
                    key,
                    value: CacheValue::Int(new_val),
                    expires_at_ms,
                });
                CacheResponse::Int(new_val)
            }
            CacheRequest::Expire { key, ttl_ms } => {
                let updated = {
                    let mut st = self.state.lock().unwrap();
                    if !st.touch_or_remove_expired(&key) {
                        None
                    } else {
                        st.entries.get(&key).map(|e| e.value.clone())
                    }
                };
                if let Some(value) = updated {
                    let expires_at_ms = Some(unix_ms() + ttl_ms);
                    self.propose_and_apply_mutation(RaftCommand::Set {
                        key,
                        value,
                        expires_at_ms,
                    });
                    CacheResponse::Int(1)
                } else {
                    CacheResponse::Int(0)
                }
            }
            CacheRequest::Persist { key } => {
                let updated = {
                    let mut st = self.state.lock().unwrap();
                    if !st.touch_or_remove_expired(&key) {
                        None
                    } else {
                        st.entries
                            .get(&key)
                            .filter(|e| e.expires_at_ms.is_some())
                            .map(|e| e.value.clone())
                    }
                };
                if let Some(value) = updated {
                    self.propose_and_apply_mutation(RaftCommand::Set {
                        key,
                        value,
                        expires_at_ms: None,
                    });
                    CacheResponse::Int(1)
                } else {
                    CacheResponse::Int(0)
                }
            }
            CacheRequest::Ttl { key } => {
                let mut st = self.state.lock().unwrap();
                if !st.touch_or_remove_expired(&key) {
                    return CacheResponse::Int(-2);
                }
                match st.entries.get(&key).and_then(|e| e.expires_at_ms) {
                    Some(exp) => {
                        let now = unix_ms();
                        let rem = exp.saturating_sub(now) as i64;
                        CacheResponse::Int(rem)
                    }
                    None => CacheResponse::Int(-1),
                }
            }
            CacheRequest::Keys { prefix } => {
                let mut st = self.state.lock().unwrap();
                st.evict_expired_and_lru(self.cfg.max_entries, self.cfg.max_bytes);
                let pat = prefix.trim_end_matches('*');
                let mut matching: Vec<String> = st
                    .entries
                    .keys()
                    .filter(|k| pat.is_empty() || k.starts_with(pat))
                    .cloned()
                    .collect();
                matching.sort();
                CacheResponse::Array(
                    matching
                        .into_iter()
                        .map(|k| CacheResponse::Bulk(k.into_bytes()))
                        .collect(),
                )
            }
            CacheRequest::FlushAll => {
                self.propose_and_apply_mutation(RaftCommand::FlushAll);
                CacheResponse::Ok
            }

            // --- Hashes ---
            CacheRequest::HSet { key, field, value } => {
                let (map, is_new, exp) = {
                    let mut st = self.state.lock().unwrap();
                    let _ = st.touch_or_remove_expired(&key);
                    match st.entries.get(&key) {
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
                self.propose_and_apply_mutation(RaftCommand::Set {
                    key,
                    value: CacheValue::Hash(map),
                    expires_at_ms: exp,
                });
                CacheResponse::Int(i64::from(is_new))
            }
            CacheRequest::HGet { key, field } => {
                let mut st = self.state.lock().unwrap();
                if !st.touch_or_remove_expired(&key) {
                    st.misses += 1;
                    return CacheResponse::Nil;
                }
                match st.entries.get(&key).map(|e| &e.value) {
                    Some(CacheValue::Hash(m)) => match m.get(&field).cloned() {
                        Some(v) => {
                            st.hits += 1;
                            CacheResponse::Bulk(v)
                        }
                        None => {
                            st.misses += 1;
                            CacheResponse::Nil
                        }
                    },
                    Some(_) => CacheResponse::Error("WRONGTYPE Key is not a hash".into()),
                    None => CacheResponse::Nil,
                }
            }
            CacheRequest::HDel { key, fields } => {
                let res = {
                    let mut st = self.state.lock().unwrap();
                    if !st.touch_or_remove_expired(&key) {
                        return CacheResponse::Int(0);
                    }
                    match st.entries.get(&key) {
                        Some(entry) => match &entry.value {
                            CacheValue::Hash(m) => {
                                let mut m = m.clone();
                                let mut removed = 0i64;
                                for f in &fields {
                                    if m.remove(f).is_some() {
                                        removed += 1;
                                    }
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
                            self.propose_and_apply_mutation(RaftCommand::Del { keys: vec![key] });
                        } else if removed > 0 {
                            self.propose_and_apply_mutation(RaftCommand::Set {
                                key,
                                value: CacheValue::Hash(m),
                                expires_at_ms: exp,
                            });
                        }
                        CacheResponse::Int(removed)
                    }
                    None => CacheResponse::Error("WRONGTYPE Key is not a hash".into()),
                }
            }
            CacheRequest::HGetAll { key } => {
                let mut st = self.state.lock().unwrap();
                if !st.touch_or_remove_expired(&key) {
                    return CacheResponse::Map(BTreeMap::new());
                }
                match st.entries.get(&key).map(|e| &e.value) {
                    Some(CacheValue::Hash(m)) => {
                        let m = m.clone();
                        st.hits += 1;
                        CacheResponse::Map(m)
                    }
                    Some(_) => CacheResponse::Error("WRONGTYPE Key is not a hash".into()),
                    None => CacheResponse::Map(BTreeMap::new()),
                }
            }

            // --- Lists ---
            CacheRequest::LPush { key, values } => self.exec_list_push(key, values, true),
            CacheRequest::RPush { key, values } => self.exec_list_push(key, values, false),
            CacheRequest::LPop { key } => self.exec_list_pop(key, true),
            CacheRequest::RPop { key } => self.exec_list_pop(key, false),
            CacheRequest::LRange { key, start, stop } => {
                let mut st = self.state.lock().unwrap();
                if !st.touch_or_remove_expired(&key) {
                    return CacheResponse::Array(Vec::new());
                }
                match st.entries.get(&key).map(|e| &e.value) {
                    Some(CacheValue::List(list)) => {
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
                        st.hits += 1;
                        CacheResponse::Array(slice)
                    }
                    Some(_) => CacheResponse::Error("WRONGTYPE Key is not a list".into()),
                    None => CacheResponse::Array(Vec::new()),
                }
            }

            // --- Sets ---
            CacheRequest::SAdd { key, members } => {
                let (set, added, exp) = {
                    let mut st = self.state.lock().unwrap();
                    let _ = st.touch_or_remove_expired(&key);
                    match st.entries.get(&key) {
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
                self.propose_and_apply_mutation(RaftCommand::Set {
                    key,
                    value: CacheValue::Set(set),
                    expires_at_ms: exp,
                });
                CacheResponse::Int(added)
            }
            CacheRequest::SRem { key, members } => {
                let res = {
                    let mut st = self.state.lock().unwrap();
                    if !st.touch_or_remove_expired(&key) {
                        return CacheResponse::Int(0);
                    }
                    match st.entries.get(&key) {
                        Some(entry) => match &entry.value {
                            CacheValue::Set(s) => {
                                let mut s = s.clone();
                                let mut removed = 0i64;
                                for m in &members {
                                    if s.remove(m) {
                                        removed += 1;
                                    }
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
                            self.propose_and_apply_mutation(RaftCommand::Del { keys: vec![key] });
                        } else if removed > 0 {
                            self.propose_and_apply_mutation(RaftCommand::Set {
                                key,
                                value: CacheValue::Set(s),
                                expires_at_ms: exp,
                            });
                        }
                        CacheResponse::Int(removed)
                    }
                    None => CacheResponse::Error("WRONGTYPE Key is not a set".into()),
                }
            }
            CacheRequest::SMembers { key } => {
                let mut st = self.state.lock().unwrap();
                if !st.touch_or_remove_expired(&key) {
                    return CacheResponse::Array(Vec::new());
                }
                match st.entries.get(&key).map(|e| &e.value) {
                    Some(CacheValue::Set(s)) => {
                        let out = s
                            .iter()
                            .map(|m| CacheResponse::Bulk(m.as_bytes().to_vec()))
                            .collect();
                        st.hits += 1;
                        CacheResponse::Array(out)
                    }
                    Some(_) => CacheResponse::Error("WRONGTYPE Key is not a set".into()),
                    None => CacheResponse::Array(Vec::new()),
                }
            }
            CacheRequest::SIsMember { key, member } => {
                let mut st = self.state.lock().unwrap();
                if !st.touch_or_remove_expired(&key) {
                    return CacheResponse::Int(0);
                }
                match st.entries.get(&key).map(|e| &e.value) {
                    Some(CacheValue::Set(s)) => CacheResponse::Int(i64::from(s.contains(&member))),
                    Some(_) => CacheResponse::Error("WRONGTYPE Key is not a set".into()),
                    None => CacheResponse::Int(0),
                }
            }

            // --- Pub/Sub ---
            CacheRequest::Publish { channel, message } => {
                let (local_subs, event, peers, members_snap) = {
                    let mut st = self.state.lock().unwrap();
                    let eid = (crc32fast::hash(self.cfg.node_id.as_bytes()) as u64) << 32
                        | st.pubsub.next_event_id;
                    st.pubsub.next_event_id += 1;
                    st.pubsub.mark_seen(eid);
                    let ev = PubSubEvent {
                        id: eid,
                        channel: channel.clone(),
                        payload: message.clone(),
                        origin_node: self.cfg.node_id.clone(),
                    };
                    st.pubsub.recent_outbox.push_back(ev.clone());
                    while st.pubsub.recent_outbox.len() > 64 {
                        st.pubsub.recent_outbox.pop_front();
                    }
                    let count = st.publish_local(&channel, &message, &self.cfg.node_id);
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
                // Propagate Pub/Sub event immediately to alive cluster peers via GossipSync
                if !peers.is_empty() {
                    let sync_req = CacheRequest::GossipSync {
                        sender_id: self.cfg.node_id.clone(),
                        sender_addr: self.cfg.listen_addr.clone(),
                        members: members_snap,
                        pubsub_events: vec![event],
                    };
                    for p in peers {
                        let _ = send_rpc(&p, &sync_req);
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
                let res = {
                    let mut st = self.state.lock().unwrap();
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
                let st = self.state.lock().unwrap();
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
                    st.hits,
                    st.misses,
                    st.evictions,
                    st.expired_cleaned,
                    db_stats.r#gen,
                    db_stats.docs,
                    db_stats.facts,
                );
                CacheResponse::Text(info)
            }
            CacheRequest::ClusterNodes => {
                let st = self.state.lock().unwrap();
                let mut lines = Vec::new();
                for m in st.members.values() {
                    let self_tag = if m.node_id == self.cfg.node_id {
                        "myself,"
                    } else {
                        ""
                    };
                    let state_tag = if m.alive { "alive" } else { "fail" };
                    lines.push(format!(
                        "{} {} {}{},{} term={} hb={}",
                        m.node_id,
                        m.addr,
                        self_tag,
                        m.role.as_str(),
                        state_tag,
                        m.term,
                        m.heartbeat
                    ));
                }
                CacheResponse::Text(lines.join("\n"))
            }
            CacheRequest::ClusterMeet { node_id, addr } => {
                self.meet(node_id, addr.clone());
                // Immediately trigger a gossip handshake with the new peer
                let _ = self.gossip_once_with(&addr);
                CacheResponse::Ok
            }
            CacheRequest::ClusterKeySlot { key } => {
                let st = self.state.lock().unwrap();
                let hash = ConsistentHashRing::hash_key(&key);
                let replicas = st.ring.replicas_for_key(&key);
                let owners: Vec<String> = replicas
                    .into_iter()
                    .map(|(id, addr)| format!("{id}@{addr}"))
                    .collect();
                CacheResponse::Text(format!(
                    "key={key} ring_hash={hash} owners=[{}]",
                    owners.join(", ")
                ))
            }
            CacheRequest::RaftStatus => {
                let st = self.state.lock().unwrap();
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
                let st = self.state.lock().unwrap();
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
            CacheRequest::InternalShardExec { .. } => unreachable!(),
        }
    }

    fn extract_routing_key(req: &CacheRequest) -> Option<&str> {
        match req {
            CacheRequest::Get { key }
            | CacheRequest::Set { key, .. }
            | CacheRequest::Exists { key }
            | CacheRequest::IncrBy { key, .. }
            | CacheRequest::Expire { key, .. }
            | CacheRequest::Persist { key }
            | CacheRequest::Ttl { key }
            | CacheRequest::HSet { key, .. }
            | CacheRequest::HGet { key, .. }
            | CacheRequest::HDel { key, .. }
            | CacheRequest::HGetAll { key }
            | CacheRequest::LPush { key, .. }
            | CacheRequest::RPush { key, .. }
            | CacheRequest::LPop { key }
            | CacheRequest::RPop { key }
            | CacheRequest::LRange { key, .. }
            | CacheRequest::SAdd { key, .. }
            | CacheRequest::SRem { key, .. }
            | CacheRequest::SMembers { key }
            | CacheRequest::SIsMember { key, .. } => Some(key.as_str()),
            CacheRequest::Del { keys } if keys.len() == 1 => Some(keys[0].as_str()),
            _ => None,
        }
    }

    fn exec_list_push(&self, key: String, values: Vec<Vec<u8>>, left: bool) -> CacheResponse {
        let (list, len, exp) = {
            let mut st = self.state.lock().unwrap();
            let _ = st.touch_or_remove_expired(&key);
            match st.entries.get(&key) {
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
        self.propose_and_apply_mutation(RaftCommand::Set {
            key,
            value: CacheValue::List(list),
            expires_at_ms: exp,
        });
        CacheResponse::Int(len)
    }

    fn exec_list_pop(&self, key: String, left: bool) -> CacheResponse {
        let res = {
            let mut st = self.state.lock().unwrap();
            if !st.touch_or_remove_expired(&key) {
                return CacheResponse::Nil;
            }
            match st.entries.get(&key) {
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
                    self.propose_and_apply_mutation(RaftCommand::Del { keys: vec![key] });
                } else {
                    self.propose_and_apply_mutation(RaftCommand::Set {
                        key,
                        value: CacheValue::List(l),
                        expires_at_ms: exp,
                    });
                }
                CacheResponse::Bulk(popped)
            }
            Some((_, None, _)) => CacheResponse::Nil,
            None => CacheResponse::Error("WRONGTYPE Key is not a list".into()),
        }
    }

    /// Start background Gossip & TTL cleanup loop + serve TCP connections on `listener`.
    pub fn serve(&self, listener: TcpListener) -> Result<(), Error> {
        listener.set_nonblocking(true).map_err(cache_err)?;
        let bg_node = self.clone();
        let gossip_handle = thread::spawn(move || {
            bg_node.background_loop();
        });

        while !self.stop.load(Ordering::Relaxed) {
            match listener.accept() {
                Ok((stream, _peer)) => {
                    let _ = stream.set_nonblocking(false);
                    let node = self.clone();
                    thread::spawn(move || {
                        let _ = node.handle_connection(stream);
                    });
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(5));
                }
                Err(_) => {
                    if self.stop.load(Ordering::Relaxed) {
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
                let mut st = self.state.lock().unwrap();
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
                for dn in dead_nodes {
                    st.ring.remove_node(&dn);
                }
                st.members
                    .values()
                    .filter(|m| m.node_id != self.cfg.node_id)
                    .map(|m| (m.node_id.clone(), m.addr.clone()))
                    .collect()
            };

            for (peer_id, peer_addr) in peers {
                if self.gossip_once_with(&peer_addr).is_err() {
                    let mut st = self.state.lock().unwrap();
                    let now = unix_ms();
                    if let Some(m) = st.members.get_mut(&peer_id)
                        && now.saturating_sub(m.last_seen_ms) > self.cfg.node_timeout_ms
                    {
                        m.alive = false;
                        st.ring.remove_node(&peer_id);
                    }
                }
            }
        }
    }

    fn handle_connection(&self, mut stream: TcpStream) -> Result<(), Error> {
        let _ = stream.set_nodelay(true);
        loop {
            if self.stop.load(Ordering::Relaxed) {
                break;
            }
            let req: CacheRequest = match read_frame(&mut stream) {
                Ok(r) => r,
                Err(_) => break,
            };

            // Special streaming path for SUBSCRIBE
            if let CacheRequest::Subscribe { channels } = req {
                let queue = Arc::new((Mutex::new(VecDeque::new()), Condvar::new()));
                {
                    let mut st = self.state.lock().unwrap();
                    for ch in &channels {
                        st.pubsub
                            .subscribers
                            .entry(ch.clone())
                            .or_default()
                            .push(Arc::clone(&queue));
                    }
                }
                // Send initial ACK
                write_frame(
                    &mut stream,
                    &CacheResponse::Text(format!("Subscribed to: {}", channels.join(", "))),
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
                        && write_frame(&mut stream, &m).is_err()
                    {
                        break;
                    }
                }
                break;
            }

            let resp = self.execute(req);
            write_frame(&mut stream, &resp)?;
        }
        Ok(())
    }
}
