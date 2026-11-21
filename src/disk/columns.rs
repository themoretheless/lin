use std::borrow::Cow;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use memchr::memmem::Finder;
use memmap2::{Mmap, MmapOptions};

use super::DiskColKind;
use crate::embed::{cosine_with_q_norm, dot_sq8, l2_norm_f64, quantize_sq8_into};
use crate::error::Error;
use crate::persist::io_err;
use crate::store::{Cell, InlineStr};

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

struct MmapTgram {
    mmap: Mmap,
    block_count: u32,
}

pub(crate) struct MmapTextCol {
    /// `row_count` entries of `(block_idx: u32, in_block_offset: u32, byte_len: u32, is_null: u32)` = 16 bytes/row.
    row_loc: Mmap,
    /// `block_count + 1` entries of `u64` byte offsets into `blocks_data`.
    block_offsets: Mmap,
    /// Concatenated Zstd-compressed blocks.
    blocks_data: Mmap,
    row_count: u32,
    pub(super) block_count: u32,
    cache_shards: [Mutex<Vec<Option<BlockCacheSlot>>>; BLOCK_CACHE_SHARDS],
    /// Per-block UTF-8 validation cache for raw and decoded blocks: `0` = unchecked, `1` = valid UTF-8, `2` = invalid.
    utf8_state: Box<[AtomicU8]>,
    /// Optional memory-mapped per-Zstd-block 4096-bit trigram Bloom filters (`col_<name>.tgram`).
    tgram: Option<MmapTgram>,
    /// Number of cache-miss block decodes performed on this column instance.
    decoded_blocks: AtomicU64,
}

impl MmapTextCol {
    #[allow(dead_code)]
    pub(super) fn open(dir: &Path, col_name: &str, row_count: u32) -> Result<Self, Error> {
        Self::open_with_opts(dir, col_name, row_count, false)
    }

    pub(super) fn open_with_opts(
        dir: &Path,
        col_name: &str,
        row_count: u32,
        skip_tgram_body_crc: bool,
    ) -> Result<Self, Error> {
        let loc_f = File::open(dir.join(format!("col_{col_name}.loc"))).map_err(io_err)?;
        let boff_f = File::open(dir.join(format!("col_{col_name}.boff"))).map_err(io_err)?;
        let bdat_f = File::open(dir.join(format!("col_{col_name}.bdat"))).map_err(io_err)?;

        let row_loc = unsafe { MmapOptions::new().map(&loc_f) }.map_err(io_err)?;
        let block_offsets = unsafe { MmapOptions::new().map(&boff_f) }.map_err(io_err)?;
        let blocks_data = unsafe { MmapOptions::new().map(&bdat_f) }.map_err(io_err)?;

        // A short file would otherwise read as missing values for the rows past its end.
        let rows = row_count as usize;
        if row_loc.len() < rows.saturating_mul(16) {
            return Err(io_err(format!("corrupt cold column {col_name}: row index shorter than {rows} rows")));
        }
        if rows > 0 && (block_offsets.len() < 8 || block_offsets.len() % 8 != 0) {
            return Err(io_err(format!("corrupt cold column {col_name}: bad block offsets")));
        }
        if let Some(last) = block_offsets.len().checked_sub(8)
            && block_offsets.len() >= 8
        {
            let end = u64::from_le_bytes(block_offsets[last..last + 8].try_into().unwrap_or([0; 8]));
            if end > blocks_data.len() as u64 {
                return Err(io_err(format!("corrupt cold column {col_name}: block data shorter than its offsets")));
            }
        }
        let block_count = if block_offsets.len() >= 8 {
            ((block_offsets.len() / 8) - 1) as u32
        } else {
            0
        };
        // Rows fill blocks in order, so the last stored row names the highest block; it
        // must exist (a truncated offsets file would otherwise read as missing values).
        for row in (0..rows).rev() {
            let base = row * 16;
            let flags = u32::from_le_bytes(row_loc[base + 12..base + 16].try_into().unwrap_or([1; 4]));
            if flags != 0 {
                continue;
            }
            let block = u32::from_le_bytes(row_loc[base..base + 4].try_into().unwrap_or([0; 4]));
            let len = u32::from_le_bytes(row_loc[base + 8..base + 12].try_into().unwrap_or([0; 4]));
            if len > 0 && block >= block_count {
                return Err(io_err(format!(
                    "corrupt cold column {col_name}: row {row} refers to block {block} of {block_count}"
                )));
            }
            if len > 0 {
                break;
            }
        }

        let cache_shards = std::array::from_fn(|_| Mutex::new(Vec::new()));
        let utf8_state = (0..block_count as usize)
            .map(|_| AtomicU8::new(0))
            .collect::<Vec<_>>()
            .into_boxed_slice();

        let tgram = open_tgram_mmap(
            &dir.join(format!("col_{col_name}.tgram")),
            block_count,
            skip_tgram_body_crc,
        );

        Ok(Self {
            row_loc,
            block_offsets,
            blocks_data,
            row_count,
            block_count,
            cache_shards,
            utf8_state,
            tgram,
            decoded_blocks: AtomicU64::new(0),
        })
    }

    pub(super) fn clear_tgram(&mut self) {
        self.tgram = None;
    }

    #[inline]
    pub(super) fn row_loc_bytes(&self) -> &[u8] {
        &self.row_loc
    }

    #[inline]
    pub(super) fn block_offsets_bytes(&self) -> &[u8] {
        &self.block_offsets
    }

    #[inline]
    pub(super) fn blocks_data_bytes(&self) -> &[u8] {
        &self.blocks_data
    }

