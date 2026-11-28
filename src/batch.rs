//! Columnar result batch — in-process OLAP shape beside row-API (`Vec<Row>`).

use std::collections::BTreeSet;
use std::sync::Arc;

use crate::error::Error;
use crate::store::{Cell, Row};

const BATCH_WIRE_MAGIC: &[u8; 5] = b"LINB\x01";
const BATCH_CRC_TAG: &[u8; 4] = b"LCRC";

/// Bit-packed validity of a typed column: a set bit means the value is present.
/// While no null has been pushed the mask is not materialized at all.
#[derive(Debug, Clone, Default)]
pub struct Validity {
    /// Empty while `nulls == 0`.
    words: Vec<u64>,
    len: usize,
    nulls: usize,
}

impl Validity {
    #[inline]
    pub(crate) fn all_valid(n: usize) -> Self {
        Self {
            words: Vec::new(),
            len: n,
            nulls: 0,
        }
    }

    fn all_null(n: usize) -> Self {
        Self {
            words: if n == 0 { Vec::new() } else { vec![0; n.div_ceil(64)] },
            len: n,
            nulls: n,
        }
    }

    #[inline]
    pub(crate) fn push(&mut self, valid: bool) {
        if self.nulls == 0 {
            if valid {
                self.len += 1;
                return;
            }
            // First null: materialize the mask for the values seen so far.
            self.words = vec![u64::MAX; (self.len + 1).div_ceil(64)];
            let tail = self.len % 64;
            if tail != 0 || self.len == 0 {
                let last = self.words.len() - 1;
                self.words[last] = if tail == 0 { 0 } else { (1u64 << tail) - 1 };
            } else {
                // `len` is a multiple of 64: the new bit starts a fresh, empty word.
                let last = self.words.len() - 1;
                self.words[last] = 0;
            }
            self.nulls = 1;
            self.len += 1;
            return;
        }
        if self.len % 64 == 0 {
            self.words.push(0);
        }
        if valid {
            self.words[self.len / 64] |= 1u64 << (self.len % 64);
        } else {
            self.nulls += 1;
        }
        self.len += 1;
    }

    #[inline]
    pub fn is_valid(&self, i: usize) -> bool {
        i < self.len && (self.nulls == 0 || (self.words[i / 64] >> (i % 64)) & 1 == 1)
    }

    /// Number of absent values.
    pub fn null_count(&self) -> usize {
        self.nulls
    }

    /// Logical row length tracked by this validity mask.
    #[inline]
    pub fn len(&self) -> usize {
        self.len
    }

    /// True when this validity mask has length `0`.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Slice `[range.start .. range.end]` (clamped to `0..self.len`) with an exact
    /// recomputed `null_count`, avoiding allocation when all sliced values are valid.
    pub fn slice_range(&self, range: std::ops::Range<usize>) -> Validity {
        let start = range.start.min(self.len);
        let end = range.end.min(self.len);
        if start >= end {
            return Validity::default();
        }
        let n = end - start;
        if self.nulls == 0 {
            return Validity::all_valid(n);
        }
        let out_words = n.div_ceil(64);
        let word_shift = start / 64;
        let bit_shift = start % 64;
        let mut words = Vec::with_capacity(out_words);
        if bit_shift == 0 {
            words.extend_from_slice(&self.words[word_shift..word_shift + out_words]);
        } else {
            for w in 0..out_words {
                let lo = self.words[word_shift + w] >> bit_shift;
                let hi = self
                    .words
                    .get(word_shift + w + 1)
                    .copied()
                    .unwrap_or(0)
                    << (64 - bit_shift);
                words.push(lo | hi);
            }
        }
        let tail = n % 64;
        if tail != 0 {
            words[out_words - 1] &= (1u64 << tail) - 1;
        }
        let ones: usize = words.iter().map(|w| w.count_ones() as usize).sum();
        let nulls = n - ones;
        if nulls == 0 {
            Validity::all_valid(n)
        } else {
            Validity {
                words,
                len: n,
                nulls,
            }
        }
    }

    fn truncate(&mut self, n: usize) {
        if n >= self.len {
            return;
        }
        if n == 0 {
            self.words.clear();
            self.len = 0;
            self.nulls = 0;
            return;
        }
        if self.nulls == 0 {
            self.len = n;
            return;
        }
        let new_words = n.div_ceil(64);
        self.words.truncate(new_words);
        let tail = n % 64;
        if tail != 0 {
            self.words[new_words - 1] &= (1u64 << tail) - 1;
        }
        let ones: usize = self.words.iter().map(|w| w.count_ones() as usize).sum();
        self.len = n;
        self.nulls = n - ones;
        if self.nulls == 0 {
            self.words.clear();
        }
    }

    fn rebuild(&mut self, range: std::ops::Range<usize>) {
        let start = range.start.min(self.len);
        let end = range.end.min(self.len);
        if start >= end {
            *self = Validity::default();
            return;
        }
        if self.nulls == 0 {
            self.len = end - start;
            return;
        }
        if start == 0 {
            self.truncate(end);
            return;
        }
        let n = end - start;
        let out_words = n.div_ceil(64);
        let word_shift = start / 64;
        let bit_shift = start % 64;
        if bit_shift == 0 {
            self.words.drain(0..word_shift);
            self.words.truncate(out_words);
        } else {
            for w in 0..out_words {
                let lo = self.words[word_shift + w] >> bit_shift;
                let hi = self
                    .words
                    .get(word_shift + w + 1)
                    .copied()
                    .unwrap_or(0)
                    << (64 - bit_shift);
                self.words[w] = lo | hi;
            }
            self.words.truncate(out_words);
        }
        let tail = n % 64;
        if tail != 0 {
            self.words[out_words - 1] &= (1u64 << tail) - 1;
        }
        let ones: usize = self.words.iter().map(|w| w.count_ones() as usize).sum();
        self.len = n;
        self.nulls = n - ones;
        if self.nulls == 0 {
            self.words.clear();
        }
    }

    fn extend(&mut self, other: &Validity) {
        if other.len == 0 {
            return;
        }
        if self.len == 0 {
            *self = other.clone();
            return;
        }
        if self.nulls == 0 && other.nulls == 0 {
            self.len += other.len;
            return;
        }
        if self.nulls == 0 {
            let cur_words = self.len.div_ceil(64);
            self.words.resize(cur_words, u64::MAX);
            let tail = self.len % 64;
            if tail != 0 {
                self.words[cur_words - 1] = (1u64 << tail) - 1;
            }
        }
        let new_len = self.len + other.len;
        let new_words = new_len.div_ceil(64);
        let bit_off = self.len % 64;
        if other.nulls == 0 {
            if bit_off == 0 {
                self.words.resize(new_words, u64::MAX);
                let end_tail = new_len % 64;
                if end_tail != 0 {
                    self.words[new_words - 1] = (1u64 << end_tail) - 1;
                }
            } else if new_words == self.words.len() {
                let mask = ((1u64 << other.len) - 1) << bit_off;
                *self.words.last_mut().unwrap() |= mask;
            } else {
                *self.words.last_mut().unwrap() |= u64::MAX << bit_off;
                self.words.resize(new_words, u64::MAX);
                let end_tail = new_len % 64;
                if end_tail != 0 {
                    self.words[new_words - 1] = (1u64 << end_tail) - 1;
                }
            }
            self.len = new_len;
        } else {
            if bit_off == 0 {
                self.words.extend_from_slice(&other.words);
            } else {
                self.words.reserve(new_words - self.words.len());
                let last_idx = self.words.len() - 1;
                self.words[last_idx] |= other.words[0] << bit_off;
                for w in 1..other.words.len() {
                    let prev_hi = other.words[w - 1] >> (64 - bit_off);
                    let cur_lo = other.words[w] << bit_off;
                    self.words.push(prev_hi | cur_lo);
                }
                if self.words.len() < new_words {
                    self.words
                        .push(other.words[other.words.len() - 1] >> (64 - bit_off));
                }
            }
            self.len = new_len;
            self.nulls += other.nulls;
        }
    }

    #[inline]
    pub(crate) fn raw_parts(&self) -> (&[u64], usize, usize) {
        (&self.words, self.len, self.nulls)
    }

    pub(crate) fn from_raw_parts(
        words: Vec<u64>,
        len: usize,
        nulls: usize,
    ) -> Result<Self, Error> {
        if nulls > len {
            return Err(Error::runtime(format!(
                "batch: invalid validity nulls ({nulls}) > len ({len})"
            )));
        }
        let expected_words = if nulls == 0 { 0 } else { len.div_ceil(64) };
        if words.len() != expected_words {
            return Err(Error::runtime(format!(
                "batch: invalid validity word count {}, expected {expected_words}",
                words.len()
            )));
        }
        Ok(Self { words, len, nulls })
    }
}

/// One column of a [`RecordBatch`].
///
/// Integers, timestamps, floats and booleans are stored as dense native buffers (8 or 1
/// bytes per value instead of a 24-byte tagged [`Cell`]) with a bit-packed null mask, so
/// numeric kernels can run over plain slices. Text, vectors and columns that mix types keep
/// the general `Cell` representation. A column picks its representation from the first
/// non-null value and falls back to `Cells` if a later value has another type.
#[derive(Debug, Clone)]
pub enum Column {
    /// Only nulls so far; the first value decides the representation.
    Nulls { n: usize, cap: usize },
    Int { vals: Vec<i64>, valid: Validity },
    Time { vals: Vec<i64>, valid: Validity },
    Float { vals: Vec<f64>, valid: Validity },
    Bool { vals: Vec<bool>, valid: Validity },
    /// Contiguous UTF-8 buffer with `len + 1` byte offsets (`offsets[0] == 0`).
    Text {
        offsets: Vec<u32>,
        data: Vec<u8>,
        valid: Validity,
    },
    /// Dictionary-encoded text column (`values` table + `u32` code per row).
    Dict {
        values: Vec<Arc<str>>,
        codes: Vec<u32>,
        valid: Validity,
    },
    Cells(Vec<Cell>),
}

impl Default for Column {
    fn default() -> Self {
        Column::new()
    }
}

impl Column {
    pub fn new() -> Self {
        Column::Nulls { n: 0, cap: 0 }
    }

    pub fn with_capacity(cap: usize) -> Self {
        Column::Nulls { n: 0, cap }
    }

