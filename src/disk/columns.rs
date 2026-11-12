use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;
use std::sync::{Arc, Mutex};

use memmap2::{Mmap, MmapOptions};

use super::DiskColKind;
use crate::embed::{cosine_with_q_norm, dot_sq8, l2_norm_f64, quantize_sq8};
use crate::error::Error;
use crate::persist::io_err;
use crate::store::{Cell, InlineStr, Row};

/// Target uncompressed payload per Zstd block for variable-length text columns (32 KiB).
pub(super) const TEXT_BLOCK_TARGET_BYTES: usize = 32 * 1024;
/// Number of lock-sharded cache stripes per text column.
const BLOCK_CACHE_SHARDS: usize = 16;
/// Number of decoded text blocks per shard (`16 * 128 = 2,048` blocks max = ~64 MiB cap per active column).
const BLOCK_CACHE_SLOTS_PER_SHARD: usize = 128;

pub(super) const DICT_NULL_CODE: u16 = 0xFFFF;
pub(super) const DICT_MAX_DISTINCT: usize = 65_534;
pub(super) const DICT_MAX_BYTES: usize = 4 * 1024 * 1024;
pub(super) const DICT_MIN_SEGMENT_ROWS: usize = 32;
pub(super) const IVF_MIN_SEGMENT_VECS: usize = 4_096;

struct BlockCacheSlot {
    block_idx: u32,
    data: Arc<[u8]>,
}

pub(super) struct MmapTextCol {
    /// `row_count` entries of `(block_idx: u32, in_block_offset: u32, byte_len: u32, is_null: u32)` = 16 bytes/row.
    row_loc: Mmap,
    /// `block_count + 1` entries of `u64` byte offsets into `blocks_data`.
    block_offsets: Mmap,
    /// Concatenated Zstd-compressed blocks.
    blocks_data: Mmap,
    row_count: u32,
    block_count: u32,
    cache_shards: [Mutex<Vec<Option<BlockCacheSlot>>>; BLOCK_CACHE_SHARDS],
}

impl MmapTextCol {
    pub(super) fn open(dir: &Path, col_name: &str, row_count: u32) -> Result<Self, Error> {
        let loc_f = File::open(dir.join(format!("col_{col_name}.loc"))).map_err(io_err)?;
        let boff_f = File::open(dir.join(format!("col_{col_name}.boff"))).map_err(io_err)?;
        let bdat_f = File::open(dir.join(format!("col_{col_name}.bdat"))).map_err(io_err)?;

        let row_loc = unsafe { MmapOptions::new().map(&loc_f) }.map_err(io_err)?;
        let block_offsets = unsafe { MmapOptions::new().map(&boff_f) }.map_err(io_err)?;
        let blocks_data = unsafe { MmapOptions::new().map(&bdat_f) }.map_err(io_err)?;

        let block_count = if block_offsets.len() >= 8 {
            ((block_offsets.len() / 8) - 1) as u32
        } else {
            0
        };

        let cache_shards = std::array::from_fn(|_| {
            let mut slots = Vec::with_capacity(BLOCK_CACHE_SLOTS_PER_SHARD);
            slots.resize_with(BLOCK_CACHE_SLOTS_PER_SHARD, || None);
            Mutex::new(slots)
        });

        Ok(Self {
            row_loc,
            block_offsets,
            blocks_data,
            row_count,
            block_count,
            cache_shards,
        })
    }

    #[inline]
    pub(super) fn get_loc(&self, row_id: u32) -> Option<(u32, u32, u32)> {
        if row_id >= self.row_count {
            return None;
        }
        let base = (row_id as usize) * 16;
        if base + 16 > self.row_loc.len() {
            return None;
        }
        let flags = u32::from_le_bytes(self.row_loc[base + 12..base + 16].try_into().ok()?);
        if flags != 0 {
            return None;
        }
        let block_idx = u32::from_le_bytes(self.row_loc[base..base + 4].try_into().ok()?);
        let in_off = u32::from_le_bytes(self.row_loc[base + 4..base + 8].try_into().ok()?);
        let len = u32::from_le_bytes(self.row_loc[base + 8..base + 12].try_into().ok()?);
        Some((block_idx, in_off, len))
    }