    #[inline]
    pub(super) fn tgram_bytes(&self) -> Option<&[u8]> {
        self.tgram.as_ref().map(|t| &t.mmap[..])
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

    #[inline]
    fn block_slice(&self, block_idx: u32) -> Result<&[u8], Error> {
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
        Ok(&self.blocks_data[start..end])
    }

    #[inline]
    pub(crate) fn raw_uncompressed_block(&self, block_idx: u32) -> Result<Option<&[u8]>, Error> {
        let slice = self.block_slice(block_idx)?;
        if slice.is_empty() {
            Ok(Some(&[]))
        } else if slice[0] == 0x00 {
            Ok(Some(&slice[1..]))
        } else {
            Ok(None)
        }
    }

    #[inline]
    pub(super) fn block_is_utf8_cached(&self, block_idx: u32, bytes: &[u8]) -> bool {
        self.is_block_utf8_ok(block_idx, bytes)
    }

    #[inline]
    fn is_block_utf8_ok(&self, block_idx: u32, bytes: &[u8]) -> bool {
        let Some(state) = self.utf8_state.get(block_idx as usize) else {
            return std::str::from_utf8(bytes).is_ok();
        };
        let st = state.load(Ordering::Relaxed);
        if st == 0 {
            let valid = std::str::from_utf8(bytes).is_ok();
            state.store(if valid { 1 } else { 2 }, Ordering::Relaxed);
            valid
        } else {
            st == 1
        }
    }

    #[inline]
    pub(crate) fn raw_uncompressed_block_checked(
        &self,
        block_idx: u32,
    ) -> Result<Option<(&[u8], bool)>, Error> {
        let Some(raw) = self.raw_uncompressed_block(block_idx)? else {
            return Ok(None);
        };
        let ok = self.is_block_utf8_ok(block_idx, raw);
        Ok(Some((raw, ok)))
    }

    #[inline]
    pub(crate) fn decode_block_checked(
        &self,
        block_idx: u32,
    ) -> Result<(Arc<[u8]>, bool), Error> {
        let bytes = self.decode_block(block_idx)?;
        let ok = self.is_block_utf8_ok(block_idx, &bytes);
        Ok((bytes, ok))
    }

    #[inline]
    pub(crate) fn slice_block_str(
        raw: &[u8],
        block_utf8_ok: bool,
        s_off: usize,
        e_off: usize,
    ) -> Result<&str, Error> {
        if s_off > e_off || e_off > raw.len() {
            return Err(io_err("corrupt in-block text slice"));
        }
        if block_utf8_ok
            && (s_off == raw.len() || (raw[s_off] & 0xC0) != 0x80)
            && (e_off == raw.len() || (raw[e_off] & 0xC0) != 0x80)
        {
            // SAFETY: `raw` is validated UTF-8 and neither boundary falls on a UTF-8 continuation byte.
            return Ok(unsafe { std::str::from_utf8_unchecked(&raw[s_off..e_off]) });
        }
        std::str::from_utf8(&raw[s_off..e_off]).map_err(io_err)
    }

    pub(super) fn decode_block(&self, block_idx: u32) -> Result<Arc<[u8]>, Error> {
        let shard_idx = (block_idx as usize) & (BLOCK_CACHE_SHARDS - 1);
        let slot_idx = ((block_idx as usize) >> 4) & (BLOCK_CACHE_SLOTS_PER_SHARD - 1);
        if let Ok(guard) = self.cache_shards[shard_idx].lock()
            && let Some(Some(entry)) = guard.get(slot_idx)
            && entry.block_idx == block_idx
        {
            return Ok(Arc::clone(&entry.data));
        }

        let slice = self.block_slice(block_idx)?;
        self.prefetch_blocks_ahead(block_idx);
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
        self.decoded_blocks.fetch_add(1, Ordering::Relaxed);
        let arc: Arc<[u8]> = Arc::from(raw);
        if let Ok(mut guard) = self.cache_shards[shard_idx].lock() {
            if guard.is_empty() {
                guard.resize_with(BLOCK_CACHE_SLOTS_PER_SHARD, || None);
            }
            guard[slot_idx] = Some(BlockCacheSlot {
                block_idx,
                data: Arc::clone(&arc),
            });
        }
        Ok(arc)
    }

    /// Issue a bounded `MADV_WILLNEED` readahead hint on the next up to 4 compressed blocks
    /// (`<= 256 KiB`) so sequential cold text scans overlap SSD page-ins with Zstd decode.
    #[inline]
    pub(crate) fn prefetch_blocks_ahead(&self, block_idx: u32) {
        #[cfg(unix)]
        {
            let next = block_idx.saturating_add(1);
            if next >= self.block_count {
                return;
            }
            let last = next.saturating_add(4).min(self.block_count);
            let s_idx = (next as usize) * 8;
            let e_idx = (last as usize) * 8;
            if e_idx + 8 > self.block_offsets.len() {
                return;
            }
            let Ok(sb) = self.block_offsets[s_idx..s_idx + 8].try_into() else {
                return;
            };
            let Ok(eb) = self.block_offsets[e_idx..e_idx + 8].try_into() else {
                return;
            };
            let start = u64::from_le_bytes(sb) as usize;
            let end = (u64::from_le_bytes(eb) as usize).min(self.blocks_data.len());
            if end <= start || end - start > 256 * 1024 {
                return;
            }
            let page = 4096usize;
            let aligned_start = start & !(page - 1);
            let len = end - aligned_start;
            if len > 0 && aligned_start + len <= self.blocks_data.len() {
                // SAFETY: `aligned_start..aligned_start + len` lies within `self.blocks_data`
                // and is page-aligned; `MADV_WILLNEED` is purely advisory.
                unsafe {
                    let _ = libc::madvise(
                        self.blocks_data.as_ptr().add(aligned_start) as *mut libc::c_void,
                        len,
                        libc::MADV_WILLNEED,
                    );
                }
            }
        }
        #[cfg(not(unix))]
        {
            let _ = block_idx;
        }
    }

    #[inline]
    pub(crate) fn block_may_match(&self, block_idx: u32, mask: &TgramMask) -> bool {
        let Some(ref tgram) = self.tgram else {
            return true;
        };
        if block_idx >= tgram.block_count {
            return true;
        }
        let base = TGRAM_HEADER + (block_idx as usize) * (TGRAM_WORDS * 8);
        let bytes = &tgram.mmap[..];
        if base + TGRAM_WORDS * 8 > bytes.len() {
            return true;
        }
        for &(wi, req) in &mask.probes {
            let off = base + (wi as usize) * 8;
            let word = u64::from_le_bytes(bytes[off..off + 8].try_into().unwrap());
            if (word & req) != req {
                return false;
            }
        }
        true
    }

    #[allow(dead_code)]
    #[inline]
    pub(crate) fn decoded_block_count(&self) -> u64 {
        self.decoded_blocks.load(Ordering::Relaxed)
    }

    pub(super) fn get_cell(&self, row_id: u32) -> Result<Cell, Error> {
        let Some((block_idx, in_off, len)) = self.get_loc(row_id) else {
            return Ok(Cell::Null);
        };
        if len == 0 {
            return Ok(Cell::SmallText(InlineStr::new_truncate("")));
        }
        let s_off = in_off as usize;
        let e_off = s_off + (len as usize);
        if let Some((raw, utf8_ok)) = self.raw_uncompressed_block_checked(block_idx)? {
            let s = Self::slice_block_str(raw, utf8_ok, s_off, e_off)?;
            return Ok(Cell::from_text(s));
        }
        let (block, utf8_ok) = self.decode_block_checked(block_idx)?;
        let s = Self::slice_block_str(&block, utf8_ok, s_off, e_off)?;
        Ok(Cell::from_text(s))
    }

    #[allow(dead_code)]
    #[inline]
    pub(crate) fn row_count(&self) -> u32 {
        self.row_count
    }

    #[allow(dead_code)]
    #[inline]
    pub(crate) fn get_cell_cached(
        &self,
        row_id: u32,
        cached: &mut Option<(u32, Arc<[u8]>)>,
    ) -> Result<Cell, Error> {
        let Some((block_idx, in_off, len)) = self.get_loc(row_id) else {
            return Ok(Cell::Null);
        };
        if len == 0 {
            return Ok(Cell::SmallText(InlineStr::new_truncate("")));
        }
        let s_off = in_off as usize;
        let e_off = s_off + (len as usize);
        if let Some((raw, utf8_ok)) = self.raw_uncompressed_block_checked(block_idx)? {
            let s = Self::slice_block_str(raw, utf8_ok, s_off, e_off)?;
            return Ok(Cell::from_text(s));
        }
        let need_load = match cached {
            Some((lb, _)) => *lb != block_idx,
            None => true,
        };
        if need_load {
            let (decoded, _) = self.decode_block_checked(block_idx)?;
            *cached = Some((block_idx, decoded));
        }
        let (_, ref block) = *cached.as_ref().unwrap();
        let utf8_ok = self.is_block_utf8_ok(block_idx, block);
        let s = Self::slice_block_str(block, utf8_ok, s_off, e_off)?;
        Ok(Cell::from_text(s))
    }

    /// Count live rows whose string value contains `needle`, running SIMD `memmem`
    /// over each decoded 32 KiB text block rather than per-row UTF-8 conversion.
    pub(crate) fn count_contains_live(
        &self,
        needle: &[u8],
        mut is_live: impl FnMut(u32) -> bool,
    ) -> Result<usize, Error> {
        if self.row_count == 0 {
            return Ok(0);
        }
        if needle.is_empty() {
            let mut count = 0usize;
            for rid in 0..self.row_count {
                if self.get_loc(rid).is_some() && is_live(rid) {
                    count += 1;
                }
            }
            return Ok(count);
        }

        let mask = query_tgram_mask_bytes(needle);
        let finder = Finder::new(needle);
        let row_count = self.row_count as usize;
        let loc = &self.row_loc[..];
        let mut rid = 0usize;
        let mut hits = 0usize;

        #[inline]
        fn read_loc(loc: &[u8], rid: usize) -> Option<(u32, usize, usize, u32)> {
            let base = rid * 16;
            if base + 16 > loc.len() {
                return None;
            }
            let bidx = u32::from_le_bytes(loc[base..base + 4].try_into().ok()?);
            let in_off = u32::from_le_bytes(loc[base + 4..base + 8].try_into().ok()?) as usize;
            let len = u32::from_le_bytes(loc[base + 8..base + 12].try_into().ok()?) as usize;
            let flags = u32::from_le_bytes(loc[base + 12..base + 16].try_into().ok()?);
            Some((bidx, in_off, len, flags))
        }

        for bidx in 0..self.block_count {
            if let Some(ref m) = mask
                && !self.block_may_match(bidx, m)
            {
                continue;
            }
            let decoded_arc: Arc<[u8]>;
            let block: &[u8] = if let Some(raw) = self.raw_uncompressed_block(bidx)? {
                raw
            } else {
                decoded_arc = self.decode_block(bidx)?;
                &decoded_arc
            };
            let mut search_pos = 0usize;
            while search_pos < block.len() {
                let Some(rel) = finder.find(&block[search_pos..]) else {
                    break;
                };
                let hit_pos = search_pos + rel;

                // Non-null `(r_blk, in_off + len)` is monotonically non-decreasing across rows,
                // so we can stride across skipped blocks and non-matching rows in `row_loc`.
                while rid + 64 < row_count {
                    if let Some((rb, ioff, rlen, rflags)) = read_loc(loc, rid + 64)
                        && rflags == 0
                        && (rb < bidx || (rb == bidx && ioff + rlen <= hit_pos))
                    {
                        rid += 64;
                    } else {
                        break;
                    }
                }
                while rid + 8 < row_count {
                    if let Some((rb, ioff, rlen, rflags)) = read_loc(loc, rid + 8)
                        && rflags == 0
                        && (rb < bidx || (rb == bidx && ioff + rlen <= hit_pos))
                    {
                        rid += 8;
                    } else {
                        break;
                    }
                }

                let mut matched_span: Option<(usize, usize)> = None;
                while rid < row_count {
                    let Some((r_blk, in_off, len, flags)) = read_loc(loc, rid) else {
                        break;
                    };
                    if flags != 0 || r_blk < bidx {
                        rid += 1;
                        continue;
                    }
                    if r_blk > bidx {
                        break;
                    }
                    if in_off + len <= hit_pos {
                        rid += 1;
                        continue;
                    }
                    matched_span = Some((in_off, len));
                    break;
                }
                let Some((in_off, len)) = matched_span else {
                    break;
                };
                if hit_pos < in_off {
                    search_pos = in_off;
                    continue;
                }
                if hit_pos + needle.len() <= in_off + len && is_live(rid as u32) {
                    hits += 1;
                }
                search_pos = in_off + len;
                rid += 1;
            }
        }
        Ok(hits)
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
        let s_off = in_off as usize;
        let e_off = s_off + (len as usize);
        if let Some((raw, utf8_ok)) = self.raw_uncompressed_block_checked(block_idx)? {
            let s = Self::slice_block_str(raw, utf8_ok, s_off, e_off)?;
            return Ok(f(Some(s)));
        }
        let (block, utf8_ok) = self.decode_block_checked(block_idx)?;
        let s = Self::slice_block_str(&block, utf8_ok, s_off, e_off)?;
        Ok(f(Some(s)))
    }
}

/// Memory-mapped Dictionary-Encoded UTF-8 string column (`col_<name>.dstr`, `.doff`, `.dord`).
pub(crate) struct MmapDictCol {
    pub(crate) strings: Vec<Arc<str>>,
    cells: Vec<Cell>,
    ordinals: Mmap,
    row_count: u32,
    zone_map: OnceLock<DictZoneMap>,
}

impl MmapDictCol {
    #[allow(dead_code)]
    pub(super) fn open(dir: &Path, col_name: &str, row_count: u32) -> Result<Self, Error> {
        Self::open_checked(dir, col_name, row_count, 0, None)
    }

