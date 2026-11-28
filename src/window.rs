//! Window functions for analytical queries
//!
//! Supports: RANK, DENSE_RANK, ROW_NUMBER, LEAD, LAG, FIRST_VALUE, LAST_VALUE,
//! SUM, AVG, MIN, MAX, COUNT

/// Window function types for analytical queries
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
            WindowFunction::Rank => f.write_str("RANK"),
            WindowFunction::DenseRank => f.write_str("DENSE_RANK"),
            WindowFunction::RowNumber => f.write_str("ROW_NUMBER"),
            WindowFunction::Lead { offset } => write!(f, "LEAD({offset})"),
            WindowFunction::Lag { offset } => write!(f, "LAG({offset})"),
            WindowFunction::FirstValue => f.write_str("FIRST_VALUE"),
            WindowFunction::LastValue => f.write_str("LAST_VALUE"),
            WindowFunction::Sum => f.write_str("SUM"),
            WindowFunction::Avg => f.write_str("AVG"),
            WindowFunction::Min => f.write_str("MIN"),
            WindowFunction::Max => f.write_str("MAX"),
            WindowFunction::Count => f.write_str("COUNT"),
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
            FrameBound::UnboundedPreceding => f.write_str("unbounded preceding"),
            FrameBound::UnboundedFollowing => f.write_str("unbounded following"),
            FrameBound::CurrentRow => f.write_str("current row"),
            FrameBound::Preceding(n) => write!(f, "{n} preceding"),
            FrameBound::Following(n) => write!(f, "{n} following"),
        }
    }
}

/// Extended window frame specification supporting `ROWS`, numeric/timestamp `RANGE`,
/// and peer `GROUPS` semantics (`D22`: `LIN-0854`..`LIN-0856`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WindowFrameKind {
    Rows {
        preceding: Option<usize>,
        following: Option<usize>,
    },
    Range {
        max_delta_f64: f64,
    },
    Groups {
        preceding_peers: usize,
        following_peers: usize,
    },
}

/// SQL:2011 window frame exclusion clause (`D22`: `LIN-0856`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum WindowFrameExclude {
    #[default]
    NoOthers,
    CurrentRow,
    Group,
    Ties,
}

/// Memory and partition-size budget for window evaluation (`D22`: `LIN-0869`..`LIN-0872`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowEvalBudget {
    pub max_partition_rows: usize,
    pub max_bytes: usize,
}

/// Execution telemetry for a window evaluation pass (`D22`: `LIN-0875`..`LIN-0878`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WindowDiagnostics {
    pub partitions: usize,
    pub max_partition_rows: usize,
    pub frame_evaluations: usize,
}

/// Compute `PERCENT_RANK` values `(rank - 1) / (n - 1)` for 1-based `ranks`,
/// handling empty (`0` rows) and single-row (`0.0`) partitions cleanly without `NaN` (`LIN-0860`).
pub fn percent_rank_values(n: usize, ranks: &[usize]) -> Vec<f64> {
    if n == 0 || ranks.is_empty() {
        return Vec::new();
    }
    if n == 1 {
        return vec![0.0; ranks.len()];
    }
    let denom = (n - 1) as f64;
    ranks
        .iter()
        .map(|&r| ((r.saturating_sub(1) as f64) / denom).clamp(0.0, 1.0))
        .collect()
}

