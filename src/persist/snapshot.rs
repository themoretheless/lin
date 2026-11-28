//! Snapshot and portable backup serialization, parallel chunked encoding, and mmap decoding.

use std::cell::RefCell;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;

use crate::error::Error;
use crate::store::Row;

use super::codec::{decode_lcol_to_rows, encode_insert_rows_lcol};
use super::types::{
    LOG_MAGIC_SPARSE, SNAPSHOT_MAGIC, Snapshot, SnapshotHeader, SnapshotHeaderRef, SnapshotRef,
};
use super::wal::{durable_sync, io_err, snapshot_path, sync_dir};

type ShapeCacheSlot = Option<(Vec<String>, std::sync::Arc<crate::store::RowShape>)>;

struct SnapTlsEncode {
    raw_buf: Vec<u8>,
    #[cfg(feature = "compress-zstd")]
    compressor: Option<zstd::bulk::Compressor<'static>>,
}

impl SnapTlsEncode {
    const fn new() -> Self {
        Self {
            raw_buf: Vec::new(),
            #[cfg(feature = "compress-zstd")]
            compressor: None,
        }
    }
}

struct SnapTlsDecode {
    #[cfg(feature = "compress-zstd")]
    decompressor: Option<zstd::bulk::Decompressor<'static>>,
    #[cfg(feature = "compress-zstd")]
    decomp_buf: Vec<u8>,
    shape_cache: ShapeCacheSlot,
}

impl SnapTlsDecode {
    const fn new() -> Self {
        Self {
            #[cfg(feature = "compress-zstd")]
            decompressor: None,
            #[cfg(feature = "compress-zstd")]
            decomp_buf: Vec::new(),
            shape_cache: None,
        }
    }
}

thread_local! {
    static SNAP_TLS_ENC: RefCell<SnapTlsEncode> = const { RefCell::new(SnapTlsEncode::new()) };
    static SNAP_TLS_DEC: RefCell<SnapTlsDecode> = const { RefCell::new(SnapTlsDecode::new()) };
}

/// Maximum distinct collections permitted when decoding a snapshot (`LIN-0172`).
pub const MAX_SNAPSHOT_COLLECTIONS: usize = 65_536;
/// Maximum graph edges permitted when decoding a snapshot (`LIN-0172`).
pub const MAX_SNAPSHOT_EDGES: usize = 67_108_864;
/// Maximum byte length permitted for any single snapshot header or compressed/uncompressed section (`LIN-0171`).
pub const MAX_SNAPSHOT_SECTION_BYTES: usize = 256 * 1024 * 1024;

pub fn read_snapshot(dir: &Path) -> Result<Option<Snapshot>, Error> {
    let path = snapshot_path(dir);
    let file = match fs::File::open(&path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(io_err(e)),
    };
    let meta = file.metadata().map_err(io_err)?;
    let len = meta.len();
    if len == 0 {
        return Ok(None);
    }
    if len >= 16 * 1024 {
        let mmap = unsafe { memmap2::MmapOptions::new().map(&file) }.map_err(io_err)?;
        #[cfg(unix)]
        let _ = mmap.advise(memmap2::Advice::Sequential);
        match decode_snapshot_bytes_inner(&mmap) {
            Ok(s) => Ok(Some(s)),
            Err(SnapshotDecodeFail::Limit(e)) => Err(e),
            Err(SnapshotDecodeFail::Corrupt) => Err(io_err("corrupt snapshot")),
        }
    } else {
        use std::io::Read;
        let mut bytes = Vec::with_capacity(len as usize);
        let mut reader = std::io::BufReader::new(file);
        reader.read_to_end(&mut bytes).map_err(io_err)?;
        match decode_snapshot_bytes_inner(&bytes) {
            Ok(s) => Ok(Some(s)),
            Err(SnapshotDecodeFail::Limit(e)) => Err(e),
            Err(SnapshotDecodeFail::Corrupt) => Err(io_err("corrupt snapshot")),
        }
    }
}

pub(super) const SNAPSHOT_CRC_MAGIC: [u8; 4] = *b"LCRC";

/// Encode a snapshot ref to a writer using the chunked parallel snapshot format (`0x03` + `LIN\x04`).
///
/// Appends an 8-byte `LCRC` + CRC32C trailer (little endian) over the entire preceding snapshot
/// while keeping the `0x03` leading version byte so previous releases (`v0.7.1`) still decode
/// the chunks and current builds detect corruption or partial trailer truncation; earlier `0x03`
/// files without a trailer also decode.
pub fn encode_snapshot_ref_to_writer<W: std::io::Write>(
    writer: &mut W,
    snap: &SnapshotRef<'_>,
) -> Result<(), Error> {
    let mut hashing = CrcWriter {
        inner: writer,
        crc: crate::crc::Hasher::new(),
    };
    encode_chunked_body(&mut hashing, snap)?;
    hashing.write_all(&SNAPSHOT_CRC_MAGIC).map_err(io_err)?;
    let crc = hashing.crc.finalize();
    writer.write_all(&crc.to_le_bytes()).map_err(io_err)
}

/// Passes writes through while feeding them to a CRC32C.
struct CrcWriter<'a, W: std::io::Write> {
    inner: &'a mut W,
    crc: crate::crc::Hasher,
}

impl<W: std::io::Write> std::io::Write for CrcWriter<'_, W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let n = self.inner.write(buf)?;
        self.crc.update(&buf[..n]);
        Ok(n)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