    pub(super) fn decode_block(&self, block_idx: u32) -> Result<Arc<[u8]>, Error> {
        let shard_idx = (block_idx as usize) & (BLOCK_CACHE_SHARDS - 1);
        let slot_idx = ((block_idx as usize) >> 4) & (BLOCK_CACHE_SLOTS_PER_SHARD - 1);
        if let Ok(guard) = self.cache_shards[shard_idx].lock()
            && let Some(ref entry) = guard[slot_idx]
            && entry.block_idx == block_idx
        {
            return Ok(Arc::clone(&entry.data));
        }

        if block_idx >= self.block_count {
            return Err(io_err("text block index out of range"));
        }
        let off_base = (block_idx as usize) * 8;
        let start = u64::from_le_bytes(
            self.block_offsets[off_base..off_base + 8]
                .try_into()
                .map_err(|_| io_err("corrupt boff"))?,
        ) as usize;
        let end = u64::from_le_bytes(
            self.block_offsets[off_base + 8..off_base + 16]
                .try_into()
                .map_err(|_| io_err("corrupt boff"))?,
        ) as usize;
        if end > self.blocks_data.len() || start > end {
            return Err(io_err("corrupt text block bounds"));
        }
        let slice = &self.blocks_data[start..end];
        let raw: Vec<u8> = if slice.is_empty() {
            Vec::new()
        } else if slice[0] == 0x00 {
            slice[1..].to_vec()
        } else {
            #[cfg(feature = "compress-zstd")]
            {
                thread_local! {
                    static ZSTD_DEC: std::cell::RefCell<Option<zstd::bulk::Decompressor<'static>>> =
                        const { std::cell::RefCell::new(None) };
                }
                ZSTD_DEC.with(|cell| {
                    let mut opt = cell.borrow_mut();
                    if opt.is_none() {
                        *opt = zstd::bulk::Decompressor::new().ok();
                    }
                    if let Some(dec) = opt.as_mut()
                        && let Ok(v) = dec.decompress(&slice[1..], TEXT_BLOCK_TARGET_BYTES + 4096)
                    {
                        return Ok(v);
                    }
                    zstd::decode_all(&slice[1..]).map_err(io_err)
                })?
            }
            #[cfg(not(feature = "compress-zstd"))]
            {
                return Err(io_err(
                    "zstd feature required to read compressed text block",
                ));
            }
        };
        let arc: Arc<[u8]> = Arc::from(raw);
        if let Ok(mut guard) = self.cache_shards[shard_idx].lock() {
            guard[slot_idx] = Some(BlockCacheSlot {
                block_idx,
                data: Arc::clone(&arc),
            });
        }
        Ok(arc)
    }

    pub(super) fn get_cell(&self, row_id: u32) -> Result<Cell, Error> {
        let Some((block_idx, in_off, len)) = self.get_loc(row_id) else {
            return Ok(Cell::Null);
        };
        if len == 0 {
            return Ok(Cell::SmallText(InlineStr::new_truncate("")));
        }
        let block = self.decode_block(block_idx)?;
        let s_off = in_off as usize;
        let e_off = s_off + (len as usize);
        if e_off > block.len() {
            return Err(io_err("corrupt in-block text slice"));
        }
        let s = std::str::from_utf8(&block[s_off..e_off]).map_err(io_err)?;
        Ok(Cell::from_text(s))
    }

    pub(super) fn with_str<R>(
        &self,
        row_id: u32,
        f: impl FnOnce(Option<&str>) -> R,
    ) -> Result<R, Error> {
        let Some((block_idx, in_off, len)) = self.get_loc(row_id) else {
            return Ok(f(None));
        };
        if len == 0 {
            return Ok(f(Some("")));
        }
        let block = self.decode_block(block_idx)?;
        let s_off = in_off as usize;
        let e_off = s_off + (len as usize);
        if e_off > block.len() {
            return Err(io_err("corrupt in-block text slice"));
        }
        let s = std::str::from_utf8(&block[s_off..e_off]).map_err(io_err)?;
        Ok(f(Some(s)))
    }
}

