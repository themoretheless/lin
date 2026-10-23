//! Distributed Cache (`cacha`) built natively on top of `lin::Db`.
//!
//! Combines:
//! 1. **In-Memory Data Structures + LRU Eviction + TTL** (Strings, Hashes, Lists, Sets)
//!    with durable persistence into `lin::Db` (`col cache_kv`) and direct Lin query execution (`LIN <query>`).
//! 2. **Consistent Hashing Ring** with virtual nodes (`CRC32`) for automatic key sharding and transparent routing across cluster nodes.
//! 3. **Gossip Protocol** for peer discovery, membership heartbeats, failure detection, and cluster-wide **Pub/Sub** (`PUBLISH` / `SUBSCRIBE`).
//! 4. **Raft Consensus & WAL Replication** (Leader Election, Term, Log Replication, Commit Index, plus `lin` WAL frame shipping).
//! 5. **Custom Binary TCP Protocol** (`LINC\x01` + `u32` LE length + MessagePack payload) and interactive CLI client (`lin cache cli`).

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream, ToSocketAddrs};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};

use crate::error::Error;
use crate::exec::{Db, OpenOpts, SyncMode};

/// Protocol magic for `lin::cache` binary framing (`LINC` v1).
pub const CACHE_MAGIC: [u8; 4] = *b"LINC";
/// Protocol version byte.
pub const CACHE_VERSION: u8 = 1;
/// Max frame size (64 MiB).
pub const MAX_FRAME_LEN: u32 = 64 * 1024 * 1024;
/// Default number of virtual nodes per physical node on the consistent hash ring.
pub const DEFAULT_VNODES: usize = 64;

fn cache_err(msg: impl std::fmt::Display) -> Error {
    Error::runtime(format!("cache: {msg}"))
}

fn unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ============================================================================
// 1. Wire Protocol & Data Types (Custom Binary TCP Protocol)
// ============================================================================

/// Value stored in the distributed cache.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CacheValue {
    String(Vec<u8>),
    Int(i64),
    Hash(BTreeMap<String, Vec<u8>>),
    List(VecDeque<Vec<u8>>),
    Set(BTreeSet<String>),
}

impl CacheValue {
    pub fn type_name(&self) -> &'static str {
        match self {
            CacheValue::String(_) | CacheValue::Int(_) => "string",
            CacheValue::Hash(_) => "hash",
            CacheValue::List(_) => "list",
            CacheValue::Set(_) => "set",
        }
    }

    pub fn estimated_bytes(&self) -> usize {
        match self {
            CacheValue::String(b) => 24 + b.len(),
            CacheValue::Int(_) => 32,
            CacheValue::Hash(m) => {
                32 + m.iter().map(|(k, v)| k.len() + v.len() + 16).sum::<usize>()
            }
            CacheValue::List(l) => 32 + l.iter().map(|v| v.len() + 8).sum::<usize>(),
            CacheValue::Set(s) => 32 + s.iter().map(|v| v.len() + 8).sum::<usize>(),
        }
    }

    pub fn as_str_lossy(&self) -> String {
        match self {
            CacheValue::String(b) => String::from_utf8_lossy(b).into_owned(),
            CacheValue::Int(n) => n.to_string(),
            CacheValue::Hash(m) => serde_json::to_string(
                &m.iter()
                    .map(|(k, v)| (k.clone(), String::from_utf8_lossy(v).into_owned()))
                    .collect::<BTreeMap<_, _>>(),
            )
            .unwrap_or_default(),
            CacheValue::List(l) => serde_json::to_string(
                &l.iter()
                    .map(|v| String::from_utf8_lossy(v).into_owned())
                    .collect::<Vec<_>>(),
            )
            .unwrap_or_default(),
            CacheValue::Set(s) => serde_json::to_string(s).unwrap_or_default(),
        }
    }
}

/// Entry in the cache with TTL and LRU metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheEntry {
    pub value: CacheValue,
    /// Optional absolute expiration timestamp in Unix ms.
    pub expires_at_ms: Option<u64>,
    /// Monotonic counter or timestamp of last access for LRU.
    pub last_access_seq: u64,
    /// Generation/version in the cluster.
    pub version: u64,
}

impl CacheEntry {
    pub fn is_expired(&self, now: u64) -> bool {
        self.expires_at_ms.is_some_and(|exp| now >= exp)
    }
}

/// Client and inter-node Commands over the custom binary protocol.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CacheRequest {
    // --- Basic & String Operations ---
    Ping(Option<String>),
    Get {
        key: String,
    },
    Set {
        key: String,
        value: Vec<u8>,
        ttl_ms: Option<u64>,
    },
    Del {
        keys: Vec<String>,
    },
    Exists {
        key: String,
    },
    IncrBy {
        key: String,
        delta: i64,
    },
    Expire {
        key: String,
        ttl_ms: u64,
    },
    Persist {
        key: String,
    },
    Ttl {
        key: String,
    },
    Keys {
        prefix: String,
    },
    FlushAll,

    // --- Hash Operations ---
    HSet {
        key: String,
        field: String,
        value: Vec<u8>,
    },
    HGet {
        key: String,
        field: String,
    },
    HDel {
        key: String,
        fields: Vec<String>,
    },
    HGetAll {
        key: String,
    },

    // --- List Operations ---
    LPush {
        key: String,
        values: Vec<Vec<u8>>,
    },
    RPush {
        key: String,
        values: Vec<Vec<u8>>,
    },
    LPop {
        key: String,
    },
    RPop {
        key: String,
    },
    LRange {
        key: String,
        start: isize,
        stop: isize,
    },

    // --- Set Operations ---
    SAdd {
        key: String,
        members: Vec<String>,
    },
    SRem {
        key: String,
        members: Vec<String>,
    },
    SMembers {
        key: String,
    },
    SIsMember {
        key: String,
        member: String,
    },

    // --- Pub/Sub Operations ---
    Publish {
        channel: String,
        message: Vec<u8>,
    },
    Subscribe {
        channels: Vec<String>,
    },

    // --- Native Lin Pipe Query Integration ---
    /// Execute a native Lin query or mutation on the underlying `lin::Db`!
    LinQuery {
        query: String,
    },

    // --- Cluster, Sharding, Gossip & Raft Introspection ---
    ClusterInfo,
    ClusterNodes,
    ClusterMeet {
        node_id: String,
        addr: String,
    },
    ClusterKeySlot {
        key: String,
    },
    RaftStatus,
    RaftTriggerElection,

    // --- Internal Inter-Node RPCs (Sharding, Gossip, Raft) ---
    /// Direct execution on owning shard without re-routing loop.
    InternalShardExec {
        inner: Box<CacheRequest>,
    },
    /// Gossip membership + Pub/Sub + anti-entropy digest exchange.
    GossipSync {
        sender_id: String,
        sender_addr: String,
        members: Vec<NodeMember>,
        pubsub_events: Vec<PubSubEvent>,
    },
    /// Raft RequestVote RPC.
    RaftRequestVote {
        term: u64,
        candidate_id: String,
        last_log_index: u64,
        last_log_term: u64,
    },
    /// Raft AppendEntries / Heartbeat RPC (includes optional Lin WAL frames).
    RaftAppendEntries {
        term: u64,
        leader_id: String,
        leader_addr: String,
        prev_log_index: u64,
        prev_log_term: u64,
        entries: Vec<RaftLogEntry>,
        leader_commit: u64,
    },
}

/// Binary Response returned by `CacheNode`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CacheResponse {
    Pong(String),
    Ok,
    Nil,
    Int(i64),
    Bulk(Vec<u8>),
    Array(Vec<CacheResponse>),
    Map(BTreeMap<String, Vec<u8>>),
    Text(String),
    Error(String),
    /// Pushed to subscribed clients during `SUBSCRIBE`.
    PubSubMessage {
        channel: String,
        payload: Vec<u8>,
        origin_node: String,
    },
    /// Gossip reply.
    GossipAck {
        node_id: String,
        members: Vec<NodeMember>,
    },
    /// Raft RequestVote reply.
    RaftVoteReply {
        term: u64,
        vote_granted: bool,
    },
    /// Raft AppendEntries reply.
    RaftAppendReply {
        term: u64,
        success: bool,
        match_index: u64,
    },
}

