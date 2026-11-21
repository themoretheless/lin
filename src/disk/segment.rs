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
    /// Column positions in field-name order, and the shape of a row carrying every
    /// column: a row with no absent value is built from them without touching names.
    pub(super) name_order: Vec<usize>,
    pub(super) full_shape: Arc<crate::store::RowShape>,
    pub(super) id_col_pos: Option<usize>,
    pub(super) title_col_pos: Option<usize>,
    pub(super) can_cache_full_row: bool,
}

/// Pre-resolved projection plan for materializing rows from a single [`DiskSegment`]
/// without per-row `FxHashMap` lookups, field-name sorting, `Shape` lock probes, or
/// shared Zstd block-cache mutex contention.
pub(super) struct SegProjPlan<'s> {
    seg: &'s DiskSegment,
    cols: smallvec::SmallVec<[usize; 8]>,
    shape: Option<Arc<crate::store::RowShape>>,
    caches: smallvec::SmallVec<[super::SegBlockCache<'s>; 8]>,
}

impl<'s> SegProjPlan<'s> {
    #[inline]
    pub(super) fn materialize(&mut self, row_id: u32) -> Result<Row, Error> {
        let Some(ref shape) = self.shape else {
            return Ok(Row::new());
        };
        let n = self.cols.len();
        let mut cells: smallvec::SmallVec<[Cell; 8]> = smallvec::SmallVec::with_capacity(n);
        let mut absent = 0usize;
        for (i, &pos) in self.cols.iter().enumerate() {
            let cell = super::read_seg_cell(self.seg, pos, row_id, &mut self.caches[i])?;
            absent += usize::from(matches!(cell, Cell::Null));
            cells.push(cell);
        }
        if absent == 0 {
            return Ok(Row::from_parts(Arc::clone(shape), cells));
        }
        if absent == n {
            return Ok(Row::new());
        }
        let present_cols: smallvec::SmallVec<[usize; 8]> = self
            .cols
            .iter()
            .zip(cells.iter())
            .filter(|(_, c)| !matches!(c, Cell::Null))
            .map(|(&pos, _)| pos)
            .collect();
        Ok(Row::from_sorted(
            present_cols.iter().map(|&pos| self.seg.cols[pos].0.as_str()),
            cells.into_iter().filter(|c| !matches!(c, Cell::Null)),
        ))
    }

    #[inline]
    pub(super) fn materialize_into(&mut self, row_id: u32, dst: &mut Row) -> Result<(), Error> {
        let Some(ref shape) = self.shape else {
            dst.clear();
            return Ok(());
        };
        let n = self.cols.len();
        if dst.shape().is_some_and(|s| Arc::ptr_eq(s, shape)) {
            let dst_cells = dst.cells_mut();
            let mut first_null: Option<usize> = None;
            for i in 0..n {
                let pos = self.cols[i];
                if !super::write_seg_cell_in_place(
                    self.seg,
                    pos,
                    row_id,
                    &mut self.caches[i],
                    &mut dst_cells[i],
                )? {
                    first_null = Some(i);
                    break;
                }
            }
            let Some(null_idx) = first_null else {
                return Ok(());
            };
            let mut present_pos: smallvec::SmallVec<[usize; 8]> =
                smallvec::SmallVec::with_capacity(n - 1);
            let mut present_cells: smallvec::SmallVec<[Cell; 8]> =
                smallvec::SmallVec::with_capacity(n - 1);
            for i in 0..null_idx {
                present_pos.push(self.cols[i]);
                present_cells.push(std::mem::replace(&mut dst_cells[i], Cell::Null));
            }
            for i in (null_idx + 1)..n {
                let pos = self.cols[i];
                let cell = super::read_seg_cell(self.seg, pos, row_id, &mut self.caches[i])?;
                if !matches!(cell, Cell::Null) {
                    present_pos.push(pos);
                    present_cells.push(cell);
                }
            }
            if present_cells.is_empty() {
                dst.clear();
            } else {
                *dst = Row::from_sorted(
                    present_pos.iter().map(|&pos| self.seg.cols[pos].0.as_str()),
                    present_cells,
                );
            }
            return Ok(());
        }

        let mut cells: smallvec::SmallVec<[Cell; 8]> = smallvec::SmallVec::with_capacity(n);
        let mut absent = 0usize;
        for (i, &pos) in self.cols.iter().enumerate() {
            let cell = super::read_seg_cell(self.seg, pos, row_id, &mut self.caches[i])?;
            absent += usize::from(matches!(cell, Cell::Null));
            cells.push(cell);
        }
        if absent == 0 {
            dst.reset_with_shape_ref(shape, cells);
        } else if absent == n {
            dst.clear();
        } else {
            let present_cols: smallvec::SmallVec<[usize; 8]> = self
                .cols
                .iter()
                .zip(cells.iter())
                .filter(|(_, c)| !matches!(c, Cell::Null))
                .map(|(&pos, _)| pos)
                .collect();
            *dst = Row::from_sorted(
                present_cols.iter().map(|&pos| self.seg.cols[pos].0.as_str()),
                cells.into_iter().filter(|c| !matches!(c, Cell::Null)),
            );
        }
        Ok(())
    }
}

