use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::Path;
use std::sync::Arc;

use rustc_hash::FxHashMap;

use super::columns::{
    DICT_MAX_BYTES, DICT_MAX_DISTINCT, DICT_MIN_SEGMENT_ROWS, DICT_NULL_CODE, IVF_MIN_SEGMENT_VECS,
    MmapCol, MmapDictCol, MmapFixedCol, MmapTextCol, MmapVecCol, TEXT_BLOCK_TARGET_BYTES,
    build_vec_ivf_file,
};
use super::index::{MmapHashIndex, encode_index_cell_key};
use super::{DiskColKind, DiskColSchema, DiskTableSchema, SEGMENTS_DIR, SegmentMeta};
use crate::embed::quantize_sq8_into;
use crate::error::Error;
use crate::persist::io_err;
use crate::store::{Cell, Row};

pub struct DiskSegment {
    pub meta: SegmentMeta,
    pub(super) cols: Vec<(String, MmapCol)>,
    pub(super) col_pos: FxHashMap<String, usize>,
    pub(super) pk_index: MmapHashIndex,
    pub(super) sec_indexes: FxHashMap<String, MmapHashIndex>,
}

impl DiskSegment {
    pub(super) fn open(seg_dir: &Path, meta: SegmentMeta) -> Result<Self, Error> {
        let mut cols = Vec::with_capacity(meta.columns.len());
        let mut col_pos = FxHashMap::default();
        for (i, col_schema) in meta.columns.iter().enumerate() {
            let mcol = match col_schema.kind {
                DiskColKind::Int64
                | DiskColKind::Float64
                | DiskColKind::Bool
                | DiskColKind::Time64 => MmapCol::Fixed(MmapFixedCol::open(
                    seg_dir,
                    &col_schema.name,
                    col_schema.kind,
                    meta.row_count,
                )?),
                DiskColKind::TextZstd => MmapCol::Text(MmapTextCol::open(
                    seg_dir,
                    &col_schema.name,
                    meta.row_count,
                )?),
                DiskColKind::TextDict => MmapCol::Dict(MmapDictCol::open(
                    seg_dir,
                    &col_schema.name,
                    meta.row_count,
                )?),
                DiskColKind::VecF32Sq8 { dim } => MmapCol::Vec(MmapVecCol::open(
                    seg_dir,
                    &col_schema.name,
                    dim,
                    meta.row_count,
                )?),
            };
            col_pos.insert(col_schema.name.clone(), i);
            cols.push((col_schema.name.clone(), mcol));
        }

        let pk_index = MmapHashIndex::open(&seg_dir.join("pk_id.ht"))?;
        let mut sec_indexes = FxHashMap::default();
        for field in &meta.indexed_fields {
            let p = seg_dir.join(format!("sec_{field}.ht"));
            if p.exists() {
                sec_indexes.insert(field.clone(), MmapHashIndex::open(&p)?);
            }
        }

        Ok(Self {
            meta,
            cols,
            col_pos,
            pk_index,
            sec_indexes,
        })
    }

    #[inline]
    pub fn materialize_row(&self, row_id: u32) -> Result<Row, Error> {
        let mut row = Row::new();
        for (name, col) in &self.cols {
            let cell = col.get_cell(row_id)?;
            if !matches!(cell, Cell::Null) {
                row.insert(name.clone(), cell);
            }
        }
        Ok(row)
    }

    #[inline]
    pub fn materialize_projected(&self, row_id: u32, fields: &[&str]) -> Result<Row, Error> {
        let mut row = Row::new();
        for &fname in fields {
            if let Some(&pos) = self.col_pos.get(fname) {
                let cell = self.cols[pos].1.get_cell(row_id)?;
                if !matches!(cell, Cell::Null) {
                    row.insert(fname.to_string(), cell);
                }
            }
        }
        Ok(row)
    }

