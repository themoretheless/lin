//! Columnar, sparse-vector, and dictionary WAL codecs plus background encoder worker pool.

use std::fs::File;
use std::io;
use std::sync::Arc;

use crate::Cell;
use crate::error::Error;
use crate::store::{Edge, Row};

use super::types::{
    ColData, LOG_MAGIC_CHECKSUM, LOG_MAGIC_DICTIONARY, LOG_MAGIC_SPARSE, LOG_MAGIC_V2, LogRecord,
    MAX_RECORD, MAX_SPARSE_DECODED_BYTES, Pack, Persist, SPARSE_VECTOR_THRESHOLD, SyncMode,
    V2_FACTS_BULK, V2_INSERT_COLS,
};
use super::wal::{
    append_record, append_record_v1, durable_sync, finish_in_place_frame,
    finish_in_place_frame_buf, io_err, reserve_wal_space, write_all_vectored,
    write_in_place_frame_buf,
};

#[inline(always)]
pub(crate) fn put_str(buf: &mut Vec<u8>, s: &str) {
    let b = s.as_bytes();
    buf.extend_from_slice(&(b.len() as u32).to_le_bytes());
    buf.extend_from_slice(b);
}

pub(crate) fn append_facts_v2_preencoded(
    log: &mut File,
    rec_gen: u64,
    next_id: u64,
    n: u32,
    encoded_spo: &[u8],
    _buf: &mut Vec<u8>,
    sync: SyncMode,
    log_bytes: &mut u64,
) -> Result<(), Error> {
    let Some(payload_len) = 21usize.checked_add(encoded_spo.len()) else {
        return Err(io_err("log record exceeds 16MiB"));
    };
    let Some(len) = payload_len
        .checked_add(5)
        .filter(|&n| n <= MAX_RECORD as usize)
    else {
        return Err(io_err("log record exceeds 16MiB"));
    };
    let codec = LOG_MAGIC_V2[3];
    let mut hdr = [0u8; 30];
    hdr[..4].copy_from_slice(&LOG_MAGIC_CHECKSUM);
    hdr[4..8].copy_from_slice(&(len as u32).to_le_bytes());
    hdr[8] = codec;
    hdr[9] = V2_FACTS_BULK;
    hdr[10..18].copy_from_slice(&rec_gen.to_le_bytes());
    hdr[18..26].copy_from_slice(&next_id.to_le_bytes());
    hdr[26..30].copy_from_slice(&n.to_le_bytes());

    let mut hasher = crate::crc::Hasher::new();
    hasher.update(&hdr[8..30]);
    hasher.update(encoded_spo);
    let checksum = hasher.finalize().to_le_bytes();

    if payload_len >= 64 * 1024 {
        reserve_wal_space(log, payload_len.saturating_add(13), false);
    }
    let slices = [
        io::IoSlice::new(&hdr),
        io::IoSlice::new(encoded_spo),
        io::IoSlice::new(&checksum),
    ];
    write_all_vectored(log, &slices).map_err(io_err)?;
    if sync == SyncMode::Full {
        durable_sync(log).map_err(io_err)?;
    }
    *log_bytes += 8 + len as u64;
    Ok(())
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
    buf.reserve(9 + 25 + n * 48);
    buf.extend_from_slice(&[0u8; 9]);
    buf.push(V2_FACTS_BULK);
    buf.extend_from_slice(&rec_gen.to_le_bytes());
    buf.extend_from_slice(&next_id.to_le_bytes());
    buf.extend_from_slice(&(n as u32).to_le_bytes());
    for i in 0..n {
        put_str(buf, &s[i]);
        put_str(buf, &p[i]);
        put_str(buf, &o[i]);
    }
    finish_in_place_frame(log, &LOG_MAGIC_V2, buf, sync, log_bytes)
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
        let max_bytes = emb
            .len()
            .checked_mul(8)
            .and_then(|n| n.checked_add(8))
            .ok_or_else(|| io_err("sparse vector size overflow"))?;
        let base = buf.len();
        buf.reserve(max_bytes);
        let mut count = 0u32;
        unsafe {
            let start_ptr = buf.as_mut_ptr().add(base);
            std::ptr::copy_nonoverlapping(dim.to_le_bytes().as_ptr(), start_ptr, 4);
            let mut cur = start_ptr.add(8);
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
                        #[cfg(target_endian = "little")]
                        {
                            let packed = (index as u64) | ((bits as u64) << 32);
                            std::ptr::write_unaligned(cur as *mut u64, packed);
                        }
                        #[cfg(not(target_endian = "little"))]
                        {
                            std::ptr::copy_nonoverlapping(index.to_le_bytes().as_ptr(), cur, 4);
                            std::ptr::copy_nonoverlapping(bits.to_le_bytes().as_ptr(), cur.add(4), 4);
                        }
                        cur = cur.add(8);
                        count += 1;
                    }
                }
            }
            std::ptr::copy_nonoverlapping(count.to_le_bytes().as_ptr(), start_ptr.add(4), 4);
            buf.set_len(cur.offset_from(buf.as_ptr()) as usize);
        }
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
        let max_bytes = emb
            .len()
            .checked_mul(8)
            .and_then(|n| n.checked_add(8))
            .ok_or_else(|| io_err("sparse vector size overflow"))?;
        let base = buf.len();
        buf.reserve(max_bytes);
        let mut count = 0u32;
        unsafe {
            let start_ptr = buf.as_mut_ptr().add(base);
            std::ptr::copy_nonoverlapping(dim.to_le_bytes().as_ptr(), start_ptr, 4);
            let mut cur = start_ptr.add(8);
            for (block, values) in emb.chunks(8).enumerate() {
                let mut mask = if values.len() == 8 {
                    presence_mask8(values)
                } else {
                    values
                        .iter()
                        .enumerate()
                        .fold(0u32, |m, (i, x)| m | (u32::from(x.to_bits() != 0) << i))
                };
                let base_idx = (block * 8) as u32;
                while mask != 0 {
                    let offset = mask.trailing_zeros() as usize;
                    let index = base_idx + (offset as u32);
                    let bits = values[offset].to_bits();
                    #[cfg(target_endian = "little")]
                    {
                        let packed = (index as u64) | ((bits as u64) << 32);
                        std::ptr::write_unaligned(cur as *mut u64, packed);
                    }
                    #[cfg(not(target_endian = "little"))]
                    {
                        std::ptr::copy_nonoverlapping(index.to_le_bytes().as_ptr(), cur, 4);
                        std::ptr::copy_nonoverlapping(bits.to_le_bytes().as_ptr(), cur.add(4), 4);
                    }
                    cur = cur.add(8);
                    count += 1;
                    mask &= mask - 1;
                }
            }
            std::ptr::copy_nonoverlapping(count.to_le_bytes().as_ptr(), start_ptr.add(4), 4);
            buf.set_len(cur.offset_from(buf.as_ptr()) as usize);
        }
    }
    Ok(())
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
pub(super) unsafe fn append_sparse_iter_avx2<'a, const WIDE: bool>(
    iter: impl Iterator<Item = Option<&'a [f32]>>,
    buf: &mut Vec<u8>,
) -> Result<(), Error> {
    use std::arch::x86_64::*;
    for row in iter {
        let Some(emb) = row else {
            buf.extend_from_slice(&0u32.to_le_bytes());
            continue;
        };
        let dim = u32::try_from(emb.len())
            .ok()
            .and_then(|n| n.checked_add(1))
            .ok_or_else(|| io_err("sparse vector dimension overflow"))?;
        let max_bytes = emb
            .len()
            .checked_mul(8)
            .and_then(|n| n.checked_add(8))
            .ok_or_else(|| io_err("sparse vector size overflow"))?;
        let base = buf.len();
        buf.reserve(max_bytes);
        let mut count = 0u32;
        unsafe {
            let start_ptr = buf.as_mut_ptr().add(base);
            std::ptr::copy_nonoverlapping(dim.to_le_bytes().as_ptr(), start_ptr, 4);
            let mut cur = start_ptr.add(8);
            let width = if WIDE { 32 } else { 8 };
            let vector_mask = |values: &[f32]| {
                let bits = _mm256_loadu_si256(values.as_ptr().cast());
                let zero = _mm256_cmpeq_epi32(bits, _mm256_setzero_si256());
                !(_mm256_movemask_ps(_mm256_castsi256_ps(zero)) as u32) & 0xff
            };
            for (block, values) in emb.chunks(width).enumerate() {
                let mut mask = if values.len() == width {
                    let first = vector_mask(values);
                    if WIDE {
                        first | (vector_mask(&values[8..]) << 8)
                            | (vector_mask(&values[16..]) << 16)
                            | (vector_mask(&values[24..]) << 24)
                    } else {
                        first
                    }
                } else {
                    values.iter().enumerate()
                        .fold(0u32, |m, (i, x)| m | (u32::from(x.to_bits() != 0) << i))
                };
                let base_idx = (block * width) as u32;
                while mask != 0 {
                    let offset = mask.trailing_zeros() as usize;
                    let index = base_idx + (offset as u32);
                    let bits = values[offset].to_bits();
                    #[cfg(target_endian = "little")]
                    {
                        let packed = (index as u64) | ((bits as u64) << 32);
                        std::ptr::write_unaligned(cur as *mut u64, packed);
                    }
                    #[cfg(not(target_endian = "little"))]
                    {
                        std::ptr::copy_nonoverlapping(index.to_le_bytes().as_ptr(), cur, 4);
                        std::ptr::copy_nonoverlapping(bits.to_le_bytes().as_ptr(), cur.add(4), 4);
                    }
                    cur = cur.add(8);
                    count += 1;
                    mask &= mask - 1;
                }
            }
            std::ptr::copy_nonoverlapping(count.to_le_bytes().as_ptr(), start_ptr.add(4), 4);
            buf.set_len(cur.offset_from(buf.as_ptr()) as usize);
        }
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
    let (sender, receiver) = std::sync::mpsc::sync_channel::<EncodeJob>(0);
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
    pub(super) wal_dictionary: Option<bool>,
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
            wal_dictionary,
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
            let mut staged = with_dictionary_writer_opt(wal_dictionary, || {
                prepare_insert_columns_ext(
                    generation,
                    next_id,
                    &collection,
                    &rows,
                    uniform_keys,
                    bytes,
                )
            });
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
        wal_dictionary: persist.wal_dictionary,
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
    #[allow(dead_code)]
    pub(super) magic: [u8; 4],
    pub(super) framed_ok: bool,
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
    if let Some((magic, fields)) = encode_insert_rows_direct_at(
        9,
        generation,
        next_id,
        collection,
        rows,
        uniform_keys,
        &mut bytes,
    ) {
        let n = rows.len() as u32;
        let framed_ok = finish_in_place_frame_buf(&magic, &mut bytes).is_ok();
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
                framed_ok,
                bytes,
            }),
        };
    }
    let mut pack = rows_to_insert_cols(collection, rows, Vec::new());
    let encoding = match &mut pack {
        Pack::InsertCols {
            fields, cols, n, ..
        } => {
            match encode_insert_cols_v2_at(
                9, generation, next_id, collection, fields, cols, *n, &mut bytes,
            ) {
                Ok(magic) => {
                    cols.clear();
                    let framed_ok = finish_in_place_frame_buf(&magic, &mut bytes).is_ok();
                    PreparedEncoding::Ready(EncodedColumns {
                        generation,
                        next_id,
                        collection: collection.to_owned(),
                        fields: fields.clone(),
                        rows: *n,
                        magic,
                        framed_ok,
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
        if !prepared.framed_ok {
            return Err(io_err("log record exceeds 16MiB"));
        }
        write_in_place_frame_buf(log, buf, sync, log_bytes)
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
    let magic = encode_insert_cols_v2_at(9, rec_gen, next_id, collection, fields, cols, n, buf)?;
    finish_in_place_frame(log, &magic, buf, sync, log_bytes)
}

/// Encode the existing typed-column payload without touching a file.
#[allow(dead_code)]
pub(super) fn encode_insert_cols_v2(
    rec_gen: u64,
    next_id: u64,
    collection: &str,
    fields: &[String],
    cols: &[ColData],
    n: u32,
    buf: &mut Vec<u8>,
) -> Result<[u8; 4], Error> {
    encode_insert_cols_v2_at(0, rec_gen, next_id, collection, fields, cols, n, buf)
}

#[allow(clippy::too_many_arguments)]
fn encode_insert_cols_v2_at(
    start_off: usize,
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
                if let Some(first_v) = v.iter().find_map(|opt| opt.as_deref()) {
                    let sparse_est = 8usize.saturating_add(
                        count_non_zero_floats(first_v).saturating_mul(8),
                    );
                    let dense_est = 8usize.saturating_add(first_v.len().saturating_mul(4));
                    est_per_row = est_per_row.saturating_add(sparse_est.min(dense_est));
                }
            }
            ColData::Text(v) => {
                let first_len = v.first().map_or(12, |s| s.len());
                est_per_row = est_per_row.saturating_add(4 + first_len);
            }
            ColData::Int(_) | ColData::Float(_) | ColData::Time(_) => {
                est_per_row = est_per_row.saturating_add(8)
            }
            ColData::Bool(_) => est_per_row = est_per_row.saturating_add(1),
            ColData::Null => {}
        }
    }
    buf.clear();
    buf.reserve(start_off + 68 + fields.len() * 16 + (n as usize) * est_per_row);
    if start_off > 0 {
        buf.resize(start_off, 0);
    }
    buf.push(V2_INSERT_COLS);
    buf.extend_from_slice(&rec_gen.to_le_bytes());
    buf.extend_from_slice(&next_id.to_le_bytes());
    put_str(buf, collection);
    buf.extend_from_slice(&n.to_le_bytes());
    buf.extend_from_slice(&(fields.len() as u32).to_le_bytes());
    let mut sparse_codec = false;
    let mut dictionary_codec = false;
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
                #[cfg(target_endian = "little")]
                {
                    // SAFETY: `[i64]` on LE targets has exact 8-byte little-endian layout.
                    let raw = unsafe {
                        std::slice::from_raw_parts(v.as_ptr() as *const u8, v.len() * 8)
                    };
                    buf.extend_from_slice(raw);
                }
                #[cfg(not(target_endian = "little"))]
                for x in v {
                    buf.extend_from_slice(&x.to_le_bytes());
                }
            }
            ColData::Float(v) => {
                buf.push(3);
                #[cfg(target_endian = "little")]
                {
                    // SAFETY: `[f64]` on LE targets has exact IEEE-754 little-endian layout.
                    let raw = unsafe {
                        std::slice::from_raw_parts(v.as_ptr() as *const u8, v.len() * 8)
                    };
                    buf.extend_from_slice(raw);
                }
                #[cfg(not(target_endian = "little"))]
                for x in v {
                    buf.extend_from_slice(&x.to_le_bytes());
                }
            }
            ColData::Bool(v) => {
                buf.push(4);
                buf.reserve(v.len());
                for x in v {
                    buf.push(u8::from(*x));
                }
            }
            ColData::Time(v) => {
                buf.push(5);
                #[cfg(target_endian = "little")]
                {
                    // SAFETY: `[i64]` on LE targets has exact 8-byte little-endian layout.
                    let raw = unsafe {
                        std::slice::from_raw_parts(v.as_ptr() as *const u8, v.len() * 8)
                    };
                    buf.extend_from_slice(raw);
                }
                #[cfg(not(target_endian = "little"))]
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
                        sparse_decoded_bytes =
                            account_sparse_decoded_budget(v, n, sparse_decoded_bytes)?;
                        sparse_codec = true;
                        let mark = buf.len();
                        let large_vec = v
                            .iter()
                            .flatten()
                            .fold(0usize, |n, emb| n.saturating_add(emb.len()))
                            >= 2 * 1024 * 1024;
                        if dictionary_writer_enabled() {
                            buf.push(8);
                            let payload_start = buf.len();
                            let (_, exact_sparse_bytes) = append_dictionary_rows_counted(
                                v.iter().map(|r| r.as_deref()),
                                buf,
                                usize::MAX,
                            )?;
                            if buf.len() - payload_start < exact_sparse_bytes {
                                dictionary_codec = true;
                            } else {
                                buf.truncate(mark);
                                buf.push(7);
                                if large_vec {
                                    append_sparse_rows_parallel(v, buf)?;
                                } else {
                                    append_sparse_rows_serial(v, buf)?;
                                }
                            }
                        } else {
                            buf.push(7);
                            if large_vec {
                                append_sparse_rows_parallel(v, buf)?;
                            } else {
                                append_sparse_rows_serial(v, buf)?;
                            }
                        }
                        wrote_sparse = true;
                    } else if dictionary_writer_enabled() {
                        let mark = buf.len();
                        buf.push(8);
                        let payload_start = buf.len();
                        let limit = if has_empty {
                            usize::MAX
                        } else {
                            payload_start.saturating_add(dense)
                        };
                        let (under_limit, exact_sparse_bytes) = match append_dictionary_rows_counted(
                            v.iter().map(|r| r.as_deref()),
                            buf,
                            limit,
                        ) {
                            Ok(pair) => pair,
                            Err(e) => {
                                buf.truncate(mark);
                                return Err(e);
                            }
                        };
                        let dict_bytes = buf.len() - payload_start;
                        let use_dict = under_limit && dict_bytes < exact_sparse_bytes;
                        if !use_dict {
                            buf.truncate(mark);
                            if has_empty || exact_sparse_bytes < dense {
                                buf.push(7);
                                if let Err(e) = append_sparse_rows_serial(v, buf) {
                                    buf.truncate(mark);
                                    return Err(e);
                                }
                            }
                        }
                        if buf.len() > mark {
                            sparse_decoded_bytes =
                                match account_sparse_decoded_budget(v, n, sparse_decoded_bytes) {
                                    Ok(next) => next,
                                    Err(e) => {
                                        buf.truncate(mark);
                                        return Err(e);
                                    }
                                };
                            sparse_codec = true;
                            dictionary_codec |= use_dict;
                            wrote_sparse = true;
                        }
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
                            sparse_decoded_bytes =
                                match account_sparse_decoded_budget(v, n, sparse_decoded_bytes) {
                                    Ok(next) => next,
                                    Err(e) => {
                                        buf.truncate(mark);
                                        return Err(e);
                                    }
                                };
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
    Ok(if dictionary_codec {
        LOG_MAGIC_DICTIONARY
    } else if sparse_codec {
        LOG_MAGIC_SPARSE
    } else {
        LOG_MAGIC_V2
    })
}

/// Whether new WAL frames may use the dictionary vector codec (codec 5, column tag 8).
/// Off by default: a reader built before the codec refuses such frames, so it may only
/// be turned on once every consumer of the log (followers, backup tools, the previous
/// release kept for rollback) can read it.
static DICTIONARY_WRITER: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

thread_local! {
    /// Per-thread override of [`DICTIONARY_WRITER`], so a test can exercise the writer
    /// without changing what other tests in the same process encode.
    static DICTIONARY_WRITER_OVERRIDE: std::cell::Cell<Option<bool>> =
        const { std::cell::Cell::new(None) };
}

/// Allow (or stop) writing the dictionary vector WAL codec in this process; returns the
/// previous setting. Frames already written stay readable either way. Experimental: see
/// `docs/wal-dictionary.md` for what must hold before turning it on.
pub fn set_wal_dictionary_writer(enabled: bool) -> bool {
    DICTIONARY_WRITER.swap(enabled, std::sync::atomic::Ordering::Relaxed)
}

/// Whether [`set_wal_dictionary_writer`] is on.
pub fn wal_dictionary_writer() -> bool {
    DICTIONARY_WRITER.load(std::sync::atomic::Ordering::Relaxed)
}

#[cfg(test)]
pub(super) fn with_dictionary_writer<R>(enabled: bool, f: impl FnOnce() -> R) -> R {
    with_dictionary_writer_opt(Some(enabled), f)
}

#[inline]
pub(crate) fn with_dictionary_writer_opt<R>(ovr: Option<bool>, f: impl FnOnce() -> R) -> R {
    match ovr {
        None => f(),
        Some(enabled) => {
            let before = DICTIONARY_WRITER_OVERRIDE.with(|o| o.replace(Some(enabled)));
            struct Guard(Option<bool>);
            impl Drop for Guard {
                fn drop(&mut self) {
                    DICTIONARY_WRITER_OVERRIDE.with(|o| o.set(self.0));
                }
            }
            let _guard = Guard(before);
            f()
        }
    }
}

#[inline]
fn dictionary_writer_enabled() -> bool {
    DICTIONARY_WRITER_OVERRIDE
        .with(std::cell::Cell::get)
        .unwrap_or_else(wal_dictionary_writer)
}

/// Most distinct nonzero bit patterns a dictionary row may carry.
const DICTIONARY_MAX_VALUES: usize = 16;

/// Append vector rows in the dictionary layout (column tag 8), stopping as soon as the
/// output reaches `give_up_at` bytes: `Ok(false)` then means "not smaller, discard `out`".
///
/// Per row: `u32` dimension code (0 = absent, otherwise dimension + 1), `u32` count of
/// nonzero values, `u8` dictionary length `d`. With `d` in `1..=16` follow `d` distinct
/// nonzero `f32` bit patterns and one `(index, u8 ordinal)` per nonzero value, the index
/// being `u16` for dimensions up to 65536 and `u32` above; with `d = 0` (more than 16
/// distinct values) follow raw `(u32 index, u32 bits)` entries. Values are compared and
/// stored as bits, so `-0.0`, infinities and NaN payloads survive; `+0.0` is the absent
/// value.
#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
unsafe fn collect_nz_and_dict_neon(
    emb: &[f32],
    nz: &mut smallvec::SmallVec<[(u32, u32); 32]>,
    dictionary: &mut [u32; DICTIONARY_MAX_VALUES],
) -> (usize, bool) {
    let mut d = 0usize;
    let mut raw = false;
    for (block, values) in emb.chunks(8).enumerate() {
        let base_idx = (block * 8) as u32;
        if values.len() == 8 {
            // SAFETY: target_feature(enable = "neon") covers presence_mask8 for 8-float slice.
            let mut mask = unsafe { presence_mask8(values) };
            while mask != 0 {
                let offset = mask.trailing_zeros() as usize;
                mask &= mask - 1;
                let bits = values[offset].to_bits();
                nz.push((base_idx + offset as u32, bits));
                if !raw && !dictionary[..d].contains(&bits) {
                    if d == DICTIONARY_MAX_VALUES {
                        raw = true;
                    } else {
                        dictionary[d] = bits;
                        d += 1;
                    }
                }
            }
        } else {
            for (offset, value) in values.iter().enumerate() {
                let bits = value.to_bits();
                if bits == 0 {
                    continue;
                }
                nz.push((base_idx + offset as u32, bits));
                if !raw && !dictionary[..d].contains(&bits) {
                    if d == DICTIONARY_MAX_VALUES {
                        raw = true;
                    } else {
                        dictionary[d] = bits;
                        d += 1;
                    }
                }
            }
        }
    }
    (d, raw)
}

#[inline]
fn collect_nz_and_dict_scalar(
    emb: &[f32],
    nz: &mut smallvec::SmallVec<[(u32, u32); 32]>,
    dictionary: &mut [u32; DICTIONARY_MAX_VALUES],
) -> (usize, bool) {
    let mut d = 0usize;
    let mut raw = false;
    for (block, values) in emb.chunks(8).enumerate() {
        if values
            .iter()
            .fold(0u32, |bits, value| bits | value.to_bits())
            == 0
        {
            continue;
        }
        let base_idx = (block * 8) as u32;
        for (offset, value) in values.iter().enumerate() {
            let bits = value.to_bits();
            if bits == 0 {
                continue;
            }
            nz.push((base_idx + offset as u32, bits));
            if !raw && !dictionary[..d].contains(&bits) {
                if d == DICTIONARY_MAX_VALUES {
                    raw = true;
                } else {
                    dictionary[d] = bits;
                    d += 1;
                }
            }
        }
    }
    (d, raw)
}

pub(super) fn append_dictionary_rows_counted<'a>(
    rows: impl Iterator<Item = Option<&'a [f32]>>,
    out: &mut Vec<u8>,
    give_up_at: usize,
) -> Result<(bool, usize), Error> {
    #[cfg(target_arch = "aarch64")]
    let use_neon = std::arch::is_aarch64_feature_detected!("neon");
    let mut nz: smallvec::SmallVec<[(u32, u32); 32]> = smallvec::SmallVec::new();
    let mut dictionary = [0u32; DICTIONARY_MAX_VALUES];
    let mut exact_sparse_bytes = 0usize;
    for row in rows {
        if out.len() >= give_up_at {
            return Ok((false, exact_sparse_bytes));
        }
        let Some(emb) = row else {
            out.extend_from_slice(&0u32.to_le_bytes());
            exact_sparse_bytes = exact_sparse_bytes.saturating_add(4);
            continue;
        };
        let code = u32::try_from(emb.len())
            .ok()
            .and_then(|n| n.checked_add(1))
            .ok_or_else(|| io_err("dictionary vector dimension overflow"))?;
        nz.clear();
        #[cfg(target_arch = "aarch64")]
        let (d, raw) = if use_neon {
            // SAFETY: guarded by runtime NEON detection above.
            unsafe { collect_nz_and_dict_neon(emb, &mut nz, &mut dictionary) }
        } else {
            collect_nz_and_dict_scalar(emb, &mut nz, &mut dictionary)
        };
        #[cfg(not(target_arch = "aarch64"))]
        let (d, raw) = collect_nz_and_dict_scalar(emb, &mut nz, &mut dictionary);
        let count = nz.len() as u32;
        exact_sparse_bytes = exact_sparse_bytes
            .saturating_add(8usize.saturating_add(nz.len().saturating_mul(8)));
        out.extend_from_slice(&code.to_le_bytes());
        out.extend_from_slice(&count.to_le_bytes());
        if raw {
            out.push(0);
            out.reserve(nz.len() * 8);
            for &(index, bits) in &nz {
                out.extend_from_slice(&index.to_le_bytes());
                out.extend_from_slice(&bits.to_le_bytes());
            }
            continue;
        }
        out.push(d as u8);
        for bits in &dictionary[..d] {
            out.extend_from_slice(&bits.to_le_bytes());
        }
        let narrow = d > 0 && emb.len() <= 65536;
        out.reserve(nz.len() * if narrow { 3 } else { 5 });
        for &(index, bits) in &nz {
            if narrow {
                out.extend_from_slice(&(index as u16).to_le_bytes());
            } else {
                out.extend_from_slice(&index.to_le_bytes());
            }
            // Present by construction: every nonzero pattern of the row is in the table.
            let ordinal = dictionary[..d].iter().position(|b| *b == bits).unwrap_or(0);
            out.push(ordinal as u8);
        }
    }
    Ok((out.len() < give_up_at, exact_sparse_bytes))
}

#[inline]
pub(super) fn append_dictionary_rows_range(
    rows: &[Row],
    fi: usize,
    start_row: usize,
    end_row: usize,
    buf: &mut Vec<u8>,
) -> Result<usize, Error> {
    let iter = rows[start_row..end_row].iter().map(|r| {
        match r.cells_slice().get(fi) {
            Some(Cell::Vec(emb)) => Some(emb.as_ref()),
            _ => None,
        }
    });
    let (_, exact_sparse_bytes) = append_dictionary_rows_counted(iter, buf, usize::MAX)?;
    Ok(exact_sparse_bytes)
}

#[inline]
pub(super) fn append_sparse_rows_range(
    rows: &[Row],
    fi: usize,
    start_row: usize,
    end_row: usize,
    buf: &mut Vec<u8>,
) -> Result<(), Error> {
    let iter = rows[start_row..end_row].iter().map(|r| {
        match r.cells_slice().get(fi) {
            Some(Cell::Vec(emb)) => Some(emb.as_ref()),
            _ => None,
        }
    });
    append_sparse_iter_serial(iter, buf)
}

pub(super) fn append_sparse_rows_field_serial(
    rows: &[Row],
    fi: usize,
    num_rows: usize,
    buf: &mut Vec<u8>,
) -> Result<(), Error> {
    append_sparse_rows_range(rows, fi, 0, num_rows, buf)
}

struct VecEncodeTask {
    rows_ptr: usize,
    rows_len: usize,
    fi: usize,
    chunk_size: usize,
    total_chunks: usize,
    dict_mode: bool,
    give_up_bytes: usize,
    next_chunk: std::sync::atomic::AtomicUsize,
    done_chunks: std::sync::atomic::AtomicUsize,
    accum_bytes: std::sync::atomic::AtomicUsize,
    aborted: std::sync::atomic::AtomicBool,
    panicked: std::sync::atomic::AtomicBool,
    finished: std::sync::atomic::AtomicBool,
    caller: std::thread::Thread,
    slots: Vec<std::sync::Mutex<Option<Result<(Vec<u8>, usize), Error>>>>,
}

fn run_vec_encode_chunks(task: &VecEncodeTask, is_caller: bool) {
    use std::sync::atomic::Ordering;
    loop {
        let c = task.next_chunk.fetch_add(1, Ordering::AcqRel);
        if c >= task.total_chunks {
            break;
        }
        struct ChunkDone<'a> {
            task: &'a VecEncodeTask,
            is_caller: bool,
            armed: bool,
        }
        impl Drop for ChunkDone<'_> {
            fn drop(&mut self) {
                if self.armed {
                    self.task.panicked.store(true, Ordering::Release);
                }
                if self.task.done_chunks.fetch_add(1, Ordering::AcqRel) + 1
                    == self.task.total_chunks
                    && !self.is_caller
                {
                    self.task.finished.store(true, Ordering::Release);
                    self.task.caller.unpark();
                }
            }
        }
        let mut guard = ChunkDone {
            task,
            is_caller,
            armed: true,
        };
        if !task.aborted.load(Ordering::Relaxed) {
            let rows = unsafe {
                std::slice::from_raw_parts(task.rows_ptr as *const Row, task.rows_len)
            };
            let start = c * task.chunk_size;
            let end = (start + task.chunk_size).min(task.rows_len);
            let mut cbuf = Vec::with_capacity((end - start) * 64);
            let res = if task.dict_mode {
                append_dictionary_rows_range(rows, task.fi, start, end, &mut cbuf)
                    .map(|exact| (cbuf, exact))
            } else {
                append_sparse_rows_range(rows, task.fi, start, end, &mut cbuf).map(|()| {
                    let len = cbuf.len();
                    (cbuf, len)
                })
            };
            if let Ok((ref b, _)) = res {
                let total = task.accum_bytes.fetch_add(b.len(), Ordering::Relaxed) + b.len();
                if total >= task.give_up_bytes {
                    task.aborted.store(true, Ordering::Relaxed);
                }
            } else {
                task.aborted.store(true, Ordering::Relaxed);
            }
            if let Ok(mut slot) = task.slots[c].lock() {
                *slot = Some(res);
            }
        }
        guard.armed = false;
    }
}

