//! Distributed Cache (`cacha`) built natively on top of `lin::Db`.
//!
//! Combines:
//! 1. **In-Memory Data Structures + LRU Eviction + TTL** (Strings, Hashes, Lists, Sets)
//!    with durable persistence into `lin::Db` (`col cache_kv`) and direct Lin query execution (`LIN <query>`).
//! 2. **Consistent Hashing Ring** with virtual nodes (`CRC32`) for automatic key sharding and transparent routing across cluster nodes.
//! 3. **Gossip Protocol** for peer discovery, membership heartbeats, failure detection, and cluster-wide **Pub/Sub** (`PUBLISH` / `SUBSCRIBE`).
//! 4. **Raft Consensus & WAL Replication** (Leader Election, Term, Log Replication, Commit Index, plus `lin` WAL frame shipping).
//! 5. **Custom Binary TCP Protocol** (`LINC\x01` + `u32` LE length + MessagePack payload) and interactive CLI client (`lin cache cli`).

use std::time::{SystemTime, UNIX_EPOCH};

use crate::error::Error;

/// Protocol magic for `lin::cache` binary framing (`LINC` v1).
pub const CACHE_MAGIC: [u8; 4] = *b"LINC";
/// Protocol version byte.
pub const CACHE_VERSION: u8 = 1;
/// Max frame size (64 MiB).
pub const MAX_FRAME_LEN: u32 = 64 * 1024 * 1024;
/// Default number of virtual nodes per physical node on the consistent hash ring.
pub const DEFAULT_VNODES: usize = 64;

pub(crate) fn cache_err(msg: impl std::fmt::Display) -> Error {
    Error::runtime(format!("cache: {msg}"))
}

pub(crate) fn unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

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

pub mod client;
pub mod cluster;
pub mod protocol;
mod raft;
mod server;
mod store;
#[cfg(test)]
mod tests;

pub use client::{CacheClient, parse_cli_command, send_rpc, tokenize_cli};
pub use cluster::*;
pub use protocol::*;
pub use server::CacheNode;
