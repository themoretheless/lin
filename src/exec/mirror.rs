//! Declared and schema-driven column mirrors (`ColMirror` / `TableSoa`): a lazily rebuilt
//! columnar copy of scalar and text fields.
//!
//! Homogeneous scalar columns (`Int`, `Float`, `Time`, `Bool`) are stored as dense typed vectors
//! plus a `Validity` bitmask so high-cardinality numeric/timestamp columns never fail at
//! `MAX_DICT` and project into `RecordBatch` in `O(1)` buffer clones/gathers. Low-cardinality
//! text or mixed scalar columns use compact dictionary codes (`<= MAX_DICT`), while
//! high-cardinality text columns spill to `Column::Text` (`DenseText`). Rows stay the source
//! of truth: a mirror is valid only for the row-store epoch it was built at.

use std::sync::Arc;

use rustc_hash::FxHashMap;
use smallvec::SmallVec;

use super::eval::{eval_leaf, value_cell};
use crate::ast::{CmpOp, Pred};
use crate::batch::{Column, RecordBatch, Validity};
use crate::store::row::FieldSlot;
use crate::store::{Cell, Row};

/// Above this many distinct values a dictionary column spills to `DenseText` (if text)
/// or declines mirroring (if heterogeneous mixed scalars).
const MAX_DICT: usize = 1 << 16;

struct TextDictMeta {
    values: Vec<Arc<str>>,
    /// `None` when every dictionary entry is non-null text (`codes` map 1-to-1 to `values`).
    /// `Some(remap)` when `Cell::Null` appears in `dict`.
    code_remap: Option<Vec<Option<u32>>>,
}

enum MirrorStorage {
    AllNull,
    DenseInt {
        vals: Vec<i64>,
        valid: Validity,
    },
    DenseFloat {
        vals: Vec<f64>,
        valid: Validity,
    },
    DenseTime {
        vals: Vec<i64>,
        valid: Validity,
    },
    DenseBool {
        vals: Vec<bool>,
        valid: Validity,
    },
    DenseText(Column),
    Dict {
        codes: Vec<u32>,
        dict: Vec<Cell>,
        text_meta: Option<TextDictMeta>,
    },
}

pub(crate) struct FieldMirror {
    storage: MirrorStorage,
}

impl FieldMirror {
    fn build(rows: &[Row], name: &str, force_text_dict: bool) -> Option<Self> {
        let n = rows.len();
        if n == 0 {
            return Some(FieldMirror {
                storage: MirrorStorage::AllNull,
            });
        }

        let mut seen_int = false;
        let mut seen_float = false;
        let mut seen_time = false;
        let mut seen_bool = false;
        let mut seen_text = false;
        let mut has_short_arc_text = false;
        let mut text_bytes = 0usize;
        let mut slot = FieldSlot::new(name);
        for row in rows {
            match slot.get(row).unwrap_or(&Cell::Null) {
                Cell::Null => {}
                Cell::Int(_) => seen_int = true,
                Cell::Float(_) => seen_float = true,
                Cell::Time(_) => seen_time = true,
                Cell::Bool(_) => seen_bool = true,
                Cell::Text(s) => {
                    seen_text = true;
                    if s.len() <= 22 {
                        has_short_arc_text = true;
                    }
                    text_bytes = text_bytes.saturating_add(s.len());
                }
                Cell::SmallText(s) => {
                    seen_text = true;
                    text_bytes = text_bytes.saturating_add(s.len());
                }
                Cell::Vec(_) => return None,
            }
        }

        let type_count = u8::from(seen_int)
            + u8::from(seen_float)
            + u8::from(seen_time)
            + u8::from(seen_bool)
            + u8::from(seen_text);

        if type_count == 0 {
            return Some(FieldMirror {
                storage: MirrorStorage::AllNull,
            });
        }

        if type_count == 1 && !seen_text {
            let mut slot = FieldSlot::new(name);
            if seen_int {
                let mut vals = Vec::with_capacity(n);
                let mut valid = Validity::default();
                for row in rows {
                    match slot.get(row).unwrap_or(&Cell::Null) {
                        Cell::Int(v) => {
                            vals.push(*v);
                            valid.push(true);
                        }
                        _ => {
                            vals.push(0);
                            valid.push(false);
                        }
                    }
                }
                return Some(FieldMirror {
                    storage: MirrorStorage::DenseInt { vals, valid },
                });
            }
            if seen_float {
                let mut vals = Vec::with_capacity(n);
                let mut valid = Validity::default();
                for row in rows {
                    match slot.get(row).unwrap_or(&Cell::Null) {
                        Cell::Float(v) => {
                            vals.push(*v);
                            valid.push(true);
                        }
                        _ => {
                            vals.push(0.0);
                            valid.push(false);
                        }
                    }
                }
                return Some(FieldMirror {
                    storage: MirrorStorage::DenseFloat { vals, valid },
                });
            }
            if seen_time {
                let mut vals = Vec::with_capacity(n);
                let mut valid = Validity::default();
                for row in rows {
                    match slot.get(row).unwrap_or(&Cell::Null) {
                        Cell::Time(v) => {
                            vals.push(*v);
                            valid.push(true);
                        }
                        _ => {
                            vals.push(0);
                            valid.push(false);
                        }
                    }
                }
                return Some(FieldMirror {
                    storage: MirrorStorage::DenseTime { vals, valid },
                });
            }
            if seen_bool {
                let mut vals = Vec::with_capacity(n);
                let mut valid = Validity::default();
                for row in rows {
                    match slot.get(row).unwrap_or(&Cell::Null) {
                        Cell::Bool(v) => {
                            vals.push(*v);
                            valid.push(true);
                        }
                        _ => {
                            vals.push(false);
                            valid.push(false);
                        }
                    }
                }
                return Some(FieldMirror {
                    storage: MirrorStorage::DenseBool { vals, valid },
                });
            }
        }

        let mut index: FxHashMap<Cell, u32> = FxHashMap::default();
        let mut dict: Vec<Cell> = Vec::new();
        let mut codes = Vec::with_capacity(n);
        let mut slot = FieldSlot::new(name);
        let mut dict_overflow = false;
        for row in rows {
            let cell = slot.get(row).unwrap_or(&Cell::Null);
            let code = if let Some(&c) = index.get(cell) {
                c
            } else {
                if dict.len() >= MAX_DICT {
                    dict_overflow = true;
                    break;
                }
                let next = dict.len() as u32;
                let cloned = cell.clone_for_borrow();
                dict.push(cloned.clone_for_borrow());
                index.insert(cloned, next);
                next
            };
            codes.push(code);
        }

        if dict_overflow {
            if type_count == 1 && seen_text {
                drop(index);
                drop(dict);
                drop(codes);
                let mut slot = FieldSlot::new(name);
                let col = if force_text_dict || !has_short_arc_text {
                    let mut c = Column::new_text_with_capacity(n, text_bytes);
                    for row in rows {
                        let cell = slot.get(row).unwrap_or(&Cell::Null);
                        c.push(cell.clone_for_borrow());
                    }
                    c
                } else {
                    let mut cells = Vec::with_capacity(n);
                    for row in rows {
                        let cell = slot.get(row).unwrap_or(&Cell::Null);
                        cells.push(cell.clone_for_borrow());
                    }
                    Column::Cells(cells)
                };
                return Some(FieldMirror {
                    storage: MirrorStorage::DenseText(col),
                });
            }
            return None;
        }

        let text_meta = if type_count == 1 && seen_text && (force_text_dict || !has_short_arc_text) {
            let has_null = dict.iter().any(|c| matches!(c, Cell::Null));
            if !has_null {
                let values = dict
                    .iter()
                    .map(|c| {
                        c.text_shared()
                            .unwrap_or_else(|| Arc::from(c.text().unwrap_or("")))
                    })
                    .collect();
                Some(TextDictMeta {
                    values,
                    code_remap: None,
                })
            } else {
                let mut values = Vec::with_capacity(dict.len() - 1);
                let mut remap = Vec::with_capacity(dict.len());
                for c in &dict {
                    match c {
                        Cell::Null => remap.push(None),
                        other => {
                            let idx = values.len() as u32;
                            values.push(
                                other
                                    .text_shared()
                                    .unwrap_or_else(|| Arc::from(other.text().unwrap_or(""))),
                            );
                            remap.push(Some(idx));
                        }
                    }
                }
                Some(TextDictMeta {
                    values,
                    code_remap: Some(remap),
                })
            }
        } else {
            None
        };

        Some(FieldMirror {
            storage: MirrorStorage::Dict {
                codes,
                dict,
                text_meta,
            },
        })
    }
}