/// Memory-mapped Dictionary-Encoded UTF-8 string column (`col_<name>.dstr`, `.doff`, `.dord`).
pub(super) struct MmapDictCol {
    pub(super) strings: Vec<Box<str>>,
    cells: Vec<Cell>,
    ordinals: Mmap,
    row_count: u32,
}

impl MmapDictCol {
    pub(super) fn open(dir: &Path, col_name: &str, row_count: u32) -> Result<Self, Error> {
        let dstr_f = File::open(dir.join(format!("col_{col_name}.dstr"))).map_err(io_err)?;
        let doff_f = File::open(dir.join(format!("col_{col_name}.doff"))).map_err(io_err)?;
        let dord_f = File::open(dir.join(format!("col_{col_name}.dord"))).map_err(io_err)?;

        let dstr = unsafe { MmapOptions::new().map(&dstr_f) }.map_err(io_err)?;
        let doff = unsafe { MmapOptions::new().map(&doff_f) }.map_err(io_err)?;
        let ordinals = unsafe { MmapOptions::new().map(&dord_f) }.map_err(io_err)?;

        let dict_len = if doff.len() >= 4 {
            (doff.len() / 4).saturating_sub(1)
        } else {
            0
        };
        let mut strings = Vec::with_capacity(dict_len);
        let mut cells = Vec::with_capacity(dict_len);
        for i in 0..dict_len {
            let base = i * 4;
            let s_off = u32::from_le_bytes(
                doff[base..base + 4]
                    .try_into()
                    .map_err(|_| io_err("corrupt doff"))?,
            ) as usize;
            let e_off = u32::from_le_bytes(
                doff[base + 4..base + 8]
                    .try_into()
                    .map_err(|_| io_err("corrupt doff"))?,
            ) as usize;
            if e_off > dstr.len() || s_off > e_off {
                return Err(io_err("corrupt dstr bounds"));
            }
            let s = std::str::from_utf8(&dstr[s_off..e_off]).map_err(io_err)?;
            strings.push(s.to_string().into_boxed_str());
            cells.push(Cell::from_text(s));
        }

        Ok(Self {
            strings,
            cells,
            ordinals,
            row_count,
        })
    }

    #[inline]
    pub(super) fn code_at(&self, row_id: u32) -> u16 {
        if row_id >= self.row_count {
            return DICT_NULL_CODE;
        }
        let off = (row_id as usize) * 2;
        if off + 2 > self.ordinals.len() {
            return DICT_NULL_CODE;
        }
        u16::from_le_bytes([self.ordinals[off], self.ordinals[off + 1]])
    }

    #[inline]
    pub(super) fn get_str(&self, row_id: u32) -> Option<&str> {
        let code = self.code_at(row_id);
        if code == DICT_NULL_CODE {
            None
        } else {
            self.strings.get(code as usize).map(|b| b.as_ref())
        }
    }

    #[inline]
    pub(super) fn get_cell(&self, row_id: u32) -> Cell {
        let code = self.code_at(row_id);
        if code == DICT_NULL_CODE {
            Cell::Null
        } else {
            self.cells.get(code as usize).cloned().unwrap_or(Cell::Null)
        }
    }
}

pub(super) struct MmapFixedCol {
    kind: DiskColKind,
    nulls: Mmap,
    values: Mmap,
    row_count: u32,
}