    pub(super) fn open_checked(
        dir: &Path,
        col_name: &str,
        row_count: u32,
        seg_id: u64,
        expected_crcs: Option<&std::collections::BTreeMap<String, (u64, u32)>>,
    ) -> Result<Self, Error> {
        let dstr_f = File::open(dir.join(format!("col_{col_name}.dstr"))).map_err(io_err)?;
        let doff_f = File::open(dir.join(format!("col_{col_name}.doff"))).map_err(io_err)?;
        let dord_f = File::open(dir.join(format!("col_{col_name}.dord"))).map_err(io_err)?;

        let dstr = unsafe { MmapOptions::new().map(&dstr_f) }.map_err(io_err)?;
        let doff = unsafe { MmapOptions::new().map(&doff_f) }.map_err(io_err)?;
        let ordinals = unsafe { MmapOptions::new().map(&dord_f) }.map_err(io_err)?;
        if let Some(crcs) = expected_crcs {
            for (ext, bytes) in [("dstr", &dstr[..]), ("doff", &doff[..]), ("dord", &ordinals[..])]
            {
                let fname = format!("col_{col_name}.{ext}");
                if let Some(&(exp_len, exp_crc)) = crcs.get(&fname) {
                    let got_len = bytes.len() as u64;
                    let got_crc = crate::crc::hardware_crc32c(bytes);
                    if got_len != exp_len || got_crc != exp_crc {
                        return Err(Error::runtime(format!(
                            "corrupt segment file 'seg_{seg_id:08}/{fname}': expected (len={exp_len}, crc={exp_crc:#010x}), got (len={got_len}, crc={got_crc:#010x})"
                        )));
                    }
                }
            }
        }
        if ordinals.len() < (row_count as usize).saturating_mul(2) {
            return Err(io_err(format!("corrupt cold column {col_name}: codes shorter than {row_count} rows")));
        }

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
            let arc = crate::store::value::intern_wing_str(s);
            let cell = Cell::from_arc_str(&arc);
            strings.push(arc);
            cells.push(cell);
        }

        Ok(Self {
            strings,
            cells,
            ordinals,
            row_count,
            zone_map: OnceLock::new(),
        })
    }

    #[inline]
    pub(crate) fn code_at(&self, row_id: u32) -> u16 {
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
    pub(crate) fn get_cell(&self, row_id: u32) -> Cell {
        let code = self.code_at(row_id);
        if code == DICT_NULL_CODE {
            Cell::Null
        } else {
            self.cells.get(code as usize).cloned().unwrap_or(Cell::Null)
        }
    }

    #[inline]
    pub(crate) fn get_cell_by_code(&self, code: u16) -> Cell {
        if code == DICT_NULL_CODE {
            Cell::Null
        } else {
            self.cells.get(code as usize).cloned().unwrap_or(Cell::Null)
        }
    }

    #[inline]
    pub(crate) fn find_code(&self, target: &str) -> Option<u16> {
        self.strings
            .iter()
            .position(|s| s.as_ref() == target)
            .map(|i| i as u16)
    }

    #[allow(dead_code)]
    #[inline]
    pub(crate) fn ordinals_bytes(&self) -> &[u8] {
        &self.ordinals
    }

    #[allow(dead_code)]
    #[inline]
    pub(crate) fn row_count(&self) -> u32 {
        self.row_count
    }

    pub(crate) fn count_contains_live(
        &self,
        needle: &[u8],
        mut is_live: impl FnMut(u32) -> bool,
    ) -> usize {
        let n_dict = self.strings.len();
        if n_dict == 0 || self.row_count == 0 {
            return 0;
        }
        let n_words = (n_dict + 63) >> 6;
        let mut allow = vec![0u64; n_words];
        let mut any = false;
        if needle.is_empty() {
            allow.fill(u64::MAX);
            any = true;
        } else {
            let finder = Finder::new(needle);
            for (i, s) in self.strings.iter().enumerate() {
                if finder.find(s.as_bytes()).is_some() {
                    allow[i >> 6] |= 1u64 << (i & 63);
                    any = true;
                }
            }
        }
        if !any {
            return 0;
        }
        let mut count = 0usize;
        for rid in 0..self.row_count {
            let code = self.code_at(rid);
            if code != DICT_NULL_CODE {
                let ci = code as usize;
                if (allow[ci >> 6] & (1u64 << (ci & 63))) != 0 && is_live(rid) {
                    count += 1;
                }
            }
        }
        count
    }

    #[inline]
    fn zone_map(&self) -> &DictZoneMap {
        self.zone_map
            .get_or_init(|| DictZoneMap::build(self))
    }

    /// `(skips, all)`: for `col == strings[code]` within `ZONE_BLOCK_ROWS` block `bi`.
    pub(crate) fn block_eq(&self, bi: usize, code: u16) -> (bool, bool) {
        let zm = self.zone_map();
        if bi >= zm.nblocks {
            return (false, false);
        }
        let ci = code as usize;
        let present = ci < self.strings.len() && zm.has_code(bi, ci);
        let skips = !present;
        let all = !zm.has_null[bi] && zm.single_code[bi] == Some(code);
        (skips, all)
    }

    /// `(skips, all)`: for `col in allowed` within `ZONE_BLOCK_ROWS` block `bi`.
    /// Null values are never allowed here (`allowed` indexes non-null dictionary codes only).
    pub(crate) fn block_in_set(&self, bi: usize, allowed: &[bool]) -> (bool, bool) {
        let zm = self.zone_map();
        if bi >= zm.nblocks {
            return (false, false);
        }
        if !zm.has_non_null[bi] {
            return (true, false);
        }
        let mut any_match = false;
        let mut any_disallowed = false;
        for (wi, &word) in zm.block_words(bi).iter().enumerate() {
            let mut w = word;
            while w != 0 {
                let bit = w.trailing_zeros() as usize;
                let code = (wi << 6) | bit;
                if allowed.get(code).copied().unwrap_or(false) {
                    any_match = true;
                } else {
                    any_disallowed = true;
                }
                w &= w - 1;
            }
        }
        let skips = !any_match;
        let all = !zm.has_null[bi] && zm.has_non_null[bi] && !any_disallowed;
        (skips, all)
    }

    /// `(skips, all)`: for `col is null` within `ZONE_BLOCK_ROWS` block `bi`.
    pub(crate) fn block_is_null(&self, bi: usize) -> (bool, bool) {
        let zm = self.zone_map();
        if bi >= zm.nblocks {
            return (false, false);
        }
        let skips = !zm.has_null[bi];
        let all = zm.has_null[bi] && !zm.has_non_null[bi];
        (skips, all)
    }
}

struct DictZoneMap {
    nblocks: usize,
    words_per_block: usize,
    has_null: Vec<bool>,
    has_non_null: Vec<bool>,
    single_code: Vec<Option<u16>>,
    bits: Vec<u64>,
}

impl DictZoneMap {
    fn build(dc: &MmapDictCol) -> Self {
        let n = dc.row_count as usize;
        let nblocks = n.div_ceil(ZONE_BLOCK_ROWS);
        let words_per_block = dc.strings.len().div_ceil(64).max(1);
        let mut has_null = Vec::with_capacity(nblocks);
        let mut has_non_null = Vec::with_capacity(nblocks);
        let mut single_code = Vec::with_capacity(nblocks);
        let mut bits = vec![0u64; nblocks.saturating_mul(words_per_block)];

        for (bi, start) in (0..n).step_by(ZONE_BLOCK_ROWS).enumerate() {
            let end = (start + ZONE_BLOCK_ROWS).min(n);
            let mut blk_null = false;
            let mut blk_non_null = false;
            let mut first_code: Option<u16> = None;
            let mut mixed = false;
            let blk_slice = &mut bits[bi * words_per_block..(bi + 1) * words_per_block];
            for rid in start..end {
                let c = dc.code_at(rid as u32);
                if c == DICT_NULL_CODE {
                    blk_null = true;
                    continue;
                }
                blk_non_null = true;
                match first_code {
                    None => first_code = Some(c),
                    Some(prev) if prev != c => mixed = true,
                    _ => {}
                }
                let ci = c as usize;
                let w = ci >> 6;
                if w < blk_slice.len() {
                    blk_slice[w] |= 1u64 << (ci & 63);
                }
            }
            has_null.push(blk_null);
            has_non_null.push(blk_non_null);
            single_code.push(if blk_non_null && !mixed { first_code } else { None });
        }

        Self {
            nblocks,
            words_per_block,
            has_null,
            has_non_null,
            single_code,
            bits,
        }
    }

    #[inline]
    fn block_words(&self, bi: usize) -> &[u64] {
        let off = bi * self.words_per_block;
        &self.bits[off..off + self.words_per_block]
    }

