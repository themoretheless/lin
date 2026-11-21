//! Exact accumulation for `sum` / `avg`: integer cells are added in `i128`, so a total
//! that passes through or ends outside the `i64` range (e.g. `i64::MAX + i64::MIN`)
//! loses no precision. Float cells are added with Neumaier compensation, so a small value
//! between two large ones that cancel (`1e16 + 1 - 1e16`) is kept instead of rounded
//! away. The result is still reported as a float; only the rounding moves to the end.

use crate::store::Cell;

#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct ExactSum {
    int: i128,
    float: f64,
    /// Rounding error lost by `float` so far (Neumaier); only meaningful while `float`
    /// is finite.
    comp: f64,
}

impl ExactSum {
    #[inline]
    pub(crate) fn add_cell(&mut self, cell: Option<&Cell>) {
        match cell {
            Some(Cell::Int(i)) => self.int += *i as i128,
            Some(c) => {
                if let Some(f) = c.as_f64() {
                    self.add_f64(f);
                }
            }
            None => {}
        }
    }

    /// Adds a numeric cell and reports whether it counted, for `avg`: integers stay exact
    /// here exactly as they do for `sum`, instead of being rounded to `f64` one by one.
    #[inline]
    pub(crate) fn add_numeric(&mut self, cell: Option<&Cell>) -> bool {
        match cell {
            Some(Cell::Int(i)) => {
                self.int += *i as i128;
                true
            }
            Some(c) => match c.as_f64() {
                Some(f) => {
                    self.add_f64(f);
                    true
                }
                None => false,
            },
            None => false,
        }
    }

    #[inline]
    pub(crate) fn add_i64(&mut self, v: i64) {
        self.int += v as i128;
    }

    #[inline]
    pub(crate) fn int_part(&self) -> i128 {
        self.int
    }

    #[inline]
    pub(crate) fn add_i64_slice(&mut self, vals: &[i64]) {
        self.int += vals.iter().map(|&v| v as i128).sum::<i128>();
    }

    #[inline]
    pub(crate) fn add_f64(&mut self, v: f64) {
        let t = self.float + v;
        if t.is_finite() {
            self.comp += if self.float.abs() >= v.abs() {
                (self.float - t) + v
            } else {
                (v - t) + self.float
            };
        }
        self.float = t;
    }

    #[inline]
    pub(crate) fn add_f64_slice(&mut self, vals: &[f64]) {
        for &v in vals {
            self.add_f64(v);
        }
    }

    /// A sum assembled from an exact integer part and a float part computed elsewhere
    /// (the GPU or a distributed shard partial state) with the same compensated additions
    /// as [`Self::add_f64`].
    pub(crate) fn from_parts(int: i128, float: f64, comp: f64) -> Self {
        Self { int, float, comp }
    }

    /// Decompose into exact integer part, float part, and Neumaier compensation term.
    pub(crate) fn into_parts(self) -> (i128, f64, f64) {
        (self.int, self.float, self.comp)
    }

    #[inline]
    pub(crate) fn merge(&mut self, other: ExactSum) {
        self.int += other.int;
        self.add_f64(other.float);
        self.comp += other.comp;
    }

    #[inline]
    pub(crate) fn value(self) -> f64 {
        let float = if self.float.is_finite() { self.float + self.comp } else { self.float };
        if float == 0.0 {
            self.int as f64
        } else {
            self.int as f64 + float
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

    #[test]
    fn cancelling_floats_keep_the_small_term() {
        let mut s = ExactSum::default();
        for v in [1e16, 1.0, -1e16] {
            s.add_f64(v);
        }
        assert_eq!(s.value(), 1.0);
        // Split across two partial sums and merged: same answer.
        let (mut a, mut b) = (ExactSum::default(), ExactSum::default());
        a.add_f64(1e16);
        a.add_f64(1.0);
        b.add_f64(-1e16);
        a.merge(b);
        assert_eq!(a.value(), 1.0);
        // Infinities and NaN are not turned into NaN by the compensation.
        let mut inf = ExactSum::default();
        inf.add_f64(f64::INFINITY);
        inf.add_f64(1.0);
        assert_eq!(inf.value(), f64::INFINITY);
        let mut nan = ExactSum::default();
        nan.add_f64(f64::NAN);
        assert!(nan.value().is_nan());
    }
}
