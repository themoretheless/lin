//! On-disk layout of a durable Lin data dir (default `./.lin`):
//!
//! ```text
//! <data>/
//!   head       JSON object: gen, catalog_hash, embed_id
//!   log        append-only checksummed records (`LIN\x06`; reads v1/v2)
//!   snapshot   `LIN\x04` + MessagePack(Snapshot); legacy JSON still reads
//!   fts/       posting lists per collection (`LIN\x05`), written on checkpoint
//!   cold/      optional mmap cache of large collections (same rows also inlined
//!              in snapshot — self-contained)
//! ```
//!
//! Log record framing (v1 binary, dual-read with legacy JSON):
//! - **v1:** magic `LIN\x01` + `u32` LE payload length + MessagePack(`LogRecord`)
//! - **v2:** magic `LIN\x02` + raw columnar hot packs
//! - **v3:** magic `LIN\x06` + `u32` LE envelope length + codec byte (1/2/3/5)
//!   + encoded payload + CRC32 LE of codec/payload; all new writes use v3
//! - **legacy:** `u32` LE length + JSON(`LogRecord`) — still replayed on open
//!
//! A truncated trailing record is ignored. After replay the log is truncated
//! to the last complete record. Checkpoint compacts the log to empty.
//! Portable backup = self-contained `LIN\x04` MessagePack (never cold stubs).
//! FTS blobs are local accel; missing/stale files rebuild on open.

use std::cell::RefCell;
use std::fs::{self, File, OpenOptions};
use std::io::{self, IoSlice, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::Error;
use crate::store::{Edge, Row};

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
const MAX_RECORD: u32 = 16 * 1024 * 1024;
/// New WAL framing magic (`LIN` + version).
/// `\x01` = MessagePack(LogRecord); `\x02` = raw columnar hot packs.
const LOG_MAGIC_V1: [u8; 4] = *b"LIN\x01";
const LOG_MAGIC_V2: [u8; 4] = *b"LIN\x02";
// Internal codec identity; new sparse records are only written inside LIN\x06.
const LOG_MAGIC_SPARSE: [u8; 4] = *b"LIN\x03";
// Bit-exact dictionary column codec inside the checksum envelope.
const LOG_MAGIC_DICTIONARY: [u8; 4] = *b"LIN\x05";
const MAX_SPARSE_DECODED_BYTES: usize = 64 * 1024 * 1024;
const SPARSE_VECTOR_THRESHOLD: usize = 64 * 1024;
/// Checksummed envelope: codec byte (1/2/3/5), encoded record, CRC32 of codec + record.
const LOG_MAGIC_CHECKSUM: [u8; 4] = *b"LIN\x06";
const V2_FACTS_BULK: u8 = 1;
const V2_INSERT_COLS: u8 = 2;

thread_local! {
    static FRAME_WRITE_SCRATCH: RefCell<Vec<u8>> = RefCell::new(Vec::with_capacity(64 * 1024));
}

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
        s: Vec<String>,
        p: Vec<String>,
        o: Vec<String>,
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
    pub catalog_hash: String,
    pub writes_since_snapshot: u32,
    /// Reused WAL encode buffer (avoids per-commit alloc).
    pub(crate) encode_buf: Vec<u8>,
    pub sync: SyncMode,
    /// Spill large collections to mmap cold files on checkpoint.
    pub cold: bool,
    /// Cached `log` file length (avoids metadata() on every commit).
    pub log_bytes: u64,
}

pub fn io_err(e: impl std::fmt::Display) -> Error {
    Error::runtime(format!("persist: {e}"))
}

pub fn ensure_dir(path: &Path) -> Result<(), Error> {
    fs::create_dir_all(path).map_err(io_err)
}

pub fn head_path(dir: &Path) -> PathBuf {
    dir.join(HEAD_NAME)
}

pub fn log_path(dir: &Path) -> PathBuf {
    dir.join(LOG_NAME)
}

pub fn snapshot_path(dir: &Path) -> PathBuf {
    dir.join(SNAPSHOT_NAME)
}

pub fn lock_path(dir: &Path) -> PathBuf {
    dir.join(LOCK_NAME)
}

pub fn fence_path(dir: &Path) -> PathBuf {
    dir.join(FENCE_NAME)
}

pub fn log_len(dir: &Path) -> Result<u64, Error> {
    let path = log_path(dir);
    if !path.exists() {
        return Ok(0);
    }
    Ok(fs::metadata(&path).map_err(io_err)?.len())
}

/// Exclusive advisory lock for a writer (writer↔writer only).
pub fn acquire_writer_lock(dir: &Path) -> Result<File, Error> {
    ensure_dir(dir)?;
    let f = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(lock_path(dir))
        .map_err(io_err)?;
    flock_nb(&f, true).map_err(|e| {
        if e.kind() == io::ErrorKind::WouldBlock {
            Error::runtime("data dir locked by another writer")
        } else {
            io_err(e)
        }
    })?;
    // Ensure fence file exists for readers.
    let _ = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(fence_path(dir))
        .map_err(io_err)?;
    Ok(f)
}

/// Shared fence lock for cold readers — **compatible with a live writer**.
/// Fails fast (never waits) while a writer holds the exclusive fence during a
/// checkpoint: a reader that finds the dir mid-publish retries at its own pace.
pub fn acquire_reader_lock(dir: &Path) -> Result<File, Error> {
    let path = fence_path(dir);
    if !path.exists() {
        let lock = lock_path(dir);
        if !lock.exists() && !dir.exists() {
            return Err(Error::runtime(format!(
                "data dir not found: {}",
                dir.display()
            )));
        }
        let _ = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&path)
            .map_err(io_err)?;
    }
    let f = OpenOptions::new().read(true).open(&path).map_err(io_err)?;
    flock_nb(&f, false).map_err(|e| {
        if e.kind() == io::ErrorKind::WouldBlock {
            Error::runtime("data dir checkpoint in progress")
        } else {
            io_err(e)
        }
    })?;
    Ok(f)
}

/// Exclusive fence for publishing a snapshot (waits out shared readers).
pub fn acquire_fence_exclusive(dir: &Path) -> Result<File, Error> {
    let f = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(fence_path(dir))
        .map_err(io_err)?;
    // Blocking exclusive — checkpoint waits for open_read holders.
    #[cfg(unix)]
    {
        use std::os::unix::io::AsRawFd;
        let rc = unsafe { libc::flock(f.as_raw_fd(), libc::LOCK_EX) };
        if rc != 0 {
            return Err(io_err(std::io::Error::last_os_error()));
        }
    }
    Ok(f)
}

#[cfg(unix)]
fn flock_nb(f: &File, exclusive: bool) -> io::Result<()> {
    use std::os::unix::io::AsRawFd;
    let op = if exclusive {
        libc::LOCK_EX | libc::LOCK_NB
    } else {
        libc::LOCK_SH | libc::LOCK_NB
    };
    let rc = unsafe { libc::flock(f.as_raw_fd(), op) };
    if rc == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(not(unix))]
fn flock_nb(_f: &File, _exclusive: bool) -> io::Result<()> {
    // Best-effort: no advisory flock on this platform.
    Ok(())
}

pub fn read_head(dir: &Path) -> Result<Option<Head>, Error> {
    let path = head_path(dir);
    if !path.exists() {
        return Ok(None);
    }
    let bytes = fs::read(&path).map_err(io_err)?;
    if bytes.is_empty() {
        return Ok(None);
    }
    let head = serde_json::from_slice(&bytes).map_err(io_err)?;
    Ok(Some(head))
}

pub fn write_head(dir: &Path, head: &Head) -> Result<(), Error> {
    let bytes = serde_json::to_vec(head).map_err(io_err)?;
    atomic_write(&head_path(dir), &bytes)
}

/// Update `head` without fsync — log is authoritative; head is refreshed durably on snapshot/close.
/// Soft head update — kept for tools; hot path skips this (head on checkpoint/close).
#[allow(dead_code)]
pub fn write_head_soft(dir: &Path, head: &Head) -> Result<(), Error> {
    let bytes = serde_json::to_vec(head).map_err(io_err)?;
    let path = head_path(dir);
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, &bytes).map_err(io_err)?;
    fs::rename(&tmp, &path).map_err(io_err)?;
    Ok(())
}

pub fn read_snapshot(dir: &Path) -> Result<Option<Snapshot>, Error> {
    let path = snapshot_path(dir);
    if !path.exists() {
        return Ok(None);
    }
    let file = fs::File::open(&path).map_err(io_err)?;
    let meta = file.metadata().map_err(io_err)?;
    let len = meta.len();
    if len == 0 {
        return Ok(None);
    }
    if len >= 16 * 1024 {
        let mmap = unsafe { memmap2::MmapOptions::new().map(&file) }.map_err(io_err)?;
        let _ = mmap.advise(memmap2::Advice::Sequential);
        decode_snapshot_bytes(&mmap)
            .map(Some)
            .ok_or_else(|| io_err("corrupt snapshot"))
    } else {
        use std::io::Read;
        let mut bytes = Vec::with_capacity(len as usize);
        let mut reader = std::io::BufReader::new(file);
        reader.read_to_end(&mut bytes).map_err(io_err)?;
        decode_snapshot_bytes(&bytes)
            .map(Some)
            .ok_or_else(|| io_err("corrupt snapshot"))
    }
}

/// Write snapshot directly to a file via a temporary file without intermediate full-image in-memory buffering.
pub fn write_snapshot_ref_to_file(path: &Path, snap: &SnapshotRef<'_>) -> Result<(), Error> {
    let tmp = path.with_extension("bin.tmp");
    let result = (|| -> Result<(), Error> {
        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&tmp)
            .map_err(io_err)?;
        let mut writer = std::io::BufWriter::with_capacity(128 * 1024, file);
        writer.write_all(&SNAPSHOT_MAGIC).map_err(io_err)?;
        rmp_serde::encode::write_named(&mut writer, snap).map_err(io_err)?;
        writer.flush().map_err(io_err)?;
        let file = writer.into_inner().map_err(io_err)?;
        durable_sync(&file)
            .or_else(|_| file.sync_all())
            .map_err(io_err)?;
        drop(file);
        fs::rename(&tmp, path).map_err(io_err)?;
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            sync_dir(parent).map_err(io_err)?;
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

pub fn write_snapshot(dir: &Path, snap: &Snapshot) -> Result<(), Error> {
    let snap_ref = SnapshotRef {
        r#gen: snap.r#gen,
        embed_id: &snap.embed_id,
        next_id: snap.next_id,
        catalog_hash: &snap.catalog_hash,
        log_offset: snap.log_offset,
        collections: &snap.collections,
        edges: &snap.edges,
        extra_collections: &snap.extra_collections,
        extra_rels: &snap.extra_rels,
        extra_indexes: &snap.extra_indexes,
        extra_filters: &snap.extra_filters,
        extra_owned: &snap.extra_owned,
        cold_collections: &snap.cold_collections,
    };
    write_snapshot_ref(dir, &snap_ref)
}

pub fn write_snapshot_ref(dir: &Path, snap: &SnapshotRef<'_>) -> Result<(), Error> {
    write_snapshot_ref_to_file(&snapshot_path(dir), snap)
}

/// Portable backup: always self-contained MessagePack (`LIN\x04`).
pub fn write_backup(path: &Path, snap: &Snapshot) -> Result<(), Error> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent).map_err(io_err)?;
    }
    // Portable: never ship cold stubs — rows must already be inlined.
    // Use SnapshotRef with cold_collections: &[] to avoid cloning collections/edges.
    let snap_ref = SnapshotRef {
        r#gen: snap.r#gen,
        embed_id: &snap.embed_id,
        next_id: snap.next_id,
        catalog_hash: &snap.catalog_hash,
        log_offset: snap.log_offset,
        collections: &snap.collections,
        edges: &snap.edges,
        extra_collections: &snap.extra_collections,
        extra_rels: &snap.extra_rels,
        extra_indexes: &snap.extra_indexes,
        extra_filters: &snap.extra_filters,
        extra_owned: &snap.extra_owned,
        cold_collections: &[],
    };
    write_snapshot_ref_to_file(path, &snap_ref)
}

pub fn read_backup(path: &Path) -> Result<Snapshot, Error> {
    let file = fs::File::open(path).map_err(io_err)?;
    let meta = file.metadata().map_err(io_err)?;
    let len = meta.len();
    if len == 0 {
        return Err(io_err("empty backup"));
    }
    if len >= 16 * 1024 {
        let mmap = unsafe { memmap2::MmapOptions::new().map(&file) }.map_err(io_err)?;
        let _ = mmap.advise(memmap2::Advice::Sequential);
        decode_snapshot_bytes(&mmap).ok_or_else(|| io_err("corrupt backup"))
    } else {
        use std::io::Read;
        let mut bytes = Vec::with_capacity(len as usize);
        let mut reader = std::io::BufReader::new(file);
        reader.read_to_end(&mut bytes).map_err(io_err)?;
        decode_snapshot_bytes(&bytes).ok_or_else(|| io_err("corrupt backup"))
    }
}

/// Expand cold file refs into inlined collections (for backup / portable copy).
/// Expand cold file refs into inlined collections (for tools / dir copy).
#[allow(dead_code)]
pub fn expand_cold_into(dir: &Path, snap: &mut Snapshot) -> Result<(), Error> {
    for name in snap.cold_collections.clone() {
        let need = snap
            .collections
            .get(&name)
            .map(|c| c.is_empty())
            .unwrap_or(true);
        if need {
            let col = crate::cold::map_cold(dir, &name)?;
            snap.collections.insert(name, col.into_rows()?);
        }
    }
    snap.cold_collections.clear();
    Ok(())
}

#[allow(dead_code)]
pub fn encode_snapshot_ref_bytes(snap: &SnapshotRef<'_>) -> Result<Vec<u8>, Error> {
    let mut out = Vec::with_capacity(64 * 1024);
    out.extend_from_slice(&SNAPSHOT_MAGIC);
    rmp_serde::encode::write_named(&mut out, snap).map_err(io_err)?;
    Ok(out)
}

#[allow(dead_code)]
fn encode_snapshot_bytes(snap: &Snapshot) -> Result<Vec<u8>, Error> {
    let mut out = Vec::with_capacity(64 * 1024);
    out.extend_from_slice(&SNAPSHOT_MAGIC);
    rmp_serde::encode::write_named(&mut out, snap).map_err(io_err)?;
    Ok(out)
}

/// Encode snapshot with optional compression (feature-gated)
#[cfg(any(feature = "compress-flate", feature = "compress-lz4"))]
#[allow(dead_code)]
pub fn encode_snapshot_compressed(snap: &Snapshot) -> Result<Vec<u8>, Error> {
    let mut body = Vec::new();
    rmp_serde::encode::write_named(&mut body, snap).map_err(io_err)?;

    #[cfg(feature = "compress-flate")]
    {
        use flate2::Compression;
        use flate2::write::GzEncoder;
        use std::io::Write;

        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(&body).map_err(io_err)?;
        let compressed = encoder.finish().map_err(io_err)?;

        // Prepend magic byte indicating gzip format
        let mut out = Vec::with_capacity(5 + compressed.len());
        out.push(0x01); // Compressed flag
        out.extend_from_slice(&SNAPSHOT_MAGIC);
        out.extend_from_slice(&compressed);
        Ok(out)
    }

    #[cfg(all(not(feature = "compress-flate"), feature = "compress-lz4"))]
    {
        use lz4::block::{CompressionMode, compress};

        let compressed = compress(&body, Some(CompressionMode::DEFAULT), true).map_err(io_err)?;
        let mut out = Vec::with_capacity(5 + compressed.len());
        out.push(0x02); // LZ4 flag
        out.extend_from_slice(&SNAPSHOT_MAGIC);
        out.extend_from_slice(&compressed);
        return Ok(out);
    }
}

