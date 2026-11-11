//! Columnar, sparse-vector, and dictionary WAL codecs plus background encoder worker pool.

use std::cell::RefCell;
use std::fs::File;
use std::io;
use std::sync::Arc;

use crate::Cell;
use crate::error::Error;
use crate::store::{Edge, Row};

use super::types::{
    ColData, LOG_MAGIC_SPARSE, LOG_MAGIC_V2, LogRecord, MAX_SPARSE_DECODED_BYTES, Pack, Persist,
    SPARSE_VECTOR_THRESHOLD, SyncMode, V2_FACTS_BULK, V2_INSERT_COLS,
};
use super::wal::{
    append_record, append_record_v1, compute_frame_checksum, io_err, write_frame,
    write_frame_precomputed,
};

#[inline(always)]
pub(super) fn put_str(buf: &mut Vec<u8>, s: &str) {
    let b = s.as_bytes();
    buf.extend_from_slice(&(b.len() as u32).to_le_bytes());
    buf.extend_from_slice(b);
}

pub(super) fn append_facts_v2(
    log: &mut File,
    rec_gen: u64,
    next_id: u64,
    s: &[std::sync::Arc<str>],
    p: &[std::sync::Arc<str>],
    o: &[std::sync::Arc<str>],
    buf: &mut Vec<u8>,
    sync: SyncMode,
    log_bytes: &mut u64,
) -> Result<(), Error> {
    let n = s.len().min(p.len()).min(o.len());
    buf.clear();
    buf.reserve(25 + n * 48);
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

#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
pub(super) unsafe fn count_non_zero8(values: &[f32]) -> usize {
    use std::arch::aarch64::*;
    unsafe {
        let a = vreinterpretq_u32_f32(vld1q_f32(values.as_ptr()));
        let b = vreinterpretq_u32_f32(vld1q_f32(values.as_ptr().add(4)));
        let zero = vdupq_n_u32(0);
        let a_nz = vshrq_n_u32(vmvnq_u32(vceqq_u32(a, zero)), 31);
        let b_nz = vshrq_n_u32(vmvnq_u32(vceqq_u32(b, zero)), 31);
        vaddvq_u32(vaddq_u32(a_nz, b_nz)) as usize
    }
}

#[inline]
pub(super) fn count_non_zero_floats(values: &[f32]) -> usize {
    #[cfg(target_arch = "aarch64")]
    if std::arch::is_aarch64_feature_detected!("neon") {
        let mut count = 0usize;
        let chunks = values.chunks_exact(8);
        let remainder = chunks.remainder();
        for chunk in chunks {
            count += unsafe { count_non_zero8(chunk) };
        }
        for &x in remainder {
            if x.to_bits() != 0 {
                count += 1;
            }
        }
        return count;
    }
    values.iter().filter(|x| x.to_bits() != 0).count()
}

pub(super) fn use_sparse_vectors(rows: &[Option<std::sync::Arc<[f32]>>]) -> bool {
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
    let mut sparse_so_far = 0usize;
    let mut dense_so_far = 0usize;
    let n = rows.len();
    for (i, row) in rows.iter().enumerate() {
        let (d, s) = match row.as_ref() {
            None => (4, 4),
            Some(v) => {
                let d = 4usize.saturating_add(v.len().saturating_mul(4));
                let s = 8usize.saturating_add(count_non_zero_floats(v).saturating_mul(8));
                (d, s)
            }
        };
        sparse_so_far = sparse_so_far.saturating_add(s);
        dense_so_far = dense_so_far.saturating_add(d);
        if sparse_so_far > dense_so_far {
            let deficit = sparse_so_far - dense_so_far;
            let remaining_rows = n - 1 - i;
            let dense_remaining = dense.saturating_sub(dense_so_far);
            let max_future_savings = dense_remaining.saturating_sub(remaining_rows.saturating_mul(4));
            if remaining_rows > 0 && deficit >= max_future_savings {
                return false;
            }
        }
    }
    sparse_so_far < dense
}

pub(super) fn sparse_bytes_serial(rows: &[Option<std::sync::Arc<[f32]>>]) -> usize {
    rows.iter()
        .map(|row| {
            row.as_ref().map_or(4, |v| {
                8usize.saturating_add(count_non_zero_floats(v).saturating_mul(8))
            })
        })
        .fold(0usize, usize::saturating_add)
}

pub(super) fn sparse_count_worker(
    rows: &[Option<std::sync::Arc<[f32]>>],
    worker: io::Result<std::thread::ScopedJoinHandle<'_, usize>>,
) -> usize {
    match worker {
        Ok(worker) => worker.join().expect("sparse size worker panicked"),
        Err(_) => sparse_bytes_serial(rows),
    }
}

pub(super) fn sparse_bytes_parallel(rows: &[Option<std::sync::Arc<[f32]>>]) -> usize {
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

pub(super) fn append_sparse_iter_serial<'a>(
    iter: impl Iterator<Item = Option<&'a [f32]>>,
    buf: &mut Vec<u8>,
) -> Result<(), Error> {
    #[cfg(target_arch = "aarch64")]
    if std::arch::is_aarch64_feature_detected!("neon") {
        // Runtime feature check covers all NEON loads; helper only loads full eight-value chunks.
        return unsafe { append_sparse_iter_neon(iter, buf) };
    }
    append_sparse_iter_scalar(iter, buf)
}

pub(super) fn append_sparse_rows_serial(
    rows: &[Option<std::sync::Arc<[f32]>>],
    buf: &mut Vec<u8>,
) -> Result<(), Error> {
    append_sparse_iter_serial(rows.iter().map(|r| r.as_deref()), buf)
}

pub(super) fn append_sparse_iter_scalar<'a>(
    iter: impl Iterator<Item = Option<&'a [f32]>>,
    buf: &mut Vec<u8>,
) -> Result<(), Error> {
    for row in iter {
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

#[cfg(test)]
pub(super) fn append_sparse_rows_scalar(
    rows: &[Option<std::sync::Arc<[f32]>>],
    buf: &mut Vec<u8>,
) -> Result<(), Error> {
    append_sparse_iter_scalar(rows.iter().map(|r| r.as_deref()), buf)
}

#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
pub(super) unsafe fn presence_mask8(values: &[f32]) -> u32 {
    assert_eq!(values.len(), 8);
    use std::arch::aarch64::*;
    unsafe {
        let a = vreinterpretq_u32_f32(vld1q_f32(values.as_ptr()));
        let b = vreinterpretq_u32_f32(vld1q_f32(values.as_ptr().add(4)));
        if vmaxvq_u32(vorrq_u32(a, b)) == 0 {
            return 0;
        }
        let weights = vld1q_u32([1u32, 2, 4, 8].as_ptr());
        let zero = vdupq_n_u32(0);
        let lo = vaddvq_u32(vandq_u32(vmvnq_u32(vceqq_u32(a, zero)), weights));
        let hi = vaddvq_u32(vandq_u32(vmvnq_u32(vceqq_u32(b, zero)), weights));
        lo | (hi << 4)
    }
}
#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
pub(super) unsafe fn append_sparse_iter_neon<'a>(
    iter: impl Iterator<Item = Option<&'a [f32]>>,
    buf: &mut Vec<u8>,
) -> Result<(), Error> {
    for row in iter {
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
            let mut mask = if values.len() == 8 {
                unsafe { presence_mask8(values) }
            } else {
                values
                    .iter()
                    .enumerate()
                    .fold(0u32, |m, (i, x)| m | (u32::from(x.to_bits() != 0) << i))
            };
            while mask != 0 {
                let offset = mask.trailing_zeros() as usize;
                let index = (block * 8 + offset) as u32;
                let mut entry = [0u8; 8];
                entry[..4].copy_from_slice(&index.to_le_bytes());
                entry[4..].copy_from_slice(&values[offset].to_bits().to_le_bytes());
                buf.extend_from_slice(&entry);
                count += 1;
                mask &= mask - 1;
            }
        }
        buf[count_offset..count_offset + 4].copy_from_slice(&count.to_le_bytes());
    }
    Ok(())
}

