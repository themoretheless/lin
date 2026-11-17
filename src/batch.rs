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
    fn push(&mut self, valid: bool) {
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

    fn rebuild(&mut self, range: std::ops::Range<usize>) {
        if self.nulls == 0 {
            self.len = range.len();
            return;
        }
        let mut out = Validity::default();
        for i in range {
            out.push(self.is_valid(i));
        }
        *self = out;
    }

    fn extend(&mut self, other: &Validity) {
        if other.len == 0 {
            return;
        }
        if self.nulls == 0 && other.nulls == 0 {
            self.len += other.len;
            return;
        }
        for i in 0..other.len {
            self.push(other.is_valid(i));
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
            Column::Cells(c) => c.get(i).map(Cell::clone_for_borrow).unwrap_or(Cell::Null),
            _ => Cell::Null,
        }
    }

    pub fn is_null(&self, i: usize) -> bool {
        match self {
            Column::Nulls { .. } => true,
            Column::Int { valid, .. }
            | Column::Time { valid, .. }
            | Column::Float { valid, .. }
            | Column::Bool { valid, .. } => !valid.is_valid(i),
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

    /// Sum of the numeric values, skipping nulls; `None` for non-numeric columns.
    pub fn sum_f64(&self) -> Option<f64> {
        match self {
            Column::Nulls { .. } => Some(0.0),
            Column::Float { vals, valid } => Some(if valid.null_count() == 0 {
                vals.iter().sum()
            } else {
                vals.iter()
                    .enumerate()
                    .filter(|(i, _)| valid.is_valid(*i))
                    .map(|(_, v)| *v)
                    .sum()
            }),
            Column::Int { vals, valid } => Some(if valid.null_count() == 0 {
                vals.iter().map(|v| *v as f64).sum()
            } else {
                vals.iter()
                    .enumerate()
                    .filter(|(i, _)| valid.is_valid(*i))
                    .map(|(_, v)| *v as f64)
                    .sum()
            }),
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

    pub(crate) fn gather(&self, indices: &[u32]) -> Self {
        let n = indices.len();
        match self {
            Column::Nulls { .. } => Column::Nulls { n, cap: n },
            Column::Int { vals, valid } => {
                if valid.null_count() == 0 {
                    Column::Int {
                        vals: indices.iter().map(|&i| vals[i as usize]).collect(),
                        valid: Validity::all_valid(n),
                    }
                } else {
                    let mut out_vals = Vec::with_capacity(n);
                    let mut out_valid = Validity::default();
                    for &i in indices {
                        let idx = i as usize;
                        out_vals.push(vals[idx]);
                        out_valid.push(valid.is_valid(idx));
                    }
                    Column::Int {
                        vals: out_vals,
                        valid: out_valid,
                    }
                }
            }
            Column::Time { vals, valid } => {
                if valid.null_count() == 0 {
                    Column::Time {
                        vals: indices.iter().map(|&i| vals[i as usize]).collect(),
                        valid: Validity::all_valid(n),
                    }
                } else {
                    let mut out_vals = Vec::with_capacity(n);
                    let mut out_valid = Validity::default();
                    for &i in indices {
                        let idx = i as usize;
                        out_vals.push(vals[idx]);
                        out_valid.push(valid.is_valid(idx));
                    }
                    Column::Time {
                        vals: out_vals,
                        valid: out_valid,
                    }
                }
            }
            Column::Float { vals, valid } => {
                if valid.null_count() == 0 {
                    Column::Float {
                        vals: indices.iter().map(|&i| vals[i as usize]).collect(),
                        valid: Validity::all_valid(n),
                    }
                } else {
                    let mut out_vals = Vec::with_capacity(n);
                    let mut out_valid = Validity::default();
                    for &i in indices {
                        let idx = i as usize;
                        out_vals.push(vals[idx]);
                        out_valid.push(valid.is_valid(idx));
                    }
                    Column::Float {
                        vals: out_vals,
                        valid: out_valid,
                    }
                }
            }
            Column::Bool { vals, valid } => {
                if valid.null_count() == 0 {
                    Column::Bool {
                        vals: indices.iter().map(|&i| vals[i as usize]).collect(),
                        valid: Validity::all_valid(n),
                    }
                } else {
                    let mut out_vals = Vec::with_capacity(n);
                    let mut out_valid = Validity::default();
                    for &i in indices {
                        let idx = i as usize;
                        out_vals.push(vals[idx]);
                        out_valid.push(valid.is_valid(idx));
                    }
                    Column::Bool {
                        vals: out_vals,
                        valid: out_valid,
                    }
                }
            }
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
            other => other.gather(indices),
        }
    }

    pub fn reserve(&mut self, additional: usize) {
        match self {
            Column::Nulls { n, cap } => *cap = (*cap).max(*n + additional),
            Column::Int { vals, .. } | Column::Time { vals, .. } => vals.reserve(additional),
            Column::Float { vals, .. } => vals.reserve(additional),
            Column::Bool { vals, .. } => vals.reserve(additional),
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
                valid.rebuild(0..n);
            }
            Column::Float { vals, valid } => {
                vals.truncate(n);
                valid.rebuild(0..n);
            }
            Column::Bool { vals, valid } => {
                vals.truncate(n);
                valid.rebuild(0..n);
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
}

fn prefilled<T: Clone>(n: usize, cap: usize, fill: T) -> Vec<T> {
    let mut v = Vec::with_capacity(cap.max(n));
    v.resize(n, fill);
    v
}

/// Columns are equal when they hold the same values, whatever their representation.
impl PartialEq for Column {
    fn eq(&self, other: &Self) -> bool {
        self.len() == other.len() && (0..self.len()).all(|i| self.get(i) == other.get(i))
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
        if n < 65536
            && self.cols.len() == 2
            && self.names.len() == 2
            && self.names[0] != self.names[1]
        {
            let shape = if self.names.as_ref() == ["id", "title"] {
                Arc::clone(crate::store::shape_id_title())
            } else {
                Row::shape_of(&self.names)
            };
            let swap = self.names[0] > self.names[1];
            let (col0_slice, col1_slice) = self.cols.split_at_mut(1);
            let col0 = &mut col0_slice[0];
            let col1 = &mut col1_slice[0];
            let mut out = if n >= 1024 {
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
        if n < 65536
            && self.cols.len() == 3
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
            let mut out = if n >= 1024 {
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
        self.to_rows()
    }

    /// Materialize a slice of rows with zero-copy Cell cloning - avoids per-row allocations
    pub fn to_rows_slice(&self, start: usize, end: usize) -> Vec<Row> {
        self.to_rows_batch(start, end)
    }

    fn to_rows_batch(&self, start: usize, end: usize) -> Vec<Row> {
        if start >= end || start >= self.n() {
            return Vec::new();
        }
        let end = end.min(self.n());
        let n = end - start;
        if self.names.len() == 2 && self.names[0] != self.names[1] {
            let shape = Row::shape_of(&self.names);
            let swap = self.names[0] > self.names[1];
            let fill = |i: usize| -> Row {
                let c0 = self.cols[0].get(i);
                let c1 = self.cols[1].get(i);
                Row::from_parts(Arc::clone(&shape), if swap { [c1, c0] } else { [c0, c1] })
            };
            if n >= 65536 {
                let threads = std::thread::available_parallelism()
                    .map(|p| p.get())
                    .unwrap_or(1)
                    .min(8)
                    .min((n + 8191) / 8192);
                if threads > 1 {
                    let chunk_size = (n + threads - 1) / threads;
                    let fill = &fill;
                    let chunks: Vec<Vec<Row>> = std::thread::scope(|scope| {
                        let mut handles = Vec::with_capacity(threads);
                        for chunk_idx in 0..threads {
                            let c_start = start + chunk_idx * chunk_size;
                            if c_start >= end {
                                break;
                            }
                            let c_end = (c_start + chunk_size).min(end);
                            handles.push(scope.spawn(move || (c_start..c_end).map(fill).collect()));
                        }
                        handles.into_iter().map(|h| h.join().unwrap()).collect()
                    });
                    let mut out = Vec::with_capacity(n);
                    for chunk in chunks {
                        out.extend(chunk);
                    }
                    return out;
                }
            }
            return (start..end).map(fill).collect();
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
        let fill = |i: usize| -> Row {
            let mut row = template.clone();
            let cells = row.cells_mut();
            for (&slot, col) in slots.iter().zip(self.cols.iter()) {
                cells[slot] = col.get(i);
            }
            row
        };

        if n >= 65536 {
            let threads = std::thread::available_parallelism()
                .map(|p| p.get())
                .unwrap_or(1)
                .min(8)
                .min((n + 8191) / 8192);
            if threads > 1 {
                let chunk_size = (n + threads - 1) / threads;
                let fill = &fill;
                let chunks: Vec<Vec<Row>> = std::thread::scope(|scope| {
                    let mut handles = Vec::with_capacity(threads);
                    for chunk_idx in 0..threads {
                        let c_start = start + chunk_idx * chunk_size;
                        if c_start >= end {
                            break;
                        }
                        let c_end = (c_start + chunk_size).min(end);
                        handles.push(scope.spawn(move || (c_start..c_end).map(fill).collect()));
                    }
                    handles.into_iter().map(|h| h.join().unwrap()).collect()
                });
                let mut out = Vec::with_capacity(n);
                for chunk in chunks {
                    out.extend(chunk);
                }
                return out;
            }
        }

        (start..end).map(fill).collect()
    }

    /// Drain and materialize all rows, consuming the batch efficiently without cloning cells
    pub fn drain_to_rows(&mut self) -> Vec<Row> {
        let n = self.n();
        if n == 0 {
            return Vec::new();
        }
        let cols = std::mem::take(&mut self.cols);
        self.cols = (0..self.names.len()).map(|_| Column::new()).collect();
        let mut col_iters: Vec<_> = cols
            .into_iter()
            .map(|col| col.into_cells().into_iter())
            .collect();
        let mut out = Vec::with_capacity(n);
        if n >= 2 && self.names.windows(2).all(|w| w[0] < w[1]) {
            let template: Row = self
                .names
                .iter()
                .map(|k| (k.clone(), Cell::Null))
                .collect();
            for _ in 0..n {
                let mut row = template.clone();
                for (slot, iter) in row.values_mut().zip(col_iters.iter_mut()) {
                    if let Some(cell) = iter.next() {
                        *slot = cell;
                    }
                }
                out.push(row);
            }
            return out;
        }
        for _ in 0..n {
            let mut row = crate::store::acquire_row();
            for (name, iter) in self.names.iter().zip(col_iters.iter_mut()) {
                if let Some(cell) = iter.next() {
                    row.insert(name.clone(), cell);
                }
            }
            out.push(row);
        }
        out
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
            for row in rows {
                for (col, cell) in batch.cols.iter_mut().zip(row.values()) {
                    col.push(cell.clone_for_borrow());
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
            for row in rows {
                let cell = row
                    .get(name.as_str())
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
}
