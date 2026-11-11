//! Snapshot and portable backup serialization, parallel chunked encoding, and mmap decoding.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;

use crate::error::Error;
use crate::store::Row;

use super::codec::{cols_to_rows, decode_column_record, encode_insert_rows_direct};
use super::types::{
    LOG_MAGIC_SPARSE, Pack, SNAPSHOT_MAGIC, Snapshot, SnapshotHeader, SnapshotHeaderRef,
    SnapshotRef,
};
use super::wal::{durable_sync, io_err, snapshot_path, sync_dir};

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
        #[cfg(unix)]
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

/// Encode a snapshot ref to a writer using the chunked parallel snapshot format (`0x03` + `LIN\x04`).
pub fn encode_snapshot_ref_to_writer<W: std::io::Write>(
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
        let mut col_buf = Vec::new();
        let raw = if let Some((magic, _)) = encode_insert_rows_direct(0, 0, "", rows, &mut col_buf)
        {
            let mut framed = Vec::with_capacity(8 + col_buf.len());
            framed.extend_from_slice(b"LCOL");
            framed.extend_from_slice(&magic);
            framed.extend_from_slice(&col_buf);
            framed
        } else {
            rmp_serde::encode::to_vec_named(rows).map_err(io_err)?
        };
        #[cfg(feature = "compress-zstd")]
        {
            zstd::encode_all(&raw[..], 1).map_err(io_err)
        }
        #[cfg(not(feature = "compress-zstd"))]
        {
            Ok(raw)
        }
    };

    let encoded_chunks: Vec<Result<Vec<u8>, Error>> = if chunk_defs.len() > 1 {
        #[cfg(feature = "parallel")]
        {
            use rayon::prelude::*;
            chunk_defs.par_iter().map(|c| encode_chunk(c.rows)).collect()
        }
        #[cfg(not(feature = "parallel"))]
        {
            std::thread::scope(|s| {
                let mut handles = Vec::with_capacity(chunk_defs.len());
                for c in &chunk_defs {
                    handles.push(s.spawn(move || encode_chunk(c.rows)));
                }
                handles.into_iter().map(|h| h.join().unwrap()).collect()
            })
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
        decode_snapshot_bytes(&mmap).ok_or_else(|| io_err("corrupt backup"))
    } else {
        use std::io::Read;
        let mut bytes = Vec::with_capacity(len as usize);
        let mut reader = std::io::BufReader::new(file);
        reader.read_to_end(&mut bytes).map_err(io_err)?;
        decode_snapshot_bytes(&bytes).ok_or_else(|| io_err("corrupt backup"))
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

pub(super) fn decode_snapshot_bytes(bytes: &[u8]) -> Option<Snapshot> {
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
                    let mut limited = decoder.take(256 * 1024 * 1024 + 1);
                    limited.read_to_end(&mut result).ok()?;
                    if result.len() > 256 * 1024 * 1024 {
                        return None;
                    }
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
                    let decompressed =
                        lz4::block::decompress(compressed_data, Some(256 * 1024 * 1024)).ok()?;
                    return rmp_serde::from_slice(&decompressed).ok();
                }
                #[cfg(not(feature = "compress-lz4"))]
                {
                    let _ = compressed_data;
                    return None;
                }
            }
            0x03 => return decode_chunked_snapshot(compressed_data),
            0x04 => {
                #[cfg(feature = "compress-zstd")]
                {
                    use std::io::Read;
                    let decoder = zstd::Decoder::new(compressed_data).ok()?;
                    let mut limited = decoder.take(256 * 1024 * 1024 + 1);
                    let mut result = Vec::new();
                    limited.read_to_end(&mut result).ok()?;
                    if result.len() > 256 * 1024 * 1024 {
                        return None;
                    }
                    return rmp_serde::from_slice(&result).ok();
                }
                #[cfg(not(feature = "compress-zstd"))]
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

pub(super) struct ChunkSlice<'a> {
    name: &'a str,
    data: &'a [u8],
}

pub(super) const ZSTD_MAGIC: [u8; 4] = [0x28, 0xb5, 0x2f, 0xfd];

pub(super) fn decode_snapshot_payload(payload: &[u8]) -> Result<Vec<Row>, ()> {
    if payload.len() >= 8 && &payload[0..4] == b"LCOL" {
        let allow_sparse = payload[4..8] == LOG_MAGIC_SPARSE;
        let rec = decode_column_record(&payload[8..], allow_sparse)?;
        if let Pack::InsertCols {
            fields, cols, n, ..
        } = rec.pack
        {
            return Ok(cols_to_rows(&fields, &cols, n as usize));
        }
        return Err(());
    }
    rmp_serde::from_slice(payload).map_err(|_| ())
}

pub(super) fn decode_snapshot_chunk<'a>(c: &ChunkSlice<'a>) -> Result<(&'a str, Vec<Row>), ()> {
    if c.data.is_empty() {
        return Ok((c.name, Vec::new()));
    }
    if c.data.len() >= 4 && c.data[0..4] == ZSTD_MAGIC {
        #[cfg(feature = "compress-zstd")]
        {
            use std::io::Read;
            let decoder = zstd::Decoder::new(c.data).map_err(|_| ())?;
            let mut limited = decoder.take(256 * 1024 * 1024 + 1);
            let mut decompressed = Vec::with_capacity((c.data.len() * 3).clamp(16 * 1024, 2 * 1024 * 1024));
            limited.read_to_end(&mut decompressed).map_err(|_| ())?;
            if decompressed.len() > 256 * 1024 * 1024 {
                return Err(());
            }
            let rows = decode_snapshot_payload(&decompressed)?;
            return Ok((c.name, rows));
        }
        #[cfg(not(feature = "compress-zstd"))]
        {
            return Err(());
        }
    }
    let rows = decode_snapshot_payload(c.data)?;
    Ok((c.name, rows))
}

