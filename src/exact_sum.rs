//! Exact accumulation for `sum` / `avg`: integer cells are added in `i128`, so a total
//! that passes through or ends outside the `i64` range (e.g. `i64::MAX + i64::MIN`)
//! loses no precision, and float cells are added separately. The result is still
//! reported as a float, as before; it is only the rounding that moves to the very end.

use crate::store::Cell;

#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct ExactSum {
    int: i128,
    float: f64,
}

impl ExactSum {
    #[inline]
    pub(crate) fn add_cell(&mut self, cell: Option<&Cell>) {
        match cell {
            Some(Cell::Int(i)) => self.int += *i as i128,
            Some(c) => {
                if let Some(f) = c.as_f64() {
                    self.float += f;
                }
            }
            None => {}
        }
    }

    #[inline]
    pub(crate) fn add_i64(&mut self, v: i64) {
        self.int += v as i128;
    }

    #[inline]
    pub(crate) fn add_f64(&mut self, v: f64) {
        self.float += v;
    }

    #[inline]
    pub(crate) fn merge(&mut self, other: ExactSum) {
        self.int += other.int;
        self.float += other.float;
    }

    #[inline]
    pub(crate) fn value(self) -> f64 {
        if self.float == 0.0 {
            self.int as f64
        } else {
            self.int as f64 + self.float
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extremes_cancel_exactly() {
        let mut s = ExactSum::default();
        s.add_cell(Some(&Cell::Int(i64::MAX)));
        s.add_cell(Some(&Cell::Int(i64::MIN)));
        assert_eq!(s.value(), -1.0);
        s.add_cell(Some(&Cell::Float(0.5)));
        assert_eq!(s.value(), -0.5);
        s.add_cell(None);
        s.add_cell(Some(&Cell::Null));
        assert_eq!(s.value(), -0.5);
    }
}