fn decode_snapshot_bytes(bytes: &[u8]) -> Option<Snapshot> {
    if bytes.len() >= 5 && bytes[1..5] == SNAPSHOT_MAGIC {
        let compressor = bytes[0];
        let compressed_data = &bytes[5..];
        match compressor {
            0x01 => {
                #[cfg(feature = "compress-flate")]
                {
                    use flate2::read::GzDecoder;
                    use std::io::Read;

                    let mut decoder = GzDecoder::new(compressed_data);
                    let mut result = Vec::new();
                    decoder.read_to_end(&mut result).ok()?;
                    return rmp_serde::from_slice(&result).ok();
                }
                #[cfg(not(feature = "compress-flate"))]
                {
                    let _ = compressed_data;
                    return None;
                }
            }
            0x02 => {
                #[cfg(feature = "compress-lz4")]
                {
                    let decompressed = lz4::block::decompress(compressed_data, None).ok()?;
                    return rmp_serde::from_slice(&decompressed).ok();
                }
                #[cfg(not(feature = "compress-lz4"))]
                {
                    let _ = compressed_data;
                    return None;
                }
            }
            _ => return None,
        }
    }

    // Uncompressed snapshot or legacy JSON backup
    if bytes.len() >= 4 && bytes[..4] == SNAPSHOT_MAGIC {
        return rmp_serde::from_slice(&bytes[4..]).ok();
    }
    serde_json::from_slice(bytes).ok()
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), Error> {
    write_through_tmp(path, &path.with_extension("tmp"), bytes)?;
    if let Some(parent) = path.parent() {
        sync_dir(parent).map_err(io_err)?;
    }
    Ok(())
}

/// Replace `path` with `bytes` through `tmp`: write, fsync, rename. A failed
/// step removes the temp file, and the `cold`/`fts` prunes sweep whatever a
/// killed process left behind — safe because prune only runs under the
/// exclusive fence, when no live writer can hold a temp open.
pub(crate) fn write_through_tmp(path: &Path, tmp: &Path, bytes: &[u8]) -> Result<(), Error> {
    let result = (|| -> Result<(), Error> {
        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(tmp)
            .map_err(io_err)?;
        file.write_all(bytes).map_err(io_err)?;
        durable_sync(&file)
            .or_else(|_| file.sync_all())
            .map_err(io_err)?;
        drop(file);
        fs::rename(tmp, path).map_err(io_err)
    })();
    if result.is_err() {
        let _ = fs::remove_file(tmp);
    }
    result
}

/// Drop `dir`'s `*.bin` files whose stem is not in `keep`, and every `*.tmp`
/// still sitting there (see [`write_through_tmp`]).
pub(crate) fn prune_bins(dir: &Path, keep: &[String]) -> Result<(), Error> {
    if !dir.exists() {
        return Ok(());
    }
    for ent in fs::read_dir(dir).map_err(io_err)? {
        let ent = ent.map_err(io_err)?;
        let path = ent.path();
        let extension = path.extension().and_then(|e| e.to_str());
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        let stale = match extension {
            Some("tmp") => true,
            Some("bin") => !keep.iter().any(|k| k == stem),
            _ => false,
        };
        if stale {
            let _ = fs::remove_file(&path);
        }
    }
    Ok(())
}

fn sync_dir(dir: &Path) -> io::Result<()> {
    let f = File::open(dir)?;
    f.sync_all()
}

pub fn open_log(dir: &Path) -> Result<File, Error> {
    OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(log_path(dir))
        .map_err(io_err)
}

/// Open log for read-only replay (no create / no truncate).
/// Returns `None` if the log file does not exist yet.
pub fn open_log_read(dir: &Path) -> Result<Option<File>, Error> {
    let path = log_path(dir);
    if !path.exists() {
        return Ok(None);
    }
    OpenOptions::new()
        .read(true)
        .open(&path)
        .map(Some)
        .map_err(io_err)
}

/// Append multiple records atomically with parallel encoding — feature-gated optimization
#[cfg(feature = "parallel")]
#[allow(dead_code)]
pub fn append_batch_records(
    log: &mut File,
    records: &[LogRecord],
    sync: SyncMode,
    log_bytes: &mut u64,
) -> Result<(), Error> {
    use rayon::prelude::*;

    // Encode all records in parallel using rayon
    let encoded: Vec<Result<Vec<u8>, Error>> = records
        .par_iter()
        .map(|rec| {
            let mut buf = Vec::with_capacity(256);
            rmp_serde::encode::write_named(&mut buf, rec).map_err(io_err)?;
            Ok(buf)
        })
        .collect();

    // Write all records sequentially to maintain order
    for (i, result) in encoded.into_iter().enumerate() {
        let payload = result?;

        if payload.len() > MAX_RECORD as usize {
            return Err(io_err(format!("batch record {} exceeds max size", i)));
        }

        // Write with checksum envelope (codec 1 = LOG_MAGIC_V1 MessagePack)
        let len = payload.len() + 5; // codec byte + payload + CRC
        if len > MAX_RECORD as usize {
            return Err(io_err("encoded batch exceeds max size"));
        }

        let codec = LOG_MAGIC_V1[3];
        let mut checksum = crate::crc::Hasher::new();
        checksum.update(&[codec]);
        checksum.update(&payload);
        let checksum_bytes = checksum.finalize().to_le_bytes();

        write_frame_bytes(log, codec, &payload, checksum_bytes, len as u32).map_err(io_err)?;

        *log_bytes += 8 + len as u64;
    }

    if sync == SyncMode::Full {
        durable_sync(log).map_err(io_err)?;
    }

    Ok(())
}

/// Compressed batch write for large payloads — compresses records into a single WAL frame
#[cfg(any(feature = "compress-wal", feature = "parallel"))]
#[allow(dead_code)]
pub fn append_compressed_batch(
    log: &mut File,
    records: &[LogRecord],
    sync: SyncMode,
    log_bytes: &mut u64,
) -> Result<(), Error> {
    #[cfg(feature = "compress-wal")]
    {
        use flate2::Compression;
        use flate2::write::GzEncoder;
        use std::io::Write;

        for rec in records {
            let mut raw = Vec::new();
            rmp_serde::encode::write_named(&mut raw, rec).map_err(io_err)?;
            let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
            encoder.write_all(&raw).map_err(io_err)?;
            let compressed = encoder.finish().map_err(io_err)?;

            // Codec 4 = Gzip-compressed MessagePack LogRecord
            let codec = [4u8];
            let len = 1 + compressed.len() + 4;
            if len > MAX_RECORD as usize {
                return Err(io_err("compressed record exceeds max size"));
            }
            let mut checksum = crate::crc::Hasher::new();
            checksum.update(&codec);
            checksum.update(&compressed);
            let crc = checksum.finalize().to_le_bytes();

            let mut hdr = [0u8; 8];
            hdr[..4].copy_from_slice(&LOG_MAGIC_CHECKSUM);
            hdr[4..].copy_from_slice(&(len as u32).to_le_bytes());
            let slices = [
                IoSlice::new(&hdr),
                IoSlice::new(&codec),
                IoSlice::new(&compressed),
                IoSlice::new(&crc),
            ];
            write_all_vectored(log, &slices).map_err(io_err)?;
            *log_bytes += 8 + len as u64;
        }

        if sync == SyncMode::Full {
            durable_sync(log).map_err(io_err)?;
        }

        Ok(())
    }

    #[cfg(not(feature = "compress-wal"))]
    {
        // Fallback to normal batch if compression not enabled
        append_batch_records(log, records, sync, log_bytes)
    }
}