#[inline]
fn gather_dense<T: Copy>(vals: &[T], valid: &Validity, ix: &[usize], default: T) -> (Vec<T>, Validity) {
    let mut out_vals = Vec::with_capacity(ix.len());
    if valid.null_count() == 0 {
        for &r in ix {
            out_vals.push(vals[r]);
        }
        (out_vals, Validity::all_valid(ix.len()))
    } else {
        let mut out_valid = Validity::default();
        for &r in ix {
            if valid.is_valid(r) {
                out_vals.push(vals[r]);
                out_valid.push(true);
            } else {
                out_vals.push(default);
                out_valid.push(false);
            }
        }
        (out_vals, out_valid)
    }
}

#[inline]
fn gather_text_col_usize(col: &Column, ix: &[usize]) -> Column {
    if let Column::Cells(cells) = col {
        let mut out = Column::with_capacity(ix.len());
        for &r in ix {
            out.push(cells[r].clone_for_borrow());
        }
        return out;
    }
    if let Ok(max_ok) = u32::try_from(col.len()) {
        let _ = max_ok;
        let ix_u32: Vec<u32> = ix.iter().map(|&r| r as u32).collect();
        return col.gather_u32(&ix_u32);
    }
    let mut out = Column::new_text_with_capacity(ix.len(), ix.len() * 16);
    for &r in ix {
        out.push(col.get(r));
    }
    out
}

#[inline]
fn bits_from_valid(n: usize, valid: &Validity) -> Vec<u64> {
    let words = n.div_ceil(64);
    if words == 0 {
        return Vec::new();
    }
    if valid.null_count() == 0 {
        let mut bits = vec![u64::MAX; words];
        let tail = n % 64;
        if tail != 0 {
            bits[words - 1] = (1u64 << tail) - 1;
        }
        return bits;
    }
    if valid.null_count() == n {
        return vec![0u64; words];
    }
    let mut bits = vec![0u64; words];
    for (w, slot) in bits.iter_mut().enumerate() {
        let base = w * 64;
        let lim = (n - base).min(64);
        let mut word = 0u64;
        for b in 0..lim {
            if valid.is_valid(base + b) {
                word |= 1u64 << b;
            }
        }
        *slot = word;
    }
    bits
}

#[inline]
fn eval_dense_bits<T: Copy>(
    vals: &[T],
    valid: &Validity,
    null_matches: bool,
    mut pred: impl FnMut(T) -> bool,
) -> Vec<u64> {
    let n = vals.len();
    let words = n.div_ceil(64);
    if words == 0 {
        return Vec::new();
    }
    let mut bits = Vec::with_capacity(words);
    if valid.null_count() == 0 {
        let mut chunks = vals.chunks_exact(64);
        for chunk in &mut chunks {
            let mut word = 0u64;
            for (b, &v) in chunk.iter().enumerate() {
                word |= u64::from(pred(v)) << b;
            }
            bits.push(word);
        }
        let rem = chunks.remainder();
        if !rem.is_empty() {
            let mut word = 0u64;
            for (b, &v) in rem.iter().enumerate() {
                word |= u64::from(pred(v)) << b;
            }
            bits.push(word);
        }
    } else {
        for w in 0..words {
            let base = w * 64;
            let lim = (n - base).min(64);
            let mut word = 0u64;
            for b in 0..lim {
                let idx = base + b;
                let ok = if valid.is_valid(idx) {
                    pred(vals[idx])
                } else {
                    null_matches
                };
                word |= u64::from(ok) << b;
            }
            bits.push(word);
        }
    }
    bits
}

#[inline]
fn eval_i64_bits(vals: &[i64], valid: &Validity, op: CmpOp, rv: i64) -> Vec<u64> {
    match op {
        CmpOp::Eq => eval_dense_bits(vals, valid, false, |v| v == rv),
        CmpOp::Ne => eval_dense_bits(vals, valid, true, |v| v != rv),
        CmpOp::Lt => eval_dense_bits(vals, valid, false, |v| v < rv),
        CmpOp::Le => eval_dense_bits(vals, valid, false, |v| v <= rv),
        CmpOp::Gt => eval_dense_bits(vals, valid, false, |v| v > rv),
        CmpOp::Ge => eval_dense_bits(vals, valid, false, |v| v >= rv),
    }
}