    pub fn len(&self) -> usize {
        match self {
            Column::Nulls { n, .. } => *n,
            Column::Int { vals, .. } | Column::Time { vals, .. } => vals.len(),
            Column::Float { vals, .. } => vals.len(),
            Column::Bool { vals, .. } => vals.len(),
            Column::Text { valid, .. } | Column::Dict { valid, .. } => valid.len,
            Column::Cells(c) => c.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// True for the dense native representations.
    pub fn is_typed(&self) -> bool {
        !matches!(self, Column::Cells(_) | Column::Nulls { .. })
    }

    #[inline]
    pub fn push(&mut self, cell: Cell) {
        if let Column::Cells(c) = self {
            c.push(cell);
            return;
        }
        match (&mut *self, cell) {
            (Column::Nulls { n, .. }, Cell::Null) => *n += 1,
            (Column::Nulls { n, cap }, cell) => {
                let (n, cap) = (*n, (*cap).max(*n + 1));
                *self = match &cell {
                    Cell::Int(_) => Column::Int {
                        vals: prefilled(n, cap, 0i64),
                        valid: Validity::all_null(n),
                    },
                    Cell::Time(_) => Column::Time {
                        vals: prefilled(n, cap, 0i64),
                        valid: Validity::all_null(n),
                    },
                    Cell::Float(_) => Column::Float {
                        vals: prefilled(n, cap, 0.0f64),
                        valid: Validity::all_null(n),
                    },
                    Cell::Bool(_) => Column::Bool {
                        vals: prefilled(n, cap, false),
                        valid: Validity::all_null(n),
                    },
                    _ => Column::Cells(prefilled(n, cap, Cell::Null)),
                };
                self.push(cell);
            }
            (Column::Int { vals, valid }, Cell::Int(v))
            | (Column::Time { vals, valid }, Cell::Time(v)) => {
                vals.push(v);
                valid.push(true);
            }
            (Column::Float { vals, valid }, Cell::Float(v)) => {
                vals.push(v);
                valid.push(true);
            }
            (Column::Bool { vals, valid }, Cell::Bool(v)) => {
                vals.push(v);
                valid.push(true);
            }
            (Column::Int { vals, valid } | Column::Time { vals, valid }, Cell::Null) => {
                vals.push(0);
                valid.push(false);
            }
            (Column::Float { vals, valid }, Cell::Null) => {
                vals.push(0.0);
                valid.push(false);
            }
            (Column::Bool { vals, valid }, Cell::Null) => {
                vals.push(false);
                valid.push(false);
            }
            (
                Column::Text {
                    offsets,
                    data,
                    valid,
                },
                Cell::Null,
            ) => {
                let cur_off = data.len() as u32;
                offsets.push(cur_off);
                valid.push(false);
            }
            (
                Column::Text {
                    offsets,
                    data,
                    valid,
                },
                cell,
            ) if cell.text().is_some() => {
                let s = cell.text().unwrap();
                if let Some(next_len) = data.len().checked_add(s.len())
                    && u32::try_from(next_len).is_ok()
                {
                    data.extend_from_slice(s.as_bytes());
                    offsets.push(data.len() as u32);
                    valid.push(true);
                } else {
                    let mut cells = std::mem::take(self).into_cells();
                    cells.push(cell);
                    *self = Column::Cells(cells);
                }
            }
            (Column::Dict { codes, valid, .. }, Cell::Null) => {
                codes.push(0);
                valid.push(false);
            }
            (
                Column::Dict {
                    values,
                    codes,
                    valid,
                },
                cell,
            ) if cell.text().is_some() => {
                let s = cell.text().unwrap();
                let code = match values.iter().position(|v| v.as_ref() == s) {
                    Some(idx) => idx as u32,
                    None => {
                        let idx = values.len() as u32;
                        values.push(cell.text_shared().unwrap_or_else(|| Arc::from(s)));
                        idx
                    }
                };
                codes.push(code);
                valid.push(true);
            }
            (Column::Cells(c), cell) => c.push(cell),
            (_, cell) => {
                // A value of another type: give up the dense representation.
                let mut cells = std::mem::take(self).into_cells();
                cells.push(cell);
                *self = Column::Cells(cells);
            }
        }
    }

    #[inline]
    pub fn push_float_opt(&mut self, v: Option<f64>) {
        if let Column::Float { vals, valid } = self {
            match v {
                Some(x) => {
                    vals.push(x);
                    valid.push(true);
                }
                None => {
                    vals.push(0.0);
                    valid.push(false);
                }
            }
            return;
        }
        self.push(match v {
            Some(x) => Cell::Float(x),
            None => Cell::Null,
        });
    }

    #[inline]
    pub fn push_int_opt(&mut self, v: Option<i64>) {
        if let Column::Int { vals, valid } = self {
            match v {
                Some(x) => {
                    vals.push(x);
                    valid.push(true);
                }
                None => {
                    vals.push(0);
                    valid.push(false);
                }
            }
            return;
        }
        self.push(match v {
            Some(x) => Cell::Int(x),
            None => Cell::Null,
        });
    }

    /// Value at `i` (`Cell::Null` when absent or out of range).
    pub fn get(&self, i: usize) -> Cell {
        match self {
            Column::Nulls { .. } => Cell::Null,
            Column::Int { vals, valid } if valid.is_valid(i) => Cell::Int(vals[i]),
            Column::Time { vals, valid } if valid.is_valid(i) => Cell::Time(vals[i]),
            Column::Float { vals, valid } if valid.is_valid(i) => Cell::Float(vals[i]),
            Column::Bool { vals, valid } if valid.is_valid(i) => Cell::Bool(vals[i]),
            Column::Text {
                offsets,
                data,
                valid,
            } if valid.is_valid(i) => {
                let s = offsets[i] as usize;
                let e = offsets[i + 1] as usize;
                // SAFETY: `Column::Text` only accepts validated UTF-8 slices.
                let txt = std::str::from_utf8(&data[s..e]).unwrap_or("");
                Cell::from_text(txt)
            }
            Column::Dict {
                values,
                codes,
                valid,
            } if valid.is_valid(i) => values
                .get(codes[i] as usize)
                .map(Cell::from_arc_str)
                .unwrap_or(Cell::Null),
            Column::Cells(c) => c.get(i).map(Cell::clone_for_borrow).unwrap_or(Cell::Null),
            _ => Cell::Null,
        }
    }

    /// Borrowed string slice at `i` across `Text`, `Dict`, and `Cells` representations.
    #[inline]
    pub fn text_at(&self, i: usize) -> Option<&str> {
        match self {
            Column::Text {
                offsets,
                data,
                valid,
            } if valid.is_valid(i) => {
                let s = offsets[i] as usize;
                let e = offsets[i + 1] as usize;
                std::str::from_utf8(&data[s..e]).ok()
            }
            Column::Dict {
                values,
                codes,
                valid,
            } if valid.is_valid(i) => values.get(codes[i] as usize).map(Arc::as_ref),
            Column::Cells(c) => c.get(i).and_then(Cell::text),
            _ => None,
        }
    }

    pub fn is_null(&self, i: usize) -> bool {
        match self {
            Column::Nulls { .. } => true,
            Column::Int { valid, .. }
            | Column::Time { valid, .. }
            | Column::Float { valid, .. }
            | Column::Bool { valid, .. }
            | Column::Text { valid, .. }
            | Column::Dict { valid, .. } => !valid.is_valid(i),
            Column::Cells(c) => c.get(i).is_none_or(|c| matches!(c, Cell::Null)),
        }
    }

    /// Borrowed cell, available only for the general representation.
    pub fn cell_ref(&self, i: usize) -> Option<&Cell> {
        match self {
            Column::Cells(c) => c.get(i),
            _ => None,
        }
    }

    /// Dense integer buffer and its null mask (`Int` columns only).
    pub fn as_i64(&self) -> Option<(&[i64], &Validity)> {
        match self {
            Column::Int { vals, valid } => Some((vals, valid)),
            _ => None,
        }
    }

    /// Dense float buffer and its null mask (`Float` columns only).
    pub fn as_f64(&self) -> Option<(&[f64], &Validity)> {
        match self {
            Column::Float { vals, valid } => Some((vals, valid)),
            _ => None,
        }
    }

    /// Contiguous UTF-8 offsets/bytes and null mask (`Text` columns only).
    pub fn as_text(&self) -> Option<(&[u32], &[u8], &Validity)> {
        match self {
            Column::Text {
                offsets,
                data,
                valid,
            } => Some((offsets, data, valid)),
            _ => None,
        }
    }

    /// Dictionary values, row codes, and null mask (`Dict` columns only).
    pub fn as_dict(&self) -> Option<(&[Arc<str>], &[u32], &Validity)> {
        match self {
            Column::Dict {
                values,
                codes,
                valid,
            } => Some((values, codes, valid)),
            _ => None,
        }
    }

    /// Construct an empty dense `Column::Text` with pre-allocated row and byte capacities.
    pub fn new_text_with_capacity(row_cap: usize, byte_cap: usize) -> Self {
        let mut offsets = Vec::with_capacity(row_cap + 1);
        offsets.push(0);
        Column::Text {
            offsets,
            data: Vec::with_capacity(byte_cap),
            valid: Validity::default(),
        }
    }

    /// Construct a dense `Column::Text` from optional string slices.
    pub fn from_strs<'a>(iter: impl IntoIterator<Item = Option<&'a str>>) -> Self {
        let it = iter.into_iter();
        let (lo, _) = it.size_hint();
        let mut col = Self::new_text_with_capacity(lo, lo * 8);
        for opt in it {
            match (&mut col, opt) {
                (
                    Column::Text {
                        offsets,
                        data,
                        valid,
                    },
                    Some(s),
                ) if data.len().checked_add(s.len()).is_some_and(|n| u32::try_from(n).is_ok()) => {
                    data.extend_from_slice(s.as_bytes());
                    offsets.push(data.len() as u32);
                    valid.push(true);
                }
                (
                    Column::Text {
                        offsets,
                        data,
                        valid,
                    },
                    None,
                ) => {
                    offsets.push(data.len() as u32);
                    valid.push(false);
                }
                (_, Some(s)) => col.push(Cell::from_text(s)),
                (_, None) => col.push(Cell::Null),
            }
        }
        col
    }

    /// Construct a `Column::Dict` from a dictionary, row codes, and validity mask.
    pub fn from_dict(values: Vec<Arc<str>>, codes: Vec<u32>, valid: Validity) -> Result<Self, Error> {
        if codes.len() != valid.len {
            return Err(Error::runtime("batch: dict codes length mismatch"));
        }
        for (i, &c) in codes.iter().enumerate() {
            if valid.is_valid(i) && (c as usize) >= values.len() {
                return Err(Error::runtime("batch: dict code out of bounds"));
            }
        }
        Ok(Column::Dict {
            values,
            codes,
            valid,
        })
    }

    /// Dictionary-encode a text-bearing column (`Text`, `Dict`, or all-text/null `Cells`).
    /// Returns `None` if any non-null element is not text.
    pub fn encode_dict(&self) -> Option<Self> {
        if matches!(self, Column::Dict { .. }) {
            return Some(self.clone());
        }
        let n = self.len();
        let mut map: rustc_hash::FxHashMap<&str, u32> = rustc_hash::FxHashMap::default();
        let mut values: Vec<Arc<str>> = Vec::new();
        let mut codes: Vec<u32> = Vec::with_capacity(n);
        let mut valid = Validity::default();
        for i in 0..n {
            if self.is_null(i) {
                codes.push(0);
                valid.push(false);
            } else {
                let s = self.text_at(i)?;
                let code = match map.get(s) {
                    Some(&c) => c,
                    None => {
                        let c = values.len() as u32;
                        let arc: Arc<str> = match self {
                            Column::Cells(cs) => cs[i].text_shared().unwrap_or_else(|| Arc::from(s)),
                            _ => Arc::from(s),
                        };
                        values.push(arc);
                        codes.push(c);
                        valid.push(true);
                        // Re-borrow from `values.last()` so `map` holds `'a` tied to `self`?
                        // Wait: `s` already borrows from `self` (`'self`)!
                        map.insert(s, c);
                        continue;
                    }
                };
                codes.push(code);
                valid.push(true);
            }
        }
        Some(Column::Dict {
            values,
            codes,
            valid,
        })
    }

    /// Sum of the numeric values, skipping nulls (`i128`-exact for `Int` and Neumaier-compensated for `Float`); `None` for non-numeric columns.
    pub fn sum_f64(&self) -> Option<f64> {
        match self {
            Column::Nulls { .. } => Some(0.0),
            Column::Float { vals, valid } => {
                let mut acc = crate::exact_sum::ExactSum::default();
                if valid.null_count() == 0 {
                    acc.add_f64_slice(vals);
                } else {
                    for (i, &v) in vals.iter().enumerate() {
                        if valid.is_valid(i) {
                            acc.add_f64(v);
                        }
                    }
                }
                Some(acc.value())
            }
            Column::Int { vals, valid } => {
                let mut acc = crate::exact_sum::ExactSum::default();
                if valid.null_count() == 0 {
                    acc.add_i64_slice(vals);
                } else {
                    for (i, &v) in vals.iter().enumerate() {
                        if valid.is_valid(i) {
                            acc.add_i64(v);
                        }
                    }
                }
                Some(acc.value())
            }
            _ => None,
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = Cell> + '_ {
        (0..self.len()).map(move |i| self.get(i))
    }

    pub fn into_cells(self) -> Vec<Cell> {
        match self {
            Column::Cells(c) => c,
            other => other.iter().collect(),
        }
    }

    #[inline]
    pub(crate) fn from_ints(vals: Vec<i64>) -> Self {
        let len = vals.len();
        Column::Int {
            vals,
            valid: Validity::all_valid(len),
        }
    }

    #[inline]
    pub(crate) fn from_floats(vals: Vec<f64>) -> Self {
        let len = vals.len();
        Column::Float {
            vals,
            valid: Validity::all_valid(len),
        }
    }

    #[inline]
    pub(crate) fn gather(&self, indices: &[u32]) -> Self {
        self.gather_u32(indices)
    }

    #[allow(dead_code)]
    pub(crate) fn gather_u32(&self, indices: &[u32]) -> Column {
        let n = indices.len();
        if n == 0 {
            return match self {
                Column::Nulls { .. } => Column::Nulls { n: 0, cap: 0 },
                Column::Int { .. } => Column::Int {
                    vals: Vec::new(),
                    valid: Validity::all_valid(0),
                },
                Column::Time { .. } => Column::Time {
                    vals: Vec::new(),
                    valid: Validity::all_valid(0),
                },
                Column::Float { .. } => Column::Float {
                    vals: Vec::new(),
                    valid: Validity::all_valid(0),
                },
                Column::Bool { .. } => Column::Bool {
                    vals: Vec::new(),
                    valid: Validity::all_valid(0),
                },
                Column::Text { .. } => Column::new_text_with_capacity(0, 0),
                Column::Dict { .. } => Column::Dict {
                    values: Vec::new(),
                    codes: Vec::new(),
                    valid: Validity::all_valid(0),
                },
                Column::Cells(_) => Column::Cells(Vec::new()),
            };
        }
        match self {
            Column::Nulls { .. } => Column::Nulls { n, cap: n },
            Column::Int { vals, valid } => Column::Int {
                vals: indices.iter().map(|&i| vals[i as usize]).collect(),
                valid: gather_validity(valid, indices),
            },
            Column::Time { vals, valid } => Column::Time {
                vals: indices.iter().map(|&i| vals[i as usize]).collect(),
                valid: gather_validity(valid, indices),
            },
            Column::Float { vals, valid } => Column::Float {
                vals: indices.iter().map(|&i| vals[i as usize]).collect(),
                valid: gather_validity(valid, indices),
            },
            Column::Bool { vals, valid } => Column::Bool {
                vals: indices.iter().map(|&i| vals[i as usize]).collect(),
                valid: gather_validity(valid, indices),
            },
            Column::Text {
                offsets,
                data,
                valid,
            } => {
                if valid.null_count() == 0 {
                    let total_bytes: usize = indices
                        .iter()
                        .map(|&i| {
                            let idx = i as usize;
                            (offsets[idx + 1] - offsets[idx]) as usize
                        })
                        .sum();
                    if u32::try_from(total_bytes).is_err() {
                        return Column::Cells(
                            indices.iter().map(|&i| self.get(i as usize)).collect(),
                        );
                    }
                    let mut out_offsets = Vec::with_capacity(n + 1);
                    out_offsets.push(0);
                    let mut out_data = Vec::with_capacity(total_bytes);
                    for &i in indices {
                        let idx = i as usize;
                        let s = offsets[idx] as usize;
                        let e = offsets[idx + 1] as usize;
                        out_data.extend_from_slice(&data[s..e]);
                        out_offsets.push(out_data.len() as u32);
                    }
                    Column::Text {
                        offsets: out_offsets,
                        data: out_data,
                        valid: Validity::all_valid(n),
                    }
                } else {
                    let out_valid = gather_validity(valid, indices);
                    let valid_words = &valid.words;
                    let total_bytes: usize = indices
                        .iter()
                        .map(|&i| {
                            let idx = i as usize;
                            if (valid_words[idx >> 6] >> (idx & 63)) & 1 != 0 {
                                (offsets[idx + 1] - offsets[idx]) as usize
                            } else {
                                0
                            }
                        })
                        .sum();
                    if u32::try_from(total_bytes).is_err() {
                        return Column::Cells(
                            indices.iter().map(|&i| self.get(i as usize)).collect(),
                        );
                    }
                    let mut out_offsets = Vec::with_capacity(n + 1);
                    out_offsets.push(0);
                    let mut out_data = Vec::with_capacity(total_bytes);
                    for &i in indices {
                        let idx = i as usize;
                        if (valid_words[idx >> 6] >> (idx & 63)) & 1 != 0 {
                            let s = offsets[idx] as usize;
                            let e = offsets[idx + 1] as usize;
                            out_data.extend_from_slice(&data[s..e]);
                        }
                        out_offsets.push(out_data.len() as u32);
                    }
                    Column::Text {
                        offsets: out_offsets,
                        data: out_data,
                        valid: out_valid,
                    }
                }
            }
            Column::Dict {
                values,
                codes,
                valid,
            } => Column::Dict {
                values: values.clone(),
                codes: indices.iter().map(|&i| codes[i as usize]).collect(),
                valid: gather_validity(valid, indices),
            },
            Column::Cells(c) => Column::Cells(
                indices
                    .iter()
                    .map(|&i| c[i as usize].clone_for_borrow())
                    .collect(),
            ),
        }
    }

    #[inline]
    pub(crate) fn permute_in_place(&mut self, perm: &[u32]) {
        *self = self.gather(perm);
    }

    /// Reorder elements according to a permutation `indices` without duplicates, moving inline
    /// or shared text cells out of `self` to avoid atomic refcount bumps.
    pub(crate) fn gather_unique_owned(self, indices: &[u32]) -> Self {
        match self {
            Column::Cells(mut c) => Column::Cells(
                indices
                    .iter()
                    .map(|&i| std::mem::replace(&mut c[i as usize], Cell::Null))
                    .collect(),
            ),
            Column::Dict {
                values,
                codes,
                valid,
            } => Column::Dict {
                values: if indices.is_empty() { Vec::new() } else { values },
                codes: indices.iter().map(|&i| codes[i as usize]).collect(),
                valid: gather_validity(&valid, indices),
            },
            other => other.gather_u32(indices),
        }
    }

    pub fn reserve(&mut self, additional: usize) {
        match self {
            Column::Nulls { n, cap } => *cap = (*cap).max(*n + additional),
            Column::Int { vals, .. } | Column::Time { vals, .. } => vals.reserve(additional),
            Column::Float { vals, .. } => vals.reserve(additional),
            Column::Bool { vals, .. } => vals.reserve(additional),
            Column::Text { offsets, .. } => offsets.reserve(additional),
            Column::Dict { codes, .. } => codes.reserve(additional),
            Column::Cells(c) => c.reserve(additional),
        }
    }

    pub fn truncate(&mut self, n: usize) {
        if n >= self.len() {
            return;
        }
        match self {
            Column::Nulls { n: len, .. } => *len = n,
            Column::Int { vals, valid } | Column::Time { vals, valid } => {
                vals.truncate(n);
                valid.truncate(n);
            }
            Column::Float { vals, valid } => {
                vals.truncate(n);
                valid.truncate(n);
            }
            Column::Bool { vals, valid } => {
                vals.truncate(n);
                valid.truncate(n);
            }
            Column::Text {
                offsets,
                data,
                valid,
            } => {
                offsets.truncate(n + 1);
                let new_byte_len = offsets.last().copied().unwrap_or(0) as usize;
                data.truncate(new_byte_len);
                valid.truncate(n);
            }
            Column::Dict { codes, valid, .. } => {
                codes.truncate(n);
                valid.truncate(n);
            }
            Column::Cells(c) => c.truncate(n),
        }
    }

    pub fn clear(&mut self) {
        self.truncate(0);
    }

    /// Drop the first `n` values.
    pub fn drain_prefix(&mut self, n: usize) {
        let len = self.len();
        if n == 0 {
            return;
        }
        if n >= len {
            self.clear();
            return;
        }
        match self {
            Column::Nulls { n: l, .. } => *l -= n,
            Column::Int { vals, valid } | Column::Time { vals, valid } => {
                vals.drain(0..n);
                valid.rebuild(n..len);
            }
            Column::Float { vals, valid } => {
                vals.drain(0..n);
                valid.rebuild(n..len);
            }
            Column::Bool { vals, valid } => {
                vals.drain(0..n);
                valid.rebuild(n..len);
            }
            Column::Text {
                offsets,
                data,
                valid,
            } => {
                let drop_bytes = offsets[n];
                data.drain(0..drop_bytes as usize);
                offsets.drain(0..n);
                for off in offsets.iter_mut() {
                    *off -= drop_bytes;
                }
                valid.rebuild(n..len);
            }
            Column::Dict { codes, valid, .. } => {
                codes.drain(0..n);
                valid.rebuild(n..len);
            }
            Column::Cells(c) => {
                c.drain(0..n);
            }
        }
    }

    /// Append every value of `other`.
    pub fn extend(&mut self, other: Column) {
        if other.is_empty() {
            return;
        }
        if self.is_empty() && !matches!(self, Column::Cells(_)) {
            let want_cap = match self {
                Column::Nulls { cap, .. } => *cap,
                _ => 0,
            };
            *self = other;
            if want_cap > self.len() {
                self.reserve(want_cap - self.len());
            }
            return;
        }
        match (&mut *self, other) {
            (Column::Cells(a), Column::Cells(b)) => a.extend(b),
            (Column::Int { vals: a, valid: va }, Column::Int { vals: b, valid: vb })
            | (Column::Time { vals: a, valid: va }, Column::Time { vals: b, valid: vb }) => {
                va.extend(&vb);
                a.extend(b);
            }
            (Column::Float { vals: a, valid: va }, Column::Float { vals: b, valid: vb }) => {
                va.extend(&vb);
                a.extend(b);
            }
            (Column::Bool { vals: a, valid: va }, Column::Bool { vals: b, valid: vb }) => {
                va.extend(&vb);
                a.extend(b);
            }
            (
                Column::Text {
                    offsets: oa,
                    data: da,
                    valid: va,
                },
                Column::Text {
                    offsets: ob,
                    data: db,
                    valid: vb,
                },
            ) if da.len().checked_add(db.len()).is_some_and(|n| u32::try_from(n).is_ok()) => {
                let base = da.len() as u32;
                da.extend_from_slice(&db);
                oa.extend(ob[1..].iter().map(|&o| base + o));
                va.extend(&vb);
            }
            (
                Column::Dict {
                    values: va_vals,
                    codes: ca,
                    valid: va,
                },
                Column::Dict {
                    values: vb_vals,
                    codes: cb,
                    valid: vb,
                },
            ) => {
                let same_dict = va_vals.len() == vb_vals.len()
                    && va_vals
                        .iter()
                        .zip(vb_vals.iter())
                        .all(|(a, b)| Arc::ptr_eq(a, b) || a == b);
                if same_dict {
                    va.extend(&vb);
                    ca.extend(cb);
                } else {
                    let mut remap = Vec::with_capacity(vb_vals.len());
                    if va_vals.len() + vb_vals.len() <= 32 {
                        for s in vb_vals {
                            if let Some(pos) =
                                va_vals.iter().position(|x| Arc::ptr_eq(x, &s) || x == &s)
                            {
                                remap.push(pos as u32);
                            } else {
                                let pos = va_vals.len() as u32;
                                va_vals.push(s);
                                remap.push(pos);
                            }
                        }
                    } else {
                        let mut idx_map: rustc_hash::FxHashMap<Arc<str>, u32> =
                            rustc_hash::FxHashMap::with_capacity_and_hasher(
                                va_vals.len() + vb_vals.len(),
                                Default::default(),
                            );
                        for (i, a) in va_vals.iter().enumerate() {
                            idx_map.entry(Arc::clone(a)).or_insert(i as u32);
                        }
                        for s in vb_vals {
                            if let Some(&pos) = idx_map.get(s.as_ref()) {
                                remap.push(pos);
                            } else {
                                let pos = va_vals.len() as u32;
                                idx_map.insert(Arc::clone(&s), pos);
                                va_vals.push(s);
                                remap.push(pos);
                            }
                        }
                    }
                    va.extend(&vb);
                    if vb.null_count() == 0 {
                        ca.extend(cb.into_iter().map(|code| remap[code as usize]));
                    } else {
                        ca.reserve(cb.len());
                        for (i, code) in cb.into_iter().enumerate() {
                            ca.push(if vb.is_valid(i) {
                                remap[code as usize]
                            } else {
                                0
                            });
                        }
                    }
                }
            }
            (_, Column::Nulls { n, .. }) => {
                self.reserve(n);
                for _ in 0..n {
                    self.push(Cell::Null);
                }
            }
            (_, other) => {
                self.reserve(other.len());
                for cell in other.into_cells() {
                    self.push(cell);
                }
            }
        }
    }

    /// Estimated heap bytes held by this column.
    pub fn heap_bytes(&self) -> usize {
        match self {
            Column::Nulls { .. } => 0,
            Column::Int { vals, valid } | Column::Time { vals, valid } => {
                vals.capacity() * 8 + valid.words.capacity() * 8
            }
            Column::Float { vals, valid } => vals.capacity() * 8 + valid.words.capacity() * 8,
            Column::Bool { vals, valid } => vals.capacity() + valid.words.capacity() * 8,
            Column::Text {
                offsets,
                data,
                valid,
            } => offsets.capacity() * 4 + data.capacity() + valid.words.capacity() * 8,
            Column::Dict {
                values,
                codes,
                valid,
            } => {
                codes.capacity() * 4
                    + valid.words.capacity() * 8
                    + values.capacity() * std::mem::size_of::<Arc<str>>()
                    + values.iter().map(|s| s.len()).sum::<usize>()
            }
            Column::Cells(c) => {
                let mut total = c.capacity() * std::mem::size_of::<Cell>();
                for cell in c {
                    match cell {
                        Cell::Text(s) => total += s.len(),
                        Cell::Vec(v) => total += v.len() * 4,
                        _ => {}
                    }
                }
                total
            }
        }
    }

    /// Borrowed zero-allocation view over this column's typed buffer and validity bitmap.
    #[inline]
    pub fn as_view(&self) -> ColumnView<'_> {
        match self {
            Column::Nulls { n, .. } => ColumnView::Nulls(*n),
            Column::Int { vals, valid } => ColumnView::Int(vals.as_slice(), valid),
            Column::Float { vals, valid } => ColumnView::Float(vals.as_slice(), valid),
            Column::Bool { vals, valid } => ColumnView::Bool(vals.as_slice(), valid),
            Column::Time { vals, valid } => ColumnView::Time(vals.as_slice(), valid),
            Column::Text {
                offsets,
                data,
                valid,
            } => ColumnView::Text {
                offsets: offsets.as_slice(),
                data: data.as_slice(),
                valid,
            },
            Column::Dict {
                values,
                codes,
                valid,
            } => ColumnView::Dict {
                values: values.as_slice(),
                codes: codes.as_slice(),
                valid,
            },
            Column::Cells(cells) => ColumnView::Mixed(cells.as_slice()),
        }
    }

    /// Slice `range` from this column, preserving dense representation, exact `null_count`,
    /// and sharing `Arc<str>` / `Arc<[f32]>` payloads without deep string copies.
    pub fn slice_range(&self, range: std::ops::Range<usize>) -> Column {
        let len = self.len();
        let start = range.start.min(len);
        let end = range.end.min(len);
        let n = end.saturating_sub(start);
        match self {
            Column::Nulls { .. } => Column::Nulls { n, cap: n },
            Column::Int { vals, valid } => Column::Int {
                vals: if n == 0 { Vec::new() } else { vals[start..end].to_vec() },
                valid: valid.slice_range(start..end),
            },
            Column::Time { vals, valid } => Column::Time {
                vals: if n == 0 { Vec::new() } else { vals[start..end].to_vec() },
                valid: valid.slice_range(start..end),
            },
            Column::Float { vals, valid } => Column::Float {
                vals: if n == 0 { Vec::new() } else { vals[start..end].to_vec() },
                valid: valid.slice_range(start..end),
            },
            Column::Bool { vals, valid } => Column::Bool {
                vals: if n == 0 { Vec::new() } else { vals[start..end].to_vec() },
                valid: valid.slice_range(start..end),
            },
            Column::Text {
                offsets,
                data,
                valid,
            } => {
                if n == 0 {
                    return Column::new_text_with_capacity(0, 0);
                }
                let base = offsets[start];
                let end_byte = offsets[end];
                let out_offsets: Vec<u32> = offsets[start..=end].iter().map(|&o| o - base).collect();
                let out_data = data[base as usize..end_byte as usize].to_vec();
                Column::Text {
                    offsets: out_offsets,
                    data: out_data,
                    valid: valid.slice_range(start..end),
                }
            }
            Column::Dict {
                values,
                codes,
                valid,
            } => Column::Dict {
                values: if n == 0 { Vec::new() } else { values.clone() },
                codes: if n == 0 { Vec::new() } else { codes[start..end].to_vec() },
                valid: valid.slice_range(start..end),
            },
            Column::Cells(cells) => Column::Cells(
                if n == 0 {
                    Vec::new()
                } else {
                    cells[start..end].iter().map(Cell::clone_for_borrow).collect()
                },
            ),
        }
    }

    /// Number of null entries in this column.
    pub fn null_count(&self) -> usize {
        match self {
            Column::Nulls { n, .. } => *n,
            Column::Int { valid, .. }
            | Column::Time { valid, .. }
            | Column::Float { valid, .. }
            | Column::Bool { valid, .. }
            | Column::Text { valid, .. }
            | Column::Dict { valid, .. } => valid.null_count(),
            Column::Cells(c) => c.iter().filter(|v| matches!(v, Cell::Null)).count(),
        }
    }
}

/// Zero-copy borrowed view over a [`Column`] (`D18`: `LIN-0699`).
#[derive(Debug, Clone, Copy)]
pub enum ColumnView<'a> {
    Nulls(usize),
    Int(&'a [i64], &'a Validity),
    Float(&'a [f64], &'a Validity),
    Bool(&'a [bool], &'a Validity),
    Time(&'a [i64], &'a Validity),
    Text {
        offsets: &'a [u32],
        data: &'a [u8],
        valid: &'a Validity,
    },
    Dict {
        values: &'a [Arc<str>],
        codes: &'a [u32],
        valid: &'a Validity,
    },
    Mixed(&'a [Cell]),
}

impl<'a> ColumnView<'a> {
    #[inline]
    pub fn len(&self) -> usize {
        match self {
            ColumnView::Nulls(n) => *n,
            ColumnView::Int(v, _) | ColumnView::Time(v, _) => v.len(),
            ColumnView::Float(v, _) => v.len(),
            ColumnView::Bool(v, _) => v.len(),
            ColumnView::Text { valid, .. } | ColumnView::Dict { valid, .. } => valid.len(),
            ColumnView::Mixed(c) => c.len(),
        }
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    #[inline]
    pub fn null_count(&self) -> usize {
        match self {
            ColumnView::Nulls(n) => *n,
            ColumnView::Int(_, valid)
            | ColumnView::Float(_, valid)
            | ColumnView::Bool(_, valid)
            | ColumnView::Time(_, valid)
            | ColumnView::Text { valid, .. }
            | ColumnView::Dict { valid, .. } => valid.null_count(),
            ColumnView::Mixed(c) => c.iter().filter(|cell| matches!(cell, Cell::Null)).count(),
        }
    }

    #[inline]
    pub fn text_at(&self, i: usize) -> Option<&'a str> {
        match self {
            ColumnView::Text {
                offsets,
                data,
                valid,
            } if valid.is_valid(i) => {
                let s = *offsets.get(i)? as usize;
                let e = *offsets.get(i + 1)? as usize;
                std::str::from_utf8(data.get(s..e)?).ok()
            }
            ColumnView::Dict {
                values,
                codes,
                valid,
            } if valid.is_valid(i) => values.get(*codes.get(i)? as usize).map(Arc::as_ref),
            ColumnView::Mixed(cells) => cells.get(i).and_then(Cell::text),
            _ => None,
        }
    }
}

/// Memory and cardinality ceiling for a [`RecordBatch`] (`D18`: `LIN-0700`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BatchAllocationBudget {
    pub max_rows: usize,
    pub max_bytes: usize,
}

