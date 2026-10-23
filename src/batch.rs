//! Columnar result batch — in-process OLAP shape beside row-API (`Vec<Row>`).

use std::sync::Arc;

use crate::store::{Cell, Row};

/// Named columns of equal length. Prefer this for analytical project/join hot paths;
/// call [`RecordBatch::to_rows`] when the row-API is required.
#[derive(Debug, Clone, PartialEq)]
pub struct RecordBatch {
    pub names: Arc<[String]>,
    pub cols: Vec<Vec<Cell>>,
}

impl RecordBatch {
    pub fn empty(names: impl Into<Arc<[String]>>) -> Self {
        let names = names.into();
        let cols = (0..names.len()).map(|_| Vec::new()).collect();
        Self { names, cols }
    }

    pub fn with_capacity(names: impl Into<Arc<[String]>>, cap: usize) -> Self {
        let names = names.into();
        let cols = (0..names.len()).map(|_| Vec::with_capacity(cap)).collect();
        Self { names, cols }
    }

    pub fn n(&self) -> usize {
        self.cols.first().map(|c| c.len()).unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.n() == 0
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

        // Pre-allocate based on expected size
        let mut out = Vec::with_capacity(n);

        for i in start..end {
            let mut row = crate::store::acquire_row();
            // Use clone_for_borrow() to preserve Arc references instead of full clone
            for (name, col) in self.names.iter().zip(self.cols.iter()) {
                row.insert(name.clone(), col[i].clone_for_borrow());
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
        self.cols = (0..self.names.len()).map(|_| Vec::new()).collect();
        let mut col_iters: Vec<_> = cols.into_iter().map(|col| col.into_iter()).collect();
        let mut out = Vec::with_capacity(n);
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
            if n >= col.len() {
                col.clear();
            } else {
                col.drain(0..n);
            }
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
            cols,
        }
    }

    /// Reserve space for n rows across all columns (batch append optimization)
    pub fn reserve_cols(&mut self, n: usize) {
        for col in &mut self.cols {
            if col.capacity() < n {
                col.reserve(n);
            }
        }
    }

    /// Append multiple rows efficiently by pre-allocating
    pub fn extend_rows(&mut self, cells_iter: impl Iterator<Item = Vec<Cell>>) {
        for cells in cells_iter {
            self.push_cells(cells.into_iter());
        }
    }
}