/// Version byte, magic, header and chunks of a checksummed (`0x03` + CRC32C trailer) snapshot.
fn encode_chunked_body<W: std::io::Write>(
    writer: &mut W,
    snap: &SnapshotRef<'_>,
) -> Result<(), Error> {
    writer.write_all(&[0x03]).map_err(io_err)?;
    writer.write_all(&SNAPSHOT_MAGIC).map_err(io_err)?;

    let header = SnapshotHeaderRef {
        r#gen: snap.r#gen,
        embed_id: snap.embed_id,
        next_id: snap.next_id,
        catalog_hash: snap.catalog_hash,
        log_offset: snap.log_offset,
        edges: snap.edges,
        extra_collections: snap.extra_collections,
        extra_rels: snap.extra_rels,
        extra_indexes: snap.extra_indexes,
        extra_filters: snap.extra_filters,
        extra_owned: snap.extra_owned,
        cold_collections: snap.cold_collections,
    };
    let header_bytes = rmp_serde::encode::to_vec_named(&header).map_err(io_err)?;
    writer
        .write_all(&(header_bytes.len() as u32).to_le_bytes())
        .map_err(io_err)?;
    writer.write_all(&header_bytes).map_err(io_err)?;

    const CHUNK_ROWS: usize = 512;
    struct ChunkDef<'a> {
        name: &'a str,
        rows: &'a [Row],
    }
    let mut chunk_defs = Vec::new();
    for (name, rows) in snap.collections {
        if rows.is_empty() {
            chunk_defs.push(ChunkDef {
                name: name.as_str(),
                rows: &[],
            });
        } else {
            for chunk in rows.chunks(CHUNK_ROWS) {
                chunk_defs.push(ChunkDef {
                    name: name.as_str(),
                    rows: chunk,
                });
            }
        }
    }

    let encode_chunk = |rows: &[Row]| -> Result<Vec<u8>, Error> {
        if rows.is_empty() {
            return Ok(Vec::new());
        }
        SNAP_TLS_ENC.with(|cell| {
            let mut tls = cell.borrow_mut();
            let SnapTlsEncode {
                raw_buf,
                #[cfg(feature = "compress-zstd")]
                compressor,
            } = &mut *tls;
            if !encode_insert_rows_lcol(rows, raw_buf) {
                raw_buf.clear();
                rmp_serde::encode::write_named(&mut *raw_buf, rows).map_err(io_err)?;
            }
            #[cfg(feature = "compress-zstd")]
            {
                if compressor.is_none() {
                    *compressor = zstd::bulk::Compressor::new(1).ok();
                }
                if let Some(comp) = compressor.as_mut()
                    && let Ok(compressed) = comp.compress(raw_buf)
                {
                    return Ok(compressed);
                }
                zstd::encode_all(&raw_buf[..], 1).map_err(io_err)
            }
            #[cfg(not(feature = "compress-zstd"))]
            {
                Ok(raw_buf.clone())
            }
        })
    };

    let encoded_chunks: Vec<Result<Vec<u8>, Error>> = if chunk_defs.len() > 1 {
        #[cfg(feature = "parallel")]
        {
            use rayon::prelude::*;
            chunk_defs.par_iter().map(|c| encode_chunk(c.rows)).collect()
        }
        #[cfg(not(feature = "parallel"))]
        {
            let num_workers = std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(4)
                .min(8)
                .min(chunk_defs.len());
            if num_workers > 1 {
                let block_len = chunk_defs.len().div_ceil(num_workers);
                std::thread::scope(|s| {
                    let mut handles = Vec::with_capacity(num_workers);
                    for block in chunk_defs.chunks(block_len) {
                        handles.push(s.spawn(move || {
                            let mut part = Vec::with_capacity(block.len());
                            for c in block {
                                part.push(encode_chunk(c.rows));
                            }
                            part
                        }));
                    }
                    let mut all = Vec::with_capacity(chunk_defs.len());
                    for h in handles {
                        match h.join() {
                            Ok(part) => all.extend(part),
                            Err(_) => all.push(Err(io_err("snapshot encode worker panicked"))),
                        }
                    }
                    all
                })
            } else {
                chunk_defs.iter().map(|c| encode_chunk(c.rows)).collect()
            }
        }
    } else {
        chunk_defs.iter().map(|c| encode_chunk(c.rows)).collect()
    };

    writer
        .write_all(&(chunk_defs.len() as u32).to_le_bytes())
        .map_err(io_err)?;
    for (c, encoded) in chunk_defs.iter().zip(encoded_chunks) {
        let data = encoded?;
        let name_bytes = c.name.as_bytes();
        writer
            .write_all(&(name_bytes.len() as u16).to_le_bytes())
            .map_err(io_err)?;
        writer.write_all(name_bytes).map_err(io_err)?;
        writer
            .write_all(&(data.len() as u32).to_le_bytes())
            .map_err(io_err)?;
        if !data.is_empty() {
            writer.write_all(&data).map_err(io_err)?;
        }
    }
    Ok(())
}

/// Write snapshot directly to a file via a temporary file without intermediate full-image in-memory buffering.
pub fn write_snapshot_ref_to_file(path: &Path, snap: &SnapshotRef<'_>) -> Result<(), Error> {
    write_snapshot_ref_to_file_ext(path, snap, true)
}

pub(crate) fn write_snapshot_ref_to_file_ext(
    path: &Path,
    snap: &SnapshotRef<'_>,
    sync_parent: bool,
) -> Result<(), Error> {
    let tmp = path.with_extension("bin.tmp");
    let result = (|| -> Result<(), Error> {
        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&tmp)
            .map_err(io_err)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&tmp, fs::Permissions::from_mode(0o600));
        }
        let mut writer = std::io::BufWriter::with_capacity(128 * 1024, file);
        encode_snapshot_ref_to_writer(&mut writer, snap)?;
        writer.flush().map_err(io_err)?;
        let file = writer.into_inner().map_err(io_err)?;
        durable_sync(&file)
            .or_else(|_| file.sync_all())
            .map_err(io_err)?;
        drop(file);
        fs::rename(&tmp, path).map_err(io_err)?;
        if sync_parent
            && let Some(parent) = path.parent()
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
    if path
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(Error::runtime(
            "path traversal rejected: backup path cannot contain '..'",
        ));
    }
    if path.is_symlink() {
        return Err(Error::runtime("backup symlink policy: symlinks rejected"));
    }
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
    if path
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(Error::runtime(
            "path traversal rejected: backup path cannot contain '..'",
        ));
    }
    if path.is_symlink() {
        return Err(Error::runtime("backup symlink policy: symlinks rejected"));
    }
    let file = fs::File::open(path).map_err(io_err)?;
    let meta = file.metadata().map_err(io_err)?;
    let len = meta.len();
    if len == 0 {
        return Err(io_err("empty backup"));
    }
    if len >= 16 * 1024 {
        let mmap = unsafe { memmap2::MmapOptions::new().map(&file) }.map_err(io_err)?;
        #[cfg(unix)]
        let _ = mmap.advise(memmap2::Advice::Sequential);
        match decode_snapshot_bytes_inner(&mmap) {
            Ok(s) => Ok(s),
            Err(SnapshotDecodeFail::Limit(e)) => Err(e),
            Err(SnapshotDecodeFail::Corrupt) => Err(io_err("corrupt backup")),
        }
    } else {
        use std::io::Read;
        let mut bytes = Vec::with_capacity(len as usize);
        let mut reader = std::io::BufReader::new(file);
        reader.read_to_end(&mut bytes).map_err(io_err)?;
        match decode_snapshot_bytes_inner(&bytes) {
            Ok(s) => Ok(s),
            Err(SnapshotDecodeFail::Limit(e)) => Err(e),
            Err(SnapshotDecodeFail::Corrupt) => Err(io_err("corrupt backup")),
        }
    }
}

