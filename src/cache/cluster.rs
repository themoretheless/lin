use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use super::protocol::CacheValue;
use super::{DEFAULT_VNODES, unix_ms};

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