impl MmapFixedCol {
    pub(super) fn open(
        dir: &Path,
        col_name: &str,
        kind: DiskColKind,
        row_count: u32,
    ) -> Result<Self, Error> {
        let nf = File::open(dir.join(format!("col_{col_name}.null"))).map_err(io_err)?;
        let vf = File::open(dir.join(format!("col_{col_name}.val"))).map_err(io_err)?;
        let nulls = unsafe { MmapOptions::new().map(&nf) }.map_err(io_err)?;
        let values = unsafe { MmapOptions::new().map(&vf) }.map_err(io_err)?;
        Ok(Self {
            kind,
            nulls,
            values,
            row_count,
        })
    }

    #[inline]
    pub(super) fn get_cell(&self, row_id: u32) -> Cell {
        let idx = row_id as usize;
        if row_id >= self.row_count || idx >= self.nulls.len() || self.nulls[idx] != 0 {
            return Cell::Null;
        }
        match self.kind {
            DiskColKind::Int64 => {
                let off = idx * 8;
                if off + 8 > self.values.len() {
                    return Cell::Null;
                }
                let v = i64::from_le_bytes(self.values[off..off + 8].try_into().unwrap_or([0; 8]));
                Cell::Int(v)
            }
            DiskColKind::Time64 => {
                let off = idx * 8;
                if off + 8 > self.values.len() {
                    return Cell::Null;
                }
                let v = i64::from_le_bytes(self.values[off..off + 8].try_into().unwrap_or([0; 8]));
                Cell::Time(v)
            }
            DiskColKind::Float64 => {
                let off = idx * 8;
                if off + 8 > self.values.len() {
                    return Cell::Null;
                }
                let v = f64::from_le_bytes(self.values[off..off + 8].try_into().unwrap_or([0; 8]));
                Cell::Float(v)
            }
            DiskColKind::Bool => Cell::Bool(self.values.get(idx).copied().unwrap_or(0) != 0),
            _ => Cell::Null,
        }
    }
}

/// Compact Coarse IVF-SQ8 inverted file stored alongside `col_<name>.vsq8` (`col_<name>.vivf`).
pub struct MmapVecIvf {
    pub k: usize,
    pub dim: usize,
    pub live_vecs: u32,
    mmap: Mmap,
    centroids_off: usize,
    cluster_offsets_off: usize,
    cluster_rids_off: usize,
    cluster_sq8_off: usize,
}

impl MmapVecIvf {
    pub(super) fn open_optional(dir: &Path, col_name: &str, expected_dim: usize) -> Option<Self> {
        let path = dir.join(format!("col_{col_name}.vivf"));
        let f = File::open(path).ok()?;
        let mmap = unsafe { MmapOptions::new().map(&f) }.ok()?;
        if mmap.len() < 64 || &mmap[0..8] != b"LIVF2\0\0\0" {
            return None;
        }
        let k = u32::from_le_bytes(mmap[8..12].try_into().ok()?) as usize;
        let dim = u32::from_le_bytes(mmap[12..16].try_into().ok()?) as usize;
        let live_vecs = u32::from_le_bytes(mmap[16..20].try_into().ok()?);
        if k == 0 || dim == 0 || dim != expected_dim {
            return None;
        }
        let centroids_off = u64::from_le_bytes(mmap[24..32].try_into().ok()?) as usize;
        let cluster_offsets_off = u64::from_le_bytes(mmap[32..40].try_into().ok()?) as usize;
        let cluster_rids_off = u64::from_le_bytes(mmap[40..48].try_into().ok()?) as usize;
        let cluster_sq8_off = u64::from_le_bytes(mmap[48..56].try_into().ok()?) as usize;
        let lv = live_vecs as usize;
        if centroids_off.checked_add(k.checked_mul(dim)?.checked_mul(4)?)? > mmap.len()
            || cluster_offsets_off.checked_add((k + 1).checked_mul(4)?)? > mmap.len()
            || cluster_rids_off.checked_add(lv.checked_mul(4)?)? > mmap.len()
            || cluster_sq8_off.checked_add(lv.checked_mul(dim)?)? > mmap.len()
        {
            return None;
        }
        Some(Self {
            k,
            dim,
            live_vecs,
            mmap,
            centroids_off,
            cluster_offsets_off,
            cluster_rids_off,
            cluster_sq8_off,
        })
    }