    #[inline]
    fn has_code(&self, bi: usize, code: usize) -> bool {
        let w = code >> 6;
        w < self.words_per_block && (self.block_words(bi)[w] & (1u64 << (code & 63))) != 0
    }
}

pub(crate) const TGRAM_WORDS: usize = 64; // 4096 bits (512 bytes) per 32 KiB text block
pub(crate) type TgramBlock = [u64; TGRAM_WORDS];

/// Sparse probe mask compiled once per query literal for a 4096-bit block trigram Bloom filter.
#[derive(Clone)]
pub(crate) struct TgramMask {
    probes: Vec<(u8, u64)>,
}

const TGRAM_MAGIC: &[u8; 4] = b"LTG1";
const TGRAM_VERSION: u32 = 2;
const TGRAM_HEADER: usize = 16;

#[inline]
fn add_tgram_bits(bits: &mut TgramBlock, tri: [u8; 3]) {
    let k = u32::from(tri[0]) | (u32::from(tri[1]) << 8) | (u32::from(tri[2]) << 16);
    let mut h = u64::from(k)
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        .rotate_left(17)
        .wrapping_mul(0xBF58_476D_1CE4_E5B9);
    h ^= h >> 31;
    let b1 = (h & 4095) as usize;
    let b2 = ((h >> 12) & 4095) as usize;
    let b3 = ((h >> 24) & 4095) as usize;
    bits[b1 >> 6] |= 1u64 << (b1 & 63);
    bits[b2 >> 6] |= 1u64 << (b2 & 63);
    bits[b3 >> 6] |= 1u64 << (b3 & 63);
}

#[inline]
fn add_bytes_trigrams(bits: &mut TgramBlock, bytes: &[u8]) {
    for w in bytes.windows(3) {
        add_tgram_bits(
            bits,
            [
                w[0].to_ascii_lowercase(),
                w[1].to_ascii_lowercase(),
                w[2].to_ascii_lowercase(),
            ],
        );
    }
}

/// Add ASCII-lowercased byte trigrams of `s` (plus char-by-char Unicode-lowercased trigrams
/// when non-ASCII, matching `exec::eval::has_word`) to a 4096-bit block Bloom filter.
pub(super) fn add_str_trigrams(bits: &mut TgramBlock, s: &str) {
    add_bytes_trigrams(bits, s.as_bytes());
    if !s.is_ascii() {
        let mut low = String::with_capacity(s.len());
        for ch in s.chars() {
            for lc in ch.to_lowercase() {
                low.push(lc);
            }
        }
        add_bytes_trigrams(bits, low.as_bytes());
    }
}

#[inline]
pub(crate) fn query_tgram_mask_bytes(needle: &[u8]) -> Option<TgramMask> {
    if needle.len() < 3 {
        return None;
    }
    let mut words: TgramBlock = [0u64; TGRAM_WORDS];
    add_bytes_trigrams(&mut words, needle);
    let probes: Vec<(u8, u64)> = words
        .iter()
        .enumerate()
        .filter(|&(_, &w)| w != 0)
        .map(|(i, &w)| (i as u8, w))
        .collect();
    if probes.is_empty() {
        None
    } else {
        Some(TgramMask { probes })
    }
}

pub(crate) fn query_tgram_mask(needle: &str, lower_unicode: bool) -> Option<TgramMask> {
    let cow: Cow<'_, str> = if lower_unicode && !needle.is_ascii() {
        let mut low = String::with_capacity(needle.len());
        for ch in needle.chars() {
            for lc in ch.to_lowercase() {
                low.push(lc);
            }
        }
        Cow::Owned(low)
    } else {
        Cow::Borrowed(needle)
    };
    query_tgram_mask_bytes(cow.as_bytes())
}

pub(super) fn encode_tgram(blocks: &[TgramBlock]) -> Vec<u8> {
    let mut body = Vec::with_capacity(blocks.len().saturating_mul(TGRAM_WORDS * 8));
    for blk in blocks {
        for &w in blk {
            body.extend_from_slice(&w.to_le_bytes());
        }
    }
    let mut crc = crate::crc::Hasher::new();
    crc.update(&body);
    let mut out = Vec::with_capacity(TGRAM_HEADER + body.len());
    out.extend_from_slice(TGRAM_MAGIC);
    out.extend_from_slice(&TGRAM_VERSION.to_le_bytes());
    out.extend_from_slice(&(blocks.len() as u32).to_le_bytes());
    out.extend_from_slice(&crc.finalize().to_le_bytes());
    out.extend_from_slice(&body);
    out
}

fn open_tgram_mmap(path: &Path, expected_blocks: u32, skip_body_crc: bool) -> Option<MmapTgram> {
    let f = File::open(path).ok()?;
    let file_len = f.metadata().ok()?.len() as usize;
    let blk_bytes = TGRAM_WORDS * 8;
    let want_len = TGRAM_HEADER.checked_add((expected_blocks as usize).checked_mul(blk_bytes)?)?;
    if file_len != want_len {
        return None;
    }
    let mmap = unsafe { MmapOptions::new().map(&f) }.ok()?;
    let u32_at = |off: usize| -> Option<u32> {
        Some(u32::from_le_bytes(mmap.get(off..off + 4)?.try_into().ok()?))
    };
    if mmap.get(0..4)? != TGRAM_MAGIC {
        return None;
    }
    if u32_at(4)? != TGRAM_VERSION || u32_at(8)? != expected_blocks {
        return None;
    }
    if !skip_body_crc {
        let body = mmap.get(TGRAM_HEADER..)?;
        if crate::crc::hardware_crc32c(body) != u32_at(12)? {
            return None;
        }
    }
    Some(MmapTgram {
        mmap,
        block_count: expected_blocks,
    })
}

pub(crate) const ZONE_BLOCK_ROWS: usize = 4096;

/// Comparison operand converted to a fixed-width column's native type.
#[derive(Debug, Clone, Copy)]
pub(crate) enum FixedRhs {
    I(i64),
    F(f64),
}

#[derive(Clone, Copy)]
pub(crate) struct IntZoneBlock {
    pub(crate) min: i64,
    pub(crate) max: i64,
    pub(crate) non_null: u32,
}

#[derive(Clone, Copy)]
pub(crate) struct FloatZoneBlock {
    pub(crate) min: f64,
    pub(crate) max: f64,
    pub(crate) non_null: u32,
    pub(crate) has_nan: bool,
}

pub(crate) enum FixedZoneMap {
    Int(Vec<IntZoneBlock>),
    Float(Vec<FloatZoneBlock>),
    None,
}

/// Per-block min/max/non-null statistics of a fixed-width column.
pub(super) fn build_fixed_zone_map(
    kind: DiskColKind,
    nulls: &[u8],
    values: &[u8],
    row_count: usize,
) -> FixedZoneMap {
    let n = row_count.min(nulls.len()).min(values.len() / 8);
    let word = |idx: usize| -> [u8; 8] { values[idx * 8..idx * 8 + 8].try_into().unwrap_or([0; 8]) };
    match kind {
        DiskColKind::Int64 | DiskColKind::Time64 => {
            let mut blks = Vec::with_capacity(n.div_ceil(ZONE_BLOCK_ROWS));
            for start in (0..n).step_by(ZONE_BLOCK_ROWS) {
                let end = (start + ZONE_BLOCK_ROWS).min(n);
                let mut min = i64::MAX;
                let mut max = i64::MIN;
                let mut non_null = 0u32;
                for idx in start..end {
                    if nulls[idx] == 0 {
                        let v = i64::from_le_bytes(word(idx));
                        min = min.min(v);
                        max = max.max(v);
                        non_null += 1;
                    }
                }
                blks.push(IntZoneBlock { min, max, non_null });
            }
            FixedZoneMap::Int(blks)
        }
        DiskColKind::Float64 => {
            let mut blks = Vec::with_capacity(n.div_ceil(ZONE_BLOCK_ROWS));
            for start in (0..n).step_by(ZONE_BLOCK_ROWS) {
                let end = (start + ZONE_BLOCK_ROWS).min(n);
                let mut min = f64::INFINITY;
                let mut max = f64::NEG_INFINITY;
                let mut non_null = 0u32;
                let mut has_nan = false;
                for idx in start..end {
                    if nulls[idx] == 0 {
                        let v = f64::from_le_bytes(word(idx));
                        if v.is_nan() {
                            has_nan = true;
                        } else {
                            if v < min {
                                min = v;
                            }
                            if v > max {
                                max = v;
                            }
                        }
                        non_null += 1;
                    }
                }
                blks.push(FloatZoneBlock {
                    min,
                    max,
                    non_null,
                    has_nan,
                });
            }
            FixedZoneMap::Float(blks)
        }
        _ => FixedZoneMap::None,
    }
}

const ZMAP_MAGIC: &[u8; 4] = b"LZM1";
const ZMAP_HEADER: usize = 24;
const ZMAP_BLOCK: usize = 24;

/// Serialized zone map (`col_<name>.zmap`): magic, kind tag, row count, block size, block
/// count, CRC32C of the body, then 24 bytes per block.
pub(super) fn encode_zone_map(zm: &FixedZoneMap, row_count: u32) -> Option<Vec<u8>> {
    let (tag, nblocks) = match zm {
        FixedZoneMap::Int(b) => (0u32, b.len()),
        FixedZoneMap::Float(b) => (1u32, b.len()),
        FixedZoneMap::None => return None,
    };
    let mut body = Vec::with_capacity(nblocks * ZMAP_BLOCK);
    match zm {
        FixedZoneMap::Int(blks) => {
            for b in blks {
                body.extend_from_slice(&b.min.to_le_bytes());
                body.extend_from_slice(&b.max.to_le_bytes());
                body.extend_from_slice(&b.non_null.to_le_bytes());
                body.extend_from_slice(&0u32.to_le_bytes());
            }
        }
        FixedZoneMap::Float(blks) => {
            for b in blks {
                body.extend_from_slice(&b.min.to_bits().to_le_bytes());
                body.extend_from_slice(&b.max.to_bits().to_le_bytes());
                body.extend_from_slice(&b.non_null.to_le_bytes());
                body.extend_from_slice(&u32::from(b.has_nan).to_le_bytes());
            }
        }
        FixedZoneMap::None => {}
    }
    let mut crc = crate::crc::Hasher::new();
    crc.update(&body);
    let mut out = Vec::with_capacity(ZMAP_HEADER + body.len());
    out.extend_from_slice(ZMAP_MAGIC);
    out.extend_from_slice(&tag.to_le_bytes());
    out.extend_from_slice(&row_count.to_le_bytes());
    out.extend_from_slice(&(ZONE_BLOCK_ROWS as u32).to_le_bytes());
    out.extend_from_slice(&(nblocks as u32).to_le_bytes());
    out.extend_from_slice(&crc.finalize().to_le_bytes());
    out.extend_from_slice(&body);
    Some(out)
}