#[inline]
fn eval_f64_bits(vals: &[f64], valid: &Validity, op: CmpOp, rf: f64, all_ones: impl FnOnce() -> Vec<u64>) -> Vec<u64> {
    let n = vals.len();
    let words = n.div_ceil(64);
    if rf.is_nan() {
        return if matches!(op, CmpOp::Ne) {
            all_ones()
        } else {
            vec![0u64; words]
        };
    }
    match op {
        CmpOp::Eq => eval_dense_bits(vals, valid, false, |v| v == rf),
        CmpOp::Ne => eval_dense_bits(vals, valid, true, |v| v != rf),
        CmpOp::Lt => eval_dense_bits(vals, valid, false, |v| v < rf),
        CmpOp::Le => eval_dense_bits(vals, valid, false, |v| v <= rf),
        CmpOp::Gt => eval_dense_bits(vals, valid, false, |v| v > rf),
        CmpOp::Ge => eval_dense_bits(vals, valid, false, |v| v >= rf),
    }
}

/// Mirrors of one collection's declared or schema-derived fields at one row-store epoch.
pub(crate) struct ColMirror {
    pub(crate) epoch: u64,
    pub(crate) len: usize,
    pub(crate) force_text_dict: bool,
    fields: FxHashMap<String, Arc<FieldMirror>>,
}

impl ColMirror {
    pub(crate) fn build(
        rows: &[Row],
        fields: &[String],
        epoch: u64,
        force_text_dict: bool,
    ) -> Self {
        let fields = fields
            .iter()
            .filter_map(|f| {
                Some((
                    f.clone(),
                    Arc::new(FieldMirror::build(rows, f, force_text_dict)?),
                ))
            })
            .collect();
        ColMirror {
            epoch,
            len: rows.len(),
            force_text_dict,
            fields,
        }
    }

    /// Row positions matching `pred`, in ascending order, or `None` when some leaf
    /// reads a field that is not mirrored.
    pub(crate) fn select(&self, pred: &Pred, now: i64) -> Option<Vec<usize>> {
        let bits = self.eval(pred, now)?;
        let n: usize = bits.iter().map(|w| w.count_ones() as usize).sum();
        let mut out = Vec::with_capacity(n);
        for (w, &word) in bits.iter().enumerate() {
            let mut word = word;
            while word != 0 {
                let b = word.trailing_zeros() as usize;
                out.push(w * 64 + b);
                word &= word - 1;
            }
        }
        Some(out)
    }

    /// Count of rows matching `pred`, or `None` when some leaf reads a field that is not
    /// mirrored. Avoids materializing a `Vec<usize>`.
    pub(crate) fn count(&self, pred: &Pred, now: i64) -> Option<usize> {
        let bits = self.eval(pred, now)?;
        Some(bits.iter().map(|w| w.count_ones() as usize).sum())
    }

    /// Count rows grouped by a mirrored leaf column `by` (optionally filtered by `pred`),
    /// accumulating directly in a flat dictionary-code or typed scalar histogram without touching `Row`s.
    pub(crate) fn group_count(
        &self,
        by: &str,
        pred: Option<&Pred>,
        now: i64,
    ) -> Option<FxHashMap<Cell, i64>> {
        let fm = self.fields.get(by)?;
        if self.len == 0 {
            return Some(FxHashMap::default());
        }
        let bits_opt = match pred {
            Some(p) => Some(self.eval(p, now)?),
            None => None,
        };
        match &fm.storage {
            MirrorStorage::AllNull => {
                let total = match &bits_opt {
                    Some(bits) => bits.iter().map(|w| w.count_ones() as i64).sum(),
                    None => self.len as i64,
                };
                let mut out = FxHashMap::default();
                if total > 0 {
                    out.insert(Cell::Null, total);
                }
                Some(out)
            }
            MirrorStorage::Dict { codes, dict, .. } => {
                let dict_len = dict.len();
                let mut counts = vec![0i64; dict_len];
                match &bits_opt {
                    None => {
                        let chunks = codes.chunks_exact(4);
                        let rem = chunks.remainder();
                        if dict_len <= 256 {
                            let mut c0 = [0i64; 256];
                            let mut c1 = [0i64; 256];
                            let mut c2 = [0i64; 256];
                            let mut c3 = [0i64; 256];
                            for ch in chunks {
                                c0[ch[0] as usize] += 1;
                                c1[ch[1] as usize] += 1;
                                c2[ch[2] as usize] += 1;
                                c3[ch[3] as usize] += 1;
                            }
                            for &r in rem {
                                c0[r as usize] += 1;
                            }
                            for i in 0..dict_len {
                                counts[i] = c0[i] + c1[i] + c2[i] + c3[i];
                            }
                        } else {
                            for &code in codes {
                                counts[code as usize] += 1;
                            }
                        }
                    }
                    Some(bits) => {
                        for (w, &word) in bits.iter().enumerate() {
                            let mut word = word;
                            let base = w * 64;
                            while word != 0 {
                                let b = word.trailing_zeros() as usize;
                                counts[codes[base + b] as usize] += 1;
                                word &= word - 1;
                            }
                        }
                    }
                }
                let non_zero = counts.iter().filter(|&&c| c > 0).count();
                let mut out = FxHashMap::with_capacity_and_hasher(non_zero, Default::default());
                for (idx, count) in counts.into_iter().enumerate() {
                    if count > 0 {
                        out.insert(dict[idx].clone_for_borrow(), count);
                    }
                }
                Some(out)
            }
            MirrorStorage::DenseBool { vals, valid } => {
                let mut cnt_null = 0i64;
                let mut cnt_false = 0i64;
                let mut cnt_true = 0i64;
                let mut visit = |i: usize| {
                    if !valid.is_valid(i) {
                        cnt_null += 1;
                    } else if vals[i] {
                        cnt_true += 1;
                    } else {
                        cnt_false += 1;
                    }
                };
                match &bits_opt {
                    None => {
                        for i in 0..self.len {
                            visit(i);
                        }
                    }
                    Some(bits) => {
                        for (w, &word) in bits.iter().enumerate() {
                            let mut word = word;
                            let base = w * 64;
                            while word != 0 {
                                let b = word.trailing_zeros() as usize;
                                visit(base + b);
                                word &= word - 1;
                            }
                        }
                    }
                }
                let mut out = FxHashMap::default();
                if cnt_null > 0 {
                    out.insert(Cell::Null, cnt_null);
                }
                if cnt_false > 0 {
                    out.insert(Cell::Bool(false), cnt_false);
                }
                if cnt_true > 0 {
                    out.insert(Cell::Bool(true), cnt_true);
                }
                Some(out)
            }
            MirrorStorage::DenseInt { vals, valid } => {
                let mut map: FxHashMap<i64, i64> = FxHashMap::default();
                let mut nulls = 0i64;
                let mut visit = |i: usize| {
                    if valid.is_valid(i) {
                        *map.entry(vals[i]).or_insert(0) += 1;
                    } else {
                        nulls += 1;
                    }
                };
                match &bits_opt {
                    None => {
                        if valid.null_count() == 0 {
                            for &v in vals {
                                *map.entry(v).or_insert(0) += 1;
                            }
                        } else {
                            for i in 0..self.len {
                                visit(i);
                            }
                        }
                    }
                    Some(bits) => {
                        for (w, &word) in bits.iter().enumerate() {
                            let mut word = word;
                            let base = w * 64;
                            while word != 0 {
                                let b = word.trailing_zeros() as usize;
                                visit(base + b);
                                word &= word - 1;
                            }
                        }
                    }
                }
                let mut out =
                    FxHashMap::with_capacity_and_hasher(map.len() + usize::from(nulls > 0), Default::default());
                if nulls > 0 {
                    out.insert(Cell::Null, nulls);
                }
                for (k, c) in map {
                    out.insert(Cell::Int(k), c);
                }
                Some(out)
            }
            MirrorStorage::DenseTime { vals, valid } => {
                let mut map: FxHashMap<i64, i64> = FxHashMap::default();
                let mut nulls = 0i64;
                let mut visit = |i: usize| {
                    if valid.is_valid(i) {
                        *map.entry(vals[i]).or_insert(0) += 1;
                    } else {
                        nulls += 1;
                    }
                };
                match &bits_opt {
                    None => {
                        if valid.null_count() == 0 {
                            for &v in vals {
                                *map.entry(v).or_insert(0) += 1;
                            }
                        } else {
                            for i in 0..self.len {
                                visit(i);
                            }
                        }
                    }
                    Some(bits) => {
                        for (w, &word) in bits.iter().enumerate() {
                            let mut word = word;
                            let base = w * 64;
                            while word != 0 {
                                let b = word.trailing_zeros() as usize;
                                visit(base + b);
                                word &= word - 1;
                            }
                        }
                    }
                }
                let mut out =
                    FxHashMap::with_capacity_and_hasher(map.len() + usize::from(nulls > 0), Default::default());
                if nulls > 0 {
                    out.insert(Cell::Null, nulls);
                }
                for (k, c) in map {
                    out.insert(Cell::Time(k), c);
                }
                Some(out)
            }
            MirrorStorage::DenseFloat { vals, valid } => {
                let mut out: FxHashMap<Cell, i64> = FxHashMap::default();
                let mut visit = |i: usize| {
                    let c = if valid.is_valid(i) {
                        Cell::Float(vals[i])
                    } else {
                        Cell::Null
                    };
                    *out.entry(c).or_insert(0) += 1;
                };
                match &bits_opt {
                    None => {
                        for i in 0..self.len {
                            visit(i);
                        }
                    }
                    Some(bits) => {
                        for (w, &word) in bits.iter().enumerate() {
                            let mut word = word;
                            let base = w * 64;
                            while word != 0 {
                                let b = word.trailing_zeros() as usize;
                                visit(base + b);
                                word &= word - 1;
                            }
                        }
                    }
                }
                Some(out)
            }
            MirrorStorage::DenseText(col) => {
                let mut out: FxHashMap<Cell, i64> = FxHashMap::default();
                match &bits_opt {
                    None => {
                        for i in 0..self.len {
                            *out.entry(col.get(i)).or_insert(0) += 1;
                        }
                    }
                    Some(bits) => {
                        for (w, &word) in bits.iter().enumerate() {
                            let mut word = word;
                            let base = w * 64;
                            while word != 0 {
                                let b = word.trailing_zeros() as usize;
                                *out.entry(col.get(base + b)).or_insert(0) += 1;
                                word &= word - 1;
                            }
                        }
                    }
                }
                Some(out)
            }
        }
    }

