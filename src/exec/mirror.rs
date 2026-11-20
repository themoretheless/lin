//! Declared column mirrors: an opt-in, lazily rebuilt columnar copy of chosen fields.
//!
//! Each declared field is stored as a dictionary of distinct cells plus one code per
//! row. A filter leaf is decided once per dictionary entry with the same `eval_leaf`
//! the row path uses, so the selection is exactly the row path's. Rows stay the
//! source of truth: a mirror is valid only for the row-store epoch it was built at.

use std::sync::Arc;

use rustc_hash::FxHashMap;

use super::eval::eval_leaf;
use crate::ast::Pred;
use crate::store::row::FieldSlot;
use crate::store::{Cell, Row};

/// Above this many distinct values a field is not mirrored.
const MAX_DICT: usize = 1 << 16;

pub(crate) struct FieldMirror {
    codes: Vec<u32>,
    dict: Vec<Cell>,
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
        Some(FieldMirror { codes, dict })
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

    /// Projects `names` directly from mirrored dictionary columns into a dense [`crate::batch::RecordBatch`]
    /// (emitting `Column::Dict` for text columns and typed `Int`/`Float`/`Bool`/`Time` columns
    /// for homogeneous scalars) without touching `Vec<Row>`.
    pub(crate) fn project_batch(
        &self,
        names: &[String],
        idxs: Option<&[usize]>,
    ) -> Option<crate::batch::RecordBatch> {
        use crate::batch::{Column, RecordBatch, Validity};
        let mirrors: Vec<&FieldMirror> = names
            .iter()
            .map(|nm| self.fields.get(nm.as_str()).map(Arc::as_ref))
            .collect::<Option<_>>()?;
        let out_n = idxs.map_or(self.len, |ix| ix.len());
        let mut cols = Vec::with_capacity(mirrors.len());
        for fm in mirrors {
            let all_null = fm.dict.iter().all(|c| matches!(c, Cell::Null));
            if all_null {
                cols.push(Column::Nulls { n: out_n, cap: out_n });
                continue;
            }
            let all_text = fm
                .dict
                .iter()
                .all(|c| matches!(c, Cell::Null | Cell::Text(_) | Cell::SmallText(_)));
            if all_text {
                let mut values: Vec<Arc<str>> = Vec::with_capacity(fm.dict.len());
                let mut code_remap: Vec<Option<u32>> = Vec::with_capacity(fm.dict.len());
                for c in &fm.dict {
                    match c {
                        Cell::Null => code_remap.push(None),
                        other => {
                            let idx = values.len() as u32;
                            values.push(
                                other
                                    .text_shared()
                                    .unwrap_or_else(|| Arc::from(other.text().unwrap_or(""))),
                            );
                            code_remap.push(Some(idx));
                        }
                    }
                }
                let has_null_entry = code_remap.iter().any(Option::is_none);
                if !has_null_entry {
                    let codes: Vec<u32> = match idxs {
                        None => fm
                            .codes
                            .iter()
                            .map(|&c| code_remap[c as usize].unwrap_or(0))
                            .collect(),
                        Some(ix) => ix
                            .iter()
                            .map(|&r| code_remap[fm.codes[r] as usize].unwrap_or(0))
                            .collect(),
                    };
                    cols.push(Column::Dict {
                        values,
                        codes,
                        valid: Validity::all_valid(out_n),
                    });
                } else {
                    let mut out_col = Column::Dict {
                        values,
                        codes: Vec::with_capacity(out_n),
                        valid: Validity::default(),
                    };
                    if let Column::Dict { codes, valid, .. } = &mut out_col {
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
                    }
                    cols.push(out_col);
                }
            } else {
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
        Some(RecordBatch {
            names: names.to_vec().into(),
            cols,
        })
    }

    fn eval(&self, pred: &Pred, now: i64) -> Option<Vec<u64>> {
        let words = self.len.div_ceil(64);
        match pred {
            Pred::And(a, b) => {
                let mut l = self.eval(a, now)?;
                let r = self.eval(b, now)?;
                l.iter_mut().zip(&r).for_each(|(x, y)| *x &= y);
                Some(l)
            }
            Pred::Or(a, b) => {
                let mut l = self.eval(a, now)?;
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
                let table: Vec<bool> = m.dict.iter().map(|c| eval_leaf(pred, c, now)).collect();
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