impl CacheResponse {
    /// Format human-readable output for `lin cache cli`.
    pub fn format_cli(&self) -> String {
        match self {
            CacheResponse::Pong(s) => format!("PONG ({s})"),
            CacheResponse::Ok => "OK".to_string(),
            CacheResponse::Nil => "(nil)".to_string(),
            CacheResponse::Int(n) => format!("(integer) {n}"),
            CacheResponse::Bulk(b) => match std::str::from_utf8(b) {
                Ok(s) => format!("{s:?}"),
                Err(_) => format!("(bytes {:?})", b),
            },
            CacheResponse::Text(s) => s.clone(),
            CacheResponse::Error(e) => format!("(error) {e}"),
            CacheResponse::Array(items) => {
                if items.is_empty() {
                    return "(empty array)".to_string();
                }
                let mut out = String::new();
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        out.push('\n');
                    }
                    out.push_str(&format!("{}) {}", i + 1, item.format_cli()));
                }
                out
            }
            CacheResponse::Map(map) => {
                if map.is_empty() {
                    return "(empty map)".to_string();
                }
                let mut out = String::new();
                for (i, (k, v)) in map.iter().enumerate() {
                    if i > 0 {
                        out.push('\n');
                    }
                    let val_str = String::from_utf8_lossy(v);
                    out.push_str(&format!("{k}: {val_str:?}"));
                }
                out
            }
            CacheResponse::PubSubMessage {
                channel,
                payload,
                origin_node,
            } => {
                let msg = String::from_utf8_lossy(payload);
                format!("[pubsub ch={channel} from={origin_node}] {msg}")
            }
            other => format!("{other:?}"),
        }
    }
}

/// Write a framed binary message (`LINC` + `u8` ver + `u32` LE len + MessagePack).
pub fn write_frame<W: Write, T: Serialize>(w: &mut W, msg: &T) -> Result<(), Error> {
    let payload = rmp_serde::to_vec_named(msg).map_err(cache_err)?;
    if payload.len() as u64 > MAX_FRAME_LEN as u64 {
        return Err(cache_err(format!(
            "frame payload {} exceeds max {}",
            payload.len(),
            MAX_FRAME_LEN
        )));
    }
    let len = payload.len() as u32;
    let mut header = [0u8; 9];
    header[0..4].copy_from_slice(&CACHE_MAGIC);
    header[4] = CACHE_VERSION;
    header[5..9].copy_from_slice(&len.to_le_bytes());
    w.write_all(&header).map_err(cache_err)?;
    w.write_all(&payload).map_err(cache_err)?;
    w.flush().map_err(cache_err)?;
    Ok(())
}

/// Read a framed binary message (`LINC` + `u8` ver + `u32` LE len + MessagePack).
pub fn read_frame<R: Read, T: for<'de> Deserialize<'de>>(r: &mut R) -> Result<T, Error> {
    let mut header = [0u8; 9];
    r.read_exact(&mut header).map_err(cache_err)?;
    if header[0..4] != CACHE_MAGIC {
        return Err(cache_err("invalid protocol magic (expected LINC)"));
    }
    if header[4] != CACHE_VERSION {
        return Err(cache_err(format!(
            "unsupported protocol version {}",
            header[4]
        )));
    }
    let len = u32::from_le_bytes([header[5], header[6], header[7], header[8]]);
    if len > MAX_FRAME_LEN {
        return Err(cache_err(format!("frame length {len} exceeds maximum")));
    }
    let mut buf = vec![0u8; len as usize];
    r.read_exact(&mut buf).map_err(cache_err)?;
    rmp_serde::from_slice(&buf).map_err(cache_err)
}

// ============================================================================
// 2. Consistent Hashing Ring (Sharding + Replication)
// ============================================================================

/// Consistent hash ring using `crc32fast` and virtual nodes.
#[derive(Debug, Clone)]
pub struct ConsistentHashRing {
    /// Virtual node hash -> physical `node_id`.
    ring: BTreeMap<u32, String>,
    /// Physical `node_id` -> network address (`host:port`).
    nodes: BTreeMap<String, String>,
    /// Number of virtual nodes per physical node.
    vnodes: usize,
    /// Replication factor (how many distinct physical nodes own a key).
    replication_factor: usize,
}

impl Default for ConsistentHashRing {
    fn default() -> Self {
        Self::new(DEFAULT_VNODES, 2)
    }
}

impl ConsistentHashRing {
    pub fn new(vnodes: usize, replication_factor: usize) -> Self {
        Self {
            ring: BTreeMap::new(),
            nodes: BTreeMap::new(),
            vnodes: vnodes.max(1),
            replication_factor: replication_factor.max(1),
        }
    }

    pub fn hash_key(key: &str) -> u32 {
        // Support Redis-style hash tags `{user:123}:profile` -> hashes `user:123`
        if let Some(start) = key.find('{')
            && let Some(end) = key[start + 1..].find('}')
            && end > 0
        {
            let tag = &key[start + 1..start + 1 + end];
            return crc32fast::hash(tag.as_bytes());
        }
        crc32fast::hash(key.as_bytes())
    }

    pub fn add_node(&mut self, node_id: impl Into<String>, addr: impl Into<String>) {
        let node_id = node_id.into();
        let addr = addr.into();
        self.nodes.insert(node_id.clone(), addr);
        for v in 0..self.vnodes {
            let vkey = format!("{node_id}#vn{v}");
            let h = crc32fast::hash(vkey.as_bytes());
            self.ring.insert(h, node_id.clone());
        }
    }

    pub fn remove_node(&mut self, node_id: &str) {
        if self.nodes.remove(node_id).is_some() {
            self.ring.retain(|_, nid| nid != node_id);
        }
    }

    pub fn node_addr(&self, node_id: &str) -> Option<&str> {
        self.nodes.get(node_id).map(String::as_str)
    }

    pub fn nodes(&self) -> &BTreeMap<String, String> {
        &self.nodes
    }

    /// Return the primary owner `(node_id, addr)` for `key`.
    pub fn primary_for_key(&self, key: &str) -> Option<(String, String)> {
        let replicas = self.replicas_for_key(key);
        replicas.into_iter().next()
    }

    /// Return up to `replication_factor` distinct physical nodes `(node_id, addr)` clockwise on the ring.
    pub fn replicas_for_key(&self, key: &str) -> Vec<(String, String)> {
        if self.ring.is_empty() {
            return Vec::new();
        }
        let h = Self::hash_key(key);
        let target = self.replication_factor.min(self.nodes.len());
        let mut out = Vec::with_capacity(target);
        let mut seen = BTreeSet::new();

        for (_, nid) in self.ring.range(h..).chain(self.ring.iter()) {
            if seen.insert(nid.clone()) {
                if let Some(addr) = self.nodes.get(nid) {
                    out.push((nid.clone(), addr.clone()));
                }
                if out.len() >= target {
                    break;
                }
            }
        }
        out
    }
}

// ============================================================================
// 3. Gossip Membership & Pub/Sub State
// ============================================================================

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeRole {
    Leader,
    Follower,
    Candidate,
}