    /// Sum `val` grouped by a dictionary-encoded mirrored column `by` (optionally filtered by
    /// `pred`), accumulating directly into flat dictionary-code buckets without touching `Row`s.
    pub(crate) fn group_sum(
        &self,
        by: &str,
        val: &str,
        pred: Option<&Pred>,
        now: i64,
    ) -> Option<Vec<(Cell, f64)>> {
        let kfm = self.fields.get(by)?;
        let vfm = self.fields.get(val)?;
        let MirrorStorage::Dict { codes, dict, .. } = &kfm.storage else {
            return None;
        };
        if self.len == 0 {
            return Some(Vec::new());
        }
        let bits_opt = match pred {
            Some(p) => Some(self.eval(p, now)?),
            None => None,
        };
        let dict_len = dict.len();
        let mut seen = vec![false; dict_len];
        let mut accs = vec![crate::exact_sum::ExactSum::default(); dict_len];

        match &vfm.storage {
            MirrorStorage::AllNull => match &bits_opt {
                None => {
                    for &c in codes {
                        seen[c as usize] = true;
                    }
                }
                Some(bits) => {
                    for (w, &word) in bits.iter().enumerate() {
                        let mut word = word;
                        let base = w * 64;
                        while word != 0 {
                            let b = word.trailing_zeros() as usize;
                            seen[codes[base + b] as usize] = true;
                            word &= word - 1;
                        }
                    }
                }
            },
            MirrorStorage::DenseFloat { vals, valid } => match &bits_opt {
                None => {
                    if valid.null_count() == 0 {
                        for (i, &c) in codes.iter().enumerate() {
                            let b = c as usize;
                            seen[b] = true;
                            accs[b].add_f64(vals[i]);
                        }
                    } else {
                        for (i, &c) in codes.iter().enumerate() {
                            let b = c as usize;
                            seen[b] = true;
                            if valid.is_valid(i) {
                                accs[b].add_f64(vals[i]);
                            }
                        }
                    }
                }
                Some(bits) => {
                    for (w, &word) in bits.iter().enumerate() {
                        let mut word = word;
                        let base = w * 64;
                        while word != 0 {
                            let bit = word.trailing_zeros() as usize;
                            let i = base + bit;
                            let b = codes[i] as usize;
                            seen[b] = true;
                            if valid.is_valid(i) {
                                accs[b].add_f64(vals[i]);
                            }
                            word &= word - 1;
                        }
                    }
                }
            },
            MirrorStorage::DenseInt { vals, valid } => match &bits_opt {
                None => {
                    if valid.null_count() == 0 {
                        for (i, &c) in codes.iter().enumerate() {
                            let b = c as usize;
                            seen[b] = true;
                            accs[b].add_i64(vals[i]);
                        }
                    } else {
                        for (i, &c) in codes.iter().enumerate() {
                            let b = c as usize;
                            seen[b] = true;
                            if valid.is_valid(i) {
                                accs[b].add_i64(vals[i]);
                            }
                        }
                    }
                }
                Some(bits) => {
                    for (w, &word) in bits.iter().enumerate() {
                        let mut word = word;
                        let base = w * 64;
                        while word != 0 {
                            let bit = word.trailing_zeros() as usize;
                            let i = base + bit;
                            let b = codes[i] as usize;
                            seen[b] = true;
                            if valid.is_valid(i) {
                                accs[b].add_i64(vals[i]);
                            }
                            word &= word - 1;
                        }
                    }
                }
            },
            MirrorStorage::Dict {
                codes: vcodes,
                dict: vdict,
                ..
            } => match &bits_opt {
                None => {
                    for (i, &c) in codes.iter().enumerate() {
                        let b = c as usize;
                        seen[b] = true;
                        accs[b].add_cell(Some(&vdict[vcodes[i] as usize]));
                    }
                }
                Some(bits) => {
                    for (w, &word) in bits.iter().enumerate() {
                        let mut word = word;
                        let base = w * 64;
                        while word != 0 {
                            let bit = word.trailing_zeros() as usize;
                            let i = base + bit;
                            let b = codes[i] as usize;
                            seen[b] = true;
                            accs[b].add_cell(Some(&vdict[vcodes[i] as usize]));
                            word &= word - 1;
                        }
                    }
                }
            },
            _ => return None,
        }

        let mut entries = Vec::new();
        for (code, acc) in accs.into_iter().enumerate() {
            if seen[code] {
                entries.push((dict[code].clone_for_borrow(), acc.value()));
            }
        }
        Some(entries)
    }