    #[inline]
    pub fn get_cell(&self, row_id: u32, field: &str) -> Result<Cell, Error> {
        let Some(&pos) = self.col_pos.get(field) else {
            return Ok(Cell::Null);
        };
        self.cols[pos].1.get_cell(row_id)
    }

    #[inline]
    pub fn with_text_field<R>(
        &self,
        row_id: u32,
        field: &str,
        f: impl FnOnce(Option<&str>) -> R,
    ) -> Result<R, Error> {
        let Some(&pos) = self.col_pos.get(field) else {
            return Ok(f(None));
        };
        match &self.cols[pos].1 {
            MmapCol::Dict(dc) => Ok(f(dc.get_str(row_id))),
            MmapCol::Text(tc) => tc.with_str(row_id, f),
            _ => {
                let cell = self.cols[pos].1.get_cell(row_id)?;
                Ok(f(cell.text()))
            }
        }
    }

    #[inline]
    pub fn vec_col(&self, field: &str) -> Option<&MmapVecCol> {
        let pos = *self.col_pos.get(field)?;
        match &self.cols[pos].1 {
            MmapCol::Vec(vc) => Some(vc),
            _ => None,
        }
    }

    #[inline]
    pub(crate) fn col(&self, field: &str) -> Option<&MmapCol> {
        let pos = *self.col_pos.get(field)?;
        Some(&self.cols[pos].1)
    }

    #[inline]
    pub(crate) fn fixed_col(&self, field: &str) -> Option<&MmapFixedCol> {
        match self.col(field)? {
            MmapCol::Fixed(fc) => Some(fc),
            _ => None,
        }
    }

    #[allow(dead_code)]
    #[inline]
    pub(crate) fn dict_col(&self, field: &str) -> Option<&MmapDictCol> {
        match self.col(field)? {
            MmapCol::Dict(dc) => Some(dc),
            _ => None,
        }
    }

    #[allow(dead_code)]
    #[inline]
    pub(crate) fn text_col(&self, field: &str) -> Option<&MmapTextCol> {
        match self.col(field)? {
            MmapCol::Text(tc) => Some(tc),
            _ => None,
        }
    }

    #[inline]
    pub fn col_pos(&self, field: &str) -> Option<usize> {
        self.col_pos.get(field).copied()
    }

