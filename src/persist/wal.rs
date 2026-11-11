//! Write-Ahead Log (WAL) framing, locking, atomic file durability, replay, and replication export.

use std::cell::RefCell;
use std::fs::{self, File, OpenOptions};
use std::io::{self, IoSlice, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use crate::error::Error;

use super::codec::{
    append_facts_v2, append_insert_cols_v2, decode_column_record, decode_column_record_mode,
    decode_v2,
};
use super::types::{
    FENCE_NAME, HEAD_NAME, Head, LOCK_NAME, LOG_MAGIC_CHECKSUM, LOG_MAGIC_DICTIONARY,
    LOG_MAGIC_SPARSE, LOG_MAGIC_V1, LOG_MAGIC_V2, LOG_NAME, LogRecord, MAX_RECORD, Pack,
    SNAPSHOT_NAME, SyncMode, V2_FACTS_BULK, V2_INSERT_COLS,
};

thread_local! {
    static FRAME_WRITE_SCRATCH: RefCell<Vec<u8>> = RefCell::new(Vec::with_capacity(64 * 1024));
}


pub fn io_err(e: impl std::fmt::Display) -> Error {
    Error::runtime(format!("persist: {e}"))
}

pub fn validate_data_dir(path: &Path) -> Result<(), Error> {
    if path
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(Error::runtime(
            "data-directory traversal rejected: path cannot contain '..'",
        ));
    }
    Ok(())
}