    /// Zero-allocation ungrouped exact sum + non-null numeric count over a mirrored column,
    /// matching `agg_sum_filtered` / `agg_avg_filtered` bit-for-bit (`i128` integers, Neumaier
    /// compensated floats, `NaN` propagation, and `PAR_AGG_CHUNK_ROWS` chunking at `>= 65_536` rows).
    pub(crate) fn sum_and_count(
        &self,
        val: &str,
        pred: Option<&Pred>,
        now: i64,
    ) -> Option<(crate::exact_sum::ExactSum, usize)> {
        let vfm = self.fields.get(val)?;
        if self.len == 0 {
            return Some((crate::exact_sum::ExactSum::default(), 0));
        }
        let bits_opt = match pred {
            Some(p) => Some(self.eval(p, now)?),
            None => None,
        };
        let n = self.len;
        let chunked = n >= super::agg::PAR_AGG_MIN_ROWS;
        let fold_range = |lo: usize, hi: usize| -> Option<(crate::exact_sum::ExactSum, usize)> {
            let mut sum = crate::exact_sum::ExactSum::default();
            let mut count = 0usize;
            match &vfm.storage {
                MirrorStorage::AllNull
                | MirrorStorage::DenseBool { .. }
                | MirrorStorage::DenseTime { .. }
                | MirrorStorage::DenseText(_) => {}
                MirrorStorage::DenseInt { vals, valid } => match &bits_opt {
                    None => {
                        if valid.null_count() == 0 {
                            sum.add_i64_slice(&vals[lo..hi]);
                            count = hi - lo;
                        } else {
                            for (i, &v) in vals[lo..hi].iter().enumerate() {
                                if valid.is_valid(lo + i) {
                                    sum.add_i64(v);
                                    count += 1;
                                }
                            }
                        }
                    }
                    Some(bits) => {
                        for (w_idx, &word_raw) in bits[lo / 64..(hi + 63) / 64].iter().enumerate() {
                            let base = (lo / 64 + w_idx) * 64;
                            let mut word = word_raw;
                            while word != 0 {
                                let b = word.trailing_zeros() as usize;
                                let idx = base + b;
                                if idx >= lo && idx < hi && valid.is_valid(idx) {
                                    sum.add_i64(vals[idx]);
                                    count += 1;
                                }
                                word &= word - 1;
                            }
                        }
                    }
                },
                MirrorStorage::DenseFloat { vals, valid } => match &bits_opt {
                    None => {
                        if valid.null_count() == 0 {
                            sum.add_f64_slice(&vals[lo..hi]);
                            count = hi - lo;
                        } else {
                            for (i, &v) in vals[lo..hi].iter().enumerate() {
                                if valid.is_valid(lo + i) {
                                    sum.add_f64(v);
                                    count += 1;
                                }
                            }
                        }
                    }
                    Some(bits) => {
                        for (w_idx, &word_raw) in bits[lo / 64..(hi + 63) / 64].iter().enumerate() {
                            let base = (lo / 64 + w_idx) * 64;
                            let mut word = word_raw;
                            while word != 0 {
                                let b = word.trailing_zeros() as usize;
                                let idx = base + b;
                                if idx >= lo && idx < hi && valid.is_valid(idx) {
                                    sum.add_f64(vals[idx]);
                                    count += 1;
                                }
                                word &= word - 1;
                            }
                        }
                    }
                },
                MirrorStorage::Dict { codes, dict, .. } => match &bits_opt {
                    None => {
                        for &c in &codes[lo..hi] {
                            if sum.add_numeric(Some(&dict[c as usize])) {
                                count += 1;
                            }
                        }
                    }
                    Some(bits) => {
                        for (w_idx, &word_raw) in bits[lo / 64..(hi + 63) / 64].iter().enumerate() {
                            let base = (lo / 64 + w_idx) * 64;
                            let mut word = word_raw;
                            while word != 0 {
                                let b = word.trailing_zeros() as usize;
                                let idx = base + b;
                                if idx >= lo
                                    && idx < hi
                                    && sum.add_numeric(Some(&dict[codes[idx] as usize]))
                                {
                                    count += 1;
                                }
                                word &= word - 1;
                            }
                        }
                    }
                },
            }
            Some((sum, count))
        };

        if !chunked {
            return fold_range(0, n);
        }
        let chunk_sz = super::agg::PAR_AGG_CHUNK_ROWS;
        let num_chunks = n.div_ceil(chunk_sz);
        #[cfg(feature = "parallel")]
        let partials: Vec<(crate::exact_sum::ExactSum, usize)> = {
            use rayon::prelude::*;
            (0..num_chunks)
                .into_par_iter()
                .map(|ci| {
                    let lo = ci * chunk_sz;
                    let hi = (lo + chunk_sz).min(n);
                    fold_range(lo, hi).unwrap_or_default()
                })
                .collect()
        };
        #[cfg(not(feature = "parallel"))]
        let partials: Vec<(crate::exact_sum::ExactSum, usize)> = (0..num_chunks)
            .map(|ci| {
                let lo = ci * chunk_sz;
                let hi = (lo + chunk_sz).min(n);
                fold_range(lo, hi).unwrap_or_default()
            })
            .collect();
        let mut total = crate::exact_sum::ExactSum::default();
        let mut total_cnt = 0usize;
        for (p_sum, p_cnt) in partials {
            total.merge(p_sum);
            total_cnt += p_cnt;
        }
        Some((total, total_cnt))
    }