pub(super) fn append_sparse_worker(
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

pub(super) fn append_sparse_rows_parallel(
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

// A shared lease avoids a worker per open database. Last lease joins the worker.
#[derive(Debug)]
pub(crate) struct EncoderWorker {
    pub(super) sender: Option<std::sync::mpsc::SyncSender<EncodeJob>>,
    pub(super) handle: Option<std::thread::JoinHandle<()>>,
}
impl Drop for EncoderWorker {
    fn drop(&mut self) {
        drop(self.sender.take());
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}
pub(crate) fn acquire_encoder() -> Option<std::sync::Arc<EncoderWorker>> {
    static SHARED: std::sync::Mutex<Option<std::sync::Arc<EncoderWorker>>> =
        std::sync::Mutex::new(None);
    let mut shared = SHARED.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(worker) = shared.as_ref() {
        return Some(std::sync::Arc::clone(worker));
    }
    let (sender, receiver) = std::sync::mpsc::sync_channel::<EncodeJob>(1);
    let handle = std::thread::Builder::new()
        .name("lin-wal-encoder".into())
        .spawn(move || {
            while let Ok(job) = receiver.recv() {
                job.run();
            }
        })
        .ok()?;
    let worker = std::sync::Arc::new(EncoderWorker {
        sender: Some(sender),
        handle: Some(handle),
    });
    *shared = Some(std::sync::Arc::clone(&worker));
    Some(worker)
}
pub(crate) struct EncodeJob {
    pub(super) generation: u64,
    pub(super) next_id: u64,
    pub(super) collection: String,
    pub(super) rows: std::sync::Arc<Vec<Row>>,
    pub(super) uniform_keys: bool,
    pub(super) bytes: Vec<u8>,
    #[allow(dead_code)]
    pub(super) mode: u8,
    pub(super) reply: std::sync::mpsc::SyncSender<PreparedInsert>,
}
impl EncodeJob {
    pub(crate) fn encode(self) -> PreparedInsert {
        let Self {
            generation,
            next_id,
            collection,
            rows,
            uniform_keys,
            bytes,
            #[cfg(test)]
            mode,
            #[cfg(not(test))]
                mode: _,
            reply: _,
        } = self;
        let staged = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            #[cfg(test)]
            if mode == 2 {
                panic!("injected WAL encoder panic");
            }
            #[allow(unused_mut)]
            let mut staged = prepare_insert_columns_ext(
                generation,
                next_id,
                &collection,
                &rows,
                uniform_keys,
                bytes,
            );
            #[cfg(test)]
            if mode == 3 {
                staged.encoding =
                    PreparedEncoding::Failed(Error::runtime("injected WAL encoder failure"));
            }
            staged
        }));
        // Drop the worker input before reply; the caller recovers its Vec without copying.
        drop(rows);
        staged.unwrap_or_else(|payload| PreparedInsert {
            pack: None,
            encoding: PreparedEncoding::Panicked(payload),
        })
    }
    pub(super) fn run(self) {
        let reply = self.reply.clone();
        let staged = self.encode();
        let _ = reply.send(staged);
    }
}
pub(crate) fn start_insert_encoding(
    persist: &mut Persist,
    generation: u64,
    next_id: u64,
    collection: &str,
    rows: std::sync::Arc<Vec<Row>>,
    uniform_keys: bool,
    bytes: Vec<u8>,
    mode: u8,
) -> Result<std::sync::mpsc::Receiver<PreparedInsert>, EncodeJob> {
    let (reply, receiver) = std::sync::mpsc::sync_channel(1);
    let job = EncodeJob {
        generation,
        next_id,
        collection: collection.to_owned(),
        rows,
        uniform_keys,
        bytes,
        mode,
        reply,
    };
    #[cfg(test)]
    {
        if mode == 1 || mode == 5 {
            return Err(job);
        }
        if mode == 4 {
            drop(job);
            return Ok(receiver);
        }
    }
    if persist.encoder.is_none() {
        persist.encoder = acquire_encoder();
    }
    let Some(worker) = persist.encoder.as_ref() else {
        return Err(job);
    };
    let Some(sender) = worker.sender.as_ref() else {
        return Err(job);
    };
    match sender.try_send(job) {
        Ok(()) => Ok(receiver),
        Err(
            std::sync::mpsc::TrySendError::Full(job)
            | std::sync::mpsc::TrySendError::Disconnected(job),
        ) => Err(job),
    }
}