    #[inline]
    pub fn borrow_col_str<'a>(
        &'a self,
        col_pos: Option<usize>,
        rid: u32,
        last_block: &'a mut Option<(u32, Arc<[u8]>)>,
    ) -> Result<Option<&'a str>, Error> {
        let Some(pos) = col_pos else {
            return Ok(None);
        };
        match &self.cols[pos].1 {
            MmapCol::Dict(dc) => Ok(dc.get_str(rid)),
            MmapCol::Text(tc) => {
                let Some((bidx, in_off, len)) = tc.get_loc(rid) else {
                    return Ok(None);
                };
                if len == 0 {
                    return Ok(Some(""));
                }
                let need_load = match last_block {
                    Some((lb, _)) => *lb != bidx,
                    None => true,
                };
                if need_load {
                    let decoded = tc.decode_block(bidx)?;
                    *last_block = Some((bidx, decoded));
                }
                let (_, ref block) = *last_block.as_ref().unwrap();
                let s_off = in_off as usize;
                let e_off = s_off + (len as usize);
                if e_off > block.len() {
                    return Err(io_err("corrupt in-block text slice"));
                }
                let s = std::str::from_utf8(&block[s_off..e_off]).map_err(io_err)?;
                Ok(Some(s))
            }
            _ => Ok(None),
        }
    }

    #[inline]
    pub fn get_bool_opt(&self, rid: u32, field: &str) -> Option<bool> {
        let pos = *self.col_pos.get(field)?;
        match &self.cols[pos].1 {
            MmapCol::Fixed(fc) => match fc.get_cell(rid) {
                Cell::Bool(b) => Some(b),
                Cell::Int(n) => Some(n != 0),
                _ => None,
            },
            _ => None,
        }
    }

    #[inline]
    pub fn get_f64_opt(&self, rid: u32, field: &str) -> Option<f64> {
        let pos = *self.col_pos.get(field)?;
        match &self.cols[pos].1 {
            MmapCol::Fixed(fc) => match fc.get_cell(rid) {
                Cell::Float(f) => Some(f),
                Cell::Int(n) => Some(n as f64),
                _ => None,
            },
            _ => None,
        }
    }

    /// Build a compact bitset over dictionary codes for `field` (`MmapCol::Dict`),
    /// returning `(col_pos, bitset_u64, selectivity_fraction)` so vector scans can
    /// pre-filter rows in 2 CPU instructions before running SIMD dot products.
    pub(super) fn build_dict_allow_bitset<F>(
        &self,
        field: &str,
        allow: &F,
    ) -> Option<(usize, Vec<u64>, f32)>
    where
        F: Fn(&str) -> bool,
    {
        let pos = *self.col_pos.get(field)?;
        let MmapCol::Dict(dc) = &self.cols[pos].1 else {
            return None;
        };
        let n = dc.strings.len();
        if n == 0 {
            return Some((pos, Vec::new(), 0.0));
        }
        let mut bits = vec![0u64; n.div_ceil(64)];
        let mut allowed = 0usize;
        for (code, s) in dc.strings.iter().enumerate() {
            if allow(s.as_ref()) {
                bits[code >> 6] |= 1u64 << (code & 63);
                allowed += 1;
            }
        }
        let selectivity = (allowed as f32) / (n as f32);
        Some((pos, bits, selectivity))
    }

    #[inline]
    pub(super) fn check_dict_bitset(&self, pos: usize, rid: u32, bits: &[u64]) -> bool {
        if let MmapCol::Dict(dc) = &self.cols[pos].1 {
            let code = dc.code_at(rid);
            if code == DICT_NULL_CODE {
                return true;
            }
            let idx = (code as usize) >> 6;
            if idx < bits.len() {
                return ((bits[idx] >> (code & 63)) & 1) != 0;
            }
            false
        } else {
            false
        }
    }
}

/// Persist block min/max statistics next to a fixed-width column while its values are
/// still in memory, so opening the segment never has to scan the column to rebuild them.
fn write_zone_map(
    seg_dir: &Path,
    col_name: &str,
    kind: DiskColKind,
    nulls: &[u8],
    vals: &[u8],
) -> Result<(), Error> {
    let zm = super::columns::build_fixed_zone_map(kind, nulls, vals, nulls.len());
    if let Some(bytes) = super::columns::encode_zone_map(&zm, nulls.len() as u32) {
        fs::write(seg_dir.join(format!("col_{col_name}.zmap")), bytes).map_err(io_err)?;
    }
    Ok(())
}