    /// Projects `names` directly from mirrored columns into a dense [`crate::batch::RecordBatch`]
    /// (emitting `Column::Dict` for low-cardinality text, `Column::Text` for high-cardinality text,
    /// and `Column::Int`/`Float`/`Bool`/`Time` in `O(1)` buffer clones/gathers for scalars) without
    /// touching `Vec<Row>`.
    pub(crate) fn project_batch(
        &self,
        names: &[String],
        idxs: Option<&[usize]>,
    ) -> Option<RecordBatch> {
        let mirrors: SmallVec<[&FieldMirror; 8]> = names
            .iter()
            .map(|nm| self.fields.get(nm.as_str()).map(Arc::as_ref))
            .collect::<Option<_>>()?;
        let out_n = idxs.map_or(self.len, |ix| ix.len());
        let mut cols = Vec::with_capacity(mirrors.len());
        for fm in mirrors {
            match &fm.storage {
                MirrorStorage::AllNull => {
                    cols.push(Column::Nulls {
                        n: out_n,
                        cap: out_n,
                    });
                }
                MirrorStorage::DenseInt { vals, valid } => match idxs {
                    None => cols.push(Column::Int {
                        vals: vals.clone(),
                        valid: valid.clone(),
                    }),
                    Some(ix) => {
                        let (v, val) = gather_dense(vals, valid, ix, 0);
                        cols.push(Column::Int {
                            vals: v,
                            valid: val,
                        });
                    }
                },
                MirrorStorage::DenseFloat { vals, valid } => match idxs {
                    None => cols.push(Column::Float {
                        vals: vals.clone(),
                        valid: valid.clone(),
                    }),
                    Some(ix) => {
                        let (v, val) = gather_dense(vals, valid, ix, 0.0);
                        cols.push(Column::Float {
                            vals: v,
                            valid: val,
                        });
                    }
                },
                MirrorStorage::DenseTime { vals, valid } => match idxs {
                    None => cols.push(Column::Time {
                        vals: vals.clone(),
                        valid: valid.clone(),
                    }),
                    Some(ix) => {
                        let (v, val) = gather_dense(vals, valid, ix, 0);
                        cols.push(Column::Time {
                            vals: v,
                            valid: val,
                        });
                    }
                },
                MirrorStorage::DenseBool { vals, valid } => match idxs {
                    None => cols.push(Column::Bool {
                        vals: vals.clone(),
                        valid: valid.clone(),
                    }),
                    Some(ix) => {
                        let (v, val) = gather_dense(vals, valid, ix, false);
                        cols.push(Column::Bool {
                            vals: v,
                            valid: val,
                        });
                    }
                },
                MirrorStorage::DenseText(col) => match idxs {
                    None => cols.push(col.clone()),
                    Some(ix) => cols.push(gather_text_col_usize(col, ix)),
                },
                MirrorStorage::Dict {
                    codes,
                    dict: _,
                    text_meta: Some(meta),
                } => match &meta.code_remap {
                    None => {
                        let out_codes: Vec<u32> = match idxs {
                            None => codes.clone(),
                            Some(ix) => ix.iter().map(|&r| codes[r]).collect(),
                        };
                        cols.push(Column::Dict {
                            values: meta.values.clone(),
                            codes: out_codes,
                            valid: Validity::all_valid(out_n),
                        });
                    }
                    Some(code_remap) => {
                        let mut out_codes = Vec::with_capacity(out_n);
                        let mut valid = Validity::default();
                        let mut push_code = |raw_code: u32| match code_remap[raw_code as usize] {
                            Some(mapped) => {
                                out_codes.push(mapped);
                                valid.push(true);
                            }
                            None => {
                                out_codes.push(0);
                                valid.push(false);
                            }
                        };
                        match idxs {
                            None => {
                                for &c in codes {
                                    push_code(c);
                                }
                            }
                            Some(ix) => {
                                for &r in ix {
                                    push_code(codes[r]);
                                }
                            }
                        }
                        cols.push(Column::Dict {
                            values: meta.values.clone(),
                            codes: out_codes,
                            valid,
                        });
                    }
                },
                MirrorStorage::Dict {
                    codes,
                    dict,
                    text_meta: None,
                } => {
                    let mut col = Column::with_capacity(out_n);
                    match idxs {
                        None => {
                            for &c in codes {
                                col.push(dict[c as usize].clone_for_borrow());
                            }
                        }
                        Some(ix) => {
                            for &r in ix {
                                col.push(dict[codes[r] as usize].clone_for_borrow());
                            }
                        }
                    }
                    cols.push(col);
                }
            }
        }
        Some(RecordBatch {
            names: names.to_vec().into(),
            cols,
        })
    }

    #[inline]
    fn covers_pred(&self, pred: &Pred) -> bool {
        match pred {
            Pred::And(a, b) | Pred::Or(a, b) => self.covers_pred(a) && self.covers_pred(b),
            Pred::Not(inner) => self.covers_pred(inner),
            Pred::IsNull { field, .. }
            | Pred::In { field, .. }
            | Pred::Cmp { field, .. }
            | Pred::Has { field, .. }
            | Pred::Contains { field, .. }
            | Pred::Regex { field, .. } => {
                field.leaf().is_some_and(|lf| self.fields.contains_key(lf))
            }
        }
    }

    #[inline]
    fn all_ones_bits(&self, words: usize) -> Vec<u64> {
        let mut bits = vec![u64::MAX; words];
        let tail = self.len % 64;
        if tail != 0
            && let Some(last) = bits.last_mut()
        {
            *last = (1u64 << tail) - 1;
        }
        bits
    }

    #[inline]
    fn mask_tail_bits(&self, bits: &mut [u64]) {
        let tail = self.len % 64;
        if tail != 0
            && let Some(last) = bits.last_mut()
        {
            *last &= (1u64 << tail) - 1;
        }
    }

    #[inline]
    fn is_all_ones(&self, bits: &[u64]) -> bool {
        let Some((&last, init)) = bits.split_last() else {
            return true;
        };
        if !init.iter().all(|&w| w == u64::MAX) {
            return false;
        }
        let tail = self.len % 64;
        let expected_last = if tail == 0 {
            u64::MAX
        } else {
            (1u64 << tail) - 1
        };
        last == expected_last
    }