pub(crate) struct EncodedColumns {
    pub(super) generation: u64,
    pub(super) next_id: u64,
    pub(super) collection: String,
    pub(super) fields: Vec<String>,
    pub(super) rows: u32,
    pub(super) magic: [u8; 4],
    pub(super) checksum: [u8; 4],
    pub(super) bytes: Vec<u8>,
}

pub(crate) enum PreparedEncoding {
    Ready(EncodedColumns),
    Direct {
        generation: u64,
        next_id: u64,
        collection: String,
        fields: Vec<String>,
        rows: u32,
        magic: [u8; 4],
    },
    Failed(Error),
    Panicked(Box<dyn std::any::Any + Send>),
}

pub(crate) struct PreparedInsert {
    pub(crate) pack: Option<Pack>,
    pub(crate) encoding: PreparedEncoding,
}

#[cfg(test)]
pub(crate) fn prepare_insert_columns(
    generation: u64,
    next_id: u64,
    collection: &str,
    rows: &[Row],
    bytes: Vec<u8>,
) -> PreparedInsert {
    prepare_insert_columns_ext(generation, next_id, collection, rows, false, bytes)
}

pub(crate) fn prepare_insert_columns_ext(
    generation: u64,
    next_id: u64,
    collection: &str,
    rows: &[Row],
    uniform_keys: bool,
    mut bytes: Vec<u8>,
) -> PreparedInsert {
    if rows.len() < 4096
        && let Some((magic, fields)) = encode_insert_rows_direct_ext(
            generation,
            next_id,
            collection,
            rows,
            uniform_keys,
            &mut bytes,
        )
    {
        let n = rows.len() as u32;
        let checksum = compute_frame_checksum(&magic, &bytes);
        let pack = Pack::InsertCols {
            collection: collection.to_owned(),
            fields: fields.clone(),
            cols: Vec::new(),
            n,
            edges: Vec::new(),
        };
        return PreparedInsert {
            pack: Some(pack),
            encoding: PreparedEncoding::Ready(EncodedColumns {
                generation,
                next_id,
                collection: collection.to_owned(),
                fields,
                rows: n,
                magic,
                checksum,
                bytes,
            }),
        };
    }
    let mut pack = rows_to_insert_cols(collection, rows, Vec::new());
    let encoding = match &mut pack {
        Pack::InsertCols {
            fields, cols, n, ..
        } => {
            match encode_insert_cols_v2(
                generation, next_id, collection, fields, cols, *n, &mut bytes,
            ) {
                Ok(magic) => {
                    cols.clear();
                    let checksum = compute_frame_checksum(&magic, &bytes);
                    PreparedEncoding::Ready(EncodedColumns {
                        generation,
                        next_id,
                        collection: collection.to_owned(),
                        fields: fields.clone(),
                        rows: *n,
                        magic,
                        checksum,
                        bytes,
                    })
                }
                Err(error) => PreparedEncoding::Failed(error),
            }
        }
        _ => unreachable!("column pack builder"),
    };
    PreparedInsert {
        pack: Some(pack),
        encoding,
    }
}