/// Inverse of [`encode_zone_map`]; `None` unless the file matches this column exactly.
fn decode_zone_map(bytes: &[u8], kind: DiskColKind, row_count: u32) -> Option<FixedZoneMap> {
    let u32_at = |off: usize| -> Option<u32> {
        Some(u32::from_le_bytes(bytes.get(off..off + 4)?.try_into().ok()?))
    };
    if bytes.get(0..4)? != ZMAP_MAGIC {
        return None;
    }
    let tag = u32_at(4)?;
    let want_tag = match kind {
        DiskColKind::Int64 | DiskColKind::Time64 => 0,
        DiskColKind::Float64 => 1,
        _ => return None,
    };
    let nblocks = u32_at(16)? as usize;
    if tag != want_tag
        || u32_at(8)? != row_count
        || u32_at(12)? as usize != ZONE_BLOCK_ROWS
        || nblocks != (row_count as usize).div_ceil(ZONE_BLOCK_ROWS)
    {
        return None;
    }
    let body = bytes.get(ZMAP_HEADER..)?;
    if body.len() != nblocks * ZMAP_BLOCK {
        return None;
    }
    let mut crc = crate::crc::Hasher::new();
    crc.update(body);
    if crc.finalize() != u32_at(20)? {
        return None;
    }
    let u64_at = |b: &[u8], off: usize| u64::from_le_bytes(b[off..off + 8].try_into().unwrap_or([0; 8]));
    let u32_in = |b: &[u8], off: usize| u32::from_le_bytes(b[off..off + 4].try_into().unwrap_or([0; 4]));
    Some(if tag == 0 {
        FixedZoneMap::Int(
            body.chunks_exact(ZMAP_BLOCK)
                .map(|b| IntZoneBlock {
                    min: u64_at(b, 0) as i64,
                    max: u64_at(b, 8) as i64,
                    non_null: u32_in(b, 16),
                })
                .collect(),
        )
    } else {
        FixedZoneMap::Float(
            body.chunks_exact(ZMAP_BLOCK)
                .map(|b| FloatZoneBlock {
                    min: f64::from_bits(u64_at(b, 0)),
                    max: f64::from_bits(u64_at(b, 8)),
                    non_null: u32_in(b, 16),
                    has_nan: u32_in(b, 20) != 0,
                })
                .collect(),
        )
    })
}

#[inline]
fn eval_cmp_i64(lhs: i64, op: crate::ast::CmpOp, rhs: i64) -> bool {
    match op {
        crate::ast::CmpOp::Eq => lhs == rhs,
        crate::ast::CmpOp::Ne => lhs != rhs,
        crate::ast::CmpOp::Gt => lhs > rhs,
        crate::ast::CmpOp::Ge => lhs >= rhs,
        crate::ast::CmpOp::Lt => lhs < rhs,
        crate::ast::CmpOp::Le => lhs <= rhs,
    }
}

#[inline]
fn eval_cmp_f64(lhs: f64, op: crate::ast::CmpOp, rhs: f64) -> bool {
    match op {
        crate::ast::CmpOp::Eq => lhs == rhs,
        crate::ast::CmpOp::Ne => lhs != rhs,
        crate::ast::CmpOp::Gt => lhs > rhs,
        crate::ast::CmpOp::Ge => lhs >= rhs,
        crate::ast::CmpOp::Lt => lhs < rhs,
        crate::ast::CmpOp::Le => lhs <= rhs,
    }
}

#[inline]
fn int_zone_relation(blk: IntZoneBlock, op: crate::ast::CmpOp, rhs: i64) -> (bool, bool) {
    if blk.non_null == 0 {
        return (true, false);
    }
    match op {
        crate::ast::CmpOp::Gt => (blk.max <= rhs, blk.min > rhs),
        crate::ast::CmpOp::Ge => (blk.max < rhs, blk.min >= rhs),
        crate::ast::CmpOp::Lt => (blk.min >= rhs, blk.max < rhs),
        crate::ast::CmpOp::Le => (blk.min > rhs, blk.max <= rhs),
        crate::ast::CmpOp::Eq => (
            rhs < blk.min || rhs > blk.max,
            blk.min == blk.max && blk.min == rhs,
        ),
        crate::ast::CmpOp::Ne => (
            blk.min == blk.max && blk.min == rhs,
            rhs < blk.min || rhs > blk.max,
        ),
    }
}

#[inline]
fn float_zone_relation(blk: FloatZoneBlock, op: crate::ast::CmpOp, rhs: f64) -> (bool, bool) {
    if blk.non_null == 0 || rhs.is_nan() {
        return (true, false);
    }
    if blk.has_nan {
        return (false, false);
    }
    match op {
        crate::ast::CmpOp::Gt => (blk.max <= rhs, blk.min > rhs),
        crate::ast::CmpOp::Ge => (blk.max < rhs, blk.min >= rhs),
        crate::ast::CmpOp::Lt => (blk.min >= rhs, blk.max < rhs),
        crate::ast::CmpOp::Le => (blk.min > rhs, blk.max <= rhs),
        crate::ast::CmpOp::Eq => (
            rhs < blk.min || rhs > blk.max,
            blk.min == blk.max && blk.min == rhs,
        ),
        crate::ast::CmpOp::Ne => (
            blk.min == blk.max && blk.min == rhs,
            rhs < blk.min || rhs > blk.max,
        ),
    }
}

#[inline]
fn count_dense_i64(val_slice: &[u8], op: crate::ast::CmpOp, rhs: i64) -> usize {
    use crate::ast::CmpOp;
    let mut cnt = 0usize;
    match op {
        CmpOp::Eq => {
            for chunk in val_slice.chunks_exact(8) {
                let v = i64::from_le_bytes(chunk.try_into().unwrap());
                cnt += usize::from(v == rhs);
            }
        }
        CmpOp::Ne => {
            for chunk in val_slice.chunks_exact(8) {
                let v = i64::from_le_bytes(chunk.try_into().unwrap());
                cnt += usize::from(v != rhs);
            }
        }
        CmpOp::Gt => {
            for chunk in val_slice.chunks_exact(8) {
                let v = i64::from_le_bytes(chunk.try_into().unwrap());
                cnt += usize::from(v > rhs);
            }
        }
        CmpOp::Ge => {
            for chunk in val_slice.chunks_exact(8) {
                let v = i64::from_le_bytes(chunk.try_into().unwrap());
                cnt += usize::from(v >= rhs);
            }
        }
        CmpOp::Lt => {
            for chunk in val_slice.chunks_exact(8) {
                let v = i64::from_le_bytes(chunk.try_into().unwrap());
                cnt += usize::from(v < rhs);
            }
        }
        CmpOp::Le => {
            for chunk in val_slice.chunks_exact(8) {
                let v = i64::from_le_bytes(chunk.try_into().unwrap());
                cnt += usize::from(v <= rhs);
            }
        }
    }
    cnt
}

#[inline]
fn count_dense_i64_with_dict(
    val_slice: &[u8],
    ord_slice: &[u8],
    op: crate::ast::CmpOp,
    rhs: i64,
    target_code: u16,
) -> usize {
    use crate::ast::CmpOp;
    let mut cnt = 0usize;
    match op {
        CmpOp::Eq => {
            for (vchunk, ochunk) in val_slice.chunks_exact(8).zip(ord_slice.chunks_exact(2)) {
                let v = i64::from_le_bytes(vchunk.try_into().unwrap());
                let c = u16::from_le_bytes([ochunk[0], ochunk[1]]);
                cnt += usize::from((c == target_code) & (v == rhs));
            }
        }
        CmpOp::Ne => {
            for (vchunk, ochunk) in val_slice.chunks_exact(8).zip(ord_slice.chunks_exact(2)) {
                let v = i64::from_le_bytes(vchunk.try_into().unwrap());
                let c = u16::from_le_bytes([ochunk[0], ochunk[1]]);
                cnt += usize::from((c == target_code) & (v != rhs));
            }
        }
        CmpOp::Gt => {
            for (vchunk, ochunk) in val_slice.chunks_exact(8).zip(ord_slice.chunks_exact(2)) {
                let v = i64::from_le_bytes(vchunk.try_into().unwrap());
                let c = u16::from_le_bytes([ochunk[0], ochunk[1]]);
                cnt += usize::from((c == target_code) & (v > rhs));
            }
        }
        CmpOp::Ge => {
            for (vchunk, ochunk) in val_slice.chunks_exact(8).zip(ord_slice.chunks_exact(2)) {
                let v = i64::from_le_bytes(vchunk.try_into().unwrap());
                let c = u16::from_le_bytes([ochunk[0], ochunk[1]]);
                cnt += usize::from((c == target_code) & (v >= rhs));
            }
        }
        CmpOp::Lt => {
            for (vchunk, ochunk) in val_slice.chunks_exact(8).zip(ord_slice.chunks_exact(2)) {
                let v = i64::from_le_bytes(vchunk.try_into().unwrap());
                let c = u16::from_le_bytes([ochunk[0], ochunk[1]]);
                cnt += usize::from((c == target_code) & (v < rhs));
            }
        }
        CmpOp::Le => {
            for (vchunk, ochunk) in val_slice.chunks_exact(8).zip(ord_slice.chunks_exact(2)) {
                let v = i64::from_le_bytes(vchunk.try_into().unwrap());
                let c = u16::from_le_bytes([ochunk[0], ochunk[1]]);
                cnt += usize::from((c == target_code) & (v <= rhs));
            }
        }
    }
    cnt
}

#[inline]
fn count_dense_f64(val_slice: &[u8], op: crate::ast::CmpOp, rhs: f64) -> usize {
    use crate::ast::CmpOp;
    let mut cnt = 0usize;
    match op {
        CmpOp::Eq => {
            for chunk in val_slice.chunks_exact(8) {
                let v = f64::from_le_bytes(chunk.try_into().unwrap());
                cnt += usize::from(v == rhs);
            }
        }
        CmpOp::Ne => {
            for chunk in val_slice.chunks_exact(8) {
                let v = f64::from_le_bytes(chunk.try_into().unwrap());
                cnt += usize::from(v != rhs);
            }
        }
        CmpOp::Gt => {
            for chunk in val_slice.chunks_exact(8) {
                let v = f64::from_le_bytes(chunk.try_into().unwrap());
                cnt += usize::from(v > rhs);
            }
        }
        CmpOp::Ge => {
            for chunk in val_slice.chunks_exact(8) {
                let v = f64::from_le_bytes(chunk.try_into().unwrap());
                cnt += usize::from(v >= rhs);
            }
        }
        CmpOp::Lt => {
            for chunk in val_slice.chunks_exact(8) {
                let v = f64::from_le_bytes(chunk.try_into().unwrap());
                cnt += usize::from(v < rhs);
            }
        }
        CmpOp::Le => {
            for chunk in val_slice.chunks_exact(8) {
                let v = f64::from_le_bytes(chunk.try_into().unwrap());
                cnt += usize::from(v <= rhs);
            }
        }
    }
    cnt
}