#[cfg(test)]
pub fn encode_snapshot_ref_bytes(snap: &SnapshotRef<'_>) -> Result<Vec<u8>, Error> {
    let mut out = Vec::with_capacity(64 * 1024);
    encode_snapshot_ref_to_writer(&mut out, snap)?;
    Ok(out)
}

#[cfg(test)]
pub(super) fn encode_snapshot_bytes(snap: &Snapshot) -> Result<Vec<u8>, Error> {
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
    encode_snapshot_ref_bytes(&snap_ref)
}

/// Encode snapshot with optional compression (feature-gated)
#[cfg(all(
    test,
    any(feature = "compress-flate", feature = "compress-lz4", feature = "compress-zstd")
))]
pub fn encode_snapshot_compressed(snap: &Snapshot) -> Result<Vec<u8>, Error> {
    let mut body = Vec::new();
    rmp_serde::encode::write_named(&mut body, snap).map_err(io_err)?;

    #[cfg(feature = "compress-zstd")]
    {
        let compressed = zstd::encode_all(&body[..], 3).map_err(io_err)?;
        let mut out = Vec::with_capacity(5 + compressed.len());
        out.push(0x04); // Zstd flag
        out.extend_from_slice(&SNAPSHOT_MAGIC);
        out.extend_from_slice(&compressed);
        return Ok(out);
    }

    #[cfg(all(not(feature = "compress-zstd"), feature = "compress-flate"))]
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

    #[cfg(all(not(feature = "compress-zstd"), not(feature = "compress-flate"), feature = "compress-lz4"))]
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

enum SnapshotDecodeFail {
    Corrupt,
    Limit(Error),
}

fn msgpack_read_map_len(bytes: &[u8], pos: &mut usize) -> Option<usize> {
    let tag = *bytes.get(*pos)?;
    *pos += 1;
    match tag {
        0x80..=0x8f => Some((tag & 0x0f) as usize),
        0xde => {
            let n = u16::from_be_bytes(bytes.get(*pos..*pos + 2)?.try_into().ok()?) as usize;
            *pos += 2;
            Some(n)
        }
        0xdf => {
            let n = u32::from_be_bytes(bytes.get(*pos..*pos + 4)?.try_into().ok()?) as usize;
            *pos += 4;
            Some(n)
        }
        _ => None,
    }
}

fn msgpack_read_array_len(bytes: &[u8], pos: &mut usize) -> Option<usize> {
    let tag = *bytes.get(*pos)?;
    *pos += 1;
    match tag {
        0x90..=0x9f => Some((tag & 0x0f) as usize),
        0xdc => {
            let n = u16::from_be_bytes(bytes.get(*pos..*pos + 2)?.try_into().ok()?) as usize;
            *pos += 2;
            Some(n)
        }
        0xdd => {
            let n = u32::from_be_bytes(bytes.get(*pos..*pos + 4)?.try_into().ok()?) as usize;
            *pos += 4;
            Some(n)
        }
        _ => None,
    }
}

fn msgpack_read_str<'a>(bytes: &'a [u8], pos: &mut usize) -> Option<&'a str> {
    let tag = *bytes.get(*pos)?;
    *pos += 1;
    let len = match tag {
        0xa0..=0xbf => (tag & 0x1f) as usize,
        0xd9 => {
            let n = *bytes.get(*pos)? as usize;
            *pos += 1;
            n
        }
        0xda => {
            let n = u16::from_be_bytes(bytes.get(*pos..*pos + 2)?.try_into().ok()?) as usize;
            *pos += 2;
            n
        }
        0xdb => {
            let n = u32::from_be_bytes(bytes.get(*pos..*pos + 4)?.try_into().ok()?) as usize;
            *pos += 4;
            n
        }
        _ => return None,
    };
    let s = std::str::from_utf8(bytes.get(*pos..*pos + len)?).ok()?;
    *pos += len;
    Some(s)
}