fn append_vec_rows_field_parallel(
    rows: &[Row],
    fi: usize,
    num_rows: usize,
    dict_mode: bool,
    give_up_bytes: usize,
    buf: &mut Vec<u8>,
) -> Result<(bool, usize), Error> {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    let short_serial = !dict_mode && (512..2048).contains(&num_rows)
        && std::env::var_os("LIN_BENCH_SERIAL_WAL_VECTORS").is_some();
    if num_rows < 128 || short_serial {
        if dict_mode {
            let payload_start = buf.len();
            let iter = rows[..num_rows].iter().map(|r| {
                match r.cells_slice().get(fi) {
                    Some(Cell::Vec(emb)) => Some(emb.as_ref()),
                    _ => None,
                }
            });
            return append_dictionary_rows_counted(
                iter,
                buf,
                payload_start.saturating_add(give_up_bytes),
            );
        }
        let payload_start = buf.len();
        #[cfg(target_arch = "x86_64")]
        if short_serial
            && std::env::var_os("LIN_BENCH_AVX2_SERIAL").is_some()
            && std::arch::is_x86_feature_detected!("avx2")
        {
            let iter = rows[..num_rows].iter().map(|row| match row.cells_slice().get(fi) {
                Some(Cell::Vec(vector)) => Some(vector.as_ref()),
                _ => None,
            });
            // SAFETY: the detected feature covers full eight-element loads.
            unsafe {
                if std::env::var_os("LIN_BENCH_AVX2_WIDE").is_some() {
                    append_sparse_iter_avx2::<true>(iter, buf)?;
                } else {
                    append_sparse_iter_avx2::<false>(iter, buf)?;
                }
            }
            let bytes = buf.len() - payload_start;
            return Ok((bytes < give_up_bytes, bytes));
        }
        append_sparse_rows_field_serial(rows, fi, num_rows, buf)?;
        let bytes = buf.len() - payload_start;
        return Ok((bytes < give_up_bytes, bytes));
    }

    static POOL: std::sync::OnceLock<
        Vec<std::sync::mpsc::SyncSender<std::sync::Arc<VecEncodeTask>>>,
    > = std::sync::OnceLock::new();
    let workers = POOL.get_or_init(|| {
        let threads = std::thread::available_parallelism()
            .map(|p| p.get())
            .unwrap_or(1)
            .min(8);
        if threads <= 1 {
            return Vec::new();
        }
        let num_workers = threads - 1;
        let mut txs = Vec::with_capacity(num_workers);
        for idx in 0..num_workers {
            let (tx, rx) = std::sync::mpsc::sync_channel::<std::sync::Arc<VecEncodeTask>>(16);
            if std::thread::Builder::new()
                .name(format!("lin-vec-enc-{idx}"))
                .spawn(move || {
                    while let Ok(task) = rx.recv() {
                        run_vec_encode_chunks(&task, false);
                    }
                })
                .is_ok()
            {
                txs.push(tx);
            }
        }
        txs
    });

    if workers.is_empty() {
        if dict_mode {
            let payload_start = buf.len();
            let iter = rows[..num_rows].iter().map(|r| {
                match r.cells_slice().get(fi) {
                    Some(Cell::Vec(emb)) => Some(emb.as_ref()),
                    _ => None,
                }
            });
            return append_dictionary_rows_counted(
                iter,
                buf,
                payload_start.saturating_add(give_up_bytes),
            );
        }
        let payload_start = buf.len();
        append_sparse_rows_field_serial(rows, fi, num_rows, buf)?;
        let bytes = buf.len() - payload_start;
        return Ok((bytes < give_up_bytes, bytes));
    }

    let chunk_size = 64usize;
    let total_chunks = num_rows.div_ceil(chunk_size);
    let mut slots = Vec::with_capacity(total_chunks);
    for _ in 0..total_chunks {
        slots.push(std::sync::Mutex::new(None));
    }
    let task = std::sync::Arc::new(VecEncodeTask {
        rows_ptr: rows.as_ptr() as usize,
        rows_len: num_rows,
        fi,
        chunk_size,
        total_chunks,
        dict_mode,
        give_up_bytes,
        next_chunk: AtomicUsize::new(0),
        done_chunks: AtomicUsize::new(0),
        accum_bytes: AtomicUsize::new(0),
        aborted: AtomicBool::new(false),
        panicked: AtomicBool::new(false),
        finished: AtomicBool::new(false),
        caller: std::thread::current(),
        slots,
    });
    struct VecTaskWaitGuard<'a>(&'a VecEncodeTask);
    impl Drop for VecTaskWaitGuard<'_> {
        fn drop(&mut self) {
            if std::thread::panicking() {
                self.0.aborted.store(true, Ordering::Relaxed);
            }
            while self.0.done_chunks.load(Ordering::Acquire) < self.0.total_chunks {
                if !self.0.finished.load(Ordering::Acquire) {
                    std::thread::park_timeout(std::time::Duration::from_micros(50));
                }
            }
        }
    }
    let _wait_guard = VecTaskWaitGuard(&task);

    let wake = (total_chunks - 1).min(workers.len());
    for worker in workers.iter().take(wake) {
        let _ = worker.try_send(std::sync::Arc::clone(&task));
    }
    run_vec_encode_chunks(&task, true);

    for _ in 0..64 {
        if task.done_chunks.load(Ordering::Acquire) >= total_chunks {
            break;
        }
        std::hint::spin_loop();
    }
    while task.done_chunks.load(Ordering::Acquire) < total_chunks {
        if !task.finished.load(Ordering::Acquire) {
            std::thread::park_timeout(std::time::Duration::from_micros(50));
        }
    }
    if task.panicked.load(Ordering::Acquire) {
        panic!("vector encode worker panicked");
    }

    if task.aborted.load(Ordering::Acquire) {
        for slot in &task.slots {
            if let Some(Err(e)) = slot.lock().ok().and_then(|mut g| g.take()) {
                return Err(e);
            }
        }
        return Ok((false, 0));
    }

    let mut total_exact_sparse = 0usize;
    let total_payload = task.accum_bytes.load(Ordering::Acquire);
    buf.reserve(total_payload);
    for slot in &task.slots {
        let item = slot
            .lock()
            .ok()
            .and_then(|mut g| g.take())
            .ok_or_else(|| io_err("missing vector encode chunk"))??;
        buf.extend_from_slice(&item.0);
        total_exact_sparse = total_exact_sparse.saturating_add(item.1);
    }
    Ok((true, total_exact_sparse))
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
    encode_insert_rows_direct_at(0, rec_gen, next_id, collection, rows, uniform_keys, buf)
}