impl NodeRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            NodeRole::Leader => "leader",
            NodeRole::Follower => "follower",
            NodeRole::Candidate => "candidate",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeMember {
    pub node_id: String,
    pub addr: String,
    pub heartbeat: u64,
    pub last_seen_ms: u64,
    pub alive: bool,
    pub role: NodeRole,
    pub term: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PubSubEvent {
    pub id: u64,
    pub channel: String,
    pub payload: Vec<u8>,
    pub origin_node: String,
}

// ============================================================================
// 4. Raft Consensus & Replicated Log
// ============================================================================

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RaftCommand {
    Set {
        key: String,
        value: CacheValue,
        expires_at_ms: Option<u64>,
    },
    Del {
        keys: Vec<String>,
    },
    FlushAll,
    LinMutation {
        query: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RaftLogEntry {
    pub index: u64,
    pub term: u64,
    pub command: RaftCommand,
}

#[derive(Debug, Clone)]
pub struct RaftState {
    pub current_term: u64,
    pub voted_for: Option<String>,
    pub role: NodeRole,
    pub leader_id: Option<String>,
    pub leader_addr: Option<String>,
    pub log: Vec<RaftLogEntry>,
    pub commit_index: u64,
    pub last_applied: u64,
    pub last_leader_contact_ms: u64,
}

impl Default for RaftState {
    fn default() -> Self {
        Self {
            current_term: 1,
            voted_for: None,
            role: NodeRole::Leader, // Single node starts as leader; multi-node adjusts on gossip/election
            leader_id: None,
            leader_addr: None,
            log: Vec::new(),
            commit_index: 0,
            last_applied: 0,
            last_leader_contact_ms: unix_ms(),
        }
    }
}

impl RaftState {
    pub fn last_log_index(&self) -> u64 {
        self.log.last().map(|e| e.index).unwrap_or(0)
    }

    pub fn last_log_term(&self) -> u64 {
        self.log.last().map(|e| e.term).unwrap_or(0)
    }

    pub fn term_at(&self, index: u64) -> u64 {
        if index == 0 {
            return 0;
        }
        self.log
            .iter()
            .find(|e| e.index == index)
            .map(|e| e.term)
            .unwrap_or(0)
    }
}

// ============================================================================
// 5. Storage Engine: In-Memory Cache + LRU + TTL + Lin Db Persistence
// ============================================================================

#[derive(Debug, Clone)]
pub struct CacheConfig {
    pub node_id: String,
    pub listen_addr: String,
    pub max_entries: usize,
    pub max_bytes: usize,
    pub vnodes: usize,
    pub replication_factor: usize,
    /// Forward keys to their primary shard on the Consistent Hash Ring.
    pub sharding_enabled: bool,
    /// Replicate mutations via Raft AppendEntries to cluster peers.
    pub raft_replication: bool,
    /// Gossip interval in milliseconds.
    pub gossip_interval_ms: u64,
    /// Failure timeout in milliseconds.
    pub node_timeout_ms: u64,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            node_id: "node-1".to_string(),
            listen_addr: "127.0.0.1:6380".to_string(),
            max_entries: 100_000,
            max_bytes: 256 * 1024 * 1024,
            vnodes: DEFAULT_VNODES,
            replication_factor: 2,
            sharding_enabled: true,
            raft_replication: true,
            gossip_interval_ms: 300,
            node_timeout_ms: 5_000,
        }
    }
}

struct PubSubHub {
    next_event_id: u64,
    seen_events: BTreeSet<u64>,
    recent_outbox: VecDeque<PubSubEvent>,
    /// Channel -> list of subscriber queues.
    subscribers: FxHashMap<String, Vec<Arc<(Mutex<VecDeque<CacheResponse>>, Condvar)>>>,
}

impl Default for PubSubHub {
    fn default() -> Self {
        Self {
            next_event_id: 1,
            seen_events: BTreeSet::new(),
            recent_outbox: VecDeque::new(),
            subscribers: FxHashMap::default(),
        }
    }
}

struct EngineState {
    entries: FxHashMap<String, CacheEntry>,
    access_seq: u64,
    used_bytes: usize,
    hits: u64,
    misses: u64,
    evictions: u64,
    expired_cleaned: u64,
    /// Underlying `lin::Db` for durable persistence and pipe queries!
    db: Db,
    /// Ring for consistent hashing across cluster nodes.
    ring: ConsistentHashRing,
    /// Gossip membership table (`node_id` -> `NodeMember`).
    members: BTreeMap<String, NodeMember>,
    /// Local heartbeat counter.
    heartbeat: u64,
    /// Raft consensus state.
    raft: RaftState,
    /// Pub/Sub state.
    pubsub: PubSubHub,
}

impl EngineState {
    fn new(mut db: Db, cfg: &CacheConfig) -> Result<Self, Error> {
        // Ensure `cache_kv` collection exists in `lin::Db` for durable key-value snapshots
        if db.catalog.collection("cache_kv").is_none() {
            let _ = db.run(
                "col cache_kv { k: text, vtype: text, val_hex: text, exp_ms: i64, ver: i64 }\n\
                 index cache_kv [k]",
            );
        }

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
            access_seq: 1,
            used_bytes: 0,
            hits: 0,
            misses: 0,
            evictions: 0,
            expired_cleaned: 0,
            db,
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

    fn hydrate_from_lin_db(&mut self) {
        let now = unix_ms();
        let Ok(handle) = self.db.run("cache_kv | { k, vtype, val_hex, exp_ms, ver }") else {
            return;
        };
        for row in &handle.rows {
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
                let sz = k.len() + value.estimated_bytes();
                self.access_seq += 1;
                self.entries.insert(
                    k.to_string(),
                    CacheEntry {
                        value,
                        expires_at_ms: exp_ms,
                        last_access_seq: self.access_seq,
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
        let escaped_key = escape_lin_str(key);
        let _ = self.db.run(&format!(
            "delete cache_kv [k == \"{escaped_key}\"] cas \"\""
        ));
        let _ = self.db.run(&format!(
            "insert cache_kv [{{ k: \"{escaped_key}\", vtype: \"{vtype}\", val_hex: \"{hex_val}\", exp_ms: {exp}, ver: {ver} }}]"
        ));
    }

    fn delete_entry_from_lin(&mut self, key: &str) {
        let escaped_key = escape_lin_str(key);
        let _ = self.db.run(&format!(
            "delete cache_kv [k == \"{escaped_key}\"] cas \"\""
        ));
    }

    fn evict_expired_and_lru(&mut self, max_entries: usize, max_bytes: usize) {
        let now = unix_ms();
        // 1. Collect expired keys first
        let expired: Vec<String> = self
            .entries
            .iter()
            .filter(|(_, e)| e.is_expired(now))
            .map(|(k, _)| k.clone())
            .collect();
        for k in expired {
            if let Some(removed) = self.entries.remove(&k) {
                self.used_bytes = self
                    .used_bytes
                    .saturating_sub(k.len() + removed.value.estimated_bytes());
                self.expired_cleaned += 1;
                self.delete_entry_from_lin(&k);
            }
        }

        // 2. Evict LRU if over capacity
        while (self.entries.len() > max_entries || self.used_bytes > max_bytes)
            && !self.entries.is_empty()
        {
            let Some(oldest_key) = self
                .entries
                .iter()
                .min_by_key(|(_, e)| e.last_access_seq)
                .map(|(k, _)| k.clone())
            else {
                break;
            };
            if let Some(removed) = self.entries.remove(&oldest_key) {
                self.used_bytes = self
                    .used_bytes
                    .saturating_sub(oldest_key.len() + removed.value.estimated_bytes());
                self.evictions += 1;
                self.delete_entry_from_lin(&oldest_key);
            }
        }
    }

    fn touch_or_remove_expired(&mut self, key: &str) -> bool {
        let now = unix_ms();
        if let Some(e) = self.entries.get(key)
            && e.is_expired(now)
        {
            if let Some(removed) = self.entries.remove(key) {
                self.used_bytes = self
                    .used_bytes
                    .saturating_sub(key.len() + removed.value.estimated_bytes());
                self.expired_cleaned += 1;
                self.delete_entry_from_lin(key);
            }
            return false;
        }
        if let Some(e) = self.entries.get_mut(key) {
            self.access_seq += 1;
            e.last_access_seq = self.access_seq;
            true
        } else {
            false
        }
    }

    fn put_value(
        &mut self,
        key: String,
        value: CacheValue,
        expires_at_ms: Option<u64>,
        max_entries: usize,
        max_bytes: usize,
    ) {
        if let Some(old) = self.entries.remove(&key) {
            self.used_bytes = self
                .used_bytes
                .saturating_sub(key.len() + old.value.estimated_bytes());
        }
        self.access_seq += 1;
        let ver = self.raft.last_log_index() + 1;
        let entry = CacheEntry {
            value,
            expires_at_ms,
            last_access_seq: self.access_seq,
            version: ver,
        };
        self.used_bytes += key.len() + entry.value.estimated_bytes();
        self.persist_entry_to_lin(&key, &entry);
        self.entries.insert(key, entry);
        self.evict_expired_and_lru(max_entries, max_bytes);
    }

    fn remove_key(&mut self, key: &str) -> bool {
        if let Some(old) = self.entries.remove(key) {
            self.used_bytes = self
                .used_bytes
                .saturating_sub(key.len() + old.value.estimated_bytes());
            self.delete_entry_from_lin(key);
            true
        } else {
            false
        }
    }

    fn apply_raft_command(&mut self, cmd: &RaftCommand, max_entries: usize, max_bytes: usize) {
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
                self.used_bytes = 0;
                let _ = self.db.run("delete cache_kv [k is not null] cas \"\"");
            }
            RaftCommand::LinMutation { query } => {
                let _ = self.db.run(query);
            }
        }
    }

    fn publish_local(&mut self, channel: &str, payload: &[u8], origin_node: &str) -> usize {
        let Some(subs) = self.pubsub.subscribers.get_mut(channel) else {
            return 0;
        };
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
    if s.len() % 2 != 0 {
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

// ============================================================================
// 6. Distributed CacheNode Server (TCP + Gossip + Raft + Sharding)
// ============================================================================

/// A running or embeddable distributed cache node backed by `lin::Db`.
#[derive(Clone)]
pub struct CacheNode {
    cfg: Arc<CacheConfig>,
    state: Arc<Mutex<EngineState>>,
    stop: Arc<AtomicBool>,
}

impl CacheNode {
    /// Create a new in-memory distributed cache node (backed by `Db::fixture()`).
    pub fn new_in_memory(cfg: CacheConfig) -> Result<Self, Error> {
        let db = Db::fixture();
        Self::with_db(db, cfg)
    }

    /// Create a durable distributed cache node backed by a `lin::Db` directory.
    pub fn open_durable(path: impl AsRef<Path>, cfg: CacheConfig) -> Result<Self, Error> {
        let mut opts = OpenOpts::default();
        opts.sync = SyncMode::Normal;
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
        })
    }

    pub fn config(&self) -> &CacheConfig {
        &self.cfg
    }

    pub fn stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
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
                let (local_subs, event, peers) = {
                    let mut st = self.state.lock().unwrap();
                    let eid = (crc32fast::hash(self.cfg.node_id.as_bytes()) as u64) << 32
                        | st.pubsub.next_event_id;
                    st.pubsub.next_event_id += 1;
                    st.pubsub.seen_events.insert(eid);
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
                    (count, ev, peers)
                };
                // Propagate Pub/Sub event immediately to alive cluster peers via GossipSync
                if !peers.is_empty() {
                    let members_snap = self
                        .state
                        .lock()
                        .unwrap()
                        .members
                        .values()
                        .cloned()
                        .collect::<Vec<_>>();
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
                    st.db.run(&query).map(|h| h.compact())
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
                self.meet(node_id.clone(), addr.clone());
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
                    let mut l = VecDeque::new();
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

    // --- Raft Log Proposal & Replication ---

    fn propose_and_apply_mutation(&self, cmd: RaftCommand) {
        let (entry, prev_idx, prev_term, term, peers) = {
            let mut st = self.state.lock().unwrap();
            let prev_idx = st.raft.last_log_index();
            let prev_term = st.raft.last_log_term();
            let term = st.raft.current_term;
            let idx = prev_idx + 1;
            let entry = RaftLogEntry {
                index: idx,
                term,
                command: cmd.clone(),
            };
            st.raft.log.push(entry.clone());
            st.apply_raft_command(&cmd, self.cfg.max_entries, self.cfg.max_bytes);
            st.raft.commit_index = idx;
            st.raft.last_applied = idx;

            let peers: Vec<String> = if self.cfg.raft_replication {
                st.members
                    .values()
                    .filter(|m| m.node_id != self.cfg.node_id && m.alive)
                    .map(|m| m.addr.clone())
                    .collect()
            } else {
                Vec::new()
            };
            (entry, prev_idx, prev_term, term, peers)
        };

        if !peers.is_empty() {
            let rpc = CacheRequest::RaftAppendEntries {
                term,
                leader_id: self.cfg.node_id.clone(),
                leader_addr: self.cfg.listen_addr.clone(),
                prev_log_index: prev_idx,
                prev_log_term: prev_term,
                entries: vec![entry],
                leader_commit: prev_idx + 1,
            };
            for peer_addr in peers {
                let _ = send_rpc(&peer_addr, &rpc);
            }
        }
    }

    fn propose_raft_only(&self, cmd: RaftCommand) {
        let (entry, prev_idx, prev_term, term, peers) = {
            let mut st = self.state.lock().unwrap();
            let prev_idx = st.raft.last_log_index();
            let prev_term = st.raft.last_log_term();
            let term = st.raft.current_term;
            let idx = prev_idx + 1;
            let entry = RaftLogEntry {
                index: idx,
                term,
                command: cmd,
            };
            st.raft.log.push(entry.clone());
            st.raft.commit_index = idx;
            st.raft.last_applied = idx;
            let peers: Vec<String> = if self.cfg.raft_replication {
                st.members
                    .values()
                    .filter(|m| m.node_id != self.cfg.node_id && m.alive)
                    .map(|m| m.addr.clone())
                    .collect()
            } else {
                Vec::new()
            };
            (entry, prev_idx, prev_term, term, peers)
        };

        if !peers.is_empty() {
            let rpc = CacheRequest::RaftAppendEntries {
                term,
                leader_id: self.cfg.node_id.clone(),
                leader_addr: self.cfg.listen_addr.clone(),
                prev_log_index: prev_idx,
                prev_log_term: prev_term,
                entries: vec![entry],
                leader_commit: prev_idx + 1,
            };
            for peer_addr in peers {
                let _ = send_rpc(&peer_addr, &rpc);
            }
        }
    }

    fn handle_gossip_sync(
        &self,
        sender_id: String,
        sender_addr: String,
        incoming_members: Vec<NodeMember>,
        pubsub_events: Vec<PubSubEvent>,
    ) -> CacheResponse {
        let mut st = self.state.lock().unwrap();
        let now = unix_ms();

        // Ensure sender is in the ring & membership table
        st.ring.add_node(&sender_id, &sender_addr);
        for inc in incoming_members {
            if inc.node_id == self.cfg.node_id {
                continue;
            }
            st.ring.add_node(&inc.node_id, &inc.addr);
            st.members
                .entry(inc.node_id.clone())
                .and_modify(|m| {
                    if inc.heartbeat >= m.heartbeat {
                        m.heartbeat = inc.heartbeat;
                        m.addr = inc.addr.clone();
                        m.role = inc.role.clone();
                        m.term = inc.term;
                        m.alive = true;
                        m.last_seen_ms = now;
                    }
                })
                .or_insert_with(|| {
                    let mut m = inc;
                    m.last_seen_ms = now;
                    m
                });
        }

        // Process incoming Pub/Sub events
        for ev in pubsub_events {
            if st.pubsub.seen_events.insert(ev.id) {
                st.publish_local(&ev.channel, &ev.payload, &ev.origin_node);
            }
        }

        // Update our own heartbeat in membership table
        st.heartbeat += 1;
        let hb = st.heartbeat;
        let role = st.raft.role.clone();
        let term = st.raft.current_term;
        if let Some(me) = st.members.get_mut(&self.cfg.node_id) {
            me.heartbeat = hb;
            me.last_seen_ms = now;
            me.alive = true;
            me.role = role;
            me.term = term;
        }

        let members = st.members.values().cloned().collect();
        CacheResponse::GossipAck {
            node_id: self.cfg.node_id.clone(),
            members,
        }
    }

    fn handle_raft_request_vote(
        &self,
        term: u64,
        candidate_id: String,
        last_log_index: u64,
        last_log_term: u64,
    ) -> CacheResponse {
        let mut st = self.state.lock().unwrap();
        if term < st.raft.current_term {
            return CacheResponse::RaftVoteReply {
                term: st.raft.current_term,
                vote_granted: false,
            };
        }
        if term > st.raft.current_term {
            st.raft.current_term = term;
            st.raft.role = NodeRole::Follower;
            st.raft.voted_for = None;
        }
        let my_last_term = st.raft.last_log_term();
        let my_last_idx = st.raft.last_log_index();
        let log_ok = last_log_term > my_last_term
            || (last_log_term == my_last_term && last_log_index >= my_last_idx);
        let can_vote = st
            .raft
            .voted_for
            .as_ref()
            .is_none_or(|v| v == &candidate_id);

        if can_vote && log_ok {
            st.raft.voted_for = Some(candidate_id.clone());
            st.raft.last_leader_contact_ms = unix_ms();
            let cur_term = st.raft.current_term;
            if let Some(me) = st.members.get_mut(&self.cfg.node_id) {
                me.role = NodeRole::Follower;
                me.term = cur_term;
            }
            CacheResponse::RaftVoteReply {
                term: cur_term,
                vote_granted: true,
            }
        } else {
            CacheResponse::RaftVoteReply {
                term: st.raft.current_term,
                vote_granted: false,
            }
        }
    }

    fn handle_raft_append_entries(
        &self,
        term: u64,
        leader_id: String,
        leader_addr: String,
        _prev_log_index: u64,
        _prev_log_term: u64,
        entries: Vec<RaftLogEntry>,
        leader_commit: u64,
    ) -> CacheResponse {
        let mut st = self.state.lock().unwrap();
        if term < st.raft.current_term {
            return CacheResponse::RaftAppendReply {
                term: st.raft.current_term,
                success: false,
                match_index: st.raft.last_log_index(),
            };
        }

        st.raft.current_term = term;
        st.raft.role = NodeRole::Follower;
        st.raft.leader_id = Some(leader_id.clone());
        st.raft.leader_addr = Some(leader_addr.clone());
        st.raft.last_leader_contact_ms = unix_ms();
        st.ring.add_node(&leader_id, &leader_addr);

        if let Some(me) = st.members.get_mut(&self.cfg.node_id) {
            me.role = NodeRole::Follower;
            me.term = term;
        }

        for entry in entries {
            if entry.index > st.raft.last_log_index() {
                st.apply_raft_command(&entry.command, self.cfg.max_entries, self.cfg.max_bytes);
                st.raft.last_applied = entry.index;
                st.raft.log.push(entry);
            }
        }
        if leader_commit > st.raft.commit_index {
            st.raft.commit_index = leader_commit.min(st.raft.last_log_index());
        }

        CacheResponse::RaftAppendReply {
            term: st.raft.current_term,
            success: true,
            match_index: st.raft.last_log_index(),
        }
    }

    /// Trigger a Raft leader election across known cluster peers.
    pub fn run_leader_election(&self) -> bool {
        let (term, last_idx, last_term, peers, total_nodes) = {
            let mut st = self.state.lock().unwrap();
            st.raft.current_term += 1;
            st.raft.role = NodeRole::Candidate;
            st.raft.voted_for = Some(self.cfg.node_id.clone());
            let term = st.raft.current_term;
            if let Some(me) = st.members.get_mut(&self.cfg.node_id) {
                me.role = NodeRole::Candidate;
                me.term = term;
            }
            let peers: Vec<String> = st
                .members
                .values()
                .filter(|m| m.node_id != self.cfg.node_id && m.alive)
                .map(|m| m.addr.clone())
                .collect();
            let total = st.members.values().filter(|m| m.alive).count().max(1);
            (
                term,
                st.raft.last_log_index(),
                st.raft.last_log_term(),
                peers,
                total,
            )
        };

        let mut votes = 1usize; // Vote for self
        let quorum = (total_nodes / 2) + 1;
        let req = CacheRequest::RaftRequestVote {
            term,
            candidate_id: self.cfg.node_id.clone(),
            last_log_index: last_idx,
            last_log_term: last_term,
        };

        for peer_addr in &peers {
            if let Ok(CacheResponse::RaftVoteReply {
                term: reply_term,
                vote_granted,
            }) = send_rpc(peer_addr, &req)
            {
                if reply_term > term {
                    let mut st = self.state.lock().unwrap();
                    st.raft.current_term = reply_term;
                    st.raft.role = NodeRole::Follower;
                    st.raft.voted_for = None;
                    return false;
                }
                if vote_granted {
                    votes += 1;
                }
            }
        }

        let won = votes >= quorum;
        if won {
            let mut st = self.state.lock().unwrap();
            if st.raft.current_term == term {
                st.raft.role = NodeRole::Leader;
                st.raft.leader_id = Some(self.cfg.node_id.clone());
                st.raft.leader_addr = Some(self.cfg.listen_addr.clone());
                if let Some(me) = st.members.get_mut(&self.cfg.node_id) {
                    me.role = NodeRole::Leader;
                    me.term = term;
                }
            }
            drop(st);
            // Send immediate heartbeat AppendEntries to establish leadership
            let hb = CacheRequest::RaftAppendEntries {
                term,
                leader_id: self.cfg.node_id.clone(),
                leader_addr: self.cfg.listen_addr.clone(),
                prev_log_index: last_idx,
                prev_log_term: last_term,
                entries: Vec::new(),
                leader_commit: last_idx,
            };
            for peer_addr in &peers {
                let _ = send_rpc(peer_addr, &hb);
            }
        }
        won
    }

    /// Perform one Gossip round with a specific peer address.
    pub fn gossip_once_with(&self, peer_addr: &str) -> Result<(), Error> {
        let req = {
            let mut st = self.state.lock().unwrap();
            st.heartbeat += 1;
            let hb = st.heartbeat;
            let now = unix_ms();
            let role = st.raft.role.clone();
            let term = st.raft.current_term;
            if let Some(me) = st.members.get_mut(&self.cfg.node_id) {
                me.heartbeat = hb;
                me.last_seen_ms = now;
                me.alive = true;
                me.role = role;
                me.term = term;
            }
            CacheRequest::GossipSync {
                sender_id: self.cfg.node_id.clone(),
                sender_addr: self.cfg.listen_addr.clone(),
                members: st.members.values().cloned().collect(),
                pubsub_events: st.pubsub.recent_outbox.iter().cloned().collect(),
            }
        };

        let resp = send_rpc(peer_addr, &req)?;
        if let CacheResponse::GossipAck {
            node_id: _,
            members,
        } = resp
        {
            let mut st = self.state.lock().unwrap();
            let now = unix_ms();
            for inc in members {
                if inc.node_id == self.cfg.node_id {
                    continue;
                }
                st.ring.add_node(&inc.node_id, &inc.addr);
                st.members
                    .entry(inc.node_id.clone())
                    .and_modify(|m| {
                        if inc.heartbeat >= m.heartbeat {
                            m.heartbeat = inc.heartbeat;
                            m.addr = inc.addr.clone();
                            m.role = inc.role.clone();
                            m.term = inc.term;
                            m.alive = true;
                            m.last_seen_ms = now;
                        }
                    })
                    .or_insert_with(|| {
                        let mut m = inc;
                        m.last_seen_ms = now;
                        m
                    });
            }
        }
        Ok(())
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
                    thread::sleep(Duration::from_millis(10));
                }
                Err(_) => {
                    if self.stop.load(Ordering::Relaxed) {
                        break;
                    }
                    thread::sleep(Duration::from_millis(25));
                }
            }
        }

        let _ = gossip_handle.join();
        Ok(())
    }

    fn background_loop(&self) {
        let interval = Duration::from_millis(self.cfg.gossip_interval_ms.max(50));
        while !self.stop.load(Ordering::Relaxed) {
            thread::sleep(interval);
            if self.stop.load(Ordering::Relaxed) {
                break;
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
                            .push(queue.clone());
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
                                .wait_timeout(guard, Duration::from_millis(200))
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

// ============================================================================
// 7. Binary TCP Client & CLI Command Parser
// ============================================================================

/// Send a single request-response RPC to a `CacheNode` over TCP.
pub fn send_rpc(addr: impl ToSocketAddrs, req: &CacheRequest) -> Result<CacheResponse, Error> {
    let addr_resolved: SocketAddr = addr
        .to_socket_addrs()
        .map_err(cache_err)?
        .next()
        .ok_or_else(|| cache_err("unable to resolve address"))?;
    let mut stream =
        TcpStream::connect_timeout(&addr_resolved, Duration::from_secs(2)).map_err(cache_err)?;
    let _ = stream.set_nodelay(true);
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));
    write_frame(&mut stream, req)?;
    read_frame(&mut stream)
}

/// Persistent TCP client for `CacheNode`.
pub struct CacheClient {
    addr: String,
    stream: TcpStream,
}

impl CacheClient {
    pub fn connect(addr: impl Into<String>) -> Result<Self, Error> {
        let addr = addr.into();
        let stream = TcpStream::connect(&addr).map_err(cache_err)?;
        let _ = stream.set_nodelay(true);
        Ok(Self { addr, stream })
    }

    pub fn addr(&self) -> &str {
        &self.addr
    }

    pub fn send(&mut self, req: &CacheRequest) -> Result<CacheResponse, Error> {
        write_frame(&mut self.stream, req)?;
        read_frame(&mut self.stream)
    }

    pub fn read_next(&mut self) -> Result<CacheResponse, Error> {
        read_frame(&mut self.stream)
    }

    pub fn execute_line(&mut self, line: &str) -> Result<CacheResponse, Error> {
        let req = parse_cli_command(line)?;
        self.send(&req)
    }
}

/// Tokenize a CLI command line respecting double/single quotes.
pub fn tokenize_cli(line: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut cur = String::new();
    let mut in_quote: Option<char> = None;
    let mut chars = line.chars().peekable();

    while let Some(c) = chars.next() {
        if let Some(q) = in_quote {
            if c == q {
                in_quote = None;
            } else if c == '\\' {
                if let Some(next_c) = chars.next() {
                    cur.push(next_c);
                }
            } else {
                cur.push(c);
            }
        } else if c == '"' || c == '\'' {
            in_quote = Some(c);
        } else if c.is_whitespace() {
            if !cur.is_empty() {
                tokens.push(std::mem::take(&mut cur));
            }
        } else {
            cur.push(c);
        }
    }
    if !cur.is_empty() {
        tokens.push(cur);
    }
    tokens
}

/// Parse a CLI command string (e.g., `SET k v EX 10`, `HSET user name Alice`, `LIN docs | take 5`) into a `CacheRequest`.
pub fn parse_cli_command(line: &str) -> Result<CacheRequest, Error> {
    let tokens = tokenize_cli(line);
    let Some(cmd) = tokens.first() else {
        return Err(cache_err("empty command"));
    };
    let upper = cmd.to_ascii_uppercase();
    match upper.as_str() {
        "PING" => Ok(CacheRequest::Ping(tokens.get(1).cloned())),
        "GET" => {
            let key = tokens
                .get(1)
                .ok_or_else(|| cache_err("usage: GET <key>"))?
                .clone();
            Ok(CacheRequest::Get { key })
        }
        "SET" => {
            let key = tokens
                .get(1)
                .ok_or_else(|| cache_err("usage: SET <key> <value> [EX <sec> | PX <ms>]"))?
                .clone();
            let val = tokens
                .get(2)
                .ok_or_else(|| cache_err("usage: SET <key> <value> [EX <sec> | PX <ms>]"))?
                .as_bytes()
                .to_vec();
            let mut ttl_ms = None;
            if let Some(opt) = tokens.get(3) {
                let n: u64 = tokens
                    .get(4)
                    .ok_or_else(|| cache_err("missing TTL number"))?
                    .parse()
                    .map_err(cache_err)?;
                match opt.to_ascii_uppercase().as_str() {
                    "EX" => ttl_ms = Some(n * 1000),
                    "PX" => ttl_ms = Some(n),
                    other => return Err(cache_err(format!("unknown SET option {other}"))),
                }
            }
            Ok(CacheRequest::Set {
                key,
                value: val,
                ttl_ms,
            })
        }
        "DEL" => {
            if tokens.len() < 2 {
                return Err(cache_err("usage: DEL <key> [key ...]"));
            }
            Ok(CacheRequest::Del {
                keys: tokens[1..].to_vec(),
            })
        }
        "EXISTS" => {
            let key = tokens
                .get(1)
                .ok_or_else(|| cache_err("usage: EXISTS <key>"))?
                .clone();
            Ok(CacheRequest::Exists { key })
        }
        "INCR" => {
            let key = tokens
                .get(1)
                .ok_or_else(|| cache_err("usage: INCR <key>"))?
                .clone();
            Ok(CacheRequest::IncrBy { key, delta: 1 })
        }
        "DECR" => {
            let key = tokens
                .get(1)
                .ok_or_else(|| cache_err("usage: DECR <key>"))?
                .clone();
            Ok(CacheRequest::IncrBy { key, delta: -1 })
        }
        "INCRBY" => {
            let key = tokens
                .get(1)
                .ok_or_else(|| cache_err("usage: INCRBY <key> <delta>"))?
                .clone();
            let delta: i64 = tokens
                .get(2)
                .ok_or_else(|| cache_err("usage: INCRBY <key> <delta>"))?
                .parse()
                .map_err(cache_err)?;
            Ok(CacheRequest::IncrBy { key, delta })
        }
        "EXPIRE" => {
            let key = tokens
                .get(1)
                .ok_or_else(|| cache_err("usage: EXPIRE <key> <seconds>"))?
                .clone();
            let secs: u64 = tokens
                .get(2)
                .ok_or_else(|| cache_err("usage: EXPIRE <key> <seconds>"))?
                .parse()
                .map_err(cache_err)?;
            Ok(CacheRequest::Expire {
                key,
                ttl_ms: secs * 1000,
            })
        }
        "PEXPIRE" => {
            let key = tokens
                .get(1)
                .ok_or_else(|| cache_err("usage: PEXPIRE <key> <ms>"))?
                .clone();
            let ttl_ms: u64 = tokens
                .get(2)
                .ok_or_else(|| cache_err("usage: PEXPIRE <key> <ms>"))?
                .parse()
                .map_err(cache_err)?;
            Ok(CacheRequest::Expire { key, ttl_ms })
        }
        "PERSIST" => {
            let key = tokens
                .get(1)
                .ok_or_else(|| cache_err("usage: PERSIST <key>"))?
                .clone();
            Ok(CacheRequest::Persist { key })
        }
        "TTL" | "PTTL" => {
            let key = tokens
                .get(1)
                .ok_or_else(|| cache_err("usage: TTL <key>"))?
                .clone();
            Ok(CacheRequest::Ttl { key })
        }
        "KEYS" | "SCAN" => {
            let prefix = tokens.get(1).cloned().unwrap_or_else(|| "*".into());
            Ok(CacheRequest::Keys { prefix })
        }
        "FLUSHALL" | "FLUSHDB" => Ok(CacheRequest::FlushAll),

        // Hashes
        "HSET" => {
            let key = tokens
                .get(1)
                .ok_or_else(|| cache_err("usage: HSET <key> <field> <value>"))?
                .clone();
            let field = tokens
                .get(2)
                .ok_or_else(|| cache_err("usage: HSET <key> <field> <value>"))?
                .clone();
            let value = tokens
                .get(3)
                .ok_or_else(|| cache_err("usage: HSET <key> <field> <value>"))?
                .as_bytes()
                .to_vec();
            Ok(CacheRequest::HSet { key, field, value })
        }
        "HGET" => {
            let key = tokens
                .get(1)
                .ok_or_else(|| cache_err("usage: HGET <key> <field>"))?
                .clone();
            let field = tokens
                .get(2)
                .ok_or_else(|| cache_err("usage: HGET <key> <field>"))?
                .clone();
            Ok(CacheRequest::HGet { key, field })
        }
        "HDEL" => {
            if tokens.len() < 3 {
                return Err(cache_err("usage: HDEL <key> <field> [field ...]"));
            }
            Ok(CacheRequest::HDel {
                key: tokens[1].clone(),
                fields: tokens[2..].to_vec(),
            })
        }
        "HGETALL" => {
            let key = tokens
                .get(1)
                .ok_or_else(|| cache_err("usage: HGETALL <key>"))?
                .clone();
            Ok(CacheRequest::HGetAll { key })
        }

        // Lists
        "LPUSH" => {
            if tokens.len() < 3 {
                return Err(cache_err("usage: LPUSH <key> <val> [val ...]"));
            }
            Ok(CacheRequest::LPush {
                key: tokens[1].clone(),
                values: tokens[2..].iter().map(|s| s.as_bytes().to_vec()).collect(),
            })
        }
        "RPUSH" => {
            if tokens.len() < 3 {
                return Err(cache_err("usage: RPUSH <key> <val> [val ...]"));
            }
            Ok(CacheRequest::RPush {
                key: tokens[1].clone(),
                values: tokens[2..].iter().map(|s| s.as_bytes().to_vec()).collect(),
            })
        }
        "LPOP" => {
            let key = tokens
                .get(1)
                .ok_or_else(|| cache_err("usage: LPOP <key>"))?
                .clone();
            Ok(CacheRequest::LPop { key })
        }
        "RPOP" => {
            let key = tokens
                .get(1)
                .ok_or_else(|| cache_err("usage: RPOP <key>"))?
                .clone();
            Ok(CacheRequest::RPop { key })
        }
        "LRANGE" => {
            let key = tokens
                .get(1)
                .ok_or_else(|| cache_err("usage: LRANGE <key> <start> <stop>"))?
                .clone();
            let start: isize = tokens
                .get(2)
                .ok_or_else(|| cache_err("usage: LRANGE <key> <start> <stop>"))?
                .parse()
                .map_err(cache_err)?;
            let stop: isize = tokens
                .get(3)
                .ok_or_else(|| cache_err("usage: LRANGE <key> <start> <stop>"))?
                .parse()
                .map_err(cache_err)?;
            Ok(CacheRequest::LRange { key, start, stop })
        }

        // Sets
        "SADD" => {
            if tokens.len() < 3 {
                return Err(cache_err("usage: SADD <key> <member> [member ...]"));
            }
            Ok(CacheRequest::SAdd {
                key: tokens[1].clone(),
                members: tokens[2..].to_vec(),
            })
        }
        "SREM" => {
            if tokens.len() < 3 {
                return Err(cache_err("usage: SREM <key> <member> [member ...]"));
            }
            Ok(CacheRequest::SRem {
                key: tokens[1].clone(),
                members: tokens[2..].to_vec(),
            })
        }
        "SMEMBERS" => {
            let key = tokens
                .get(1)
                .ok_or_else(|| cache_err("usage: SMEMBERS <key>"))?
                .clone();
            Ok(CacheRequest::SMembers { key })
        }
        "SISMEMBER" => {
            let key = tokens
                .get(1)
                .ok_or_else(|| cache_err("usage: SISMEMBER <key> <member>"))?
                .clone();
            let member = tokens
                .get(2)
                .ok_or_else(|| cache_err("usage: SISMEMBER <key> <member>"))?
                .clone();
            Ok(CacheRequest::SIsMember { key, member })
        }

        // Pub/Sub
        "PUBLISH" => {
            let channel = tokens
                .get(1)
                .ok_or_else(|| cache_err("usage: PUBLISH <channel> <message>"))?
                .clone();
            let message = tokens
                .get(2)
                .ok_or_else(|| cache_err("usage: PUBLISH <channel> <message>"))?
                .as_bytes()
                .to_vec();
            Ok(CacheRequest::Publish { channel, message })
        }
        "SUBSCRIBE" => {
            if tokens.len() < 2 {
                return Err(cache_err("usage: SUBSCRIBE <channel> [channel ...]"));
            }
            Ok(CacheRequest::Subscribe {
                channels: tokens[1..].to_vec(),
            })
        }

        // Native Lin Pipe Query
        "LIN" | "QUERY" => {
            let rest = line
                .trim_start()
                .split_once(char::is_whitespace)
                .map(|x| x.1.trim())
                .unwrap_or("");
            if rest.is_empty() {
                return Err(cache_err("usage: LIN <lin-pipe-query>"));
            }
            Ok(CacheRequest::LinQuery {
                query: rest.to_string(),
            })
        }

        // Cluster & Raft
        "INFO" => Ok(CacheRequest::ClusterInfo),
        "CLUSTER" => {
            let sub = tokens
                .get(1)
                .map(|s| s.to_ascii_uppercase())
                .unwrap_or_else(|| "INFO".into());
            match sub.as_str() {
                "INFO" => Ok(CacheRequest::ClusterInfo),
                "NODES" => Ok(CacheRequest::ClusterNodes),
                "MEET" => {
                    let node_id = tokens
                        .get(2)
                        .ok_or_else(|| cache_err("usage: CLUSTER MEET <node_id> <host:port>"))?
                        .clone();
                    let addr = tokens
                        .get(3)
                        .ok_or_else(|| cache_err("usage: CLUSTER MEET <node_id> <host:port>"))?
                        .clone();
                    Ok(CacheRequest::ClusterMeet { node_id, addr })
                }
                "KEYSLOT" | "SHARD" => {
                    let key = tokens
                        .get(2)
                        .ok_or_else(|| cache_err("usage: CLUSTER KEYSLOT <key>"))?
                        .clone();
                    Ok(CacheRequest::ClusterKeySlot { key })
                }
                other => Err(cache_err(format!(
                    "unknown CLUSTER subcommand {other} (INFO|NODES|MEET|KEYSLOT)"
                ))),
            }
        }
        "RAFT" => {
            let sub = tokens
                .get(1)
                .map(|s| s.to_ascii_uppercase())
                .unwrap_or_else(|| "STATUS".into());
            match sub.as_str() {
                "STATUS" => Ok(CacheRequest::RaftStatus),
                "ELECT" | "ELECTION" => Ok(CacheRequest::RaftTriggerElection),
                other => Err(cache_err(format!(
                    "unknown RAFT subcommand {other} (STATUS|ELECT)"
                ))),
            }
        }
        other => Err(cache_err(format!("unknown command '{other}'"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spawn_test_node(node_id: &str, max_entries: usize) -> (CacheNode, String) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap().to_string();
        let cfg = CacheConfig {
            node_id: node_id.to_string(),
            listen_addr: addr.clone(),
            max_entries,
            max_bytes: 64 * 1024 * 1024,
            vnodes: 32,
            replication_factor: 2,
            sharding_enabled: true,
            raft_replication: true,
            gossip_interval_ms: 100,
            node_timeout_ms: 5_000,
        };
        let node = CacheNode::new_in_memory(cfg).unwrap();
        let srv = node.clone();
        thread::spawn(move || {
            let _ = srv.serve(listener);
        });
        (node, addr)
    }

    #[test]
    fn test_data_structures_and_lin_integration() {
        let (node, addr) = spawn_test_node("node-solo", 50);
        let mut client = CacheClient::connect(&addr).unwrap();

        // 1. Strings & INCR & TTL
        assert_eq!(
            client.execute_line("SET counter 10").unwrap(),
            CacheResponse::Ok
        );
        assert_eq!(
            client.execute_line("INCRBY counter 5").unwrap(),
            CacheResponse::Int(15)
        );
        assert_eq!(
            client.execute_line("GET counter").unwrap(),
            CacheResponse::Bulk(b"15".to_vec())
        );

        // 2. Hashes
        assert_eq!(
            client.execute_line("HSET user:1 name Alice").unwrap(),
            CacheResponse::Int(1)
        );
        assert_eq!(
            client.execute_line("HSET user:1 role admin").unwrap(),
            CacheResponse::Int(1)
        );
        assert_eq!(
            client.execute_line("HGET user:1 name").unwrap(),
            CacheResponse::Bulk(b"Alice".to_vec())
        );

        // 3. Lists
        assert_eq!(
            client.execute_line("RPUSH queue job1 job2 job3").unwrap(),
            CacheResponse::Int(3)
        );
        assert_eq!(
            client.execute_line("LPOP queue").unwrap(),
            CacheResponse::Bulk(b"job1".to_vec())
        );
        assert_eq!(
            client.execute_line("LRANGE queue 0 -1").unwrap(),
            CacheResponse::Array(vec![
                CacheResponse::Bulk(b"job2".to_vec()),
                CacheResponse::Bulk(b"job3".to_vec()),
            ])
        );

        // 4. Sets
        assert_eq!(
            client.execute_line("SADD tags rust db cache rust").unwrap(),
            CacheResponse::Int(3)
        );
        assert_eq!(
            client.execute_line("SISMEMBER tags cache").unwrap(),
            CacheResponse::Int(1)
        );

        // 5. Native Lin query over `cache_kv` and `docs`
        let CacheResponse::Text(lin_out) =
            client.execute_line("LIN cache_kv | { k, vtype }").unwrap()
        else {
            panic!("expected text from LIN query");
        };
        assert!(lin_out.contains("counter"));
        assert!(lin_out.contains("user:1"));
        assert!(lin_out.contains("queue"));
        assert!(lin_out.contains("tags"));

        node.stop();
    }

    #[test]
    fn test_lru_eviction_and_ttl() {
        let (node, addr) = spawn_test_node("node-lru", 3);
        let mut client = CacheClient::connect(&addr).unwrap();

        client.execute_line("SET k1 v1").unwrap();
        client.execute_line("SET k2 v2").unwrap();
        client.execute_line("SET k3 v3").unwrap();
        // Touch k1 so k2 becomes least-recently-used
        assert_eq!(
            client.execute_line("GET k1").unwrap(),
            CacheResponse::Bulk(b"v1".to_vec())
        );
        // Inserting k4 exceeds max_entries=3 -> should evict k2
        client.execute_line("SET k4 v4").unwrap();
        assert_eq!(client.execute_line("GET k2").unwrap(), CacheResponse::Nil);
        assert_eq!(
            client.execute_line("GET k1").unwrap(),
            CacheResponse::Bulk(b"v1".to_vec())
        );

        // Short TTL expiration
        client.execute_line("SET temp secret PX 40").unwrap();
        thread::sleep(Duration::from_millis(60));
        assert_eq!(client.execute_line("GET temp").unwrap(), CacheResponse::Nil);

        node.stop();
    }

    #[test]
    fn test_multi_node_cluster_sharding_gossip_raft_and_pubsub() {
        let (n1, addr1) = spawn_test_node("node-1", 1000);
        let (n2, addr2) = spawn_test_node("node-2", 1000);
        let (n3, addr3) = spawn_test_node("node-3", 1000);

        let mut c1 = CacheClient::connect(&addr1).unwrap();
        let mut c2 = CacheClient::connect(&addr2).unwrap();
        let mut c3 = CacheClient::connect(&addr3).unwrap();

        // Join nodes via CLUSTER MEET
        assert_eq!(
            c1.execute_line(&format!("CLUSTER MEET node-2 {addr2}"))
                .unwrap(),
            CacheResponse::Ok
        );
        assert_eq!(
            c1.execute_line(&format!("CLUSTER MEET node-3 {addr3}"))
                .unwrap(),
            CacheResponse::Ok
        );
        // Allow gossip to converge across all 3 nodes
        let _ = n2.gossip_once_with(&addr1);
        let _ = n3.gossip_once_with(&addr1);
        let _ = n2.gossip_once_with(&addr3);

        // Verify cluster membership sees 3 nodes
        let CacheResponse::Text(nodes_txt) = c2.execute_line("CLUSTER NODES").unwrap() else {
            panic!("expected text");
        };
        assert!(nodes_txt.contains("node-1"));
        assert!(nodes_txt.contains("node-2"));
        assert!(nodes_txt.contains("node-3"));

        // Trigger Raft leader election on node-1
        let CacheResponse::Text(elect_txt) = c1.execute_line("RAFT ELECT").unwrap() else {
            panic!("expected election status");
        };
        assert!(elect_txt.contains("election_won=true"));

        // Write 20 keys through node-1; Consistent Hashing routes each key to its primary shard,
        // and Raft replicates mutations across peers!
        for i in 0..20 {
            let cmd = format!("SET item:{i} val-{i}");
            assert_eq!(c1.execute_line(&cmd).unwrap(), CacheResponse::Ok);
        }

        // Read all 20 keys from node-3 (transparent shard routing / replicated state)
        for i in 0..20 {
            let resp = c3.execute_line(&format!("GET item:{i}")).unwrap();
            assert_eq!(resp, CacheResponse::Bulk(format!("val-{i}").into_bytes()));
        }

        // Test Cross-Node Pub/Sub: subscribe on node-3, publish on node-1
        let addr3_sub = addr3.clone();
        let sub_handle = thread::spawn(move || {
            let mut sub_client = CacheClient::connect(&addr3_sub).unwrap();
            let ack = sub_client.execute_line("SUBSCRIBE events").unwrap();
            assert!(matches!(ack, CacheResponse::Text(_)));
            sub_client.read_next().unwrap()
        });

        thread::sleep(Duration::from_millis(50));
        c1.execute_line("PUBLISH events \"hello-cluster\"").unwrap();

        let received = sub_handle.join().unwrap();
        assert_eq!(
            received,
            CacheResponse::PubSubMessage {
                channel: "events".to_string(),
                payload: b"hello-cluster".to_vec(),
                origin_node: "node-1".to_string(),
            }
        );

        n1.stop();
        n2.stop();
        n3.stop();
    }

    #[test]
    fn test_durable_persistence_across_restart() {
        let tmp_dir = std::env::temp_dir().join(format!("lin_cacha_test_{}", unix_ms()));
        let _ = std::fs::remove_dir_all(&tmp_dir);

        let cfg = CacheConfig {
            node_id: "durable-1".to_string(),
            listen_addr: "127.0.0.1:0".to_string(),
            ..CacheConfig::default()
        };

        {
            let node = CacheNode::open_durable(&tmp_dir, cfg.clone()).unwrap();
            assert_eq!(
                node.execute(parse_cli_command("SET persist_str \"survives_restart\"").unwrap()),
                CacheResponse::Ok
            );
            assert_eq!(
                node.execute(parse_cli_command("HSET persist_hash f1 val1").unwrap()),
                CacheResponse::Int(1)
            );
            assert_eq!(
                node.execute(parse_cli_command("SADD persist_set m1 m2").unwrap()),
                CacheResponse::Int(2)
            );
            // Checkpoint & close underlying Db cleanly
            let mut st = node.state.lock().unwrap();
            st.db.close().unwrap();
        }

        // Reopen durable CacheNode from the same `lin::Db` directory
        {
            let node2 = CacheNode::open_durable(&tmp_dir, cfg).unwrap();
            assert_eq!(
                node2.execute(parse_cli_command("GET persist_str").unwrap()),
                CacheResponse::Bulk(b"survives_restart".to_vec())
            );
            assert_eq!(
                node2.execute(parse_cli_command("HGET persist_hash f1").unwrap()),
                CacheResponse::Bulk(b"val1".to_vec())
            );
            assert_eq!(
                node2.execute(parse_cli_command("SISMEMBER persist_set m2").unwrap()),
                CacheResponse::Int(1)
            );
            let mut st = node2.state.lock().unwrap();
            st.db.close().unwrap();
        }

        let _ = std::fs::remove_dir_all(&tmp_dir);
    }
}