fn msgpack_skip_value(bytes: &[u8], pos: &mut usize, depth: usize) -> Option<()> {
    if depth > 32 {
        return None;
    }
    let tag = *bytes.get(*pos)?;
    *pos += 1;
    match tag {
        0x00..=0x7f | 0xc0 | 0xc2 | 0xc3 | 0xe0..=0xff => {}
        0x80..=0x8f => {
            let n = (tag & 0x0f) as usize;
            for _ in 0..n {
                msgpack_skip_value(bytes, pos, depth + 1)?;
                msgpack_skip_value(bytes, pos, depth + 1)?;
            }
        }
        0x90..=0x9f => {
            let n = (tag & 0x0f) as usize;
            for _ in 0..n {
                msgpack_skip_value(bytes, pos, depth + 1)?;
            }
        }
        0xa0..=0xbf => {
            let len = (tag & 0x1f) as usize;
            *pos = pos.checked_add(len)?;
            if *pos > bytes.len() {
                return None;
            }
        }
        0xc4 | 0xd9 => {
            let len = *bytes.get(*pos)? as usize;
            *pos = pos.checked_add(1 + len)?;
            if *pos > bytes.len() {
                return None;
            }
        }
        0xc5 | 0xda => {
            let len = u16::from_be_bytes(bytes.get(*pos..*pos + 2)?.try_into().ok()?) as usize;
            *pos = pos.checked_add(2 + len)?;
            if *pos > bytes.len() {
                return None;
            }
        }
        0xc6 | 0xdb => {
            let len = u32::from_be_bytes(bytes.get(*pos..*pos + 4)?.try_into().ok()?) as usize;
            *pos = pos.checked_add(4 + len)?;
            if *pos > bytes.len() {
                return None;
            }
        }
        0xc7 => {
            let len = *bytes.get(*pos)? as usize;
            *pos = pos.checked_add(2 + len)?;
            if *pos > bytes.len() {
                return None;
            }
        }
        0xc8 => {
            let len = u16::from_be_bytes(bytes.get(*pos..*pos + 2)?.try_into().ok()?) as usize;
            *pos = pos.checked_add(3 + len)?;
            if *pos > bytes.len() {
                return None;
            }
        }
        0xc9 => {
            let len = u32::from_be_bytes(bytes.get(*pos..*pos + 4)?.try_into().ok()?) as usize;
            *pos = pos.checked_add(5 + len)?;
            if *pos > bytes.len() {
                return None;
            }
        }
        0xcc | 0xd0 => {
            *pos = pos.checked_add(1)?;
            if *pos > bytes.len() {
                return None;
            }
        }
        0xcd | 0xd1 | 0xd4 => {
            *pos = pos.checked_add(2)?;
            if *pos > bytes.len() {
                return None;
            }
        }
        0xd5 => {
            *pos = pos.checked_add(3)?;
            if *pos > bytes.len() {
                return None;
            }
        }
        0xca | 0xce | 0xd2 => {
            *pos = pos.checked_add(4)?;
            if *pos > bytes.len() {
                return None;
            }
        }
        0xd6 => {
            *pos = pos.checked_add(5)?;
            if *pos > bytes.len() {
                return None;
            }
        }
        0xcb | 0xcf | 0xd3 => {
            *pos = pos.checked_add(8)?;
            if *pos > bytes.len() {
                return None;
            }
        }
        0xd7 => {
            *pos = pos.checked_add(9)?;
            if *pos > bytes.len() {
                return None;
            }
        }
        0xd8 => {
            *pos = pos.checked_add(17)?;
            if *pos > bytes.len() {
                return None;
            }
        }
        0xdc => {
            let n = u16::from_be_bytes(bytes.get(*pos..*pos + 2)?.try_into().ok()?) as usize;
            *pos += 2;
            for _ in 0..n {
                msgpack_skip_value(bytes, pos, depth + 1)?;
            }
        }
        0xdd => {
            let n = u32::from_be_bytes(bytes.get(*pos..*pos + 4)?.try_into().ok()?) as usize;
            *pos += 4;
            for _ in 0..n {
                msgpack_skip_value(bytes, pos, depth + 1)?;
            }
        }
        0xde => {
            let n = u16::from_be_bytes(bytes.get(*pos..*pos + 2)?.try_into().ok()?) as usize;
            *pos += 2;
            for _ in 0..n {
                msgpack_skip_value(bytes, pos, depth + 1)?;
                msgpack_skip_value(bytes, pos, depth + 1)?;
            }
        }
        0xdf => {
            let n = u32::from_be_bytes(bytes.get(*pos..*pos + 4)?.try_into().ok()?) as usize;
            *pos += 4;
            for _ in 0..n {
                msgpack_skip_value(bytes, pos, depth + 1)?;
                msgpack_skip_value(bytes, pos, depth + 1)?;
            }
        }
        _ => return None,
    }
    Some(())
}

/// Pre-validate top-level MessagePack header counts before `rmp_serde` allocates huge `Vec`/`Map`
/// capacities on crafted headers (`LIN-0171`, `LIN-0172`).
pub(super) fn validate_msgpack_header_limits(bytes: &[u8]) -> Result<(), Error> {
    let mut pos = 0usize;
    let Some(fields) = msgpack_read_map_len(bytes, &mut pos) else {
        return Ok(());
    };
    if fields > 64 {
        return Err(Error::runtime(format!(
            "snapshot header field count {fields} exceeds supported schema bounds"
        )));
    }
    for _ in 0..fields {
        let Some(key) = msgpack_read_str(bytes, &mut pos) else {
            return Ok(());
        };
        match key {
            "edges" => {
                let mut peek = pos;
                if let Some(edge_len) = msgpack_read_array_len(bytes, &mut peek)
                    && edge_len > MAX_SNAPSHOT_EDGES
                {
                    return Err(Error::runtime(format!(
                        "snapshot edges count {edge_len} exceeds MAX_SNAPSHOT_EDGES ({MAX_SNAPSHOT_EDGES})"
                    )));
                }
            }
            "collections" | "extra_collections" => {
                let mut peek = pos;
                if let Some(col_len) = msgpack_read_map_len(bytes, &mut peek)
                    && col_len > MAX_SNAPSHOT_COLLECTIONS
                {
                    return Err(Error::runtime(format!(
                        "snapshot {key} count {col_len} exceeds MAX_SNAPSHOT_COLLECTIONS ({MAX_SNAPSHOT_COLLECTIONS})"
                    )));
                }
            }
            "cold_collections" => {
                let mut peek = pos;
                if let Some(col_len) = msgpack_read_array_len(bytes, &mut peek)
                    && col_len > MAX_SNAPSHOT_COLLECTIONS
                {
                    return Err(Error::runtime(format!(
                        "snapshot cold_collections count {col_len} exceeds MAX_SNAPSHOT_COLLECTIONS ({MAX_SNAPSHOT_COLLECTIONS})"
                    )));
                }
            }
            _ => {}
        }
        if msgpack_skip_value(bytes, &mut pos, 0).is_none() {
            return Ok(());
        }
    }
    Ok(())
}