#[inline]
fn verify_slice_crc(
    seg_id: u64,
    fname: &str,
    bytes: &[u8],
    exp_len: u64,
    exp_crc: u32,
) -> Result<(), Error> {
    let got_len = bytes.len() as u64;
    let got_crc = crate::crc::hardware_crc32c(bytes);
    if got_len != exp_len || got_crc != exp_crc {
        return Err(Error::runtime(format!(
            "corrupt segment file 'seg_{seg_id:08}/{fname}': expected (len={exp_len}, crc={exp_crc:#010x}), got (len={got_len}, crc={got_crc:#010x})"
        )));
    }
    Ok(())
}

impl DiskSegment {
    pub(super) fn open(seg_dir: &Path, meta: SegmentMeta) -> Result<Self, Error> {
        Self::open_inner(seg_dir, meta, true)
    }

    pub(super) fn open_newly_written(seg_dir: &Path, meta: SegmentMeta) -> Result<Self, Error> {
        Self::open_inner(seg_dir, meta, false)
    }

    fn open_inner(seg_dir: &Path, meta: SegmentMeta, verify_crcs: bool) -> Result<Self, Error> {
        let check_crcs = verify_crcs && !meta.file_crcs.is_empty();
        let skip_tgram_body_crc = !meta.file_crcs.is_empty();
        let mut checked_files = 0usize;
        let seg_id = meta.seg_id;

        let mut cols = Vec::with_capacity(meta.columns.len());
        let mut col_pos = FxHashMap::default();
        col_pos.reserve(meta.columns.len());
        for (i, col_schema) in meta.columns.iter().enumerate() {
            let cname = &col_schema.name;
            let mcol = match col_schema.kind {
                DiskColKind::Int64
                | DiskColKind::Float64
                | DiskColKind::Bool
                | DiskColKind::Time64 => {
                    let mut fc =
                        MmapFixedCol::open(seg_dir, cname, col_schema.kind, meta.row_count)?;
                    if check_crcs {
                        let null_name = format!("col_{cname}.null");
                        if let Some(&(el, ec)) = meta.file_crcs.get(&null_name) {
                            verify_slice_crc(seg_id, &null_name, fc.nulls_bytes(), el, ec)?;
                            checked_files += 1;
                        }
                        let val_name = format!("col_{cname}.val");
                        if let Some(&(el, ec)) = meta.file_crcs.get(&val_name) {
                            verify_slice_crc(seg_id, &val_name, fc.values_bytes(), el, ec)?;
                            checked_files += 1;
                        }
                        let zmap_name = format!("col_{cname}.zmap");
                        if let Some(&(el, ec)) = meta.file_crcs.get(&zmap_name) {
                            checked_files += 1;
                            match fc.zmap_bytes() {
                                Some(zb)
                                    if zb.len() as u64 == el
                                        && crate::crc::hardware_crc32c(zb) == ec => {}
                                _ => fc.clear_zone_map(),
                            }
                        }
                    }
                    MmapCol::Fixed(fc)
                }
                DiskColKind::TextZstd => {
                    let mut tc = MmapTextCol::open_with_opts(
                        seg_dir,
                        cname,
                        meta.row_count,
                        skip_tgram_body_crc,
                    )?;
                    if check_crcs {
                        let loc_name = format!("col_{cname}.loc");
                        if let Some(&(el, ec)) = meta.file_crcs.get(&loc_name) {
                            verify_slice_crc(seg_id, &loc_name, tc.row_loc_bytes(), el, ec)?;
                            checked_files += 1;
                        }
                        let boff_name = format!("col_{cname}.boff");
                        if let Some(&(el, ec)) = meta.file_crcs.get(&boff_name) {
                            verify_slice_crc(seg_id, &boff_name, tc.block_offsets_bytes(), el, ec)?;
                            checked_files += 1;
                        }
                        let bdat_name = format!("col_{cname}.bdat");
                        if let Some(&(el, ec)) = meta.file_crcs.get(&bdat_name) {
                            verify_slice_crc(seg_id, &bdat_name, tc.blocks_data_bytes(), el, ec)?;
                            checked_files += 1;
                        }
                        let tgram_name = format!("col_{cname}.tgram");
                        if let Some(&(el, ec)) = meta.file_crcs.get(&tgram_name) {
                            checked_files += 1;
                            match tc.tgram_bytes() {
                                Some(tb)
                                    if tb.len() as u64 == el
                                        && crate::crc::hardware_crc32c(tb) == ec => {}
                                _ => tc.clear_tgram(),
                            }
                        }
                    }
                    MmapCol::Text(tc)
                }
                DiskColKind::TextDict => {
                    let dc = MmapDictCol::open_checked(
                        seg_dir,
                        cname,
                        meta.row_count,
                        seg_id,
                        if check_crcs {
                            Some(&meta.file_crcs)
                        } else {
                            None
                        },
                    )?;
                    if check_crcs {
                        for ext in ["dstr", "doff", "dord"] {
                            if meta.file_crcs.contains_key(&format!("col_{cname}.{ext}")) {
                                checked_files += 1;
                            }
                        }
                    }
                    MmapCol::Dict(dc)
                }
                DiskColKind::VecF32Sq8 { dim } => {
                    let mut vc = MmapVecCol::open(seg_dir, cname, dim, meta.row_count)?;
                    if check_crcs {
                        let vnull_name = format!("col_{cname}.vnull");
                        if let Some(&(el, ec)) = meta.file_crcs.get(&vnull_name) {
                            verify_slice_crc(seg_id, &vnull_name, vc.nulls_bytes(), el, ec)?;
                            checked_files += 1;
                        }
                        let vf32_name = format!("col_{cname}.vf32");
                        if let Some(&(el, ec)) = meta.file_crcs.get(&vf32_name) {
                            verify_slice_crc(seg_id, &vf32_name, vc.f32_bytes(), el, ec)?;
                            checked_files += 1;
                        }
                        let vsq8_name = format!("col_{cname}.vsq8");
                        if let Some(&(el, ec)) = meta.file_crcs.get(&vsq8_name) {
                            verify_slice_crc(seg_id, &vsq8_name, vc.sq8_bytes(), el, ec)?;
                            checked_files += 1;
                        }
                        let vivf_name = format!("col_{cname}.vivf");
                        if let Some(&(el, ec)) = meta.file_crcs.get(&vivf_name) {
                            checked_files += 1;
                            match vc.ivf_bytes() {
                                Some(ib)
                                    if ib.len() as u64 == el
                                        && crate::crc::hardware_crc32c(ib) == ec => {}
                                _ => vc.ivf = None,
                            }
                        }
                    }
                    MmapCol::Vec(vc)
                }
            };
            col_pos.insert(col_schema.name.clone(), i);
            cols.push((col_schema.name.clone(), mcol));
        }

        let pk_index = MmapHashIndex::open(&seg_dir.join("pk_id.ht"))?;
        if check_crcs && let Some(&(el, ec)) = meta.file_crcs.get("pk_id.ht") {
            verify_slice_crc(seg_id, "pk_id.ht", pk_index.as_bytes(), el, ec)?;
            checked_files += 1;
        }
        let mut sec_indexes = FxHashMap::default();
        for field in &meta.indexed_fields {
            let sec_name = format!("sec_{field}.ht");
            let p = seg_dir.join(&sec_name);
            match MmapHashIndex::open(&p) {
                Ok(idx) => {
                    if check_crcs && let Some(&(el, ec)) = meta.file_crcs.get(&sec_name) {
                        verify_slice_crc(seg_id, &sec_name, idx.as_bytes(), el, ec)?;
                        checked_files += 1;
                    }
                    sec_indexes.insert(field.clone(), idx);
                }
                Err(_) if !p.exists() => {
                    if check_crcs && meta.file_crcs.contains_key(&sec_name) {
                        checked_files += 1;
                    }
                }
                Err(e) => return Err(e),
            }
        }

        // Fallback check only if `meta.file_crcs` listed any extra unmapped file.
        if check_crcs && checked_files < meta.file_crcs.len() {
            let _ = verify_segment_file_crcs(seg_dir, seg_id, &meta.file_crcs)?;
        }

        let mut name_order: Vec<usize> = (0..cols.len()).collect();
        name_order.sort_unstable_by(|&a, &b| cols[a].0.cmp(&cols[b].0));
        let names: Vec<&str> = cols.iter().map(|(n, _)| n.as_str()).collect();
        let full_shape = Row::shape_of(&names);
        let id_col_pos = col_pos.get("id").copied();
        let title_col_pos = col_pos.get("title").copied();
        let can_cache_full_row =
            cols.len() <= 16 && !cols.iter().any(|(_, c)| matches!(c, MmapCol::Vec(_)));

        Ok(Self {
            meta,
            cols,
            col_pos,
            pk_index,
            sec_indexes,
            name_order,
            full_shape,
            id_col_pos,
            title_col_pos,
            can_cache_full_row,
        })
    }

