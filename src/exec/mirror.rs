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
                let cloned = cell.clone();
                dict.push(cloned.clone());
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
                let mut bits = vec![0u64; words];
                for (i, &code) in m.codes.iter().enumerate() {
                    if table[code as usize] {
                        bits[i >> 6] |= 1u64 << (i & 63);
                    }
                }
                Some(bits)
            }
        }
    }
}
