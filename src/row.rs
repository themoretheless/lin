//! Typed row mapping (`FromRow` / `LinRow`), zero-copy borrowed text views, batch decoders,
//! field-provenance diagnostics, and newtype/enum cell macros (`D38`: `LIN-1489`..`LIN-1520`).

use std::fmt;
use std::ops::Deref;

use crate::batch::RecordBatch;
use crate::error::Error;
use crate::store::{Cell, Row};

/// Zero-copy borrowed string slice tied to the lifetime of a [`Row`] or [`Cell`] (`LIN-1497`, `LIN-1498`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BorrowedText<'a>(pub &'a str);

impl<'a> BorrowedText<'a> {
    #[inline]
    pub const fn new(s: &'a str) -> Self {
        Self(s)
    }

    #[inline]
    pub const fn as_str(self) -> &'a str {
        self.0
    }
}

impl Deref for BorrowedText<'_> {
    type Target = str;

    #[inline]
    fn deref(&self) -> &str {
        self.0
    }
}

impl AsRef<str> for BorrowedText<'_> {
    #[inline]
    fn as_ref(&self) -> &str {
        self.0
    }
}

impl fmt::Display for BorrowedText<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

impl ToCell for BorrowedText<'_> {
    #[inline]
    fn to_cell(&self) -> Cell {
        Cell::from_text(self.0)
    }
}

impl Row {
    /// Borrow a string field directly from the row (`Cell::Text` or `Cell::SmallText`) without allocating (`LIN-1497`, `LIN-1498`).
    #[inline]
    pub fn borrow_str<'a>(&'a self, field: &str) -> Option<&'a str> {
        self.get_str(field).and_then(Cell::text)
    }

    /// Borrow a required string field as [`BorrowedText`], attaching field provenance on missing/type errors (`LIN-1497`, `LIN-1520`).
    #[inline]
    pub fn borrow_text<'a>(&'a self, field: &str) -> Result<BorrowedText<'a>, Error> {
        match self.get_str(field) {
            Some(cell) => cell.text().map(BorrowedText).ok_or_else(|| {
                let err = match cell {
                    Cell::Null => Error::runtime("expected text, got null"),
                    other => Error::runtime(format!("expected text, got {other:?}")),
                };
                enrich_field_err(field, err)
            }),
            None => Err(enrich_field_err(field, missing_column_err(field))),
        }
    }
}

/// Convert a single [`Cell`] into a Rust value.
pub trait FromCell: Sized {
    fn from_cell(cell: &Cell) -> Result<Self, Error>;

    #[inline]
    fn from_missing(key: &str) -> Result<Self, Error> {
        Self::from_cell(&Cell::Null)
            .map_err(|_| Error::runtime(format!("missing column `{key}`")))
    }
}

#[cold]
#[inline(never)]
fn missing_column_err(key: &str) -> Error {
    Error::runtime(format!("missing column `{key}`"))
}

#[cold]
#[inline(never)]
pub fn enrich_field_err(key: &str, err: Error) -> Error {
    let prefix = format!("field `{key}`: ");
    let msg = if err.message.starts_with(&prefix) {
        err.message
    } else {
        format!("{prefix}{}", err.message)
    };
    Error::runtime(msg).with_field(key)
}

impl FromCell for String {
    #[inline]
    fn from_cell(cell: &Cell) -> Result<Self, Error> {
        match cell {
            Cell::Text(s) => Ok(s.to_string()),
            Cell::SmallText(s) => Ok(s.as_str().to_string()),
            Cell::Null => Err(Error::runtime("expected text, got null")),
            other => Err(Error::runtime(format!("expected text, got {other:?}"))),
        }
    }

    #[cold]
    fn from_missing(key: &str) -> Result<Self, Error> {
        Err(missing_column_err(key))
    }
}

impl FromCell for i64 {
    #[inline]
    fn from_cell(cell: &Cell) -> Result<Self, Error> {
        match cell {
            Cell::Int(n) => Ok(*n),
            Cell::Time(n) => Ok(*n),
            Cell::Null => Err(Error::runtime("expected int, got null")),
            other => Err(Error::runtime(format!("expected int, got {other:?}"))),
        }
    }

    #[cold]
    fn from_missing(key: &str) -> Result<Self, Error> {
        Err(missing_column_err(key))
    }
}