fn validate_decoded_snapshot_limits(snap: &Snapshot) -> Result<(), SnapshotDecodeFail> {
    if snap.collections.len() > MAX_SNAPSHOT_COLLECTIONS {
        return Err(SnapshotDecodeFail::Limit(Error::runtime(format!(
            "snapshot collections count {} exceeds MAX_SNAPSHOT_COLLECTIONS ({MAX_SNAPSHOT_COLLECTIONS})",
            snap.collections.len()
        ))));
    }
    if snap.extra_collections.len() > MAX_SNAPSHOT_COLLECTIONS {
        return Err(SnapshotDecodeFail::Limit(Error::runtime(format!(
            "snapshot extra_collections count {} exceeds MAX_SNAPSHOT_COLLECTIONS ({MAX_SNAPSHOT_COLLECTIONS})",
            snap.extra_collections.len()
        ))));
    }
    if snap.cold_collections.len() > MAX_SNAPSHOT_COLLECTIONS {
        return Err(SnapshotDecodeFail::Limit(Error::runtime(format!(
            "snapshot cold_collections count {} exceeds MAX_SNAPSHOT_COLLECTIONS ({MAX_SNAPSHOT_COLLECTIONS})",
            snap.cold_collections.len()
        ))));
    }
    if snap.edges.len() > MAX_SNAPSHOT_EDGES {
        return Err(SnapshotDecodeFail::Limit(Error::runtime(format!(
            "snapshot edges count {} exceeds MAX_SNAPSHOT_EDGES ({MAX_SNAPSHOT_EDGES})",
            snap.edges.len()
        ))));
    }
    Ok(())
}

/// Decode raw snapshot bytes with explicit section and collection/edge bounds enforcement
/// (`LIN-0171`, `LIN-0172`).
pub fn decode_snapshot_bytes_checked(bytes: &[u8]) -> Result<Snapshot, Error> {
    match decode_snapshot_bytes_inner(bytes) {
        Ok(s) => Ok(s),
        Err(SnapshotDecodeFail::Limit(e)) => Err(e),
        Err(SnapshotDecodeFail::Corrupt) => Err(io_err("corrupt snapshot")),
    }
}

pub(crate) fn decode_snapshot_bytes(bytes: &[u8]) -> Option<Snapshot> {
    decode_snapshot_bytes_inner(bytes).ok()
}

fn decode_snapshot_bytes_inner(bytes: &[u8]) -> Result<Snapshot, SnapshotDecodeFail> {
    if bytes.len() >= 5 && bytes[1..5] == SNAPSHOT_MAGIC {
        let compressor = bytes[0];
        let compressed_data = &bytes[5..];
        match compressor {
            0x01 => {
                #[cfg(feature = "compress-flate")]
                {
                    use flate2::read::GzDecoder;
                    use std::io::Read;

                    let decoder = GzDecoder::new(compressed_data);
                    let mut result = Vec::new();
                    let mut limited = decoder.take((MAX_SNAPSHOT_SECTION_BYTES as u64) + 1);
                    limited
                        .read_to_end(&mut result)
                        .map_err(|_| SnapshotDecodeFail::Corrupt)?;
                    if result.len() > MAX_SNAPSHOT_SECTION_BYTES {
                        return Err(SnapshotDecodeFail::Limit(Error::runtime(format!(
                            "snapshot decompressed section exceeds MAX_SNAPSHOT_SECTION_BYTES ({MAX_SNAPSHOT_SECTION_BYTES})"
                        ))));
                    }
                    validate_msgpack_header_limits(&result).map_err(SnapshotDecodeFail::Limit)?;
                    let snap: Snapshot = rmp_serde::from_slice(&result)
                        .map_err(|_| SnapshotDecodeFail::Corrupt)?;
                    validate_decoded_snapshot_limits(&snap)?;
                    return Ok(snap);
                }
                #[cfg(not(feature = "compress-flate"))]
                {
                    let _ = compressed_data;
                    return Err(SnapshotDecodeFail::Corrupt);
                }
            }
            0x02 => {
                #[cfg(feature = "compress-lz4")]
                {
                    let max_i32 = i32::try_from(MAX_SNAPSHOT_SECTION_BYTES).unwrap_or(i32::MAX);
                    let decompressed = lz4::block::decompress(compressed_data, Some(max_i32))
                        .map_err(|_| SnapshotDecodeFail::Corrupt)?;
                    if decompressed.len() > MAX_SNAPSHOT_SECTION_BYTES {
                        return Err(SnapshotDecodeFail::Limit(Error::runtime(format!(
                            "snapshot decompressed section exceeds MAX_SNAPSHOT_SECTION_BYTES ({MAX_SNAPSHOT_SECTION_BYTES})"
                        ))));
                    }
                    validate_msgpack_header_limits(&decompressed)
                        .map_err(SnapshotDecodeFail::Limit)?;
                    let snap: Snapshot = rmp_serde::from_slice(&decompressed)
                        .map_err(|_| SnapshotDecodeFail::Corrupt)?;
                    validate_decoded_snapshot_limits(&snap)?;
                    return Ok(snap);
                }
                #[cfg(not(feature = "compress-lz4"))]
                {
                    let _ = compressed_data;
                    return Err(SnapshotDecodeFail::Corrupt);
                }
            }
            0x03 => return decode_chunked_snapshot_checked(compressed_data, Some(bytes)),
            0x04 => {
                #[cfg(feature = "compress-zstd")]
                {
                    use std::io::Read;
                    if let Some(exact_len) = zstd_frame_content_size(compressed_data)
                        && exact_len > MAX_SNAPSHOT_SECTION_BYTES
                    {
                        return Err(SnapshotDecodeFail::Limit(Error::runtime(format!(
                            "snapshot zstd frame size {exact_len} exceeds MAX_SNAPSHOT_SECTION_BYTES ({MAX_SNAPSHOT_SECTION_BYTES})"
                        ))));
                    }
                    let decoder = zstd::Decoder::new(compressed_data)
                        .map_err(|_| SnapshotDecodeFail::Corrupt)?;
                    let mut limited = decoder.take((MAX_SNAPSHOT_SECTION_BYTES as u64) + 1);
                    let mut result = Vec::new();
                    limited
                        .read_to_end(&mut result)
                        .map_err(|_| SnapshotDecodeFail::Corrupt)?;
                    if result.len() > MAX_SNAPSHOT_SECTION_BYTES {
                        return Err(SnapshotDecodeFail::Limit(Error::runtime(format!(
                            "snapshot decompressed section exceeds MAX_SNAPSHOT_SECTION_BYTES ({MAX_SNAPSHOT_SECTION_BYTES})"
                        ))));
                    }
                    validate_msgpack_header_limits(&result).map_err(SnapshotDecodeFail::Limit)?;
                    let snap: Snapshot = rmp_serde::from_slice(&result)
                        .map_err(|_| SnapshotDecodeFail::Corrupt)?;
                    validate_decoded_snapshot_limits(&snap)?;
                    return Ok(snap);
                }
                #[cfg(not(feature = "compress-zstd"))]
                {
                    let _ = compressed_data;
                    return Err(SnapshotDecodeFail::Corrupt);
                }
            }
            _ => return Err(SnapshotDecodeFail::Corrupt),
        }
    }

    // Uncompressed snapshot or legacy JSON backup
    if bytes.len() >= 4 && bytes[..4] == SNAPSHOT_MAGIC {
        let body = &bytes[4..];
        if body.len() > MAX_SNAPSHOT_SECTION_BYTES {
            return Err(SnapshotDecodeFail::Limit(Error::runtime(format!(
                "snapshot body size {} exceeds MAX_SNAPSHOT_SECTION_BYTES ({MAX_SNAPSHOT_SECTION_BYTES})",
                body.len()
            ))));
        }
        validate_msgpack_header_limits(body).map_err(SnapshotDecodeFail::Limit)?;
        let snap: Snapshot =
            rmp_serde::from_slice(body).map_err(|_| SnapshotDecodeFail::Corrupt)?;
        validate_decoded_snapshot_limits(&snap)?;
        return Ok(snap);
    }
    if bytes.len() > MAX_SNAPSHOT_SECTION_BYTES {
        return Err(SnapshotDecodeFail::Limit(Error::runtime(format!(
            "snapshot json size {} exceeds MAX_SNAPSHOT_SECTION_BYTES ({MAX_SNAPSHOT_SECTION_BYTES})",
            bytes.len()
        ))));
    }
    let snap: Snapshot = serde_json::from_slice(bytes).map_err(|_| SnapshotDecodeFail::Corrupt)?;
    validate_decoded_snapshot_limits(&snap)?;
    Ok(snap)
}