/// Encode snapshot rows directly into `buf` with an in-place 8-byte `LCOL` + `magic` header,
/// avoiding a second buffer allocation and `memcpy` per chunk.
pub(super) fn encode_insert_rows_lcol(rows: &[Row], buf: &mut Vec<u8>) -> bool {
    let Some((magic, _)) = encode_insert_rows_direct_at(8, 0, 0, "", rows, false, buf) else {
        return false;
    };
    buf[..4].copy_from_slice(b"LCOL");
    buf[4..8].copy_from_slice(&magic);
    true
}

fn encode_insert_rows_direct_at(
    start_off: usize,
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
    let mut est_per_row = 0usize;
    for val in first.values() {
        let t = match val {
            Cell::Text(s) => {
                est_per_row = est_per_row.saturating_add(4 + s.len());
                DirectColType::Text
            }
            Cell::SmallText(s) => {
                est_per_row = est_per_row.saturating_add(4 + s.len());
                DirectColType::Text
            }
            Cell::Int(_) => {
                est_per_row = est_per_row.saturating_add(8);
                DirectColType::Int
            }
            Cell::Float(_) => {
                est_per_row = est_per_row.saturating_add(8);
                DirectColType::Float
            }
            Cell::Bool(_) => {
                est_per_row = est_per_row.saturating_add(1);
                DirectColType::Bool
            }
            Cell::Time(_) => {
                est_per_row = est_per_row.saturating_add(8);
                DirectColType::Time
            }
            Cell::Vec(emb) => {
                let sparse_est =
                    8usize.saturating_add(count_non_zero_floats(emb).saturating_mul(8));
                let dense_est = 4usize.saturating_add(emb.len().saturating_mul(4));
                est_per_row = est_per_row.saturating_add(sparse_est.min(dense_est));
                DirectColType::Vec
            }
            _ => return None,
        };
        col_types.push(t);
    }

    let first_shape = first.shape_arc()?;
    if uniform_keys {
        for r in rows {
            if r.cells_slice().len() != num_fields {
                return None;
            }
        }
    } else {
        for r in &rows[1..] {
            if r.cells_slice().len() != num_fields
                || (!r.shape_arc().is_some_and(|s| Arc::ptr_eq(s, first_shape))
                    && !r.keys().eq(first.keys()))
            {
                return None;
            }
        }
    }

    let fields: Vec<String> = first.keys().cloned().collect();
    let n = rows.len() as u32;
    let fields_meta_bytes: usize = fields.iter().map(|f| 5 + f.len()).sum();

    buf.clear();
    buf.reserve(
        start_off
            + 68
            + collection.len()
            + fields_meta_bytes
            + rows.len().saturating_mul(est_per_row.saturating_add(num_fields * 2)),
    );
    if start_off > 0 {
        buf.resize(start_off, 0);
    }
    buf.push(V2_INSERT_COLS);
    buf.extend_from_slice(&rec_gen.to_le_bytes());
    buf.extend_from_slice(&next_id.to_le_bytes());
    put_str(buf, collection);
    buf.extend_from_slice(&n.to_le_bytes());
    buf.extend_from_slice(&(num_fields as u32).to_le_bytes());

    let mut sparse_codec = false;
    let mut dictionary_codec = false;
    let mut sparse_decoded_bytes = 0usize;

    for (fi, f) in fields.iter().enumerate() {
        put_str(buf, f);
        match col_types[fi] {
            DirectColType::Text => {
                buf.push(1);
                for r in rows {
                    if let Some(s) = r.cells_slice()[fi].text() {
                        put_str(buf, s);
                    } else {
                        return None;
                    }
                }
            }
            DirectColType::Int => {
                buf.push(2);
                let base = buf.len();
                let byte_len = rows.len().checked_mul(8)?;
                buf.reserve(byte_len);
                unsafe {
                    let mut dst = buf.as_mut_ptr().add(base);
                    for r in rows {
                        match &r.cells_slice()[fi] {
                            Cell::Int(x) => {
                                std::ptr::copy_nonoverlapping(x.to_le_bytes().as_ptr(), dst, 8);
                                dst = dst.add(8);
                            }
                            _ => return None,
                        }
                    }
                    buf.set_len(base + byte_len);
                }
            }
            DirectColType::Float => {
                buf.push(3);
                let base = buf.len();
                let byte_len = rows.len().checked_mul(8)?;
                buf.reserve(byte_len);
                unsafe {
                    let mut dst = buf.as_mut_ptr().add(base);
                    for r in rows {
                        match &r.cells_slice()[fi] {
                            Cell::Float(x) => {
                                std::ptr::copy_nonoverlapping(x.to_le_bytes().as_ptr(), dst, 8);
                                dst = dst.add(8);
                            }
                            _ => return None,
                        }
                    }
                    buf.set_len(base + byte_len);
                }
            }
            DirectColType::Bool => {
                buf.push(4);
                let base = buf.len();
                let byte_len = rows.len();
                buf.reserve(byte_len);
                unsafe {
                    let mut dst = buf.as_mut_ptr().add(base);
                    for r in rows {
                        match &r.cells_slice()[fi] {
                            Cell::Bool(x) => {
                                *dst = u8::from(*x);
                                dst = dst.add(1);
                            }
                            _ => return None,
                        }
                    }
                    buf.set_len(base + byte_len);
                }
            }
            DirectColType::Time => {
                buf.push(5);
                let base = buf.len();
                let byte_len = rows.len().checked_mul(8)?;
                buf.reserve(byte_len);
                unsafe {
                    let mut dst = buf.as_mut_ptr().add(base);
                    for r in rows {
                        match &r.cells_slice()[fi] {
                            Cell::Time(x) => {
                                std::ptr::copy_nonoverlapping(x.to_le_bytes().as_ptr(), dst, 8);
                                dst = dst.add(8);
                            }
                            _ => return None,
                        }
                    }
                    buf.set_len(base + byte_len);
                }
            }
            DirectColType::Vec => {
                let mut dense = 0usize;
                let mut has_empty = false;
                let mut first_emb: Option<&[f32]> = None;
                for (ri, r) in rows.iter().enumerate() {
                    match &r.cells_slice()[fi] {
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
                    let mut accepted_sparse = false;
                    let mut used_dict = false;
                    let mark = buf.len();
                    let mut try_raw_sparse = !dictionary_writer_enabled();
                    if !try_raw_sparse {
                        let dict_give_up = if has_empty {
                            usize::MAX
                        } else {
                            dense.saturating_add(rows.len().saturating_mul(65))
                        };
                        buf.push(8);
                        let payload_start = buf.len();
                        let Ok((dict_ok, exact_sparse_bytes)) = append_vec_rows_field_parallel(
                            rows,
                            fi,
                            rows.len(),
                            true,
                            dict_give_up,
                            buf,
                        ) else {
                            buf.truncate(mark);
                            return None;
                        };
                        let dict_bytes = buf.len() - payload_start;
                        if dict_ok
                            && dict_bytes < exact_sparse_bytes
                            && (has_empty || exact_sparse_bytes < dense)
                        {
                            accepted_sparse = true;
                            used_dict = true;
                        } else {
                            buf.truncate(mark);
                            try_raw_sparse = has_empty || (dict_ok && exact_sparse_bytes < dense);
                        }
                    }
                    if try_raw_sparse {
                        buf.push(7);
                        let payload_start = buf.len();
                        let sparse_give_up = if has_empty { usize::MAX } else { dense };
                        let Ok((sparse_ok, _)) = append_vec_rows_field_parallel(
                            rows,
                            fi,
                            rows.len(),
                            false,
                            sparse_give_up,
                            buf,
                        ) else {
                            buf.truncate(mark);
                            return None;
                        };
                        let sparse_bytes = buf.len() - payload_start;
                        if !has_empty && (!sparse_ok || sparse_bytes >= dense) {
                            buf.truncate(mark);
                        } else {
                            accepted_sparse = true;
                        }
                    }
                    if accepted_sparse {
                        let storage = rows
                            .len()
                            .checked_mul(std::mem::size_of::<Option<Vec<f32>>>())?;
                        let mut next_decoded = sparse_decoded_bytes.checked_add(storage)?;
                        for r in rows {
                            if let Cell::Vec(emb) = &r.cells_slice()[fi] {
                                let bytes = emb.len().checked_mul(4)?;
                                next_decoded = next_decoded
                                    .checked_add(bytes)
                                    .filter(|&b| b <= MAX_SPARSE_DECODED_BYTES)?;
                            }
                        }
                        sparse_decoded_bytes = next_decoded;
                        sparse_codec = true;
                        dictionary_codec |= used_dict;
                        wrote_sparse = true;
                    }
                }
                if !wrote_sparse {
                    buf.push(6);
                    for r in rows {
                        match &r.cells_slice()[fi] {
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
        if dictionary_codec {
            LOG_MAGIC_DICTIONARY
        } else if sparse_codec {
            LOG_MAGIC_SPARSE
        } else {
            LOG_MAGIC_V2
        },
        fields,
    ))
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
                            #[cfg(target_endian = "little")]
                            let emb = {
                                let mut emb = vec![0.0f32; dim];
                                // SAFETY: `emb` has `dim * 4 == nbytes` initialized bytes and `buf[i..i+nbytes]` is in bounds.
                                unsafe {
                                    std::ptr::copy_nonoverlapping(
                                        buf.as_ptr().add(i),
                                        emb.as_mut_ptr() as *mut u8,
                                        nbytes,
                                    );
                                }
                                i += nbytes;
                                emb
                            };
                            #[cfg(not(target_endian = "little"))]
                            let emb = {
                                let mut emb = Vec::with_capacity(dim);
                                for _ in 0..dim {
                                    emb.push(f32::from_le_bytes(buf[i..i + 4].try_into().unwrap()));
                                    i += 4;
                                }
                                emb
                            };
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
/// WAL pack for freshly inserted rows that replays to exactly those rows.
///
/// [`rows_to_insert_cols`] is compact but has no null marker and one type per column: a
/// missing field comes back as `0` / `""` / `false`, and a float in an integer column as
/// `0`. Batches it cannot represent exactly are therefore logged row by row.
pub fn rows_to_insert_pack(
    collection: impl Into<String>,
    rows: &[crate::store::Row],
    edges: Vec<Edge>,
) -> Pack {
    if insert_cols_lossless(rows) {
        rows_to_insert_cols(collection, rows, edges)
    } else {
        Pack::InsertBulk {
            collection: collection.into(),
            rows: rows.to_vec(),
            edges,
        }
    }
}

/// True when every row has the same fields, none of them null, and each field holds one
/// value type throughout — the only shape [`ColData`] round-trips exactly.
pub(crate) fn insert_cols_lossless(rows: &[crate::store::Row]) -> bool {
    use crate::store::Cell;
    fn kind(cell: &Cell) -> u8 {
        match cell {
            Cell::Null => 0,
            Cell::Text(_) | Cell::SmallText(_) => 1,
            Cell::Int(_) => 2,
            Cell::Float(_) => 3,
            Cell::Bool(_) => 4,
            Cell::Time(_) => 5,
            // An empty vector decodes as "no vector".
            Cell::Vec(v) => {
                if v.is_empty() {
                    0
                } else {
                    6
                }
            }
        }
    }
    let Some(first) = rows.first() else {
        return true;
    };
    let Some(first_shape) = first.shape_arc() else {
        return rows.len() == 1 || rows[1..].iter().all(crate::store::Row::is_empty);
    };
    let first_cells = first.cells_slice();
    let mut kinds: smallvec::SmallVec<[u8; 16]> =
        smallvec::SmallVec::with_capacity(first_cells.len());
    for c in first_cells {
        let k = kind(c);
        if k == 0 {
            return false;
        }
        kinds.push(k);
    }
    rows[1..].iter().all(|r| {
        let cells = r.cells_slice();
        cells.len() == kinds.len()
            && (r.shape_arc().is_some_and(|s| Arc::ptr_eq(s, first_shape))
                || r.keys().eq(first.keys()))
            && cells.iter().zip(&kinds).all(|(c, k)| kind(c) == *k)
    })
}

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

fn account_sparse_decoded_budget(
    v: &[Option<Arc<[f32]>>],
    n: u32,
    current: usize,
) -> Result<usize, Error> {
    if v.len() != n as usize {
        return Err(io_err("sparse vector column length mismatch"));
    }
    let storage = v
        .len()
        .checked_mul(std::mem::size_of::<Option<Vec<f32>>>())
        .ok_or_else(|| io_err("sparse vector size overflow"))?;
    let mut next_decoded = current
        .checked_add(storage)
        .ok_or_else(|| io_err("sparse vector size overflow"))?;
    for emb in v.iter().flatten() {
        let bytes = emb
            .len()
            .checked_mul(4)
            .ok_or_else(|| io_err("sparse vector size overflow"))?;
        next_decoded = next_decoded
            .checked_add(bytes)
            .filter(|&b| b <= MAX_SPARSE_DECODED_BYTES)
            .ok_or_else(|| io_err("sparse vectors exceed decoded 64MiB budget"))?;
    }
    if next_decoded > MAX_SPARSE_DECODED_BYTES {
        return Err(io_err("sparse vectors exceed decoded 64MiB budget"));
    }
    Ok(next_decoded)
}

#[inline(always)]
fn col_cell_at(cols: &[ColData], fi: usize, i: usize) -> crate::store::Cell {
    use crate::store::Cell;
    match cols.get(fi) {
        Some(ColData::Text(v)) => v
            .get(i)
            .map(|s| Cell::Text(Arc::clone(s)))
            .unwrap_or_else(|| Cell::text_arc("")),
        Some(ColData::Int(v)) => Cell::Int(v.get(i).copied().unwrap_or(0)),
        Some(ColData::Float(v)) => Cell::Float(v.get(i).copied().unwrap_or(0.0)),
        Some(ColData::Bool(v)) => Cell::Bool(v.get(i).copied().unwrap_or(false)),
        Some(ColData::Time(v)) => Cell::Time(v.get(i).copied().unwrap_or(0)),
        Some(ColData::Vec(v)) => match v.get(i).and_then(|o| o.as_ref()) {
            Some(emb) => Cell::vec_arc(Arc::clone(emb)),
            None => Cell::Null,
        },
        Some(ColData::Null) | None => Cell::Null,
    }
}

/// Decode an `LCOL` snapshot column payload (`&payload[8..]`) directly into `Vec<Row>`
/// without allocating intermediate `ColData` vectors or heap `Arc<str>`s for short strings
/// (`<= InlineStr::MAX_LEN` = 22 bytes), reusing `shape_cache` across chunks of a collection.
pub(super) fn decode_lcol_to_rows(
    buf: &[u8],
    allow_sparse: bool,
    shape_cache: &mut Option<(Vec<String>, Arc<crate::store::RowShape>)>,
) -> Result<Vec<Row>, ()> {
    use crate::store::{InlineStr, RowShape};

    if buf.is_empty() {
        return Err(());
    }
    let mut i = 0usize;
    let kind = buf[i];
    i += 1;
    if kind != V2_INSERT_COLS {
        return Err(());
    }
    let _rec_gen = take_u64(buf, &mut i)?;
    let _next_id = take_u64(buf, &mut i)?;
    let _collection = take_str_ref(buf, &mut i)?;
    let n = take_u32(buf, &mut i)? as usize;
    let nf = take_u32(buf, &mut i)? as usize;
    if allow_sparse
        && (nf > buf.len().saturating_sub(i) / 5
            || n.checked_mul(std::mem::size_of::<Option<Vec<f32>>>())
                .is_none_or(|bytes| bytes > MAX_SPARSE_DECODED_BYTES))
    {
        return Err(());
    }
    if nf == 0 {
        if allow_sparse {
            return Err(());
        }
        return Ok(vec![Row::new(); n]);
    }

    // Pass 1: validate framing/bounds of all `nf` columns and collect `(field_name, col_offset, tag)`
    // in O(nf) for scalar columns and O(n) bounds checks for variable-width columns.
    let mut sparse_decoded_bytes = 0usize;
    let mut saw_sparse = false;
    let mut col_metas: smallvec::SmallVec<[(&str, usize, u8); 16]> =
        smallvec::SmallVec::with_capacity(nf.min(64));
    for _ in 0..nf {
        let fname = take_str_ref(buf, &mut i)?;
        if i >= buf.len() {
            return Err(());
        }
        let ct = buf[i];
        i += 1;
        let col_start = i;
        match ct {
            0 => {}
            1 => {
                for _ in 0..n {
                    let slen = take_u32(buf, &mut i)? as usize;
                    i = i.checked_add(slen).filter(|&end| end <= buf.len()).ok_or(())?;
                }
            }
            2 | 3 | 5 => {
                let bytes = n.checked_mul(8).ok_or(())?;
                i = i.checked_add(bytes).filter(|&end| end <= buf.len()).ok_or(())?;
            }
            4 => {
                i = i.checked_add(n).filter(|&end| end <= buf.len()).ok_or(())?;
            }
            6 => {
                for _ in 0..n {
                    let dim = take_u32(buf, &mut i)? as usize;
                    if dim > 0 {
                        let nbytes = dim.checked_mul(4).ok_or(())?;
                        i = i
                            .checked_add(nbytes)
                            .filter(|&end| end <= buf.len())
                            .ok_or(())?;
                    }
                }
            }
            7 if allow_sparse => {
                saw_sparse = true;
                let storage = n
                    .checked_mul(std::mem::size_of::<Option<Vec<f32>>>())
                    .ok_or(())?;
                sparse_decoded_bytes = sparse_decoded_bytes
                    .checked_add(storage)
                    .filter(|&bytes| bytes <= MAX_SPARSE_DECODED_BYTES)
                    .ok_or(())?;
                if n > buf.len().saturating_sub(i) / 4 {
                    return Err(());
                }
                for _ in 0..n {
                    let code = take_u32(buf, &mut i)?;
                    if code == 0 {
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
                    let mut previous = None;
                    for _ in 0..count {
                        let pos = take_u32(buf, &mut i)? as usize;
                        let _bits = take_u32(buf, &mut i)?;
                        if pos >= dim || previous.is_some_and(|last| pos <= last) {
                            return Err(());
                        }
                        previous = Some(pos);
                    }
                }
            }
            _ => return Err(()),
        }
        col_metas.push((fname, col_start, ct));
    }
    if allow_sparse && (!saw_sparse || i != buf.len()) {
        return Err(());
    }
    if n == 0 {
        return Ok(Vec::new());
    }

    // Resolve `Arc<RowShape>` (reusing `shape_cache` across chunks of the same collection).
    let is_sorted = col_metas.windows(2).all(|w| w[0].0 < w[1].0);
    let shape: Arc<RowShape> = if is_sorted {
        if let Some((cached_names, cached_shape)) = shape_cache.as_ref()
            && cached_names.len() == nf
            && cached_names
                .iter()
                .zip(col_metas.iter())
                .all(|(a, (b, _, _))| a == *b)
        {
            Arc::clone(cached_shape)
        } else {
            let names_vec: Vec<String> =
                col_metas.iter().map(|(s, _, _)| (*s).to_owned()).collect();
            let built = Row::shape_of(&names_vec);
            *shape_cache = Some((names_vec, Arc::clone(&built)));
            built
        }
    } else {
        let names_ref: smallvec::SmallVec<[&str; 16]> =
            col_metas.iter().map(|(s, _, _)| *s).collect();
        let built = Row::shape_of(&names_ref);
        if built.names().len() != nf {
            // Duplicate field names fallback.
            let rec = decode_column_record(buf, allow_sparse)?;
            if let Pack::InsertCols {
                fields, cols, n, ..
            } = rec.pack
            {
                return Ok(cols_to_rows(&fields, &cols, n as usize));
            }
            return Err(());
        }
        built
    };

    let mut perm: smallvec::SmallVec<[usize; 16]> = smallvec::SmallVec::with_capacity(nf);
    if is_sorted {
        for fi in 0..nf {
            perm.push(fi);
        }
    } else {
        for (fname, _, _) in &col_metas {
            let pos = shape
                .names()
                .binary_search_by(|s| s.as_str().cmp(fname))
                .map_err(|_| ())?;
            perm.push(pos);
        }
    }

    let mut rows = Vec::with_capacity(n);
    for _ in 0..n {
        rows.push(Row::from_parts(
            Arc::clone(&shape),
            std::iter::repeat_n(Cell::Null, nf),
        ));
    }

    // Pass 2: materialize each column directly into its row cell slot.
    for (fi, &(fname, mut col_i, ct)) in col_metas.iter().enumerate() {
        let slot = perm[fi];
        match ct {
            0 => {}
            1 => {
                // Fields mirrored as `Arc<str>` in `Store::rebuild_row_maps` stay `Cell::Text`
                // so `rebuild_row_maps` shares the `Arc<str>` instead of re-allocating via
                // `SmallText::text_shared`, and `Row` Debug output matches live inserts.
                // All other short strings (`<= 22B`, e.g. `hash`, `room`, tags) use inline `SmallText`.
                let needs_arc = matches!(
                    fname,
                    "id" | "uri" | "title" | "layer" | "wing" | "user_id" | "email" | "s" | "p" | "o"
                );
                if needs_arc {
                    let mut recent: [Option<Arc<str>>; 4] = [None, None, None, None];
                    let mut recent_idx = 0usize;
                    for row in &mut rows {
                        let slen =
                            u32::from_le_bytes(buf[col_i..col_i + 4].try_into().unwrap()) as usize;
                        col_i += 4;
                        let s_ref =
                            std::str::from_utf8(&buf[col_i..col_i + slen]).map_err(|_| ())?;
                        col_i += slen;
                        let arc = match s_ref {
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
                            "rag" | "sys" | "core" | "test" => {
                                crate::store::value::intern_wing_str(s_ref)
                            }
                            _ => {
                                if slen <= 64
                                    && let Some(hit) = recent
                                        .iter()
                                        .flatten()
                                        .find(|prev| prev.as_ref() == s_ref)
                                {
                                    Arc::clone(hit)
                                } else {
                                    let a: Arc<str> = Arc::from(s_ref);
                                    if slen <= 64 {
                                        recent[recent_idx & 3] = Some(Arc::clone(&a));
                                        recent_idx = recent_idx.wrapping_add(1);
                                    }
                                    a
                                }
                            }
                        };
                        row.cells_mut()[slot] = Cell::Text(arc);
                    }
                } else {
                    let mut prev_arc: Option<Arc<str>> = None;
                    for row in &mut rows {
                        let slen =
                            u32::from_le_bytes(buf[col_i..col_i + 4].try_into().unwrap()) as usize;
                        col_i += 4;
                        let s_ref =
                            std::str::from_utf8(&buf[col_i..col_i + slen]).map_err(|_| ())?;
                        col_i += slen;
                        row.cells_mut()[slot] = if slen <= InlineStr::MAX_LEN {
                            Cell::SmallText(InlineStr::new_truncate(s_ref))
                        } else if let Some(ref prev) = prev_arc
                            && prev.as_ref() == s_ref
                        {
                            Cell::Text(Arc::clone(prev))
                        } else {
                            let a: Arc<str> = Arc::from(s_ref);
                            if slen <= 64 {
                                prev_arc = Some(Arc::clone(&a));
                            }
                            Cell::Text(a)
                        };
                    }
                }
            }
            2 => {
                for (row, chunk) in rows.iter_mut().zip(buf[col_i..col_i + n * 8].chunks_exact(8))
                {
                    row.cells_mut()[slot] =
                        Cell::Int(i64::from_le_bytes(chunk.try_into().unwrap()));
                }
            }
            3 => {
                for (row, chunk) in rows.iter_mut().zip(buf[col_i..col_i + n * 8].chunks_exact(8))
                {
                    row.cells_mut()[slot] =
                        Cell::Float(f64::from_le_bytes(chunk.try_into().unwrap()));
                }
            }
            4 => {
                for (row, &b) in rows.iter_mut().zip(&buf[col_i..col_i + n]) {
                    row.cells_mut()[slot] = Cell::Bool(b != 0);
                }
            }
            5 => {
                for (row, chunk) in rows.iter_mut().zip(buf[col_i..col_i + n * 8].chunks_exact(8))
                {
                    row.cells_mut()[slot] =
                        Cell::Time(i64::from_le_bytes(chunk.try_into().unwrap()));
                }
            }
            6 => {
                for row in &mut rows {
                    let dim =
                        u32::from_le_bytes(buf[col_i..col_i + 4].try_into().unwrap()) as usize;
                    col_i += 4;
                    if dim == 0 {
                        continue;
                    }
                    let nbytes = dim * 4;
                    #[cfg(target_endian = "little")]
                    let emb = {
                        let mut emb = vec![0.0f32; dim];
                        // SAFETY: bounds checked in Pass 1; `emb` has `nbytes` bytes.
                        unsafe {
                            std::ptr::copy_nonoverlapping(
                                buf.as_ptr().add(col_i),
                                emb.as_mut_ptr() as *mut u8,
                                nbytes,
                            );
                        }
                        col_i += nbytes;
                        emb
                    };
                    #[cfg(not(target_endian = "little"))]
                    let emb = {
                        let mut emb = Vec::with_capacity(dim);
                        for _ in 0..dim {
                            emb.push(f32::from_le_bytes(
                                buf[col_i..col_i + 4].try_into().unwrap(),
                            ));
                            col_i += 4;
                        }
                        emb
                    };
                    row.cells_mut()[slot] = Cell::vec_arc(emb);
                }
            }
            7 => {
                for row in &mut rows {
                    let code = u32::from_le_bytes(buf[col_i..col_i + 4].try_into().unwrap());
                    col_i += 4;
                    if code == 0 {
                        continue;
                    }
                    let dim = (code - 1) as usize;
                    let count =
                        u32::from_le_bytes(buf[col_i..col_i + 4].try_into().unwrap()) as usize;
                    col_i += 4;
                    let mut values = vec![0.0f32; dim];
                    for _ in 0..count {
                        let pos =
                            u32::from_le_bytes(buf[col_i..col_i + 4].try_into().unwrap()) as usize;
                        let bits =
                            u32::from_le_bytes(buf[col_i + 4..col_i + 8].try_into().unwrap());
                        col_i += 8;
                        values[pos] = f32::from_bits(bits);
                    }
                    row.cells_mut()[slot] = Cell::vec_arc(values);
                }
            }
            _ => unreachable!(),
        }
    }

    Ok(rows)
}

pub fn cols_to_rows(fields: &[String], cols: &[ColData], n: usize) -> Vec<crate::store::Row> {
    if n == 0 {
        return Vec::new();
    }
    if fields.is_empty() {
        return vec![crate::store::Row::new(); n];
    }
    let nf = fields.len();
    if fields.windows(2).all(|w| w[0] < w[1]) {
        let shape = crate::store::Row::shape_of(fields);
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
                            let sh = Arc::clone(&shape);
                            Some(s.spawn(move || {
                                let mut part = Vec::with_capacity(end - start);
                                for i in start..end {
                                    part.push(crate::store::Row::from_parts(
                                        Arc::clone(&sh),
                                        (0..nf).map(|fi| col_cell_at(cols, fi, i)),
                                    ));
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
            rows.push(crate::store::Row::from_parts(
                Arc::clone(&shape),
                (0..nf).map(|fi| col_cell_at(cols, fi, i)),
            ));
        }
        return rows;
    }
    let shape = crate::store::Row::shape_of(fields);
    if shape.names().len() == nf {
        let mut sorted_to_fi: smallvec::SmallVec<[usize; 16]> = smallvec::SmallVec::with_capacity(nf);
        for name in shape.names() {
            if let Some(fi) = fields.iter().position(|f| f == name) {
                sorted_to_fi.push(fi);
            }
        }
        if sorted_to_fi.len() == nf {
            let mut rows = Vec::with_capacity(n);
            for i in 0..n {
                rows.push(crate::store::Row::from_parts(
                    Arc::clone(&shape),
                    sorted_to_fi.iter().map(|&fi| col_cell_at(cols, fi, i)),
                ));
            }
            return rows;
        }
    }
    let mut rows = Vec::with_capacity(n);
    for i in 0..n {
        let mut row = crate::store::Row::new();
        for (fi, f) in fields.iter().enumerate() {
            row.insert(f.as_str(), col_cell_at(cols, fi, i));
        }
        rows.push(row);
    }
    rows
}