#[inline]
fn count_dense_f64_with_dict(
    val_slice: &[u8],
    ord_slice: &[u8],
    op: crate::ast::CmpOp,
    rhs: f64,
    target_code: u16,
) -> usize {
    use crate::ast::CmpOp;
    let mut cnt = 0usize;
    match op {
        CmpOp::Eq => {
            for (vchunk, ochunk) in val_slice.chunks_exact(8).zip(ord_slice.chunks_exact(2)) {
                let v = f64::from_le_bytes(vchunk.try_into().unwrap());
                let c = u16::from_le_bytes([ochunk[0], ochunk[1]]);
                cnt += usize::from((c == target_code) & (v == rhs));
            }
        }
        CmpOp::Ne => {
            for (vchunk, ochunk) in val_slice.chunks_exact(8).zip(ord_slice.chunks_exact(2)) {
                let v = f64::from_le_bytes(vchunk.try_into().unwrap());
                let c = u16::from_le_bytes([ochunk[0], ochunk[1]]);
                cnt += usize::from((c == target_code) & (v != rhs));
            }
        }
        CmpOp::Gt => {
            for (vchunk, ochunk) in val_slice.chunks_exact(8).zip(ord_slice.chunks_exact(2)) {
                let v = f64::from_le_bytes(vchunk.try_into().unwrap());
                let c = u16::from_le_bytes([ochunk[0], ochunk[1]]);
                cnt += usize::from((c == target_code) & (v > rhs));
            }
        }
        CmpOp::Ge => {
            for (vchunk, ochunk) in val_slice.chunks_exact(8).zip(ord_slice.chunks_exact(2)) {
                let v = f64::from_le_bytes(vchunk.try_into().unwrap());
                let c = u16::from_le_bytes([ochunk[0], ochunk[1]]);
                cnt += usize::from((c == target_code) & (v >= rhs));
            }
        }
        CmpOp::Lt => {
            for (vchunk, ochunk) in val_slice.chunks_exact(8).zip(ord_slice.chunks_exact(2)) {
                let v = f64::from_le_bytes(vchunk.try_into().unwrap());
                let c = u16::from_le_bytes([ochunk[0], ochunk[1]]);
                cnt += usize::from((c == target_code) & (v < rhs));
            }
        }
        CmpOp::Le => {
            for (vchunk, ochunk) in val_slice.chunks_exact(8).zip(ord_slice.chunks_exact(2)) {
                let v = f64::from_le_bytes(vchunk.try_into().unwrap());
                let c = u16::from_le_bytes([ochunk[0], ochunk[1]]);
                cnt += usize::from((c == target_code) & (v <= rhs));
            }
        }
    }
    cnt
}