pub(super) struct ChunkSlice<'a> {
    name: &'a str,
    data: &'a [u8],
}

pub(super) const ZSTD_MAGIC: [u8; 4] = [0x28, 0xb5, 0x2f, 0xfd];

/// Parse the uncompressed `Frame_Content_Size` from a single-frame Zstandard header (RFC 8878 §3.1.1.1).
#[cfg(feature = "compress-zstd")]
#[inline]
fn zstd_frame_content_size(frame: &[u8]) -> Option<usize> {
    if frame.len() < 6 || frame[0..4] != ZSTD_MAGIC {
        return None;
    }
    let fhd = frame[4];
    let fcs_flag = fhd >> 6;
    let single_segment = (fhd & 0x20) != 0;
    let dict_id_bytes = match fhd & 0x03 {
        0 => 0usize,
        1 => 1,
        2 => 2,
        _ => 4,
    };
    let off = 5 + usize::from(!single_segment) + dict_id_bytes;
    let size_u64 = match fcs_flag {
        0 if single_segment => u64::from(*frame.get(off)?),
        0 => return None,
        1 => u64::from(u16::from_le_bytes(frame.get(off..off + 2)?.try_into().ok()?)) + 256,
        2 => u64::from(u32::from_le_bytes(frame.get(off..off + 4)?.try_into().ok()?)),
        _ => u64::from_le_bytes(frame.get(off..off + 8)?.try_into().ok()?),
    };
    usize::try_from(size_u64).ok()
}

#[inline]
fn decode_snapshot_payload_cached(
    payload: &[u8],
    shape_cache: &mut ShapeCacheSlot,
) -> Result<Vec<Row>, ()> {
    if payload.len() >= 8 && &payload[0..4] == b"LCOL" {
        let allow_sparse = payload[4..8] == LOG_MAGIC_SPARSE;
        return decode_lcol_to_rows(&payload[8..], allow_sparse, shape_cache);
    }
    rmp_serde::from_slice(payload).map_err(|_| ())
}

#[allow(dead_code)]
pub(super) fn decode_snapshot_payload(payload: &[u8]) -> Result<Vec<Row>, ()> {
    decode_snapshot_payload_cached(payload, &mut None)
}

