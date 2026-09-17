//! Typed row mapping (`FromRow` / `LinRow`).

use crate::error::Error;
use crate::store::{Cell, Row};

/// Convert a single [`Cell`] into a Rust value.
pub trait FromCell: Sized {
    fn from_cell(cell: &Cell) -> Result<Self, Error>;
}

impl FromCell for String {
    fn from_cell(cell: &Cell) -> Result<Self, Error> {
        match cell {
            Cell::Text(s) => Ok(s.to_string()),
            Cell::Null => Err(Error::runtime("expected text, got null")),
            other => Err(Error::runtime(format!("expected text, got {other:?}"))),
        }
    }
}

impl FromCell for i64 {
    fn from_cell(cell: &Cell) -> Result<Self, Error> {
        match cell {
            Cell::Int(n) => Ok(*n),
            Cell::Time(n) => Ok(*n),
            Cell::Null => Err(Error::runtime("expected int, got null")),
            other => Err(Error::runtime(format!("expected int, got {other:?}"))),
        }
    }
}

impl FromCell for f64 {
    fn from_cell(cell: &Cell) -> Result<Self, Error> {
        cell.as_f64()
            .ok_or_else(|| Error::runtime(format!("expected float, got {cell:?}")))
    }
}

impl FromCell for bool {
    fn from_cell(cell: &Cell) -> Result<Self, Error> {
        match cell {
            Cell::Bool(b) => Ok(*b),
            Cell::Null => Err(Error::runtime("expected bool, got null")),
            other => Err(Error::runtime(format!("expected bool, got {other:?}"))),
        }
    }
}

impl FromCell for std::sync::Arc<str> {
    fn from_cell(cell: &Cell) -> Result<Self, Error> {
        match cell {
            Cell::Text(s) => Ok(s.clone()),
            Cell::Null => Err(Error::runtime("expected text, got null")),
            other => Err(Error::runtime(format!("expected text, got {other:?}"))),
        }
    }
}

impl FromCell for Vec<f32> {
    fn from_cell(cell: &Cell) -> Result<Self, Error> {
        match cell {
            Cell::Vec(v) => Ok(v.to_vec()),
            Cell::Null => Err(Error::runtime("expected vec, got null")),
            other => Err(Error::runtime(format!("expected vec, got {other:?}"))),
        }
    }
}

impl FromCell for std::sync::Arc<[f32]> {
    fn from_cell(cell: &Cell) -> Result<Self, Error> {
        match cell {
            Cell::Vec(v) => Ok(v.clone()),
            Cell::Null => Err(Error::runtime("expected vec, got null")),
            other => Err(Error::runtime(format!("expected vec, got {other:?}"))),
        }
    }
}

impl<T: FromCell> FromCell for Option<T> {
    fn from_cell(cell: &Cell) -> Result<Self, Error> {
        match cell {
            Cell::Null => Ok(None),
            other => T::from_cell(other).map(Some),
        }
    }
}

/// Convert a Rust value into a store [`Cell`] (value converter).
pub trait ToCell {
    fn to_cell(&self) -> Cell;
}

impl ToCell for Cell {
    fn to_cell(&self) -> Cell {
        self.clone()
    }
}

impl ToCell for String {
    fn to_cell(&self) -> Cell {
        Cell::text_arc(self.as_str())
    }
}

impl ToCell for str {
    fn to_cell(&self) -> Cell {
        Cell::text_arc(self)
    }
}

impl ToCell for i64 {
    fn to_cell(&self) -> Cell {
        Cell::Int(*self)
    }
}

impl ToCell for f64 {
    fn to_cell(&self) -> Cell {
        Cell::Float(*self)
    }
}

impl ToCell for bool {
    fn to_cell(&self) -> Cell {
        Cell::Bool(*self)
    }
}

impl ToCell for [f32] {
    fn to_cell(&self) -> Cell {
        Cell::vec_arc(self.to_vec())
    }
}

impl ToCell for Vec<f32> {
    fn to_cell(&self) -> Cell {
        Cell::vec_arc(self.clone())
    }
}

impl ToCell for std::sync::Arc<str> {
    fn to_cell(&self) -> Cell {
        Cell::Text(self.clone())
    }
}

impl ToCell for std::sync::Arc<[f32]> {
    fn to_cell(&self) -> Cell {
        Cell::Vec(self.clone())
    }
}

impl<T: ToCell> ToCell for Option<T> {
    fn to_cell(&self) -> Cell {
        match self {
            Some(v) => v.to_cell(),
            None => Cell::Null,
        }
    }
}

/// Map a Lin [`Row`] into a typed Rust value.
pub trait FromRow: Sized {
    fn from_row(row: &Row) -> Result<Self, Error>;
}

impl FromRow for Row {
    fn from_row(row: &Row) -> Result<Self, Error> {
        Ok(row.clone())
    }
}

/// Typed collection row: column list + optional default collection name.
pub trait LinRow: FromRow {
    /// Default collection for `db.from_typed::<Self>()`, if any.
    const COLLECTION: Option<&'static str> = None;
    /// Projection column names (leaf or dotted).
    const COLUMNS: &'static [&'static str];
}

/// Read a required column via [`FromCell`].
pub fn cell_get<T: FromCell>(row: &Row, key: &str) -> Result<T, Error> {
    let cell = row
        .get(key)
        .ok_or_else(|| Error::runtime(format!("missing column `{key}`")))?;
    T::from_cell(cell)
}

/// Map a result set into typed rows.
pub fn map_rows<T: FromRow>(rows: &[Row]) -> Result<Vec<T>, Error> {
    rows.iter().map(T::from_row).collect()
}

/// Optional column (`Null` or missing → `None`).
pub fn cell_opt<T: FromCell>(row: &Row, key: &str) -> Result<Option<T>, Error> {
    match row.get(key) {
        None | Some(Cell::Null) => Ok(None),
        Some(cell) => T::from_cell(cell).map(Some),
    }
}
