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

impl<T: FromCell> FromCell for Option<T> {
    fn from_cell(cell: &Cell) -> Result<Self, Error> {
        match cell {
            Cell::Null => Ok(None),
            other => T::from_cell(other).map(Some),
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