pub(super) fn decode_snapshot_chunk<'a>(c: &ChunkSlice<'a>) -> Result<(&'a str, Vec<Row>), ()> {
    if c.data.is_empty() {
        return Ok((c.name, Vec::new()));
    }
    SNAP_TLS_DEC.with(|cell| {
        let mut tls = cell.borrow_mut();
        let SnapTlsDecode {
            #[cfg(feature = "compress-zstd")]
            decompressor,
            #[cfg(feature = "compress-zstd")]
            decomp_buf,
            shape_cache,
        } = &mut *tls;
        if c.data.len() >= 4 && c.data[0..4] == ZSTD_MAGIC {
            #[cfg(feature = "compress-zstd")]
            {
                const MAX_SNAP_CHUNK_RAW: usize = 256 * 1024 * 1024;
                if let Some(exact_len) = zstd_frame_content_size(c.data) {
                    if exact_len > MAX_SNAP_CHUNK_RAW {
                        return Err(());
                    }
                    if decompressor.is_none() {
                        *decompressor = zstd::bulk::Decompressor::new().ok();
                    }
                    if let Some(dec) = decompressor.as_mut() {
                        decomp_buf.clear();
                        decomp_buf.reserve(exact_len);
                        if dec.decompress_to_buffer(c.data, decomp_buf).is_ok()
                            && decomp_buf.len() == exact_len
                        {
                            let rows = decode_snapshot_payload_cached(decomp_buf, shape_cache)?;
                            return Ok((c.name, rows));
                        }
                    }
                }
                use std::io::Read;
                let decoder = zstd::Decoder::new(c.data).map_err(|_| ())?;
                let mut limited = decoder.take((MAX_SNAP_CHUNK_RAW as u64) + 1);
                decomp_buf.clear();
                let est = (c.data.len() * 3).clamp(16 * 1024, 2 * 1024 * 1024);
                if decomp_buf.capacity() < est {
                    decomp_buf.reserve(est - decomp_buf.capacity());
                }
                limited.read_to_end(decomp_buf).map_err(|_| ())?;
                if decomp_buf.len() > MAX_SNAP_CHUNK_RAW {
                    return Err(());
                }
                let rows = decode_snapshot_payload_cached(decomp_buf, shape_cache)?;
                return Ok((c.name, rows));
            }
            #[cfg(not(feature = "compress-zstd"))]
            {
                let _ = shape_cache;
                return Err(());
            }
        }
        let rows = decode_snapshot_payload_cached(c.data, shape_cache)?;
        Ok((c.name, rows))
    })
}

#[allow(dead_code)]
pub(super) fn decode_chunked_snapshot(
    data: &[u8],
    full_bytes: Option<&[u8]>,
) -> Option<Snapshot> {
    decode_chunked_snapshot_checked(data, full_bytes).ok()
}