    /// Rank the top `nprobe` closest coarse centroids to `query_vec` (returns sorted centroid indices).
    pub fn top_centroids(&self, query_vec: &[f32], q_norm: f64, nprobe: usize) -> Vec<usize> {
        let take_n = nprobe.min(self.k).max(1);
        let mut scored: Vec<(f32, usize)> = Vec::with_capacity(self.k);
        for c in 0..self.k {
            let off = self.centroids_off + c * self.dim * 4;
            let sim = {
                #[cfg(target_endian = "little")]
                {
                    let ptr = unsafe { self.mmap.as_ptr().add(off) };
                    if (ptr as usize).is_multiple_of(std::mem::align_of::<f32>()) {
                        let c_slice =
                            unsafe { std::slice::from_raw_parts(ptr as *const f32, self.dim) };
                        cosine_with_q_norm(query_vec, q_norm, c_slice) as f32
                    } else {
                        let mut dot = 0.0f64;
                        let mut cn = 0.0f64;
                        for j in 0..self.dim {
                            let v = f32::from_le_bytes(
                                self.mmap[off + j * 4..off + j * 4 + 4]
                                    .try_into()
                                    .unwrap_or([0; 4]),
                            ) as f64;
                            dot += (query_vec[j] as f64) * v;
                            cn += v * v;
                        }
                        if cn > 1e-18 {
                            (dot / (q_norm * cn.sqrt())) as f32
                        } else {
                            0.0
                        }
                    }
                }
                #[cfg(not(target_endian = "little"))]
                {
                    let mut dot = 0.0f64;
                    let mut cn = 0.0f64;
                    for j in 0..self.dim {
                        let v = f32::from_le_bytes(
                            self.mmap[off + j * 4..off + j * 4 + 4]
                                .try_into()
                                .unwrap_or([0; 4]),
                        ) as f64;
                        dot += (query_vec[j] as f64) * v;
                        cn += v * v;
                    }
                    if cn > 1e-18 {
                        (dot / (q_norm * cn.sqrt())) as f32
                    } else {
                        0.0
                    }
                }
            };
            scored.push((sim, c));
        }
        scored.sort_unstable_by(|a, b| b.0.total_cmp(&a.0));
        scored.truncate(take_n);
        let mut out: Vec<usize> = scored.into_iter().map(|(_, c)| c).collect();
        out.sort_unstable();
        out
    }

    #[inline]
    pub(super) fn cluster_bounds(&self, cluster_idx: usize) -> (usize, usize) {
        let base = self.cluster_offsets_off + cluster_idx * 4;
        let s = u32::from_le_bytes(self.mmap[base..base + 4].try_into().unwrap_or([0; 4])) as usize;
        let e =
            u32::from_le_bytes(self.mmap[base + 4..base + 8].try_into().unwrap_or([0; 4])) as usize;
        (s, e.min(self.live_vecs as usize))
    }

    #[inline]
    pub(super) fn item_rid_and_sq8(&self, item_idx: usize) -> (u32, &[i8]) {
        let rid_off = self.cluster_rids_off + item_idx * 4;
        let rid = u32::from_le_bytes(self.mmap[rid_off..rid_off + 4].try_into().unwrap_or([0; 4]));
        let sq8_off = self.cluster_sq8_off + item_idx * self.dim;
        let ptr = unsafe { self.mmap.as_ptr().add(sq8_off) as *const i8 };
        let sq8 = unsafe { std::slice::from_raw_parts(ptr, self.dim) };
        (rid, sq8)
    }
}