pub(crate) fn append_prepared_record(
    log: &mut File,
    rec: &LogRecord,
    prepared: EncodedColumns,
    buf: &mut Vec<u8>,
    sync: SyncMode,
    log_bytes: &mut u64,
) -> Result<(), Error> {
    let valid = rec.r#gen == prepared.generation
        && rec.next_id == prepared.next_id
        && matches!(&rec.pack, Pack::InsertCols { collection, fields, cols, n, edges }
            if collection == &prepared.collection && fields == &prepared.fields
                && (cols.is_empty() || fields.len() == cols.len())
                && *n == prepared.rows && edges.is_empty());
    *buf = prepared.bytes;
    if valid {
        write_frame_precomputed(
            log,
            &prepared.magic,
            buf,
            prepared.checksum,
            sync,
            log_bytes,
        )
    } else {
        append_record(log, rec, buf, sync, log_bytes)
    }
}

pub(super) fn append_insert_cols_v2(
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
    let magic = encode_insert_cols_v2(rec_gen, next_id, collection, fields, cols, n, buf)?;
    write_frame(log, &magic, buf, sync, log_bytes)
}

/// Encode the existing typed-column payload without touching a file.
pub(super) fn encode_insert_cols_v2(
    rec_gen: u64,
    next_id: u64,
    collection: &str,
    fields: &[String],
    cols: &[ColData],
    n: u32,
    buf: &mut Vec<u8>,
) -> Result<[u8; 4], Error> {
    if fields.len() != cols.len() {
        return Err(io_err("typed column field count mismatch"));
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
            ColData::Vec(v) => {
                let mut dense = 0usize;
                let mut has_empty = false;
                for row in v {
                    match row.as_ref() {
                        None => dense = dense.saturating_add(4),
                        Some(emb) => {
                            if emb.is_empty() {
                                has_empty = true;
                            }
                            dense = dense.saturating_add(
                                4usize.saturating_add(emb.len().saturating_mul(4)),
                            );
                        }
                    }
                }
                let try_sparse = has_empty
                    || (dense >= SPARSE_VECTOR_THRESHOLD
                        && if v.len() >= 4096 {
                            use_sparse_vectors(v)
                        } else if let Some(Some(first)) = v.first()
                            && 8usize.saturating_add(count_non_zero_floats(first).saturating_mul(8))
                                > 4usize.saturating_add(first.len().saturating_mul(4))
                        {
                            use_sparse_vectors(v)
                        } else {
                            true
                        });
                let mut wrote_sparse = false;
                if try_sparse {
                    if v.len() >= 4096 {
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
                        if v.iter()
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
                                    .ok_or_else(|| {
                                        io_err("sparse vectors exceed decoded 64MiB budget")
                                    })?;
                            }
                            append_sparse_rows_parallel(v, buf)?;
                        } else {
                            for emb in v.iter().flatten() {
                                let bytes = emb
                                    .len()
                                    .checked_mul(4)
                                    .ok_or_else(|| io_err("sparse vector size overflow"))?;
                                sparse_decoded_bytes = sparse_decoded_bytes
                                    .checked_add(bytes)
                                    .filter(|&bytes| bytes <= MAX_SPARSE_DECODED_BYTES)
                                    .ok_or_else(|| {
                                        io_err("sparse vectors exceed decoded 64MiB budget")
                                    })?;
                            }
                            append_sparse_rows_serial(v, buf)?;
                        }
                        if sparse_decoded_bytes > MAX_SPARSE_DECODED_BYTES {
                            return Err(io_err("sparse vectors exceed decoded 64MiB budget"));
                        }
                        wrote_sparse = true;
                    } else {
                        let mark = buf.len();
                        buf.push(7);
                        let payload_start = buf.len();
                        if let Err(e) = append_sparse_rows_serial(v, buf) {
                            buf.truncate(mark);
                            return Err(e);
                        }
                        let sparse_bytes = buf.len() - payload_start;
                        if !has_empty && sparse_bytes >= dense {
                            buf.truncate(mark);
                        } else {
                            if v.len() != n as usize {
                                buf.truncate(mark);
                                return Err(io_err("sparse vector column length mismatch"));
                            }
                            let storage = v
                                .len()
                                .checked_mul(std::mem::size_of::<Option<Vec<f32>>>())
                                .ok_or_else(|| io_err("sparse vector size overflow"))?;
                            let mut next_decoded = sparse_decoded_bytes
                                .checked_add(storage)
                                .ok_or_else(|| io_err("sparse vector size overflow"))?;
                            for emb in v.iter().flatten() {
                                let bytes = emb
                                    .len()
                                    .checked_mul(4)
                                    .ok_or_else(|| io_err("sparse vector size overflow"))?;
                                next_decoded = next_decoded
                                    .checked_add(bytes)
                                    .filter(|&bytes| bytes <= MAX_SPARSE_DECODED_BYTES)
                                    .ok_or_else(|| {
                                        io_err("sparse vectors exceed decoded 64MiB budget")
                                    })?;
                            }
                            if next_decoded > MAX_SPARSE_DECODED_BYTES {
                                return Err(io_err("sparse vectors exceed decoded 64MiB budget"));
                            }
                            sparse_decoded_bytes = next_decoded;
                            sparse_codec = true;
                            wrote_sparse = true;
                        }
                    }
                }
                if !wrote_sparse {
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
            }
            ColData::Null => buf.push(0),
        }
    }
    Ok(if sparse_codec {
        LOG_MAGIC_SPARSE
    } else {
        LOG_MAGIC_V2
    })
}

