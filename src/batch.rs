//! Columnar result batch — in-process OLAP shape beside row-API (`Vec<Row>`).

use std::sync::Arc;

use crate::store::{Cell, Row};

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
        if self.is_empty() && !matches!(self, Column::Cells(_)) {
            *self = other;
            return;
        }
        match (&mut *self, other) {
            (Column::Cells(a), Column::Cells(b)) => a.extend(b),
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
        let sorted_names = self.names.windows(2).all(|w| w[0] < w[1]);

        if n >= 65536 {
            let threads = std::thread::available_parallelism()
                .map(|p| p.get())
                .unwrap_or(1)
                .min(8)
                .min((n + 8191) / 8192);
            if threads > 1 {
                let chunk_size = (n + threads - 1) / threads;
                let template: Option<Row> = sorted_names.then(|| {
                    self.names
                        .iter()
                        .map(|k| (k.clone(), Cell::Null))
                        .collect()
                });
                let template_ref = template.as_ref();
                let chunks: Vec<Vec<Row>> = std::thread::scope(|scope| {
                    let mut handles = Vec::with_capacity(threads);
                    for chunk_idx in 0..threads {
                        let c_start = start + chunk_idx * chunk_size;
                        if c_start >= end {
                            break;
                        }
                        let c_end = (c_start + chunk_size).min(end);
                        handles.push(scope.spawn(move || {
                            let mut chunk_out = Vec::with_capacity(c_end - c_start);
                            if let Some(tpl) = template_ref {
                                for i in c_start..c_end {
                                    let mut row = tpl.clone();
                                    for (slot, col) in row.values_mut().zip(self.cols.iter()) {
                                        *slot = col.get(i);
                                    }
                                    chunk_out.push(row);
                                }
                            } else {
                                for i in c_start..c_end {
                                    let mut row = crate::store::acquire_row();
                                    for (name, col) in self.names.iter().zip(self.cols.iter()) {
                                        row.insert(name.clone(), col.get(i));
                                    }
                                    chunk_out.push(row);
                                }
                            }
                            chunk_out
                        }));
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

        let mut out = Vec::with_capacity(n);
        if n >= 2 && sorted_names {
            let template: Row = self
                .names
                .iter()
                .map(|k| (k.clone(), Cell::Null))
                .collect();
            for i in start..end {
                let mut row = template.clone();
                for (slot, col) in row.values_mut().zip(self.cols.iter()) {
                    *slot = col.get(i);
                }
                out.push(row);
            }
            return out;
        }

        for i in start..end {
            let mut row = crate::store::acquire_row();
            for (name, col) in self.names.iter().zip(self.cols.iter()) {
                row.insert(name.clone(), col.get(i));
            }
            out.push(row);
        }
        out
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
}