fn decode_chunked_snapshot_checked(
    data: &[u8],
    full_bytes: Option<&[u8]>,
) -> Result<Snapshot, SnapshotDecodeFail> {
    if data.len() < 8 {
        return Err(SnapshotDecodeFail::Corrupt);
    }
    let header_len = u32::from_le_bytes(
        data[0..4]
            .try_into()
            .map_err(|_| SnapshotDecodeFail::Corrupt)?,
    ) as usize;
    if header_len > MAX_SNAPSHOT_SECTION_BYTES {
        return Err(SnapshotDecodeFail::Limit(Error::runtime(format!(
            "snapshot header size {header_len} exceeds MAX_SNAPSHOT_SECTION_BYTES ({MAX_SNAPSHOT_SECTION_BYTES})"
        ))));
    }
    if data.len() < 4 + header_len + 4 {
        return Err(SnapshotDecodeFail::Corrupt);
    }
    let header_bytes = &data[4..4 + header_len];
    validate_msgpack_header_limits(header_bytes).map_err(SnapshotDecodeFail::Limit)?;
    let header: SnapshotHeader =
        rmp_serde::from_slice(header_bytes).map_err(|_| SnapshotDecodeFail::Corrupt)?;
    if header.edges.len() > MAX_SNAPSHOT_EDGES {
        return Err(SnapshotDecodeFail::Limit(Error::runtime(format!(
            "snapshot edges count {} exceeds MAX_SNAPSHOT_EDGES ({MAX_SNAPSHOT_EDGES})",
            header.edges.len()
        ))));
    }
    if header.extra_collections.len() > MAX_SNAPSHOT_COLLECTIONS {
        return Err(SnapshotDecodeFail::Limit(Error::runtime(format!(
            "snapshot extra_collections count {} exceeds MAX_SNAPSHOT_COLLECTIONS ({MAX_SNAPSHOT_COLLECTIONS})",
            header.extra_collections.len()
        ))));
    }
    if header.cold_collections.len() > MAX_SNAPSHOT_COLLECTIONS {
        return Err(SnapshotDecodeFail::Limit(Error::runtime(format!(
            "snapshot cold_collections count {} exceeds MAX_SNAPSHOT_COLLECTIONS ({MAX_SNAPSHOT_COLLECTIONS})",
            header.cold_collections.len()
        ))));
    }

    let mut offset = 4 + header_len;
    let chunk_count = u32::from_le_bytes(
        data[offset..offset + 4]
            .try_into()
            .map_err(|_| SnapshotDecodeFail::Corrupt)?,
    ) as usize;
    offset += 4;
    if chunk_count > MAX_SNAPSHOT_COLLECTIONS.saturating_mul(1024) {
        return Err(SnapshotDecodeFail::Limit(Error::runtime(format!(
            "snapshot chunk count {chunk_count} exceeds maximum section count"
        ))));
    }

    let mut chunk_slices = Vec::with_capacity(chunk_count.min(4096));
    let mut distinct_names: rustc_hash::FxHashSet<&str> = rustc_hash::FxHashSet::default();
    for k in header.extra_collections.keys() {
        distinct_names.insert(k.as_str());
    }
    for k in &header.cold_collections {
        distinct_names.insert(k.as_str());
    }
    if distinct_names.len() > MAX_SNAPSHOT_COLLECTIONS {
        return Err(SnapshotDecodeFail::Limit(Error::runtime(format!(
            "snapshot collections count {} exceeds MAX_SNAPSHOT_COLLECTIONS ({MAX_SNAPSHOT_COLLECTIONS})",
            distinct_names.len()
        ))));
    }

    for _ in 0..chunk_count {
        if data.len() < offset + 2 {
            return Err(SnapshotDecodeFail::Corrupt);
        }
        let name_len = u16::from_le_bytes(
            data[offset..offset + 2]
                .try_into()
                .map_err(|_| SnapshotDecodeFail::Corrupt)?,
        ) as usize;
        offset += 2;
        if data.len() < offset + name_len + 4 {
            return Err(SnapshotDecodeFail::Corrupt);
        }
        let name = std::str::from_utf8(&data[offset..offset + name_len])
            .map_err(|_| SnapshotDecodeFail::Corrupt)?;
        distinct_names.insert(name);
        if distinct_names.len() > MAX_SNAPSHOT_COLLECTIONS {
            return Err(SnapshotDecodeFail::Limit(Error::runtime(format!(
                "snapshot collections count {} exceeds MAX_SNAPSHOT_COLLECTIONS ({MAX_SNAPSHOT_COLLECTIONS})",
                distinct_names.len()
            ))));
        }
        offset += name_len;
        let data_len = u32::from_le_bytes(
            data[offset..offset + 4]
                .try_into()
                .map_err(|_| SnapshotDecodeFail::Corrupt)?,
        ) as usize;
        offset += 4;
        if data_len > MAX_SNAPSHOT_SECTION_BYTES {
            return Err(SnapshotDecodeFail::Limit(Error::runtime(format!(
                "snapshot chunk section size {data_len} exceeds MAX_SNAPSHOT_SECTION_BYTES ({MAX_SNAPSHOT_SECTION_BYTES})"
            ))));
        }
        if data.len() < offset + data_len {
            return Err(SnapshotDecodeFail::Corrupt);
        }
        let chunk_data = &data[offset..offset + data_len];
        #[cfg(feature = "compress-zstd")]
        if chunk_data.len() >= 4
            && chunk_data[0..4] == ZSTD_MAGIC
            && let Some(exact_len) = zstd_frame_content_size(chunk_data)
            && exact_len > MAX_SNAPSHOT_SECTION_BYTES
        {
            return Err(SnapshotDecodeFail::Limit(Error::runtime(format!(
                "snapshot chunk decompressed size {exact_len} exceeds MAX_SNAPSHOT_SECTION_BYTES ({MAX_SNAPSHOT_SECTION_BYTES})"
            ))));
        }
        offset += data_len;
        chunk_slices.push(ChunkSlice {
            name,
            data: chunk_data,
        });
    }

    if let Some(full) = full_bytes {
        match data.len() - offset {
            0 => {}
            8 if full[5 + offset..5 + offset + 4] == SNAPSHOT_CRC_MAGIC => {
                let crc_off = 5 + offset + 4;
                let stored = u32::from_le_bytes(
                    full[crc_off..crc_off + 4]
                        .try_into()
                        .map_err(|_| SnapshotDecodeFail::Corrupt)?,
                );
                if crate::crc::hardware_crc32c(&full[..crc_off]) != stored {
                    return Err(SnapshotDecodeFail::Corrupt);
                }
            }
            _ => return Err(SnapshotDecodeFail::Corrupt),
        }
    } else if offset != data.len() {
        return Err(SnapshotDecodeFail::Corrupt);
    }

    let decoded_chunks: Vec<Result<(&str, Vec<Row>), ()>> = if chunk_slices.len() > 1 {
        #[cfg(feature = "parallel")]
        {
            use rayon::prelude::*;
            chunk_slices.par_iter().map(decode_snapshot_chunk).collect()
        }
        #[cfg(not(feature = "parallel"))]
        {
            let num_workers = std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(4)
                .min(chunk_slices.len());
            if num_workers > 1 {
                let chunk_size = chunk_slices.len().div_ceil(num_workers);
                let chunk_blocks: Vec<&[ChunkSlice]> = chunk_slices.chunks(chunk_size).collect();
                std::thread::scope(|s| {
                    let mut handles = Vec::with_capacity(chunk_blocks.len());
                    for block in chunk_blocks {
                        handles.push(s.spawn(move || {
                            let mut res = Vec::with_capacity(block.len());
                            for c in block {
                                res.push(decode_snapshot_chunk(c));
                            }
                            res
                        }));
                    }
                    let mut all = Vec::with_capacity(chunk_slices.len());
                    for h in handles {
                        if let Ok(block_res) = h.join() {
                            all.extend(block_res);
                        } else {
                            all.push(Err(()));
                        }
                    }
                    all
                })
            } else {
                chunk_slices.iter().map(decode_snapshot_chunk).collect()
            }
        }
    } else {
        chunk_slices.iter().map(decode_snapshot_chunk).collect()
    };

    let mut ok_chunks: Vec<(&str, Vec<Row>)> = Vec::with_capacity(decoded_chunks.len());
    for res in decoded_chunks {
        ok_chunks.push(res.map_err(|_| SnapshotDecodeFail::Corrupt)?);
    }

    let mut collections: std::collections::BTreeMap<String, Vec<Row>> =
        std::collections::BTreeMap::new();
    let mut idx = 0usize;
    while idx < ok_chunks.len() {
        let name = ok_chunks[idx].0;
        let mut end = idx + 1;
        let mut run_rows = ok_chunks[idx].1.len();
        while end < ok_chunks.len() && ok_chunks[end].0 == name {
            run_rows += ok_chunks[end].1.len();
            end += 1;
        }
        if let Some(existing) = collections.get_mut(name) {
            existing.reserve(run_rows);
            for (_, part) in &mut ok_chunks[idx..end] {
                existing.append(part);
            }
        } else if end == idx + 1 {
            let single = std::mem::take(&mut ok_chunks[idx].1);
            collections.insert(name.to_string(), single);
        } else {
            let mut vec = Vec::with_capacity(run_rows);
            for (_, part) in &mut ok_chunks[idx..end] {
                vec.append(part);
            }
            collections.insert(name.to_string(), vec);
        }
        idx = end;
    }

    if collections.len() > MAX_SNAPSHOT_COLLECTIONS {
        return Err(SnapshotDecodeFail::Limit(Error::runtime(format!(
            "snapshot collections count {} exceeds MAX_SNAPSHOT_COLLECTIONS ({MAX_SNAPSHOT_COLLECTIONS})",
            collections.len()
        ))));
    }

    Ok(Snapshot {
        r#gen: header.r#gen,
        embed_id: header.embed_id,
        next_id: header.next_id,
        catalog_hash: header.catalog_hash,
        log_offset: header.log_offset,
        collections,
        edges: header.edges,
        extra_collections: header.extra_collections,
        extra_rels: header.extra_rels,
        extra_indexes: header.extra_indexes,
        extra_filters: header.extra_filters,
        extra_owned: header.extra_owned,
        cold_collections: header.cold_collections,
    })
}