/// Per-column memory breakdown for diagnostics and budget enforcement (`D18`: `LIN-0713`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnMemoryDiagnostics {
    pub name: String,
    pub kind: &'static str,
    pub len: usize,
    pub null_count: usize,
    pub payload_bytes: usize,
    pub validity_bytes: usize,
}

/// Safe Arrow C Data Interface schema boundary descriptor (`D18`: `LIN-0709`..`LIN-0712`).
///
/// Captures the exact Arrow format string, nullability, dictionary ordering, and buffer
/// ownership/release contract without exposing raw pointer lifetimes or `unsafe` FFI UB.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArrowSchemaDescriptor {
    pub name: String,
    pub format: &'static str,
    pub nullable: bool,
    pub dictionary_ordered: bool,
    pub children: Vec<ArrowSchemaDescriptor>,
    pub ownership_contract: &'static str,
}

/// Safe Arrow C Data Interface column export metadata (`D18`: `LIN-0710`..`LIN-0712`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArrowColumnExport {
    pub schema: ArrowSchemaDescriptor,
    pub length: usize,
    pub null_count: usize,
    pub offset: usize,
    pub buffer_lengths_bytes: Vec<usize>,
    pub release_required: bool,
}

fn prefilled<T: Clone>(n: usize, cap: usize, fill: T) -> Vec<T> {
    let mut v = Vec::with_capacity(cap.max(n));
    v.resize(n, fill);
    v
}

fn gather_validity(valid: &Validity, indices: &[u32]) -> Validity {
    let n = indices.len();
    if n == 0 || valid.null_count() == 0 {
        return Validity::all_valid(n);
    }
    let valid_words = &valid.words;
    let valid_len = valid.len;
    let num_words = n.div_ceil(64);
    let mut out_words: Vec<u64> = Vec::new();
    let mut ones = 0usize;
    for (w_idx, chunk) in indices.chunks(64).enumerate() {
        let mut word = 0u64;
        for (bit, &idx_u32) in chunk.iter().enumerate() {
            let i = idx_u32 as usize;
            debug_assert!(i < valid_len);
            let is_v = (valid_words[i >> 6] >> (i & 63)) & 1;
            word |= is_v << bit;
        }
        let full = if chunk.len() == 64 {
            u64::MAX
        } else {
            (1u64 << chunk.len()) - 1
        };
        ones += word.count_ones() as usize;
        if out_words.is_empty() {
            if word != full {
                out_words.reserve_exact(num_words);
                out_words.resize(w_idx, u64::MAX);
                out_words.push(word);
            }
        } else {
            out_words.push(word);
        }
    }
    let nulls = n - ones;
    if nulls == 0 {
        Validity::all_valid(n)
    } else {
        Validity {
            words: out_words,
            len: n,
            nulls,
        }
    }
}

#[inline(always)]
fn collect_matching_u32<T: Copy>(slice: &[T], out: &mut Vec<u32>, mut pred: impl FnMut(T) -> bool) {
    let n = slice.len();
    out.clear();
    if n == 0 {
        return;
    }
    out.reserve(n);
    // SAFETY: `out` is cleared and reserved for at least `n` elements. On step `i < n`,
    // `count <= i < n <= out.capacity()`, so `dst.add(count)` is always valid for writes,
    // and upon completion `0 <= count <= n` contiguous elements are initialized.
    unsafe {
        let dst = out.as_mut_ptr();
        let mut count = 0usize;
        for (i, &v) in slice.iter().enumerate() {
            *dst.add(count) = i as u32;
            count += usize::from(pred(v));
        }
        out.set_len(count);
    }
}

#[inline]
fn collect_matching_u32_nullable<T: Copy>(
    slice: &[T],
    valid: &Validity,
    null_matches: bool,
    out: &mut Vec<u32>,
    mut pred: impl FnMut(T) -> bool,
) {
    if valid.null_count() == 0 {
        collect_matching_u32(slice, out, pred);
        return;
    }
    let n = slice.len();
    out.clear();
    if n == 0 {
        return;
    }
    out.reserve(n);
    let words = &valid.words;
    // SAFETY: `out` has capacity >= `n`, and `count <= global_i < n` at every write.
    unsafe {
        let dst = out.as_mut_ptr();
        let mut count = 0usize;
        for (w_idx, chunk) in slice.chunks(64).enumerate() {
            let mask = words[w_idx];
            let base = w_idx * 64;
            for (bit, &v) in chunk.iter().enumerate() {
                *dst.add(count) = (base + bit) as u32;
                let is_v = ((mask >> bit) & 1) != 0;
                let matched = if is_v { pred(v) } else { null_matches };
                count += usize::from(matched);
            }
        }
        out.set_len(count);
    }
}

fn collect_all_or_valid(n: usize, valid: &Validity, include_nulls: bool, out: &mut Vec<u32>) {
    out.clear();
    if include_nulls || valid.null_count() == 0 {
        out.extend(0..n as u32);
        return;
    }
    let keep_len = n - valid.null_count();
    if keep_len == 0 {
        return;
    }
    out.reserve(keep_len);
    for (w_idx, &word) in valid.words.iter().enumerate() {
        let mut w = word;
        let base = (w_idx * 64) as u32;
        while w != 0 {
            let bit = w.trailing_zeros();
            out.push(base + bit);
            w &= w - 1;
        }
    }
}

fn eval_i64_slice_cmp(
    vals: &[i64],
    valid: &Validity,
    op: crate::ast::CmpOp,
    rv: i64,
    out: &mut Vec<u32>,
) {
    use crate::ast::CmpOp;
    match op {
        CmpOp::Eq => collect_matching_u32_nullable(vals, valid, false, out, |v| v == rv),
        CmpOp::Ne => collect_matching_u32_nullable(vals, valid, true, out, |v| v != rv),
        CmpOp::Lt => collect_matching_u32_nullable(vals, valid, false, out, |v| v < rv),
        CmpOp::Le => collect_matching_u32_nullable(vals, valid, false, out, |v| v <= rv),
        CmpOp::Gt => collect_matching_u32_nullable(vals, valid, false, out, |v| v > rv),
        CmpOp::Ge => collect_matching_u32_nullable(vals, valid, false, out, |v| v >= rv),
    }
}

fn eval_f64_slice_cmp(
    vals: &[f64],
    valid: &Validity,
    op: crate::ast::CmpOp,
    rf: f64,
    out: &mut Vec<u32>,
) {
    use crate::ast::CmpOp;
    if rf.is_nan() {
        out.clear();
        if matches!(op, CmpOp::Ne) {
            out.extend(0..vals.len() as u32);
        }
        return;
    }
    match op {
        CmpOp::Eq => collect_matching_u32_nullable(vals, valid, false, out, |v| v == rf),
        CmpOp::Ne => collect_matching_u32_nullable(vals, valid, true, out, |v| v != rf),
        CmpOp::Lt => collect_matching_u32_nullable(vals, valid, false, out, |v| v < rf),
        CmpOp::Le => collect_matching_u32_nullable(vals, valid, false, out, |v| v <= rf),
        CmpOp::Gt => collect_matching_u32_nullable(vals, valid, false, out, |v| v > rf),
        CmpOp::Ge => collect_matching_u32_nullable(vals, valid, false, out, |v| v >= rf),
    }
}