pub(super) fn decode_chunked_snapshot(data: &[u8]) -> Option<Snapshot> {
    if data.len() < 8 {
        return None;
    }
    let header_len = u32::from_le_bytes(data[0..4].try_into().ok()?) as usize;
    if data.len() < 4 + header_len + 4 {
        return None;
    }
    let header_bytes = &data[4..4 + header_len];
    let header: SnapshotHeader = rmp_serde::from_slice(header_bytes).ok()?;

    let mut offset = 4 + header_len;
    let chunk_count = u32::from_le_bytes(data[offset..offset + 4].try_into().ok()?) as usize;
    offset += 4;

    let mut chunk_slices = Vec::with_capacity(chunk_count);
    for _ in 0..chunk_count {
        if data.len() < offset + 2 {
            return None;
        }
        let name_len = u16::from_le_bytes(data[offset..offset + 2].try_into().ok()?) as usize;
        offset += 2;
        if data.len() < offset + name_len + 4 {
            return None;
        }
        let name = std::str::from_utf8(&data[offset..offset + name_len]).ok()?;
        offset += name_len;
        let data_len = u32::from_le_bytes(data[offset..offset + 4].try_into().ok()?) as usize;
        offset += 4;
        if data.len() < offset + data_len {
            return None;
        }
        let chunk_data = &data[offset..offset + data_len];
        offset += data_len;
        chunk_slices.push(ChunkSlice {
            name,
            data: chunk_data,
        });
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
                let chunk_size = (chunk_slices.len() + num_workers - 1) / num_workers;
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

    let mut collections: std::collections::BTreeMap<String, Vec<Row>> =
        std::collections::BTreeMap::new();
    for res in decoded_chunks {
        let (name, rows) = res.ok()?;
        if let Some(vec) = collections.get_mut(name) {
            vec.extend(rows);
        } else {
            let mut vec = Vec::with_capacity(rows.len());
            vec.extend(rows);
            collections.insert(name.to_string(), vec);
        }
    }

    Some(Snapshot {
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