macro_rules! impl_int_cell {
    ($($t:ty),*) => {
        $(
            impl FromCell for $t {
                #[inline]
                fn from_cell(cell: &Cell) -> Result<Self, Error> {
                    let n = i64::from_cell(cell)?;
                    <$t>::try_from(n).map_err(|_| Error::runtime(format!("int {n} out of range for {}", stringify!($t))))
                }

                #[cold]
                fn from_missing(key: &str) -> Result<Self, Error> {
                    Err(missing_column_err(key))
                }
            }
            impl ToCell for $t {
                #[inline]
                fn to_cell(&self) -> Cell {
                    Cell::Int(*self as i64)
                }
            }
        )*
    };
}

impl_int_cell!(i32, i16, i8, u32, u16, u8, usize);

impl FromCell for f64 {
    #[inline]
    fn from_cell(cell: &Cell) -> Result<Self, Error> {
        cell.as_f64()
            .ok_or_else(|| Error::runtime(format!("expected float, got {cell:?}")))
    }

    #[cold]
    fn from_missing(key: &str) -> Result<Self, Error> {
        Err(missing_column_err(key))
    }
}

impl FromCell for f32 {
    #[inline]
    fn from_cell(cell: &Cell) -> Result<Self, Error> {
        f64::from_cell(cell).map(|n| n as f32)
    }

    #[cold]
    fn from_missing(key: &str) -> Result<Self, Error> {
        Err(missing_column_err(key))
    }
}

impl FromCell for bool {
    #[inline]
    fn from_cell(cell: &Cell) -> Result<Self, Error> {
        match cell {
            Cell::Bool(b) => Ok(*b),
            Cell::Null => Err(Error::runtime("expected bool, got null")),
            other => Err(Error::runtime(format!("expected bool, got {other:?}"))),
        }
    }

    #[cold]
    fn from_missing(key: &str) -> Result<Self, Error> {
        Err(missing_column_err(key))
    }
}

impl FromCell for std::sync::Arc<str> {
    #[inline]
    fn from_cell(cell: &Cell) -> Result<Self, Error> {
        match cell {
            Cell::Text(s) => Ok(s.clone()),
            Cell::SmallText(s) => Ok(std::sync::Arc::from(s.as_str())),
            Cell::Null => Err(Error::runtime("expected text, got null")),
            other => Err(Error::runtime(format!("expected text, got {other:?}"))),
        }
    }

    #[cold]
    fn from_missing(key: &str) -> Result<Self, Error> {
        Err(missing_column_err(key))
    }
}

impl FromCell for crate::store::InlineStr {
    #[inline]
    fn from_cell(cell: &Cell) -> Result<Self, Error> {
        match cell {
            Cell::SmallText(s) => Ok(*s),
            Cell::Text(s) => crate::store::InlineStr::try_new(s.as_ref())
                .ok_or_else(|| Error::runtime(format!("expected small text, got {s}"))),
            Cell::Null => Err(Error::runtime("expected small text, got null")),
            other => Err(Error::runtime(format!(
                "expected small text, got {other:?}"
            ))),
        }
    }

    #[cold]
    fn from_missing(key: &str) -> Result<Self, Error> {
        Err(missing_column_err(key))
    }
}

impl FromCell for Vec<f32> {
    #[inline]
    fn from_cell(cell: &Cell) -> Result<Self, Error> {
        match cell {
            Cell::Vec(v) => Ok(v.to_vec()),
            Cell::Null => Err(Error::runtime("expected vec, got null")),
            other => Err(Error::runtime(format!("expected vec, got {other:?}"))),
        }
    }

    #[cold]
    fn from_missing(key: &str) -> Result<Self, Error> {
        Err(missing_column_err(key))
    }
}

impl FromCell for std::sync::Arc<[f32]> {
    #[inline]
    fn from_cell(cell: &Cell) -> Result<Self, Error> {
        match cell {
            Cell::Vec(v) => Ok(v.clone()),
            Cell::Null => Err(Error::runtime("expected vec, got null")),
            other => Err(Error::runtime(format!("expected vec, got {other:?}"))),
        }
    }

    #[cold]
    fn from_missing(key: &str) -> Result<Self, Error> {
        Err(missing_column_err(key))
    }
}

impl<T: FromCell> FromCell for Option<T> {
    #[inline]
    fn from_cell(cell: &Cell) -> Result<Self, Error> {
        match cell {
            Cell::Null => Ok(None),
            other => T::from_cell(other).map(Some),
        }
    }