fn write_single_column(
    seg_dir: &Path,
    col: &DiskColSchema,
    src: ColSrc<'_>,
) -> Result<DiskColKind, Error> {
    match col.kind {
        DiskColKind::Int64 | DiskColKind::Time64 => {
            let mut nulls = vec![1u8; src.len()];
            let mut vals = Vec::with_capacity(src.len() * 8);
            for i in 0..src.len() {
                let v = src.int_or_time(i);
                if let Some(n) = v {
                    nulls[i] = 0;
                    vals.extend_from_slice(&n.to_le_bytes());
                } else {
                    vals.extend_from_slice(&0i64.to_le_bytes());
                }
            }
            fs::write(seg_dir.join(format!("col_{}.null", col.name)), &nulls).map_err(io_err)?;
            fs::write(seg_dir.join(format!("col_{}.val", col.name)), &vals).map_err(io_err)?;
            write_zone_map(seg_dir, &col.name, col.kind, &nulls, &vals)?;
            Ok(col.kind)
        }
        DiskColKind::Float64 => {
            let mut nulls = vec![1u8; src.len()];
            let mut vals = Vec::with_capacity(src.len() * 8);
            for i in 0..src.len() {
                let v = src.float_or_int(i);
                if let Some(f) = v {
                    nulls[i] = 0;
                    vals.extend_from_slice(&f.to_le_bytes());
                } else {
                    vals.extend_from_slice(&0.0f64.to_le_bytes());
                }
            }
            fs::write(seg_dir.join(format!("col_{}.null", col.name)), &nulls).map_err(io_err)?;
            fs::write(seg_dir.join(format!("col_{}.val", col.name)), &vals).map_err(io_err)?;
            write_zone_map(seg_dir, &col.name, DiskColKind::Float64, &nulls, &vals)?;
            Ok(DiskColKind::Float64)
        }
        DiskColKind::Bool => {
            let mut nulls = vec![1u8; src.len()];
            let mut vals = vec![0u8; src.len()];
            for i in 0..src.len() {
                if let Some(b) = src.bool(i) {
                    nulls[i] = 0;
                    vals[i] = u8::from(b);
                }
            }
            fs::write(seg_dir.join(format!("col_{}.null", col.name)), &nulls).map_err(io_err)?;
            fs::write(seg_dir.join(format!("col_{}.val", col.name)), &vals).map_err(io_err)?;
            Ok(DiskColKind::Bool)
        }
        DiskColKind::TextZstd | DiskColKind::TextDict => {
            if src.len() >= DICT_MIN_SEGMENT_ROWS && col.name != "id" && col.name != "content" {
                let mut dict_map: FxHashMap<&str, u16> = FxHashMap::default();
                let mut dict_list: Vec<&str> = Vec::new();
                let mut dict_bytes = 0usize;
                let mut codes: Vec<u16> = Vec::with_capacity(src.len());
                let mut eligible = true;

                for row_idx in 0..src.len() {
                    if row_idx == 64 && src.len() >= 128 && dict_list.len() > 56 {
                        eligible = false;
                        break;
                    }
                    match src.text(row_idx) {
                        None => codes.push(DICT_NULL_CODE),
                        Some(s) => {
                            if let Some(&c) = dict_map.get(s) {
                                codes.push(c);
                            } else {
                                if dict_list.len() >= DICT_MAX_DISTINCT
                                    || dict_bytes.saturating_add(s.len()) > DICT_MAX_BYTES
                                {
                                    eligible = false;
                                    break;
                                }
                                let c = dict_list.len() as u16;
                                dict_map.insert(s, c);
                                dict_list.push(s);
                                dict_bytes += s.len();
                                codes.push(c);
                            }
                        }
                    }
                }

                if eligible && dict_list.len() * 2 <= src.len() {
                    let mut dstr_bytes = Vec::with_capacity(dict_bytes);
                    let mut doff_bytes = Vec::with_capacity((dict_list.len() + 1) * 4);
                    doff_bytes.extend_from_slice(&0u32.to_le_bytes());
                    for s in &dict_list {
                        dstr_bytes.extend_from_slice(s.as_bytes());
                        doff_bytes.extend_from_slice(&(dstr_bytes.len() as u32).to_le_bytes());
                    }
                    let mut dord_bytes = Vec::with_capacity(codes.len() * 2);
                    for &c in &codes {
                        dord_bytes.extend_from_slice(&c.to_le_bytes());
                    }
                    fs::write(seg_dir.join(format!("col_{}.dstr", col.name)), &dstr_bytes)
                        .map_err(io_err)?;
                    fs::write(seg_dir.join(format!("col_{}.doff", col.name)), &doff_bytes)
                        .map_err(io_err)?;
                    fs::write(seg_dir.join(format!("col_{}.dord", col.name)), &dord_bytes)
                        .map_err(io_err)?;
                    return Ok(DiskColKind::TextDict);
                }
            }

            let mut loc_bytes = Vec::with_capacity(src.len() * 16);
            let mut block_offsets: Vec<u64> = vec![0];
            let mut bdat_file = BufWriter::with_capacity(
                128 * 1024,
                File::create(seg_dir.join(format!("col_{}.bdat", col.name))).map_err(io_err)?,
            );
            let mut cur_bdat_len: u64 = 0;
            let mut cur_block = Vec::with_capacity(TEXT_BLOCK_TARGET_BYTES + 4096);
            let mut cur_block_idx: u32 = 0;

            #[cfg(feature = "compress-zstd")]
            let mut zstd_comp = zstd::bulk::Compressor::new(1).ok();

            #[allow(unused_mut)]
            let mut flush_block = |raw: &mut Vec<u8>,
                                   bdat: &mut BufWriter<File>,
                                   cur_len: &mut u64,
                                   boffs: &mut Vec<u64>,
                                   bidx: &mut u32|
             -> Result<(), Error> {
                if raw.is_empty() {
                    return Ok(());
                }
                #[cfg(feature = "compress-zstd")]
                let encoded = {
                    let compressed = if let Some(c) = zstd_comp.as_mut() {
                        c.compress(&raw[..]).map_err(io_err)?
                    } else {
                        zstd::encode_all(&raw[..], 1).map_err(io_err)?
                    };
                    if compressed.len() + 1 < raw.len() {
                        let mut out = Vec::with_capacity(1 + compressed.len());
                        out.push(0x01);
                        out.extend_from_slice(&compressed);
                        out
                    } else {
                        let mut out = Vec::with_capacity(1 + raw.len());
                        out.push(0x00);
                        out.extend_from_slice(raw);
                        out
                    }
                };
                #[cfg(not(feature = "compress-zstd"))]
                let encoded = {
                    let mut out = Vec::with_capacity(1 + raw.len());
                    out.push(0x00);
                    out.extend_from_slice(raw);
                    out
                };

                bdat.write_all(&encoded).map_err(io_err)?;
                *cur_len += encoded.len() as u64;
                boffs.push(*cur_len);
                raw.clear();
                *bidx += 1;
                Ok(())
            };

            for i in 0..src.len() {
                match src.text(i) {
                    None => {
                        loc_bytes.extend_from_slice(&0u32.to_le_bytes());
                        loc_bytes.extend_from_slice(&0u32.to_le_bytes());
                        loc_bytes.extend_from_slice(&0u32.to_le_bytes());
                        loc_bytes.extend_from_slice(&1u32.to_le_bytes());
                    }
                    Some(s) => {
                        let sb = s.as_bytes();
                        if !cur_block.is_empty()
                            && cur_block.len() + sb.len() > TEXT_BLOCK_TARGET_BYTES
                        {
                            flush_block(
                                &mut cur_block,
                                &mut bdat_file,
                                &mut cur_bdat_len,
                                &mut block_offsets,
                                &mut cur_block_idx,
                            )?;
                        }
                        let in_off = cur_block.len() as u32;
                        let blen = sb.len() as u32;
                        cur_block.extend_from_slice(sb);
                        loc_bytes.extend_from_slice(&cur_block_idx.to_le_bytes());
                        loc_bytes.extend_from_slice(&in_off.to_le_bytes());
                        loc_bytes.extend_from_slice(&blen.to_le_bytes());
                        loc_bytes.extend_from_slice(&0u32.to_le_bytes());
                    }
                }
            }
            flush_block(
                &mut cur_block,
                &mut bdat_file,
                &mut cur_bdat_len,
                &mut block_offsets,
                &mut cur_block_idx,
            )?;
            bdat_file.flush().map_err(io_err)?;

            let mut boff_bytes = Vec::with_capacity(block_offsets.len() * 8);
            for &off in &block_offsets {
                boff_bytes.extend_from_slice(&off.to_le_bytes());
            }
            fs::write(seg_dir.join(format!("col_{}.loc", col.name)), &loc_bytes).map_err(io_err)?;
            fs::write(seg_dir.join(format!("col_{}.boff", col.name)), &boff_bytes)
                .map_err(io_err)?;
            Ok(DiskColKind::TextZstd)
        }
        DiskColKind::VecF32Sq8 { dim } => {
            let d = (0..src.len())
                .find_map(|i| match src.vec(i) {
                    Some(v) if !v.is_empty() => Some(v.len()),
                    _ => None,
                })
                .unwrap_or(dim as usize);
            let mut nulls = vec![1u8; src.len()];
            let mut f32_writer = BufWriter::with_capacity(
                256 * 1024,
                File::create(seg_dir.join(format!("col_{}.vf32", col.name))).map_err(io_err)?,
            );
            let mut sq8_writer = BufWriter::with_capacity(
                128 * 1024,
                File::create(seg_dir.join(format!("col_{}.vsq8", col.name))).map_err(io_err)?,
            );
            let zero_f32 = vec![0u8; d * 4];
            let zero_sq8 = vec![0u8; d];
            #[cfg(not(target_endian = "little"))]
            let mut f32_row_buf = vec![0u8; d * 4];
            let mut sq8_row_buf = vec![0i8; d];

            for i in 0..src.len() {
                if d > 0
                    && let Some(v) = src.vec(i)
                    && v.len() == d
                {
                    nulls[i] = 0;
                    #[cfg(target_endian = "little")]
                    {
                        let f32_u8 =
                            unsafe { std::slice::from_raw_parts(v.as_ptr() as *const u8, d * 4) };
                        f32_writer.write_all(f32_u8).map_err(io_err)?;
                    }
                    #[cfg(not(target_endian = "little"))]
                    {
                        for (j, &val) in v.iter().enumerate() {
                            f32_row_buf[j * 4..j * 4 + 4].copy_from_slice(&val.to_le_bytes());
                        }
                        f32_writer.write_all(&f32_row_buf).map_err(io_err)?;
                    }
                    quantize_sq8_into(v, &mut sq8_row_buf);
                    let sq8_u8 =
                        unsafe { std::slice::from_raw_parts(sq8_row_buf.as_ptr() as *const u8, d) };
                    sq8_writer.write_all(sq8_u8).map_err(io_err)?;
                } else {
                    f32_writer.write_all(&zero_f32).map_err(io_err)?;
                    sq8_writer.write_all(&zero_sq8).map_err(io_err)?;
                }
            }
            f32_writer.flush().map_err(io_err)?;
            sq8_writer.flush().map_err(io_err)?;
            fs::write(seg_dir.join(format!("col_{}.vnull", col.name)), &nulls).map_err(io_err)?;
            if src.len() >= IVF_MIN_SEGMENT_VECS && d > 0 {
                build_vec_ivf_file(seg_dir, &col.name, d, src)?;
            }
            Ok(DiskColKind::VecF32Sq8 { dim: d as u32 })
        }
    }
}