fn eval_col_cmp_indices(
    col: &Column,
    op: crate::ast::CmpOp,
    lit: &Cell,
    out: &mut Vec<u32>,
) -> bool {
    use crate::ast::CmpOp;
    match col {
        Column::Nulls { n, .. } => {
            out.clear();
            if matches!(op, CmpOp::Ne) {
                out.extend(0..*n as u32);
            }
            true
        }
        Column::Int { vals, valid } => match lit {
            Cell::Int(rv) => {
                eval_i64_slice_cmp(vals, valid, op, *rv, out);
                true
            }
            Cell::Float(rf) => {
                let rf = *rf;
                let n = vals.len();
                if rf.is_nan() {
                    out.clear();
                    if matches!(op, CmpOp::Ne) {
                        out.extend(0..n as u32);
                    }
                    return true;
                }
                const TWO_63: f64 = 9_223_372_036_854_775_808.0;
                match op {
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
                            (_, Some(fi)) => eval_i64_slice_cmp(vals, valid, op, fi, out),
                            (CmpOp::Eq, None) => out.clear(),
                            (CmpOp::Ne, None) => {
                                out.clear();
                                out.extend(0..n as u32);
                            }
                            _ => unreachable!(),
                        }
                    }
                    CmpOp::Lt | CmpOp::Le | CmpOp::Gt | CmpOp::Ge => {
                        if rf >= TWO_63 {
                            if matches!(op, CmpOp::Lt | CmpOp::Le) {
                                collect_all_or_valid(n, valid, false, out);
                            } else {
                                out.clear();
                            }
                        } else if rf < -TWO_63 {
                            if matches!(op, CmpOp::Gt | CmpOp::Ge) {
                                collect_all_or_valid(n, valid, false, out);
                            } else {
                                out.clear();
                            }
                        } else {
                            let fi = rf as i64;
                            let f_back = fi as f64;
                            if f_back == rf {
                                eval_i64_slice_cmp(vals, valid, op, fi, out);
                            } else if rf > f_back {
                                let eff_op = match op {
                                    CmpOp::Lt | CmpOp::Le => CmpOp::Le,
                                    CmpOp::Gt | CmpOp::Ge => CmpOp::Gt,
                                    _ => unreachable!(),
                                };
                                eval_i64_slice_cmp(vals, valid, eff_op, fi, out);
                            } else {
                                let eff_op = match op {
                                    CmpOp::Lt | CmpOp::Le => CmpOp::Lt,
                                    CmpOp::Gt | CmpOp::Ge => CmpOp::Ge,
                                    _ => unreachable!(),
                                };
                                eval_i64_slice_cmp(vals, valid, eff_op, fi, out);
                            }
                        }
                    }
                }
                true
            }
            _ => false,
        },
        Column::Time { vals, valid } => match lit {
            Cell::Time(rt) => {
                eval_i64_slice_cmp(vals, valid, op, *rt, out);
                true
            }
            _ => false,
        },
        Column::Float { vals, valid } => match lit {
            Cell::Float(rf) => {
                eval_f64_slice_cmp(vals, valid, op, *rf, out);
                true
            }
            Cell::Int(ri) => {
                let ri = *ri;
                const TWO_63: f64 = 9_223_372_036_854_775_808.0;
                let rf = ri as f64;
                if rf < TWO_63 && (rf as i64 == ri) {
                    eval_f64_slice_cmp(vals, valid, op, rf, out);
                } else {
                    match op {
                        CmpOp::Eq => collect_matching_u32_nullable(vals, valid, false, out, |v| {
                            crate::eval::int_eq_f64(ri, v)
                        }),
                        CmpOp::Ne => collect_matching_u32_nullable(vals, valid, true, out, |v| {
                            !crate::eval::int_eq_f64(ri, v)
                        }),
                        CmpOp::Lt | CmpOp::Le | CmpOp::Gt | CmpOp::Ge => {
                            collect_matching_u32_nullable(vals, valid, false, out, |v| {
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
                            });
                        }
                    }
                }
                true
            }
            _ => false,
        },
        Column::Bool { vals, valid } => match lit {
            Cell::Bool(rb) => {
                let rb = *rb;
                match op {
                    CmpOp::Eq => {
                        collect_matching_u32_nullable(vals, valid, false, out, |v| v == rb)
                    }
                    CmpOp::Ne => collect_matching_u32_nullable(vals, valid, true, out, |v| v != rb),
                    CmpOp::Lt | CmpOp::Le | CmpOp::Gt | CmpOp::Ge => out.clear(),
                }
                true
            }
            _ => false,
        },
        Column::Dict {
            values,
            codes,
            valid,
        } => {
            let Some(target) = lit.text() else {
                return false;
            };
            let n = codes.len();
            match op {
                CmpOp::Eq => {
                    let Some(tc) = values
                        .iter()
                        .position(|v| v.as_ref() == target)
                        .map(|p| p as u32)
                    else {
                        out.clear();
                        return true;
                    };
                    collect_matching_u32_nullable(codes, valid, false, out, |c| c == tc);
                }
                CmpOp::Ne => {
                    let Some(tc) = values
                        .iter()
                        .position(|v| v.as_ref() == target)
                        .map(|p| p as u32)
                    else {
                        out.clear();
                        out.extend(0..n as u32);
                        return true;
                    };
                    collect_matching_u32_nullable(codes, valid, true, out, |c| c != tc);
                }
                CmpOp::Lt | CmpOp::Le | CmpOp::Gt | CmpOp::Ge => {
                    if values.is_empty() {
                        out.clear();
                        return true;
                    }
                    let mut code_ok: smallvec::SmallVec<[bool; 64]> =
                        smallvec::SmallVec::with_capacity(values.len());
                    let mut any_ok = false;
                    for v in values {
                        let s = v.as_ref();
                        let ok = match op {
                            CmpOp::Lt => s < target,
                            CmpOp::Le => s <= target,
                            CmpOp::Gt => s > target,
                            CmpOp::Ge => s >= target,
                            _ => unreachable!(),
                        };
                        any_ok |= ok;
                        code_ok.push(ok);
                    }
                    if !any_ok {
                        out.clear();
                        return true;
                    }
                    let ok_slice = code_ok.as_slice();
                    collect_matching_u32_nullable(codes, valid, false, out, |c| {
                        ok_slice[c as usize]
                    });
                }
            }
            true
        }
        Column::Text {
            offsets,
            data,
            valid,
        } => {
            let Some(target) = lit.text() else {
                return false;
            };
            let tb = target.as_bytes();
            let tlen = tb.len() as u32;
            let n = valid.len;
            out.clear();
            if n == 0 {
                return true;
            }
            if valid.null_count() == 0 {
                match op {
                    CmpOp::Eq => {
                        for i in 0..n {
                            let s = offsets[i];
                            let e = offsets[i + 1];
                            if e - s == tlen && &data[s as usize..e as usize] == tb {
                                out.push(i as u32);
                            }
                        }
                    }
                    CmpOp::Ne => {
                        for i in 0..n {
                            let s = offsets[i];
                            let e = offsets[i + 1];
                            if e - s != tlen || &data[s as usize..e as usize] != tb {
                                out.push(i as u32);
                            }
                        }
                    }
                    CmpOp::Lt => {
                        for i in 0..n {
                            if &data[offsets[i] as usize..offsets[i + 1] as usize] < tb {
                                out.push(i as u32);
                            }
                        }
                    }
                    CmpOp::Le => {
                        for i in 0..n {
                            if &data[offsets[i] as usize..offsets[i + 1] as usize] <= tb {
                                out.push(i as u32);
                            }
                        }
                    }
                    CmpOp::Gt => {
                        for i in 0..n {
                            if &data[offsets[i] as usize..offsets[i + 1] as usize] > tb {
                                out.push(i as u32);
                            }
                        }
                    }
                    CmpOp::Ge => {
                        for i in 0..n {
                            if &data[offsets[i] as usize..offsets[i + 1] as usize] >= tb {
                                out.push(i as u32);
                            }
                        }
                    }
                }
            } else {
                let words = &valid.words;
                for i in 0..n {
                    let is_v = ((words[i >> 6] >> (i & 63)) & 1) != 0;
                    if !is_v {
                        if matches!(op, CmpOp::Ne) {
                            out.push(i as u32);
                        }
                        continue;
                    }
                    let s = offsets[i];
                    let e = offsets[i + 1];
                    let matched = match op {
                        CmpOp::Eq => e - s == tlen && &data[s as usize..e as usize] == tb,
                        CmpOp::Ne => e - s != tlen || &data[s as usize..e as usize] != tb,
                        CmpOp::Lt => &data[s as usize..e as usize] < tb,
                        CmpOp::Le => &data[s as usize..e as usize] <= tb,
                        CmpOp::Gt => &data[s as usize..e as usize] > tb,
                        CmpOp::Ge => &data[s as usize..e as usize] >= tb,
                    };
                    if matched {
                        out.push(i as u32);
                    }
                }
            }
            true
        }
        Column::Cells(_) => false,
    }
}

#[inline(always)]
fn float_sort_rank_u64(v: f64, is_valid: bool) -> u64 {
    if !is_valid {
        return 0;
    }
    if v.is_nan() {
        return u64::MAX;
    }
    let norm = if v == 0.0 { 0.0f64 } else { v };
    let bits = norm.to_bits();
    if (bits >> 63) != 0 {
        !bits
    } else {
        bits | (1u64 << 63)
    }
}

#[inline(always)]
fn text_prefix8_be(bytes: &[u8]) -> u64 {
    let mut buf = [0u8; 8];
    let take = bytes.len().min(8);
    buf[..take].copy_from_slice(&bytes[..take]);
    u64::from_be_bytes(buf)
}

fn select_and_sort_window<T>(
    dec: &mut [T],
    skip: usize,
    end: usize,
    mut cmp: impl FnMut(&T, &T) -> std::cmp::Ordering,
) {
    let n = dec.len();
    if n > 32 && end < n {
        dec.select_nth_unstable_by(end - 1, &mut cmp);
        let head = &mut dec[..end];
        if skip > 32 && end - skip < skip / 2 {
            head.select_nth_unstable_by(skip, &mut cmp);
            head[skip..].sort_unstable_by(&mut cmp);
        } else {
            head.sort_unstable_by(&mut cmp);
        }
    } else {
        dec.sort_unstable_by(&mut cmp);
    }
}

fn select_and_sort_window_ord<T: Ord>(dec: &mut [T], skip: usize, end: usize) {
    let n = dec.len();
    if n > 32 && end < n {
        dec.select_nth_unstable(end - 1);
        let head = &mut dec[..end];
        if skip > 32 && end - skip < skip / 2 {
            head.select_nth_unstable(skip);
            head[skip..].sort_unstable();
        } else {
            head.sort_unstable();
        }
    } else {
        dec.sort_unstable();
    }
}

fn sorted_col_indices(col: &Column, desc: bool, skip: usize, end: usize) -> Vec<u32> {
    let n = col.len();
    debug_assert!(skip < end && end <= n);
    match col {
        Column::Nulls { .. } => (skip as u32..end as u32).collect(),
        Column::Int { vals, valid } | Column::Time { vals, valid } => {
            if valid.null_count() == 0 {
                let mut dec: Vec<(u64, u32)> = vals
                    .iter()
                    .enumerate()
                    .map(|(i, &v)| {
                        let k = (v as u64) ^ (1u64 << 63);
                        (if desc { !k } else { k }, i as u32)
                    })
                    .collect();
                select_and_sort_window_ord(&mut dec, skip, end);
                dec[skip..end].iter().map(|&(_, idx)| idx).collect()
            } else {
                let words = &valid.words;
                let mut dec: Vec<(u8, u64, u32)> = vals
                    .iter()
                    .enumerate()
                    .map(|(i, &v)| {
                        let is_v = ((words[i >> 6] >> (i & 63)) & 1) != 0;
                        let (tag, key) = if is_v {
                            let k = (v as u64) ^ (1u64 << 63);
                            (if desc { 0u8 } else { 1u8 }, if desc { !k } else { k })
                        } else {
                            (if desc { 1u8 } else { 0u8 }, 0u64)
                        };
                        (tag, key, i as u32)
                    })
                    .collect();
                select_and_sort_window_ord(&mut dec, skip, end);
                dec[skip..end].iter().map(|&(_, _, idx)| idx).collect()
            }
        }
        Column::Float { vals, valid } => {
            let has_nulls = valid.null_count() > 0;
            let words = &valid.words;
            let mut dec: Vec<(u64, u32)> = vals
                .iter()
                .enumerate()
                .map(|(i, &v)| {
                    let is_v = !has_nulls || (((words[i >> 6] >> (i & 63)) & 1) != 0);
                    let rk = float_sort_rank_u64(v, is_v);
                    (if desc { !rk } else { rk }, i as u32)
                })
                .collect();
            select_and_sort_window_ord(&mut dec, skip, end);
            dec[skip..end].iter().map(|&(_, idx)| idx).collect()
        }
        Column::Bool { vals, valid } => {
            let has_nulls = valid.null_count() > 0;
            let words = &valid.words;
            let mut dec: Vec<u64> = vals
                .iter()
                .enumerate()
                .map(|(i, &b)| {
                    let is_v = !has_nulls || (((words[i >> 6] >> (i & 63)) & 1) != 0);
                    let rk: u32 = if !is_v { 0 } else if b { 2 } else { 1 };
                    let eff = if desc { !rk } else { rk };
                    ((eff as u64) << 32) | (i as u64)
                })
                .collect();
            select_and_sort_window_ord(&mut dec, skip, end);
            dec[skip..end].iter().map(|&p| p as u32).collect()
        }
        Column::Dict {
            values,
            codes,
            valid,
        } => {
            let mut d_ord: Vec<u32> = (0..values.len() as u32).collect();
            d_ord.sort_unstable_by(|&a, &b| values[a as usize].cmp(&values[b as usize]));
            let mut rank = vec![0u32; values.len()];
            if !d_ord.is_empty() {
                let mut cur_rank = 1u32;
                rank[d_ord[0] as usize] = cur_rank;
                for idx_d in 1..d_ord.len() {
                    if values[d_ord[idx_d - 1] as usize] != values[d_ord[idx_d] as usize] {
                        cur_rank += 1;
                    }
                    rank[d_ord[idx_d] as usize] = cur_rank;
                }
            }
            let has_nulls = valid.null_count() > 0;
            let words = &valid.words;
            let mut dec: Vec<u64> = codes
                .iter()
                .enumerate()
                .map(|(i, &c)| {
                    let rk = if has_nulls && (((words[i >> 6] >> (i & 63)) & 1) == 0) {
                        0u32
                    } else {
                        rank[c as usize]
                    };
                    let eff = if desc { !rk } else { rk };
                    ((eff as u64) << 32) | (i as u64)
                })
                .collect();
            select_and_sort_window_ord(&mut dec, skip, end);
            dec[skip..end].iter().map(|&p| p as u32).collect()
        }
        Column::Text {
            offsets,
            data,
            valid,
        } => {
            if valid.null_count() == 0 {
                let mut dec: Vec<(u64, u32)> = (0..n)
                    .map(|i| {
                        let s = offsets[i] as usize;
                        let e = offsets[i + 1] as usize;
                        let pref = text_prefix8_be(&data[s..e]);
                        (if desc { !pref } else { pref }, i as u32)
                    })
                    .collect();
                select_and_sort_window(&mut dec, skip, end, |a, b| {
                    let p_ord = a.0.cmp(&b.0);
                    if p_ord != std::cmp::Ordering::Equal {
                        return p_ord;
                    }
                    let ia = a.1 as usize;
                    let ib = b.1 as usize;
                    let sa = &data[offsets[ia] as usize..offsets[ia + 1] as usize];
                    let sb = &data[offsets[ib] as usize..offsets[ib + 1] as usize];
                    let ord = if desc { sb.cmp(sa) } else { sa.cmp(sb) };
                    ord.then_with(|| a.1.cmp(&b.1))
                });
                dec[skip..end].iter().map(|&(_, idx)| idx).collect()
            } else {
                let words = &valid.words;
                let mut dec: Vec<(u8, u64, u32)> = (0..n)
                    .map(|i| {
                        let is_v = ((words[i >> 6] >> (i & 63)) & 1) != 0;
                        if is_v {
                            let s = offsets[i] as usize;
                            let e = offsets[i + 1] as usize;
                            let pref = text_prefix8_be(&data[s..e]);
                            (
                                if desc { 0u8 } else { 1u8 },
                                if desc { !pref } else { pref },
                                i as u32,
                            )
                        } else {
                            (if desc { 1u8 } else { 0u8 }, 0u64, i as u32)
                        }
                    })
                    .collect();
                select_and_sort_window(&mut dec, skip, end, |a, b| {
                    let t_ord = a.0.cmp(&b.0);
                    if t_ord != std::cmp::Ordering::Equal {
                        return t_ord;
                    }
                    let a_is_null = (!desc && a.0 == 0) || (desc && a.0 == 1);
                    if a_is_null {
                        return a.2.cmp(&b.2);
                    }
                    let p_ord = a.1.cmp(&b.1);
                    if p_ord != std::cmp::Ordering::Equal {
                        return p_ord;
                    }
                    let ia = a.2 as usize;
                    let ib = b.2 as usize;
                    let sa = &data[offsets[ia] as usize..offsets[ia + 1] as usize];
                    let sb = &data[offsets[ib] as usize..offsets[ib + 1] as usize];
                    let ord = if desc { sb.cmp(sa) } else { sa.cmp(sb) };
                    ord.then_with(|| a.2.cmp(&b.2))
                });
                dec[skip..end].iter().map(|&(_, _, idx)| idx).collect()
            }
        }
        Column::Cells(cells) => {
            let mut dec: Vec<(&Cell, u32)> = cells
                .iter()
                .enumerate()
                .map(|(i, c)| (c, i as u32))
                .collect();
            select_and_sort_window(&mut dec, skip, end, |a, b| {
                let ord = match (a.0.text(), b.0.text()) {
                    (Some(sa), Some(sb)) => sa.cmp(sb),
                    _ => crate::eval::cmp_sort(a.0, b.0),
                };
                let ord = if desc { ord.reverse() } else { ord };
                ord.then_with(|| a.1.cmp(&b.1))
            });
            dec[skip..end].iter().map(|&(_, idx)| idx).collect()
        }
    }
}

/// Columns are equal when they hold the same values, whatever their representation.
impl PartialEq for Column {
    fn eq(&self, other: &Self) -> bool {
        let n = self.len();
        if n != other.len() {
            return false;
        }
        if n == 0 {
            return true;
        }
        match (self, other) {
            (Column::Nulls { .. }, Column::Nulls { .. }) => true,
            (Column::Cells(a), Column::Cells(b)) => a == b,
            (Column::Int { vals: a, valid: va }, Column::Int { vals: b, valid: vb })
            | (Column::Time { vals: a, valid: va }, Column::Time { vals: b, valid: vb }) => {
                if va.null_count() == 0 && vb.null_count() == 0 {
                    a == b
                } else {
                    va.null_count() == vb.null_count()
                        && (0..n).all(|i| match (va.is_valid(i), vb.is_valid(i)) {
                            (true, true) => a[i] == b[i],
                            (false, false) => true,
                            _ => false,
                        })
                }
            }
            (Column::Float { vals: a, valid: va }, Column::Float { vals: b, valid: vb }) => {
                if va.null_count() == 0 && vb.null_count() == 0 {
                    a.iter().zip(b).all(|(x, y)| x.to_bits() == y.to_bits())
                } else {
                    va.null_count() == vb.null_count()
                        && (0..n).all(|i| match (va.is_valid(i), vb.is_valid(i)) {
                            (true, true) => a[i].to_bits() == b[i].to_bits(),
                            (false, false) => true,
                            _ => false,
                        })
                }
            }
            (Column::Bool { vals: a, valid: va }, Column::Bool { vals: b, valid: vb }) => {
                if va.null_count() == 0 && vb.null_count() == 0 {
                    a == b
                } else {
                    va.null_count() == vb.null_count()
                        && (0..n).all(|i| match (va.is_valid(i), vb.is_valid(i)) {
                            (true, true) => a[i] == b[i],
                            (false, false) => true,
                            _ => false,
                        })
                }
            }
            (
                Column::Dict {
                    values: va_vals,
                    codes: ca,
                    valid: va,
                },
                Column::Dict {
                    values: vb_vals,
                    codes: cb,
                    valid: vb,
                },
            ) if va.null_count() == 0
                && vb.null_count() == 0
                && va_vals.len() == vb_vals.len()
                && va_vals
                    .iter()
                    .zip(vb_vals)
                    .all(|(x, y)| Arc::ptr_eq(x, y) || x == y) =>
            {
                ca == cb
            }
            (Column::Text { .. } | Column::Dict { .. }, Column::Text { .. } | Column::Dict { .. }) => {
                (0..n).all(|i| match (self.is_null(i), other.is_null(i)) {
                    (true, true) => true,
                    (false, false) => self.text_at(i) == other.text_at(i),
                    _ => false,
                })
            }
            (tc @ (Column::Text { .. } | Column::Dict { .. }), Column::Cells(cells))
            | (Column::Cells(cells), tc @ (Column::Text { .. } | Column::Dict { .. })) => {
                (0..n).all(|i| match (tc.is_null(i), &cells[i]) {
                    (true, Cell::Null) => true,
                    (false, c) => c.text().is_some_and(|s| Some(s) == tc.text_at(i)),
                    _ => false,
                })
            }
            _ => (0..n).all(|i| self.get(i) == other.get(i)),
        }
    }
}

impl From<Vec<Cell>> for Column {
    fn from(cells: Vec<Cell>) -> Self {
        let mut col = Column::with_capacity(cells.len());
        for cell in cells {
            col.push(cell);
        }
        col
    }
}

impl FromIterator<Cell> for Column {
    fn from_iter<I: IntoIterator<Item = Cell>>(iter: I) -> Self {
        let mut col = Column::new();
        for cell in iter {
            col.push(cell);
        }
        col
    }
}

/// Named columns of equal length. Prefer this for analytical project/join hot paths;
/// call [`RecordBatch::to_rows`] when the row-API is required.
#[derive(Debug, Clone, PartialEq)]
pub struct RecordBatch {
    pub names: Arc<[String]>,
    pub cols: Vec<Column>,
}