    pub(super) fn prepare_projection<'s>(&'s self, fields: &[&str]) -> SegProjPlan<'s> {
        let mut cols: smallvec::SmallVec<[usize; 8]> = smallvec::SmallVec::with_capacity(fields.len());
        for &fname in fields {
            if let Some(&pos) = self.col_pos.get(fname) {
                cols.push(pos);
            }
        }
        cols.sort_unstable_by(|&a, &b| self.cols[a].0.cmp(&self.cols[b].0));
        cols.dedup();
        let shape = if cols.is_empty() {
            None
        } else if cols.len() == 2
            && Some(cols[0]) == self.id_col_pos
            && Some(cols[1]) == self.title_col_pos
        {
            Some(Arc::clone(crate::store::shape_id_title()))
        } else if cols.as_slice() == self.name_order.as_slice() {
            Some(Arc::clone(&self.full_shape))
        } else {
            let sorted_names: smallvec::SmallVec<[&str; 8]> =
                cols.iter().map(|&pos| self.cols[pos].0.as_str()).collect();
            Some(Row::shape_of(&sorted_names))
        };
        let caches = (0..cols.len())
            .map(|_| super::SegBlockCache::Empty)
            .collect();
        SegProjPlan {
            seg: self,
            cols,
            shape,
            caches,
        }
    }

    #[inline]
    pub fn materialize_row(&self, row_id: u32) -> Result<Row, Error> {
        self.materialize_row_skipping(row_id, &[])
    }

    #[inline]
    pub fn materialize_row_skipping(&self, row_id: u32, skip: &[&str]) -> Result<Row, Error> {
        let mut cells: smallvec::SmallVec<[Cell; 8]> =
            smallvec::SmallVec::with_capacity(self.name_order.len());
        let mut absent = 0usize;
        for &pos in &self.name_order {
            let cell = if !skip.is_empty() && skip.contains(&self.cols[pos].0.as_str()) {
                Cell::Null
            } else {
                self.cols[pos].1.get_cell(row_id)?
            };
            absent += usize::from(matches!(cell, Cell::Null));
            cells.push(cell);
        }
        if absent == cells.len() {
            return Ok(Row::new());
        }
        if absent == 0 {
            return Ok(Row::from_parts(Arc::clone(&self.full_shape), cells));
        }
        // Absent values are not stored in a row: keep the present ones, in name order.
        let present: smallvec::SmallVec<[usize; 8]> = self
            .name_order
            .iter()
            .zip(cells.iter())
            .filter(|(_, c)| !matches!(c, Cell::Null))
            .map(|(&pos, _)| pos)
            .collect();
        Ok(Row::from_sorted(
            present.iter().map(|&pos| self.cols[pos].0.as_str()),
            cells.into_iter().filter(|c| !matches!(c, Cell::Null)),
        ))
    }

    #[inline]
    pub fn materialize_projected(&self, row_id: u32, fields: &[&str]) -> Result<Row, Error> {
        if fields.len() == 1 {
            let f0 = fields[0];
            let Some(&pos) = self.col_pos.get(f0) else {
                return Ok(Row::new());
            };
            let cell = self.cols[pos].1.get_cell(row_id)?;
            if matches!(cell, Cell::Null) {
                return Ok(Row::new());
            }
            return Ok(Row::from_sorted(std::iter::once(f0), std::iter::once(cell)));
        }
        if fields == ["id", "title"]
            && let (Some(id_pos), Some(title_pos)) = (self.id_col_pos, self.title_col_pos)
        {
            let c_id = self.cols[id_pos].1.get_cell(row_id)?;
            let c_title = self.cols[title_pos].1.get_cell(row_id)?;
            if !matches!(c_id, Cell::Null) && !matches!(c_title, Cell::Null) {
                return Ok(Row::from_parts(
                    Arc::clone(crate::store::shape_id_title()),
                    [c_id, c_title],
                ));
            }
        }
        let mut present: smallvec::SmallVec<[(&str, Cell); 8]> =
            smallvec::SmallVec::with_capacity(fields.len());
        for &fname in fields {
            if let Some(&pos) = self.col_pos.get(fname) {
                let cell = self.cols[pos].1.get_cell(row_id)?;
                if !matches!(cell, Cell::Null) {
                    present.push((fname, cell));
                }
            }
        }
        if present.is_empty() {
            return Ok(Row::new());
        }
        present.sort_by(|a, b| a.0.cmp(b.0));
        // Keep last value on duplicate field names, matching repeated `Row::insert`.
        if present.windows(2).any(|w| w[0].0 == w[1].0) {
            let mut deduped: smallvec::SmallVec<[(&str, Cell); 8]> =
                smallvec::SmallVec::with_capacity(present.len());
            for (k, v) in present {
                if let Some(last) = deduped.last_mut().filter(|(lk, _)| *lk == k) {
                    last.1 = v;
                } else {
                    deduped.push((k, v));
                }
            }
            present = deduped;
        }
        let names: smallvec::SmallVec<[&str; 8]> = present.iter().map(|&(k, _)| k).collect();
        Ok(Row::from_sorted(
            names.into_iter(),
            present.into_iter().map(|(_, v)| v),
        ))
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
                let s_off = in_off as usize;
                let e_off = s_off + (len as usize);
                if let Some((raw, utf8_ok)) = tc.raw_uncompressed_block_checked(bidx)? {
                    let s = MmapTextCol::slice_block_str(raw, utf8_ok, s_off, e_off)?;
                    return Ok(Some(s));
                }
                let need_load = match last_block {
                    Some((lb, _)) => *lb != bidx,
                    None => true,
                };
                if need_load {
                    let (decoded, _) = tc.decode_block_checked(bidx)?;
                    *last_block = Some((bidx, decoded));
                }
                let (_, ref block) = *last_block.as_ref().unwrap();
                let utf8_ok = tc.block_is_utf8_cached(bidx, block);
                let s = MmapTextCol::slice_block_str(block, utf8_ok, s_off, e_off)?;
                Ok(Some(s))
            }
            _ => Ok(None),
        }
    }

    #[inline]
    pub(super) fn borrow_col_str_cached<'a, 's: 'a>(
        &'s self,
        col_pos: Option<usize>,
        rid: u32,
        cache: &'a mut super::SegBlockCache<'s>,
    ) -> Result<Option<&'a str>, Error> {
        let Some(pos) = col_pos else {
            return Ok(None);
        };
        super::borrow_seg_str_cached(self, pos, rid, cache)
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
    // A column typed `Int64` from its first values may later receive fractional numbers
    // (`f64` fields accept integer literals). Writing those into an integer file would store
    // them as nulls, so the segment's column widens to `Float64` instead.
    let kind = if col.kind == DiskColKind::Int64 && src.has_float() {
        DiskColKind::Float64
    } else {
        col.kind
    };
    match kind {
        DiskColKind::Int64 | DiskColKind::Time64 => {
            #[cfg(target_endian = "little")]
            if let ColSrc::Col(
                crate::batch::Column::Int { vals, valid }
                | crate::batch::Column::Time { vals, valid },
            ) = src
                && valid.null_count() == 0
            {
                let nulls = vec![0u8; vals.len()];
                let vals_bytes = unsafe {
                    std::slice::from_raw_parts(vals.as_ptr() as *const u8, vals.len() * 8)
                };
                fs::write(seg_dir.join(format!("col_{}.null", col.name)), &nulls)
                    .map_err(io_err)?;
                fs::write(seg_dir.join(format!("col_{}.val", col.name)), vals_bytes)
                    .map_err(io_err)?;
                write_zone_map(seg_dir, &col.name, kind, &nulls, vals_bytes)?;
                return Ok(kind);
            }
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
            write_zone_map(seg_dir, &col.name, kind, &nulls, &vals)?;
            Ok(kind)
        }
        DiskColKind::Float64 => {
            #[cfg(target_endian = "little")]
            if let ColSrc::Col(crate::batch::Column::Float { vals, valid }) = src
                && valid.null_count() == 0
            {
                let nulls = vec![0u8; vals.len()];
                let vals_bytes = unsafe {
                    std::slice::from_raw_parts(vals.as_ptr() as *const u8, vals.len() * 8)
                };
                fs::write(seg_dir.join(format!("col_{}.null", col.name)), &nulls)
                    .map_err(io_err)?;
                fs::write(seg_dir.join(format!("col_{}.val", col.name)), vals_bytes)
                    .map_err(io_err)?;
                write_zone_map(seg_dir, &col.name, DiskColKind::Float64, &nulls, vals_bytes)?;
                return Ok(DiskColKind::Float64);
            }
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
                    if row_idx == 2048 && src.len() >= 4096 && dict_list.len() > 1920 {
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
                    fs::write(seg_dir.join(format!("col_{}.dstr", col.name)), &dstr_bytes)
                        .map_err(io_err)?;
                    fs::write(seg_dir.join(format!("col_{}.doff", col.name)), &doff_bytes)
                        .map_err(io_err)?;
                    #[cfg(target_endian = "little")]
                    {
                        let dord_bytes = unsafe {
                            std::slice::from_raw_parts(codes.as_ptr() as *const u8, codes.len() * 2)
                        };
                        fs::write(seg_dir.join(format!("col_{}.dord", col.name)), dord_bytes)
                            .map_err(io_err)?;
                    }
                    #[cfg(not(target_endian = "little"))]
                    {
                        let mut dord_bytes = Vec::with_capacity(codes.len() * 2);
                        for &c in &codes {
                            dord_bytes.extend_from_slice(&c.to_le_bytes());
                        }
                        fs::write(seg_dir.join(format!("col_{}.dord", col.name)), &dord_bytes)
                            .map_err(io_err)?;
                    }
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
            let mut cur_tgram: super::columns::TgramBlock = [0u64; super::columns::TGRAM_WORDS];
            let mut tgram_blocks: Vec<super::columns::TgramBlock> = Vec::new();

            #[cfg(feature = "compress-zstd")]
            let mut zstd_comp = zstd::bulk::Compressor::new(1).ok();
            #[cfg(feature = "compress-zstd")]
            let mut comp_buf: Vec<u8> = Vec::new();

            #[allow(unused_mut)]
            let mut flush_block = |raw: &mut Vec<u8>,
                                   bdat: &mut BufWriter<File>,
                                   cur_len: &mut u64,
                                   boffs: &mut Vec<u64>,
                                   bidx: &mut u32,
                                   tgram: &mut super::columns::TgramBlock,
                                   tgrams: &mut Vec<super::columns::TgramBlock>|
             -> Result<(), Error> {
                if raw.is_empty() {
                    return Ok(());
                }
                let written_len: usize;
                #[cfg(feature = "compress-zstd")]
                {
                    let bound = zstd::zstd_safe::compress_bound(raw.len());
                    if comp_buf.len() < bound {
                        comp_buf.resize(bound, 0);
                    }
                    let comp_len = if let Some(c) = zstd_comp.as_mut() {
                        c.compress_to_buffer(&raw[..], &mut comp_buf[..bound])
                            .map_err(io_err)?
                    } else {
                        let v = zstd::encode_all(&raw[..], 1).map_err(io_err)?;
                        if comp_buf.len() < v.len() {
                            comp_buf.resize(v.len(), 0);
                        }
                        comp_buf[..v.len()].copy_from_slice(&v);
                        v.len()
                    };
                    if comp_len + 1 < raw.len() {
                        bdat.write_all(&[0x01]).map_err(io_err)?;
                        bdat.write_all(&comp_buf[..comp_len]).map_err(io_err)?;
                        written_len = 1 + comp_len;
                    } else {
                        bdat.write_all(&[0x00]).map_err(io_err)?;
                        bdat.write_all(raw).map_err(io_err)?;
                        written_len = 1 + raw.len();
                    }
                }
                #[cfg(not(feature = "compress-zstd"))]
                {
                    bdat.write_all(&[0x00]).map_err(io_err)?;
                    bdat.write_all(raw).map_err(io_err)?;
                    written_len = 1 + raw.len();
                }

                *cur_len += written_len as u64;
                boffs.push(*cur_len);
                tgrams.push(std::mem::replace(
                    tgram,
                    [0u64; super::columns::TGRAM_WORDS],
                ));
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
                                &mut cur_tgram,
                                &mut tgram_blocks,
                            )?;
                        }
                        super::columns::add_str_trigrams(&mut cur_tgram, s);
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
                &mut cur_tgram,
                &mut tgram_blocks,
            )?;
            bdat_file.flush().map_err(io_err)?;

            fs::write(seg_dir.join(format!("col_{}.loc", col.name)), &loc_bytes).map_err(io_err)?;
            #[cfg(target_endian = "little")]
            {
                let boff_bytes = unsafe {
                    std::slice::from_raw_parts(
                        block_offsets.as_ptr() as *const u8,
                        block_offsets.len() * 8,
                    )
                };
                fs::write(seg_dir.join(format!("col_{}.boff", col.name)), boff_bytes)
                    .map_err(io_err)?;
            }
            #[cfg(not(target_endian = "little"))]
            {
                let mut boff_bytes = Vec::with_capacity(block_offsets.len() * 8);
                for &off in &block_offsets {
                    boff_bytes.extend_from_slice(&off.to_le_bytes());
                }
                fs::write(seg_dir.join(format!("col_{}.boff", col.name)), &boff_bytes)
                    .map_err(io_err)?;
            }
            let tgram_bytes = super::columns::encode_tgram(&tgram_blocks);
            fs::write(seg_dir.join(format!("col_{}.tgram", col.name)), &tgram_bytes)
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
    let mut pk_map: FxHashMap<Vec<u8>, smallvec::SmallVec<[u32; 1]>> = FxHashMap::default();
    pk_map.reserve(ids.len());
    let mut key_buf = Vec::with_capacity(64);
    for i in 0..ids.len() {
        if let Some(id_cell) = ids.key_cell(i)
            && encode_index_cell_key(&id_cell, &mut key_buf)
        {
            if let Some(v) = pk_map.get_mut(key_buf.as_slice()) {
                v.push(i as u32);
            } else {
                pk_map.insert(key_buf.clone(), smallvec::smallvec![i as u32]);
            }
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
            if let Some(v) = sec_map.get_mut(key_buf.as_slice()) {
                v.push(i as u32);
            } else {
                sec_map.insert(key_buf.clone(), vec![i as u32]);
            }
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

    /// True when some value of the column is a float.
    pub(super) fn has_float(&self) -> bool {
        use crate::batch::Column;
        match self {
            ColSrc::Col(Column::Float { vals, valid }) => valid.null_count() < vals.len(),
            ColSrc::Col(col) if col.is_typed() => false,
            ColSrc::Absent(_) => false,
            _ => (0..self.len()).any(|i| matches!(self.cell_ref(i), Some(Cell::Float(_)))),
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
        match self {
            ColSrc::Col(c) => c.text_at(i),
            _ => self.cell_ref(i).and_then(Cell::text),
        }
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

    let file_crcs = compute_segment_file_crcs(&seg_dir)?;

    Ok(SegmentMeta {
        seg_id,
        table: schema.name.clone(),
        row_count,
        columns: seg_columns,
        indexed_fields: schema.indexed_fields.clone(),
        file_crcs,
    })
}

fn crc32c_file(path: &Path) -> Result<(u64, u32), Error> {
    let file = File::open(path).map_err(io_err)?;
    let len = file.metadata().map_err(io_err)?.len();
    if len == 0 {
        return Ok((0, crate::crc::hardware_crc32c(&[])));
    }
    // SAFETY: segment files are immutable once written; mapping read-only avoids heap copies.
    let mmap = unsafe { memmap2::MmapOptions::new().map(&file).map_err(io_err)? };
    Ok((len, crate::crc::hardware_crc32c(&mmap)))
}

fn compute_segment_file_crcs(
    seg_dir: &Path,
) -> Result<std::collections::BTreeMap<String, (u64, u32)>, Error> {
    let mut out = std::collections::BTreeMap::new();
    for entry in fs::read_dir(seg_dir).map_err(io_err)? {
        let entry = entry.map_err(io_err)?;
        if entry.file_type().map_err(io_err)?.is_file() {
            let fname = entry.file_name().to_string_lossy().into_owned();
            let crc = crc32c_file(&entry.path())?;
            out.insert(fname, crc);
        }
    }
    Ok(out)
}

fn verify_segment_file_crcs(
    seg_dir: &Path,
    seg_id: u64,
    expected: &std::collections::BTreeMap<String, (u64, u32)>,
) -> Result<rustc_hash::FxHashSet<String>, Error> {
    let mut corrupt_hints = rustc_hash::FxHashSet::default();
    for (fname, &(exp_len, exp_crc)) in expected {
        let is_hint =
            fname.ends_with(".tgram") || fname.ends_with(".zmap") || fname.ends_with(".vivf");
        let is_sec_idx = fname.starts_with("sec_") && fname.ends_with(".ht");
        let path = seg_dir.join(fname);
        if !path.exists() {
            if is_hint || is_sec_idx {
                continue;
            }
            return Err(Error::runtime(format!(
                "missing segment file 'seg_{seg_id:08}/{fname}'"
            )));
        }
        let (got_len, got_crc) = crc32c_file(&path)?;
        if got_len != exp_len || got_crc != exp_crc {
            if is_hint {
                corrupt_hints.insert(fname.clone());
                continue;
            }
            return Err(Error::runtime(format!(
                "corrupt segment file 'seg_{seg_id:08}/{fname}': expected (len={exp_len}, crc={exp_crc:#010x}), got (len={got_len}, crc={got_crc:#010x})"
            )));
        }
    }
    Ok(corrupt_hints)
}
