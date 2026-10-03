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
//! - **v3:** magic `LIN\x06` + `u32` LE envelope length + codec byte (1/2/3)
//!   + encoded payload + CRC32 LE of codec/payload; all new writes use v3
//! - **legacy:** `u32` LE length + JSON(`LogRecord`) — still replayed on open
//!
//! A truncated trailing record is ignored. After replay the log is truncated
//! to the last complete record. Checkpoint compacts the log to empty.
//! Portable backup = self-contained `LIN\x04` MessagePack (never cold stubs).
//! FTS blobs are local accel; missing/stale files rebuild on open.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
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
pub const BACKUP_MAGIC: [u8; 4] = *b"LIN\x04";
pub const SNAPSHOT_EVERY: u32 = 32;
const MAX_RECORD: u32 = 16 * 1024 * 1024;
/// New WAL framing magic (`LIN` + version).
/// `\x01` = MessagePack(LogRecord); `\x02` = raw columnar hot packs.
const LOG_MAGIC_V1: [u8; 4] = *b"LIN\x01";
const LOG_MAGIC_V2: [u8; 4] = *b"LIN\x02";
// Internal codec identity; new sparse records are only written inside LIN\x06.
const LOG_MAGIC_SPARSE: [u8; 4] = *b"LIN\x03";
const MAX_SPARSE_DECODED_BYTES: usize = 64 * 1024 * 1024;
const SPARSE_VECTOR_THRESHOLD: usize = 64 * 1024;
/// Checksummed envelope: codec byte (1/2/3), encoded record, CRC32 of codec + record.
const LOG_MAGIC_CHECKSUM: [u8; 4] = *b"LIN\x06";
const V2_FACTS_BULK: u8 = 1;
const V2_INSERT_COLS: u8 = 2;

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
    Vec(Vec<Option<Vec<f32>>>),
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
    let bytes = fs::read(&path).map_err(io_err)?;
    if bytes.is_empty() {
        return Ok(None);
    }
    Ok(decode_snapshot_bytes(&bytes))
}

pub fn write_snapshot(dir: &Path, snap: &Snapshot) -> Result<(), Error> {
    let bytes = encode_snapshot_bytes(snap)?;
    atomic_write(&snapshot_path(dir), &bytes)
}

/// Portable backup: always self-contained MessagePack (`LIN\x04`).
pub fn write_backup(path: &Path, snap: &Snapshot) -> Result<(), Error> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent).map_err(io_err)?;
    }
    let mut snap = snap.clone();
    // Portable: never ship cold stubs — rows must already be inlined.
    snap.cold_collections.clear();
    let bytes = encode_snapshot_bytes(&snap)?;
    atomic_write(path, &bytes)
}