impl RecordBatch {
    pub fn empty(names: impl Into<Arc<[String]>>) -> Self {
        let names = names.into();
        let cols = (0..names.len()).map(|_| Column::new()).collect();
        Self { names, cols }
    }

    pub fn with_capacity(names: impl Into<Arc<[String]>>, cap: usize) -> Self {
        let names = names.into();
        let cols = (0..names.len()).map(|_| Column::with_capacity(cap)).collect();
        Self { names, cols }
    }

    pub fn n(&self) -> usize {
        self.cols.first().map(|c| c.len()).unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.n() == 0
    }

    /// Estimated byte size of column buffers and schema in memory.
    pub fn byte_size(&self) -> usize {
        let mut total = std::mem::size_of::<Self>();
        for name in self.names.iter() {
            total += name.len() + 16;
        }
        for col in &self.cols {
            total += std::mem::size_of::<Column>() + col.heap_bytes();
        }
        total
    }

    pub fn push_row(&mut self, cells: &[Cell]) {
        debug_assert_eq!(cells.len(), self.cols.len());
        for (col, cell) in self.cols.iter_mut().zip(cells.iter()) {
            col.push(cell.clone());
        }
    }

    pub fn push_cells(&mut self, cells: impl IntoIterator<Item = Cell>) {
        for (col, cell) in self.cols.iter_mut().zip(cells) {
            col.push(cell);
        }
    }

    /// Materialize row-maps (row-API) with zero-copy Cell cloning
    pub fn to_rows(&self) -> Vec<Row> {
        self.to_rows_batch(0, self.n())
    }