    fn eval_leaf_dense(&self, fm: &FieldMirror, pred: &Pred, now: i64, words: usize) -> Vec<u64> {
        if let Pred::IsNull { negated, .. } = pred {
            return match &fm.storage {
                MirrorStorage::AllNull => {
                    if *negated {
                        vec![0u64; words]
                    } else {
                        self.all_ones_bits(words)
                    }
                }
                MirrorStorage::DenseInt { valid, .. }
                | MirrorStorage::DenseFloat { valid, .. }
                | MirrorStorage::DenseTime { valid, .. }
                | MirrorStorage::DenseBool { valid, .. } => {
                    let mut b = bits_from_valid(self.len, valid);
                    if !*negated {
                        b.iter_mut().for_each(|w| *w = !*w);
                        self.mask_tail_bits(&mut b);
                    }
                    b
                }
                MirrorStorage::DenseText(col) => {
                    let mut bits = vec![0u64; words];
                    for w in 0..words {
                        let base = w * 64;
                        let lim = (self.len - base).min(64);
                        let mut word = 0u64;
                        for b in 0..lim {
                            let is_null = col.is_null(base + b);
                            let ok = if *negated { !is_null } else { is_null };
                            word |= u64::from(ok) << b;
                        }
                        bits[w] = word;
                    }
                    bits
                }
                MirrorStorage::Dict { .. } => unreachable!(),
            };
        }

        if let Pred::Cmp { op, value, .. } = pred {
            let lit = value_cell(value, now);
            let op = *op;
            if matches!(lit, Cell::Null) {
                return vec![0u64; words];
            }
            match &fm.storage {
                MirrorStorage::AllNull => {
                    return if matches!(op, CmpOp::Ne) {
                        self.all_ones_bits(words)
                    } else {
                        vec![0u64; words]
                    };
                }
                MirrorStorage::DenseInt { vals, valid } => match &lit {
                    Cell::Int(rv) => return eval_i64_bits(vals, valid, op, *rv),
                    Cell::Float(rf) => {
                        let rf = *rf;
                        if rf.is_nan() {
                            return if matches!(op, CmpOp::Ne) {
                                self.all_ones_bits(words)
                            } else {
                                vec![0u64; words]
                            };
                        }
                        const TWO_63: f64 = 9_223_372_036_854_775_808.0;
                        return match op {
                            CmpOp::Eq | CmpOp::Ne => {
                                let exact_int = if rf.is_finite()
                                    && rf.fract() == 0.0
                                    && (-TWO_63..TWO_63).contains(&rf)
                                {
                                    let fi = rf as i64;
                                    (fi as f64 == rf).then_some(fi)
                                } else {
                                    None
                                };
                                match (op, exact_int) {
                                    (_, Some(fi)) => eval_i64_bits(vals, valid, op, fi),
                                    (CmpOp::Eq, None) => vec![0u64; words],
                                    (CmpOp::Ne, None) => self.all_ones_bits(words),
                                    _ => unreachable!(),
                                }
                            }
                            CmpOp::Lt | CmpOp::Le | CmpOp::Gt | CmpOp::Ge => {
                                if rf >= TWO_63 {
                                    if matches!(op, CmpOp::Lt | CmpOp::Le) {
                                        bits_from_valid(self.len, valid)
                                    } else {
                                        vec![0u64; words]
                                    }
                                } else if rf < -TWO_63 {
                                    if matches!(op, CmpOp::Gt | CmpOp::Ge) {
                                        bits_from_valid(self.len, valid)
                                    } else {
                                        vec![0u64; words]
                                    }
                                } else {
                                    let fi = rf as i64;
                                    let f_back = fi as f64;
                                    if f_back == rf {
                                        eval_i64_bits(vals, valid, op, fi)
                                    } else if rf > f_back {
                                        let eff_op = match op {
                                            CmpOp::Lt | CmpOp::Le => CmpOp::Le,
                                            CmpOp::Gt | CmpOp::Ge => CmpOp::Gt,
                                            _ => unreachable!(),
                                        };
                                        eval_i64_bits(vals, valid, eff_op, fi)
                                    } else {
                                        let eff_op = match op {
                                            CmpOp::Lt | CmpOp::Le => CmpOp::Lt,
                                            CmpOp::Gt | CmpOp::Ge => CmpOp::Ge,
                                            _ => unreachable!(),
                                        };
                                        eval_i64_bits(vals, valid, eff_op, fi)
                                    }
                                }
                            }
                        };
                    }
                    _ => {
                        return if matches!(op, CmpOp::Ne) {
                            self.all_ones_bits(words)
                        } else {
                            vec![0u64; words]
                        };
                    }
                },
                MirrorStorage::DenseTime { vals, valid } => match &lit {
                    Cell::Time(rt) => return eval_i64_bits(vals, valid, op, *rt),
                    _ => {
                        return if matches!(op, CmpOp::Ne) {
                            self.all_ones_bits(words)
                        } else {
                            vec![0u64; words]
                        };
                    }
                },
                MirrorStorage::DenseFloat { vals, valid } => match &lit {
                    Cell::Float(rf) => {
                        return eval_f64_bits(vals, valid, op, *rf, || self.all_ones_bits(words));
                    }
                    Cell::Int(ri) => {
                        let ri = *ri;
                        const TWO_63: f64 = 9_223_372_036_854_775_808.0;
                        let rf = ri as f64;
                        if rf < TWO_63 && (rf as i64 == ri) {
                            return eval_f64_bits(vals, valid, op, rf, || {
                                self.all_ones_bits(words)
                            });
                        }
                        return match op {
                            CmpOp::Eq => eval_dense_bits(vals, valid, false, |v| {
                                crate::eval::int_eq_f64(ri, v)
                            }),
                            CmpOp::Ne => eval_dense_bits(vals, valid, true, |v| {
                                !crate::eval::int_eq_f64(ri, v)
                            }),
                            CmpOp::Lt | CmpOp::Le | CmpOp::Gt | CmpOp::Ge => {
                                eval_dense_bits(vals, valid, false, |v| {
                                    match crate::eval::cmp_i64_f64(ri, v).map(|o| o.reverse()) {
                                        Some(ord) => match op {
                                            CmpOp::Gt => ord == std::cmp::Ordering::Greater,
                                            CmpOp::Lt => ord == std::cmp::Ordering::Less,
                                            CmpOp::Ge => ord != std::cmp::Ordering::Less,
                                            CmpOp::Le => ord != std::cmp::Ordering::Greater,
                                            _ => false,
                                        },
                                        None => false,
                                    }
                                })
                            }
                        };
                    }
                    _ => {
                        return if matches!(op, CmpOp::Ne) {
                            self.all_ones_bits(words)
                        } else {
                            vec![0u64; words]
                        };
                    }
                },
                MirrorStorage::DenseBool { vals, valid } => match &lit {
                    Cell::Bool(rb) => {
                        let rb = *rb;
                        return match op {
                            CmpOp::Eq => eval_dense_bits(vals, valid, false, |v| v == rb),
                            CmpOp::Ne => eval_dense_bits(vals, valid, true, |v| v != rb),
                            CmpOp::Lt | CmpOp::Le | CmpOp::Gt | CmpOp::Ge => vec![0u64; words],
                        };
                    }
                    _ => {
                        return if matches!(op, CmpOp::Ne) {
                            self.all_ones_bits(words)
                        } else {
                            vec![0u64; words]
                        };
                    }
                },
                MirrorStorage::DenseText(col) => {
                    if let Some(target) = lit.text() {
                        let mut bits = Vec::with_capacity(words);
                        for w in 0..words {
                            let base = w * 64;
                            let lim = (self.len - base).min(64);
                            let mut word = 0u64;
                            for b in 0..lim {
                                let idx = base + b;
                                let ok = match col.text_at(idx) {
                                    Some(s) => match op {
                                        CmpOp::Eq => s == target,
                                        CmpOp::Ne => s != target,
                                        CmpOp::Lt => s < target,
                                        CmpOp::Le => s <= target,
                                        CmpOp::Gt => s > target,
                                        CmpOp::Ge => s >= target,
                                    },
                                    None => matches!(op, CmpOp::Ne),
                                };
                                word |= u64::from(ok) << b;
                            }
                            bits.push(word);
                        }
                        return bits;
                    }
                    return if matches!(op, CmpOp::Ne) {
                        self.all_ones_bits(words)
                    } else {
                        vec![0u64; words]
                    };
                }
                MirrorStorage::Dict { .. } => unreachable!(),
            }
        }

        // Fallback for `In`, `Contains`, `Regex`, `Has` on dense columns: evaluate `eval_leaf` directly.
        match &fm.storage {
            MirrorStorage::AllNull => {
                if eval_leaf(pred, &Cell::Null, now) {
                    self.all_ones_bits(words)
                } else {
                    vec![0u64; words]
                }
            }
            MirrorStorage::DenseInt { vals, valid } => {
                let null_ok = eval_leaf(pred, &Cell::Null, now);
                eval_dense_bits(vals, valid, null_ok, |v| eval_leaf(pred, &Cell::Int(v), now))
            }
            MirrorStorage::DenseFloat { vals, valid } => {
                let null_ok = eval_leaf(pred, &Cell::Null, now);
                eval_dense_bits(vals, valid, null_ok, |v| {
                    eval_leaf(pred, &Cell::Float(v), now)
                })
            }
            MirrorStorage::DenseTime { vals, valid } => {
                let null_ok = eval_leaf(pred, &Cell::Null, now);
                eval_dense_bits(vals, valid, null_ok, |v| {
                    eval_leaf(pred, &Cell::Time(v), now)
                })
            }
            MirrorStorage::DenseBool { vals, valid } => {
                let null_ok = eval_leaf(pred, &Cell::Null, now);
                let false_ok = eval_leaf(pred, &Cell::Bool(false), now);
                let true_ok = eval_leaf(pred, &Cell::Bool(true), now);
                eval_dense_bits(vals, valid, null_ok, |v| if v { true_ok } else { false_ok })
            }
            MirrorStorage::DenseText(col) => {
                let null_ok = eval_leaf(pred, &Cell::Null, now);
                let mut bits = Vec::with_capacity(words);
                for w in 0..words {
                    let base = w * 64;
                    let lim = (self.len - base).min(64);
                    let mut word = 0u64;
                    for b in 0..lim {
                        let idx = base + b;
                        let ok = if col.is_null(idx) {
                            null_ok
                        } else {
                            eval_leaf(pred, &col.get(idx), now)
                        };
                        word |= u64::from(ok) << b;
                    }
                    bits.push(word);
                }
                bits
            }
            MirrorStorage::Dict { .. } => unreachable!(),
        }
    }

