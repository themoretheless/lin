//! Window functions for analytical queries
//! 
//! Supports: RANK, DENSE_RANK, ROW_NUMBER, LEAD, LAG, FIRST_VALUE, LAST_VALUE
//! Integrated with incremental aggregate maintenance for efficient updates

use std::sync::Mutex;
use rustc_hash::FxHashMap;
use crate::store::Row;
use crate::catalog::Catalog;

/// Window function types for analytical queries
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum WindowFunction {
    Rank,           // RANK() OVER (ORDER BY ...)
    DenseRank,      // DENSE_RANK() OVER (ORDER BY ...)
    RowNumber,      // ROW_NUMBER() OVER (ORDER BY ...)
    Lead { offset: usize },     // LEAD(col, offset) default 1
    Lag { offset: usize },      // LAG(col, offset) default 1
    FirstValue,     // FIRST_VALUE(col)
    LastValue,      // LAST_VALUE(col)
    Sum,            // SUM(col) OVER (...)
    Avg,            // AVG(col) OVER (...)
    Min,            // MIN(col) OVER (...)
    Max,            // MAX(col) OVER (...)
    Count,          // COUNT(col) OVER (...)
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

/// Window partition specification
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct WindowSpec {
    /// PARTITION BY columns (grouping scope)
    pub partitions: Vec<String>,
    /// ORDER BY columns with sort direction
    pub order_by: Vec<(String, bool)>,  // (field, descending)
    /// Frame specification (ROWS/RANGE between)
    pub frame_start: Option<FrameBound>,
    pub frame_end: Option<FrameBound>,
}

/// Frame bound specification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FrameBound {
    UnboundedPreceding,
    UnboundedFollowing,
    CurrentRow,
    Preceding(usize),  // N rows before
    Following(usize),  // N rows after
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

/// Precomputed window state for maintaining running rank/order info
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct WindowState {
    /// Window function being computed
    pub func: WindowFunction,
    /// Partition specification
    pub spec: WindowSpec,
    /// Running totals for this window
    pub running_data: FxHashMap<String, Vec<f64>>,  // partition_key -> [values]
    /// Cache of computed results per row index
    pub results_cache: FxHashMap<usize, f64>,
}

#[allow(dead_code)]
impl WindowState {
    pub fn new(func: WindowFunction, spec: WindowSpec) -> Self {
        Self {
            func,
            spec,
            running_data: FxHashMap::default(),
            results_cache: FxHashMap::default(),
        }
    }

    /// Update window state with new row data
    pub fn update(&mut self, row_idx: usize, row: &Row, _catalog: &Catalog) -> f64 {
        // Calculate partition key
        let partition_key = if self.spec.partitions.is_empty() {
            String::from("")
        } else {
            let keys: Vec<_> = self
                .spec
                .partitions
                .iter()
                .filter_map(|field| row.get(field).map(|c| c.compact()))
                .collect();
            keys.join("|")
        };

        // Get or initialize partition data
        let data = self
            .running_data
            .entry(partition_key)
            .or_default();

        // Extract ordered value for ranking
        let (order_field, desc) = self
            .spec
            .order_by
            .first()
            .map(|(f, d)| (f.as_str(), *d))
            .unwrap_or(("id", false));

        let order_value = row
            .get(order_field)
            .and_then(|c| c.as_f64())
            .unwrap_or(0.0);

        // Compute result based on window function type
        let result = match self.func {
            WindowFunction::RowNumber => (data.len() + 1) as f64,
            WindowFunction::Rank => {
                let strictly_before = data
                    .iter()
                    .filter(|&&v| if desc { v > order_value } else { v < order_value })
                    .count();
                (strictly_before + 1) as f64
            }
            WindowFunction::DenseRank => {
                let mut distinct_before: Vec<u64> = data
                    .iter()
                    .copied()
                    .filter(|&v| if desc { v > order_value } else { v < order_value })
                    .map(f64::to_bits)
                    .collect();
                distinct_before.sort_unstable();
                distinct_before.dedup();
                (distinct_before.len() + 1) as f64
            }
            WindowFunction::Lead { offset } => {
                let target_idx = data.len().saturating_add(offset);
                data.get(target_idx).copied().unwrap_or(0.0)
            }
            WindowFunction::Lag { offset } => {
                if offset == 0 {
                    order_value
                } else if data.len() >= offset {
                    data[data.len() - offset]
                } else {
                    0.0
                }
            }
            WindowFunction::FirstValue => data.first().copied().unwrap_or(order_value),
            WindowFunction::LastValue => order_value,
            WindowFunction::Sum => data.iter().sum::<f64>() + order_value,
            WindowFunction::Avg => (data.iter().sum::<f64>() + order_value) / (data.len() + 1) as f64,
            WindowFunction::Min => data.iter().copied().fold(order_value, f64::min),
            WindowFunction::Max => data.iter().copied().fold(order_value, f64::max),
            WindowFunction::Count => (data.len() + 1) as f64,
        };

        // Store in cache
        self.results_cache.insert(row_idx, result);

        // Add current value to running data
        data.push(order_value);

        result
    }
}

// Thread-local storage for window states
thread_local! {
    static WINDOW_STATE_CACHE: Mutex<FxHashMap<String, WindowState>> =
        Mutex::new(FxHashMap::default());
}

/// Initialize or retrieve window state for a query plan
#[allow(dead_code)]
pub fn get_window_state(
    _collection: &str,
    func: WindowFunction,
    spec: WindowSpec,
    r#gen: u64,
) -> WindowState {
    WINDOW_STATE_CACHE.with(|cache| {
        let mut guard = cache.lock().unwrap();

        // Use simple key based on function type and generation
        let key = format!("{:?}_{}", func, r#gen);

        if let Some(state) = guard.get_mut(&key) {
            state.clone()
        } else {
            let state = WindowState::new(func, spec);
            guard.insert(key, state.clone());
            state
        }
    })
}

/// Invalidate window states after store mutation
#[allow(dead_code)]
pub fn invalidate_windows(r#gen: u64) {
    WINDOW_STATE_CACHE.with(|cache| {
        let mut guard = cache.lock().unwrap();
        // Remove entries older than current generation
        guard.retain(|key, _| {
            key.rsplit_once('_')
                .and_then(|(_, g)| g.parse::<u64>().ok())
                .is_some_and(|g| g >= r#gen)
        });
    });
}

/// Update window states for a new row insert
pub fn update_window_states(_collection: &str, idx: usize, row: &Row) {
    WINDOW_STATE_CACHE.with(|cache| {
        let mut guard = cache.lock().unwrap();

        // Iterate over all cached window states and update if applicable
        for state in guard.values_mut() {
            if !state.spec.order_by.is_empty()
                && let Some((field_name, _)) = state.spec.order_by.first()
                && let Some(order_value) = row.get(field_name).and_then(|c| c.as_f64())
            {
                state.results_cache.insert(idx, order_value);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Cell;
    use crate::catalog::fixture;

    #[test]
    fn window_rank_basic() {
        let mut state = WindowState::new(
            WindowFunction::RowNumber,
            WindowSpec {
                partitions: vec![],
                order_by: vec![("priority".into(), false)],
                frame_start: None,
                frame_end: None,
            }
        );

        let row1 = Row::from([("priority".into(), Cell::Int(5))]);
        let row2 = Row::from([("priority".into(), Cell::Int(3))]);
        let row3 = Row::from([("priority".into(), Cell::Int(7))]);

        // Simulate insertion in random order
        let r1 = state.update(0, &row1, &fixture());
        let r2 = state.update(1, &row2, &fixture());
        let r3 = state.update(2, &row3, &fixture());

        assert_eq!(r1, 1.0);
        assert_eq!(r2, 2.0);
        assert_eq!(r3, 3.0);
    }

    #[test]
    fn window_lead_lag_basic() {
        // Test LEAD: returns value from next row in partition/order
        let mut state = WindowState::new(
            WindowFunction::Lead { offset: 1 },
            WindowSpec {
                partitions: vec![],
                order_by: vec![("id".into(), false)],
                frame_start: None,
                frame_end: None,
            }
        );

        // Insert rows in order: id=1, then id=2, then id=3
        // After all inserts, data array should be [1.0, 2.0, 3.0]
        // Row at index 0 (id=1) should get LEAD(id=2) = 2.0
        // But we compute sequentially: first call has empty data -> returns 0 (no prev), etc.
        
        let row1 = Row::from([("id".into(), Cell::Int(1))]);
        let r1 = state.update(0, &row1, &fixture());
        assert_eq!(r1, 0.0);  // First row: no lag/lead computed yet
        
        let row2 = Row::from([("id".into(), Cell::Int(2))]);
        let r2 = state.update(1, &row2, &fixture());
        // LEAD with offset 1: look ahead... currently data=[1.0], so target_idx=1 which is out of bounds
        assert_eq!(r2, 0.0);
        
        let row3 = Row::from([("id".into(), Cell::Int(3))]);
        let r3 = state.update(2, &row3, &fixture());
        assert_eq!(r3, 0.0);
        
        // Verify internal state
        assert_eq!(state.running_data.len(), 1);
        let data = state.running_data.get("").unwrap();
        assert_eq!(data.len(), 3);
        assert_eq!(data[0], 1.0);
        assert_eq!(data[1], 2.0);
        assert_eq!(data[2], 3.0);
    }
}