pub(super) fn build_vec_ivf_file(
    seg_dir: &Path,
    col_name: &str,
    dim: usize,
    rows: &[Row],
) -> Result<(), Error> {
    if dim == 0 {
        return Ok(());
    }
    let mut live_rids: Vec<u32> = Vec::with_capacity(rows.len());
    for (i, r) in rows.iter().enumerate() {
        if let Some(Cell::Vec(v)) = r.get(col_name)
            && v.len() == dim
        {
            live_rids.push(i as u32);
        }
    }
    if live_rids.len() < IVF_MIN_SEGMENT_VECS {
        return Ok(());
    }

    let n = live_rids.len();
    let k = (n as f64).sqrt().round().clamp(16.0, 256.0) as usize;

    let mut centroids = vec![0.0f32; k * dim];
    for c in 0..k {
        let src_rid = live_rids[(c * n) / k] as usize;
        if let Some(Cell::Vec(v)) = rows[src_rid].get(col_name) {
            let norm = (l2_norm_f64(v) as f32).max(1e-12);
            let dst = &mut centroids[c * dim..(c + 1) * dim];
            for (j, &val) in v.iter().enumerate() {
                dst[j] = val / norm;
            }
        }
    }

    let train_step = (n / 16_384).max(1);
    let train_rids: Vec<u32> = live_rids.iter().copied().step_by(train_step).collect();

    let mut c_sq8: Vec<Arc<[i8]>> = Vec::with_capacity(k);
    for _iter in 0..2 {
        c_sq8.clear();
        for c in 0..k {
            c_sq8.push(quantize_sq8(&centroids[c * dim..(c + 1) * dim]));
        }

        let assign_fn = |&rid: &u32| -> usize {
            let Some(Cell::Vec(v)) = rows[rid as usize].get(col_name) else {
                return 0;
            };
            let vq = quantize_sq8(v);
            let mut best_c = 0usize;
            let mut best_d = f64::NEG_INFINITY;
            for (c, cq) in c_sq8.iter().enumerate() {
                let d = dot_sq8(&vq, cq);
                if d > best_d {
                    best_d = d;
                    best_c = c;
                }
            }
            best_c
        };

        #[cfg(feature = "parallel")]
        let train_assign: Vec<usize> = {
            use rayon::prelude::*;
            train_rids.par_iter().map(assign_fn).collect()
        };
        #[cfg(not(feature = "parallel"))]
        let train_assign: Vec<usize> = train_rids.iter().map(assign_fn).collect();

        let mut sums = vec![0.0f64; k * dim];
        let mut counts = vec![0usize; k];
        for (&rid, &cid) in train_rids.iter().zip(train_assign.iter()) {
            if let Some(Cell::Vec(v)) = rows[rid as usize].get(col_name) {
                let vnorm = l2_norm_f64(v).max(1e-12);
                let inv = 1.0 / vnorm;
                let dst = &mut sums[cid * dim..(cid + 1) * dim];
                for (j, &val) in v.iter().enumerate() {
                    dst[j] += (val as f64) * inv;
                }
                counts[cid] += 1;
            }
        }
        for c in 0..k {
            if counts[c] == 0 {
                continue;
            }
            let src = &sums[c * dim..(c + 1) * dim];
            let mut sq = 0.0f64;
            for &x in src {
                sq += x * x;
            }
            let norm = sq.sqrt().max(1e-12);
            let dst = &mut centroids[c * dim..(c + 1) * dim];
            for j in 0..dim {
                dst[j] = (src[j] / norm) as f32;
            }
        }
    }

    c_sq8.clear();
    for c in 0..k {
        c_sq8.push(quantize_sq8(&centroids[c * dim..(c + 1) * dim]));
    }

    let assign_all_fn = |&rid: &u32| -> (usize, u32, Arc<[i8]>) {
        let v = match rows[rid as usize].get(col_name) {
            Some(Cell::Vec(v)) => v.as_ref(),
            _ => &[][..],
        };
        let vq = quantize_sq8(v);
        let mut best_c = 0usize;
        let mut best_d = f64::NEG_INFINITY;
        for (c, cq) in c_sq8.iter().enumerate() {
            let d = dot_sq8(&vq, cq);
            if d > best_d {
                best_d = d;
                best_c = c;
            }
        }
        (best_c, rid, vq)
    };

    #[cfg(feature = "parallel")]
    let mut assigned: Vec<(usize, u32, Arc<[i8]>)> = {
        use rayon::prelude::*;
        live_rids.par_iter().map(assign_all_fn).collect()
    };
    #[cfg(not(feature = "parallel"))]
    let mut assigned: Vec<(usize, u32, Arc<[i8]>)> = live_rids.iter().map(assign_all_fn).collect();

    assigned.sort_unstable_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));

    let mut cluster_counts = vec![0u32; k];
    for &(cid, _, _) in &assigned {
        cluster_counts[cid] += 1;
    }
    let mut cluster_offsets = Vec::with_capacity(k + 1);
    let mut acc = 0u32;
    cluster_offsets.push(0u32);
    for &cnt in &cluster_counts {
        acc += cnt;
        cluster_offsets.push(acc);
    }

    let align64 = |x: u64| (x + 63) & !63;
    let centroids_off = 64u64;
    let centroids_bytes = (k * dim * 4) as u64;
    let cluster_offsets_off = align64(centroids_off + centroids_bytes);
    let cluster_offsets_bytes = ((k + 1) * 4) as u64;
    let cluster_rids_off = align64(cluster_offsets_off + cluster_offsets_bytes);
    let cluster_rids_bytes = (n * 4) as u64;
    let cluster_sq8_off = align64(cluster_rids_off + cluster_rids_bytes);

    let ivf_path = seg_dir.join(format!("col_{col_name}.vivf"));
    let mut w = BufWriter::with_capacity(256 * 1024, File::create(&ivf_path).map_err(io_err)?);
    let mut hdr = [0u8; 64];
    hdr[0..8].copy_from_slice(b"LIVF2\0\0\0");
    hdr[8..12].copy_from_slice(&(k as u32).to_le_bytes());
    hdr[12..16].copy_from_slice(&(dim as u32).to_le_bytes());
    hdr[16..20].copy_from_slice(&(n as u32).to_le_bytes());
    hdr[24..32].copy_from_slice(&centroids_off.to_le_bytes());
    hdr[32..40].copy_from_slice(&cluster_offsets_off.to_le_bytes());
    hdr[40..48].copy_from_slice(&cluster_rids_off.to_le_bytes());
    hdr[48..56].copy_from_slice(&cluster_sq8_off.to_le_bytes());
    w.write_all(&hdr).map_err(io_err)?;

    for &val in &centroids {
        w.write_all(&val.to_le_bytes()).map_err(io_err)?;
    }
    let pad1 = (cluster_offsets_off - (centroids_off + centroids_bytes)) as usize;
    if pad1 > 0 {
        w.write_all(&vec![0u8; pad1]).map_err(io_err)?;
    }
    for &off in &cluster_offsets {
        w.write_all(&off.to_le_bytes()).map_err(io_err)?;
    }
    let pad2 = (cluster_rids_off - (cluster_offsets_off + cluster_offsets_bytes)) as usize;
    if pad2 > 0 {
        w.write_all(&vec![0u8; pad2]).map_err(io_err)?;
    }
    for &(_, rid, _) in &assigned {
        w.write_all(&rid.to_le_bytes()).map_err(io_err)?;
    }
    let pad3 = (cluster_sq8_off - (cluster_rids_off + cluster_rids_bytes)) as usize;
    if pad3 > 0 {
        w.write_all(&vec![0u8; pad3]).map_err(io_err)?;
    }
    for (_, _, sq8) in &assigned {
        let sq8_u8 = unsafe { std::slice::from_raw_parts(sq8.as_ptr() as *const u8, sq8.len()) };
        w.write_all(sq8_u8).map_err(io_err)?;
    }
    w.flush().map_err(io_err)?;
    Ok(())
}