pub(crate) struct MmapFixedCol {
    kind: DiskColKind,
    nulls: Mmap,
    values: Mmap,
    row_count: u32,
    zmap_mmap: Option<Mmap>,
    zone_map: OnceLock<FixedZoneMap>,
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
        let rows = row_count as usize;
        let width = if kind == DiskColKind::Bool { 1 } else { 8 };
        if nulls.len() < rows || values.len() < rows.saturating_mul(width) {
            return Err(io_err(format!("corrupt cold column {col_name}: shorter than {rows} rows")));
        }
        // Memory-map `.zmap` lazily without decoding on open; `zones()` decodes on first query
        // and falls back to scanning the column if missing or invalid.
        let zmap_mmap = File::open(dir.join(format!("col_{col_name}.zmap")))
            .ok()
            .and_then(|zf| {
                let len = zf.metadata().ok()?.len() as usize;
                if len < ZMAP_HEADER {
                    return None;
                }
                unsafe { MmapOptions::new().map(&zf) }.ok()
            });
        Ok(Self {
            kind,
            nulls,
            values,
            row_count,
            zmap_mmap,
            zone_map: OnceLock::new(),
        })
    }

    pub(super) fn clear_zone_map(&mut self) {
        self.zmap_mmap = None;
        self.zone_map = OnceLock::new();
    }

    #[inline]
    pub(crate) fn nulls_bytes(&self) -> &[u8] {
        &self.nulls
    }

    #[inline]
    pub(crate) fn values_bytes(&self) -> &[u8] {
        &self.values
    }

    #[inline]
    pub(super) fn zmap_bytes(&self) -> Option<&[u8]> {
        self.zmap_mmap.as_deref()
    }

    #[inline]
    pub(crate) fn kind(&self) -> DiskColKind {
        self.kind
    }

    #[inline]
    pub(crate) fn row_count(&self) -> u32 {
        self.row_count
    }

    #[inline]
    pub(crate) fn is_null(&self, row_id: u32) -> bool {
        let idx = row_id as usize;
        row_id >= self.row_count || idx >= self.nulls.len() || self.nulls[idx] != 0
    }

    #[inline]
    pub(crate) fn read_i64(&self, row_id: u32) -> i64 {
        let off = (row_id as usize) * 8;
        if off + 8 > self.values.len() {
            return 0;
        }
        i64::from_le_bytes(self.values[off..off + 8].try_into().unwrap_or([0; 8]))
    }

    #[inline]
    pub(crate) fn read_f64(&self, row_id: u32) -> f64 {
        let off = (row_id as usize) * 8;
        if off + 8 > self.values.len() {
            return 0.0;
        }
        f64::from_le_bytes(self.values[off..off + 8].try_into().unwrap_or([0; 8]))
    }

    pub(crate) fn zones(&self) -> &FixedZoneMap {
        self.zone_map.get_or_init(|| {
            if let Some(ref zm_mmap) = self.zmap_mmap
                && let Some(zm) = decode_zone_map(zm_mmap, self.kind, self.row_count)
            {
                return zm;
            }
            build_fixed_zone_map(self.kind, &self.nulls, &self.values, self.row_count as usize)
        })
    }

    /// True when every row in `0..limit_rows.min(self.row_count)` is non-null according to the zone map.
    pub(crate) fn all_non_null(&self, limit_rows: u32) -> bool {
        let n = (limit_rows.min(self.row_count) as usize)
            .min(self.nulls.len())
            .min(self.values.len() / 8);
        if n < limit_rows.min(self.row_count) as usize {
            return false;
        }
        if n == 0 {
            return true;
        }
        match self.zones() {
            FixedZoneMap::Int(blks) => blks.iter().enumerate().all(|(bi, b)| {
                let s = bi * ZONE_BLOCK_ROWS;
                if s >= n {
                    return true;
                }
                let blk_len = (s + ZONE_BLOCK_ROWS).min(n) - s;
                b.non_null as usize == blk_len
            }),
            FixedZoneMap::Float(blks) => blks.iter().enumerate().all(|(bi, b)| {
                let s = bi * ZONE_BLOCK_ROWS;
                if s >= n {
                    return true;
                }
                let blk_len = (s + ZONE_BLOCK_ROWS).min(n) - s;
                b.non_null as usize == blk_len
            }),
            FixedZoneMap::None => false,
        }
    }

    /// Right-hand side of `col <op> value` in this column's native representation, under the
    /// same type-strict rules as row-level comparison. `None` when the pairing has no
    /// columnar equivalent (the row-level comparison is then always false or needs exact
    /// int/float handling).
    pub(crate) fn fixed_rhs(&self, rhs: &Cell) -> Option<FixedRhs> {
        match (self.kind, rhs) {
            (DiskColKind::Int64, Cell::Int(r)) | (DiskColKind::Time64, Cell::Time(r)) => {
                Some(FixedRhs::I(*r))
            }
            (DiskColKind::Float64, Cell::Float(r)) if !r.is_nan() => Some(FixedRhs::F(*r)),
            (DiskColKind::Float64, Cell::Int(r)) if r.unsigned_abs() <= (1u64 << 53) => {
                Some(FixedRhs::F(*r as f64))
            }
            _ => None,
        }
    }

    /// `col[rid] <op> rhs`; a null cell never matches.
    #[inline]
    pub(crate) fn cmp_at(&self, rid: u32, op: crate::ast::CmpOp, rhs: FixedRhs) -> bool {
        if self.is_null(rid) {
            return false;
        }
        match rhs {
            FixedRhs::I(r) => eval_cmp_i64(self.read_i64(rid), op, r),
            FixedRhs::F(r) => eval_cmp_f64(self.read_f64(rid), op, r),
        }
    }


    /// `(none, all)` for `is null` over block `bi`: whether no value, or every value, of
    /// the block is null. `(false, false)` when the column keeps no zone map.
    pub(crate) fn zone_block_null_relation(&self, bi: usize) -> (bool, bool) {
        let rows = (self.row_count as usize).saturating_sub(bi * ZONE_BLOCK_ROWS);
        let blk_len = rows.min(ZONE_BLOCK_ROWS) as u32;
        let non_null = match self.zones() {
            FixedZoneMap::Int(blks) => blks.get(bi).map(|b| b.non_null),
            FixedZoneMap::Float(blks) => blks.get(bi).map(|b| b.non_null),
            FixedZoneMap::None => None,
        };
        match non_null {
            _ if blk_len == 0 => (true, false),
            Some(nn) => (nn == blk_len, nn == 0),
            None => (false, false),
        }
    }

    /// `(none, all)` for one zone block under scan semantics: whether no row, or every
    /// row, of the block satisfies `value <op> rhs`. A null never satisfies a comparison
    /// except `!=`, which it always satisfies. `(false, false)` when min/max cannot decide.
    pub(crate) fn zone_block_relation(
        &self,
        bi: usize,
        op: crate::ast::CmpOp,
        rhs: &Cell,
    ) -> (bool, bool) {
        let rows = (self.row_count as usize).saturating_sub(bi * ZONE_BLOCK_ROWS);
        let blk_len = rows.min(ZONE_BLOCK_ROWS) as u32;
        let (rel, non_null) = match (self.zones(), self.kind, rhs) {
            (FixedZoneMap::Int(blks), DiskColKind::Int64, Cell::Int(r))
            | (FixedZoneMap::Int(blks), DiskColKind::Time64, Cell::Time(r)) => match blks.get(bi) {
                Some(b) => (int_zone_relation(*b, op, *r), b.non_null),
                None => return (false, false),
            },
            (FixedZoneMap::Float(blks), DiskColKind::Float64, Cell::Float(r)) if !r.is_nan() => {
                match blks.get(bi) {
                    Some(b) if !b.has_nan => (float_zone_relation(*b, op, *r), b.non_null),
                    _ => return (false, false),
                }
            }
            (FixedZoneMap::Float(blks), DiskColKind::Float64, Cell::Int(r))
                if r.unsigned_abs() <= (1u64 << 53) =>
            {
                match blks.get(bi) {
                    Some(b) if !b.has_nan => (float_zone_relation(*b, op, *r as f64), b.non_null),
                    _ => return (false, false),
                }
            }
            _ => return (false, false),
        };
        if blk_len == 0 {
            return (true, false);
        }
        let is_ne = op == crate::ast::CmpOp::Ne;
        if non_null == 0 {
            // Every value is null: only `!=` holds, and it holds for all of them.
            return (!is_ne, is_ne);
        }
        let no_nulls = non_null == blk_len;
        if is_ne {
            (rel.0 && no_nulls, rel.1)
        } else {
            (rel.0, rel.1 && no_nulls)
        }
    }

    pub(crate) fn count_cmp_with_opt_dict_eq(
        &self,
        op: crate::ast::CmpOp,
        rhs_i64: Option<i64>,
        rhs_f64: Option<f64>,
        opt_dict: Option<(&MmapDictCol, u16)>,
        seg_all_live: bool,
        mut is_live: impl FnMut(u32) -> bool,
    ) -> Option<usize> {
        let n = (self.row_count as usize)
            .min(self.nulls.len())
            .min(self.values.len() / 8);
        match (self.zones(), self.kind) {
            (FixedZoneMap::Int(blks), DiskColKind::Int64 | DiskColKind::Time64) => {
                let rhs = rhs_i64?;
                let mut total = 0usize;
                for (bi, blk) in blks.iter().enumerate() {
                    let start = bi * ZONE_BLOCK_ROWS;
                    if start >= n {
                        break;
                    }
                    let end = (start + ZONE_BLOCK_ROWS).min(n);
                    let blk_len = end - start;
                    let (skip, all_match) = int_zone_relation(*blk, op, rhs);
                    if skip {
                        continue;
                    }
                    let (d_skip, d_all) = match opt_dict {
                        Some((dc, target_code)) => dc.block_eq(bi, target_code),
                        None => (false, true),
                    };
                    if d_skip {
                        continue;
                    }
                    if seg_all_live && blk.non_null as usize == blk_len {
                        if all_match {
                            if d_all {
                                total += blk_len;
                            } else if let Some((dc, target_code)) = opt_dict {
                                if let Some(ord_slice) = dc.ordinals_bytes().get(start * 2..end * 2) {
                                    let mut blk_hits = 0usize;
                                    for pair in ord_slice.chunks_exact(2) {
                                        let c = u16::from_le_bytes([pair[0], pair[1]]);
                                        blk_hits += usize::from(c == target_code);
                                    }
                                    total += blk_hits;
                                } else {
                                    for rid in start..end {
                                        total += usize::from(dc.code_at(rid as u32) == target_code);
                                    }
                                }
                            }
                            continue;
                        }
                        let val_slice = &self.values[start * 8..end * 8];
                        if d_all {
                            total += count_dense_i64(val_slice, op, rhs);
                            continue;
                        }
                        if let Some((dc, target_code)) = opt_dict
                            && let Some(ord_slice) = dc.ordinals_bytes().get(start * 2..end * 2)
                        {
                            total += count_dense_i64_with_dict(
                                val_slice,
                                ord_slice,
                                op,
                                rhs,
                                target_code,
                            );
                            continue;
                        }
                    }
                    match opt_dict {
                        None => {
                            for idx in start..end {
                                let rid = idx as u32;
                                if self.nulls[idx] == 0
                                    && eval_cmp_i64(self.read_i64(rid), op, rhs)
                                    && (seg_all_live || is_live(rid))
                                {
                                    total += 1;
                                }
                            }
                        }
                        Some((dc, target_code)) => {
                            for idx in start..end {
                                let rid = idx as u32;
                                if dc.code_at(rid) == target_code
                                    && self.nulls[idx] == 0
                                    && eval_cmp_i64(self.read_i64(rid), op, rhs)
                                    && (seg_all_live || is_live(rid))
                                {
                                    total += 1;
                                }
                            }
                        }
                    }
                }
                Some(total)
            }
            (FixedZoneMap::Float(blks), DiskColKind::Float64) => {
                let rhs = rhs_f64?;
                let mut total = 0usize;
                for (bi, blk) in blks.iter().enumerate() {
                    let start = bi * ZONE_BLOCK_ROWS;
                    if start >= n {
                        break;
                    }
                    let end = (start + ZONE_BLOCK_ROWS).min(n);
                    let blk_len = end - start;
                    let (skip, all_match) = float_zone_relation(*blk, op, rhs);
                    if skip {
                        continue;
                    }
                    let (d_skip, d_all) = match opt_dict {
                        Some((dc, target_code)) => dc.block_eq(bi, target_code),
                        None => (false, true),
                    };
                    if d_skip {
                        continue;
                    }
                    if seg_all_live && blk.non_null as usize == blk_len {
                        if all_match {
                            if d_all {
                                total += blk_len;
                            } else if let Some((dc, target_code)) = opt_dict {
                                if let Some(ord_slice) = dc.ordinals_bytes().get(start * 2..end * 2) {
                                    let mut blk_hits = 0usize;
                                    for pair in ord_slice.chunks_exact(2) {
                                        let c = u16::from_le_bytes([pair[0], pair[1]]);
                                        blk_hits += usize::from(c == target_code);
                                    }
                                    total += blk_hits;
                                } else {
                                    for rid in start..end {
                                        total += usize::from(dc.code_at(rid as u32) == target_code);
                                    }
                                }
                            }
                            continue;
                        }
                        let val_slice = &self.values[start * 8..end * 8];
                        if d_all {
                            total += count_dense_f64(val_slice, op, rhs);
                            continue;
                        }
                        if let Some((dc, target_code)) = opt_dict
                            && let Some(ord_slice) = dc.ordinals_bytes().get(start * 2..end * 2)
                        {
                            total += count_dense_f64_with_dict(
                                val_slice,
                                ord_slice,
                                op,
                                rhs,
                                target_code,
                            );
                            continue;
                        }
                    }
                    match opt_dict {
                        None => {
                            for idx in start..end {
                                let rid = idx as u32;
                                if self.nulls[idx] == 0
                                    && eval_cmp_f64(self.read_f64(rid), op, rhs)
                                    && (seg_all_live || is_live(rid))
                                {
                                    total += 1;
                                }
                            }
                        }
                        Some((dc, target_code)) => {
                            for idx in start..end {
                                let rid = idx as u32;
                                if dc.code_at(rid) == target_code
                                    && self.nulls[idx] == 0
                                    && eval_cmp_f64(self.read_f64(rid), op, rhs)
                                    && (seg_all_live || is_live(rid))
                                {
                                    total += 1;
                                }
                            }
                        }
                    }
                }
                Some(total)
            }
            _ => None,
        }
    }

    pub(crate) fn sum_f64_live(
        &self,
        seg_all_live: bool,
        mut is_live: impl FnMut(u32) -> bool,
    ) -> Option<crate::exact_sum::ExactSum> {
        let n = (self.row_count as usize)
            .min(self.nulls.len())
            .min(self.values.len() / 8);
        match self.kind {
            DiskColKind::Float64 => {
                // Compensated, value by value in row order, like the in-memory `sum`, so a
                // cold column and the same rows in memory give the same answer.
                let mut out = crate::exact_sum::ExactSum::default();
                if seg_all_live
                    && let FixedZoneMap::Float(blks) = self.zones()
                {
                    for (bi, blk) in blks.iter().enumerate() {
                        let start = bi * ZONE_BLOCK_ROWS;
                        if start >= n {
                            break;
                        }
                        if blk.non_null == 0 {
                            continue;
                        }
                        let end = (start + ZONE_BLOCK_ROWS).min(n);
                        let blk_len = end - start;
                        if blk.non_null as usize == blk_len {
                            for chunk in self.values[start * 8..end * 8].chunks_exact(8) {
                                out.add_f64(f64::from_le_bytes(chunk.try_into().unwrap()));
                            }
                        } else {
                            for rid in start..end {
                                if self.nulls[rid] == 0 {
                                    out.add_f64(self.read_f64(rid as u32));
                                }
                            }
                        }
                    }
                    return Some(out);
                }
                for rid in 0..n {
                    if self.nulls[rid] == 0 && (seg_all_live || is_live(rid as u32)) {
                        out.add_f64(self.read_f64(rid as u32));
                    }
                }
                Some(out)
            }
            DiskColKind::Int64 => {
                // i128 so totals through or beyond the i64 range stay exact until the end.
                if seg_all_live
                    && let FixedZoneMap::Int(blks) = self.zones()
                {
                    let mut acc_i128: i128 = 0;
                    for (bi, blk) in blks.iter().enumerate() {
                        let start = bi * ZONE_BLOCK_ROWS;
                        if start >= n {
                            break;
                        }
                        if blk.non_null == 0 {
                            continue;
                        }
                        let end = (start + ZONE_BLOCK_ROWS).min(n);
                        let blk_len = end - start;
                        if blk.non_null as usize == blk_len {
                            let mut blk_sum: i128 = 0;
                            for chunk in self.values[start * 8..end * 8].chunks_exact(8) {
                                blk_sum += i64::from_le_bytes(chunk.try_into().unwrap()) as i128;
                            }
                            acc_i128 += blk_sum;
                        } else {
                            for rid in start..end {
                                if self.nulls[rid] == 0 {
                                    acc_i128 += self.read_i64(rid as u32) as i128;
                                }
                            }
                        }
                    }
                    return Some(crate::exact_sum::ExactSum::from_parts(acc_i128, 0.0, 0.0));
                }
                let mut out = crate::exact_sum::ExactSum::default();
                for rid in 0..n {
                    if self.nulls[rid] == 0 && (seg_all_live || is_live(rid as u32)) {
                        out.add_i64(self.read_i64(rid as u32));
                    }
                }
                Some(out)
            }
            _ => None,
        }
    }

    #[inline]
    pub(crate) fn get_cell(&self, row_id: u32) -> Cell {
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

    #[inline]
    pub(super) fn as_bytes(&self) -> &[u8] {
        &self.mmap
    }
}