    /// Consume `self` and materialize rows, moving `Cells` out without atomic refcount churn
    /// and reusing pooled `Vec<Row>` buffers when available.
    pub fn into_rows(mut self) -> Vec<Row> {
        let n = self.n();
        if n == 0 {
            return Vec::new();
        }
        #[inline]
        fn take_col_cell(col: &mut Column, i: usize) -> Cell {
            match col {
                Column::Cells(c) => std::mem::replace(&mut c[i], Cell::Null),
                _ => col.get(i),
            }
        }
        let width = self.cols.len();
        let unique_small_names = (1..=8).contains(&width)
            && self.names.len() == width
            && (0..width).all(|i| (i + 1..width).all(|j| self.names[i] != self.names[j]));

        if n >= 65_536 && unique_small_names {
            let threads = std::thread::available_parallelism()
                .map(|p| p.get())
                .unwrap_or(1)
                .min(8)
                .min(n.div_ceil(8192));
            if threads > 1 {
                let shape = if self.names.as_ref() == ["id", "title"] {
                    Arc::clone(crate::store::shape_id_title())
                } else {
                    Row::shape_of(&self.names)
                };
                let mut inv_slots = [0usize; 8];
                for (pos, target) in shape.names().iter().enumerate() {
                    inv_slots[pos] = self
                        .names
                        .iter()
                        .position(|name| name == target)
                        .unwrap_or(pos);
                }
                // Extract `Column::Cells` buffers so workers receive disjoint `&mut [Cell]`
                // slices and move cells via `mem::replace` with zero atomic refcount traffic.
                let mut cell_vecs: smallvec::SmallVec<[Option<Vec<Cell>>; 8]> =
                    smallvec::SmallVec::with_capacity(width);
                for col in &mut self.cols {
                    if let Column::Cells(v) = col {
                        cell_vecs.push(Some(std::mem::take(v)));
                        *col = Column::Nulls { n: 0, cap: 0 };
                    } else {
                        cell_vecs.push(None);
                    }
                }
                let cols_ref = self.cols.as_slice();
                let chunk_size = n.div_ceil(threads);
                let mut out_buf: Vec<std::mem::MaybeUninit<Row>> = Vec::with_capacity(n);
                // SAFETY: Every element in `0..n` is initialized across disjoint `chunks_mut` below.
                unsafe {
                    out_buf.set_len(n);
                }
                let mut cell_chunk_iters: smallvec::SmallVec<
                    [Option<std::slice::ChunksMut<'_, Cell>>; 8],
                > = cell_vecs
                    .iter_mut()
                    .map(|opt| opt.as_mut().map(|v| v.chunks_mut(chunk_size)))
                    .collect();

                std::thread::scope(|scope| {
                    let mut c_start = 0usize;
                    for out_chunk in out_buf.chunks_mut(chunk_size) {
                        let c_len = out_chunk.len();
                        let mut worker_cells: smallvec::SmallVec<[Option<&mut [Cell]>; 8]> =
                            smallvec::SmallVec::with_capacity(width);
                        for it_opt in &mut cell_chunk_iters {
                            worker_cells.push(it_opt.as_mut().and_then(Iterator::next));
                        }
                        let shape_ref = &shape;
                        scope.spawn(move || {
                            if width == 2 {
                                let s0 = inv_slots[0];
                                let s1 = inv_slots[1];
                                for (local_i, slot_out) in out_chunk.iter_mut().enumerate() {
                                    let gi = c_start + local_i;
                                    let c0 = match &mut worker_cells[s0] {
                                        Some(sl) => std::mem::replace(&mut sl[local_i], Cell::Null),
                                        None => cols_ref[s0].get(gi),
                                    };
                                    let c1 = match &mut worker_cells[s1] {
                                        Some(sl) => std::mem::replace(&mut sl[local_i], Cell::Null),
                                        None => cols_ref[s1].get(gi),
                                    };
                                    slot_out.write(Row::from_parts(Arc::clone(shape_ref), [c0, c1]));
                                }
                            } else if width == 3 {
                                let (s0, s1, s2) = (inv_slots[0], inv_slots[1], inv_slots[2]);
                                for (local_i, slot_out) in out_chunk.iter_mut().enumerate() {
                                    let gi = c_start + local_i;
                                    let c0 = match &mut worker_cells[s0] {
                                        Some(sl) => std::mem::replace(&mut sl[local_i], Cell::Null),
                                        None => cols_ref[s0].get(gi),
                                    };
                                    let c1 = match &mut worker_cells[s1] {
                                        Some(sl) => std::mem::replace(&mut sl[local_i], Cell::Null),
                                        None => cols_ref[s1].get(gi),
                                    };
                                    let c2 = match &mut worker_cells[s2] {
                                        Some(sl) => std::mem::replace(&mut sl[local_i], Cell::Null),
                                        None => cols_ref[s2].get(gi),
                                    };
                                    slot_out.write(Row::from_parts(
                                        Arc::clone(shape_ref),
                                        [c0, c1, c2],
                                    ));
                                }
                            } else {
                                for (local_i, slot_out) in out_chunk.iter_mut().enumerate() {
                                    let gi = c_start + local_i;
                                    let row = Row::from_parts(
                                        Arc::clone(shape_ref),
                                        (0..width).map(|pos| {
                                            let ci = inv_slots[pos];
                                            match &mut worker_cells[ci] {
                                                Some(sl) => {
                                                    std::mem::replace(&mut sl[local_i], Cell::Null)
                                                }
                                                None => cols_ref[ci].get(gi),
                                            }
                                        }),
                                    );
                                    slot_out.write(row);
                                }
                            }
                        });
                        c_start += c_len;
                    }
                });
                // SAFETY: `MaybeUninit<Row>` has identical layout to `Row`, and all `n` slots were written.
                return unsafe {
                    let mut md = std::mem::ManuallyDrop::new(out_buf);
                    Vec::from_raw_parts(md.as_mut_ptr() as *mut Row, n, md.capacity())
                };
            }
        }

        if width == 2 && self.names.len() == 2 && self.names[0] != self.names[1] {
            let shape = if self.names.as_ref() == ["id", "title"] {
                Arc::clone(crate::store::shape_id_title())
            } else {
                Row::shape_of(&self.names)
            };
            let swap = self.names[0] > self.names[1];
            let (col0_slice, col1_slice) = self.cols.split_at_mut(1);
            let col0 = &mut col0_slice[0];
            let col1 = &mut col1_slice[0];
            let mut out = if (1024..65_536).contains(&n) {
                crate::store::take_general_rows().unwrap_or_else(|| Vec::with_capacity(n))
            } else {
                Vec::with_capacity(n)
            };
            if out.capacity() < n {
                out.reserve(n - out.len());
            }
            let reuse_len = out.len().min(n);
            for i in 0..reuse_len {
                let c0 = take_col_cell(col0, i);
                let c1 = take_col_cell(col1, i);
                let (a, b) = if swap { (c1, c0) } else { (c0, c1) };
                out[i].set_pair_with_shape(&shape, a, b);
            }
            for i in reuse_len..n {
                let c0 = take_col_cell(col0, i);
                let c1 = take_col_cell(col1, i);
                let (a, b) = if swap { (c1, c0) } else { (c0, c1) };
                out.push(Row::from_parts(Arc::clone(&shape), [a, b]));
            }
            out.truncate(n);
            return out;
        }
        if width == 3
            && self.names.len() == 3
            && self.names[0] != self.names[1]
            && self.names[1] != self.names[2]
            && self.names[0] != self.names[2]
        {
            let shape = Row::shape_of(&self.names);
            let pos_of = |c: usize| {
                shape
                    .names()
                    .iter()
                    .position(|t| t == &self.names[c])
                    .unwrap_or(c)
            };
            let slots = [pos_of(0), pos_of(1), pos_of(2)];
            let (s0, s12) = self.cols.split_at_mut(1);
            let (s1, s2) = s12.split_at_mut(1);
            let c0 = &mut s0[0];
            let c1 = &mut s1[0];
            let c2 = &mut s2[0];
            let mut out = if (1024..65_536).contains(&n) {
                crate::store::take_general_rows().unwrap_or_else(|| Vec::with_capacity(n))
            } else {
                Vec::with_capacity(n)
            };
            if out.capacity() < n {
                out.reserve(n - out.len());
            }
            let order = |v0: Cell, v1: Cell, v2: Cell| -> [Cell; 3] {
                let mut arr = [Cell::Null, Cell::Null, Cell::Null];
                arr[slots[0]] = v0;
                arr[slots[1]] = v1;
                arr[slots[2]] = v2;
                arr
            };
            let reuse_len = out.len().min(n);
            for i in 0..reuse_len {
                let v0 = take_col_cell(c0, i);
                let v1 = take_col_cell(c1, i);
                let v2 = take_col_cell(c2, i);
                out[i].reset_with_shape_ref(&shape, order(v0, v1, v2));
            }
            for i in reuse_len..n {
                let v0 = take_col_cell(c0, i);
                let v1 = take_col_cell(c1, i);
                let v2 = take_col_cell(c2, i);
                out.push(Row::from_parts(Arc::clone(&shape), order(v0, v1, v2)));
            }
            out.truncate(n);
            return out;
        }
        if unique_small_names {
            let shape = Row::shape_of(&self.names);
            let mut inv_slots = [0usize; 8];
            for (pos, target) in shape.names().iter().enumerate() {
                inv_slots[pos] = self
                    .names
                    .iter()
                    .position(|name| name == target)
                    .unwrap_or(pos);
            }
            let cols = self.cols.as_mut_slice();
            let mut out = if (1024..65_536).contains(&n) {
                crate::store::take_general_rows().unwrap_or_else(|| Vec::with_capacity(n))
            } else {
                Vec::with_capacity(n)
            };
            if out.capacity() < n {
                out.reserve(n - out.len());
            }
            let reuse_len = out.len().min(n);
            for (i, row) in out[..reuse_len].iter_mut().enumerate() {
                row.reset_with_shape_ref(
                    &shape,
                    (0..width).map(|pos| take_col_cell(&mut cols[inv_slots[pos]], i)),
                );
            }
            for i in reuse_len..n {
                out.push(Row::from_parts(
                    Arc::clone(&shape),
                    (0..width).map(|pos| take_col_cell(&mut cols[inv_slots[pos]], i)),
                ));
            }
            out.truncate(n);
            return out;
        }
        // Wide (> 8 columns) or duplicate-named fallback: still move `Cells` instead of cloning!
        let template: Row = self
            .names
            .iter()
            .map(|k| (k.as_str(), Cell::Null))
            .collect();
        let slots: Vec<usize> = self
            .names
            .iter()
            .map(|k| template.keys().position(|t| t == k).unwrap_or(0))
            .collect();
        let mut out = Vec::with_capacity(n);
        for i in 0..n {
            let mut row = template.clone();
            let cells = row.cells_mut();
            for (&slot, col) in slots.iter().zip(self.cols.iter_mut()) {
                cells[slot] = take_col_cell(col, i);
            }
            out.push(row);
        }
        out
    }

    /// Materialize a slice of rows with zero-copy Cell cloning - avoids per-row allocations
    pub fn to_rows_slice(&self, start: usize, end: usize) -> Vec<Row> {
        self.to_rows_batch(start, end)
    }

    fn to_rows_batch(&self, start: usize, end: usize) -> Vec<Row> {
        #[inline]
        fn fill_rows_range<F: Fn(usize) -> Row + Sync>(
            start: usize,
            end: usize,
            fill: F,
        ) -> Vec<Row> {
            let n = end - start;
            if n >= 65_536 {
                let threads = std::thread::available_parallelism()
                    .map(|p| p.get())
                    .unwrap_or(1)
                    .min(8)
                    .min(n.div_ceil(8192));
                if threads > 1 {
                    let chunk_size = n.div_ceil(threads);
                    let mut out_buf: Vec<std::mem::MaybeUninit<Row>> = Vec::with_capacity(n);
                    // SAFETY: Every slot in `0..n` is initialized across disjoint `chunks_mut`.
                    unsafe {
                        out_buf.set_len(n);
                    }
                    let fill_ref = &fill;
                    std::thread::scope(|scope| {
                        let mut c_start = start;
                        for out_chunk in out_buf.chunks_mut(chunk_size) {
                            let c_len = out_chunk.len();
                            scope.spawn(move || {
                                for (local_i, slot) in out_chunk.iter_mut().enumerate() {
                                    slot.write(fill_ref(c_start + local_i));
                                }
                            });
                            c_start += c_len;
                        }
                    });
                    return unsafe {
                        let mut md = std::mem::ManuallyDrop::new(out_buf);
                        Vec::from_raw_parts(md.as_mut_ptr() as *mut Row, n, md.capacity())
                    };
                }
            }
            (start..end).map(fill).collect()
        }

        if start >= end || start >= self.n() {
            return Vec::new();
        }
        let end = end.min(self.n());
        if self.names.len() == 2 && self.names[0] != self.names[1] {
            let shape = if self.names.as_ref() == ["id", "title"] {
                Arc::clone(crate::store::shape_id_title())
            } else {
                Row::shape_of(&self.names)
            };
            let swap = self.names[0] > self.names[1];
            return fill_rows_range(start, end, |i| {
                let c0 = self.cols[0].get(i);
                let c1 = self.cols[1].get(i);
                Row::from_parts(Arc::clone(&shape), if swap { [c1, c0] } else { [c0, c1] })
            });
        }
        let width = self.names.len();
        if (1..=8).contains(&width)
            && self.cols.len() == width
            && (0..width).all(|i| (i + 1..width).all(|j| self.names[i] != self.names[j]))
        {
            let shape = Row::shape_of(&self.names);
            let mut inv_slots = [0usize; 8];
            for (pos, target) in shape.names().iter().enumerate() {
                inv_slots[pos] = self
                    .names
                    .iter()
                    .position(|name| name == target)
                    .unwrap_or(pos);
            }
            return fill_rows_range(start, end, |i| {
                Row::from_parts(
                    Arc::clone(&shape),
                    (0..width).map(|pos| self.cols[inv_slots[pos]].get(i)),
                )
            });
        }
        // One template row carries the shape; every output row is a clone of it with its
        // cells overwritten. `slots[c]` is where column `c` lands in field-name order.
        let template: Row = self
            .names
            .iter()
            .map(|k| (k.as_str(), Cell::Null))
            .collect();
        let slots: Vec<usize> = self
            .names
            .iter()
            .map(|k| template.keys().position(|t| t == k).unwrap_or(0))
            .collect();
        // Duplicate column names collapse to one field; the last column wins, as with
        // repeated inserts.
        fill_rows_range(start, end, |i| {
            let mut row = template.clone();
            let cells = row.cells_mut();
            for (&slot, col) in slots.iter().zip(self.cols.iter()) {
                cells[slot] = col.get(i);
            }
            row
        })
    }

    /// Drain and materialize all rows, consuming the batch efficiently without cloning cells
    pub fn drain_to_rows(&mut self) -> Vec<Row> {
        if self.is_empty() {
            return Vec::new();
        }
        let cols = std::mem::replace(
            &mut self.cols,
            (0..self.names.len()).map(|_| Column::new()).collect(),
        );
        RecordBatch {
            names: Arc::clone(&self.names),
            cols,
        }
        .into_rows()
    }

    pub fn truncate(&mut self, n: usize) {
        for col in &mut self.cols {
            if col.len() > n {
                col.truncate(n);
            }
        }
    }

    pub fn drain_prefix(&mut self, n: usize) {
        if n == 0 {
            return;
        }
        for col in &mut self.cols {
            col.drain_prefix(n);
        }
    }

    /// Gather rows at `indices` across every column without materializing `Row` maps.
    #[allow(dead_code)]
    pub(crate) fn filter_indices(&self, indices: &[u32]) -> RecordBatch {
        let cols = self.cols.iter().map(|c| c.gather_u32(indices)).collect();
        RecordBatch {
            names: Arc::clone(&self.names),
            cols,
        }
    }

    /// Owned counterpart of [`Self::filter_indices`] for unique `indices`, moving `Dict`
    /// dictionaries and `Cells` values without extra clones.
    #[allow(dead_code)]
    pub(crate) fn into_filtered_indices(self, indices: &[u32]) -> RecordBatch {
        let cols = self
            .cols
            .into_iter()
            .map(|c| c.gather_unique_owned(indices))
            .collect();
        RecordBatch {
            names: self.names,
            cols,
        }
    }

    /// Evaluate `col <op> lit` directly on columnar buffers, writing matching `0..n` row
    /// indices into `out`. Returns `false` when the column/literal pair requires fallback to
    /// generic row predicate evaluation (`eval_pred`).
    #[allow(dead_code)]
    pub(crate) fn eval_cmp_indices(
        &self,
        col_idx: usize,
        op: crate::ast::CmpOp,
        lit: &Cell,
        out: &mut Vec<u32>,
    ) -> bool {
        let Some(col) = self.cols.get(col_idx) else {
            return false;
        };
        let n = col.len();
        if n > u32::MAX as usize {
            return false;
        }
        if matches!(lit, Cell::Null | Cell::Vec(_)) {
            return false;
        }
        eval_col_cmp_indices(col, op, lit, out)
    }

    /// Select and reorder columns by index in `O(#cols)`.
    #[allow(dead_code)]
    pub(crate) fn project_cols(&self, indices: &[usize], out_names: Arc<[String]>) -> RecordBatch {
        debug_assert_eq!(indices.len(), out_names.len());
        let cols = indices.iter().map(|&idx| self.cols[idx].clone()).collect();
        RecordBatch {
            names: out_names,
            cols,
        }
    }

    /// Owned counterpart of [`Self::project_cols`], moving unique columns in `O(1)` without
    /// cloning column buffers and cloning only duplicated column indices.
    #[allow(dead_code)]
    pub(crate) fn into_projected_cols(
        mut self,
        indices: &[usize],
        out_names: Arc<[String]>,
    ) -> RecordBatch {
        debug_assert_eq!(indices.len(), out_names.len());
        let mut rem: smallvec::SmallVec<[usize; 16]> =
            smallvec::SmallVec::from_elem(0, self.cols.len());
        for &idx in indices {
            rem[idx] += 1;
        }
        let mut cols = Vec::with_capacity(indices.len());
        for &idx in indices {
            rem[idx] -= 1;
            if rem[idx] == 0 {
                cols.push(std::mem::take(&mut self.cols[idx]));
            } else {
                cols.push(self.cols[idx].clone());
            }
        }
        RecordBatch {
            names: out_names,
            cols,
        }
    }

    /// Drop named columns in `O(#cols)` without touching row buffers.
    #[allow(dead_code)]
    pub(crate) fn without_cols(self, drop_names: &[&str]) -> RecordBatch {
        if drop_names.is_empty()
            || !self.names.iter().any(|n| drop_names.contains(&n.as_str()))
        {
            return self;
        }
        let mut kept_names = Vec::with_capacity(self.names.len());
        let mut kept_cols = Vec::with_capacity(self.cols.len());
        for (name, col) in self.names.iter().zip(self.cols) {
            if !drop_names.contains(&name.as_str()) {
                kept_names.push(name.clone());
                kept_cols.push(col);
            }
        }
        RecordBatch {
            names: kept_names.into(),
            cols: kept_cols,
        }
    }

    /// Sort the batch by `cols[col_idx]` (matching [`crate::eval::cmp_sort`] with ascending
    /// row-index tie-breaking) and gather only the `skip` / `take` window.
    #[allow(dead_code)]
    pub(crate) fn sort_by_col(
        &self,
        col_idx: usize,
        desc: bool,
        skip: usize,
        take: Option<usize>,
    ) -> RecordBatch {
        let n = self.n();
        if n == 0 || skip >= n || take == Some(0) || col_idx >= self.cols.len() {
            return self.filter_indices(&[]);
        }
        if n == 1 {
            return self.clone();
        }
        let end = match take {
            Some(t) => skip.saturating_add(t).min(n),
            None => n,
        };
        let keep = sorted_col_indices(&self.cols[col_idx], desc, skip, end);
        self.filter_indices(&keep)
    }

    /// Create a batch from columnar data with pre-allocated capacity
    pub fn from_columns(names: impl Into<Arc<[String]>>, cols: Vec<Vec<Cell>>) -> Self {
        debug_assert!(
            cols.iter()
                .all(|c| c.len() == cols.first().map(|f| f.len()).unwrap_or(0))
        );
        Self {
            names: names.into(),
            cols: cols.into_iter().map(Column::from).collect(),
        }
    }

    /// Reserve space for n rows across all columns (batch append optimization)
    pub fn reserve_cols(&mut self, n: usize) {
        for col in &mut self.cols {
            col.reserve(n.saturating_sub(col.len()));
        }
    }

    /// Append multiple rows efficiently by pre-allocating
    pub fn extend_rows(&mut self, cells_iter: impl Iterator<Item = Vec<Cell>>) {
        for cells in cells_iter {
            self.push_cells(cells.into_iter());
        }
    }

    /// Build a columnar batch directly from row maps.
    pub fn from_rows(rows: &[Row]) -> Self {
        if rows.is_empty() {
            return Self::empty(Arc::<[String]>::from([]));
        }
        let first_shape = rows[0].shape();
        let same_shape = first_shape.is_some()
            && rows.iter().all(|r| match (first_shape, r.shape()) {
                (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                (None, None) => true,
                _ => false,
            });
        if same_shape {
            let names_vec: Vec<String> = rows[0].keys().cloned().collect();
            let mut batch = Self::with_capacity(Arc::<[String]>::from(names_vec), rows.len());
            for (col_idx, col) in batch.cols.iter_mut().enumerate() {
                for row in rows {
                    col.push(row.cells_slice()[col_idx].clone_for_borrow());
                }
            }
            return batch;
        }

        let mut name_set: BTreeSet<&str> = BTreeSet::new();
        for row in rows {
            for k in row.keys() {
                name_set.insert(k.as_str());
            }
        }
        if name_set.is_empty() {
            return Self::empty(Arc::<[String]>::from([]));
        }
        let names_vec: Vec<String> = name_set.into_iter().map(str::to_owned).collect();
        let names_arc: Arc<[String]> = names_vec.into();
        let mut batch = Self::with_capacity(Arc::clone(&names_arc), rows.len());
        for (col, name) in batch.cols.iter_mut().zip(names_arc.iter()) {
            let mut slot = crate::store::row::FieldSlot::new(name.as_str());
            for row in rows {
                let cell = slot
                    .get(row)
                    .map(Cell::clone_for_borrow)
                    .unwrap_or(Cell::Null);
                col.push(cell);
            }
        }
        batch
    }

    /// Encode this batch into the checksummed `LINB\x01` columnar wire format.
    pub fn to_wire_bytes(&self) -> Vec<u8> {
        let row_count = self.n();
        let est = 32
            + self
                .cols
                .iter()
                .map(|c| c.heap_bytes().max(row_count))
                .sum::<usize>();
        let mut out = Vec::with_capacity(est);
        out.extend_from_slice(BATCH_WIRE_MAGIC);
        out.extend_from_slice(&(self.names.len() as u32).to_le_bytes());
        out.extend_from_slice(&(row_count as u64).to_le_bytes());
        for name in self.names.iter() {
            let b = name.as_bytes();
            out.extend_from_slice(&(b.len() as u32).to_le_bytes());
            out.extend_from_slice(b);
        }
        for col in &self.cols {
            debug_assert_eq!(col.len(), row_count);
            write_wire_column(&mut out, col);
        }
        let crc = crate::crc::hardware_crc32c(&out);
        out.extend_from_slice(BATCH_CRC_TAG);
        out.extend_from_slice(&crc.to_le_bytes());
        out
    }

    /// Decode a batch from the checksummed `LINB\x01` columnar wire format.
    pub fn from_wire_bytes(bytes: &[u8]) -> Result<Self, Error> {
        let min_len = BATCH_WIRE_MAGIC.len() + 4 + 8 + BATCH_CRC_TAG.len() + 4;
        if bytes.len() < min_len {
            return Err(Error::runtime("batch: wire payload too short"));
        }
        if &bytes[..BATCH_WIRE_MAGIC.len()] != BATCH_WIRE_MAGIC {
            return Err(Error::runtime("batch: bad wire magic"));
        }
        let payload_len = bytes.len() - 8;
        if &bytes[payload_len..payload_len + 4] != BATCH_CRC_TAG {
            return Err(Error::runtime("batch: missing wire LCRC trailer"));
        }
        let want_crc = u32::from_le_bytes(bytes[payload_len + 4..].try_into().unwrap());
        let got_crc = crate::crc::hardware_crc32c(&bytes[..payload_len]);
        if got_crc != want_crc {
            return Err(Error::runtime(format!(
                "batch: wire CRC32C mismatch (expected {want_crc:#010x}, got {got_crc:#010x})"
            )));
        }

        let mut cur = &bytes[BATCH_WIRE_MAGIC.len()..payload_len];
        let col_count = read_u32_le(&mut cur)? as usize;
        let row_count_u64 = read_u64_le(&mut cur)?;
        let row_count = usize::try_from(row_count_u64)
            .map_err(|_| Error::runtime("batch: row count overflow"))?;
        if col_count.saturating_mul(4) > cur.len() {
            return Err(Error::runtime("batch: truncated column headers"));
        }
        let mut names = Vec::with_capacity(col_count);
        for _ in 0..col_count {
            let nlen = read_u32_le(&mut cur)? as usize;
            let nbytes = read_slice(&mut cur, nlen)?;
            let name = std::str::from_utf8(nbytes)
                .map_err(|e| Error::runtime(format!("batch: invalid utf8 column name: {e}")))?;
            names.push(name.to_owned());
        }
        let mut cols = Vec::with_capacity(col_count);
        for _ in 0..col_count {
            cols.push(read_wire_column(&mut cur, row_count)?);
        }
        if !cur.is_empty() {
            return Err(Error::runtime("batch: trailing bytes in wire payload"));
        }
        Ok(Self {
            names: names.into(),
            cols,
        })
    }

    /// Logical row count (synonym for [`Self::n`]).
    #[inline]
    pub fn len(&self) -> usize {
        self.n()
    }

    /// Slice `[range.start .. range.end]` across all columns, sharing schema `Arc<[String]>`
    /// and `Arc`-backed string/dictionary payloads with exact recomputed null counts (`LIN-0689`).
    pub fn slice(&self, range: std::ops::Range<usize>) -> RecordBatch {
        let cols = self
            .cols
            .iter()
            .map(|c| c.slice_range(range.clone()))
            .collect();
        RecordBatch {
            names: Arc::clone(&self.names),
            cols,
        }
    }

    /// Concatenate batches that share the exact same column schema, promoting `Int`/`Float`
    /// mixtures cleanly to dense `Column::Float` (`LIN-0697`).
    pub fn concat(batches: &[&RecordBatch]) -> Result<RecordBatch, Error> {
        if batches.is_empty() {
            return Ok(RecordBatch::empty(Arc::<[String]>::from([])));
        }
        let first = batches[0];
        first.inspect_consistency()?;
        for (idx, b) in batches.iter().enumerate().skip(1) {
            b.inspect_consistency()?;
            if b.names.as_ref() != first.names.as_ref() {
                return Err(Error::runtime(format!(
                    "batch concat: schema mismatch at batch {idx} (expected {:?}, got {:?})",
                    first.names, b.names
                )));
            }
        }
        let col_count = first.cols.len();
        let mut out_cols = Vec::with_capacity(col_count);
        for c_idx in 0..col_count {
            let refs: Vec<(Option<&Column>, usize)> = batches
                .iter()
                .map(|b| (Some(&b.cols[c_idx]), b.n()))
                .collect();
            out_cols.push(merge_column_segments(&first.names[c_idx], &refs, false)?);
        }
        Ok(RecordBatch {
            names: Arc::clone(&first.names),
            cols: out_cols,
        })
    }

    /// Reconcile and concatenate heterogeneous batches by aligning missing columns with nulls
    /// and promoting compatible `Int`/`Float` columns cleanly (`LIN-0698`).
    pub fn reconcile_schema(batches: &[RecordBatch]) -> Result<RecordBatch, Error> {
        if batches.is_empty() {
            return Ok(RecordBatch::empty(Arc::<[String]>::from([])));
        }
        let mut unified_names: Vec<String> = Vec::new();
        for (b_idx, b) in batches.iter().enumerate() {
            b.inspect_consistency()?;
            let mut seen_in_batch: BTreeSet<&str> = BTreeSet::new();
            for nm in b.names.iter() {
                if !seen_in_batch.insert(nm.as_str()) {
                    return Err(Error::runtime(format!(
                        "batch reconcile_schema: duplicate column `{nm}` in batch {b_idx}"
                    )));
                }
                if !unified_names.iter().any(|u| u == nm) {
                    unified_names.push(nm.clone());
                }
            }
        }
        let mut out_cols = Vec::with_capacity(unified_names.len());
        for col_name in &unified_names {
            let segs: Vec<(Option<&Column>, usize)> = batches
                .iter()
                .map(|b| {
                    let col = b
                        .names
                        .iter()
                        .position(|nm| nm == col_name)
                        .map(|idx| &b.cols[idx]);
                    (col, b.n())
                })
                .collect();
            out_cols.push(merge_column_segments(col_name, &segs, true)?);
        }
        Ok(RecordBatch {
            names: unified_names.into(),
            cols: out_cols,
        })
    }

    /// Per-column memory diagnostics (`LIN-0713`).
    pub fn memory_diagnostics(&self) -> Vec<ColumnMemoryDiagnostics> {
        self.names
            .iter()
            .zip(self.cols.iter())
            .map(|(name, col)| {
                let (kind, payload_bytes, validity_bytes) = match col {
                    Column::Nulls { .. } => ("Nulls", 0, 0),
                    Column::Int { vals, valid } => {
                        ("Int", vals.capacity() * 8, valid.words.capacity() * 8)
                    }
                    Column::Time { vals, valid } => {
                        ("Time", vals.capacity() * 8, valid.words.capacity() * 8)
                    }
                    Column::Float { vals, valid } => {
                        ("Float", vals.capacity() * 8, valid.words.capacity() * 8)
                    }
                    Column::Bool { vals, valid } => {
                        ("Bool", vals.capacity(), valid.words.capacity() * 8)
                    }
                    Column::Text {
                        offsets,
                        data,
                        valid,
                    } => (
                        "Text",
                        offsets.capacity() * 4 + data.capacity(),
                        valid.words.capacity() * 8,
                    ),
                    Column::Dict {
                        values,
                        codes,
                        valid,
                    } => (
                        "Dict",
                        codes.capacity() * 4
                            + values.capacity() * std::mem::size_of::<Arc<str>>()
                            + values.iter().map(|s| s.len()).sum::<usize>(),
                        valid.words.capacity() * 8,
                    ),
                    Column::Cells(c) => {
                        let extra: usize = c
                            .iter()
                            .map(|cell| match cell {
                                Cell::Text(s) => s.len(),
                                Cell::Vec(v) => v.len() * 4,
                                _ => 0,
                            })
                            .sum();
                        ("Mixed", c.capacity() * std::mem::size_of::<Cell>() + extra, 0)
                    }
                };
                ColumnMemoryDiagnostics {
                    name: name.clone(),
                    kind,
                    len: col.len(),
                    null_count: col.null_count(),
                    payload_bytes,
                    validity_bytes,
                }
            })
            .collect()
    }

    /// Estimated total memory footprint in bytes (`LIN-0700`).
    #[inline]
    pub fn estimated_bytes(&self) -> usize {
        self.byte_size()
    }

    /// Enforce row and byte allocation budgets (`LIN-0700`).
    pub fn check_budget(&self, budget: &BatchAllocationBudget) -> Result<(), Error> {
        let rows = self.n();
        if rows > budget.max_rows {
            return Err(Error::resource_limit(format!(
                "batch row count {rows} exceeds max_rows budget {}",
                budget.max_rows
            )));
        }
        let bytes = self.estimated_bytes();
        if bytes > budget.max_bytes {
            return Err(Error::resource_limit(format!(
                "batch estimated_bytes {bytes} exceeds max_bytes budget {}",
                budget.max_bytes
            )));
        }
        Ok(())
    }

    /// Verify internal structural invariants across schema, column lengths, validity masks,
    /// text offsets, UTF-8 buffers, and dictionary codes (`LIN-0714`..`LIN-0716`).
    pub fn inspect_consistency(&self) -> Result<(), Error> {
        if self.names.len() != self.cols.len() {
            return Err(Error::corrupt(format!(
                "batch consistency: names.len() ({}) != cols.len() ({})",
                self.names.len(),
                self.cols.len()
            )));
        }
        let expected_len = self.n();
        for (idx, (name, col)) in self.names.iter().zip(self.cols.iter()).enumerate() {
            let clen = col.len();
            if clen != expected_len {
                return Err(Error::corrupt(format!(
                    "batch consistency: column {idx} (`{name}`) len {clen} != batch len {expected_len}"
                )));
            }
            match col {
                Column::Nulls { .. } | Column::Cells(_) => {}
                Column::Int { vals, valid } | Column::Time { vals, valid } => {
                    inspect_validity_invariants(name, vals.len(), valid)?;
                }
                Column::Float { vals, valid } => {
                    inspect_validity_invariants(name, vals.len(), valid)?;
                }
                Column::Bool { vals, valid } => {
                    inspect_validity_invariants(name, vals.len(), valid)?;
                }
                Column::Text {
                    offsets,
                    data,
                    valid,
                } => {
                    inspect_validity_invariants(name, clen, valid)?;
                    if offsets.len() != clen + 1 {
                        return Err(Error::corrupt(format!(
                            "batch consistency: text column `{name}` offsets.len() {} != {}",
                            offsets.len(),
                            clen + 1
                        )));
                    }
                    if offsets.first().copied() != Some(0) {
                        return Err(Error::corrupt(format!(
                            "batch consistency: text column `{name}` offsets[0] != 0"
                        )));
                    }
                    if offsets.last().copied().unwrap_or(0) as usize != data.len() {
                        return Err(Error::corrupt(format!(
                            "batch consistency: text column `{name}` last offset != data.len()"
                        )));
                    }
                    for w in offsets.windows(2) {
                        if w[0] > w[1] {
                            return Err(Error::corrupt(format!(
                                "batch consistency: text column `{name}` non-monotonic offsets {} > {}",
                                w[0], w[1]
                            )));
                        }
                        if std::str::from_utf8(&data[w[0] as usize..w[1] as usize]).is_err() {
                            return Err(Error::corrupt(format!(
                                "batch consistency: text column `{name}` invalid UTF-8 slice"
                            )));
                        }
                    }
                }
                Column::Dict {
                    values,
                    codes,
                    valid,
                } => {
                    inspect_validity_invariants(name, codes.len(), valid)?;
                    for (row_i, &code) in codes.iter().enumerate() {
                        if valid.is_valid(row_i) && (code as usize) >= values.len() {
                            return Err(Error::corrupt(format!(
                                "batch consistency: dict column `{name}` row {row_i} code {code} >= dict len {}",
                                values.len()
                            )));
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// Describe this batch's Arrow C Data Interface struct schema and ownership/release contract
    /// without invoking `unsafe` FFI (`LIN-0709`..`LIN-0712`).
    pub fn describe_arrow_schema(&self) -> ArrowSchemaDescriptor {
        let children = self
            .names
            .iter()
            .zip(self.cols.iter())
            .map(|(name, col)| column_arrow_descriptor(name, col))
            .collect();
        ArrowSchemaDescriptor {
            name: String::new(),
            format: "+s",
            nullable: false,
            dictionary_ordered: false,
            children,
            ownership_contract: ARROW_OWNERSHIP_CONTRACT,
        }
    }

    /// Produce safe per-column Arrow C Data Interface export metadata (`LIN-0710`..`LIN-0712`).
    pub fn export_arrow_descriptors(&self) -> Vec<ArrowColumnExport> {
        self.names
            .iter()
            .zip(self.cols.iter())
            .map(|(name, col)| {
                let schema = column_arrow_descriptor(name, col);
                let len = col.len();
                let null_count = col.null_count();
                let valid_bytes = if null_count == 0 { 0 } else { len.div_ceil(8) };
                let buffer_lengths_bytes = match col {
                    Column::Nulls { .. } => Vec::new(),
                    Column::Int { .. } | Column::Time { .. } | Column::Float { .. } => {
                        vec![valid_bytes, len * 8]
                    }
                    Column::Bool { .. } => vec![valid_bytes, len.div_ceil(8)],
                    Column::Text { offsets, data, .. } => {
                        vec![valid_bytes, offsets.len() * 4, data.len()]
                    }
                    Column::Dict { codes, .. } => vec![valid_bytes, codes.len() * 4],
                    Column::Cells(cells) => vec![cells.len() * std::mem::size_of::<Cell>()],
                };
                ArrowColumnExport {
                    schema,
                    length: len,
                    null_count,
                    offset: 0,
                    buffer_lengths_bytes,
                    release_required: len > 0,
                }
            })
            .collect()
    }
}

const ARROW_OWNERSHIP_CONTRACT: &str = "Producer pins backing Vec/Arc buffers until consumer invokes ArrowArray.release exactly once; consumer must set release=NULL after call and never mutate exported buffers.";

fn column_arrow_descriptor(name: &str, col: &Column) -> ArrowSchemaDescriptor {
    let (format, dictionary_ordered) = match col {
        Column::Nulls { .. } => ("n", false),
        Column::Int { .. } => ("l", false),
        Column::Float { .. } => ("g", false),
        Column::Bool { .. } => ("b", false),
        Column::Time { .. } => ("tsm:UTC", false),
        Column::Text { .. } => ("u", false),
        Column::Dict { .. } => ("I", false),
        Column::Cells(_) => ("+ud:0,1,2,3,4,5,6", false),
    };
    ArrowSchemaDescriptor {
        name: name.to_owned(),
        format,
        nullable: matches!(col, Column::Nulls { .. }) || col.null_count() > 0,
        dictionary_ordered,
        children: Vec::new(),
        ownership_contract: ARROW_OWNERSHIP_CONTRACT,
    }
}

fn inspect_validity_invariants(name: &str, col_len: usize, valid: &Validity) -> Result<(), Error> {
    if valid.len != col_len {
        return Err(Error::corrupt(format!(
            "batch consistency: column `{name}` validity len {} != col len {col_len}",
            valid.len
        )));
    }
    if valid.nulls > valid.len {
        return Err(Error::corrupt(format!(
            "batch consistency: column `{name}` validity nulls {} > len {}",
            valid.nulls, valid.len
        )));
    }
    let expected_words = if valid.nulls == 0 {
        0
    } else {
        valid.len.div_ceil(64)
    };
    if valid.words.len() != expected_words {
        return Err(Error::corrupt(format!(
            "batch consistency: column `{name}` validity words.len() {} != expected {expected_words}",
            valid.words.len()
        )));
    }
    if valid.nulls > 0 {
        let tail = valid.len % 64;
        if tail != 0 {
            let last = *valid.words.last().unwrap();
            let high_mask = !((1u64 << tail) - 1);
            if (last & high_mask) != 0 {
                return Err(Error::corrupt(format!(
                    "batch consistency: column `{name}` validity has set trailing bits beyond len"
                )));
            }
        }
        let ones: usize = valid.words.iter().map(|w| w.count_ones() as usize).sum();
        let actual_nulls = valid.len - ones;
        if actual_nulls != valid.nulls {
            return Err(Error::corrupt(format!(
                "batch consistency: column `{name}` validity null_count {} != actual {actual_nulls}",
                valid.nulls
            )));
        }
    }
    Ok(())
}

fn merge_column_segments(
    col_name: &str,
    segments: &[(Option<&Column>, usize)],
    strict_reconcile: bool,
) -> Result<Column, Error> {
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Kind {
        Nulls,
        Int,
        Float,
        Bool,
        Time,
        Text,
        Cells,
    }
    let classify = |c: &Column| -> Kind {
        match c {
            Column::Nulls { .. } => Kind::Nulls,
            Column::Int { .. } => Kind::Int,
            Column::Float { .. } => Kind::Float,
            Column::Bool { .. } => Kind::Bool,
            Column::Time { .. } => Kind::Time,
            Column::Text { .. } | Column::Dict { .. } => Kind::Text,
            Column::Cells(_) => Kind::Cells,
        }
    };

    let total_rows: usize = segments.iter().map(|(_, n)| *n).sum();
    let mut target = Kind::Nulls;
    for &(col_opt, _) in segments {
        let Some(col) = col_opt else {
            continue;
        };
        let k = classify(col);
        if k == Kind::Nulls {
            continue;
        }
        target = match (target, k) {
            (Kind::Nulls, other) => other,
            (a, b) if a == b => a,
            (Kind::Int, Kind::Float) | (Kind::Float, Kind::Int) => Kind::Float,
            (a, b) => {
                if strict_reconcile && a != Kind::Cells && b != Kind::Cells {
                    return Err(Error::runtime(format!(
                        "batch reconcile_schema: incompatible column types on `{col_name}`"
                    ))
                    .with_code(crate::error::ErrorCode::CheckType));
                }
                Kind::Cells
            }
        };
    }

    match target {
        Kind::Nulls => Ok(Column::Nulls {
            n: total_rows,
            cap: total_rows,
        }),
        Kind::Float => {
            let mut vals = Vec::with_capacity(total_rows);
            let mut valid = Validity::default();
            for &(col_opt, seg_len) in segments {
                match col_opt {
                    None | Some(Column::Nulls { .. }) => {
                        vals.resize(vals.len() + seg_len, 0.0);
                        valid.extend(&Validity::all_null(seg_len));
                    }
                    Some(Column::Float { vals: fv, valid: v }) => {
                        vals.extend_from_slice(fv);
                        valid.extend(v);
                    }
                    Some(Column::Int { vals: iv, valid: v }) => {
                        vals.extend(iv.iter().map(|&x| x as f64));
                        valid.extend(v);
                    }
                    _ => unreachable!(),
                }
            }
            Ok(Column::Float { vals, valid })
        }
        Kind::Text => {
            let all_dict = segments.iter().all(|&(c, _)| {
                matches!(c, None | Some(Column::Nulls { .. }) | Some(Column::Dict { .. }))
            });
            if all_dict {
                let mut out = Column::with_capacity(total_rows);
                for &(col_opt, seg_len) in segments {
                    match col_opt {
                        None => out.extend(Column::Nulls {
                            n: seg_len,
                            cap: 0,
                        }),
                        Some(col) => out.extend(col.clone()),
                    }
                }
                return Ok(out);
            }
            let mut out = Column::new_text_with_capacity(total_rows, total_rows * 8);
            for &(col_opt, seg_len) in segments {
                match col_opt {
                    None | Some(Column::Nulls { .. }) => {
                        for _ in 0..seg_len {
                            out.push(Cell::Null);
                        }
                    }
                    Some(col @ Column::Text { .. }) => out.extend(col.clone()),
                    Some(col @ Column::Dict { .. }) => {
                        for i in 0..col.len() {
                            match col.text_at(i) {
                                Some(s) => out.push(Cell::from_text(s)),
                                None => out.push(Cell::Null),
                            }
                        }
                    }
                    _ => unreachable!(),
                }
            }
            Ok(out)
        }
        _ => {
            let mut out = Column::with_capacity(total_rows);
            for &(col_opt, seg_len) in segments {
                match col_opt {
                    None => out.extend(Column::Nulls {
                        n: seg_len,
                        cap: 0,
                    }),
                    Some(col) => out.extend(col.clone()),
                }
            }
            Ok(out)
        }
    }
}

#[inline]
pub(crate) fn read_u8(cur: &mut &[u8]) -> Result<u8, Error> {
    let (&b, rest) = cur
        .split_first()
        .ok_or_else(|| Error::runtime("batch: unexpected end of wire stream"))?;
    *cur = rest;
    Ok(b)
}

#[inline]
pub(crate) fn read_u32_le(cur: &mut &[u8]) -> Result<u32, Error> {
    let s = read_slice(cur, 4)?;
    Ok(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

#[inline]
pub(crate) fn read_u64_le(cur: &mut &[u8]) -> Result<u64, Error> {
    let s = read_slice(cur, 8)?;
    Ok(u64::from_le_bytes([
        s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7],
    ]))
}

#[inline]
pub(crate) fn read_i64_le(cur: &mut &[u8]) -> Result<i64, Error> {
    Ok(read_u64_le(cur)? as i64)
}

#[inline]
pub(crate) fn read_slice<'a>(cur: &mut &'a [u8], len: usize) -> Result<&'a [u8], Error> {
    if len > cur.len() {
        return Err(Error::runtime("batch: truncated wire frame"));
    }
    let (head, tail) = cur.split_at(len);
    *cur = tail;
    Ok(head)
}

pub(crate) fn write_wire_cell(out: &mut Vec<u8>, cell: &Cell) {
    match cell {
        Cell::Null => out.push(0),
        Cell::Int(i) => {
            out.push(1);
            out.extend_from_slice(&i.to_le_bytes());
        }
        Cell::Float(f) => {
            out.push(2);
            out.extend_from_slice(&f.to_bits().to_le_bytes());
        }
        Cell::Bool(b) => {
            out.push(3);
            out.push(u8::from(*b));
        }
        Cell::Time(t) => {
            out.push(4);
            out.extend_from_slice(&t.to_le_bytes());
        }
        Cell::Text(s) => {
            let b = s.as_bytes();
            out.push(5);
            out.extend_from_slice(&(b.len() as u32).to_le_bytes());
            out.extend_from_slice(b);
        }
        Cell::SmallText(s) => {
            let b = s.as_str().as_bytes();
            out.push(5);
            out.extend_from_slice(&(b.len() as u32).to_le_bytes());
            out.extend_from_slice(b);
        }
        Cell::Vec(v) => {
            out.push(6);
            out.extend_from_slice(&(v.len() as u32).to_le_bytes());
            for &x in v.iter() {
                out.extend_from_slice(&x.to_bits().to_le_bytes());
            }
        }
    }
}

pub(crate) fn read_wire_cell(cur: &mut &[u8]) -> Result<Cell, Error> {
    let tag = read_u8(cur)?;
    match tag {
        0 => Ok(Cell::Null),
        1 => Ok(Cell::Int(read_i64_le(cur)?)),
        2 => Ok(Cell::Float(f64::from_bits(read_u64_le(cur)?))),
        3 => match read_u8(cur)? {
            0 => Ok(Cell::Bool(false)),
            1 => Ok(Cell::Bool(true)),
            other => Err(Error::runtime(format!("batch: invalid bool byte {other}"))),
        },
        4 => Ok(Cell::Time(read_i64_le(cur)?)),
        5 => {
            let len = read_u32_le(cur)? as usize;
            let bytes = read_slice(cur, len)?;
            let s = std::str::from_utf8(bytes)
                .map_err(|e| Error::runtime(format!("batch: invalid utf8 cell text: {e}")))?;
            Ok(Cell::from_text(s))
        }
        6 => {
            let dim = read_u32_le(cur)? as usize;
            let byte_len = dim
                .checked_mul(4)
                .ok_or_else(|| Error::runtime("batch: vec dim overflow"))?;
            let bytes = read_slice(cur, byte_len)?;
            let mut vec = Vec::with_capacity(dim);
            for chunk in bytes.chunks_exact(4) {
                vec.push(f32::from_bits(u32::from_le_bytes([
                    chunk[0], chunk[1], chunk[2], chunk[3],
                ])));
            }
            Ok(Cell::Vec(vec.into()))
        }
        other => Err(Error::runtime(format!("batch: unknown cell tag {other}"))),
    }
}

fn write_validity(out: &mut Vec<u8>, valid: &Validity) {
    let (words, _, nulls) = valid.raw_parts();
    out.extend_from_slice(&(nulls as u64).to_le_bytes());
    out.extend_from_slice(&(words.len() as u32).to_le_bytes());
    for &w in words {
        out.extend_from_slice(&w.to_le_bytes());
    }
}

fn read_validity(cur: &mut &[u8], row_count: usize) -> Result<Validity, Error> {
    let nulls_u64 = read_u64_le(cur)?;
    let nulls = usize::try_from(nulls_u64)
        .map_err(|_| Error::runtime("batch: validity null count overflow"))?;
    let word_count = read_u32_le(cur)? as usize;
    let byte_len = word_count
        .checked_mul(8)
        .ok_or_else(|| Error::runtime("batch: validity byte overflow"))?;
    let raw = read_slice(cur, byte_len)?;
    let mut words = Vec::with_capacity(word_count);
    for chunk in raw.chunks_exact(8) {
        words.push(u64::from_le_bytes([
            chunk[0], chunk[1], chunk[2], chunk[3], chunk[4], chunk[5], chunk[6], chunk[7],
        ]));
    }
    Validity::from_raw_parts(words, row_count, nulls)
}

fn write_wire_column(out: &mut Vec<u8>, col: &Column) {
    match col {
        Column::Nulls { .. } => out.push(0),
        Column::Int { vals, valid } => {
            out.push(1);
            write_validity(out, valid);
            for &v in vals {
                out.extend_from_slice(&v.to_le_bytes());
            }
        }
        Column::Time { vals, valid } => {
            out.push(2);
            write_validity(out, valid);
            for &v in vals {
                out.extend_from_slice(&v.to_le_bytes());
            }
        }
        Column::Float { vals, valid } => {
            out.push(3);
            write_validity(out, valid);
            for &v in vals {
                out.extend_from_slice(&v.to_bits().to_le_bytes());
            }
        }
        Column::Bool { vals, valid } => {
            out.push(4);
            write_validity(out, valid);
            for &b in vals {
                out.push(u8::from(b));
            }
        }
        Column::Cells(cells) => {
            out.push(5);
            for c in cells {
                write_wire_cell(out, c);
            }
        }
        Column::Text {
            offsets,
            data,
            valid,
        } => {
            out.push(6);
            write_validity(out, valid);
            for &off in offsets {
                out.extend_from_slice(&off.to_le_bytes());
            }
            out.extend_from_slice(&(data.len() as u32).to_le_bytes());
            out.extend_from_slice(data);
        }
        Column::Dict {
            values,
            codes,
            valid,
        } => {
            out.push(7);
            write_validity(out, valid);
            out.extend_from_slice(&(values.len() as u32).to_le_bytes());
            for s in values {
                let b = s.as_bytes();
                out.extend_from_slice(&(b.len() as u32).to_le_bytes());
                out.extend_from_slice(b);
            }
            for &c in codes {
                out.extend_from_slice(&c.to_le_bytes());
            }
        }
    }
}

fn read_wire_column(cur: &mut &[u8], row_count: usize) -> Result<Column, Error> {
    let tag = read_u8(cur)?;
    match tag {
        0 => Ok(Column::Nulls {
            n: row_count,
            cap: row_count,
        }),
        1 | 2 => {
            let valid = read_validity(cur, row_count)?;
            let byte_len = row_count
                .checked_mul(8)
                .ok_or_else(|| Error::runtime("batch: i64 column overflow"))?;
            let raw = read_slice(cur, byte_len)?;
            let mut vals = Vec::with_capacity(row_count);
            for chunk in raw.chunks_exact(8) {
                vals.push(i64::from_le_bytes([
                    chunk[0], chunk[1], chunk[2], chunk[3], chunk[4], chunk[5], chunk[6], chunk[7],
                ]));
            }
            if tag == 1 {
                Ok(Column::Int { vals, valid })
            } else {
                Ok(Column::Time { vals, valid })
            }
        }
        3 => {
            let valid = read_validity(cur, row_count)?;
            let byte_len = row_count
                .checked_mul(8)
                .ok_or_else(|| Error::runtime("batch: f64 column overflow"))?;
            let raw = read_slice(cur, byte_len)?;
            let mut vals = Vec::with_capacity(row_count);
            for chunk in raw.chunks_exact(8) {
                vals.push(f64::from_bits(u64::from_le_bytes([
                    chunk[0], chunk[1], chunk[2], chunk[3], chunk[4], chunk[5], chunk[6], chunk[7],
                ])));
            }
            Ok(Column::Float { vals, valid })
        }
        4 => {
            let valid = read_validity(cur, row_count)?;
            let raw = read_slice(cur, row_count)?;
            let mut vals = Vec::with_capacity(row_count);
            for &b in raw {
                match b {
                    0 => vals.push(false),
                    1 => vals.push(true),
                    other => {
                        return Err(Error::runtime(format!("batch: invalid bool byte {other}")));
                    }
                }
            }
            Ok(Column::Bool { vals, valid })
        }
        5 => {
            if row_count > cur.len() {
                return Err(Error::runtime("batch: truncated cell column"));
            }
            let mut cells = Vec::with_capacity(row_count);
            for _ in 0..row_count {
                cells.push(read_wire_cell(cur)?);
            }
            Ok(Column::Cells(cells))
        }
        6 => {
            let valid = read_validity(cur, row_count)?;
            let off_count = row_count
                .checked_add(1)
                .ok_or_else(|| Error::runtime("batch: text offset count overflow"))?;
            let off_bytes = off_count
                .checked_mul(4)
                .ok_or_else(|| Error::runtime("batch: text offsets byte overflow"))?;
            let raw_offs = read_slice(cur, off_bytes)?;
            let mut offsets = Vec::with_capacity(off_count);
            for chunk in raw_offs.chunks_exact(4) {
                offsets.push(u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
            }
            let data_len = read_u32_le(cur)? as usize;
            let raw_data = read_slice(cur, data_len)?;
            if offsets.first() != Some(&0) || offsets.last().copied().unwrap_or(0) as usize != data_len {
                return Err(Error::runtime("batch: invalid text offsets bounds"));
            }
            for w in offsets.windows(2) {
                if w[0] > w[1]
                    || std::str::from_utf8(&raw_data[w[0] as usize..w[1] as usize]).is_err()
                {
                    return Err(Error::runtime("batch: invalid utf8 or monotonic text offset"));
                }
            }
            Ok(Column::Text {
                offsets,
                data: raw_data.to_vec(),
                valid,
            })
        }
        7 => {
            let valid = read_validity(cur, row_count)?;
            let dict_len = read_u32_le(cur)? as usize;
            if dict_len.saturating_mul(4) > cur.len() {
                return Err(Error::runtime("batch: truncated dict entries"));
            }
            let mut values = Vec::with_capacity(dict_len);
            for _ in 0..dict_len {
                let slen = read_u32_le(cur)? as usize;
                let sbytes = read_slice(cur, slen)?;
                let s = std::str::from_utf8(sbytes)
                    .map_err(|e| Error::runtime(format!("batch: invalid utf8 dict entry: {e}")))?;
                values.push(Arc::from(s));
            }
            let code_bytes = row_count
                .checked_mul(4)
                .ok_or_else(|| Error::runtime("batch: dict codes byte overflow"))?;
            let raw_codes = read_slice(cur, code_bytes)?;
            let mut codes = Vec::with_capacity(row_count);
            for (i, chunk) in raw_codes.chunks_exact(4).enumerate() {
                let c = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
                if valid.is_valid(i) && (c as usize) >= dict_len {
                    return Err(Error::runtime("batch: dict code out of bounds"));
                }
                codes.push(c);
            }
            Ok(Column::Dict {
                values,
                codes,
                valid,
            })
        }
        other => Err(Error::runtime(format!("batch: unknown column tag {other}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cells(col: &Column) -> Vec<Cell> {
        col.iter().collect()
    }

    #[test]
    fn typed_column_round_trips_values_and_nulls() {
        let input = vec![
            Cell::Null,
            Cell::Null,
            Cell::Int(7),
            Cell::Null,
            Cell::Int(-3),
        ];
        let col = Column::from(input.clone());
        assert!(matches!(col, Column::Int { .. }), "leading nulls keep the column dense");
        assert_eq!(col.len(), 5);
        assert_eq!(cells(&col), input);
        assert!(col.is_null(0) && col.is_null(3) && !col.is_null(2));
        assert_eq!(col.sum_f64(), Some(4.0));
        let (vals, valid) = col.as_i64().unwrap();
        assert_eq!(vals.len(), 5);
        assert_eq!(valid.null_count(), 3);

        for (cell, typed) in [
            (Cell::Float(1.5), true),
            (Cell::Bool(true), true),
            (Cell::Time(42), true),
            (Cell::from_text("x"), false),
        ] {
            let col = Column::from(vec![Cell::Null, cell.clone(), Cell::Null]);
            assert_eq!(col.is_typed(), typed, "{cell:?}");
            assert_eq!(cells(&col), vec![Cell::Null, cell, Cell::Null]);
        }
        assert_eq!(Column::from(vec![Cell::Null; 3]).len(), 3);
        assert_eq!(cells(&Column::from(vec![Cell::Null; 3])), vec![Cell::Null; 3]);
    }

    #[test]
    fn lazy_null_mask_is_correct_at_word_boundaries() {
        // The mask is only built on the first null; check every position around 64-bit
        // word edges, including a null that is the very first value.
        for first_null in [0usize, 1, 63, 64, 65, 127, 128, 129, 200] {
            let input: Vec<Cell> = (0..260)
                .map(|i| {
                    if i == first_null || (i > first_null && i % 5 == 0) {
                        Cell::Null
                    } else {
                        Cell::Int(i as i64)
                    }
                })
                .collect();
            let mut col = Column::new();
            for c in &input {
                col.push(c.clone());
            }
            assert!(col.is_typed(), "first_null={first_null}");
            assert_eq!(cells(&col), input, "first_null={first_null}");
            let nulls = input.iter().filter(|c| matches!(c, Cell::Null)).count();
            assert_eq!(col.as_i64().unwrap().1.null_count(), nulls);
            let mut cut = col.clone();
            cut.drain_prefix(first_null.min(259));
            assert_eq!(cells(&cut), input[first_null.min(259)..]);
        }
        // No nulls at all: values still read back after truncation and draining.
        let mut col: Column = (0..300).map(|i| Cell::Float(i as f64)).collect();
        assert_eq!(col.as_f64().unwrap().1.null_count(), 0);
        col.truncate(130);
        col.drain_prefix(64);
        assert_eq!(cells(&col), (64..130).map(|i| Cell::Float(i as f64)).collect::<Vec<_>>());
        col.push(Cell::Null);
        assert!(col.is_null(66) && !col.is_null(65));
    }

    #[test]
    fn mixed_types_fall_back_to_cells_without_losing_values() {
        let input = vec![
            Cell::Int(1),
            Cell::Null,
            Cell::Float(2.5),
            Cell::from_text("three"),
            Cell::Int(4),
        ];
        let col = Column::from(input.clone());
        assert!(matches!(col, Column::Cells(_)));
        assert_eq!(cells(&col), input);
        // Time and Int are distinct types and must not be merged into one buffer.
        let col = Column::from(vec![Cell::Int(1), Cell::Time(1)]);
        assert_eq!(cells(&col), vec![Cell::Int(1), Cell::Time(1)]);
    }

    #[test]
    fn truncate_drain_and_extend_keep_null_mask_aligned() {
        let input: Vec<Cell> = (0..200)
            .map(|i| if i % 3 == 0 { Cell::Null } else { Cell::Float(i as f64) })
            .collect();
        let mut col = Column::from(input.clone());
        col.drain_prefix(70);
        assert_eq!(cells(&col), input[70..]);
        col.truncate(65);
        assert_eq!(cells(&col), input[70..135]);
        col.extend(Column::from(input[..10].to_vec()));
        let mut expect = input[70..135].to_vec();
        expect.extend_from_slice(&input[..10]);
        assert_eq!(cells(&col), expect);
        assert!(col.is_typed());
        // Extending with another representation still preserves every value.
        col.extend(Column::from(vec![Cell::from_text("t"), Cell::Null]));
        expect.push(Cell::from_text("t"));
        expect.push(Cell::Null);
        assert_eq!(cells(&col), expect);
        col.drain_prefix(1_000);
        assert!(col.is_empty());

        // Equality is by value, not by representation.
        assert_eq!(
            Column::from(vec![Cell::Int(1), Cell::Null]),
            Column::Cells(vec![Cell::Int(1), Cell::Null])
        );
        assert_ne!(
            Column::from(vec![Cell::Int(1)]),
            Column::from(vec![Cell::Int(2)])
        );
    }

    #[test]
    fn record_batch_rows_are_unchanged_by_typed_storage() {
        let names: Arc<[String]> = vec!["id".to_string(), "qty".to_string(), "price".to_string()].into();
        let mut batch = RecordBatch::with_capacity(Arc::clone(&names), 4);
        let rows = [
            [Cell::from_text("a"), Cell::Int(1), Cell::Float(0.5)],
            [Cell::from_text("b"), Cell::Null, Cell::Float(1.5)],
            [Cell::from_text("c"), Cell::Int(3), Cell::Null],
        ];
        for r in &rows {
            batch.push_row(r);
        }
        assert_eq!(batch.n(), 3);
        assert!(batch.cols[1].is_typed() && batch.cols[2].is_typed());
        assert!(!batch.cols[0].is_typed());
        let out = batch.to_rows();
        for (row, cells) in out.iter().zip(rows.iter()) {
            for (name, cell) in names.iter().zip(cells.iter()) {
                assert_eq!(row.get(name).unwrap_or(&Cell::Null), cell);
            }
        }
        assert_eq!(batch.clone().drain_to_rows(), out);
        let from_cols = RecordBatch::from_columns(
            Arc::clone(&names),
            (0..3).map(|c| rows.iter().map(|r| r[c].clone()).collect()).collect(),
        );
        assert_eq!(from_cols, batch);
        batch.drain_prefix(1);
        batch.truncate(1);
        assert_eq!(batch.to_rows(), out[1..2]);
    }

    #[test]
    fn wire_codec_round_trips_all_column_types_and_rejects_corruption() {
        let mut r0 = Row::new();
        r0.insert("id", Cell::from_text("r0"));
        r0.insert("qty", Cell::Int(42));
        r0.insert("ratio", Cell::Float(1.25));
        r0.insert("ok", Cell::Bool(true));
        r0.insert("ts", Cell::Time(1_700_000_000_000));
        r0.insert("vec", Cell::Vec(Arc::from([0.25f32, 0.75f32])));

        let mut r1 = Row::new();
        r1.insert("id", Cell::from_text("a_much_longer_heap_allocated_string_value"));
        r1.insert("qty", Cell::Null);
        r1.insert("ok", Cell::Bool(false));
        r1.insert("only_null", Cell::Null);

        let batch = RecordBatch::from_rows(&[r0, r1]);
        assert_eq!(batch.n(), 2);
        let wire = batch.to_wire_bytes();
        let decoded = RecordBatch::from_wire_bytes(&wire).unwrap();
        assert_eq!(decoded, batch);
        assert_eq!(decoded.to_rows(), batch.to_rows());

        // Empty batch round-trips cleanly.
        let empty = RecordBatch::from_rows(&[]);
        assert_eq!(empty.n(), 0);
        let empty_wire = empty.to_wire_bytes();
        assert_eq!(RecordBatch::from_wire_bytes(&empty_wire).unwrap(), empty);

        // Single bit flip in payload is rejected by CRC32C.
        let mut bad = wire.clone();
        bad[10] ^= 0x01;
        assert!(RecordBatch::from_wire_bytes(&bad).is_err());

        // Truncated wire payload is rejected cleanly.
        assert!(RecordBatch::from_wire_bytes(&wire[..wire.len() - 1]).is_err());
    }

    #[test]
    fn vectorized_filter_cmp_project_without_and_sort_match_row_semantics() {
        use crate::ast::{CmpOp, Field, Pred, Value};

        let names: Arc<[String]> = vec![
            "id".to_string(),
            "ival".to_string(),
            "fval".to_string(),
            "bval".to_string(),
            "tval".to_string(),
            "txt".to_string(),
            "dict".to_string(),
            "mix".to_string(),
        ]
        .into();

        let n = 150usize;
        let mut c_id = Column::new_text_with_capacity(n, n * 12);
        let mut c_int = Column::with_capacity(n);
        let mut c_flt = Column::with_capacity(n);
        let mut c_bool = Column::with_capacity(n);
        let mut c_time = Column::with_capacity(n);
        let mut c_txt = Column::new_text_with_capacity(n, n * 16);
        let mut raw_dict = Column::with_capacity(n);
        let mut c_mix = Column::Cells(Vec::with_capacity(n));

        for i in 0..n {
            c_id.push(Cell::from_text(format!("row-{i:04}")));
            c_int.push(if i % 11 == 0 {
                Cell::Null
            } else if i == 3 {
                Cell::Int((1i64 << 54) + 1)
            } else {
                Cell::Int(((i * 37) % 29) as i64 - 14)
            });
            c_flt.push(match i % 13 {
                0 => Cell::Null,
                1 => Cell::Float(f64::NAN),
                2 => Cell::Float(-0.0),
                3 => Cell::Float(0.0),
                4 => Cell::Float(f64::INFINITY),
                5 => Cell::Float(f64::NEG_INFINITY),
                _ => Cell::Float(((i * 19) % 23) as f64 - 10.5),
            });
            c_bool.push(match i % 5 {
                0 => Cell::Null,
                1 | 2 => Cell::Bool(false),
                _ => Cell::Bool(true),
            });
            c_time.push(if i % 17 == 0 {
                Cell::Null
            } else {
                Cell::Time(1_700_000_000_000 + ((i * 7) % 19) as i64)
            });
            c_txt.push(if i % 7 == 0 {
                Cell::Null
            } else {
                Cell::from_text(format!("prefix_shared_8bytes_{}", i % 9))
            });
            raw_dict.push(if i % 9 == 0 {
                Cell::Null
            } else {
                Cell::from_text(["rag", "ops", "core", "edge"][i % 4])
            });
            c_mix.push(match i % 4 {
                0 => Cell::Null,
                1 => Cell::Int((i % 7) as i64),
                2 => Cell::Float((i % 7) as f64),
                _ => Cell::from_text(format!("m{}", i % 5)),
            });
        }
        let c_dict = raw_dict.encode_dict().expect("dict encode");
        assert!(matches!(c_dict, Column::Dict { .. }));
        assert!(matches!(c_txt, Column::Text { .. }));

        let batch = RecordBatch {
            names: Arc::clone(&names),
            cols: vec![c_id, c_int, c_flt, c_bool, c_time, c_txt, c_dict, c_mix],
        };
        let ref_rows = batch.to_rows();

        let ops = [
            CmpOp::Eq,
            CmpOp::Ne,
            CmpOp::Lt,
            CmpOp::Le,
            CmpOp::Gt,
            CmpOp::Ge,
        ];
        let mut idx_buf = Vec::new();

        // 1. Exhaustive comparison checks against `eval_pred`.
        let cases: &[(usize, &str, Value)] = &[
            (1, "ival", Value::Int(0)),
            (1, "ival", Value::Int(-5)),
            (1, "ival", Value::Float(0.0)),
            (1, "ival", Value::Float(2.5)),
            (1, "ival", Value::Float(-2.5)),
            (1, "ival", Value::Float(f64::NAN)),
            (1, "ival", Value::Float(f64::INFINITY)),
            (1, "ival", Value::Float(f64::NEG_INFINITY)),
            (2, "fval", Value::Float(0.0)),
            (2, "fval", Value::Float(-3.5)),
            (2, "fval", Value::Float(f64::NAN)),
            (2, "fval", Value::Int(0)),
            (2, "fval", Value::Int(-3)),
            (2, "fval", Value::Int((1i64 << 54) + 1)),
            (3, "bval", Value::Bool(true)),
            (3, "bval", Value::Bool(false)),
            (4, "tval", Value::Timestamp(1_700_000_000_007)),
            (5, "txt", Value::String(Arc::from("prefix_shared_8bytes_3"))),
            (5, "txt", Value::String(Arc::from("prefix_shared_8bytes_z"))),
            (6, "dict", Value::String(Arc::from("rag"))),
            (6, "dict", Value::String(Arc::from("missing_code"))),
        ];
        for (col_idx, fname, val) in cases {
            let lit = crate::eval::value_cell(val, 0);
            for op in ops {
                let handled = batch.eval_cmp_indices(*col_idx, op, &lit, &mut idx_buf);
                assert!(handled, "expected vectorized handling for {fname} {op:?} {val:?}");
                let pred = Pred::Cmp {
                    field: Field::name(*fname),
                    op,
                    value: val.clone(),
                };
                let want: Vec<u32> = ref_rows
                    .iter()
                    .enumerate()
                    .filter(|(_, r)| crate::eval::eval_pred(&pred, r, 0))
                    .map(|(i, _)| i as u32)
                    .collect();
                assert_eq!(idx_buf, want, "mismatch on {fname} {op:?} {val:?}");
                let filtered = batch.filter_indices(&idx_buf);
                let want_rows: Vec<Row> = want.iter().map(|&i| ref_rows[i as usize].clone()).collect();
                assert_eq!(filtered.to_rows(), want_rows);
            }
        }
        // `Column::Cells` and `Cell::Null` literal cleanly signal fallback (`false`).
        assert!(!batch.eval_cmp_indices(7, CmpOp::Eq, &Cell::Int(1), &mut idx_buf));
        assert!(!batch.eval_cmp_indices(1, CmpOp::Eq, &Cell::Null, &mut idx_buf));

        // 2. `project_cols`, `into_projected_cols`, and `without_cols`.
        let proj_names: Arc<[String]> =
            vec!["dict".to_string(), "id".to_string(), "ival".to_string()].into();
        let p1 = batch.project_cols(&[6, 0, 1], Arc::clone(&proj_names));
        let p2 = batch.clone().into_projected_cols(&[6, 0, 1], Arc::clone(&proj_names));
        assert_eq!(p1, p2);
        assert_eq!(
            p1.to_rows(),
            crate::eval::project(
                &ref_rows,
                &[Field::name("dict"), Field::name("id"), Field::name("ival")]
            )
        );
        let dropped = p1.without_cols(&["ival"]);
        assert_eq!(dropped.names.as_ref(), &["dict".to_string(), "id".to_string()][..]);

        // 3. `sort_by_col` across every column representation, both directions, small (`<= 32`)
        // and large (`> 32`), and `skip` / `take` windows.
        let small_batch = batch.filter_indices(&(0..24u32).collect::<Vec<_>>());
        for src_batch in [&small_batch, &batch] {
            let src_rows = src_batch.to_rows();
            for (col_idx, fname) in names.iter().enumerate() {
                let field = Field::name(fname.as_str());
                for desc in [false, true] {
                    for (skip, take) in [
                        (0usize, None),
                        (0, Some(0)),
                        (0, Some(10)),
                        (5, Some(15)),
                        (45, Some(12)),
                        (src_batch.n() + 3, Some(5)),
                    ] {
                        let got = src_batch.sort_by_col(col_idx, desc, skip, take).to_rows();
                        let mut want = src_rows.clone();
                        let top_k = take.map(|t| skip.saturating_add(t));
                        crate::eval::sort_rows_top_k(&mut want, &field, desc, top_k);
                        let s = skip.min(want.len());
                        let e = take.map_or(want.len(), |t| s.saturating_add(t).min(want.len()));
                        assert_eq!(
                            got,
                            &want[s..e],
                            "sort mismatch col={fname} n={} desc={desc} skip={skip} take={take:?}",
                            src_batch.n()
                        );
                    }
                }
            }
        }
    }
}