pub fn read_backup(path: &Path) -> Result<Snapshot, Error> {
    let bytes = fs::read(path).map_err(io_err)?;
    if bytes.is_empty() {
        return Err(io_err("empty backup"));
    }
    decode_snapshot_bytes(&bytes).ok_or_else(|| io_err("corrupt backup"))
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

fn encode_snapshot_bytes(snap: &Snapshot) -> Result<Vec<u8>, Error> {
    let mut body = Vec::new();
    rmp_serde::encode::write_named(&mut body, snap).map_err(io_err)?;
    let mut out = Vec::with_capacity(4 + body.len());
    out.extend_from_slice(&SNAPSHOT_MAGIC);
    out.extend_from_slice(&body);
    Ok(out)
}

fn decode_snapshot_bytes(bytes: &[u8]) -> Option<Snapshot> {
    if bytes.len() >= 4 && bytes[..4] == SNAPSHOT_MAGIC {
        return rmp_serde::from_slice(&bytes[4..]).ok();
    }
    // Legacy JSON snapshot / backup.
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
        file.sync_all().map_err(io_err)?;
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

/// Append one record and optionally durability-flush the log ([`SyncMode::Full`]).
/// Hot packs use a raw columnar codec; others use MessagePack. Both are wrapped
/// in checksummed v3 frames. Legacy v1/v2/JSON still replay.
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
    let codec = [magic[3]];
    let mut checksum = crc32fast::Hasher::new();
    checksum.update(&codec);
    checksum.update(payload);
    let checksum = checksum.finalize().to_le_bytes();
    let mut hdr = [0u8; 8];
    hdr[..4].copy_from_slice(&LOG_MAGIC_CHECKSUM);
    hdr[4..].copy_from_slice(&(len as u32).to_le_bytes());
    log.write_all(&hdr).map_err(io_err)?;
    log.write_all(&codec).map_err(io_err)?;
    log.write_all(payload).map_err(io_err)?;
    log.write_all(&checksum).map_err(io_err)?;
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
        return file.sync_data();
    }
    #[cfg(not(target_vendor = "apple"))]
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

fn use_sparse_vectors(rows: &[Option<Vec<f32>>]) -> bool {
    let dense = rows
        .iter()
        .map(|row| 4usize.saturating_add(row.as_ref().map_or(0, |v| v.len().saturating_mul(4))))
        .fold(0usize, usize::saturating_add);
    if rows
        .iter()
        .any(|row| row.as_ref().is_some_and(Vec::is_empty))
    {
        return true; // Preserve Some(empty), which the legacy vector tag cannot express.
    }
    if dense < SPARSE_VECTOR_THRESHOLD {
        return false;
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
    buf.clear();
    buf.reserve(64 + fields.len() * 16 + (n as usize) * 32);
    buf.push(V2_INSERT_COLS);
    buf.extend_from_slice(&rec_gen.to_le_bytes());
    buf.extend_from_slice(&next_id.to_le_bytes());
    put_str(buf, collection);
    buf.extend_from_slice(&n.to_le_bytes());
    buf.extend_from_slice(&(fields.len() as u32).to_le_bytes());
    let mut sparse_codec = false;
    let mut sparse_decoded_bytes = 0usize;
    for (f, c) in fields.iter().zip(cols.iter()) {
        put_str(buf, f);
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
                for row in v {
                    let Some(emb) = row else {
                        buf.extend_from_slice(&0u32.to_le_bytes());
                        continue;
                    };
                    let bytes = emb
                        .len()
                        .checked_mul(4)
                        .ok_or_else(|| io_err("sparse vector size overflow"))?;
                    sparse_decoded_bytes = sparse_decoded_bytes
                        .checked_add(bytes)
                        .filter(|&bytes| bytes <= MAX_SPARSE_DECODED_BYTES)
                        .ok_or_else(|| io_err("sparse vectors exceed decoded 64MiB budget"))?;
                    let dim = u32::try_from(emb.len())
                        .ok()
                        .and_then(|n| n.checked_add(1))
                        .ok_or_else(|| io_err("sparse vector dimension overflow"))?;
                    let count = emb.iter().filter(|value| value.to_bits() != 0).count() as u32;
                    buf.extend_from_slice(&dim.to_le_bytes());
                    buf.extend_from_slice(&count.to_le_bytes());
                    for (block, values) in emb.chunks(8).enumerate() {
                        // Bitwise presence includes negative zero and NaN payloads.
                        let mut mask =
                            values
                                .iter()
                                .enumerate()
                                .fold(0u8, |mask, (offset, value)| {
                                    mask | (u8::from(value.to_bits() != 0) << offset)
                                });
                        while mask != 0 {
                            let offset = mask.trailing_zeros() as usize;
                            mask &= mask - 1;
                            let index = block * 8 + offset;
                            buf.extend_from_slice(&(index as u32).to_le_bytes());
                            buf.extend_from_slice(&values[offset].to_bits().to_le_bytes());
                        }
                    }
                }
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
                            for x in emb {
                                buf.extend_from_slice(&x.to_le_bytes());
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
        if sparse_codec {
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
    loop {
        match read_one(log, file_len, pos) {
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

fn read_one(log: &mut File, file_len: u64, pos: u64) -> ReadOne {
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
        let mut buf = vec![0u8; len as usize];
        match log.read_exact(&mut buf) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return ReadOne::Truncated,
            Err(e) => return ReadOne::Io(io_err(e)),
        }
        let rec = match decode_frame(&head, &buf) {
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
    let mut buf = vec![0u8; len as usize];
    match log.read_exact(&mut buf) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return ReadOne::Truncated,
        Err(e) => return ReadOne::Io(io_err(e)),
    }
    match serde_json::from_slice::<LogRecord>(&buf) {
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

fn decode_column_record(buf: &[u8], allow_sparse: bool) -> Result<LogRecord, ()> {
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
                            let mut emb = Vec::with_capacity(dim);
                            for _ in 0..dim {
                                emb.push(f32::from_le_bytes(buf[i..i + 4].try_into().unwrap()));
                                i += 4;
                            }
                            v.push(Some(emb));
                        }
                        ColData::Vec(v)
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
                            rows.push(Some(values));
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
    let mut out = Vec::new();
    let mut pos = 0u64;
    loop {
        match read_one(&mut log, file_len, pos) {
            ReadOne::Eof | ReadOne::Truncated => break,
            ReadOne::Corrupt => return Err(io_err("corrupt log")),
            ReadOne::Io(e) => return Err(e),
            ReadOne::Ok { rec, next } => {
                if rec.r#gen > since_gen {
                    // Re-read raw frame bytes [pos, next).
                    let len = (next - pos) as usize;
                    let mut frame = vec![0u8; len];
                    log.seek(SeekFrom::Start(pos)).map_err(io_err)?;
                    log.read_exact(&mut frame).map_err(io_err)?;
                    out.extend_from_slice(&frame);
                    log.seek(SeekFrom::Start(next)).map_err(io_err)?;
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
        if crc32fast::hash(&payload[..end]) != expected {
            return Err(io_err("wal checksum mismatch"));
        }
        let codec = match payload[0] {
            1 => LOG_MAGIC_V1,
            2 => LOG_MAGIC_V2,
            3 => LOG_MAGIC_SPARSE,
            _ => return Err(io_err("unknown wal codec")),
        };
        return decode_frame(&codec, &payload[1..end]);
    }
    if *magic == LOG_MAGIC_SPARSE {
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
                Some(Cell::Text(_)) => {
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
                    v.push(cell.and_then(Cell::as_vec).map(|s| s.to_vec()));
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
                    .map(|s| Cell::text_arc(std::sync::Arc::clone(s)))
                    .unwrap_or_else(|| Cell::text_arc("")),
                Some(ColData::Int(v)) => Cell::Int(v.get(i).copied().unwrap_or(0)),
                Some(ColData::Float(v)) => Cell::Float(v.get(i).copied().unwrap_or(0.0)),
                Some(ColData::Bool(v)) => Cell::Bool(v.get(i).copied().unwrap_or(false)),
                Some(ColData::Time(v)) => Cell::Time(v.get(i).copied().unwrap_or(0)),
                Some(ColData::Vec(v)) => match v.get(i).and_then(|o| o.as_ref()) {
                    Some(emb) if !emb.is_empty() => Cell::vec_arc(emb.as_slice()),
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
            cols: vec![ColData::Vec(vectors)],
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
                let bits = |rows: &[Option<Vec<f32>>]| {
                    rows.iter()
                        .map(|row| {
                            row.as_ref()
                                .map(|v| v.iter().map(|x| x.to_bits()).collect::<Vec<_>>())
                        })
                        .collect::<Vec<_>>()
                };
                assert_eq!(bits(actual), bits(&expected));
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
}