    fn eval(&self, pred: &Pred, now: i64) -> Option<Vec<u64>> {
        let words = self.len.div_ceil(64);
        match pred {
            Pred::And(a, b) => {
                let mut l = self.eval(a, now)?;
                if l.iter().all(|&w| w == 0) {
                    return self.covers_pred(b).then_some(l);
                }
                let r = self.eval(b, now)?;
                l.iter_mut().zip(&r).for_each(|(x, y)| *x &= y);
                Some(l)
            }
            Pred::Or(a, b) => {
                let mut l = self.eval(a, now)?;
                if self.is_all_ones(&l) {
                    return self.covers_pred(b).then_some(l);
                }
                let r = self.eval(b, now)?;
                l.iter_mut().zip(&r).for_each(|(x, y)| *x |= y);
                Some(l)
            }
            Pred::Not(inner) => {
                let mut v = self.eval(inner, now)?;
                v.iter_mut().for_each(|x| *x = !*x);
                self.mask_tail_bits(&mut v);
                Some(v)
            }
            Pred::IsNull { field, .. }
            | Pred::In { field, .. }
            | Pred::Cmp { field, .. }
            | Pred::Has { field, .. }
            | Pred::Contains { field, .. }
            | Pred::Regex { field, .. } => {
                let leaf = field.leaf()?;
                let m = self.fields.get(leaf)?;
                if words == 0 {
                    return Some(Vec::new());
                }
                match &m.storage {
                    MirrorStorage::Dict { codes, dict, .. } => {
                        let mut any_true = false;
                        let mut all_true = true;
                        let table: SmallVec<[bool; 256]> = dict
                            .iter()
                            .map(|c| {
                                let ok = eval_leaf(pred, c, now);
                                any_true |= ok;
                                all_true &= ok;
                                ok
                            })
                            .collect();
                        if !any_true {
                            return Some(vec![0u64; words]);
                        }
                        if all_true {
                            return Some(self.all_ones_bits(words));
                        }
                        let mut bits = Vec::with_capacity(words);
                        let mut chunks = codes.chunks_exact(64);
                        for chunk in &mut chunks {
                            let mut word = 0u64;
                            for (b, &code) in chunk.iter().enumerate() {
                                word |= u64::from(table[code as usize]) << b;
                            }
                            bits.push(word);
                        }
                        let rem = chunks.remainder();
                        if !rem.is_empty() {
                            let mut word = 0u64;
                            for (b, &code) in rem.iter().enumerate() {
                                word |= u64::from(table[code as usize]) << b;
                            }
                            bits.push(word);
                        }
                        Some(bits)
                    }
                    _ => Some(self.eval_leaf_dense(m, pred, now, words)),
                }
            }
        }
    }
}