fn write_pk_index(seg_dir: &Path, ids: ColSrc<'_>) -> Result<(), Error> {
    let mut pk_map: FxHashMap<Vec<u8>, Vec<u32>> = FxHashMap::default();
    pk_map.reserve(ids.len());
    let mut key_buf = Vec::with_capacity(64);
    for i in 0..ids.len() {
        if let Some(id_cell) = ids.key_cell(i)
            && encode_index_cell_key(&id_cell, &mut key_buf)
        {
            pk_map.entry(key_buf.clone()).or_default().push(i as u32);
        }
    }
    MmapHashIndex::write_from_map(&seg_dir.join("pk_id.ht"), &pk_map)
}

fn write_sec_index(seg_dir: &Path, field: &str, src: ColSrc<'_>) -> Result<(), Error> {
    let mut sec_map: FxHashMap<Vec<u8>, Vec<u32>> = FxHashMap::default();
    let mut key_buf = Vec::with_capacity(64);
    for i in 0..src.len() {
        if let Some(cell) = src.key_cell(i)
            && encode_index_cell_key(&cell, &mut key_buf)
        {
            sec_map.entry(key_buf.clone()).or_default().push(i as u32);
        }
    }
    MmapHashIndex::write_from_map(&seg_dir.join(format!("sec_{field}.ht")), &sec_map)
}

