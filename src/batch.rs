//! Columnar result batch — in-process OLAP shape beside row-API (`Vec<Row>`).

use std::collections::BTreeMap;
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
        let cols = (0..names.len())
            .map(|_| Vec::with_capacity(cap))
            .collect();
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

    /// Materialize row-maps (row-API). Allocates one `BTreeMap` per row.
    pub fn to_rows(&self) -> Vec<Row> {
        let n = self.n();
        let mut out = Vec::with_capacity(n);
        for i in 0..n {
            let mut row = BTreeMap::new();
            for (name, col) in self.names.iter().zip(self.cols.iter()) {
                row.insert(name.clone(), col[i].clone());
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
}