pub fn append_record(
    log: &mut File,
    rec: &LogRecord,
    buf: &mut Vec<u8>,
    sync: SyncMode,
    log_bytes: &mut u64,
) -> Result<(), Error> {
    match &rec.pack {
        Pack::AppendFactsBulk { s, p, o } => {
            append_facts_v2(log, rec.r#gen, rec.next_id, s, p, o, buf, sync, log_bytes)
        }
        Pack::InsertCols {
            collection,
            fields,
            cols,
            n,
            edges,
        } => append_insert_cols_v2(
            log,
            rec.r#gen,
            rec.next_id,
            collection,
            fields,
            cols,
            *n,
            edges,
            buf,
            sync,
            log_bytes,
        ),
        _ => append_record_v1(log, rec, buf, sync, log_bytes),
    }
}

fn append_record_v1(
    log: &mut File,
    rec: &LogRecord,
    buf: &mut Vec<u8>,
    sync: SyncMode,
    log_bytes: &mut u64,
) -> Result<(), Error> {
    buf.clear();
    rmp_serde::encode::write_named(buf, rec).map_err(io_err)?;
    if buf.len() > MAX_RECORD as usize {
        return Err(io_err("log record exceeds 16MiB"));
    }
    write_frame(log, &LOG_MAGIC_V1, buf, sync, log_bytes)
}

fn write_all_vectored(log: &mut File, slices: &[IoSlice<'_>]) -> io::Result<()> {
    let total: usize = slices.iter().map(|s| s.len()).sum();
    let mut written = log.write_vectored(slices)?;
    if written == total {
        return Ok(());
    }
    let mut slice_idx = 0;
    while written > 0 && slice_idx < slices.len() {
        if written >= slices[slice_idx].len() {
            written -= slices[slice_idx].len();
            slice_idx += 1;
        } else {
            log.write_all(&slices[slice_idx][written..])?;
            slice_idx += 1;
            break;
        }
    }
    for slice in &slices[slice_idx..] {
        log.write_all(slice)?;
    }
    Ok(())
}

fn write_frame_bytes(
    log: &mut File,
    codec_byte: u8,
    payload: &[u8],
    checksum: [u8; 4],
    len: u32,
) -> io::Result<()> {
    const INLINE_MAX: usize = 1024 * 1024;
    if payload.len() <= INLINE_MAX {
        FRAME_WRITE_SCRATCH.with(|cell| {
            let mut scratch = cell.borrow_mut();
            scratch.clear();
            scratch.reserve(8 + 1 + payload.len() + 4);
            scratch.extend_from_slice(&LOG_MAGIC_CHECKSUM);
            scratch.extend_from_slice(&len.to_le_bytes());
            scratch.push(codec_byte);
            scratch.extend_from_slice(payload);
            scratch.extend_from_slice(&checksum);
            log.write_all(&scratch)
        })
    } else {
        let mut hdr = [0u8; 8];
        hdr[..4].copy_from_slice(&LOG_MAGIC_CHECKSUM);
        hdr[4..].copy_from_slice(&len.to_le_bytes());
        let codec = [codec_byte];
        let slices = [
            IoSlice::new(&hdr),
            IoSlice::new(&codec),
            IoSlice::new(payload),
            IoSlice::new(&checksum),
        ];
        write_all_vectored(log, &slices)
    }
}

fn write_frame(
    log: &mut File,
    magic: &[u8; 4],
    payload: &[u8],
    sync: SyncMode,
    log_bytes: &mut u64,
) -> Result<(), Error> {
    let Some(len) = payload
        .len()
        .checked_add(5)
        .filter(|&n| n <= MAX_RECORD as usize)
    else {
        return Err(io_err("log record exceeds 16MiB"));
    };
    let codec = magic[3];
    let mut checksum = crate::crc::Hasher::new();
    checksum.update(&[codec]);
    checksum.update(payload);
    let checksum = checksum.finalize().to_le_bytes();

    write_frame_bytes(log, codec, payload, checksum, len as u32).map_err(io_err)?;

    if sync == SyncMode::Full {
        durable_sync(log).map_err(io_err)?;
    }
    *log_bytes += 8 + len as u64;
    Ok(())
}

/// Durability flush after a WAL append.
///
/// On macOS/APFS, Rust's `File::sync_data` maps to `F_FULLFSYNC` (~ms). SQLite's
/// `synchronous=FULL` uses `F_BARRIERFSYNC` on modern Darwin — same crash model
/// for APFS, ~10× cheaper. Match that. Elsewhere: `sync_data` / `fdatasync`.
pub(crate) fn durable_sync(file: &File) -> io::Result<()> {
    #[cfg(target_vendor = "apple")]
    {
        use std::os::unix::io::AsRawFd;
        // sys/fcntl.h — not always in libc crate bindings.
        const F_BARRIERFSYNC: libc::c_int = 85;
        let rc = unsafe { libc::fcntl(file.as_raw_fd(), F_BARRIERFSYNC) };
        if rc == 0 {
            return Ok(());
        }
        // Older kernels: fall back to FULLFSYNC via sync_data.
        file.sync_data()
    }
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::io::AsRawFd;
        let rc = unsafe { libc::fdatasync(file.as_raw_fd()) };
        if rc == 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }
    #[cfg(not(any(target_vendor = "apple", target_os = "linux")))]
    {
        file.sync_data()
    }
}

fn put_str(buf: &mut Vec<u8>, s: &str) {
    let b = s.as_bytes();
    buf.extend_from_slice(&(b.len() as u32).to_le_bytes());
    buf.extend_from_slice(b);
}

fn append_facts_v2(
    log: &mut File,
    rec_gen: u64,
    next_id: u64,
    s: &[String],
    p: &[String],
    o: &[String],
    buf: &mut Vec<u8>,
    sync: SyncMode,
    log_bytes: &mut u64,
) -> Result<(), Error> {
    let n = s.len().min(p.len()).min(o.len());
    buf.clear();
    buf.reserve(1 + 8 + 8 + 4 + n * 24);
    buf.push(V2_FACTS_BULK);
    buf.extend_from_slice(&rec_gen.to_le_bytes());
    buf.extend_from_slice(&next_id.to_le_bytes());
    buf.extend_from_slice(&(n as u32).to_le_bytes());
    for i in 0..n {
        put_str(buf, &s[i]);
        put_str(buf, &p[i]);
        put_str(buf, &o[i]);
    }
    write_frame(log, &LOG_MAGIC_V2, buf, sync, log_bytes)
}

fn use_sparse_vectors(rows: &[Option<std::sync::Arc<[f32]>>]) -> bool {
    let dense = rows
        .iter()
        .map(|row| 4usize.saturating_add(row.as_ref().map_or(0, |v| v.len().saturating_mul(4))))
        .fold(0usize, usize::saturating_add);
    if rows
        .iter()
        .any(|row| row.as_ref().is_some_and(|v| v.is_empty()))
    {
        return true; // Preserve Some(empty), which the legacy vector tag cannot express.
    }
    if dense < SPARSE_VECTOR_THRESHOLD {
        return false;
    }
    if rows.len() >= 4096 && dense >= 8 * 1024 * 1024 {
        return sparse_bytes_parallel(rows) < dense;
    }
    let sparse = rows
        .iter()
        .map(|row| {
            row.as_ref().map_or(4, |v| {
                8usize.saturating_add(
                    v.iter()
                        .filter(|value| value.to_bits() != 0)
                        .count()
                        .saturating_mul(8),
                )
            })
        })
        .fold(0usize, usize::saturating_add);
    sparse < dense
}

fn sparse_bytes_serial(rows: &[Option<std::sync::Arc<[f32]>>]) -> usize {
    rows.iter()
        .map(|row| {
            row.as_ref().map_or(4, |v| {
                8usize.saturating_add(
                    v.iter()
                        .filter(|value| value.to_bits() != 0)
                        .count()
                        .saturating_mul(8),
                )
            })
        })
        .fold(0usize, usize::saturating_add)
}

fn sparse_count_worker(
    rows: &[Option<std::sync::Arc<[f32]>>],
    worker: io::Result<std::thread::ScopedJoinHandle<'_, usize>>,
) -> usize {
    match worker {
        Ok(worker) => worker.join().expect("sparse size worker panicked"),
        Err(_) => sparse_bytes_serial(rows),
    }
}

fn sparse_bytes_parallel(rows: &[Option<std::sync::Arc<[f32]>>]) -> usize {
    std::thread::scope(|scope| {
        let chunk_len = rows.len().div_ceil(4);
        let (left, rest) = rows.split_at(chunk_len);
        let mut workers = Vec::with_capacity(3);
        for right in rest.chunks(chunk_len.max(1)) {
            let worker =
                std::thread::Builder::new().spawn_scoped(scope, move || sparse_bytes_serial(right));
            workers.push((right, worker));
        }
        let mut size = sparse_bytes_serial(left);
        for (right, worker) in workers {
            size = size.saturating_add(sparse_count_worker(right, worker));
        }
        size
    })
}

fn append_sparse_rows_serial(
    rows: &[Option<std::sync::Arc<[f32]>>],
    buf: &mut Vec<u8>,
) -> Result<(), Error> {
    for row in rows {
        let Some(emb) = row else {
            buf.extend_from_slice(&0u32.to_le_bytes());
            continue;
        };
        let dim = u32::try_from(emb.len())
            .ok()
            .and_then(|n| n.checked_add(1))
            .ok_or_else(|| io_err("sparse vector dimension overflow"))?;
        buf.extend_from_slice(&dim.to_le_bytes());
        let count_offset = buf.len();
        buf.extend_from_slice(&0u32.to_le_bytes());
        let mut count = 0u32;
        for (block, values) in emb.chunks(8).enumerate() {
            // Bitwise zero preserves negative zero and NaN payloads.
            // Skip empty blocks without branching for each element.
            if values
                .iter()
                .fold(0u32, |bits, value| bits | value.to_bits())
                == 0
            {
                continue;
            }
            for (offset, value) in values.iter().enumerate() {
                let bits = value.to_bits();
                if bits != 0 {
                    let index = (block * 8 + offset) as u32;
                    let mut entry = [0u8; 8];
                    entry[..4].copy_from_slice(&index.to_le_bytes());
                    entry[4..].copy_from_slice(&bits.to_le_bytes());
                    buf.extend_from_slice(&entry);
                    count += 1;
                }
            }
        }
        buf[count_offset..count_offset + 4].copy_from_slice(&count.to_le_bytes());
    }
    Ok(())
}

fn append_sparse_worker(
    buf: &mut Vec<u8>,
    right: &[Option<std::sync::Arc<[f32]>>],
    worker: io::Result<std::thread::ScopedJoinHandle<'_, Result<Vec<u8>, Error>>>,
) -> Result<(), Error> {
    match worker {
        Ok(worker) => buf.extend_from_slice(&worker.join().expect("sparse WAL worker panicked")?),
        Err(_) => append_sparse_rows_serial(right, buf)?,
    }
    Ok(())
}

fn append_sparse_rows_parallel(
    rows: &[Option<std::sync::Arc<[f32]>>],
    buf: &mut Vec<u8>,
) -> Result<(), Error> {
    std::thread::scope(|scope| {
        let chunk_len = rows.len().div_ceil(4);
        let (left, rest) = rows.split_at(chunk_len);
        let mut workers = Vec::with_capacity(3);
        for right in rest.chunks(chunk_len.max(1)) {
            let worker = std::thread::Builder::new().spawn_scoped(scope, move || {
                let mut bytes = Vec::with_capacity(right.len() * 32);
                append_sparse_rows_serial(right, &mut bytes)?;
                Ok(bytes)
            });
            workers.push((right, worker));
        }
        append_sparse_rows_serial(left, buf)?;
        for (right, worker) in workers {
            append_sparse_worker(buf, right, worker)?;
        }
        Ok(())
    })
}

#[derive(Default, Clone, Copy)]
struct DictionaryStats {
    legacy_bytes: usize,
}
fn append_dictionary_raw_escape(
    row: &Option<std::sync::Arc<[f32]>>,
    out: &mut Vec<u8>,
) -> Result<usize, Error> {
    let start = out.len();
    append_sparse_rows_serial(std::slice::from_ref(row), out)?;
    let legacy = out.len() - start;
    if row.is_some() {
        out.insert(start + 8, 0);
    }
    Ok(legacy)
}
fn append_dictionary_rows(
    rows: &[Option<std::sync::Arc<[f32]>>],
    out: &mut Vec<u8>,
) -> Result<DictionaryStats, Error> {
    let mut stats = DictionaryStats::default();
    'rows: for row in rows {
        let Some(values) = row else {
            out.extend_from_slice(&0u32.to_le_bytes());
            stats.legacy_bytes += 4;
            continue;
        };
        let dim = u32::try_from(values.len())
            .ok()
            .and_then(|n| n.checked_add(1))
            .ok_or_else(|| io_err("dimension overflow"))?;
        let start = out.len();
        out.extend_from_slice(&dim.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.push(0);
        let entries_start = out.len() + 64;
        out.resize(entries_start, 0);
        let mut dictionary = [0u32; 16];
        let mut d = 0usize;
        let mut count = 0u32;
        let narrow = values.len() <= 65536;
        for (block, chunk) in values.chunks(8).enumerate() {
            if chunk.iter().fold(0u32, |bits, v| bits | v.to_bits()) == 0 {
                continue;
            }
            for (offset, value) in chunk.iter().enumerate() {
                let bits = value.to_bits();
                if bits == 0 {
                    continue;
                }
                // Unused slots are zero; bits is nonzero, so prefix probes need no d guard.
                let found = if dictionary[0] == bits {
                    Some(0)
                } else if dictionary[1] == bits {
                    Some(1)
                } else if dictionary[2] == bits {
                    Some(2)
                } else if dictionary[3] == bits {
                    Some(3)
                } else {
                    dictionary[4..d.max(4)]
                        .iter()
                        .position(|&x| x == bits)
                        .map(|i| i + 4)
                };
                let code = if let Some(code) = found {
                    code
                } else {
                    if d == 16 {
                        out.truncate(start);
                        stats.legacy_bytes += append_dictionary_raw_escape(row, out)?;
                        continue 'rows;
                    }
                    dictionary[d] = bits;
                    d += 1;
                    d - 1
                };
                let index = (block * 8 + offset) as u32;
                if narrow {
                    let bytes = (index as u16).to_le_bytes();
                    out.extend_from_slice(&[bytes[0], bytes[1], code as u8]);
                } else {
                    let bytes = index.to_le_bytes();
                    out.extend_from_slice(&[bytes[0], bytes[1], bytes[2], bytes[3], code as u8]);
                }
                count += 1;
            }
        }
        let legacy = 8 + count as usize * 8;
        let dictionary_bytes = 9 + d * 4 + count as usize * if narrow { 3 } else { 5 };
        if d == 0 || dictionary_bytes >= legacy {
            out.truncate(start);
            stats.legacy_bytes += append_dictionary_raw_escape(row, out)?;
            continue;
        }
        out[start + 4..start + 8].copy_from_slice(&count.to_le_bytes());
        out[start + 8] = d as u8;
        for (i, bits) in dictionary[..d].iter().enumerate() {
            out[start + 9 + i * 4..start + 13 + i * 4].copy_from_slice(&bits.to_le_bytes());
        }
        let end = out.len();
        out.copy_within(entries_start..end, start + 9 + d * 4);
        out.truncate(start + dictionary_bytes);
        stats.legacy_bytes += legacy;
    }
    Ok(stats)
}

fn dictionary_record_eligible(cols: &[ColData], n: u32) -> bool {
    if !cols
        .iter()
        .any(|c| matches!(c,ColData::Vec(v) if (128..4096).contains(&v.len())))
    {
        return false;
    }
    let mut budget = 0usize;
    for col in cols {
        let len = match col {
            ColData::Text(v) => v.len(),
            ColData::Int(v) => v.len(),
            ColData::Float(v) => v.len(),
            ColData::Bool(v) => v.len(),
            ColData::Time(v) => v.len(),
            ColData::Vec(v) => v.len(),
            ColData::Null => n as usize,
        };
        if len != n as usize {
            return false;
        }
        if let ColData::Vec(rows) = col {
            let Some(storage) = rows
                .len()
                .checked_mul(std::mem::size_of::<Option<Vec<f32>>>())
            else {
                return false;
            };
            let Some(next) = budget.checked_add(storage) else {
                return false;
            };
            budget = next;
            for values in rows.iter().flatten() {
                let Some(bytes) = values.len().checked_mul(4) else {
                    return false;
                };
                let Some(next) = budget.checked_add(bytes) else {
                    return false;
                };
                budget = next;
                if budget > MAX_SPARSE_DECODED_BYTES {
                    return false;
                }
            }
            if budget > MAX_SPARSE_DECODED_BYTES {
                return false;
            }
        }
    }
    true
}
fn try_dictionary_vector_bytes(
    rows: &[Option<std::sync::Arc<[f32]>>],
) -> Result<Option<Vec<u8>>, Error> {
    if !(128..4096).contains(&rows.len()) {
        return Ok(None);
    }
    let dense = rows.iter().try_fold(0usize, |n, row| {
        let bytes = row.as_ref().map_or(Some(0), |v| v.len().checked_mul(4))?;
        n.checked_add(4)?.checked_add(bytes)
    });
    let Some(dense) = dense.filter(|n| (64 * 1024..=8 * 1024 * 1024).contains(n)) else {
        return Ok(None);
    };
    let mut eligible = false;
    for index in [0, rows.len() / 3, rows.len() * 2 / 3, rows.len() - 1] {
        let Some(row) = rows[index].as_ref() else {
            continue;
        };
        let mut seen = [0u32; 16];
        let mut d = 0;
        let mut count = 0;
        for value in row.iter() {
            let bits = value.to_bits();
            if bits == 0 {
                continue;
            }
            count += 1;
            if !seen[..d].contains(&bits) {
                if d == 16 {
                    return Ok(None);
                }
                seen[d] = bits;
                d += 1;
            }
        }
        if d > 0 && 9 + 4 * d + count * if row.len() <= 65536 { 3 } else { 5 } < 8 + count * 8 {
            eligible = true;
        }
    }
    if !eligible {
        return Ok(None);
    }
    let mut bytes = Vec::with_capacity(dense);
    let stats = append_dictionary_rows(rows, &mut bytes)?;
    if bytes.len() * 5 < stats.legacy_bytes.min(dense) * 4 {
        Ok(Some(bytes))
    } else {
        Ok(None)
    }
}

fn append_insert_cols_v2(
    log: &mut File,
    rec_gen: u64,
    next_id: u64,
    collection: &str,
    fields: &[String],
    cols: &[ColData],
    n: u32,
    edges: &[Edge],
    buf: &mut Vec<u8>,
    sync: SyncMode,
    log_bytes: &mut u64,
) -> Result<(), Error> {
    // Fallback to msgpack if edges present (rare in bulk benches) or column mismatch.
    if !edges.is_empty() || fields.len() != cols.len() {
        return append_record_v1(
            log,
            &LogRecord {
                r#gen: rec_gen,
                next_id,
                pack: Pack::InsertCols {
                    collection: collection.to_string(),
                    fields: fields.to_vec(),
                    cols: cols.to_vec(),
                    n,
                    edges: edges.to_vec(),
                },
            },
            buf,
            sync,
            log_bytes,
        );
    }
    let mut est_per_row = 32usize;
    for col in cols {
        match col {
            ColData::Vec(v) => {
                let dim = v
                    .first()
                    .and_then(|opt| opt.as_ref())
                    .map_or(0, |vec| vec.len());
                est_per_row = est_per_row.saturating_add(8 + dim * 4);
            }
            ColData::Text(_) => est_per_row = est_per_row.saturating_add(16),
            ColData::Int(_) | ColData::Float(_) | ColData::Time(_) => {
                est_per_row = est_per_row.saturating_add(8)
            }
            ColData::Bool(_) => est_per_row = est_per_row.saturating_add(1),
            ColData::Null => {}
        }
    }
    buf.clear();
    buf.reserve(64 + fields.len() * 16 + (n as usize) * est_per_row);
    buf.push(V2_INSERT_COLS);
    buf.extend_from_slice(&rec_gen.to_le_bytes());
    buf.extend_from_slice(&next_id.to_le_bytes());
    put_str(buf, collection);
    buf.extend_from_slice(&n.to_le_bytes());
    buf.extend_from_slice(&(fields.len() as u32).to_le_bytes());
    let mut sparse_codec = false;
    let mut dictionary_codec = false;
    let dictionary_eligible = dictionary_record_eligible(cols, n);
    let mut sparse_decoded_bytes = 0usize;
    for (f, c) in fields.iter().zip(cols.iter()) {
        put_str(buf, f);
        if dictionary_eligible
            && let ColData::Vec(rows) = c
            && let Some(bytes) = try_dictionary_vector_bytes(rows)?
        {
            buf.push(8);
            buf.extend_from_slice(&bytes);
            dictionary_codec = true;
            continue;
        }
        match c {
            ColData::Text(v) => {
                buf.push(1);
                for s in v {
                    put_str(buf, s);
                }
            }
            ColData::Int(v) => {
                buf.push(2);
                for x in v {
                    buf.extend_from_slice(&x.to_le_bytes());
                }
            }
            ColData::Float(v) => {
                buf.push(3);
                for x in v {
                    buf.extend_from_slice(&x.to_le_bytes());
                }
            }
            ColData::Bool(v) => {
                buf.push(4);
                for x in v {
                    buf.push(u8::from(*x));
                }
            }
            ColData::Time(v) => {
                buf.push(5);
                for x in v {
                    buf.extend_from_slice(&x.to_le_bytes());
                }
            }
            ColData::Vec(v) if use_sparse_vectors(v) => {
                if v.len() != n as usize {
                    return Err(io_err("sparse vector column length mismatch"));
                }
                let storage = v
                    .len()
                    .checked_mul(std::mem::size_of::<Option<Vec<f32>>>())
                    .ok_or_else(|| io_err("sparse vector size overflow"))?;
                sparse_decoded_bytes = sparse_decoded_bytes
                    .checked_add(storage)
                    .ok_or_else(|| io_err("sparse vector size overflow"))?;
                sparse_codec = true;
                buf.push(7);
                if v.len() >= 4096
                    && v.iter()
                        .flatten()
                        .fold(0usize, |n, emb| n.saturating_add(emb.len()))
                        >= 2 * 1024 * 1024
                {
                    for emb in v.iter().flatten() {
                        let bytes = emb
                            .len()
                            .checked_mul(4)
                            .ok_or_else(|| io_err("sparse vector size overflow"))?;
                        sparse_decoded_bytes = sparse_decoded_bytes
                            .checked_add(bytes)
                            .filter(|&bytes| bytes <= MAX_SPARSE_DECODED_BYTES)
                            .ok_or_else(|| io_err("sparse vectors exceed decoded 64MiB budget"))?;
                    }
                    append_sparse_rows_parallel(v, buf)?;
                    continue;
                }
                for emb in v.iter().flatten() {
                    let bytes = emb
                        .len()
                        .checked_mul(4)
                        .ok_or_else(|| io_err("sparse vector size overflow"))?;
                    sparse_decoded_bytes = sparse_decoded_bytes
                        .checked_add(bytes)
                        .filter(|&bytes| bytes <= MAX_SPARSE_DECODED_BYTES)
                        .ok_or_else(|| io_err("sparse vectors exceed decoded 64MiB budget"))?;
                }
                append_sparse_rows_serial(v, buf)?;
                if sparse_decoded_bytes > MAX_SPARSE_DECODED_BYTES {
                    return Err(io_err("sparse vectors exceed decoded 64MiB budget"));
                }
            }
            ColData::Vec(v) => {
                buf.push(6);
                for row in v {
                    match row {
                        Some(emb) if !emb.is_empty() => {
                            buf.extend_from_slice(&(emb.len() as u32).to_le_bytes());
                            #[cfg(target_endian = "little")]
                            {
                                // Safety: [f32] on little-endian has the exact IEEE 754 LE byte representation.
                                let bytes = unsafe {
                                    std::slice::from_raw_parts(
                                        emb.as_ptr() as *const u8,
                                        emb.len() * std::mem::size_of::<f32>(),
                                    )
                                };
                                buf.extend_from_slice(bytes);
                            }
                            #[cfg(not(target_endian = "little"))]
                            {
                                for x in emb.iter() {
                                    buf.extend_from_slice(&x.to_le_bytes());
                                }
                            }
                        }
                        _ => buf.extend_from_slice(&0u32.to_le_bytes()),
                    }
                }
            }
            ColData::Null => buf.push(0),
        }
    }
    write_frame(
        log,
        if dictionary_codec {
            &LOG_MAGIC_DICTIONARY
        } else if sparse_codec {
            &LOG_MAGIC_SPARSE
        } else {
            &LOG_MAGIC_V2
        },
        buf,
        sync,
        log_bytes,
    )
}

/// Replay complete records with `gen > min_gen`. Returns the byte offset of
/// the last good record (file should be truncated there).
pub fn replay_log(
    log: &mut File,
    min_gen: u64,
    start_offset: u64,
    mut on_rec: impl FnMut(LogRecord) -> Result<(), Error>,
) -> Result<u64, Error> {
    let file_len = log.metadata().map_err(io_err)?.len();
    let start = if start_offset <= file_len {
        start_offset
    } else {
        0
    };
    match replay_from(log, min_gen, start, file_len, &mut on_rec) {
        Ok(end) => Ok(end),
        Err(ReplayFail::BadStart) if start > 0 => {
            replay_from(log, min_gen, 0, file_len, &mut on_rec).map_err(|e| match e {
                ReplayFail::Io(err) => err,
                ReplayFail::BadStart => io_err("corrupt log"),
            })
        }
        Err(ReplayFail::Io(e)) => Err(e),
        Err(ReplayFail::BadStart) => Err(io_err("corrupt log")),
    }
}

enum ReplayFail {
    BadStart,
    Io(Error),
}

fn replay_from(
    log: &mut File,
    min_gen: u64,
    start: u64,
    file_len: u64,
    on_rec: &mut impl FnMut(LogRecord) -> Result<(), Error>,
) -> Result<u64, ReplayFail> {
    log.seek(SeekFrom::Start(start))
        .map_err(|e| ReplayFail::Io(io_err(e)))?;
    let mut pos = start;
    let mut saw_any = false;
    let mut buf = Vec::with_capacity(8192);
    loop {
        match read_one(log, file_len, pos, &mut buf) {
            ReadOne::Eof | ReadOne::Truncated => break,
            ReadOne::Corrupt if !saw_any && start > 0 => return Err(ReplayFail::BadStart),
            ReadOne::Corrupt => return Err(ReplayFail::Io(io_err("corrupt log record"))),
            ReadOne::Io(e) => return Err(ReplayFail::Io(e)),
            ReadOne::Ok { rec, next } => {
                saw_any = true;
                if rec.r#gen > min_gen {
                    on_rec(rec).map_err(ReplayFail::Io)?;
                }
                pos = next;
            }
        }
    }
    Ok(pos)
}

