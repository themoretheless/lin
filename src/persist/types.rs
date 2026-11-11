//! Core persistence data structures, WAL record envelopes, and snapshot schemas.

use std::fs::File;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::store::{Edge, Row};

use super::codec::EncoderWorker;

pub const HEAD_NAME: &str = "head";
pub const LOG_NAME: &str = "log";
pub const SNAPSHOT_NAME: &str = "snapshot";
pub const LOCK_NAME: &str = "LOCK";
/// Shared fence for multi-process readers; writer takes exclusive only during checkpoint.
pub const FENCE_NAME: &str = "FENCE";
/// On-disk snapshot: `LIN\x04` + MessagePack(`Snapshot`). Legacy JSON still reads.
pub const SNAPSHOT_MAGIC: [u8; 4] = *b"LIN\x04";
/// Portable backup uses the same MessagePack envelope (self-contained, no cold refs).
#[allow(dead_code)]
pub const BACKUP_MAGIC: [u8; 4] = *b"LIN\x04";
pub const SNAPSHOT_EVERY: u32 = 32;
pub(super) const MAX_RECORD: u32 = 16 * 1024 * 1024;
/// New WAL framing magic (`LIN` + version).
/// `\x01` = MessagePack(LogRecord); `\x02` = raw columnar hot packs.
pub(super) const LOG_MAGIC_V1: [u8; 4] = *b"LIN\x01";
pub(super) const LOG_MAGIC_V2: [u8; 4] = *b"LIN\x02";
// Internal codec identity; new sparse records are only written inside LIN\x06.
pub(super) const LOG_MAGIC_SPARSE: [u8; 4] = *b"LIN\x03";
// Read capability only; default writers do not emit this codec yet.
pub(super) const LOG_MAGIC_DICTIONARY: [u8; 4] = *b"LIN\x05";
pub(super) const MAX_SPARSE_DECODED_BYTES: usize = 64 * 1024 * 1024;
pub(super) const SPARSE_VECTOR_THRESHOLD: usize = 64 * 1024;
/// Checksummed envelope: codec byte (1/2/3), encoded record, CRC32 of codec + record.
pub(super) const LOG_MAGIC_CHECKSUM: [u8; 4] = *b"LIN\x06";
pub(super) const V2_FACTS_BULK: u8 = 1;
pub(super) const V2_INSERT_COLS: u8 = 2;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Head {
    pub r#gen: u64,
    pub catalog_hash: String,
    pub embed_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogRecord {
    pub r#gen: u64,
    pub next_id: u64,
    pub pack: Pack,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Pack {
    Insert {
        collection: String,
        row: Row,
        edges: Vec<Edge>,
    },
    /// One durable record for many inserts (avoids Batch-of-N JSON tax).
    InsertBulk {
        collection: String,
        rows: Vec<Row>,
        edges: Vec<Edge>,
    },
    /// Columnar insert — denser WAL than N× row maps (MessagePack / JSON).
    InsertCols {
        collection: String,
        /// Parallel field names / column payloads; all columns length `n`.
        fields: Vec<String>,
        cols: Vec<ColData>,
        n: u32,
        edges: Vec<Edge>,
    },
    AppendFact {
        row: Row,
    },
    /// Columnar facts — far denser than N× `{s,p,o}` Cell maps.
    AppendFactsBulk {
        s: Vec<std::sync::Arc<str>>,
        p: Vec<std::sync::Arc<str>>,
        o: Vec<std::sync::Arc<str>>,
    },
    AppendEdge {
        rel: String,
        from: String,
        to: String,
    },
    AppendEdgesBulk {
        rel: Vec<String>,
        from: Vec<String>,
        to: Vec<String>,
    },
    Update {
        collection: String,
        rows: Vec<Row>,
    },
    Reembed,
    Delete {
        collection: String,
        rows: Vec<Row>,
    },
    DeleteEdge {
        rel: String,
        from: String,
        to: String,
    },
    SchemaCol {
        name: String,
        append: bool,
        fields: Vec<(String, String)>,
    },
    SchemaRel {
        name: String,
        stub: bool,
        reverse_of: Option<String>,
    },
    SchemaIndex {
        collection: String,
        unique: bool,
        fields: Vec<String>,
        #[serde(default)]
        pred_src: Option<String>,
    },
    SchemaFilter {
        collection: String,
        /// `None` drops the catalog filter (`unfilter`).
        pred_src: Option<String>,
    },
    SchemaOwned {
        name: String,
        fields: Vec<(String, String)>,
    },
    Batch {
        packs: Vec<Pack>,
    },
}

impl Pack {
    pub fn byte_size(&self) -> usize {
        match self {
            Pack::Insert {
                collection,
                row,
                edges,
            } => collection.len() + crate::store::row_byte_size(row) + edges.len() * 64,
            Pack::InsertBulk {
                collection,
                rows,
                edges,
            } => {
                collection.len()
                    + rows.iter().map(crate::store::row_byte_size).sum::<usize>()
                    + edges.len() * 64
            }
            Pack::InsertCols {
                collection,
                fields,
                cols,
                edges,
                ..
            } => {
                collection.len()
                    + fields.iter().map(|f| f.len()).sum::<usize>()
                    + cols
                        .iter()
                        .map(|c| match c {
                            ColData::Text(v) => v.iter().map(|s| s.len()).sum(),
                            ColData::Int(v) => v.len() * 8,
                            ColData::Float(v) => v.len() * 8,
                            ColData::Bool(v) => v.len(),
                            ColData::Time(v) => v.len() * 8,
                            ColData::Vec(v) => v.len() * 32,
                            ColData::Null => 0,
                        })
                        .sum::<usize>()
                    + edges.len() * 64
            }
            Pack::AppendFact { row } => crate::store::row_byte_size(row),
            Pack::AppendFactsBulk { s, p, o } => {
                s.iter().map(|x| x.len()).sum::<usize>()
                    + p.iter().map(|x| x.len()).sum::<usize>()
                    + o.iter().map(|x| x.len()).sum::<usize>()
            }
            Pack::AppendEdge { rel, from, to } => rel.len() + from.len() + to.len(),
            Pack::AppendEdgesBulk { rel, from, to } => {
                rel.iter().map(|x| x.len()).sum::<usize>()
                    + from.iter().map(|x| x.len()).sum::<usize>()
                    + to.iter().map(|x| x.len()).sum::<usize>()
            }
            Pack::Update { collection, rows } | Pack::Delete { collection, rows } => {
                collection.len() + rows.iter().map(crate::store::row_byte_size).sum::<usize>()
            }
            Pack::DeleteEdge { rel, from, to } => rel.len() + from.len() + to.len(),
            Pack::Batch { packs } => packs.iter().map(|p| p.byte_size()).sum(),
            _ => 128,
        }
    }
}

/// Parallel column payload for [`Pack::InsertCols`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "t", content = "v")]
pub enum ColData {
    Text(Vec<std::sync::Arc<str>>),
    Int(Vec<i64>),
    Float(Vec<f64>),
    Bool(Vec<bool>),
    Time(Vec<i64>),
    /// Per-row dense vectors (`None` / empty = null).
    Vec(Vec<Option<std::sync::Arc<[f32]>>>),
    /// Homogeneous null column of length `n` (n stored in InsertCols.n).
    Null,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ColSnap {
    pub name: String,
    pub append_only: bool,
    pub fields: Vec<(String, String)>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RelSnap {
    pub name: String,
    pub stub: bool,
    pub reverse_of: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IndexSnap {
    pub collection: String,
    pub unique: bool,
    pub fields: Vec<String>,
    #[serde(default)]
    pub pred_src: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub r#gen: u64,
    pub embed_id: String,
    pub next_id: u64,
    pub catalog_hash: String,
    pub log_offset: u64,
    pub collections: std::collections::BTreeMap<String, Vec<Row>>,
    pub edges: Vec<Edge>,
    #[serde(default)]
    pub extra_collections: std::collections::BTreeMap<String, ColSnap>,
    #[serde(default)]
    pub extra_rels: std::collections::BTreeMap<String, RelSnap>,
    #[serde(default)]
    pub extra_indexes: std::collections::BTreeMap<String, IndexSnap>,
    #[serde(default)]
    pub extra_filters: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub extra_owned: std::collections::BTreeMap<String, Vec<(String, String)>>,
    /// Names that also have a `cold/<name>.bin` mmap cache. Rows stay inlined
    /// in the snapshot (self-contained); cold files are optional decode accel.
    #[serde(default)]
    pub cold_collections: Vec<String>,
}

/// Zero-copy borrowed snapshot view for serialization without cloning collections.
#[derive(Debug, Clone, Serialize)]
pub struct SnapshotRef<'a> {
    pub r#gen: u64,
    pub embed_id: &'a str,
    pub next_id: u64,
    pub catalog_hash: &'a str,
    pub log_offset: u64,
    pub collections: &'a std::collections::BTreeMap<String, Vec<Row>>,
    pub edges: &'a [Edge],
    #[serde(default)]
    pub extra_collections: &'a std::collections::BTreeMap<String, ColSnap>,
    #[serde(default)]
    pub extra_rels: &'a std::collections::BTreeMap<String, RelSnap>,
    #[serde(default)]
    pub extra_indexes: &'a std::collections::BTreeMap<String, IndexSnap>,
    #[serde(default)]
    pub extra_filters: &'a std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub extra_owned: &'a std::collections::BTreeMap<String, Vec<(String, String)>>,
    #[serde(default)]
    pub cold_collections: &'a [String],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct SnapshotHeader {
    pub r#gen: u64,
    pub embed_id: String,
    pub next_id: u64,
    pub catalog_hash: String,
    pub log_offset: u64,
    pub edges: Vec<Edge>,
    #[serde(default)]
    pub extra_collections: std::collections::BTreeMap<String, ColSnap>,
    #[serde(default)]
    pub extra_rels: std::collections::BTreeMap<String, RelSnap>,
    #[serde(default)]
    pub extra_indexes: std::collections::BTreeMap<String, IndexSnap>,
    #[serde(default)]
    pub extra_filters: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub extra_owned: std::collections::BTreeMap<String, Vec<(String, String)>>,
    #[serde(default)]
    pub cold_collections: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(super) struct SnapshotHeaderRef<'a> {
    pub r#gen: u64,
    pub embed_id: &'a str,
    pub next_id: u64,
    pub catalog_hash: &'a str,
    pub log_offset: u64,
    pub edges: &'a [Edge],
    #[serde(default)]
    pub extra_collections: &'a std::collections::BTreeMap<String, ColSnap>,
    #[serde(default)]
    pub extra_rels: &'a std::collections::BTreeMap<String, RelSnap>,
    #[serde(default)]
    pub extra_indexes: &'a std::collections::BTreeMap<String, IndexSnap>,
    #[serde(default)]
    pub extra_filters: &'a std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub extra_owned: &'a std::collections::BTreeMap<String, Vec<(String, String)>>,
    #[serde(default)]
    pub cold_collections: &'a [String],
}

/// When to durability-flush the WAL.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SyncMode {
    /// Flush after every commit frame (default; crash-safe when `run` returns).
    #[default]
    Full,
    /// Flush only on checkpoint / close. Faster; **crash may lose uncheckpointed
    /// commits**. Opt-in via [`OpenOpts`] / [`crate::exec::Db::with_sync_mode`] —
    /// do not treat as default durable.
    Normal,
}

/// Options for durable [`Store::open_with`] / [`crate::exec::Db::open_with`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct OpenMemOpts {
    pub sync: SyncMode,
    /// Spill large collections to `cold/*.bin` on checkpoint; load via mmap.
    pub cold: bool,
}

#[derive(Debug)]
pub struct Persist {
    pub dir: PathBuf,
    pub log: File,
    /// Advisory exclusive flock on `LOCK` — released when Persist drops.
    pub lock: File,
    pub(crate) lock_owner_pid: u32,
    pub catalog_hash: String,
    pub writes_since_snapshot: u32,
    /// Reused WAL encode buffer (avoids per-commit alloc).
    pub(crate) encode_buf: Vec<u8>,
    pub(crate) encoder: Option<std::sync::Arc<EncoderWorker>>,
    pub sync: SyncMode,
    /// Spill large collections to mmap cold files on checkpoint.
    pub cold: bool,
    /// Cached `log` file length (avoids metadata() on every commit).
    pub log_bytes: u64,
    /// Monotonic `(gen, file_offset)` index of frames currently in `log` (cleared on compact).
    pub(crate) wal_index: Vec<(u64, u64)>,
}

impl Persist {
    pub(super) fn release_writer_lock(&self) {
        // flock belongs to the open file description, which fork inherits.
        // Closing our File alone leaves the lock held by a child until exec/exit.
        // Only the process that acquired it may explicitly release it: a child
        // dropping its inherited copy must not unlock the parent's writer.
        #[cfg(unix)]
        if std::process::id() == self.lock_owner_pid {
            use std::os::fd::AsRawFd;
            unsafe {
                libc::flock(self.lock.as_raw_fd(), libc::LOCK_UN);
            }
        }
    }
}

impl Drop for Persist {
    fn drop(&mut self) {
        self.release_writer_lock();
    }
}

