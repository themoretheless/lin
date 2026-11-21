//! Declared column mirrors: an opt-in, lazily rebuilt columnar copy of chosen fields.
//!
//! Each declared field is stored as a dictionary of distinct cells plus one code per
//! row. A filter leaf is decided once per dictionary entry with the same `eval_leaf`
//! the row path uses, so the selection is exactly the row path's. Rows stay the
//! source of truth: a mirror is valid only for the row-store epoch it was built at.

use std::sync::Arc;

use rustc_hash::FxHashMap;
use smallvec::SmallVec;

use super::eval::eval_leaf;
use crate::ast::Pred;
use crate::store::row::FieldSlot;
use crate::store::{Cell, Row};

/// Above this many distinct values a field is not mirrored.
const MAX_DICT: usize = 1 << 16;

struct TextDictMeta {
    values: Vec<Arc<str>>,
    /// `None` when every dictionary entry is non-null text (`codes` map 1-to-1 to `values`).
    /// `Some(remap)` when `Cell::Null` appears in `dict`.
    code_remap: Option<Vec<Option<u32>>>,
}

enum MirrorKind {
    AllNull,
    TextDict(TextDictMeta),
    Scalar,
}

pub(crate) struct FieldMirror {
    codes: Vec<u32>,
    dict: Vec<Cell>,
    kind: MirrorKind,
}

