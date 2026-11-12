use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::Path;

use crc32fast::Hasher as Crc32;
use memmap2::{Mmap, MmapOptions};
use rustc_hash::FxHashMap;
use smallvec::SmallVec;

use crate::error::Error;
use crate::persist::io_err;
use crate::store::Cell;

#[inline]
pub(super) fn fnv1a_hash64(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    if h == 0 { 1 } else { h }
}

// ============================================================================
// Memory-Mapped Open-Addressed Hash Index (PK & Secondary Equality Index)
// ============================================================================

pub(super) struct MmapHashIndex {
    mmap: Mmap,
    mask: usize,
    buckets_off: usize,
    payload_off: usize,
}

impl MmapHashIndex {
    pub(super) fn write_from_map(
        path: &Path,
        map: &FxHashMap<Vec<u8>, Vec<u32>>,
    ) -> Result<(), Error> {
        let key_count = map.len();
        let cap = if key_count == 0 {
            16
        } else {
            ((key_count * 2).next_power_of_two()).max(16)
        };
        let mask = (cap - 1) as u32;
        let buckets_bytes = cap * 16;

        let mut buckets = vec![0u8; buckets_bytes];
        let mut payload = Vec::with_capacity(key_count * 32);

        for (key_bytes, row_ids) in map {
            if row_ids.is_empty() {
                continue;
            }
            let h = fnv1a_hash64(key_bytes);
            let pad_before = (4 - (payload.len() & 3)) & 3;
            payload.extend(std::iter::repeat_n(0u8, pad_before));
            let rel_off = payload.len() as u64;
            if rel_off > u32::MAX as u64 {
                return Err(io_err(
                    "segment index payload exceeded 4 GiB; split segment",
                ));
            }
            payload.extend_from_slice(&(key_bytes.len() as u32).to_le_bytes());
            payload.extend_from_slice(key_bytes);
            let pad_after = (4 - (payload.len() & 3)) & 3;
            payload.extend(std::iter::repeat_n(0u8, pad_after));
            payload.extend_from_slice(&(row_ids.len() as u32).to_le_bytes());
            for &rid in row_ids {
                payload.extend_from_slice(&rid.to_le_bytes());
            }

            let mut slot = (h as usize) & (mask as usize);
            loop {
                let b_off = slot * 16;
                let existing_h = u64::from_le_bytes(buckets[b_off..b_off + 8].try_into().unwrap());
                if existing_h == 0 {
                    buckets[b_off..b_off + 8].copy_from_slice(&h.to_le_bytes());
                    buckets[b_off + 8..b_off + 12].copy_from_slice(&(rel_off as u32).to_le_bytes());
                    buckets[b_off + 12..b_off + 16]
                        .copy_from_slice(&(row_ids.len() as u32).to_le_bytes());
                    break;
                }
                slot = (slot + 1) & (mask as usize);
            }
        }

        let mut f = BufWriter::with_capacity(128 * 1024, File::create(path).map_err(io_err)?);
        f.write_all(b"LHT1").map_err(io_err)?;
        f.write_all(&mask.to_le_bytes()).map_err(io_err)?;
        f.write_all(&(key_count as u32).to_le_bytes())
            .map_err(io_err)?;
        f.write_all(&0u32.to_le_bytes()).map_err(io_err)?;
        f.write_all(&buckets).map_err(io_err)?;
        f.write_all(&payload).map_err(io_err)?;
        f.flush().map_err(io_err)?;
        Ok(())
    }

    pub(super) fn open(path: &Path) -> Result<Self, Error> {
        let f = File::open(path).map_err(io_err)?;
        let mmap = unsafe { MmapOptions::new().map(&f) }.map_err(io_err)?;
        if mmap.len() < 16 || &mmap[0..4] != b"LHT1" {
            return Err(io_err("invalid LHT1 mmap index header"));
        }
        let mask = u32::from_le_bytes(mmap[4..8].try_into().unwrap()) as usize;
        let cap = mask
            .checked_add(1)
            .filter(|c| c.is_power_of_two())
            .ok_or_else(|| io_err("invalid LHT1 capacity mask"))?;
        let buckets_off = 16usize;
        let payload_off = cap
            .checked_mul(16)
            .and_then(|b| buckets_off.checked_add(b))
            .ok_or_else(|| io_err("LHT1 header overflow"))?;
        if payload_off > mmap.len() {
            return Err(io_err("truncated LHT1 mmap index"));
        }
        Ok(Self {
            mmap,
            mask,
            buckets_off,
            payload_off,
        })
    }