    #[inline]
    fn from_missing(_key: &str) -> Result<Self, Error> {
        Ok(None)
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

impl ToCell for crate::store::InlineStr {
    fn to_cell(&self) -> Cell {
        Cell::SmallText(*self)
    }
}

impl ToCell for String {
    fn to_cell(&self) -> Cell {
        Cell::from_text(self.as_str())
    }
}

impl ToCell for str {
    fn to_cell(&self) -> Cell {
        Cell::from_text(self)
    }
}

impl ToCell for &str {
    fn to_cell(&self) -> Cell {
        Cell::from_text(self)
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

impl ToCell for f32 {
    fn to_cell(&self) -> Cell {
        Cell::Float(*self as f64)
    }
}

impl ToCell for bool {
    fn to_cell(&self) -> Cell {
        Cell::Bool(*self)
    }
}

impl ToCell for [f32] {
    fn to_cell(&self) -> Cell {
        Cell::Vec(std::sync::Arc::from(self))
    }
}

impl ToCell for Vec<f32> {
    fn to_cell(&self) -> Cell {
        Cell::Vec(std::sync::Arc::from(self.as_slice()))
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

/// Implement [`FromCell`] and [`ToCell`] for a single-field tuple newtype `Wrapper(Inner)` (`LIN-1505`).
#[macro_export]
macro_rules! impl_newtype_cell {
    ($newtype:ty, $inner:ty $(,)?) => {
        impl $crate::row::FromCell for $newtype {
            #[inline]
            fn from_cell(cell: &$crate::Cell) -> ::core::result::Result<Self, $crate::Error> {
                <$inner as $crate::row::FromCell>::from_cell(cell).map(Self)
            }

            #[inline]
            fn from_missing(key: &str) -> ::core::result::Result<Self, $crate::Error> {
                <$inner as $crate::row::FromCell>::from_missing(key).map(Self)
            }
        }

        impl $crate::row::ToCell for $newtype {
            #[inline]
            fn to_cell(&self) -> $crate::Cell {
                <$inner as $crate::row::ToCell>::to_cell(&self.0)
            }
        }
    };
}

/// Implement [`FromCell`] and [`ToCell`] for a unit enum mapped to canonical string tags (`LIN-1506`).
#[macro_export]
macro_rules! impl_str_enum_cell {
    ($enum_ty:ty { $($variant:ident => $lit:literal),+ $(,)? }) => {
        impl $crate::row::FromCell for $enum_ty {
            #[inline]
            fn from_cell(cell: &$crate::Cell) -> ::core::result::Result<Self, $crate::Error> {
                let s = match cell {
                    $crate::Cell::Text(t) => t.as_ref(),
                    $crate::Cell::SmallText(t) => t.as_str(),
                    $crate::Cell::Null => {
                        return ::core::result::Result::Err($crate::Error::runtime(
                            concat!("expected ", stringify!($enum_ty), " text, got null"),
                        ));
                    }
                    other => {
                        return ::core::result::Result::Err($crate::Error::runtime(format!(
                            "expected {} text, got {other:?}",
                            stringify!($enum_ty)
                        )));
                    }
                };
                match s {
                    $($lit => ::core::result::Result::Ok(<$enum_ty>::$variant),)+
                    other => ::core::result::Result::Err($crate::Error::runtime(format!(
                        "unknown {} variant {:?} (expected one of: {})",
                        stringify!($enum_ty),
                        other,
                        [$($lit),+].join(", ")
                    ))),
                }
            }

            #[cold]
            fn from_missing(key: &str) -> ::core::result::Result<Self, $crate::Error> {
                ::core::result::Result::Err($crate::Error::runtime(format!(
                    "missing column `{key}`"
                )))
            }
        }

        impl $crate::row::ToCell for $enum_ty {
            #[inline]
            fn to_cell(&self) -> $crate::Cell {
                match self {
                    $(<$enum_ty>::$variant => $crate::Cell::from_text($lit),)+
                }
            }
        }
    };
}

pub use crate::{impl_newtype_cell, impl_str_enum_cell};

/// Cached column slot for `#[derive(LinRow)]` batch decoding (`FromRow::from_rows`).
#[doc(hidden)]
pub struct RowSlot<'k>(crate::store::row::FieldSlot<'k>);

impl<'k> RowSlot<'k> {
    #[inline]
    pub fn new(key: &'k str) -> Self {
        Self(crate::store::row::FieldSlot::new(key))
    }

    #[inline]
    pub fn read<T: FromCell>(&mut self, row: &Row) -> Result<T, Error> {
        let key = self.0.key();
        match self.0.get(row) {
            Some(cell) => T::from_cell(cell).map_err(|e| enrich_field_err(key, e)),
            None => T::from_missing(key).map_err(|e| enrich_field_err(key, e)),
        }
    }
}

/// Map a Lin [`Row`] into a typed Rust value.
pub trait FromRow: Sized {
    fn from_row(row: &Row) -> Result<Self, Error>;

    fn from_rows(rows: &[Row]) -> Result<Vec<Self>, Error> {
        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            out.push(Self::from_row(row)?);
        }
        Ok(out)
    }
}

impl FromRow for Row {
    #[inline]
    fn from_row(row: &Row) -> Result<Self, Error> {
        Ok(row.clone())
    }

    #[inline]
    fn from_rows(rows: &[Row]) -> Result<Vec<Self>, Error> {
        Ok(rows.to_vec())
    }
}

macro_rules! impl_tuple_from_row {
    ($err:literal, $($T:ident : $c:ident),+ $(,)?) => {
        impl<$($T: FromCell),+> FromRow for ($($T,)+) {
            #[inline]
            fn from_row(row: &Row) -> Result<Self, Error> {
                let [$($c),+, ..] = row.cells_slice() else {
                    return Err(Error::runtime($err));
                };
                Ok(($($T::from_cell($c)?,)+))
            }

            fn from_rows(rows: &[Row]) -> Result<Vec<Self>, Error> {
                let mut out = Vec::with_capacity(rows.len());
                for row in rows {
                    let [$($c),+, ..] = row.cells_slice() else {
                        return Err(Error::runtime($err));
                    };
                    out.push(($($T::from_cell($c)?,)+));
                }
                Ok(out)
            }
        }
    };
}

impl_tuple_from_row!("expected at least 1 column in row", A: a);
impl_tuple_from_row!("expected at least 2 columns in row", A: a, B: b);
impl_tuple_from_row!("expected at least 3 columns in row", A: a, B: b, C: c);
impl_tuple_from_row!("expected at least 4 columns in row", A: a, B: b, C: c, D: d);

/// Typed collection row: column list + optional default collection name.
pub trait LinRow: FromRow {
    /// Default collection for `db.from_typed::<Self>()`, if any.
    const COLLECTION: Option<&'static str> = None;
    /// Projection column names (leaf or dotted).
    const COLUMNS: &'static [&'static str];
    /// Column proxies for [`crate::query::Queryable::filter_by`].
    type Cols;
    fn cols() -> Self::Cols;
}

/// Read a required column via [`FromCell`], enriching errors with `"field `key`: ..."` (`LIN-1520`).
#[inline]
pub fn cell_get<T: FromCell>(row: &Row, key: &str) -> Result<T, Error> {
    match row.get(key) {
        Some(cell) => T::from_cell(cell).map_err(|e| enrich_field_err(key, e)),
        // Rows are sparse: a field never set is absent, not `Null`. A type that accepts
        // `Null` (`Option<T>`) reads that as `None`; any other type reports the column.
        None => T::from_missing(key).map_err(|e| enrich_field_err(key, e)),
    }
}

/// Map a result set into typed rows.
#[inline]
pub fn map_rows<T: FromRow>(rows: &[Row]) -> Result<Vec<T>, Error> {
    T::from_rows(rows)
}

/// Decode a columnar [`RecordBatch`] into typed rows (`LIN-1499`).
#[inline]
pub fn map_batch<T: FromRow>(batch: &RecordBatch) -> Result<Vec<T>, Error> {
    T::from_rows(&batch.to_rows())
}

impl RecordBatch {
    /// Decode this columnar batch into typed rows `Vec<T>` (`LIN-1499`).
    #[inline]
    pub fn to_typed_rows<T: FromRow>(&self) -> Result<Vec<T>, Error> {
        map_batch(self)
    }
}

/// Optional column (`Null` or missing → `None`).
pub fn cell_opt<T: FromCell>(row: &Row, key: &str) -> Result<Option<T>, Error> {
    match row.get(key) {
        None | Some(Cell::Null) => Ok(None),
        Some(cell) => T::from_cell(cell)
            .map(Some)
            .map_err(|e| enrich_field_err(key, e)),
    }
}