/// Evaluate a window frame over a partition of `rows` ordered by `order_field` (`LIN-0854`..`LIN-0878`).
///
/// When `value_field` is `Some(field)`, sums numeric/timestamp values (`Int`, `Float`, `Time`)
/// inside each row's active frame (after applying `exclude`); when `value_field` is `None`,
/// counts rows inside each row's active frame. Output element `i` corresponds to `rows[i]`.
pub fn evaluate_window_partition(
    rows: &[crate::store::Row],
    order_field: &str,
    value_field: Option<&str>,
    kind: WindowFrameKind,
    exclude: WindowFrameExclude,
    budget: &WindowEvalBudget,
) -> Result<(Vec<f64>, WindowDiagnostics), crate::error::Error> {
    use crate::error::Error;
    use crate::store::Cell;

    let n = rows.len();
    if n == 0 {
        return Ok((
            Vec::new(),
            WindowDiagnostics {
                partitions: 0,
                max_partition_rows: 0,
                frame_evaluations: 0,
            },
        ));
    }
    if n > budget.max_partition_rows {
        return Err(Error::resource_limit(format!(
            "window partition row count {n} exceeds max_partition_rows {}",
            budget.max_partition_rows
        )));
    }
    let est_bytes = rows
        .iter()
        .map(crate::store::row_byte_size)
        .sum::<usize>()
        .saturating_add(n.saturating_mul(32));
    if est_bytes > budget.max_bytes {
        return Err(Error::resource_limit(format!(
            "window partition working bytes {est_bytes} exceeds max_bytes {}",
            budget.max_bytes
        )));
    }
    if let WindowFrameKind::Range { max_delta_f64 } = kind
        && (max_delta_f64.is_nan() || max_delta_f64 < 0.0)
    {
        return Err(Error::runtime(
            "window RANGE max_delta_f64 must be finite/non-NaN and >= 0.0",
        ));
    }

    static NULL_CELL: Cell = Cell::Null;
    let mut order_slot = crate::store::row::FieldSlot::new(order_field);
    let mut val_slot = value_field.map(crate::store::row::FieldSlot::new);

    let mut perm: Vec<usize> = (0..n).collect();
    let raw_orders: Vec<&Cell> = rows
        .iter()
        .map(|r| order_slot.get(r).unwrap_or(&NULL_CELL))
        .collect();
    perm.sort_by(|&a, &b| crate::eval::cmp_sort(raw_orders[a], raw_orders[b]));

    let sorted_ord: Vec<&Cell> = perm.iter().map(|&idx| raw_orders[idx]).collect();
    let sorted_val: Vec<Option<f64>> = perm
        .iter()
        .map(|&idx| match val_slot.as_mut() {
            Some(vs) => vs.get(&rows[idx]).and_then(Cell::as_f64),
            None => Some(1.0),
        })
        .collect();

    // Build peer groups on `sorted_ord`.
    let mut group_of = vec![0usize; n];
    let mut group_bounds: Vec<(usize, usize)> = Vec::new();
    let mut g_start = 0usize;
    for pos in 1..n {
        if crate::eval::cmp_sort(sorted_ord[pos - 1], sorted_ord[pos]) != std::cmp::Ordering::Equal
        {
            let g_idx = group_bounds.len();
            group_bounds.push((g_start, pos - 1));
            for slot in &mut group_of[g_start..pos] {
                *slot = g_idx;
            }
            g_start = pos;
        }
    }
    let last_g = group_bounds.len();
    group_bounds.push((g_start, n - 1));
    for slot in &mut group_of[g_start..n] {
        *slot = last_g;
    }

    let sorted_ord_num: Vec<Option<f64>> = if matches!(kind, WindowFrameKind::Range { .. }) {
        sorted_ord
            .iter()
            .map(|c| c.as_f64().filter(|x| !x.is_nan()))
            .collect()
    } else {
        Vec::new()
    };

    let mut out = vec![0.0f64; n];
    let mut range_lo = 0usize;
    let mut range_hi = 0usize;

    for pos in 0..n {
        let g = group_of[pos];
        let (peer_s, peer_e) = group_bounds[g];
        let (frame_s, frame_e) = match kind {
            WindowFrameKind::Rows {
                preceding,
                following,
            } => {
                let s = match preceding {
                    Some(k) => pos.saturating_sub(k),
                    None => 0,
                };
                let e = match following {
                    Some(k) => pos.saturating_add(k).min(n - 1),
                    None => n - 1,
                };
                (s, e)
            }
            WindowFrameKind::Groups {
                preceding_peers,
                following_peers,
            } => {
                let gs = g.saturating_sub(preceding_peers);
                let ge = g.saturating_add(following_peers).min(group_bounds.len() - 1);
                (group_bounds[gs].0, group_bounds[ge].1)
            }
            WindowFrameKind::Range { max_delta_f64 } => match sorted_ord_num[pos] {
                None => (peer_s, peer_e),
                Some(center) => {
                    while range_lo < pos
                        && sorted_ord_num[range_lo]
                            .is_none_or(|v| center - v > max_delta_f64)
                    {
                        range_lo += 1;
                    }
                    if range_hi < pos {
                        range_hi = pos;
                    }
                    while range_hi + 1 < n
                        && sorted_ord_num[range_hi + 1]
                            .is_some_and(|v| v - center <= max_delta_f64)
                    {
                        range_hi += 1;
                    }
                    (range_lo, range_hi)
                }
            },
        };

        let mut acc = crate::exact_sum::ExactSum::default();
        if frame_s <= frame_e {
            for q in frame_s..=frame_e {
                let excluded = match exclude {
                    WindowFrameExclude::NoOthers => false,
                    WindowFrameExclude::CurrentRow => q == pos,
                    WindowFrameExclude::Group => q >= peer_s && q <= peer_e,
                    WindowFrameExclude::Ties => q >= peer_s && q <= peer_e && q != pos,
                };
                if !excluded
                    && let Some(v) = sorted_val[q]
                {
                    acc.add_f64(v);
                }
            }
        }
        out[perm[pos]] = acc.value();
    }

    Ok((
        out,
        WindowDiagnostics {
            partitions: 1,
            max_partition_rows: n,
            frame_evaluations: n,
        },
    ))
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