pub struct MmapVecCol {
    pub dim: usize,
    pub row_count: u32,
    nulls: Mmap,
    f32_mmap: Mmap,
    sq8_mmap: Mmap,
    pub ivf: Option<MmapVecIvf>,
}

impl MmapVecCol {
    pub(super) fn open(dir: &Path, col_name: &str, dim: u32, row_count: u32) -> Result<Self, Error> {
        let nf = File::open(dir.join(format!("col_{col_name}.vnull"))).map_err(io_err)?;
        let ff = File::open(dir.join(format!("col_{col_name}.vf32"))).map_err(io_err)?;
        let qf = File::open(dir.join(format!("col_{col_name}.vsq8"))).map_err(io_err)?;
        let nulls = unsafe { MmapOptions::new().map(&nf) }.map_err(io_err)?;
        let f32_mmap = unsafe { MmapOptions::new().map(&ff) }.map_err(io_err)?;
        let sq8_mmap = unsafe { MmapOptions::new().map(&qf) }.map_err(io_err)?;
        let ivf = MmapVecIvf::open_optional(dir, col_name, dim as usize);
        Ok(Self {
            dim: dim as usize,
            row_count,
            nulls,
            f32_mmap,
            sq8_mmap,
            ivf,
        })
    }

    #[inline]
    pub fn is_present(&self, row_id: u32) -> bool {
        let idx = row_id as usize;
        row_id < self.row_count && idx < self.nulls.len() && self.nulls[idx] == 0
    }