/// Where the values of one column of a new segment come from.
#[derive(Clone, Copy)]
pub(super) enum ColSrc<'a> {
    /// Field `name` of each row.
    Rows(&'a [Row], &'a str),
    /// A ready column of a [`RecordBatch`](crate::batch::RecordBatch); dense numeric
    /// columns are read straight from their native buffers.
    Col(&'a crate::batch::Column),
    /// Column not present in the input: every value absent.
    Absent(usize),
}

impl<'a> ColSrc<'a> {
    #[inline]
    pub(super) fn len(&self) -> usize {
        match self {
            ColSrc::Rows(rows, _) => rows.len(),
            ColSrc::Col(col) => col.len(),
            ColSrc::Absent(n) => *n,
        }
    }

    /// Borrowed cell where the source stores one (rows and general columns).
    #[inline]
    fn cell_ref(&self, i: usize) -> Option<&'a Cell> {
        match self {
            ColSrc::Rows(rows, name) => rows[i].get(*name),
            ColSrc::Col(col) => col.cell_ref(i),
            ColSrc::Absent(_) => None,
        }
    }

    /// Integer or timestamp value, for `Int64` / `Time64` columns.
    #[inline]
    pub(super) fn int_or_time(&self, i: usize) -> Option<i64> {
        use crate::batch::Column;
        match self {
            ColSrc::Col(Column::Int { vals, valid } | Column::Time { vals, valid }) => {
                valid.is_valid(i).then(|| vals[i])
            }
            _ => match self.cell_ref(i) {
                Some(Cell::Int(n)) => Some(*n),
                Some(Cell::Time(t)) => Some(*t),
                _ => None,
            },
        }
    }

    /// Float value (integers widen), for `Float64` columns.
    #[inline]
    pub(super) fn float_or_int(&self, i: usize) -> Option<f64> {
        use crate::batch::Column;
        match self {
            ColSrc::Col(Column::Float { vals, valid }) => valid.is_valid(i).then(|| vals[i]),
            ColSrc::Col(Column::Int { vals, valid }) => valid.is_valid(i).then(|| vals[i] as f64),
            _ => match self.cell_ref(i) {
                Some(Cell::Float(f)) => Some(*f),
                Some(Cell::Int(n)) => Some(*n as f64),
                _ => None,
            },
        }
    }

    #[inline]
    pub(super) fn bool(&self, i: usize) -> Option<bool> {
        use crate::batch::Column;
        match self {
            ColSrc::Col(Column::Bool { vals, valid }) => valid.is_valid(i).then(|| vals[i]),
            _ => match self.cell_ref(i) {
                Some(Cell::Bool(b)) => Some(*b),
                _ => None,
            },
        }
    }

    #[inline]
    pub(super) fn text(&self, i: usize) -> Option<&'a str> {
        self.cell_ref(i).and_then(Cell::text)
    }

    #[inline]
    pub(super) fn vec(&self, i: usize) -> Option<&'a Arc<[f32]>> {
        match self.cell_ref(i) {
            Some(Cell::Vec(v)) => Some(v),
            _ => None,
        }
    }

    /// Value as an index key: borrowed when the source holds a cell, rebuilt from the
    /// native buffer otherwise.
    #[inline]
    pub(super) fn key_cell(&self, i: usize) -> Option<std::borrow::Cow<'a, Cell>> {
        if let Some(c) = self.cell_ref(i) {
            return (!matches!(c, Cell::Null)).then_some(std::borrow::Cow::Borrowed(c));
        }
        match self {
            ColSrc::Col(col) if col.is_typed() => {
                let c = col.get(i);
                (!matches!(c, Cell::Null)).then_some(std::borrow::Cow::Owned(c))
            }
            _ => None,
        }
    }
}