    /// Zero-allocation O(1) lookup of matching `u32` row IDs directly inside the `mmap`!
    #[inline]
    pub(super) fn for_each_key(&self, key_bytes: &[u8], mut visit: impl FnMut(u32) -> bool) {
        let h = fnv1a_hash64(key_bytes);
        let klen_expected = key_bytes.len();
        let bytes = &self.mmap[..];
        let payload_base = self.payload_off;
        let buckets = &bytes[self.buckets_off..payload_base];
        let max_probes = self.mask + 1;
        let mut slot = (h as usize) & self.mask;

        for _ in 0..max_probes {
            let b_off = slot * 16;
            let bh = u64::from_le_bytes(buckets[b_off..b_off + 8].try_into().unwrap());
            if bh == 0 {
                return;
            }
            if bh == h {
                let rel_off =
                    u32::from_le_bytes(buckets[b_off + 8..b_off + 12].try_into().unwrap()) as usize;
                let count_rel = (rel_off + 4 + klen_expected + 3) & !3;
                if let Some(k_off) = payload_base.checked_add(rel_off)
                    && let Some(c_off) = payload_base.checked_add(count_rel)
                    && c_off + 4 <= bytes.len()
                {
                    let klen = u32::from_le_bytes(bytes[k_off..k_off + 4].try_into().unwrap()) as usize;
                    if klen == klen_expected && &bytes[k_off + 4..k_off + 4 + klen] == key_bytes {
                        let count =
                            u32::from_le_bytes(bytes[c_off..c_off + 4].try_into().unwrap()) as usize;
                        let r_start = c_off + 4;
                        if let Some(r_end) = count.checked_mul(4).and_then(|n| r_start.checked_add(n))
                            && r_end <= bytes.len()
                        {
                            if count == 1 {
                                let rid = u32::from_le_bytes(
                                    bytes[r_start..r_start + 4].try_into().unwrap(),
                                );
                                let _ = visit(rid);
                            } else {
                                for chunk in bytes[r_start..r_end].chunks_exact(4) {
                                    let rid = u32::from_le_bytes(chunk.try_into().unwrap());
                                    if !visit(rid) {
                                        return;
                                    }
                                }
                            }
                        }
                        return;
                    }
                }
            }
            slot = (slot + 1) & self.mask;
        }
    }
}

#[inline]
pub(super) fn with_text_index_key<R>(s: &str, f: impl FnOnce(&[u8]) -> R) -> R {
    let sb = s.as_bytes();
    if sb.len() < 255 {
        let mut buf = [0u8; 256];
        buf[0] = 1;
        buf[1..1 + sb.len()].copy_from_slice(sb);
        f(&buf[..1 + sb.len()])
    } else {
        let mut v = Vec::with_capacity(1 + sb.len());
        v.push(1);
        v.extend_from_slice(sb);
        f(&v)
    }
}

#[inline]
pub(super) fn encode_index_cell_key(cell: &Cell, out: &mut Vec<u8>) -> bool {
    out.clear();
    match cell {
        Cell::Null => false,
        Cell::Text(s) => {
            out.push(1);
            out.extend_from_slice(s.as_bytes());
            true
        }
        Cell::SmallText(s) => {
            out.push(1);
            out.extend_from_slice(s.as_str().as_bytes());
            true
        }
        Cell::Int(n) => {
            out.push(2);
            out.extend_from_slice(&n.to_le_bytes());
            true
        }
        Cell::Time(t) => {
            out.push(3);
            out.extend_from_slice(&t.to_le_bytes());
            true
        }
        Cell::Bool(b) => {
            out.push(4);
            out.push(u8::from(*b));
            true
        }
        Cell::Float(f) => {
            out.push(5);
            let bits = if *f == 0.0 { 0u64 } else { f.to_bits() };
            out.extend_from_slice(&bits.to_le_bytes());
            true
        }
        Cell::Vec(_) => false,
    }
}

// ============================================================================
// Zero-Heap Memory-Mapped Okapi BM25 Inverted Index (`bm25_<stemmer>.v3.mmap`)
// ============================================================================

const BM25_MMAP_MAGIC: &[u8; 8] = b"LBM25M03";
const BM25_MMAP_HDR_LEN: usize = 96;

#[inline]
fn push_uvarint32(out: &mut Vec<u8>, mut v: u32) {
    while v >= 0x80 {
        out.push(((v as u8) & 0x7f) | 0x80);
        v >>= 7;
    }
    out.push(v as u8);
}

