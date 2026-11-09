//! Window functions for analytical queries
//!
//! Supports: RANK, DENSE_RANK, ROW_NUMBER, LEAD, LAG, FIRST_VALUE, LAST_VALUE,
//! SUM, AVG, MIN, MAX, COUNT

/// Window function types for analytical queries
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum WindowFunction {
    Rank,                   // RANK() OVER (ORDER BY ...)
    DenseRank,              // DENSE_RANK() OVER (ORDER BY ...)
    RowNumber,              // ROW_NUMBER() OVER (ORDER BY ...)
    Lead { offset: usize }, // LEAD(col, offset) default 1
    Lag { offset: usize },  // LAG(col, offset) default 1
    FirstValue,             // FIRST_VALUE(col)
    LastValue,              // LAST_VALUE(col)
    Sum,                    // SUM(col) OVER (...)
    Avg,                    // AVG(col) OVER (...)
    Min,                    // MIN(col) OVER (...)
    Max,                    // MAX(col) OVER (...)
    Count,                  // COUNT(col) OVER (...)
}

impl std::fmt::Display for WindowFunction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WindowFunction::Rank => write!(f, "RANK"),
            WindowFunction::DenseRank => write!(f, "DENSE_RANK"),
            WindowFunction::RowNumber => write!(f, "ROW_NUMBER"),
            WindowFunction::Lead { offset } => write!(f, "LEAD({})", offset),
            WindowFunction::Lag { offset } => write!(f, "LAG({})", offset),
            WindowFunction::FirstValue => write!(f, "FIRST_VALUE"),
            WindowFunction::LastValue => write!(f, "LAST_VALUE"),
            WindowFunction::Sum => write!(f, "SUM"),
            WindowFunction::Avg => write!(f, "AVG"),
            WindowFunction::Min => write!(f, "MIN"),
            WindowFunction::Max => write!(f, "MAX"),
            WindowFunction::Count => write!(f, "COUNT"),
        }
    }
}

/// Window frame specification (ROWS BETWEEN start AND end)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WindowFrame {
    pub start: FrameBound,
    pub end: FrameBound,
}

impl std::fmt::Display for WindowFrame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "rows between {} and {}", self.start, self.end)
    }
}

/// Frame bound specification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FrameBound {
    UnboundedPreceding,
    UnboundedFollowing,
    CurrentRow,
    Preceding(usize), // N rows before
    Following(usize), // N rows after
}

impl std::fmt::Display for FrameBound {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FrameBound::UnboundedPreceding => write!(f, "unbounded preceding"),
            FrameBound::UnboundedFollowing => write!(f, "unbounded following"),
            FrameBound::CurrentRow => write!(f, "current row"),
            FrameBound::Preceding(n) => write!(f, "{n} preceding"),
            FrameBound::Following(n) => write!(f, "{n} following"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_rank_basic() {
        assert_eq!(WindowFunction::Rank.to_string(), "RANK");
        assert_eq!(WindowFunction::DenseRank.to_string(), "DENSE_RANK");
        assert_eq!(WindowFunction::RowNumber.to_string(), "ROW_NUMBER");
    }

    #[test]
    fn window_lead_lag_basic() {
        assert_eq!(WindowFunction::Lead { offset: 1 }.to_string(), "LEAD(1)");
        assert_eq!(WindowFunction::Lag { offset: 2 }.to_string(), "LAG(2)");
        let frame = WindowFrame {
            start: FrameBound::Preceding(2),
            end: FrameBound::CurrentRow,
        };
        assert_eq!(frame.to_string(), "rows between 2 preceding and current row");
    }
}