/// Rows or columns a new segment is written from.
#[derive(Clone, Copy)]
pub(super) enum SegSrc<'a> {
    Rows(&'a [Row]),
    Batch(&'a crate::batch::RecordBatch),
}

impl<'a> SegSrc<'a> {
    pub(super) fn len(&self) -> usize {
        match self {
            SegSrc::Rows(rows) => rows.len(),
            SegSrc::Batch(b) => b.n(),
        }
    }

    pub(super) fn col(&self, name: &'a str) -> ColSrc<'a> {
        match self {
            SegSrc::Rows(rows) => ColSrc::Rows(rows, name),
            SegSrc::Batch(b) => match b.names.iter().position(|n| n == name) {
                Some(i) => ColSrc::Col(&b.cols[i]),
                None => ColSrc::Absent(b.n()),
            },
        }
    }
}

/// Write a slice of `Row`s into a brand-new immutable columnar mmap segment on disk.
/// Uses parallel column + index encoding via `std::thread::scope` on segments with `>= 1,024` rows.
pub(super) fn write_segment_to_disk(
    base_dir: &Path,
    seg_id: u64,
    schema: &DiskTableSchema,
    rows: &[Row],
) -> Result<SegmentMeta, Error> {
    write_segment_from(base_dir, seg_id, schema, SegSrc::Rows(rows))
}

/// Same as [`write_segment_to_disk`], from rows or straight from columns.
pub(super) fn write_segment_from(
    base_dir: &Path,
    seg_id: u64,
    schema: &DiskTableSchema,
    rows: SegSrc<'_>,
) -> Result<SegmentMeta, Error> {
    let seg_dir = base_dir.join(SEGMENTS_DIR).join(format!("seg_{seg_id:08}"));
    fs::create_dir_all(&seg_dir).map_err(io_err)?;
    let row_count = rows.len() as u32;

    let mut seg_columns = schema.columns.clone();
    if rows.len() >= 1_024 && (seg_columns.len() + schema.indexed_fields.len() >= 2) {
        std::thread::scope(|s| -> Result<(), Error> {
            let col_handles: Vec<_> = schema
                .columns
                .iter()
                .map(|col| {
                    let seg_dir_ref = &seg_dir;
                    s.spawn(move || write_single_column(seg_dir_ref, col, rows.col(&col.name)))
                })
                .collect();

            let pk_handle = {
                let seg_dir_ref = &seg_dir;
                s.spawn(move || write_pk_index(seg_dir_ref, rows.col("id")))
            };

            let sec_handles: Vec<_> = schema
                .indexed_fields
                .iter()
                .filter(|f| f.as_str() != "id")
                .map(|field| {
                    let seg_dir_ref = &seg_dir;
                    s.spawn(move || write_sec_index(seg_dir_ref, field, rows.col(field)))
                })
                .collect();

            for (i, h) in col_handles.into_iter().enumerate() {
                let kind = h
                    .join()
                    .map_err(|_| Error::runtime("column writer thread panicked"))??;
                seg_columns[i].kind = kind;
            }
            pk_handle
                .join()
                .map_err(|_| Error::runtime("pk index writer thread panicked"))??;
            for h in sec_handles {
                h.join()
                    .map_err(|_| Error::runtime("secondary index writer thread panicked"))??;
            }
            Ok(())
        })?;
    } else {
        for col in &mut seg_columns {
            col.kind = write_single_column(&seg_dir, col, rows.col(&col.name))?;
        }
        write_pk_index(&seg_dir, rows.col("id"))?;
        for field in &schema.indexed_fields {
            if field != "id" {
                write_sec_index(&seg_dir, field, rows.col(field))?;
            }
        }
    }

    Ok(SegmentMeta {
        seg_id,
        table: schema.name.clone(),
        row_count,
        columns: seg_columns,
        indexed_fields: schema.indexed_fields.clone(),
    })
}