enum ReadOne {
    Ok { rec: LogRecord, next: u64 },
    Eof,
    Truncated,
    Corrupt,
    Io(Error),
}

fn read_one(log: &mut File, file_len: u64, pos: u64, buf: &mut Vec<u8>) -> ReadOne {
    if pos >= file_len {
        return ReadOne::Eof;
    }
    let mut head = [0u8; 4];
    match log.read_exact(&mut head) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return ReadOne::Truncated,
        Err(e) => return ReadOne::Io(io_err(e)),
    }
    // v1/v2 framed: magic + u32 len + payload
    if head == LOG_MAGIC_V1 || head == LOG_MAGIC_V2 || head == LOG_MAGIC_CHECKSUM {
        let mut len_buf = [0u8; 4];
        match log.read_exact(&mut len_buf) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return ReadOne::Truncated,
            Err(e) => return ReadOne::Io(io_err(e)),
        }
        let len = u32::from_le_bytes(len_buf);
        if len == 0 || len > MAX_RECORD {
            return if u64::from(len) > file_len.saturating_sub(pos + 8) || len == 0 {
                ReadOne::Truncated
            } else {
                ReadOne::Corrupt
            };
        }
        let need = u64::from(len);
        if pos + 8 + need > file_len {
            return ReadOne::Truncated;
        }
        buf.resize(8 + len as usize, 0);
        buf[..4].copy_from_slice(&head);
        buf[4..8].copy_from_slice(&len_buf);
        match log.read_exact(&mut buf[8..]) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return ReadOne::Truncated,
            Err(e) => return ReadOne::Io(io_err(e)),
        }
        let rec = match decode_frame(&head, &buf[8..]) {
            Ok(r) => r,
            Err(_) => return ReadOne::Corrupt,
        };
        return ReadOne::Ok {
            rec,
            next: pos + 8 + need,
        };
    }
    // Legacy JSON: first 4 bytes were the length.
    let len = u32::from_le_bytes(head);
    if len == 0 || len > MAX_RECORD {
        let remaining = file_len.saturating_sub(pos + 4);
        if u64::from(len) > remaining || len == 0 {
            return ReadOne::Truncated;
        }
        return ReadOne::Corrupt;
    }
    let need = u64::from(len);
    if pos + 4 + need > file_len {
        return ReadOne::Truncated;
    }
    buf.resize(4 + len as usize, 0);
    buf[..4].copy_from_slice(&head);
    match log.read_exact(&mut buf[4..]) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return ReadOne::Truncated,
        Err(e) => return ReadOne::Io(io_err(e)),
    }
    match serde_json::from_slice::<LogRecord>(&buf[4..]) {
        Ok(rec) => ReadOne::Ok {
            rec,
            next: pos + 4 + need,
        },
        Err(_) => ReadOne::Corrupt,
    }
}

fn take_u32(buf: &[u8], i: &mut usize) -> Result<u32, ()> {
    if *i + 4 > buf.len() {
        return Err(());
    }
    let v = u32::from_le_bytes(buf[*i..*i + 4].try_into().unwrap());
    *i += 4;
    Ok(v)
}

fn take_u64(buf: &[u8], i: &mut usize) -> Result<u64, ()> {
    if *i + 8 > buf.len() {
        return Err(());
    }
    let v = u64::from_le_bytes(buf[*i..*i + 8].try_into().unwrap());
    *i += 8;
    Ok(v)
}

fn take_str(buf: &[u8], i: &mut usize) -> Result<String, ()> {
    let n = take_u32(buf, i)? as usize;
    let end = (*i)
        .checked_add(n)
        .filter(|&end| end <= buf.len())
        .ok_or(())?;
    let s = std::str::from_utf8(&buf[*i..end])
        .map_err(|_| ())?
        .to_string();
    *i = end;
    Ok(s)
}

fn decode_v2(buf: &[u8]) -> Result<LogRecord, ()> {
    decode_column_record(buf, false)
}

// Shared decoded-vector accounting deliberately retains the legacy Vec metadata cost.
fn dictionary_budget_add(budget: &mut usize, bytes: usize) -> Result<(), ()> {
    *budget = budget
        .checked_add(bytes)
        .filter(|&n| n <= MAX_SPARSE_DECODED_BYTES)
        .ok_or(())?;
    Ok(())
}
fn take_dictionary_entry(
    buf: &[u8],
    i: &mut usize,
    index_bytes: usize,
    dictionary: &[u32],
) -> Result<(usize, u32, u16), ()> {
    let end = (*i).checked_add(index_bytes).ok_or(())?;
    let bytes = buf.get(*i..end).ok_or(())?;
    let index = match index_bytes {
        2 => u16::from_le_bytes(bytes.try_into().map_err(|_| ())?) as usize,
        4 => u32::from_le_bytes(bytes.try_into().map_err(|_| ())?) as usize,
        _ => return Err(()),
    };
    *i = end;
    if dictionary.is_empty() {
        let end = (*i).checked_add(4).ok_or(())?;
        let bits = u32::from_le_bytes(buf.get(*i..end).ok_or(())?.try_into().map_err(|_| ())?);
        *i = end;
        Ok((index, bits, 0))
    } else {
        let ordinal = *buf.get(*i).ok_or(())? as usize;
        *i += 1;
        let bits = *dictionary.get(ordinal).ok_or(())?;
        Ok((index, bits, 1u16 << ordinal))
    }
}
fn take_dictionary_vectors(
    buf: &[u8],
    i: &mut usize,
    n: usize,
    budget: &mut usize,
) -> Result<Vec<Option<std::sync::Arc<[f32]>>>, ()> {
    dictionary_budget_add(
        budget,
        n.checked_mul(std::mem::size_of::<Option<Vec<f32>>>())
            .ok_or(())?,
    )?;
    if n > buf.len().saturating_sub(*i) / 4 {
        return Err(());
    }
    let mut rows = Vec::with_capacity(n);
    for _ in 0..n {
        let code = take_u32(buf, i)?;
        if code == 0 {
            rows.push(None);
            continue;
        }
        let dim = (code - 1) as usize;
        let count = take_u32(buf, i)? as usize;
        let d = *buf.get(*i).ok_or(())? as usize;
        *i += 1;
        if count > dim || d > 16 || d > count {
            return Err(());
        }
        let mut dictionary = [0u32; 16];
        for slot in 0..d {
            let bits = take_u32(buf, i)?;
            if bits == 0 || dictionary[..slot].contains(&bits) {
                return Err(());
            }
            dictionary[slot] = bits;
        }
        let index_bytes = if d > 0 && dim <= 65536 {
            2usize
        } else {
            4usize
        };
        let entry_bytes = index_bytes + if d > 0 { 1 } else { 4 };
        if count.checked_mul(entry_bytes).ok_or(())? > buf.len().saturating_sub(*i) {
            return Err(());
        }
        dictionary_budget_add(budget, dim.checked_mul(4).ok_or(())?)?;
        let entries_start = *i;
        let mut previous = None;
        let mut used = 0u16;
        // Validate the entire compact row before allocating its decoded values.
        for _ in 0..count {
            let (index, bits, mask) = take_dictionary_entry(buf, i, index_bytes, &dictionary[..d])?;
            if index >= dim || previous.is_some_and(|last| index <= last) || bits == 0 {
                return Err(());
            }
            used |= mask;
            previous = Some(index);
        }
        if d > 0 && u32::from(used) != (1u32 << d) - 1 {
            return Err(());
        }
        let mut values = vec![0.0f32; dim];
        let mut entry_i = entries_start;
        for _ in 0..count {
            let (index, bits, _) =
                take_dictionary_entry(buf, &mut entry_i, index_bytes, &dictionary[..d])?;
            values[index] = f32::from_bits(bits);
        }
        debug_assert_eq!(entry_i, *i);
        rows.push(Some(values.into()));
    }
    Ok(rows)
}

fn decode_column_record(buf: &[u8], allow_sparse: bool) -> Result<LogRecord, ()> {
    decode_column_record_mode(buf, allow_sparse, false)
}
fn decode_column_record_mode(
    buf: &[u8],
    allow_sparse: bool,
    allow_dictionary: bool,
) -> Result<LogRecord, ()> {
    if buf.is_empty() {
        return Err(());
    }
    let mut i = 0usize;
    let kind = buf[i];
    i += 1;
    if allow_sparse && kind != V2_INSERT_COLS {
        return Err(());
    }
    let rec_gen = take_u64(buf, &mut i)?;
    let next_id = take_u64(buf, &mut i)?;
    let mut saw_sparse = false;
    let mut saw_dictionary = false;
    let record = match kind {
        V2_FACTS_BULK => {
            let n = take_u32(buf, &mut i)? as usize;
            let mut s = Vec::with_capacity(n);
            let mut p = Vec::with_capacity(n);
            let mut o = Vec::with_capacity(n);
            for _ in 0..n {
                s.push(take_str(buf, &mut i)?);
                p.push(take_str(buf, &mut i)?);
                o.push(take_str(buf, &mut i)?);
            }
            Ok(LogRecord {
                r#gen: rec_gen,
                next_id,
                pack: Pack::AppendFactsBulk { s, p, o },
            })
        }
        V2_INSERT_COLS => {
            let collection = take_str(buf, &mut i)?;
            let n = take_u32(buf, &mut i)?;
            let nf = take_u32(buf, &mut i)? as usize;
            if allow_sparse
                && (nf > buf.len().saturating_sub(i) / 5
                    || (n as usize)
                        .checked_mul(std::mem::size_of::<Option<Vec<f32>>>())
                        .is_none_or(|bytes| bytes > MAX_SPARSE_DECODED_BYTES))
            {
                return Err(());
            }
            let mut sparse_decoded_bytes = 0usize;
            let mut fields = Vec::with_capacity(nf);
            let mut cols = Vec::with_capacity(nf);
            for _ in 0..nf {
                fields.push(take_str(buf, &mut i)?);
                if i >= buf.len() {
                    return Err(());
                }
                let ct = buf[i];
                i += 1;
                let col = match ct {
                    0 => ColData::Null,
                    1 => {
                        let mut v = Vec::with_capacity(n as usize);
                        for _ in 0..n {
                            v.push(take_str(buf, &mut i)?.into());
                        }
                        ColData::Text(v)
                    }
                    2 => {
                        let mut v = Vec::with_capacity(n as usize);
                        for _ in 0..n {
                            if i + 8 > buf.len() {
                                return Err(());
                            }
                            v.push(i64::from_le_bytes(buf[i..i + 8].try_into().unwrap()));
                            i += 8;
                        }
                        ColData::Int(v)
                    }
                    3 => {
                        let mut v = Vec::with_capacity(n as usize);
                        for _ in 0..n {
                            if i + 8 > buf.len() {
                                return Err(());
                            }
                            v.push(f64::from_le_bytes(buf[i..i + 8].try_into().unwrap()));
                            i += 8;
                        }
                        ColData::Float(v)
                    }
                    4 => {
                        let mut v = Vec::with_capacity(n as usize);
                        for _ in 0..n {
                            if i >= buf.len() {
                                return Err(());
                            }
                            v.push(buf[i] != 0);
                            i += 1;
                        }
                        ColData::Bool(v)
                    }
                    5 => {
                        let mut v = Vec::with_capacity(n as usize);
                        for _ in 0..n {
                            if i + 8 > buf.len() {
                                return Err(());
                            }
                            v.push(i64::from_le_bytes(buf[i..i + 8].try_into().unwrap()));
                            i += 8;
                        }
                        ColData::Time(v)
                    }
                    6 => {
                        if allow_dictionary {
                            dictionary_budget_add(
                                &mut sparse_decoded_bytes,
                                (n as usize)
                                    .checked_mul(std::mem::size_of::<Option<Vec<f32>>>())
                                    .ok_or(())?,
                            )?;
                            if n as usize > buf.len().saturating_sub(i) / 4 {
                                return Err(());
                            }
                        }
                        let mut v = Vec::with_capacity(n as usize);
                        for _ in 0..n {
                            if i + 4 > buf.len() {
                                return Err(());
                            }
                            let dim =
                                u32::from_le_bytes(buf[i..i + 4].try_into().unwrap()) as usize;
                            i += 4;
                            if dim == 0 {
                                v.push(None);
                                continue;
                            }
                            let nbytes = dim.checked_mul(4).ok_or(())?;
                            if nbytes > buf.len().saturating_sub(i) {
                                return Err(());
                            }
                            if allow_dictionary {
                                dictionary_budget_add(&mut sparse_decoded_bytes, nbytes)?;
                            }
                            let mut emb = Vec::with_capacity(dim);
                            for _ in 0..dim {
                                emb.push(f32::from_le_bytes(buf[i..i + 4].try_into().unwrap()));
                                i += 4;
                            }
                            v.push(Some(emb.into()));
                        }
                        ColData::Vec(v)
                    }
                    8 if allow_dictionary => {
                        saw_sparse = true;
                        saw_dictionary = true;
                        ColData::Vec(take_dictionary_vectors(
                            buf,
                            &mut i,
                            n as usize,
                            &mut sparse_decoded_bytes,
                        )?)
                    }
                    7 if allow_sparse => {
                        saw_sparse = true;
                        let storage = (n as usize)
                            .checked_mul(std::mem::size_of::<Option<Vec<f32>>>())
                            .ok_or(())?;
                        sparse_decoded_bytes = sparse_decoded_bytes
                            .checked_add(storage)
                            .filter(|&bytes| bytes <= MAX_SPARSE_DECODED_BYTES)
                            .ok_or(())?;
                        if n as usize > buf.len().saturating_sub(i) / 4 {
                            return Err(());
                        }
                        let mut rows = Vec::with_capacity(n as usize);
                        for _ in 0..n {
                            let code = take_u32(buf, &mut i)?;
                            if code == 0 {
                                rows.push(None);
                                continue;
                            }
                            let dim = (code - 1) as usize;
                            let count = take_u32(buf, &mut i)? as usize;
                            if count > dim || count > buf.len().saturating_sub(i) / 8 {
                                return Err(());
                            }
                            sparse_decoded_bytes = sparse_decoded_bytes
                                .checked_add(dim.checked_mul(4).ok_or(())?)
                                .filter(|&bytes| bytes <= MAX_SPARSE_DECODED_BYTES)
                                .ok_or(())?;
                            let mut values = vec![0.0f32; dim];
                            let mut previous = None;
                            for _ in 0..count {
                                let pos = take_u32(buf, &mut i)? as usize;
                                let bits = take_u32(buf, &mut i)?;
                                if pos >= dim || previous.is_some_and(|last| pos <= last) {
                                    return Err(());
                                }
                                values[pos] = f32::from_bits(bits);
                                previous = Some(pos);
                            }
                            rows.push(Some(values.into()));
                        }
                        ColData::Vec(rows)
                    }
                    _ => return Err(()),
                };
                cols.push(col);
            }
            Ok(LogRecord {
                r#gen: rec_gen,
                next_id,
                pack: Pack::InsertCols {
                    collection,
                    fields,
                    cols,
                    n,
                    edges: Vec::new(),
                },
            })
        }
        _ => Err(()),
    }?;
    if allow_sparse && (!saw_sparse || i != buf.len()) {
        return Err(());
    }
    if allow_dictionary && !saw_dictionary {
        return Err(());
    }
    Ok(record)
}