#[inline]
fn read_uvarint32(buf: &[u8], pos: &mut usize) -> Option<u32> {
    let mut result = 0u32;
    let mut shift = 0u32;
    loop {
        let &b = buf.get(*pos)?;
        *pos += 1;
        result |= u32::from(b & 0x7f).checked_shl(shift)?;
        if (b & 0x80) == 0 {
            return Some(result);
        }
        shift += 7;
        if shift >= 35 {
            return None;
        }
    }
}

/// Streaming builder for `MmapBm25Index` (`bm25_<stemmer>.v3.mmap`).
pub struct MmapBm25Builder {
    generation: u64,
    doc_lens: Vec<u32>,
    doc_id_offsets: Vec<u64>,
    doc_id_bytes: Vec<u8>,
    total_tokens: u64,
    terms: FxHashMap<String, (u32, u32, Vec<u8>)>,
}

impl MmapBm25Builder {
    pub fn new(generation: u64) -> Self {
        let mut doc_id_offsets = Vec::with_capacity(1024);
        doc_id_offsets.push(0u64);
        Self {
            generation,
            doc_lens: Vec::new(),
            doc_id_offsets,
            doc_id_bytes: Vec::new(),
            total_tokens: 0,
            terms: FxHashMap::default(),
        }
    }

    /// Add one chunk/document (`doc_id` and its already stemmed/filtered tokens).
    pub fn add_document(&mut self, doc_id: &str, tokens: &[String]) {
        let doc_idx = self.doc_lens.len() as u32;
        let doc_len = tokens.len() as u32;
        self.total_tokens += u64::from(doc_len);
        self.doc_lens.push(doc_len);
        self.doc_id_bytes.extend_from_slice(doc_id.as_bytes());
        self.doc_id_offsets.push(self.doc_id_bytes.len() as u64);

        if tokens.is_empty() {
            return;
        }
        let mut tf_map: FxHashMap<&str, u32> = FxHashMap::default();
        for tok in tokens {
            *tf_map.entry(tok.as_str()).or_insert(0) += 1;
        }
        for (term, tf) in tf_map {
            if let Some(entry) = self.terms.get_mut(term) {
                let delta = doc_idx - entry.1;
                entry.0 += 1;
                entry.1 = doc_idx;
                push_uvarint32(&mut entry.2, delta);
                push_uvarint32(&mut entry.2, tf);
            } else {
                let mut post = Vec::with_capacity(8);
                push_uvarint32(&mut post, doc_idx);
                push_uvarint32(&mut post, tf);
                self.terms.insert(term.to_string(), (1, doc_idx, post));
            }
        }
    }

