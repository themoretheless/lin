//! On-disk layout of a durable Lin data dir (default `./.lin`):
//!
//! ```text
//! <data>/
//!   head       JSON object: gen, catalog_hash, embed_id
//!   log        append-only length-prefixed JSON records
//!   snapshot   optional full-store checkpoint (JSON)
//! ```
//!
//! Log record framing: `u32` little-endian payload length, then that many
//! UTF-8 JSON bytes. A truncated trailing record (short length prefix or
//! short payload) is ignored — crash before `fsync` looks like the write
//! never happened. After replay the log is truncated to the last complete
//! record so later appends do not land after garbage.
//!
//! Record body: `{"gen":N,"next_id":N,"pack":{"type":"insert"|"delete"|"schema_index"|…}}`.
//! Snapshot: collections + edges + `log_offset` (byte position after the
//! last record included in the checkpoint). Written after every 32 commits
//! and on [`Db::close`](crate::Db::close) / [`Db::checkpoint`](crate::Db::checkpoint).
//! After a successful snapshot the log is **compacted** (truncated to 0) so
//! disk stays bounded; `log_offset` in the published snapshot is 0.

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
pub const SNAPSHOT_EVERY: u32 = 32;
const MAX_RECORD: u32 = 16 * 1024 * 1024;

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
    Batch {
        packs: Vec<Pack>,
    },
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
}

#[derive(Debug)]
pub struct Persist {
    pub dir: PathBuf,
    pub log: File,
    /// Advisory exclusive flock on `LOCK` — released when Persist drops.
    pub lock: File,
    pub catalog_hash: String,
    pub writes_since_snapshot: u32,
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

pub fn log_len(dir: &Path) -> Result<u64, Error> {
    let path = log_path(dir);
    if !path.exists() {
        return Ok(0);
    }
    Ok(fs::metadata(&path).map_err(io_err)?.len())
}

/// Exclusive advisory lock for a writer. Errors if another writer holds the dir.
pub fn acquire_writer_lock(dir: &Path) -> Result<File, Error> {
    ensure_dir(dir)?;
    let f = OpenOptions::new()
        .create(true)
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
    Ok(f)
}

/// Shared advisory lock for cold readers (allows multiple readers, blocks writers).
pub fn acquire_reader_lock(dir: &Path) -> Result<File, Error> {
    let path = lock_path(dir);
    if !path.exists() {
        // No writer has created LOCK yet — open/create for shared.
        let _ = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(&path)
            .map_err(io_err)?;
    }
    let f = OpenOptions::new()
        .read(true)
        .open(&path)
        .map_err(io_err)?;
    flock_nb(&f, false).map_err(|e| {
        if e.kind() == io::ErrorKind::WouldBlock {
            Error::runtime("data dir locked by another writer")
        } else {
            io_err(e)
        }
    })?;
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
    match serde_json::from_slice(&bytes) {
        Ok(s) => Ok(Some(s)),
        Err(_) => Ok(None),
    }
}

pub fn write_snapshot(dir: &Path, snap: &Snapshot) -> Result<(), Error> {
    let bytes = serde_json::to_vec(snap).map_err(io_err)?;
    atomic_write(&snapshot_path(dir), &bytes)
}

/// Portable backup file (same JSON shape as on-disk `snapshot`, any path).
pub fn write_backup(path: &Path, snap: &Snapshot) -> Result<(), Error> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent).map_err(io_err)?;
    }
    let bytes = serde_json::to_vec(snap).map_err(io_err)?;
    atomic_write(path, &bytes)
}

pub fn read_backup(path: &Path) -> Result<Snapshot, Error> {
    let bytes = fs::read(path).map_err(io_err)?;
    if bytes.is_empty() {
        return Err(io_err("empty backup"));
    }
    serde_json::from_slice(&bytes).map_err(io_err)
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), Error> {
    let tmp = path.with_extension("tmp");
    {
        let mut f = File::create(&tmp).map_err(io_err)?;
        f.write_all(bytes).map_err(io_err)?;
        f.sync_all().map_err(io_err)?;
    }
    fs::rename(&tmp, path).map_err(io_err)?;
    if let Some(parent) = path.parent() {
        let _ = sync_dir(parent);
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

/// Append one record and durability-flush the log. Must complete before `gen` bumps.
/// Uses `sync_data` (content) rather than full `sync_all` — head/metadata catch up on checkpoint.
pub fn append_record(log: &mut File, rec: &LogRecord) -> Result<(), Error> {
    let payload = serde_json::to_vec(rec).map_err(io_err)?;
    if payload.len() > MAX_RECORD as usize {
        return Err(io_err("log record exceeds 16MiB"));
    }
    let len = payload.len() as u32;
    log.write_all(&len.to_le_bytes()).map_err(io_err)?;
    log.write_all(&payload).map_err(io_err)?;
    // Data durability without forcing inode metadata on every pack.
    log.sync_data().map_err(io_err)?;
    Ok(())
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
    let mut len_buf = [0u8; 4];
    match log.read_exact(&mut len_buf) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return ReadOne::Truncated,
        Err(e) => return ReadOne::Io(io_err(e)),
    }
    let len = u32::from_le_bytes(len_buf);
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