pub fn truncate_log(log: &mut File, end: u64) -> Result<(), Error> {
    log.set_len(end).map_err(io_err)?;
    log.seek(SeekFrom::End(0)).map_err(io_err)?;
    log.sync_all().map_err(io_err)?;
    Ok(())
}

/// After a durable snapshot of the full memory image, drop the covered log prefix
/// so the file does not grow forever. Sets length to 0 and seeks to start.
pub fn compact_log(log: &mut File) -> Result<(), Error> {
    truncate_log(log, 0)
}

/// Export raw WAL frames with `gen > since_gen` (network / follower shipping).
pub fn export_wal_since(dir: &Path, since_gen: u64) -> Result<Vec<u8>, Error> {
    let mut log = match open_log_read(dir)? {
        Some(f) => f,
        None => return Ok(Vec::new()),
    };
    let file_len = log.metadata().map_err(io_err)?.len();
    let mut out = Vec::with_capacity(file_len.min(64 * 1024) as usize);
    let mut pos = 0u64;
    let mut buf = Vec::with_capacity(8192);
    loop {
        match read_one(&mut log, file_len, pos, &mut buf) {
            ReadOne::Eof | ReadOne::Truncated => break,
            ReadOne::Corrupt => return Err(io_err("corrupt log")),
            ReadOne::Io(e) => return Err(e),
            ReadOne::Ok { rec, next } => {
                if rec.r#gen > since_gen {
                    out.extend_from_slice(&buf);
                }
                pos = next;
            }
        }
    }
    Ok(out)
}

fn decode_frame(magic: &[u8; 4], payload: &[u8]) -> Result<LogRecord, Error> {
    if *magic == LOG_MAGIC_CHECKSUM {
        if payload.len() < 5 {
            return Err(io_err("short checksummed wal payload"));
        }
        let end = payload.len() - 4;
        let expected = u32::from_le_bytes(payload[end..].try_into().unwrap());
        let hw_crc = crate::crc::hardware_crc32c(&payload[..end]);
        if hw_crc != expected && crc32fast::hash(&payload[..end]) != expected {
            return Err(io_err("wal checksum mismatch"));
        }
        let codec = match payload[0] {
            1 => LOG_MAGIC_V1,
            2 => LOG_MAGIC_V2,
            3 => LOG_MAGIC_SPARSE,
            5 => LOG_MAGIC_DICTIONARY,
            #[cfg(any(feature = "compress-wal", feature = "compress-flate"))]
            4 => {
                use flate2::read::GzDecoder;
                use std::io::Read;
                let mut decoder = GzDecoder::new(&payload[1..end]);
                let mut raw = Vec::new();
                decoder.read_to_end(&mut raw).map_err(io_err)?;
                return rmp_serde::from_slice(&raw).map_err(io_err);
            }
            _ => return Err(io_err("unknown wal codec")),
        };
        return decode_frame(&codec, &payload[1..end]);
    }
    if *magic == LOG_MAGIC_DICTIONARY {
        decode_column_record_mode(payload, true, true)
            .map_err(|_| io_err("bad dictionary wal payload"))
    } else if *magic == LOG_MAGIC_SPARSE {
        decode_column_record(payload, true).map_err(|_| io_err("bad sparse wal payload"))
    } else if *magic == LOG_MAGIC_V2 {
        decode_v2(payload).map_err(|_| io_err("bad v2 wal payload"))
    } else if *magic == LOG_MAGIC_V1 {
        rmp_serde::from_slice(payload).map_err(io_err)
    } else {
        Err(io_err("unknown wal magic"))
    }
}

/// Decode shipped WAL frames and invoke `on_rec` for each record.
pub fn for_each_wal_frame(
    frames: &[u8],
    mut on_rec: impl FnMut(LogRecord) -> Result<(), Error>,
) -> Result<usize, Error> {
    for_each_wal_frame_raw(frames, |_raw, rec| on_rec(rec))
}

/// Like [`for_each_wal_frame`], but also yields the raw framed bytes
/// (`magic + len + payload`) so a follower can append them verbatim.
pub fn for_each_wal_frame_raw(
    frames: &[u8],
    mut on_frame: impl FnMut(&[u8], LogRecord) -> Result<(), Error>,
) -> Result<usize, Error> {
    let mut pos = 0usize;
    let mut n = 0usize;
    while pos < frames.len() {
        if pos + 8 > frames.len() {
            return Err(io_err("truncated wal frame"));
        }
        let magic: [u8; 4] = frames[pos..pos + 4].try_into().unwrap();
        let len = u32::from_le_bytes(frames[pos + 4..pos + 8].try_into().unwrap()) as usize;
        if len == 0 || len > MAX_RECORD as usize || pos + 8 + len > frames.len() {
            return Err(io_err("bad wal frame length"));
        }
        let end = pos + 8 + len;
        let payload = &frames[pos + 8..end];
        let rec = decode_frame(&magic, payload)?;
        on_frame(&frames[pos..end], rec)?;
        n += 1;
        pos = end;
    }
    Ok(n)
}

/// Append pre-framed WAL bytes (as produced by [`export_wal_since`]) and
/// durability-flush according to [`SyncMode`].
pub fn append_raw_frames(
    log: &mut File,
    frames: &[u8],
    sync: SyncMode,
    log_bytes: &mut u64,
) -> Result<(), Error> {
    if frames.is_empty() {
        return Ok(());
    }
    // Validate framing before touching the log.
    for_each_wal_frame_raw(frames, |_, _| Ok(()))?;
    log.write_all(frames).map_err(io_err)?;
    if sync == SyncMode::Full {
        durable_sync(log).map_err(io_err)?;
    }
    *log_bytes += frames.len() as u64;
    Ok(())
}

/// Check if a content hash already exists in the collection (deduplication helper)
#[cfg(feature = "parallel")]
#[allow(dead_code)]
pub fn check_content_exists(
    collection_name: &str,
    body_hash: &str,
    store: &crate::store::Store,
) -> bool {
    let collection = store.collection(collection_name);
    let hash_field = "hash";

    // Parallel search across collection rows
    #[cfg(feature = "parallel")]
    {
        use rayon::prelude::*;

        collection.par_iter().any(|row| {
            row.get(hash_field)
                .and_then(|cell| cell.text())
                .is_some_and(|h| h == body_hash)
        })
    }

    #[cfg(not(feature = "parallel"))]
    {
        collection.iter().any(|row| {
            row.get(hash_field)
                .and_then(|cell| cell.text())
                .is_some_and(|h| h == body_hash)
        })
    }
}

/// Build columnar insert pack in one pass (field set from first row + union).
pub fn rows_to_insert_cols(
    collection: impl Into<String>,
    rows: &[crate::store::Row],
    edges: Vec<Edge>,
) -> Pack {
    use crate::store::Cell;
    use std::collections::BTreeSet;

    let n = rows.len() as u32;
    if rows.is_empty() {
        return Pack::InsertCols {
            collection: collection.into(),
            fields: Vec::new(),
            cols: Vec::new(),
            n: 0,
            edges,
        };
    }
    // Prefer key order of first row (hot docs path), then any extras.
    let mut field_set = BTreeSet::new();
    let mut uniform = true;
    for k in rows[0].keys() {
        field_set.insert(k.as_str());
    }
    for r in rows.iter().skip(1) {
        if r.len() == rows[0].len() && r.keys().eq(rows[0].keys()) {
            continue;
        }
        uniform = false;
        for k in r.keys() {
            field_set.insert(k.as_str());
        }
    }
    let fields: Vec<String> = field_set.into_iter().map(str::to_owned).collect();
    if uniform && !rows.is_empty() && rows[0].values().all(|cell| !matches!(cell, Cell::Null)) {
        let mut cols = rows[0]
            .values()
            .map(|cell| match cell {
                Cell::Text(_) | Cell::SmallText(_) => ColData::Text(Vec::with_capacity(rows.len())),
                Cell::Int(_) => ColData::Int(Vec::with_capacity(rows.len())),
                Cell::Float(_) => ColData::Float(Vec::with_capacity(rows.len())),
                Cell::Bool(_) => ColData::Bool(Vec::with_capacity(rows.len())),
                Cell::Time(_) => ColData::Time(Vec::with_capacity(rows.len())),
                Cell::Vec(_) => ColData::Vec(Vec::with_capacity(rows.len())),
                Cell::Null => ColData::Null,
            })
            .collect::<Vec<_>>();
        for row in rows {
            for (col, cell) in cols.iter_mut().zip(row.values()) {
                match col {
                    ColData::Text(v) => v.push(
                        cell.text_shared()
                            .unwrap_or_else(|| std::sync::Arc::from("")),
                    ),
                    ColData::Int(v) => v.push(match cell {
                        Cell::Int(n) => *n,
                        _ => 0,
                    }),
                    ColData::Float(v) => v.push(match cell {
                        Cell::Float(n) => *n,
                        Cell::Int(n) => *n as f64,
                        _ => 0.0,
                    }),
                    ColData::Bool(v) => v.push(matches!(cell, Cell::Bool(true))),
                    ColData::Time(v) => v.push(match cell {
                        Cell::Time(n) | Cell::Int(n) => *n,
                        _ => 0,
                    }),
                    ColData::Vec(v) => v.push(match cell {
                        Cell::Vec(embedding) => Some(std::sync::Arc::clone(embedding)),
                        _ => None,
                    }),
                    ColData::Null => {}
                }
            }
        }
        return Pack::InsertCols {
            collection: collection.into(),
            fields,
            cols,
            n,
            edges,
        };
    }
    // Matching BTreeMap key order lets every column borrow cells by position,
    // avoiding another tree search per row and field. No cell values are cloned here.
    let cells = uniform.then(|| rows.iter().flat_map(|row| row.values()).collect::<Vec<_>>());
    let width = fields.len();
    let column_cells = |fi: usize| {
        let field = fields[fi].as_str();
        let cached = cells.as_deref();
        rows.iter().enumerate().map(move |(ri, row)| match cached {
            Some(cells) => Some(cells[ri * width + fi]),
            None => row.get(field),
        })
    };
    let mut cols = Vec::with_capacity(fields.len());
    for fi in 0..fields.len() {
        // Infer from first non-null in column.
        let mut kind = 0u8;
        for cell in column_cells(fi) {
            match cell {
                Some(Cell::Text(_) | Cell::SmallText(_)) => {
                    kind = 1;
                    break;
                }
                Some(Cell::Int(_)) => {
                    kind = 2;
                    break;
                }
                Some(Cell::Float(_)) => {
                    kind = 3;
                    break;
                }
                Some(Cell::Bool(_)) => {
                    kind = 4;
                    break;
                }
                Some(Cell::Time(_)) => {
                    kind = 5;
                    break;
                }
                Some(Cell::Vec(_)) => {
                    kind = 6;
                    break;
                }
                _ => {}
            }
        }
        let col = match kind {
            1 => {
                let mut v = Vec::with_capacity(rows.len());
                for cell in column_cells(fi) {
                    v.push(
                        cell.and_then(Cell::text_shared)
                            .unwrap_or_else(|| std::sync::Arc::from("")),
                    );
                }
                ColData::Text(v)
            }
            2 => {
                let mut v = Vec::with_capacity(rows.len());
                for cell in column_cells(fi) {
                    v.push(match cell {
                        Some(Cell::Int(n)) => *n,
                        _ => 0,
                    });
                }
                ColData::Int(v)
            }
            3 => {
                let mut v = Vec::with_capacity(rows.len());
                for cell in column_cells(fi) {
                    v.push(match cell {
                        Some(Cell::Float(n)) => *n,
                        Some(Cell::Int(n)) => *n as f64,
                        _ => 0.0,
                    });
                }
                ColData::Float(v)
            }
            4 => {
                let mut v = Vec::with_capacity(rows.len());
                for cell in column_cells(fi) {
                    v.push(matches!(cell, Some(Cell::Bool(true))));
                }
                ColData::Bool(v)
            }
            5 => {
                let mut v = Vec::with_capacity(rows.len());
                for cell in column_cells(fi) {
                    v.push(match cell {
                        Some(Cell::Time(n)) => *n,
                        Some(Cell::Int(n)) => *n,
                        _ => 0,
                    });
                }
                ColData::Time(v)
            }
            6 => {
                let mut v = Vec::with_capacity(rows.len());
                for cell in column_cells(fi) {
                    v.push(match cell {
                        Some(Cell::Vec(embedding)) => Some(std::sync::Arc::clone(embedding)),
                        _ => None,
                    });
                }
                ColData::Vec(v)
            }
            _ => ColData::Null,
        };
        cols.push(col);
    }
    Pack::InsertCols {
        collection: collection.into(),
        fields,
        cols,
        n,
        edges,
    }
}

pub fn cols_to_rows(fields: &[String], cols: &[ColData], n: usize) -> Vec<crate::store::Row> {
    use crate::store::Cell;
    let mut rows = Vec::with_capacity(n);
    for i in 0..n {
        let mut row = crate::store::Row::new();
        for (fi, f) in fields.iter().enumerate() {
            let cell = match cols.get(fi) {
                Some(ColData::Text(v)) => v
                    .get(i)
                    .map(|s| Cell::text_arc((*s).clone())) // Zero-copy: deref Arc to str, then Into<Arc<str>> reuses
                    .unwrap_or_else(|| Cell::text_arc("")),
                Some(ColData::Int(v)) => Cell::Int(v.get(i).copied().unwrap_or(0)),
                Some(ColData::Float(v)) => Cell::Float(v.get(i).copied().unwrap_or(0.0)),
                Some(ColData::Bool(v)) => Cell::Bool(v.get(i).copied().unwrap_or(false)),
                Some(ColData::Time(v)) => Cell::Time(v.get(i).copied().unwrap_or(0)),
                Some(ColData::Vec(v)) => match v.get(i).and_then(|o| o.as_ref()) {
                    Some(emb) => Cell::vec_arc(std::sync::Arc::clone(emb)),
                    _ => Cell::Null,
                },
                Some(ColData::Null) | None => Cell::Null,
            };
            row.insert(f.clone(), cell);
        }
        rows.push(row);
    }
    rows
}