impl FieldMirror {
    fn build(rows: &[Row], name: &str) -> Option<Self> {
        let mut index: FxHashMap<Cell, u32> = FxHashMap::default();
        let mut dict: Vec<Cell> = Vec::new();
        let mut codes = Vec::with_capacity(rows.len());
        let mut slot = FieldSlot::new(name);
        for row in rows {
            let cell = slot.get(row).unwrap_or(&Cell::Null);
            if matches!(cell, Cell::Vec(_)) {
                return None;
            }
            let code = if let Some(&c) = index.get(cell) {
                c
            } else {
                if dict.len() >= MAX_DICT {
                    return None;
                }
                let next = dict.len() as u32;
                let cloned = cell.clone_for_borrow();
                dict.push(cloned.clone_for_borrow());
                index.insert(cloned, next);
                next
            };
            codes.push(code);
        }

        let kind = if dict.iter().all(|c| matches!(c, Cell::Null)) {
            MirrorKind::AllNull
        } else if dict
            .iter()
            .all(|c| matches!(c, Cell::Null | Cell::Text(_) | Cell::SmallText(_)))
        {
            let has_null = dict.iter().any(|c| matches!(c, Cell::Null));
            if !has_null {
                let values = dict
                    .iter()
                    .map(|c| {
                        c.text_shared()
                            .unwrap_or_else(|| Arc::from(c.text().unwrap_or("")))
                    })
                    .collect();
                MirrorKind::TextDict(TextDictMeta {
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
                MirrorKind::TextDict(TextDictMeta {
                    values,
                    code_remap: Some(remap),
                })
            }
        } else {
            MirrorKind::Scalar
        };

        Some(FieldMirror { codes, dict, kind })
    }
}

/// Mirrors of one collection's declared fields at one row-store epoch.
pub(crate) struct ColMirror {
    pub(crate) epoch: u64,
    pub(crate) len: usize,
    fields: FxHashMap<String, Arc<FieldMirror>>,
}

impl ColMirror {
    pub(crate) fn build(rows: &[Row], fields: &[String], epoch: u64) -> Self {
        let fields = fields
            .iter()
            .filter_map(|f| Some((f.clone(), Arc::new(FieldMirror::build(rows, f)?))))
            .collect();
        ColMirror {
            epoch,
            len: rows.len(),
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
    /// accumulating directly in a flat dictionary-code histogram without touching `Row`s.
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
        let dict_len = fm.dict.len();
        let mut counts = vec![0i64; dict_len];
        match pred {
            None => {
                let chunks = fm.codes.chunks_exact(4);
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
                    for &code in &fm.codes {
                        counts[code as usize] += 1;
                    }
                }
            }
            Some(p) => {
                let bits = self.eval(p, now)?;
                for (w, &word) in bits.iter().enumerate() {
                    let mut word = word;
                    let base = w * 64;
                    while word != 0 {
                        let b = word.trailing_zeros() as usize;
                        counts[fm.codes[base + b] as usize] += 1;
                        word &= word - 1;
                    }
                }
            }
        }
        let non_zero = counts.iter().filter(|&&c| c > 0).count();
        let mut out = FxHashMap::with_capacity_and_hasher(non_zero, Default::default());
        for (idx, count) in counts.into_iter().enumerate() {
            if count > 0 {
                out.insert(fm.dict[idx].clone_for_borrow(), count);
            }
        }
        Some(out)
    }

    /// Projects `names` directly from mirrored dictionary columns into a dense [`crate::batch::RecordBatch`]
    /// (emitting `Column::Dict` for text columns and typed `Int`/`Float`/`Bool`/`Time` columns
    /// for homogeneous scalars) without touching `Vec<Row>`.
    pub(crate) fn project_batch(
        &self,
        names: &[String],
        idxs: Option<&[usize]>,
    ) -> Option<crate::batch::RecordBatch> {
        use crate::batch::{Column, RecordBatch, Validity};
        let mirrors: SmallVec<[&FieldMirror; 8]> = names
            .iter()
            .map(|nm| self.fields.get(nm.as_str()).map(Arc::as_ref))
            .collect::<Option<_>>()?;
        let out_n = idxs.map_or(self.len, |ix| ix.len());
        let mut cols = Vec::with_capacity(mirrors.len());
        for fm in mirrors {
            match &fm.kind {
                MirrorKind::AllNull => {
                    cols.push(Column::Nulls { n: out_n, cap: out_n });
                }
                MirrorKind::TextDict(meta) => match &meta.code_remap {
                    None => {
                        let codes: Vec<u32> = match idxs {
                            None => fm.codes.clone(),
                            Some(ix) => ix.iter().map(|&r| fm.codes[r]).collect(),
                        };
                        cols.push(Column::Dict {
                            values: meta.values.clone(),
                            codes,
                            valid: Validity::all_valid(out_n),
                        });
                    }
                    Some(code_remap) => {
                        let mut codes = Vec::with_capacity(out_n);
                        let mut valid = Validity::default();
                        let mut push_code = |raw_code: u32| match code_remap[raw_code as usize] {
                            Some(mapped) => {
                                codes.push(mapped);
                                valid.push(true);
                            }
                            None => {
                                codes.push(0);
                                valid.push(false);
                            }
                        };
                        match idxs {
                            None => {
                                for &c in &fm.codes {
                                    push_code(c);
                                }
                            }
                            Some(ix) => {
                                for &r in ix {
                                    push_code(fm.codes[r]);
                                }
                            }
                        }
                        cols.push(Column::Dict {
                            values: meta.values.clone(),
                            codes,
                            valid,
                        });
                    }
                },
                MirrorKind::Scalar => {
                    let mut col = Column::with_capacity(out_n);
                    match idxs {
                        None => {
                            for &c in &fm.codes {
                                col.push(fm.dict[c as usize].clone_for_borrow());
                            }
                        }
                        Some(ix) => {
                            for &r in ix {
                                col.push(fm.dict[fm.codes[r] as usize].clone_for_borrow());
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
    fn is_all_ones(&self, bits: &[u64]) -> bool {
        let Some((&last, init)) = bits.split_last() else {
            return true;
        };
        if !init.iter().all(|&w| w == u64::MAX) {
            return false;
        }
        let tail = self.len % 64;
        let expected_last = if tail == 0 { u64::MAX } else { (1u64 << tail) - 1 };
        last == expected_last
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
                let tail = self.len % 64;
                if tail != 0
                    && let Some(last) = v.last_mut()
                {
                    *last &= (1u64 << tail) - 1;
                }
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
                let mut any_true = false;
                let mut all_true = true;
                let table: SmallVec<[bool; 256]> = m
                    .dict
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
                let mut chunks = m.codes.chunks_exact(64);
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
        }
    }
}