pub fn ensure_dir(path: &Path) -> Result<(), Error> {
    validate_data_dir(path)?;
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
    validate_data_dir(dir)?;
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
pub(super) fn flock_nb(f: &File, exclusive: bool) -> io::Result<()> {
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
pub(super) fn flock_nb(_f: &File, _exclusive: bool) -> io::Result<()> {
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

pub(crate) fn write_head_no_dir_sync(dir: &Path, head: &Head) -> Result<(), Error> {
    let bytes = serde_json::to_vec(head).map_err(io_err)?;
    let path = head_path(dir);
    write_through_tmp(&path, &path.with_extension("tmp"), &bytes)
}

pub(crate) fn sync_dir_checked(dir: &Path) -> Result<(), Error> {
    sync_dir(dir).map_err(io_err)
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


pub(super) fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), Error> {
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
pub fn write_through_tmp(path: &Path, tmp: &Path, bytes: &[u8]) -> Result<(), Error> {
    let result = (|| -> Result<(), Error> {
        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(tmp)
            .map_err(io_err)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(tmp, fs::Permissions::from_mode(0o600));
        }
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

pub(super) fn sync_dir(dir: &Path) -> io::Result<()> {
    let f = File::open(dir)?;
    f.sync_all()
}

// Best-effort physical allocation only; logical WAL bytes/length and sync stay unchanged.
pub(super) fn reserve_wal_space(log: &File, additional: usize, empty_only: bool) {
    #[cfg(target_vendor = "apple")]
    {
        use std::os::unix::{fs::MetadataExt, io::AsRawFd};
        const CHUNK: u64 = 1024 * 1024;
        let Ok(metadata) = log.metadata() else {
            return;
        };
        if empty_only && metadata.len() != 0 {
            return;
        }
        let Ok(additional) = u64::try_from(additional) else {
            return;
        };
        let Some(required) = metadata.len().checked_add(additional) else {
            return;
        };
        let allocated = metadata.blocks().saturating_mul(512);
        if allocated >= required {
            return;
        }
        let Some(rounded) = required.checked_add(CHUNK - 1).map(|n| n / CHUNK * CHUNK) else {
            return;
        };
        // Do not reserve an arbitrarily large sparse-file backlog.
        let length = rounded
            .saturating_sub(allocated)
            .min(additional.saturating_add(CHUNK));
        let Ok(length) = i64::try_from(length) else {
            return;
        };
        if length == 0 {
            return;
        }
        let mut request = libc::fstore_t {
            fst_flags: libc::F_ALLOCATEALL,
            fst_posmode: libc::F_PEOFPOSMODE,
            fst_offset: 0,
            fst_length: length,
            fst_bytesalloc: 0,
        };
        // Unsupported filesystem / allocation failure retains the ordinary write path.
        let _ = unsafe { libc::fcntl(log.as_raw_fd(), libc::F_PREALLOCATE, &mut request) };
    }
    #[cfg(not(target_vendor = "apple"))]
    let _ = (log, additional, empty_only);
}

pub fn open_log(dir: &Path) -> Result<File, Error> {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(log_path(dir))
        .map_err(io_err)?;
    reserve_wal_space(&file, 1024 * 1024, true);
    Ok(file)
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
#[cfg(all(test, feature = "parallel"))]
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

pub(super) fn append_record_v1(
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

pub(super) fn write_all_vectored(log: &mut File, slices: &[IoSlice<'_>]) -> io::Result<()> {
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

pub(super) fn write_frame_bytes(
    log: &mut File,
    codec_byte: u8,
    payload: &[u8],
    checksum: [u8; 4],
    len: u32,
) -> io::Result<()> {
    const INLINE_MAX: usize = 4096;
    if payload.len() >= 64 * 1024 {
        reserve_wal_space(log, payload.len().saturating_add(13), false);
    }
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
        let mut hdr = [0u8; 9];
        hdr[..4].copy_from_slice(&LOG_MAGIC_CHECKSUM);
        hdr[4..8].copy_from_slice(&len.to_le_bytes());
        hdr[8] = codec_byte;
        let slices = [
            IoSlice::new(&hdr),
            IoSlice::new(payload),
            IoSlice::new(&checksum),
        ];
        write_all_vectored(log, &slices)
    }
}

pub(crate) fn compute_frame_checksum(magic: &[u8; 4], payload: &[u8]) -> [u8; 4] {
    let codec = magic[3];
    let mut checksum = crate::crc::Hasher::new();
    checksum.update(&[codec]);
    checksum.update(payload);
    checksum.finalize().to_le_bytes()
}

pub(crate) fn write_frame_precomputed(
    log: &mut File,
    magic: &[u8; 4],
    payload: &[u8],
    checksum: [u8; 4],
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
    write_frame_bytes(log, codec, payload, checksum, len as u32).map_err(io_err)?;

    if sync == SyncMode::Full {
        durable_sync(log).map_err(io_err)?;
    }
    *log_bytes += 8 + len as u64;
    Ok(())
}

pub(crate) fn write_frame(
    log: &mut File,
    magic: &[u8; 4],
    payload: &[u8],
    sync: SyncMode,
    log_bytes: &mut u64,
) -> Result<(), Error> {
    let checksum = compute_frame_checksum(magic, payload);
    write_frame_precomputed(log, magic, payload, checksum, sync, log_bytes)
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


/// Replay complete records with `gen > min_gen`. Returns the byte offset of
/// the last good record (file should be truncated there).
pub fn replay_log(
    log: &mut File,
    min_gen: u64,
    start_offset: u64,
    mut on_rec: impl FnMut(LogRecord) -> Result<(), Error>,
) -> Result<u64, Error> {
    replay_log_indexed(log, min_gen, start_offset, None, &mut on_rec)
}

pub(crate) fn replay_log_indexed(
    log: &mut File,
    min_gen: u64,
    start_offset: u64,
    mut wal_index: Option<&mut Vec<(u64, u64)>>,
    on_rec: &mut impl FnMut(LogRecord) -> Result<(), Error>,
) -> Result<u64, Error> {
    let file_len = log.metadata().map_err(io_err)?.len();
    let start = if start_offset <= file_len {
        start_offset
    } else {
        0
    };
    if let Some(idx) = wal_index.as_deref_mut() {
        idx.clear();
    }
    match replay_from(log, min_gen, start, file_len, wal_index.as_deref_mut(), on_rec) {
        Ok(end) => Ok(end),
        Err(ReplayFail::BadStart) if start > 0 => {
            if let Some(idx) = wal_index.as_deref_mut() {
                idx.clear();
            }
            replay_from(log, min_gen, 0, file_len, wal_index, on_rec).map_err(|e| match e {
                ReplayFail::Io(err) => err,
                ReplayFail::BadStart => io_err("corrupt log"),
            })
        }
        Err(ReplayFail::Io(e)) => Err(e),
        Err(ReplayFail::BadStart) => Err(io_err("corrupt log")),
    }
}

pub(super) enum ReplayFail {
    BadStart,
    Io(Error),
}

pub(super) fn replay_from(
    log: &mut File,
    min_gen: u64,
    start: u64,
    file_len: u64,
    mut wal_index: Option<&mut Vec<(u64, u64)>>,
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
                if let Some(idx) = wal_index.as_deref_mut() {
                    idx.push((rec.r#gen, pos));
                }
                if rec.r#gen > min_gen {
                    on_rec(rec).map_err(ReplayFail::Io)?;
                }
                pos = next;
            }
        }
    }
    Ok(pos)
}

pub(super) enum ReadOne {
    Ok { rec: LogRecord, next: u64 },
    Eof,
    Truncated,
    Corrupt,
    Io(Error),
}

pub(super) fn read_one(log: &mut File, file_len: u64, pos: u64, buf: &mut Vec<u8>) -> ReadOne {
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


pub fn truncate_log(log: &mut File, end: u64) -> Result<(), Error> {
    let current_len = log.metadata().map_err(io_err)?.len();
    if current_len != end {
        log.set_len(end).map_err(io_err)?;
        log.seek(SeekFrom::End(0)).map_err(io_err)?;
        durable_sync(log).or_else(|_| log.sync_all()).map_err(io_err)?;
    } else {
        log.seek(SeekFrom::End(0)).map_err(io_err)?;
    }
    Ok(())
}

/// After a durable snapshot of the full memory image, drop the covered log prefix
/// so the file does not grow forever. Sets length to 0 and seeks to start.
pub fn compact_log(log: &mut File) -> Result<(), Error> {
    truncate_log(log, 0)
}

/// Peek `rec_gen` from a v2/sparse/dictionary payload without full record deserialization.
#[inline]
pub(super) fn peek_column_gen(payload: &[u8]) -> Option<u64> {
    if payload.len() >= 9 && (payload[0] == V2_FACTS_BULK || payload[0] == V2_INSERT_COLS) {
        Some(u64::from_le_bytes(payload[1..9].try_into().ok()?))
    } else {
        None
    }
}

/// Export raw WAL frames with `gen > since_gen` using an optional in-memory `(gen, offset)` index.
pub fn export_wal_since_indexed(
    dir: &Path,
    since_gen: u64,
    wal_index: &[(u64, u64)],
) -> Result<Vec<u8>, Error> {
    if let Some(&(last_gen, _)) = wal_index.last() {
        if last_gen <= since_gen {
            return Ok(Vec::new());
        }
        let idx = wal_index.partition_point(|&(g, _)| g <= since_gen);
        if let Some(&(_, start_offset)) = wal_index.get(idx)
            && (idx > 0 || wal_index.first().is_some_and(|&(_, off)| off == 0))
        {
            let mut log = match open_log_read(dir)? {
                Some(f) => f,
                None => return Ok(Vec::new()),
            };
            let file_len = log.metadata().map_err(io_err)?.len();
            if start_offset <= file_len {
                log.seek(SeekFrom::Start(start_offset)).map_err(io_err)?;
                let mut out = Vec::with_capacity((file_len - start_offset) as usize);
                log.read_to_end(&mut out).map_err(io_err)?;
                return Ok(out);
            }
        }
    }
    export_wal_since(dir, since_gen)
}

/// Export raw WAL frames with `gen > since_gen` (network / follower shipping).
pub fn export_wal_since(dir: &Path, since_gen: u64) -> Result<Vec<u8>, Error> {
    let mut log = match open_log_read(dir)? {
        Some(f) => f,
        None => return Ok(Vec::new()),
    };
    let file_len = log.metadata().map_err(io_err)?.len();
    if file_len == 0 {
        return Ok(Vec::new());
    }
    let mut out = Vec::with_capacity(file_len.min(64 * 1024) as usize);
    let mut pos = 0u64;
    let mut buf = Vec::with_capacity(8192);
    loop {
        if pos >= file_len {
            break;
        }
        // Fast header + CRC check for modern LIN\x06 + v2/sparse/dict frames when skipping old generations,
        // avoiding full LogRecord / vector / BTreeMap deserialization while preserving corruption detection.
        let mut hdr = [0u8; 18];
        if pos + 18 <= file_len
            && log.seek(SeekFrom::Start(pos)).is_ok()
            && log.read_exact(&mut hdr).is_ok()
        {
            let magic: [u8; 4] = hdr[0..4].try_into().unwrap();
            let len = u32::from_le_bytes(hdr[4..8].try_into().unwrap());
            let next = pos + 8 + u64::from(len);
            if magic == LOG_MAGIC_CHECKSUM
                && len >= 14
                && len <= MAX_RECORD
                && next <= file_len
                && matches!(hdr[8], 2 | 3 | 5)
                && let Some(rec_gen) = peek_column_gen(&hdr[9..18])
                && rec_gen <= since_gen
            {
                let payload_len = len as usize;
                buf.resize(payload_len, 0);
                buf[..10].copy_from_slice(&hdr[8..18]);
                if log.read_exact(&mut buf[10..]).is_ok() {
                    let end = payload_len - 4;
                    let expected = u32::from_le_bytes(buf[end..].try_into().unwrap());
                    let hw_crc = crate::crc::hardware_crc32c(&buf[..end]);
                    if hw_crc == expected || crc32fast::hash(&buf[..end]) == expected {
                        pos = next;
                        continue;
                    }
                    return Err(io_err("corrupt log"));
                }
            }
            let _ = log.seek(SeekFrom::Start(pos));
        } else {
            let _ = log.seek(SeekFrom::Start(pos));
        }

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

pub(super) fn decode_frame(magic: &[u8; 4], payload: &[u8]) -> Result<LogRecord, Error> {
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
pub fn for_each_wal_frame_raw<'a>(
    frames: &'a [u8],
    mut on_frame: impl FnMut(&'a [u8], LogRecord) -> Result<(), Error>,
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
    append_validated_frames(log, frames, sync, log_bytes)
}

/// Append pre-validated raw WAL frames without repeating frame decoding / CRC checks.
pub(crate) fn append_validated_frames(
    log: &mut File,
    frames: &[u8],
    sync: SyncMode,
    log_bytes: &mut u64,
) -> Result<(), Error> {
    if frames.is_empty() {
        return Ok(());
    }
    if frames.len() >= 64 * 1024 {
        reserve_wal_space(log, frames.len(), false);
    }
    log.write_all(frames).map_err(io_err)?;
    if sync == SyncMode::Full {
        durable_sync(log).map_err(io_err)?;
    }
    *log_bytes += frames.len() as u64;
    Ok(())
}

/// Append pre-validated raw WAL frames slices via vectored I/O without intermediate buffer copy.
pub(crate) fn append_validated_frame_slices(
    log: &mut File,
    slices: &[IoSlice<'_>],
    total_len: usize,
    sync: SyncMode,
    log_bytes: &mut u64,
) -> Result<(), Error> {
    if total_len == 0 || slices.is_empty() {
        return Ok(());
    }
    if total_len >= 64 * 1024 {
        reserve_wal_space(log, total_len, false);
    }
    write_all_vectored(log, slices).map_err(io_err)?;
    if sync == SyncMode::Full {
        durable_sync(log).map_err(io_err)?;
    }
    *log_bytes += total_len as u64;
    Ok(())
}