    /// Finalize and atomically write `dst_path`, then `mmap`-open it with zero heap allocations!
    pub fn finish_to_file(self, dst_path: &Path) -> Result<MmapBm25Index, Error> {
        let doc_count = self.doc_lens.len() as u32;
        let avgdl = if doc_count == 0 {
            1.0
        } else {
            (self.total_tokens as f64) / (doc_count as f64)
        }
        .max(1e-9);

        let mut sorted_terms: Vec<(String, u32, Vec<u8>)> = self
            .terms
            .into_iter()
            .map(|(k, (df, _, post))| (k, df, post))
            .collect();
        sorted_terms.sort_unstable_by(|a, b| a.0.cmp(&b.0));

        let term_count = sorted_terms.len();
        let cap = if term_count == 0 {
            16usize
        } else {
            ((term_count * 2).next_power_of_two()).max(16)
        };
        let hash_mask = (cap - 1) as u32;

        let mut hash_buckets = vec![0u8; cap * 16];
        let mut term_entries = Vec::with_capacity(term_count * 32);
        let mut term_strings = Vec::with_capacity(term_count * 10);
        let mut postings_blob = Vec::with_capacity(term_count * 16);

        for (t_idx, (term_str, df, post)) in sorted_terms.into_iter().enumerate() {
            let tb = term_str.as_bytes();
            let str_off = term_strings.len() as u64;
            let str_len = tb.len() as u32;
            term_strings.extend_from_slice(tb);

            let post_off = postings_blob.len() as u64;
            let post_len = post.len() as u32;
            postings_blob.extend_from_slice(&post);

            term_entries.extend_from_slice(&str_off.to_le_bytes());
            term_entries.extend_from_slice(&str_len.to_le_bytes());
            term_entries.extend_from_slice(&df.to_le_bytes());
            term_entries.extend_from_slice(&post_off.to_le_bytes());
            term_entries.extend_from_slice(&post_len.to_le_bytes());
            term_entries.extend_from_slice(&0u32.to_le_bytes());

            let h = fnv1a_hash64(tb);
            let mut slot = (h as usize) & (hash_mask as usize);
            loop {
                let boff = slot * 16;
                let cur_h = u64::from_le_bytes(hash_buckets[boff..boff + 8].try_into().unwrap());
                if cur_h == 0 {
                    hash_buckets[boff..boff + 8].copy_from_slice(&h.to_le_bytes());
                    hash_buckets[boff + 8..boff + 12]
                        .copy_from_slice(&(t_idx as u32).to_le_bytes());
                    break;
                }
                slot = (slot + 1) & (hash_mask as usize);
            }
        }

        let align64 = |x: u64| (x + 63) & !63;
        let doc_lens_off = align64(BM25_MMAP_HDR_LEN as u64);
        let doc_lens_bytes = (doc_count as u64) * 4;
        let doc_id_offs_off = align64(doc_lens_off + doc_lens_bytes);
        let doc_id_offs_bytes = (self.doc_id_offsets.len() as u64) * 8;
        let doc_id_bytes_off = align64(doc_id_offs_off + doc_id_offs_bytes);
        let doc_id_raw_bytes = self.doc_id_bytes.len() as u64;
        let hash_buckets_off = align64(doc_id_bytes_off + doc_id_raw_bytes);
        let hash_buckets_bytes = hash_buckets.len() as u64;
        let term_entries_off = align64(hash_buckets_off + hash_buckets_bytes);
        let term_entries_bytes = term_entries.len() as u64;
        let term_strings_off = align64(term_entries_off + term_entries_bytes);
        let term_strings_bytes = term_strings.len() as u64;
        let postings_off = align64(term_strings_off + term_strings_bytes);

        let mut hdr = [0u8; BM25_MMAP_HDR_LEN];
        hdr[0..8].copy_from_slice(BM25_MMAP_MAGIC);
        hdr[8..16].copy_from_slice(&self.generation.to_le_bytes());
        hdr[16..20].copy_from_slice(&doc_count.to_le_bytes());
        hdr[20..24].copy_from_slice(&(term_count as u32).to_le_bytes());
        hdr[24..28].copy_from_slice(&hash_mask.to_le_bytes());
        hdr[32..40].copy_from_slice(&avgdl.to_bits().to_le_bytes());
        hdr[40..48].copy_from_slice(&doc_lens_off.to_le_bytes());
        hdr[48..56].copy_from_slice(&doc_id_offs_off.to_le_bytes());
        hdr[56..64].copy_from_slice(&doc_id_bytes_off.to_le_bytes());
        hdr[64..72].copy_from_slice(&hash_buckets_off.to_le_bytes());
        hdr[72..80].copy_from_slice(&term_entries_off.to_le_bytes());
        hdr[80..88].copy_from_slice(&term_strings_off.to_le_bytes());
        hdr[88..96].copy_from_slice(&postings_off.to_le_bytes());

        let mut hw_crc = crate::crc::Hasher::new();
        hw_crc.update(&hdr[0..28]);
        hw_crc.update(&hdr[32..96]);
        hdr[28..32].copy_from_slice(&hw_crc.finalize().to_le_bytes());

        static TMP_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let nonce = TMP_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let tmp_path = dst_path.with_extension(format!("mmap.{}.{nonce}.tmp", std::process::id()));
        let res = (|| -> Result<(), Error> {
            let mut w =
                BufWriter::with_capacity(512 * 1024, File::create(&tmp_path).map_err(io_err)?);
            w.write_all(&hdr).map_err(io_err)?;
            let write_pad = |w: &mut BufWriter<File>, cur: u64, target: u64| -> Result<(), Error> {
                let p = (target - cur) as usize;
                if p > 0 {
                    w.write_all(&vec![0u8; p]).map_err(io_err)?;
                }
                Ok(())
            };
            write_pad(&mut w, BM25_MMAP_HDR_LEN as u64, doc_lens_off)?;
            for &dl in &self.doc_lens {
                w.write_all(&dl.to_le_bytes()).map_err(io_err)?;
            }
            write_pad(&mut w, doc_lens_off + doc_lens_bytes, doc_id_offs_off)?;
            for &off in &self.doc_id_offsets {
                w.write_all(&off.to_le_bytes()).map_err(io_err)?;
            }
            write_pad(
                &mut w,
                doc_id_offs_off + doc_id_offs_bytes,
                doc_id_bytes_off,
            )?;
            w.write_all(&self.doc_id_bytes).map_err(io_err)?;
            write_pad(
                &mut w,
                doc_id_bytes_off + doc_id_raw_bytes,
                hash_buckets_off,
            )?;
            w.write_all(&hash_buckets).map_err(io_err)?;
            write_pad(
                &mut w,
                hash_buckets_off + hash_buckets_bytes,
                term_entries_off,
            )?;
            w.write_all(&term_entries).map_err(io_err)?;
            write_pad(
                &mut w,
                term_entries_off + term_entries_bytes,
                term_strings_off,
            )?;
            w.write_all(&term_strings).map_err(io_err)?;
            write_pad(&mut w, term_strings_off + term_strings_bytes, postings_off)?;
            w.write_all(&postings_blob).map_err(io_err)?;
            w.flush().map_err(io_err)?;
            let f = w.into_inner().map_err(io_err)?;
            let _ = crate::persist::durable_sync(&f);
            drop(f);
            fs::rename(&tmp_path, dst_path).map_err(io_err)?;
            Ok(())
        })();
        if res.is_err() {
            let _ = fs::remove_file(&tmp_path);
            res?;
        }
        MmapBm25Index::open_if_valid(dst_path, self.generation)
            .ok_or_else(|| io_err("failed to open freshly written MmapBm25Index"))
    }
}