#[cfg(test)]
mod wal_integrity_tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    #[test]
    fn parallel_sparse_rows_preserve_serial_bytes_and_worker_fallback() {
        let rows = (0..4101)
            .map(|i| match i % 13 {
                0 => None,
                1 => Some(Vec::new()),
                _ => {
                    let mut values = vec![0.0; 768];
                    values[7] = -0.0;
                    values[8] = f32::from_bits(0x7fc00001);
                    values[32] = f32::INFINITY;
                    values[767] = i as f32;
                    Some(values)
                }
            })
            .collect::<Vec<_>>();
        let mut expected = vec![1, 2, 3];
        // Independent scalar reference, without block skipping or workers.
        for row in &rows {
            match row {
                None => expected.extend_from_slice(&0u32.to_le_bytes()),
                Some(values) => {
                    expected.extend_from_slice(&((values.len() as u32) + 1).to_le_bytes());
                    let count = values.iter().filter(|x| x.to_bits() != 0).count() as u32;
                    expected.extend_from_slice(&count.to_le_bytes());
                    for (i, x) in values.iter().enumerate().filter(|(_, x)| x.to_bits() != 0) {
                        expected.extend_from_slice(&(i as u32).to_le_bytes());
                        expected.extend_from_slice(&x.to_bits().to_le_bytes());
                    }
                }
            }
        }
        let mut actual = vec![1, 2, 3];
        let shared_rows = rows
            .iter()
            .map(|v| v.as_ref().map(|v| std::sync::Arc::from(v.as_slice())))
            .collect::<Vec<_>>();
        append_sparse_rows_parallel(&shared_rows, &mut actual).unwrap();
        assert_eq!(actual, expected);
        let mut fallback = vec![1, 2, 3];
        append_sparse_worker(
            &mut fallback,
            &shared_rows,
            Err(io::ErrorKind::WouldBlock.into()),
        )
        .unwrap();
        assert_eq!(fallback, expected);
        with_log(|file| {
            let bytes = frame(file, vector_pack(rows));
            assert_eq!(bytes[8], 3);
            assert!(bytes[9..bytes.len() - 4].ends_with(&expected[3..]));
            let decoded = decode_column_record(&bytes[9..bytes.len() - 4], true).unwrap();
            let Pack::InsertCols { cols, n, .. } = decoded.pack else {
                panic!("columns");
            };
            assert_eq!(n, 4101);
            let ColData::Vec(v) = &cols[0] else {
                panic!("vectors");
            };
            assert!(v[0].is_none());
            assert!(v[1].as_ref().unwrap().is_empty());
            assert_eq!(v[4100].as_ref().unwrap()[767], 4100.0);
            assert_eq!(v[2].as_ref().unwrap()[7].to_bits(), (-0.0f32).to_bits());
            assert_eq!(v[2].as_ref().unwrap()[8].to_bits(), 0x7fc00001);
        });
    }

    #[test]
    fn parallel_sparse_columns_keep_shared_decoded_budget_before_writing() {
        let n = 4096;
        // Each column fits separately; together the vectors and row metadata
        // exceed the shared 64MiB decoded budget.
        let vectors: Vec<Option<std::sync::Arc<[f32]>>> = vec![Some(vec![0.0; 2048].into()); n];
        let record = LogRecord {
            r#gen: 1,
            next_id: 2,
            pack: Pack::InsertCols {
                collection: "test".into(),
                fields: vec!["a".into(), "b".into()],
                cols: vec![ColData::Vec(vectors.clone()), ColData::Vec(vectors)],
                n: n as u32,
                edges: vec![],
            },
        };
        with_log(|file| {
            file.write_all(b"previous").unwrap();
            let mut log_bytes = 8;
            let err = append_record(
                file,
                &record,
                &mut Vec::new(),
                SyncMode::Normal,
                &mut log_bytes,
            )
            .unwrap_err();
            assert!(err.message.contains("decoded 64MiB budget"));
            assert_eq!(log_bytes, 8);
            file.seek(SeekFrom::Start(0)).unwrap();
            let mut actual = Vec::new();
            file.read_to_end(&mut actual).unwrap();
            assert_eq!(actual, b"previous");
        });
    }

    #[test]
    fn parallel_sparse_classifier_preserves_exact_counts_and_selection() {
        for n in [0, 1, 3, 4095, 4096, 4101] {
            let rows = (0..n)
                .map(|i| {
                    if i % 11 == 0 {
                        None
                    } else {
                        let mut values = vec![0.0; 768];
                        values[7] = -0.0;
                        values[8] = f32::from_bits(0x7fc00001);
                        values[767] = f32::INFINITY;
                        Some(values.into())
                    }
                })
                .collect::<Vec<_>>();
            let expected = rows
                .iter()
                .map(|r| if r.is_some() { 8 + 3 * 8 } else { 4 })
                .sum::<usize>();
            assert_eq!(sparse_bytes_parallel(&rows), expected);
            assert_eq!(
                sparse_count_worker(&rows, Err(io::ErrorKind::WouldBlock.into())),
                expected
            );
            assert_eq!(use_sparse_vectors(&rows), n >= 4095);
        }
        let dense: Vec<Option<std::sync::Arc<[f32]>>> = vec![Some(vec![1.0; 768].into()); 4096];
        assert_eq!(sparse_bytes_parallel(&dense), 4096 * (8 + 768 * 8));
        assert!(!use_sparse_vectors(&dense));
        let mut empty = dense;
        empty[4095] = Some(std::sync::Arc::from([]));
        assert!(use_sparse_vectors(&empty));
    }

    #[test]
    fn shared_text_columns_preserve_legacy_bytes_and_row_values() {
        use crate::store::{Cell, Row};
        use std::sync::Arc;
        #[derive(Serialize)]
        #[serde(tag = "t", content = "v")]
        enum LegacyColumn {
            Text(Vec<String>),
        }
        let values = vec!["".to_owned(), "привет\0世界".to_owned()];
        let legacy = rmp_serde::to_vec_named(&LegacyColumn::Text(values.clone())).unwrap();
        let shared = ColData::Text(values.into_iter().map(Into::into).collect());
        assert_eq!(rmp_serde::to_vec_named(&shared).unwrap(), legacy);
        let decoded: ColData = rmp_serde::from_slice(&legacy).unwrap();
        assert_eq!(rmp_serde::to_vec_named(&decoded).unwrap(), legacy);
        let text: Arc<str> = Arc::from("привет\0世界");
        let rows = [Row::from([(
            "title".into(),
            Cell::text_arc(Arc::clone(&text)),
        )])];
        let Pack::InsertCols {
            fields, cols, n, ..
        } = rows_to_insert_cols("test", &rows, vec![])
        else {
            panic!("column pack")
        };
        let ColData::Text(column) = &cols[0] else {
            panic!("text column")
        };
        assert!(Arc::ptr_eq(&column[0], &text));
        let restored = cols_to_rows(&fields, &cols, n as usize);
        assert_eq!(restored[0], rows[0]);
        let Cell::Text(actual) = &restored[0]["title"] else {
            panic!("text cell")
        };
        assert!(Arc::ptr_eq(actual, &text));
    }

    #[test]
    fn row_major_column_pack_keeps_type_coercions_and_exact_wire_bytes() {
        use crate::store::{Cell, Row};
        let values = vec![-0.0f32, f32::from_bits(0x7fc00001), f32::INFINITY];
        let rows = [
            Row::from([
                ("bool".into(), Cell::Bool(true)),
                ("float".into(), Cell::Float(1.5)),
                ("int".into(), Cell::Int(10)),
                ("text".into(), Cell::text_arc("a")),
                ("time".into(), Cell::Time(12)),
                ("vector".into(), Cell::vec_arc(values.clone())),
            ]),
            Row::from([
                ("bool".into(), Cell::Null),
                ("float".into(), Cell::Int(7)),
                ("int".into(), Cell::Float(8.5)),
                ("text".into(), Cell::Int(99)),
                ("time".into(), Cell::Int(42)),
                ("vector".into(), Cell::Null),
            ]),
        ];
        let rows = rows.into_iter().cycle().take(4096).collect::<Vec<_>>();
        let actual = rows_to_insert_cols("test", &rows, vec![]);
        let expected = Pack::InsertCols {
            collection: "test".into(),
            fields: ["bool", "float", "int", "text", "time", "vector"]
                .map(String::from)
                .to_vec(),
            n: 4096,
            edges: vec![],
            cols: vec![
                ColData::Bool([true, false].into_iter().cycle().take(4096).collect()),
                ColData::Float([1.5, 7.0].into_iter().cycle().take(4096).collect()),
                ColData::Int([10, 0].into_iter().cycle().take(4096).collect()),
                ColData::Text(
                    [std::sync::Arc::from("a"), std::sync::Arc::from("")]
                        .into_iter()
                        .cycle()
                        .take(4096)
                        .collect(),
                ),
                ColData::Time([12, 42].into_iter().cycle().take(4096).collect()),
                ColData::Vec(
                    [Some(std::sync::Arc::<[f32]>::from(values)), None]
                        .into_iter()
                        .cycle()
                        .take(4096)
                        .collect(),
                ),
            ],
        };
        with_log(|file| {
            let actual = frame(file, actual);
            file.set_len(0).unwrap();
            file.seek(SeekFrom::Start(0)).unwrap();
            let expected = frame(file, expected);
            assert_eq!(actual, expected);
        });
        let rows = [
            Row::from([("x".into(), Cell::Null)]),
            Row::from([("x".into(), Cell::Int(9))]),
        ];
        let Pack::InsertCols { cols, .. } = rows_to_insert_cols("test", &rows, vec![]) else {
            panic!("columns")
        };
        assert!(matches!(&cols[0],ColData::Int(values) if values==&[0,9]));
    }

    #[test]
    fn column_field_union_keeps_later_extra_and_replaced_fields() {
        use crate::store::{Cell, Row};
        let rows = [
            Row::from([("a".into(), Cell::Int(1)), ("b".into(), Cell::Int(2))]),
            Row::from([("a".into(), Cell::Int(3)), ("b".into(), Cell::Int(4))]),
            Row::from([("a".into(), Cell::Int(5)), ("c".into(), Cell::Int(6))]),
            Row::from([("d".into(), Cell::Int(7))]),
        ];
        let Pack::InsertCols {
            fields, cols, n, ..
        } = rows_to_insert_cols("test", &rows, vec![])
        else {
            panic!("column pack")
        };
        assert_eq!(fields, ["a", "b", "c", "d"]);
        assert_eq!(n, 4);
        let actual = cols_to_rows(&fields, &cols, n as usize);
        for (original, decoded) in rows.iter().zip(actual) {
            for field in &fields {
                assert_eq!(
                    decoded[field],
                    original.get(field).cloned().unwrap_or(Cell::Int(0))
                );
            }
        }
    }

    static SEQ: AtomicU64 = AtomicU64::new(0);

    fn with_log(test: impl FnOnce(&mut File)) {
        let path = std::env::temp_dir().join(format!(
            "lin-wal-integrity-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&path)
            .unwrap();
        test(&mut file);
        drop(file);
        fs::remove_file(path).unwrap();
    }

    fn frame(file: &mut File, pack: Pack) -> Vec<u8> {
        append_record(
            file,
            &LogRecord {
                r#gen: 1,
                next_id: 2,
                pack,
            },
            &mut Vec::new(),
            SyncMode::Normal,
            &mut 0,
        )
        .unwrap();
        file.seek(SeekFrom::Start(0)).unwrap();
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes).unwrap();
        bytes
    }

    fn vector_pack(vectors: Vec<Option<Vec<f32>>>) -> Pack {
        Pack::InsertCols {
            collection: "docs".into(),
            fields: vec!["embedding".into()],
            n: vectors.len() as u32,
            cols: vec![ColData::Vec(
                vectors.into_iter().map(|v| v.map(Into::into)).collect(),
            )],
            edges: Vec::new(),
        }
    }

    #[test]
    fn sparse_vectors_preserve_bits_presence_and_checksum() {
        let expected = vec![
            None,
            Some(Vec::new()),
            Some(vec![
                0.0,
                -0.0,
                f32::from_bits(0x7fc00001),
                f32::INFINITY,
                -1.25,
            ]),
            Some({
                let mut values = vec![0.0; 69];
                values[7] = -0.0;
                values[8] = f32::from_bits(0x7fc00002);
                values[32] = f32::INFINITY;
                values[68] = -1.25;
                values
            }),
        ];
        with_log(|file| {
            let bytes = frame(file, vector_pack(expected.clone()));
            assert_eq!(bytes[8], 3);
            for_each_wal_frame(&bytes, |record| {
                let Pack::InsertCols { cols, .. } = record.pack else {
                    panic!("insert")
                };
                let ColData::Vec(actual) = &cols[0] else {
                    panic!("vectors")
                };
                let bits = |rows: &[Option<std::sync::Arc<[f32]>>]| {
                    rows.iter()
                        .map(|row| {
                            row.as_ref()
                                .map(|v| v.iter().map(|x| x.to_bits()).collect::<Vec<_>>())
                        })
                        .collect::<Vec<_>>()
                };
                let expected_shared = expected
                    .iter()
                    .map(|v| v.clone().map(Into::into))
                    .collect::<Vec<_>>();
                assert_eq!(bits(actual), bits(&expected_shared));
                Ok(())
            })
            .unwrap();
            for offset in [8, 9, bytes.len() - 5, bytes.len() - 1] {
                let mut corrupt = bytes.clone();
                corrupt[offset] ^= 1;
                assert!(for_each_wal_frame(&corrupt, |_| Ok(())).is_err());
            }
            for cut in 1..bytes.len() {
                assert!(for_each_wal_frame(&bytes[..cut], |_| Ok(())).is_err());
            }
        });
        with_log(|file| {
            let bytes = frame(file, vector_pack(vec![Some(vec![1.0; 1024]); 100]));
            assert_eq!(bytes[8], 2, "dense data keeps legacy raw codec");
            assert_eq!(for_each_wal_frame(&bytes, |_| Ok(())).unwrap(), 1);
        });
    }

    #[test]
    fn sparse_vector_decoder_rejects_dimensions_indices_and_trailing_bytes() {
        with_log(|file| {
            let bytes = frame(
                file,
                vector_pack(vec![Some(vec![0.0, 1.0, 0.0, 2.0]), Some(Vec::new())]),
            );
            let raw = &bytes[9..bytes.len() - 4];
            assert!(
                decode_v2(raw).is_err(),
                "legacy codec must reject new vector tag"
            );
            let mut cursor = 17;
            take_str(raw, &mut cursor).unwrap();
            take_u32(raw, &mut cursor).unwrap();
            take_u32(raw, &mut cursor).unwrap();
            take_str(raw, &mut cursor).unwrap();
            assert_eq!(raw[cursor], 7);
            cursor += 1;
            for (offset, value) in [
                (cursor, u32::MAX),
                (cursor + 4, 5),
                (cursor + 8, 4),
                (cursor + 16, 1),
            ] {
                let mut bad = raw.to_vec();
                bad[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
                assert!(decode_column_record(&bad, true).is_err());
            }
            let mut extra = raw.to_vec();
            extra.push(0);
            assert!(decode_column_record(&extra, true).is_err());
            let mut wrong_kind = raw.to_vec();
            wrong_kind[0] = V2_FACTS_BULK;
            assert!(decode_column_record(&wrong_kind, true).is_err());
        });
    }

    #[test]
    fn checksum_detects_bit_flips_in_both_codecs() {
        for pack in [
            Pack::Batch { packs: Vec::new() },
            Pack::AppendFactsBulk {
                s: vec!["subject".into()],
                p: vec!["predicate".into()],
                o: vec!["object".into()],
            },
        ] {
            with_log(|file| {
                let bytes = frame(file, pack);
                assert_eq!(&bytes[..4], &LOG_MAGIC_CHECKSUM);
                assert_eq!(for_each_wal_frame(&bytes, |_| Ok(())).unwrap(), 1);
                for offset in 8..bytes.len() {
                    let mut corrupt = bytes.clone();
                    corrupt[offset] ^= 1;
                    assert!(
                        for_each_wal_frame(&corrupt, |_| Ok(())).is_err(),
                        "offset {offset}"
                    );
                }
                // Old v1/v2 envelopes remain readable by both shipping and replay.
                let mut legacy = vec![b'L', b'I', b'N', bytes[8]];
                let payload = &bytes[9..bytes.len() - 4];
                legacy.extend_from_slice(&(payload.len() as u32).to_le_bytes());
                legacy.extend_from_slice(payload);
                assert_eq!(for_each_wal_frame(&legacy, |_| Ok(())).unwrap(), 1);
                file.set_len(0).unwrap();
                file.seek(SeekFrom::Start(0)).unwrap();
                file.write_all(&legacy).unwrap();
                let mut seen = 0;
                assert_eq!(
                    replay_log(file, 0, 0, |_| {
                        seen += 1;
                        Ok(())
                    })
                    .unwrap(),
                    legacy.len() as u64
                );
                assert_eq!(seen, 1);
            });
        }
    }

    #[test]
    fn every_truncated_checksummed_tail_is_ignored_but_complete_corruption_fails() {
        with_log(|file| {
            let bytes = frame(file, Pack::Batch { packs: Vec::new() });
            for length in 0..bytes.len() {
                file.set_len(0).unwrap();
                file.seek(SeekFrom::Start(0)).unwrap();
                file.write_all(&bytes[..length]).unwrap();
                assert_eq!(
                    replay_log(file, 0, 0, |_| panic!("partial record replayed")).unwrap(),
                    0
                );
                assert_eq!(file.metadata().unwrap().len(), length as u64);
            }
            let mut corrupt = bytes.clone();
            corrupt[9] ^= 1;
            corrupt.extend_from_slice(&bytes);
            file.set_len(0).unwrap();
            file.seek(SeekFrom::Start(0)).unwrap();
            file.write_all(&corrupt).unwrap();
            assert!(replay_log(file, 0, 0, |_| Ok(())).is_err());
            assert_eq!(file.metadata().unwrap().len(), corrupt.len() as u64);
        });
    }

    #[test]
    fn wal_hardware_crc32c_and_legacy_crc32_both_replay_cleanly() {
        with_log(|file| {
            let pack = Pack::AppendFactsBulk {
                s: vec!["s".into()],
                p: vec!["p".into()],
                o: vec!["o".into()],
            };
            let hw_frame = frame(file, pack);
            assert_eq!(&hw_frame[..4], &LOG_MAGIC_CHECKSUM);

            // Verify the frame written uses hardware Castagnoli CRC32-C
            let end = hw_frame.len() - 4;
            let written_crc = u32::from_le_bytes(hw_frame[end..].try_into().unwrap());
            let expected_crc32c = crate::crc::hardware_crc32c(&hw_frame[8..end]);
            assert_eq!(written_crc, expected_crc32c);

            // Construct an equivalent legacy frame using IEEE 802.3 CRC32
            let mut legacy_frame = hw_frame.clone();
            let legacy_crc = crc32fast::hash(&hw_frame[8..end]);
            legacy_frame[end..].copy_from_slice(&legacy_crc.to_le_bytes());

            // Both frames must replay cleanly without errors
            assert_eq!(for_each_wal_frame(&hw_frame, |_| Ok(())).unwrap(), 1);
            assert_eq!(for_each_wal_frame(&legacy_frame, |_| Ok(())).unwrap(), 1);

            // Any corrupt CRC must fail
            let mut bad_frame = hw_frame.clone();
            bad_frame[end] ^= 0xff;
            assert!(for_each_wal_frame(&bad_frame, |_| Ok(())).is_err());
        });
    }
}

// Benchmark utilities for WAL operations
#[cfg(all(test, feature = "parallel"))]
mod bench {
    use super::*;
    use std::time::Instant;

    #[test]
    fn bench_wal_batch_write() {
        let path = std::env::temp_dir().join(format!("lin-bench-wal-batch-{}", std::process::id()));
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(&path)
            .unwrap();

        let records = (0..1000)
            .map(|i| LogRecord {
                r#gen: i as u64 + 1,
                next_id: i as u64 + 1,
                pack: Pack::InsertBulk {
                    collection: "bench".into(),
                    rows: vec![crate::store::Row::new()],
                    edges: vec![],
                },
            })
            .collect::<Vec<_>>();

        let start = Instant::now();
        append_batch_records(&mut file, &records, SyncMode::Normal, &mut 0).unwrap();
        let elapsed = start.elapsed();

        let mut replayed = 0usize;
        replay_log(&mut file, 0, 0, |_| {
            replayed += 1;
            Ok(())
        })
        .unwrap();
        assert_eq!(replayed, 1000);
        drop(file);
        let _ = fs::remove_file(path);

        println!(
            "BATCH WRITE: {}ms for {} records",
            elapsed.as_millis(),
            records.len()
        );
        assert!(elapsed.as_millis() < 2000, "Batch write too slow");
    }
}

#[cfg(test)]
mod shared_vector_wire_tests {
    use super::*;
    #[test]
    fn shared_vector_columns_keep_legacy_messagepack_and_share_row_storage() {
        use crate::store::{Cell, Row};
        use std::sync::Arc;
        #[derive(Serialize)]
        #[serde(tag = "t", content = "v")]
        enum LegacyColumn {
            Vec(Vec<Option<Vec<f32>>>),
        }
        let values = vec![Some(vec![0.0, -0.0, 1.25]), None, Some(vec![])];
        let legacy = rmp_serde::to_vec_named(&LegacyColumn::Vec(values.clone())).unwrap();
        let shared = ColData::Vec(values.into_iter().map(|v| v.map(Into::into)).collect());
        assert_eq!(rmp_serde::to_vec_named(&shared).unwrap(), legacy);
        let decoded: ColData = rmp_serde::from_slice(&legacy).unwrap();
        assert_eq!(rmp_serde::to_vec_named(&decoded).unwrap(), legacy);
        let embedding: Arc<[f32]> = Arc::from([0.0, -0.0, 1.25]);
        let rows = [Row::from([(
            "embedding".into(),
            Cell::vec_arc(Arc::clone(&embedding)),
        )])];
        let Pack::InsertCols {
            fields, cols, n, ..
        } = rows_to_insert_cols("test", &rows, vec![])
        else {
            panic!("column pack")
        };
        let ColData::Vec(column) = &cols[0] else {
            panic!("vector column")
        };
        assert!(Arc::ptr_eq(column[0].as_ref().unwrap(), &embedding));
        let restored = cols_to_rows(&fields, &cols, n as usize);
        let Cell::Vec(restored_embedding) = &restored[0]["embedding"] else {
            panic!("vector cell")
        };
        assert!(Arc::ptr_eq(restored_embedding, &embedding));
    }
}

#[cfg(test)]
mod dictionary_wal_tests {
    use super::*;
    use std::collections::{BTreeMap, BTreeSet};
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    fn fresh() -> PathBuf {
        std::env::temp_dir().join(format!(
            "lin-dictionary-reader-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ))
    }
    // Independent scalar fixture encoder: sorted dictionaries, unlike the CPU prototype's insertion order.
    fn rows_bytes(rows: &[Option<Vec<u32>>]) -> Vec<u8> {
        let mut out = Vec::new();
        for row in rows {
            let Some(values) = row else {
                out.extend_from_slice(&0u32.to_le_bytes());
                continue;
            };
            out.extend_from_slice(&u32::try_from(values.len() + 1).unwrap().to_le_bytes());
            let entries = values
                .iter()
                .enumerate()
                .filter(|(_, b)| **b != 0)
                .map(|(i, b)| (i, *b))
                .collect::<Vec<_>>();
            out.extend_from_slice(&(entries.len() as u32).to_le_bytes());
            let dictionary = entries
                .iter()
                .map(|(_, b)| *b)
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>();
            let use_dict = !dictionary.is_empty() && dictionary.len() <= 16;
            out.push(if use_dict { dictionary.len() as u8 } else { 0 });
            if use_dict {
                for bits in &dictionary {
                    out.extend_from_slice(&bits.to_le_bytes());
                }
            }
            for (index, bits) in entries {
                if use_dict && values.len() <= 65536 {
                    out.extend_from_slice(&(index as u16).to_le_bytes());
                } else {
                    out.extend_from_slice(&(index as u32).to_le_bytes());
                }
                if use_dict {
                    out.push(dictionary.binary_search(&bits).unwrap() as u8);
                } else {
                    out.extend_from_slice(&bits.to_le_bytes());
                }
            }
        }
        out
    }
    fn payload(n: u32, columns: Vec<(&str, u8, Vec<u8>)>) -> Vec<u8> {
        let mut out = vec![V2_INSERT_COLS];
        out.extend_from_slice(&2u64.to_le_bytes());
        out.extend_from_slice(&3u64.to_le_bytes());
        put_str(&mut out, "docs");
        out.extend_from_slice(&n.to_le_bytes());
        out.extend_from_slice(&(columns.len() as u32).to_le_bytes());
        for (name, tag, bytes) in columns {
            put_str(&mut out, name);
            out.push(tag);
            out.extend(bytes);
        }
        out
    }
    fn dict_payload(row: Vec<u8>) -> Vec<u8> {
        payload(1, vec![("embedding", 8, row)])
    }
    fn bits(rec: LogRecord) -> Vec<Option<Vec<u32>>> {
        let Pack::InsertCols { cols, .. } = rec.pack else {
            panic!("insert")
        };
        let ColData::Vec(rows) = cols.into_iter().next().unwrap() else {
            panic!("vectors")
        };
        rows.into_iter()
            .map(|v| v.map(|v| v.iter().map(|x| x.to_bits()).collect()))
            .collect()
    }
    fn frame(payload: &[u8]) -> Vec<u8> {
        let path = fresh();
        let mut f = OpenOptions::new()
            .create_new(true)
            .read(true)
            .write(true)
            .open(&path)
            .unwrap();
        write_frame(
            &mut f,
            &LOG_MAGIC_DICTIONARY,
            payload,
            SyncMode::Normal,
            &mut 0,
        )
        .unwrap();
        f.seek(SeekFrom::Start(0)).unwrap();
        let mut bytes = Vec::new();
        f.read_to_end(&mut bytes).unwrap();
        drop(f);
        fs::remove_file(path).unwrap();
        bytes
    }
    #[test]
    fn dictionary_materialization_keeps_empty_separate_from_null() {
        let columns = [ColData::Vec(vec![
            None,
            Some(Vec::<f32>::new().into()),
            Some(vec![-0.0].into()),
        ])];
        let rows = cols_to_rows(&["embedding".into()], &columns, 3);
        assert!(matches!(rows[0]["embedding"], crate::Cell::Null));
        assert_eq!(rows[1]["embedding"].as_vec(), Some(&[][..]));
        assert_eq!(
            rows[2]["embedding"].as_vec().unwrap()[0].to_bits(),
            0x80000000
        );
    }
    #[test]
    fn dictionary_mixed_vector_columns_share_budget_and_preserve_bits() {
        let a = rows_bytes(&[Some(vec![0x80000000, 0x7fc00001])]);
        let sparse = [Some(std::sync::Arc::<[f32]>::from([
            f32::from_bits(0x7fc00002),
            -0.0,
        ]))];
        let mut b = Vec::new();
        append_sparse_rows_serial(&sparse, &mut b).unwrap();
        let mut c = Vec::new();
        c.extend_from_slice(&2u32.to_le_bytes());
        c.extend_from_slice(&0x7fa00001u32.to_le_bytes());
        c.extend_from_slice(&0x80000000u32.to_le_bytes());
        let p = payload(1, vec![("a", 8, a), ("b", 7, b), ("c", 6, c)]);
        let rec = decode_column_record_mode(&p, true, true).unwrap();
        let Pack::InsertCols { cols, .. } = rec.pack else {
            panic!("insert")
        };
        let actual = cols
            .into_iter()
            .map(|c| {
                let ColData::Vec(v) = c else { panic!("vector") };
                v[0].as_ref()
                    .unwrap()
                    .iter()
                    .map(|x| x.to_bits())
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        assert_eq!(
            actual,
            vec![
                vec![0x80000000, 0x7fc00001],
                vec![0x7fc00002, 0x80000000],
                vec![0x7fa00001, 0x80000000]
            ]
        );
    }
    fn writer_vectors(n: usize, dim: usize) -> Vec<Option<std::sync::Arc<[f32]>>> {
        let mut values = vec![0.0; dim];
        for i in 0..32 {
            values[i * 7 % dim] =
                f32::from_bits([0x80000000, 0x7fc00001, 0x7f800000, 0x3f800000][i % 4]);
        }
        vec![Some(values.into()); n]
    }
    #[test]
    fn dictionary_writer_policy_boundaries_and_exact_fallbacks() {
        for n in [127, 128, 4095, 4096] {
            let rows = writer_vectors(n, 256);
            let encoded = try_dictionary_vector_bytes(&rows).unwrap();
            assert_eq!(encoded.is_some(), (128..4096).contains(&n));
            if let Some(bytes) = encoded {
                let mut i = 0;
                let mut budget = 0;
                let actual = take_dictionary_vectors(&bytes, &mut i, n, &mut budget).unwrap();
                assert_eq!(i, bytes.len());
                for (a, b) in actual.iter().zip(&rows) {
                    assert_eq!(
                        a.as_ref()
                            .unwrap()
                            .iter()
                            .map(|v| v.to_bits())
                            .collect::<Vec<_>>(),
                        b.as_ref()
                            .unwrap()
                            .iter()
                            .map(|v| v.to_bits())
                            .collect::<Vec<_>>()
                    );
                }
            }
        }
        let dense = (0..768)
            .map(|i| f32::from_bits(0x3f000001 + i))
            .collect::<Vec<_>>();
        assert!(
            try_dictionary_vector_bytes(&vec![Some(dense.into()); 1000])
                .unwrap()
                .is_none()
        );
        let mut mixed = writer_vectors(1000, 768);
        let heavy = std::sync::Arc::<[f32]>::from(
            (0..768)
                .map(|i| f32::from_bits(0x3f000001 + i))
                .collect::<Vec<_>>(),
        );
        for row in &mut mixed[4..] {
            *row = Some(heavy.clone());
        }
        assert!(try_dictionary_vector_bytes(&mixed).unwrap().is_none());
        let mut low = vec![0.0; 768];
        low[0] = 1.0;
        assert!(
            try_dictionary_vector_bytes(&vec![Some(low.into()); 1000])
                .unwrap()
                .is_none()
        );
    }
    #[test]
    fn dictionary_writer_emits_codec5_and_roundtrips_mixed_rows() {
        let mut rows = writer_vectors(1000, 768);
        rows[5] = Some(
            (1..=64)
                .map(|i| f32::from_bits(0x3f000000 + i))
                .collect::<Vec<_>>()
                .into(),
        );
        rows[6] = None;
        rows[7] = Some(Vec::<f32>::new().into());
        let expected = rows
            .iter()
            .map(|v| {
                v.as_ref()
                    .map(|v| v.iter().map(|x| x.to_bits()).collect::<Vec<_>>())
            })
            .collect::<Vec<_>>();
        let path = fresh();
        let mut f = OpenOptions::new()
            .create_new(true)
            .read(true)
            .write(true)
            .open(&path)
            .unwrap();
        let rec = LogRecord {
            r#gen: 1,
            next_id: 2,
            pack: Pack::InsertCols {
                collection: "docs".into(),
                fields: vec!["embedding".into()],
                cols: vec![ColData::Vec(rows)],
                n: 1000,
                edges: vec![],
            },
        };
        append_record(&mut f, &rec, &mut Vec::new(), SyncMode::Normal, &mut 0).unwrap();
        f.seek(SeekFrom::Start(0)).unwrap();
        let mut bytes = Vec::new();
        f.read_to_end(&mut bytes).unwrap();
        assert_eq!(&bytes[..4], &LOG_MAGIC_CHECKSUM);
        assert_eq!(bytes[8], 5);
        assert_eq!(
            bits(decode_frame(&LOG_MAGIC_CHECKSUM, &bytes[8..]).unwrap()),
            expected
        );
        drop(f);
        fs::remove_file(path).unwrap();
    }
    #[test]
    fn dictionary_writer_respects_existing_budget_without_writing_a_frame() {
        let rows = vec![Some(std::sync::Arc::<[f32]>::from(vec![0.0; 20000])); 1000];
        let pack = Pack::InsertCols {
            collection: "docs".into(),
            fields: vec!["embedding".into()],
            cols: vec![ColData::Vec(rows)],
            n: 1000,
            edges: vec![],
        };
        let path = fresh();
        let mut f = OpenOptions::new()
            .create_new(true)
            .read(true)
            .write(true)
            .open(&path)
            .unwrap();
        assert!(
            append_record(
                &mut f,
                &LogRecord {
                    r#gen: 1,
                    next_id: 2,
                    pack
                },
                &mut Vec::new(),
                SyncMode::Normal,
                &mut 0
            )
            .is_err()
        );
        assert_eq!(f.metadata().unwrap().len(), 0);
        drop(f);
        fs::remove_file(path).unwrap();
    }
    #[test]
    fn dictionary_writer_keeps_legacy_when_dictionary_has_no_savings() {
        let path = fresh();
        let mut f = OpenOptions::new()
            .create_new(true)
            .read(true)
            .write(true)
            .open(&path)
            .unwrap();
        let mut v = vec![0.0; 768];
        v[7] = -0.0;
        let rec = LogRecord {
            r#gen: 1,
            next_id: 2,
            pack: Pack::InsertCols {
                collection: "docs".into(),
                fields: vec!["embedding".into()],
                cols: vec![ColData::Vec(vec![Some(std::sync::Arc::from(v)); 1000])],
                n: 1000,
                edges: vec![],
            },
        };
        append_record(&mut f, &rec, &mut Vec::new(), SyncMode::Normal, &mut 0).unwrap();
        f.seek(SeekFrom::Start(0)).unwrap();
        let mut bytes = Vec::new();
        f.read_to_end(&mut bytes).unwrap();
        assert_eq!(&bytes[..4], &LOG_MAGIC_CHECKSUM);
        assert_eq!(bytes[8], 3);
        decode_frame(&LOG_MAGIC_CHECKSUM, &bytes[8..]).unwrap();
        drop(f);
        fs::remove_file(path).unwrap();
    }
    #[test]
    fn dictionary_roundtrip_bits_widths_none_empty_and_raw_escape() {
        let mut rows = vec![
            None,
            Some(vec![]),
            Some(vec![0; 9]),
            Some(vec![
                0x80000000, 0x7fc00001, 0x7fc00002, 0x7fa00001, 0x7f800000, 0xff800000, 0x80000000,
            ]),
        ];
        for dim in [8, 9, 65535, 65536, 65537] {
            let mut v = vec![0; dim];
            v[0] = 0x80000000;
            v[dim / 2] = 0x7fc00001;
            v[dim - 1] = 0x3fc00000;
            rows.push(Some(v));
        }
        rows.push(Some((1..=64).map(|i| 0x3f000000 + i).collect()));
        rows.push(Some((0..32).map(|i| 0x3f000000 + (i % 16) + 1).collect()));
        let p = payload(rows.len() as u32, vec![("embedding", 8, rows_bytes(&rows))]);
        assert_eq!(
            bits(decode_column_record_mode(&p, true, true).unwrap()),
            rows
        );
        let f = frame(&p);
        assert_eq!(&f[..4], &LOG_MAGIC_CHECKSUM);
        assert_eq!(f[8], 5);
        assert_eq!(
            bits(decode_frame(&LOG_MAGIC_CHECKSUM, &f[8..]).unwrap()),
            rows
        );
        let mut ieee = f.clone();
        let end = ieee.len() - 4;
        let crc = crc32fast::hash(&ieee[8..end]).to_le_bytes();
        ieee[end..].copy_from_slice(&crc);
        assert_eq!(
            bits(decode_frame(&LOG_MAGIC_CHECKSUM, &ieee[8..]).unwrap()),
            rows
        );

        assert!(decode_column_record(&p, true).is_err());
        assert!(decode_v2(&p).is_err());
        let mut bad = f.clone();
        bad[15] ^= 1;
        assert!(decode_frame(&LOG_MAGIC_CHECKSUM, &bad[8..]).is_err());
    }
    #[test]
    fn dictionary_rejects_bad_tables_indices_counts_ordinals_and_trailing_bytes() {
        let valid = rows_bytes(&[Some(vec![0x3f800000, 0, 0, 0])]); // d=1, index u16, ordinal at 15.
        assert_eq!(valid.len(), 16);
        let mut cases = Vec::new();
        let mut b = valid.clone();
        b[8] = 17;
        cases.push(b);
        let mut b = valid.clone();
        b[9..13].fill(0);
        cases.push(b);
        let mut b = valid.clone();
        b[13..15].copy_from_slice(&4u16.to_le_bytes());
        cases.push(b);
        let mut b = valid.clone();
        b[15] = 1;
        cases.push(b);
        let mut b = valid.clone();
        b[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
        cases.push(b);
        let mut b = valid.clone();
        b[0..4].copy_from_slice(&u32::MAX.to_le_bytes());
        cases.push(b);
        let mut b = valid.clone();
        b.push(0);
        cases.push(b);
        let two = rows_bytes(&[Some(vec![0x3f800000, 0x40000000, 0, 0])]);
        let mut b = two.clone();
        b[13..17].copy_from_slice(&0x3f800000u32.to_le_bytes());
        cases.push(b); // duplicate dictionary value.
        let mut b = two.clone();
        b[22] = 0;
        cases.push(b); // unused second dictionary entry.
        let mut b = two.clone();
        b[20..22].copy_from_slice(&0u16.to_le_bytes());
        cases.push(b); // duplicate index.
        let mut b = two.clone();
        b[17..19].copy_from_slice(&2u16.to_le_bytes());
        cases.push(b); // descending indexes.
        let mut raw = Vec::new();
        raw.extend_from_slice(&5u32.to_le_bytes());
        raw.extend_from_slice(&1u32.to_le_bytes());
        raw.push(0);
        raw.extend_from_slice(&0u32.to_le_bytes());
        raw.extend_from_slice(&0u32.to_le_bytes());
        cases.push(raw); // redundant +0 raw entry.
        for b in cases {
            let payload = dict_payload(b);
            assert!(decode_column_record_mode(&payload, true, true).is_err());
            let bytes = frame(&payload);
            assert!(decode_frame(&LOG_MAGIC_CHECKSUM, &bytes[8..]).is_err());
        }
        let p = dict_payload(valid);
        for end in 0..p.len() {
            assert!(
                decode_column_record_mode(&p[..end], true, true).is_err(),
                "prefix {end}"
            );
        }
        for tag in [0, 9, 255] {
            let mut b = payload(1, vec![("embedding", tag, vec![])]);
            assert!(decode_column_record_mode(&b, true, true).is_err());
            b.push(0);
        }
        let no_dictionary = payload(1, vec![("embedding", 7, vec![0; 4])]);
        assert!(decode_column_record_mode(&no_dictionary, true, true).is_err());
    }
    #[test]
    fn dictionary_budget_covers_metadata_vectors_and_mixed_dense_columns() {
        let b = rows_bytes(&[Some(vec![0x3f800000])]);
        let mut offset = 0;
        let mut budget = MAX_SPARSE_DECODED_BYTES - std::mem::size_of::<Option<Vec<f32>>>();
        assert!(take_dictionary_vectors(&b, &mut offset, 1, &mut budget).is_err()); // dimension rejected before value allocation.
        let mut offset = 0;
        let mut budget = 0;
        assert!(
            take_dictionary_vectors(
                &[],
                &mut offset,
                MAX_SPARSE_DECODED_BYTES / 24 + 1,
                &mut budget
            )
            .is_err()
        );
        let too_many = payload(
            (MAX_SPARSE_DECODED_BYTES / 24 + 1) as u32,
            vec![("embedding", 8, vec![])],
        );
        assert!(decode_column_record_mode(&too_many, true, true).is_err());
        // Two zero-valued dictionary vectors: 32MiB each plus metadata exceeds shared 64MiB.
        let mut row = Vec::new();
        row.extend_from_slice(&(8_388_608u32 + 1).to_le_bytes());
        row.extend_from_slice(&0u32.to_le_bytes());
        row.push(0);
        let p = payload(1, vec![("a", 8, row.clone()), ("b", 8, row)]);
        assert!(decode_column_record_mode(&p, true, true).is_err());
        // Include legacy dense allocation in codec-5 shared accounting.
        let mut row = Vec::new();
        row.extend_from_slice(&(16_777_210u32 + 1).to_le_bytes());
        row.extend_from_slice(&0u32.to_le_bytes());
        row.push(0);
        let mut dense = Vec::new();
        dense.extend_from_slice(&1u32.to_le_bytes());
        dense.extend_from_slice(&1f32.to_le_bytes());
        assert!(
            decode_column_record_mode(
                &payload(1, vec![("a", 8, row), ("b", 6, dense)]),
                true,
                true
            )
            .is_err()
        );
    }
    #[test]
    fn dictionary_torn_tails_and_complete_corruption_follow_replay_contract() {
        let p = dict_payload(rows_bytes(&[Some(vec![0x80000000, 0x3f800000])]));
        let valid = frame(&p);
        let path = fresh();
        let mut file = OpenOptions::new()
            .create_new(true)
            .read(true)
            .write(true)
            .open(&path)
            .unwrap();
        for end in 0..valid.len() {
            file.set_len(0).unwrap();
            file.seek(SeekFrom::Start(0)).unwrap();
            file.write_all(&valid[..end]).unwrap();
            assert_eq!(
                replay_log(&mut file, 0, 0, |_| panic!("truncated frame applied")).unwrap(),
                0
            );
            assert_eq!(file.metadata().unwrap().len(), end as u64);
        }
        file.set_len(0).unwrap();
        file.seek(SeekFrom::Start(0)).unwrap();
        file.write_all(&valid).unwrap();
        let mut applied = 0;
        assert_eq!(
            replay_log(&mut file, 0, 0, |_| {
                applied += 1;
                Ok(())
            })
            .unwrap(),
            valid.len() as u64
        );
        assert_eq!(applied, 1);
        let mut invalid = valid.clone();
        invalid[12] ^= 1;
        let mut invalid_payload = p.clone();
        *invalid_payload.last_mut().unwrap() = 255;
        let checked_but_invalid = frame(&invalid_payload);
        for bad in [invalid, checked_but_invalid] {
            let mut bytes = valid.clone();
            bytes.extend_from_slice(&bad);
            file.set_len(0).unwrap();
            file.seek(SeekFrom::Start(0)).unwrap();
            file.write_all(&bytes).unwrap();
            assert!(replay_log(&mut file, 0, 0, |_| Ok(())).is_err());
            file.seek(SeekFrom::Start(0)).unwrap();
            let mut after = Vec::new();
            file.read_to_end(&mut after).unwrap();
            assert_eq!(after, bytes);
        }
        drop(file);
        fs::remove_file(path).unwrap();
    }
    #[test]
    fn dictionary_mutated_payloads_never_panic() {
        let p = dict_payload(rows_bytes(&[Some(vec![
            0x80000000, 0x7fc00001, 0, 0x3f800000,
        ])]));
        for i in 0..p.len() {
            for bit in 0..8 {
                let mut b = p.clone();
                b[i] ^= 1 << bit;
                assert!(
                    std::panic::catch_unwind(|| decode_column_record_mode(&b, true, true)).is_ok()
                );
            }
        }
    }
    fn directory_bytes(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
        fn walk(root: &Path, path: &Path, out: &mut BTreeMap<PathBuf, Vec<u8>>) {
            for entry in fs::read_dir(path).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    walk(root, &path, out);
                } else {
                    out.insert(
                        path.strip_prefix(root).unwrap().to_path_buf(),
                        fs::read(path).unwrap(),
                    );
                }
            }
        }
        let mut out = BTreeMap::new();
        walk(root, root, &mut out);
        out
    }
    #[test]
    fn dictionary_legacy_sparse_empty_vector_survives_reopen_and_checkpoint() {
        use crate::{Db, OpenOpts};
        let root = fresh();
        assert!(!root.exists());
        {
            let mut db = Db::open_with(&root, OpenOpts::default()).unwrap();
            db.run("index docs [wing]").unwrap();
        }
        let pack = Pack::InsertCols {
            collection: "docs".into(),
            fields: vec!["id".into(), "uri".into(), "embedding".into()],
            cols: vec![
                ColData::Text(vec![std::sync::Arc::from("legacy-empty")]),
                ColData::Text(vec![std::sync::Arc::from("bench://legacy-empty")]),
                ColData::Vec(vec![Some(Vec::<f32>::new().into())]),
            ],
            n: 1,
            edges: vec![],
        };
        let mut file = open_log(&root).unwrap();
        file.seek(SeekFrom::End(0)).unwrap();
        let start = file.stream_position().unwrap();
        let mut len = start;
        append_record(
            &mut file,
            &LogRecord {
                r#gen: 2,
                next_id: 3,
                pack,
            },
            &mut Vec::new(),
            SyncMode::Full,
            &mut len,
        )
        .unwrap();
        file.seek(SeekFrom::Start(start + 8)).unwrap();
        let mut codec = [0];
        file.read_exact(&mut codec).unwrap();
        assert_eq!(codec[0], 3);
        drop(file);
        {
            let mut db = Db::open_with(&root, OpenOpts::default()).unwrap();
            assert_eq!(
                db.store.collection("docs")[0]["embedding"].as_vec(),
                Some(&[][..])
            );
            db.checkpoint().unwrap();
        }
        {
            let db = Db::open_with(&root, OpenOpts::default()).unwrap();
            assert_eq!(
                db.store.collection("docs")[0]["embedding"].as_vec(),
                Some(&[][..])
            );
        }
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn dictionary_reopen_fixture_and_old_reader_rejection() {
        use crate::{Db, OpenOpts};
        let preserve = std::env::var_os("LIN_DICTIONARY_FIXTURE").map(PathBuf::from);
        let root = preserve.clone().unwrap_or_else(fresh);
        assert!(!root.exists(), "refuse existing fixture");
        {
            let mut db = Db::open_with(&root, OpenOpts::default()).unwrap();
            db.run("index docs [wing]").unwrap();
            assert_eq!(db.r#gen(), 1);
        }
        let old = std::env::var_os("LIN_DICTIONARY_OLD_READER");
        if let Some(binary) = &old {
            let r = std::process::Command::new(binary)
                .arg(&root)
                .output()
                .unwrap();
            assert!(
                r.status.success(),
                "old reader must open pre-dictionary fixture: {:?}",
                r.stderr
            );
        }
        let expected = vec![
            Some(vec![
                0x80000000, 0x7fc00001, 0x7fa00001, 0x7f800000, 0xff800000, 0, 0x3f800000,
                0x80000000,
            ]),
            Some(vec![]),
        ];
        let mut ids = Vec::new();
        for id in ["dict-0", "dict-1"] {
            put_str(&mut ids, id);
        }
        let mut uris = Vec::new();
        for uri in ["bench://dict-0", "bench://dict-1"] {
            put_str(&mut uris, uri);
        }
        let p = payload(
            2,
            vec![
                ("id", 1, ids),
                ("uri", 1, uris),
                ("embedding", 8, rows_bytes(&expected)),
            ],
        );
        let mut file = open_log(&root).unwrap();
        file.seek(SeekFrom::End(0)).unwrap();
        let mut len = file.metadata().unwrap().len();
        write_frame(
            &mut file,
            &LOG_MAGIC_DICTIONARY,
            &p,
            SyncMode::Full,
            &mut len,
        )
        .unwrap();
        drop(file);
        if let Some(binary) = &old {
            let before = directory_bytes(&root);
            let r = std::process::Command::new(binary)
                .arg(&root)
                .output()
                .unwrap();
            assert_eq!(r.status.code(), Some(2));
            assert_eq!(
                directory_bytes(&root),
                before,
                "old reader must not change bytes"
            );
            println!(
                "DICT_COMPAT {}",
                serde_json::json!({"old_reader_exit":2,"files_unchanged":true,"files":before.iter().map(|(p,b)|(p.display().to_string(),b.len())).collect::<BTreeMap<_,_>>(),"stderr":String::from_utf8_lossy(&r.stderr)})
            );
        }
        let assert_rows = |db: &Db| {
            let actual = db
                .store
                .collection("docs")
                .iter()
                .map(|r| {
                    r.get("embedding")
                        .and_then(|x| x.as_vec())
                        .map(|v| v.iter().map(|x| x.to_bits()).collect::<Vec<_>>())
                })
                .collect::<Vec<_>>();
            assert_eq!(actual, expected);
        };
        let before_read = directory_bytes(&root);
        {
            let read = Db::open_read(&root).unwrap();
            let handle = read.run("docs | { embedding } | take all").unwrap();
            let actual = handle
                .rows
                .iter()
                .map(|row| {
                    row.get("embedding")
                        .and_then(|cell| cell.as_vec())
                        .map(|v| v.iter().map(|x| x.to_bits()).collect::<Vec<_>>())
                })
                .collect::<Vec<_>>();
            assert_eq!(actual, expected);
        }
        assert_eq!(
            directory_bytes(&root),
            before_read,
            "read-only replay must not mutate fixture"
        );
        if preserve.is_none() {
            {
                let mut db = Db::open_with(&root, OpenOpts::default()).unwrap();
                assert_rows(&db);
                db.checkpoint().unwrap();
            }
            {
                let db = Db::open_with(&root, OpenOpts::default()).unwrap();
                assert_rows(&db);
            }
            fs::remove_dir_all(root).unwrap();
        }
    }
}