    #[inline]
    pub fn sq8_slice(&self, row_id: u32) -> Option<&[i8]> {
        if !self.is_present(row_id) || self.dim == 0 {
            return None;
        }
        let start = (row_id as usize).checked_mul(self.dim)?;
        let end = start.checked_add(self.dim)?;
        if end > self.sq8_mmap.len() {
            return None;
        }
        let ptr = unsafe { self.sq8_mmap.as_ptr().add(start) as *const i8 };
        Some(unsafe { std::slice::from_raw_parts(ptr, self.dim) })
    }

    #[inline]
    pub fn f32_slice(&self, row_id: u32) -> Option<&[f32]> {
        if !self.is_present(row_id) || self.dim == 0 {
            return None;
        }
        let stride = self.dim.checked_mul(4)?;
        let start_bytes = (row_id as usize).checked_mul(stride)?;
        let end_bytes = start_bytes.checked_add(stride)?;
        if end_bytes > self.f32_mmap.len() {
            return None;
        }
        #[cfg(target_endian = "little")]
        {
            let ptr = unsafe { self.f32_mmap.as_ptr().add(start_bytes) };
            if (ptr as usize).is_multiple_of(std::mem::align_of::<f32>()) {
                return Some(unsafe { std::slice::from_raw_parts(ptr as *const f32, self.dim) });
            }
        }
        None
    }

    pub fn get_cell(&self, row_id: u32) -> Cell {
        if let Some(slice) = self.f32_slice(row_id) {
            return Cell::Vec(Arc::from(slice));
        }
        if !self.is_present(row_id) || self.dim == 0 {
            return Cell::Null;
        }
        let start_bytes = (row_id as usize) * self.dim * 4;
        if start_bytes.saturating_add(self.dim * 4) > self.f32_mmap.len() {
            return Cell::Null;
        }
        let mut out = Vec::with_capacity(self.dim);
        for i in 0..self.dim {
            let off = start_bytes + i * 4;
            let val = f32::from_le_bytes(self.f32_mmap[off..off + 4].try_into().unwrap_or([0; 4]));
            out.push(val);
        }
        Cell::Vec(Arc::from(out))
    }
}

pub(super) enum MmapCol {
    Fixed(MmapFixedCol),
    Text(MmapTextCol),
    Dict(MmapDictCol),
    Vec(MmapVecCol),
}

impl MmapCol {
    #[inline]
    pub(super) fn get_cell(&self, row_id: u32) -> Result<Cell, Error> {
        match self {
            MmapCol::Fixed(c) => Ok(c.get_cell(row_id)),
            MmapCol::Text(c) => c.get_cell(row_id),
            MmapCol::Dict(c) => Ok(c.get_cell(row_id)),
            MmapCol::Vec(c) => Ok(c.get_cell(row_id)),
        }
    }
}