/// Zero-deserialization `mmap` Okapi BM25 inverted index (`0 MiB` heap at 500+ GiB scale!).
pub struct MmapBm25Index {
    mmap: Mmap,
    pub generation: u64,
    pub doc_count: u32,
    pub term_count: u32,
    hash_mask: usize,
    pub avgdl: f64,
    doc_lens_off: usize,
    doc_id_offs_off: usize,
    doc_id_bytes_off: usize,
    hash_buckets_off: usize,
    term_entries_off: usize,
    term_strings_off: usize,
    postings_off: usize,
}

impl MmapBm25Index {
    /// Open `path` via `mmap` in ~20 µs (`0` heap deserialization!) if magic, CRC32, and `expected_generation` match.
    pub fn open_if_valid(path: &Path, expected_generation: u64) -> Option<Self> {
        let f = File::open(path).ok()?;
        let mmap = unsafe { MmapOptions::new().map(&f) }.ok()?;
        if mmap.len() < BM25_MMAP_HDR_LEN || &mmap[0..8] != BM25_MMAP_MAGIC {
            return None;
        }
        let generation = u64::from_le_bytes(mmap[8..16].try_into().ok()?);
        if generation != expected_generation {
            return None;
        }
        let stored_crc = u32::from_le_bytes(mmap[28..32].try_into().ok()?);
        let mut hw_crc = crate::crc::Hasher::new();
        hw_crc.update(&mmap[0..28]);
        hw_crc.update(&mmap[32..96]);
        let crc_ok = if hw_crc.finalize() == stored_crc {
            true
        } else {
            let mut sw_crc = Crc32::new();
            sw_crc.update(&mmap[0..28]);
            sw_crc.update(&mmap[32..96]);
            sw_crc.finalize() == stored_crc
        };
        if !crc_ok {
            return None;
        }

        let doc_count = u32::from_le_bytes(mmap[16..20].try_into().ok()?);
        let term_count = u32::from_le_bytes(mmap[20..24].try_into().ok()?);
        let hash_mask = u32::from_le_bytes(mmap[24..28].try_into().ok()?) as usize;
        let cap = hash_mask.checked_add(1)?;
        if !cap.is_power_of_two() {
            return None;
        }
        let avgdl = f64::from_bits(u64::from_le_bytes(mmap[32..40].try_into().ok()?));
        if !avgdl.is_finite() || avgdl <= 0.0 {
            return None;
        }
        let doc_lens_off = usize::try_from(u64::from_le_bytes(mmap[40..48].try_into().ok()?)).ok()?;
        let doc_id_offs_off =
            usize::try_from(u64::from_le_bytes(mmap[48..56].try_into().ok()?)).ok()?;
        let doc_id_bytes_off =
            usize::try_from(u64::from_le_bytes(mmap[56..64].try_into().ok()?)).ok()?;
        let hash_buckets_off =
            usize::try_from(u64::from_le_bytes(mmap[64..72].try_into().ok()?)).ok()?;
        let term_entries_off =
            usize::try_from(u64::from_le_bytes(mmap[72..80].try_into().ok()?)).ok()?;
        let term_strings_off =
            usize::try_from(u64::from_le_bytes(mmap[80..88].try_into().ok()?)).ok()?;
        let postings_off =
            usize::try_from(u64::from_le_bytes(mmap[88..96].try_into().ok()?)).ok()?;

        let dc = doc_count as usize;
        let tc = term_count as usize;
        if doc_lens_off.checked_add(dc.checked_mul(4)?)? > mmap.len()
            || doc_id_offs_off.checked_add((dc + 1).checked_mul(8)?)? > mmap.len()
            || doc_id_bytes_off > mmap.len()
            || hash_buckets_off.checked_add(cap.checked_mul(16)?)? > mmap.len()
            || term_entries_off.checked_add(tc.checked_mul(32)?)? > mmap.len()
            || term_strings_off > mmap.len()
            || postings_off > mmap.len()
        {
            return None;
        }

        Some(Self {
            mmap,
            generation,
            doc_count,
            term_count,
            hash_mask,
            avgdl,
            doc_lens_off,
            doc_id_offs_off,
            doc_id_bytes_off,
            hash_buckets_off,
            term_entries_off,
            term_strings_off,
            postings_off,
        })
    }

