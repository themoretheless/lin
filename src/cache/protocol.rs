use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::io::{Read, Write};

use serde::{Deserialize, Serialize};

use crate::error::Error;

use super::cluster::{NodeMember, PubSubEvent, RaftLogEntry};
use super::{CACHE_MAGIC, CACHE_VERSION, MAX_FRAME_LEN, cache_err};

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