thread_local! {
    static CELL_PTR_SCRATCH: RefCell<Vec<*const Cell>> = RefCell::new(Vec::new());
}

pub(super) fn append_sparse_cells_serial(
    cells: &[*const Cell],
    num_fields: usize,
    fi: usize,
    num_rows: usize,
    buf: &mut Vec<u8>,
) -> Result<(), Error> {
    // Safety: all pointers in `cells` borrow from `rows: &[Row]` live for the duration of `encode_insert_rows_direct_ext`.
    let iter = (0..num_rows).map(|ri| match unsafe { &*cells[ri * num_fields + fi] } {
        Cell::Vec(emb) => Some(emb.as_ref()),
        _ => None,
    });
    append_sparse_iter_serial(iter, buf)
}

pub(crate) fn encode_insert_rows_direct(
    rec_gen: u64,
    next_id: u64,
    collection: &str,
    rows: &[Row],
    buf: &mut Vec<u8>,
) -> Option<([u8; 4], Vec<String>)> {
    encode_insert_rows_direct_ext(rec_gen, next_id, collection, rows, false, buf)
}

pub(crate) fn encode_insert_rows_direct_ext(
    rec_gen: u64,
    next_id: u64,
    collection: &str,
    rows: &[Row],
    uniform_keys: bool,
    buf: &mut Vec<u8>,
) -> Option<([u8; 4], Vec<String>)> {
    if rows.is_empty() {
        return None;
    }
    let first = &rows[0];
    let num_fields = first.len();
    if num_fields == 0 {
        return None;
    }

    #[derive(Copy, Clone)]
    enum DirectColType {
        Text,
        Int,
        Float,
        Bool,
        Time,
        Vec,
    }
    let mut col_types = smallvec::SmallVec::<[DirectColType; 8]>::with_capacity(num_fields);
    let mut has_vec = false;
    for val in first.values() {
        let t = match val {
            Cell::Text(_) | Cell::SmallText(_) => DirectColType::Text,
            Cell::Int(_) => DirectColType::Int,
            Cell::Float(_) => DirectColType::Float,
            Cell::Bool(_) => DirectColType::Bool,
            Cell::Time(_) => DirectColType::Time,
            Cell::Vec(_) => {
                has_vec = true;
                DirectColType::Vec
            }
            _ => return None,
        };
        col_types.push(t);
    }
    if has_vec && rows.len() >= 4096 {
        return None;
    }

    CELL_PTR_SCRATCH.with(|scratch| {
        let mut cells = scratch.borrow_mut();
        cells.clear();
        cells.reserve(rows.len() * num_fields);
        if uniform_keys {
            for r in rows {
                if r.len() != num_fields {
                    return None;
                }
                cells.extend(r.values().map(|c| c as *const Cell));
            }
        } else {
            for r in rows {
                if r.len() != num_fields || !r.keys().eq(first.keys()) {
                    return None;
                }
                cells.extend(r.values().map(|c| c as *const Cell));
            }
        }

        let fields: Vec<String> = first.keys().cloned().collect();
        let n = rows.len() as u32;

        buf.clear();
        buf.reserve(64 + fields.len() * 16 + (rows.len() * num_fields * 32));
        buf.push(V2_INSERT_COLS);
        buf.extend_from_slice(&rec_gen.to_le_bytes());
        buf.extend_from_slice(&next_id.to_le_bytes());
        put_str(buf, collection);
        buf.extend_from_slice(&n.to_le_bytes());
        buf.extend_from_slice(&(num_fields as u32).to_le_bytes());

        let mut sparse_codec = false;
        let mut sparse_decoded_bytes = 0usize;

        for (fi, f) in fields.iter().enumerate() {
            put_str(buf, f);
            match col_types[fi] {
                DirectColType::Text => {
                    buf.push(1);
                    for ri in 0..rows.len() {
                        if let Some(s) = unsafe { (*cells[ri * num_fields + fi]).text() } {
                            put_str(buf, s);
                        } else {
                            return None;
                        }
                    }
                }
                DirectColType::Int => {
                    buf.push(2);
                    for ri in 0..rows.len() {
                        match unsafe { &*cells[ri * num_fields + fi] } {
                            Cell::Int(x) => buf.extend_from_slice(&x.to_le_bytes()),
                            _ => return None,
                        }
                    }
                }
                DirectColType::Float => {
                    buf.push(3);
                    for ri in 0..rows.len() {
                        match unsafe { &*cells[ri * num_fields + fi] } {
                            Cell::Float(x) => buf.extend_from_slice(&x.to_le_bytes()),
                            _ => return None,
                        }
                    }
                }
                DirectColType::Bool => {
                    buf.push(4);
                    for ri in 0..rows.len() {
                        match unsafe { &*cells[ri * num_fields + fi] } {
                            Cell::Bool(x) => buf.push(u8::from(*x)),
                            _ => return None,
                        }
                    }
                }
                DirectColType::Time => {
                    buf.push(5);
                    for ri in 0..rows.len() {
                        match unsafe { &*cells[ri * num_fields + fi] } {
                            Cell::Time(x) => buf.extend_from_slice(&x.to_le_bytes()),
                            _ => return None,
                        }
                    }
                }
                DirectColType::Vec => {
                    let mut dense = 0usize;
                    let mut has_empty = false;
                    let mut first_emb: Option<&[f32]> = None;
                    for ri in 0..rows.len() {
                        match unsafe { &*cells[ri * num_fields + fi] } {
                            Cell::Vec(emb) => {
                                if ri == 0 {
                                    first_emb = Some(emb.as_ref());
                                }
                                if emb.is_empty() {
                                    has_empty = true;
                                }
                                dense = dense.saturating_add(
                                    4usize.saturating_add(emb.len().saturating_mul(4)),
                                );
                            }
                            _ => return None,
                        }
                    }
                    let try_sparse = if has_empty {
                        true
                    } else if dense < SPARSE_VECTOR_THRESHOLD {
                        false
                    } else if let Some(first_v) = first_emb
                        && 8usize.saturating_add(count_non_zero_floats(first_v).saturating_mul(8))
                            > 4usize.saturating_add(first_v.len().saturating_mul(4))
                    {
                        return None;
                    } else {
                        true
                    };
                    let mut wrote_sparse = false;
                    if try_sparse {
                        let mark = buf.len();
                        buf.push(7);
                        let payload_start = buf.len();
                        if append_sparse_cells_serial(&cells, num_fields, fi, rows.len(), buf)
                            .is_err()
                        {
                            buf.truncate(mark);
                            return None;
                        }
                        let sparse_bytes = buf.len() - payload_start;
                        if !has_empty && sparse_bytes >= dense {
                            buf.truncate(mark);
                        } else {
                            let storage = rows
                                .len()
                                .checked_mul(std::mem::size_of::<Option<Vec<f32>>>())?;
                            let mut next_decoded = sparse_decoded_bytes.checked_add(storage)?;
                            for ri in 0..rows.len() {
                                if let Cell::Vec(emb) = unsafe { &*cells[ri * num_fields + fi] } {
                                    let bytes = emb.len().checked_mul(4)?;
                                    next_decoded = next_decoded
                                        .checked_add(bytes)
                                        .filter(|&b| b <= MAX_SPARSE_DECODED_BYTES)?;
                                }
                            }
                            sparse_decoded_bytes = next_decoded;
                            sparse_codec = true;
                            wrote_sparse = true;
                        }
                    }
                    if !wrote_sparse {
                        buf.push(6);
                        for ri in 0..rows.len() {
                            match unsafe { &*cells[ri * num_fields + fi] } {
                                Cell::Vec(emb) if !emb.is_empty() => {
                                    buf.extend_from_slice(&(emb.len() as u32).to_le_bytes());
                                    #[cfg(target_endian = "little")]
                                    {
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
                }
            }
        }

        Some((
            if sparse_codec {
                LOG_MAGIC_SPARSE
            } else {
                LOG_MAGIC_V2
            },
            fields,
        ))
    })
}


pub(super) fn take_u32(buf: &[u8], i: &mut usize) -> Result<u32, ()> {
    if *i + 4 > buf.len() {
        return Err(());
    }
    let v = u32::from_le_bytes(buf[*i..*i + 4].try_into().unwrap());
    *i += 4;
    Ok(v)
}

pub(super) fn take_u64(buf: &[u8], i: &mut usize) -> Result<u64, ()> {
    if *i + 8 > buf.len() {
        return Err(());
    }
    let v = u64::from_le_bytes(buf[*i..*i + 8].try_into().unwrap());
    *i += 8;
    Ok(v)
}

pub(super) fn take_str_ref<'a>(buf: &'a [u8], i: &mut usize) -> Result<&'a str, ()> {
    let n = take_u32(buf, i)? as usize;
    let end = (*i)
        .checked_add(n)
        .filter(|&end| end <= buf.len())
        .ok_or(())?;
    let s = std::str::from_utf8(&buf[*i..end]).map_err(|_| ())?;
    *i = end;
    Ok(s)
}

pub(super) fn take_str(buf: &[u8], i: &mut usize) -> Result<String, ()> {
    take_str_ref(buf, i).map(str::to_owned)
}

#[inline]
pub(super) fn take_arc_str(buf: &[u8], i: &mut usize) -> Result<Arc<str>, ()> {
    let s = take_str_ref(buf, i)?;
    Ok(match s {
        "rag" | "sys" | "core" | "test" => crate::store::intern_wing(Arc::from(s)),
        "wiki" => {
            static WIKI: std::sync::LazyLock<Arc<str>> =
                std::sync::LazyLock::new(|| Arc::from("wiki"));
            Arc::clone(&WIKI)
        }
        "raw" => {
            static RAW: std::sync::LazyLock<Arc<str>> =
                std::sync::LazyLock::new(|| Arc::from("raw"));
            Arc::clone(&RAW)
        }
        _ => Arc::from(s),
    })
}

pub(super) fn decode_v2(buf: &[u8]) -> Result<LogRecord, ()> {
    decode_column_record(buf, false)
}

// Shared decoded-vector accounting deliberately retains the legacy Vec metadata cost.
pub(super) fn dictionary_budget_add(budget: &mut usize, bytes: usize) -> Result<(), ()> {
    *budget = budget
        .checked_add(bytes)
        .filter(|&n| n <= MAX_SPARSE_DECODED_BYTES)
        .ok_or(())?;
    Ok(())
}
pub(super) fn take_dictionary_entry(
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
pub(super) fn take_dictionary_vectors(
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

pub(super) fn decode_column_record(buf: &[u8], allow_sparse: bool) -> Result<LogRecord, ()> {
    decode_column_record_mode(buf, allow_sparse, false)
}
pub(super) fn decode_column_record_mode(
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
            let mut s: Vec<Arc<str>> = Vec::with_capacity(n);
            let mut p: Vec<Arc<str>> = Vec::with_capacity(n);
            let mut o: Vec<Arc<str>> = Vec::with_capacity(n);
            for _ in 0..n {
                s.push(take_arc_str(buf, &mut i)?);
                let p_str = take_str_ref(buf, &mut i)?;
                if let Some(prev) = p.last().filter(|prev| prev.as_ref() == p_str) {
                    p.push(Arc::clone(prev));
                } else {
                    p.push(Arc::from(p_str));
                }
                o.push(take_arc_str(buf, &mut i)?);
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
                        let mut prev_arc: Option<Arc<str>> = None;
                        for _ in 0..n {
                            let s_ref = take_str_ref(buf, &mut i)?;
                            let arc = if let Some(ref prev) = prev_arc
                                && prev.as_ref() == s_ref
                            {
                                Arc::clone(prev)
                            } else {
                                let a = match s_ref {
                                    "rag" | "sys" | "core" | "test" => {
                                        crate::store::intern_wing(Arc::from(s_ref))
                                    }
                                    _ => Arc::from(s_ref),
                                };
                                if s_ref.len() <= 32 {
                                    prev_arc = Some(Arc::clone(&a));
                                }
                                a
                            };
                            v.push(arc);
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
    let first_row = &rows[0];
    let uniform = rows[1..]
        .iter()
        .all(|r| r.len() == first_row.len() && r.keys().eq(first_row.keys()));
    let fields: Vec<String> = if uniform {
        first_row.keys().cloned().collect()
    } else {
        let mut field_set = BTreeSet::new();
        for r in rows {
            for k in r.keys() {
                field_set.insert(k.as_str());
            }
        }
        field_set.into_iter().map(str::to_owned).collect()
    };
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
    if n >= 2 && fields.windows(2).all(|w| w[0] < w[1]) {
        let template: crate::store::Row = fields
            .iter()
            .map(|f| (f.clone(), Cell::Null))
            .collect();
        if n >= 4096 {
            let threads = std::thread::available_parallelism()
                .map(|p| p.get())
                .unwrap_or(4)
                .min(8)
                .max(1);
            if threads > 1 {
                let chunk_size = n.div_ceil(threads);
                let chunks: Vec<Vec<crate::store::Row>> = std::thread::scope(|s| {
                    let handles: Vec<_> = (0..threads)
                        .filter_map(|t| {
                            let start = t * chunk_size;
                            if start >= n {
                                return None;
                            }
                            let end = (start + chunk_size).min(n);
                            let tmpl = template.clone();
                            Some(s.spawn(move || {
                                let mut part = Vec::with_capacity(end - start);
                                for i in start..end {
                                    let mut row = tmpl.clone();
                                    for (fi, slot) in row.values_mut().enumerate() {
                                        *slot = match cols.get(fi) {
                                            Some(ColData::Text(v)) => v
                                                .get(i)
                                                .map(|s| Cell::Text(std::sync::Arc::clone(s)))
                                                .unwrap_or_else(|| Cell::text_arc("")),
                                            Some(ColData::Int(v)) => {
                                                Cell::Int(v.get(i).copied().unwrap_or(0))
                                            }
                                            Some(ColData::Float(v)) => {
                                                Cell::Float(v.get(i).copied().unwrap_or(0.0))
                                            }
                                            Some(ColData::Bool(v)) => {
                                                Cell::Bool(v.get(i).copied().unwrap_or(false))
                                            }
                                            Some(ColData::Time(v)) => {
                                                Cell::Time(v.get(i).copied().unwrap_or(0))
                                            }
                                            Some(ColData::Vec(v)) => {
                                                match v.get(i).and_then(|o| o.as_ref()) {
                                                    Some(emb) => {
                                                        Cell::vec_arc(std::sync::Arc::clone(emb))
                                                    }
                                                    _ => Cell::Null,
                                                }
                                            }
                                            Some(ColData::Null) | None => Cell::Null,
                                        };
                                    }
                                    part.push(row);
                                }
                                part
                            }))
                        })
                        .collect();
                    handles
                        .into_iter()
                        .filter_map(|h| h.join().ok())
                        .collect()
                });
                let total_built: usize = chunks.iter().map(Vec::len).sum();
                if total_built == n {
                    let mut rows = Vec::with_capacity(n);
                    for part in chunks {
                        rows.extend(part);
                    }
                    return rows;
                }
            }
        }
        let mut rows = Vec::with_capacity(n);
        for i in 0..n {
            let mut row = template.clone();
            for (fi, slot) in row.values_mut().enumerate() {
                *slot = match cols.get(fi) {
                    Some(ColData::Text(v)) => v
                        .get(i)
                        .map(|s| Cell::Text(std::sync::Arc::clone(s)))
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
            }
            rows.push(row);
        }
        return rows;
    }
    let mut rows = Vec::with_capacity(n);
    for i in 0..n {
        let mut row = crate::store::Row::new();
        for (fi, f) in fields.iter().enumerate() {
            let cell = match cols.get(fi) {
                Some(ColData::Text(v)) => v
                    .get(i)
                    .map(|s| Cell::Text(std::sync::Arc::clone(s)))
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