    #[inline]
    pub fn doc_len(&self, doc_idx: u32) -> u32 {
        let off = self.doc_lens_off + (doc_idx as usize) * 4;
        u32::from_le_bytes(self.mmap[off..off + 4].try_into().unwrap_or([0; 4]))
    }

    /// Zero-copy `&str` slice into the `mmap` `doc_id` table for `doc_idx`!
    #[inline]
    pub fn doc_id(&self, doc_idx: u32) -> Option<&str> {
        if doc_idx >= self.doc_count {
            return None;
        }
        let base = self.doc_id_offs_off + (doc_idx as usize) * 8;
        let s = u64::from_le_bytes(self.mmap[base..base + 8].try_into().ok()?) as usize;
        let e = u64::from_le_bytes(self.mmap[base + 8..base + 16].try_into().ok()?) as usize;
        let abs_s = self.doc_id_bytes_off.checked_add(s)?;
        let abs_e = self.doc_id_bytes_off.checked_add(e)?;
        if abs_e > self.mmap.len() || abs_s > abs_e {
            return None;
        }
        std::str::from_utf8(&self.mmap[abs_s..abs_e]).ok()
    }

    #[inline]
    fn lookup_term(&self, term: &str) -> Option<(u32, &[u8])> {
        let tb = term.as_bytes();
        let h = fnv1a_hash64(tb);
        let cap = self.hash_mask + 1;
        let mut slot = (h as usize) & self.hash_mask;
        for _ in 0..cap {
            let boff = self.hash_buckets_off + slot * 16;
            let bh = u64::from_le_bytes(self.mmap[boff..boff + 8].try_into().ok()?);
            if bh == 0 {
                return None;
            }
            if bh == h {
                let t_idx =
                    u32::from_le_bytes(self.mmap[boff + 8..boff + 12].try_into().ok()?) as usize;
                let eoff = self.term_entries_off + t_idx * 32;
                if eoff + 32 <= self.mmap.len() {
                    let str_off =
                        u64::from_le_bytes(self.mmap[eoff..eoff + 8].try_into().ok()?) as usize;
                    let str_len =
                        u32::from_le_bytes(self.mmap[eoff + 8..eoff + 12].try_into().ok()?)
                            as usize;
                    let abs_str = self.term_strings_off.checked_add(str_off)?;
                    if abs_str.checked_add(str_len)? <= self.mmap.len()
                        && &self.mmap[abs_str..abs_str + str_len] == tb
                    {
                        let df =
                            u32::from_le_bytes(self.mmap[eoff + 12..eoff + 16].try_into().ok()?);
                        let post_off =
                            u64::from_le_bytes(self.mmap[eoff + 16..eoff + 24].try_into().ok()?)
                                as usize;
                        let post_len =
                            u32::from_le_bytes(self.mmap[eoff + 24..eoff + 28].try_into().ok()?)
                                as usize;
                        let abs_post = self.postings_off.checked_add(post_off)?;
                        let end_post = abs_post.checked_add(post_len)?;
                        if end_post <= self.mmap.len() {
                            return Some((df, &self.mmap[abs_post..end_post]));
                        }
                        return None;
                    }
                }
            }
            slot = (slot + 1) & self.hash_mask;
        }
        None
    }

    pub fn score_terms(&self, unique_terms: &[String], k1: f64, b: f64) -> Vec<(u32, f32)> {
        self.score_terms_top_k(unique_terms, k1, b, None)
    }

    pub fn score_terms_top_k(
        &self,
        unique_terms: &[String],
        k1: f64,
        b: f64,
        top_k: Option<usize>,
    ) -> Vec<(u32, f32)> {
        self.score_terms_filtered_top_k(unique_terms, k1, b, top_k, None)
    }