pub(super) fn build_vec_ivf_file(
    seg_dir: &Path,
    col_name: &str,
    dim: usize,
    rows: super::segment::ColSrc<'_>,
) -> Result<(), Error> {
    if dim == 0 {
        return Ok(());
    }
    let mut live_rids: Vec<u32> = Vec::with_capacity(rows.len());
    for i in 0..rows.len() {
        if let Some(v) = rows.vec(i)
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

    // Quantize all `n` vectors once into a single flat `Vec<i8>` instead of `3 * N` `Arc<[i8]>` heap allocs.
    let mut all_sq8 = vec![0i8; n * dim];
    #[cfg(feature = "parallel")]
    {
        use rayon::prelude::*;
        all_sq8
            .par_chunks_mut(dim)
            .enumerate()
            .for_each(|(idx, dst)| {
                if let Some(v) = rows.vec(live_rids[idx] as usize) {
                    quantize_sq8_into(v, dst);
                }
            });
    }
    #[cfg(not(feature = "parallel"))]
    {
        for (idx, dst) in all_sq8.chunks_mut(dim).enumerate() {
            if let Some(v) = rows.vec(live_rids[idx] as usize) {
                quantize_sq8_into(v, dst);
            }
        }
    }

    let mut centroids = vec![0.0f32; k * dim];
    for c in 0..k {
        let src_rid = live_rids[(c * n) / k] as usize;
        if let Some(v) = rows.vec(src_rid) {
            let norm = (l2_norm_f64(v) as f32).max(1e-12);
            let dst = &mut centroids[c * dim..(c + 1) * dim];
            for (j, &val) in v.iter().enumerate() {
                dst[j] = val / norm;
            }
        }
    }

    let train_step = (n / 16_384).max(1);
    let train_indices: Vec<usize> = (0..n).step_by(train_step).collect();

    let mut c_sq8 = vec![0i8; k * dim];
    for _iter in 0..2 {
        for c in 0..k {
            quantize_sq8_into(
                &centroids[c * dim..(c + 1) * dim],
                &mut c_sq8[c * dim..(c + 1) * dim],
            );
        }

        let assign_fn = |&idx: &usize| -> usize {
            let vq = &all_sq8[idx * dim..(idx + 1) * dim];
            let mut best_c = 0usize;
            let mut best_d = f64::NEG_INFINITY;
            for (c, cq) in c_sq8.chunks_exact(dim).enumerate() {
                let d = dot_sq8(vq, cq);
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
            train_indices.par_iter().map(assign_fn).collect()
        };
        #[cfg(not(feature = "parallel"))]
        let train_assign: Vec<usize> = train_indices.iter().map(assign_fn).collect();

        let mut sums = vec![0.0f64; k * dim];
        let mut counts = vec![0usize; k];
        for (&idx, &cid) in train_indices.iter().zip(train_assign.iter()) {
            let rid = live_rids[idx];
            if let Some(v) = rows.vec(rid as usize) {
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

    for c in 0..k {
        quantize_sq8_into(
            &centroids[c * dim..(c + 1) * dim],
            &mut c_sq8[c * dim..(c + 1) * dim],
        );
    }

    let assign_all_fn = |(idx, &rid): (usize, &u32)| -> (u16, u32, u32) {
        let vq = &all_sq8[idx * dim..(idx + 1) * dim];
        let mut best_c = 0usize;
        let mut best_d = f64::NEG_INFINITY;
        for (c, cq) in c_sq8.chunks_exact(dim).enumerate() {
            let d = dot_sq8(vq, cq);
            if d > best_d {
                best_d = d;
                best_c = c;
            }
        }
        (best_c as u16, rid, idx as u32)
    };

    #[cfg(feature = "parallel")]
    let mut assigned: Vec<(u16, u32, u32)> = {
        use rayon::prelude::*;
        live_rids.par_iter().enumerate().map(assign_all_fn).collect()
    };
    #[cfg(not(feature = "parallel"))]
    let mut assigned: Vec<(u16, u32, u32)> =
        live_rids.iter().enumerate().map(assign_all_fn).collect();

    assigned.sort_unstable_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));

    let mut cluster_counts = vec![0u32; k];
    for &(cid, _, _) in &assigned {
        cluster_counts[cid as usize] += 1;
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

    const ZERO_PAD_64: [u8; 64] = [0u8; 64];
    #[cfg(target_endian = "little")]
    {
        let c_bytes = unsafe {
            std::slice::from_raw_parts(centroids.as_ptr() as *const u8, centroids.len() * 4)
        };
        w.write_all(c_bytes).map_err(io_err)?;
    }
    #[cfg(not(target_endian = "little"))]
    for &val in &centroids {
        w.write_all(&val.to_le_bytes()).map_err(io_err)?;
    }
    let pad1 = (cluster_offsets_off - (centroids_off + centroids_bytes)) as usize;
    if pad1 > 0 {
        w.write_all(&ZERO_PAD_64[..pad1]).map_err(io_err)?;
    }
    #[cfg(target_endian = "little")]
    {
        let off_bytes = unsafe {
            std::slice::from_raw_parts(
                cluster_offsets.as_ptr() as *const u8,
                cluster_offsets.len() * 4,
            )
        };
        w.write_all(off_bytes).map_err(io_err)?;
    }
    #[cfg(not(target_endian = "little"))]
    for &off in &cluster_offsets {
        w.write_all(&off.to_le_bytes()).map_err(io_err)?;
    }
    let pad2 = (cluster_rids_off - (cluster_offsets_off + cluster_offsets_bytes)) as usize;
    if pad2 > 0 {
        w.write_all(&ZERO_PAD_64[..pad2]).map_err(io_err)?;
    }
    for &(_, rid, _) in &assigned {
        w.write_all(&rid.to_le_bytes()).map_err(io_err)?;
    }
    let pad3 = (cluster_sq8_off - (cluster_rids_off + cluster_rids_bytes)) as usize;
    if pad3 > 0 {
        w.write_all(&ZERO_PAD_64[..pad3]).map_err(io_err)?;
    }
    let all_sq8_u8 =
        unsafe { std::slice::from_raw_parts(all_sq8.as_ptr() as *const u8, all_sq8.len()) };
    for &(_, _, idx) in &assigned {
        let base = (idx as usize) * dim;
        w.write_all(&all_sq8_u8[base..base + dim]).map_err(io_err)?;
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
    pub(super) fn open(
        dir: &Path,
        col_name: &str,
        dim: u32,
        row_count: u32,
    ) -> Result<Self, Error> {
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
    pub(super) fn nulls_bytes(&self) -> &[u8] {
        &self.nulls
    }

    #[inline]
    pub(super) fn f32_bytes(&self) -> &[u8] {
        &self.f32_mmap
    }

    #[inline]
    pub(super) fn sq8_bytes(&self) -> &[u8] {
        &self.sq8_mmap
    }

    #[inline]
    pub(super) fn ivf_bytes(&self) -> Option<&[u8]> {
        self.ivf.as_ref().map(|ivf| ivf.as_bytes())
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

pub(crate) enum MmapCol {
    Fixed(MmapFixedCol),
    Text(MmapTextCol),
    Dict(MmapDictCol),
    Vec(MmapVecCol),
}

impl MmapCol {
    #[inline]
    pub(crate) fn get_cell(&self, row_id: u32) -> Result<Cell, Error> {
        match self {
            MmapCol::Fixed(c) => Ok(c.get_cell(row_id)),
            MmapCol::Text(c) => c.get_cell(row_id),
            MmapCol::Dict(c) => Ok(c.get_cell(row_id)),
            MmapCol::Vec(c) => Ok(c.get_cell(row_id)),
        }
    }

    pub(crate) fn has_live_null(
        &self,
        row_count: u32,
        all_live: bool,
        mut is_live: impl FnMut(u32) -> bool,
    ) -> bool {
        let n = row_count as usize;
        match self {
            MmapCol::Fixed(fc) => {
                let bound = n.min(fc.nulls.len());
                if bound < n {
                    return true;
                }
                if all_live {
                    fc.nulls[..n].iter().any(|&b| b != 0)
                } else {
                    (0..row_count).any(|rid| fc.nulls[rid as usize] != 0 && is_live(rid))
                }
            }
            MmapCol::Dict(dc) => {
                if all_live {
                    dc.zone_map().has_null.iter().any(|&b| b)
                } else {
                    (0..row_count).any(|rid| dc.code_at(rid) == DICT_NULL_CODE && is_live(rid))
                }
            }
            MmapCol::Text(tc) => {
                (0..row_count).any(|rid| tc.get_loc(rid).is_none() && (all_live || is_live(rid)))
            }
            MmapCol::Vec(vc) => {
                (0..row_count).any(|rid| !vc.is_present(rid) && (all_live || is_live(rid)))
            }
        }
    }
}