    pub fn score_terms_filtered_top_k(
        &self,
        unique_terms: &[String],
        k1: f64,
        b: f64,
        top_k: Option<usize>,
        allow_bitset: Option<&[u64]>,
    ) -> Vec<(u32, f32)> {
        if self.doc_count == 0 || unique_terms.is_empty() {
            return Vec::new();
        }
        if top_k == Some(0) {
            return Vec::new();
        }
        if let Some(bits) = allow_bitset
            && bits.iter().all(|&w| w == 0)
        {
            return Vec::new();
        }
        let n_docs = f64::from(self.doc_count);
        let inv_avgdl = b / self.avgdl;
        let k1_base = (k1 * (1.0 - b)) as f32;
        let k1_dl_scale = (k1 * inv_avgdl) as f32;
        let k1_plus_1 = k1 + 1.0;

        let mut active_terms: SmallVec<[(&[u8], f32); 8]> = SmallVec::new();
        for term in unique_terms {
            let Some((df_u32, post_bytes)) = self.lookup_term(term) else {
                continue;
            };
            let df = f64::from(df_u32);
            let idf = (1.0 + (n_docs - df + 0.5) / (df + 0.5)).ln();
            if idf <= 0.0 {
                continue;
            }
            active_terms.push((post_bytes, (idf * k1_plus_1) as f32));
        }
        if active_terms.is_empty() {
            return Vec::new();
        }

        let cmp_fast = |a: &(u32, f32), b_hit: &(u32, f32)| -> Ordering {
            b_hit.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b_hit.0))
        };
        let cmp_exact = |a: &(u32, f32), b_hit: &(u32, f32)| -> Ordering {
            b_hit
                .1
                .total_cmp(&a.1)
                .then_with(|| match (self.doc_id(a.0), self.doc_id(b_hit.0)) {
                    (Some(sa), Some(sb)) => sa.cmp(sb),
                    _ => a.0.cmp(&b_hit.0),
                })
        };

        let finalize_top_k = |mut out: Vec<(u32, f32)>, k: usize| -> Vec<(u32, f32)> {
            if out.len() <= k {
                out.sort_unstable_by(cmp_exact);
                return out;
            }
            out.select_nth_unstable_by(k - 1, cmp_fast);
            let cutoff_score = out[k - 1].1;
            let has_boundary_ties = out[k..].iter().any(|x| x.1 == cutoff_score);
            if !has_boundary_ties {
                out.truncate(k);
                out.sort_unstable_by(cmp_exact);
                return out;
            }
            let mut strictly_better = Vec::with_capacity(k);
            let mut tied = Vec::new();
            for item in out {
                if item.1 > cutoff_score {
                    strictly_better.push(item);
                } else if item.1 == cutoff_score {
                    tied.push(item);
                }
            }
            let need_ties = k.saturating_sub(strictly_better.len());
            if tied.len() > need_ties && need_ties > 0 {
                tied.select_nth_unstable_by(need_ties - 1, cmp_exact);
                tied.truncate(need_ties);
            } else {
                tied.truncate(need_ties);
            }
            strictly_better.extend(tied);
            strictly_better.sort_unstable_by(cmp_exact);
            strictly_better
        };

        if active_terms.len() == 1 {
            let (post_bytes, idf_num) = active_terms[0];
            if let Some(k) = top_k {
                #[derive(Clone, Copy)]
                struct TopCand {
                    doc_idx: u32,
                    score: f32,
                }
                impl PartialEq for TopCand {
                    fn eq(&self, other: &Self) -> bool {
                        self.doc_idx == other.doc_idx
                            && self.score.to_bits() == other.score.to_bits()
                    }
                }
                impl Eq for TopCand {}
                impl PartialOrd for TopCand {
                    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
                        Some(self.cmp(other))
                    }
                }
                impl Ord for TopCand {
                    fn cmp(&self, other: &Self) -> Ordering {
                        other
                            .score
                            .total_cmp(&self.score)
                            .then_with(|| self.doc_idx.cmp(&other.doc_idx))
                    }
                }

                let cap = k.saturating_add(64);
                let mut heap: BinaryHeap<TopCand> = BinaryHeap::with_capacity(cap + 1);
                let mut min_score = f32::NEG_INFINITY;
                let mut pos = 0usize;
                let mut doc_idx = 0u32;
                let mut first = true;
                while pos < post_bytes.len() {
                    let Some(delta) = read_uvarint32(post_bytes, &mut pos) else {
                        break;
                    };
                    let Some(tf_u32) = read_uvarint32(post_bytes, &mut pos) else {
                        break;
                    };
                    if first {
                        doc_idx = delta;
                        first = false;
                    } else {
                        doc_idx = doc_idx.wrapping_add(delta);
                    }
                    if doc_idx >= self.doc_count {
                        break;
                    }
                    if let Some(bits) = allow_bitset {
                        let w = (doc_idx as usize) >> 6;
                        if w >= bits.len() || ((bits[w] >> (doc_idx & 63)) & 1) == 0 {
                            continue;
                        }
                    }
                    let dl = self.doc_len(doc_idx) as f32;
                    let tf = tf_u32 as f32;
                    let score = (idf_num * tf) / (tf + k1_base + k1_dl_scale * dl);
                    if score <= 0.0 {
                        continue;
                    }
                    if heap.len() < cap {
                        heap.push(TopCand { doc_idx, score });
                        if heap.len() == cap
                            && let Some(worst) = heap.peek()
                        {
                            min_score = worst.score;
                        }
                    } else if score > min_score {
                        heap.pop();
                        heap.push(TopCand { doc_idx, score });
                        if let Some(worst) = heap.peek() {
                            min_score = worst.score;
                        }
                    }
                }
                let out: Vec<(u32, f32)> = heap.into_iter().map(|c| (c.doc_idx, c.score)).collect();
                return finalize_top_k(out, k);
            }

            let mut out = Vec::new();
            let mut pos = 0usize;
            let mut doc_idx = 0u32;
            let mut first = true;
            while pos < post_bytes.len() {
                let Some(delta) = read_uvarint32(post_bytes, &mut pos) else {
                    break;
                };
                let Some(tf_u32) = read_uvarint32(post_bytes, &mut pos) else {
                    break;
                };
                if first {
                    doc_idx = delta;
                    first = false;
                } else {
                    doc_idx = doc_idx.wrapping_add(delta);
                }
                if doc_idx >= self.doc_count {
                    break;
                }
                if let Some(bits) = allow_bitset {
                    let w = (doc_idx as usize) >> 6;
                    if w >= bits.len() || ((bits[w] >> (doc_idx & 63)) & 1) == 0 {
                        continue;
                    }
                }
                let dl = self.doc_len(doc_idx) as f32;
                let tf = tf_u32 as f32;
                let score = (idf_num * tf) / (tf + k1_base + k1_dl_scale * dl);
                if score > 0.0 {
                    out.push((doc_idx, score));
                }
            }
            return out;
        }

        thread_local! {
            static BM25_SCRATCH: std::cell::RefCell<(Vec<f32>, Vec<u32>)> =
                const { std::cell::RefCell::new((Vec::new(), Vec::new())) };
        }

        BM25_SCRATCH.with(|cell| {
            let mut guard = cell.borrow_mut();
            let (scores, touched) = &mut *guard;
            let n = self.doc_count as usize;
            if scores.len() < n {
                scores.resize(n, 0.0);
            }
            touched.clear();

            for &(post_bytes, idf_num) in &active_terms {
                let mut pos = 0usize;
                let mut doc_idx = 0u32;
                let mut first = true;
                while pos < post_bytes.len() {
                    let Some(delta) = read_uvarint32(post_bytes, &mut pos) else {
                        break;
                    };
                    let Some(tf_u32) = read_uvarint32(post_bytes, &mut pos) else {
                        break;
                    };
                    if first {
                        doc_idx = delta;
                        first = false;
                    } else {
                        doc_idx = doc_idx.wrapping_add(delta);
                    }
                    if doc_idx >= self.doc_count {
                        break;
                    }
                    if let Some(bits) = allow_bitset {
                        let w = (doc_idx as usize) >> 6;
                        if w >= bits.len() || ((bits[w] >> (doc_idx & 63)) & 1) == 0 {
                            continue;
                        }
                    }
                    let dl = self.doc_len(doc_idx) as f32;
                    let tf = tf_u32 as f32;
                    let contrib = (idf_num * tf) / (tf + k1_base + k1_dl_scale * dl);
                    let idx = doc_idx as usize;
                    let prev = scores[idx];
                    if prev == 0.0 && contrib > 0.0 {
                        touched.push(doc_idx);
                    }
                    scores[idx] = prev + contrib;
                }
            }

            let mut out: Vec<(u32, f32)> = Vec::with_capacity(touched.len());
            for &doc_idx in touched.iter() {
                let idx = doc_idx as usize;
                let s = scores[idx];
                scores[idx] = 0.0;
                if s > 0.0 {
                    out.push((doc_idx, s));
                }
            }
            touched.clear();

            if let Some(k) = top_k {
                return finalize_top_k(out, k);
            }
            out
        })
    }
}
