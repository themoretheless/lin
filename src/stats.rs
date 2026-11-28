//! Collection and column statistics (`ANALYZE`), histograms, and cost-based cardinality estimators.
//!
//! Also re-exports structured `EXPLAIN`, logical optimizer controls/fingerprints, and
//! parser/checker diagnostics for downstream inspection and qualification suites.

use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};

use crate::ast::{CmpOp, Pred, Value};
use crate::error::Error;
use crate::store::{Cell, Row, row_byte_size};

// Public re-exports from sibling frontend/planner modules (since `src/lib.rs` exposes `pub mod stats;`).
pub use crate::catalog::{
    Catalog, Collection as CatalogCollection, FieldInfo as CatalogFieldInfo,
    IndexDef as CatalogIndexDef, Type as CatalogType, fixture as fixture_catalog,
};
pub use crate::check::{
    CheckDiagnostic, DiagnosticKind, bounded_levenshtein, check_diagnostics,
    check_diagnostics_with_catalog, check_program,
};
pub use crate::explain::{
    ExplainBackend, ExplainCtx, ExplainNode, PlanAccuracy, RunStats, StructuredExplain,
    compare_explain_with_actual, explain_structured, explain_structured_with_catalog,
};
pub use crate::parse::{
    DEFAULT_MAX_AST_DEPTH, DEFAULT_MAX_QUERY_BYTES, MAX_DECODED_STRING_BYTES, ParseInspection,
    SYNTAX_VERSION, format_program_canonical, format_query_canonical, format_stmt_canonical,
    minimize_malformed_query, offset_to_line_col, parse_inspect, parse_program, parse_with_limits,
};
pub use crate::plan::{
    Backend, Effect, LazyDnfIter, MAX_DNF_CLAUSES, Node, NodeKind, OptimizerDebugCounters,
    OptimizerOptions, Plan, RewriteTraceEntry, deduplicate_conjuncts, execute_with_optimizer_opts,
    has_volatile_barrier, is_deterministic_pred, plan_fingerprint, plan_program_with_opts,
    predicate_fingerprint, stream_dnf_bounded,
};

/// Persisted statistics format version (`LIN-0593`).
pub const STATS_VERSION: u32 = 1;

/// Default memory cap for [`StatsCatalog`] (`16 MiB`, `LIN-0598`).
pub const DEFAULT_MAX_STATS_BYTES: usize = 16 * 1024 * 1024;

/// Maximum heavy-hitter (MCV) entries stored per column or posting fanout summary.
const MAX_MCV_ENTRIES: usize = 16;

/// 16-bucket equi-width histogram over finite numeric values (`LIN-0566`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NumericRangeHistogram {
    pub min: f64,
    pub max: f64,
    pub nan_count: usize,
    pub buckets: [usize; 16],
}

impl NumericRangeHistogram {
    /// Estimate fraction of finite values in this histogram satisfying `op target`.
    pub fn estimate_range_fraction(&self, op: CmpOp, target: f64) -> f64 {
        let total_finite: usize = self.buckets.iter().sum();
        if total_finite == 0 {
            return 0.0;
        }
        if target.is_nan() {
            return 0.0;
        }
        if self.min == self.max {
            let matches = match op {
                CmpOp::Eq => target == self.min,
                CmpOp::Ne => target != self.min,
                CmpOp::Gt => self.min > target,
                CmpOp::Ge => self.min >= target,
                CmpOp::Lt => self.min < target,
                CmpOp::Le => self.min <= target,
            };
            return if matches { 1.0 } else { 0.0 };
        }
        match op {
            CmpOp::Lt | CmpOp::Le => {
                if target < self.min {
                    return 0.0;
                }
                if target > self.max || (op == CmpOp::Le && target == self.max) {
                    return 1.0;
                }
                self.cdf(target)
            }
            CmpOp::Gt | CmpOp::Ge => {
                if target > self.max {
                    return 0.0;
                }
                if target < self.min || (op == CmpOp::Ge && target == self.min) {
                    return 1.0;
                }
                (1.0 - self.cdf(target)).clamp(0.0, 1.0)
            }
            CmpOp::Eq => {
                if target < self.min || target > self.max {
                    return 0.0;
                }
                let b = self.bucket_index(target);
                (self.buckets[b] as f64 / total_finite as f64).min(1.0)
            }
            CmpOp::Ne => {
                if target < self.min || target > self.max {
                    return 1.0;
                }
                1.0
            }
        }
    }

    fn bucket_index(&self, v: f64) -> usize {
        if self.max <= self.min {
            return 0;
        }
        let span = self.max - self.min;
        let ratio = if span.is_infinite() {
            ((v * 0.5 - self.min * 0.5) / (self.max * 0.5 - self.min * 0.5)).clamp(0.0, 1.0)
        } else {
            ((v - self.min) / span).clamp(0.0, 1.0)
        };
        ((ratio * 16.0).floor() as usize).min(15)
    }

    fn cdf(&self, target: f64) -> f64 {
        let total_finite: usize = self.buckets.iter().sum();
        if total_finite == 0 {
            return 0.0;
        }
        let span = self.max - self.min;
        let ratio = if span.is_infinite() {
            ((target * 0.5 - self.min * 0.5) / (self.max * 0.5 - self.min * 0.5)).clamp(0.0, 1.0)
        } else {
            ((target - self.min) / span).clamp(0.0, 1.0)
        };
        let scaled = ratio * 16.0;
        let bucket = (scaled.floor() as usize).min(15);
        let frac_in_bucket = (scaled - bucket as f64).clamp(0.0, 1.0);
        let below: usize = self.buckets[..bucket].iter().sum();
        let in_b = self.buckets[bucket] as f64 * frac_in_bucket;
        ((below as f64 + in_b) / total_finite as f64).clamp(0.0, 1.0)
    }
}

/// 8-bucket log2 byte-length histogram over text values (`LIN-0564`).
///
/// Bucket boundaries:
/// - `0`: `0..=1` bytes
/// - `1`: `2..=3` bytes
/// - `2`: `4..=7` bytes
/// - `3`: `8..=15` bytes
/// - `4`: `16..=31` bytes
/// - `5`: `32..=63` bytes
/// - `6`: `64..=127` bytes
/// - `7`: `128+` bytes
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextLengthHistogram {
    pub min_len: usize,
    pub max_len: usize,
    pub avg_len: f64,
    pub buckets: [usize; 8],
}

impl TextLengthHistogram {
    #[inline]
    pub fn bucket_for_len(len: usize) -> usize {
        if len <= 1 {
            0
        } else {
            ((usize::BITS - 1 - len.leading_zeros()) as usize).min(7)
        }
    }

    #[inline]
    pub const fn bucket_bounds(bucket: usize) -> (usize, usize) {
        match bucket {
            0 => (0, 1),
            1 => (2, 3),
            2 => (4, 7),
            3 => (8, 15),
            4 => (16, 31),
            5 => (32, 63),
            6 => (64, 127),
            _ => (128, usize::MAX),
        }
    }
}

/// Vector dimension summary for `Cell::Vec` columns (`LIN-0568`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VectorDimStats {
    pub min_dim: usize,
    pub max_dim: usize,
    pub non_null_vecs: usize,
}

/// Posting list fanout distribution for indexed / key columns (`LIN-0565`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PostingFanoutHistogram {
    pub distinct_keys: usize,
    pub total_postings: usize,
    pub min_fanout: usize,
    pub max_fanout: usize,
    pub avg_fanout: f64,
    /// Fraction of postings belonging to the single most frequent key (`[0.0, 1.0]`).
    pub max_key_fraction: f64,
    /// Log2 posting-length buckets (`1`, `2..=3`, `4..=7`, ..., `128+`).
    pub buckets: [usize; 8],
    /// Top heavy-hitter keys `(canonical_key, sample_count)` for skew-aware estimation (`LIN-0582`).
    pub heavy_hitters: Vec<(String, usize)>,
}

impl PostingFanoutHistogram {
    #[inline]
    pub fn bucket_for_fanout(fanout: usize) -> usize {
        if fanout <= 1 {
            0
        } else {
            ((usize::BITS - 1 - fanout.leading_zeros()) as usize).min(7)
        }
    }

    pub fn estimate_bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + self
                .heavy_hitters
                .iter()
                .map(|(k, _)| std::mem::size_of::<(String, usize)>() + k.len())
                .sum::<usize>()
    }
}

/// Per-column statistics collected by `ANALYZE` (`LIN-0562`..`LIN-0568`, `LIN-0591`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ColumnStats {
    pub non_null_count: usize,
    pub null_fraction: f64,
    pub distinct_estimate: usize,
    /// Estimator confidence in `[0.1, 1.0]` (`1.0` when `sampled_rows == row_count`, `LIN-0591`).
    pub confidence: f64,
    pub numeric_hist: Option<NumericRangeHistogram>,
    pub text_hist: Option<TextLengthHistogram>,
    pub vec_stats: Option<VectorDimStats>,
    /// Bounded most-common values `(canonical_key, sample_fraction_of_rows)` for skew resistance.
    pub mcv: Vec<(String, f64)>,
}

impl ColumnStats {
    pub fn estimate_bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + self
                .mcv
                .iter()
                .map(|(k, _)| std::mem::size_of::<(String, f64)>() + k.len())
                .sum::<usize>()
    }
}

/// Collection-level statistics (`LIN-0561`..`LIN-0572`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TableStats {
    pub row_count: usize,
    pub sampled_rows: usize,
    pub avg_row_bytes: usize,
    pub mutations_since_analyze: usize,
    pub analyzed_at_gen: u64,
    pub columns: BTreeMap<String, ColumnStats>,
    pub index_fanouts: BTreeMap<String, PostingFanoutHistogram>,
    pub composite_prefixes: BTreeMap<String, Vec<usize>>,
}

impl TableStats {
    pub fn estimate_bytes(&self) -> usize {
        let mut bytes = std::mem::size_of::<Self>();
        for (k, v) in &self.columns {
            bytes += std::mem::size_of::<String>() + k.len() + v.estimate_bytes();
        }
        for (k, v) in &self.index_fanouts {
            bytes += std::mem::size_of::<String>() + k.len() + v.estimate_bytes();
        }
        for (k, v) in &self.composite_prefixes {
            bytes += std::mem::size_of::<String>()
                + k.len()
                + std::mem::size_of::<Vec<usize>>()
                + v.len() * std::mem::size_of::<usize>();
        }
        bytes
    }

    /// Record composite prefix distinct estimates and posting fanout for a multi-column index (`LIN-0565`, `LIN-0567`).
    pub fn record_composite_index(&mut self, label: &str, fields: &[&str], rows: &[Row]) {
        if fields.is_empty() {
            return;
        }
        let sample_n = self.sampled_rows.min(rows.len());
        let total_n = rows.len();
        let mut prefix_maps: Vec<HashMap<String, usize>> =
            (0..fields.len()).map(|_| HashMap::new()).collect();
        for idx in sample_indices(total_n, sample_n) {
            let row = &rows[idx];
            let mut key_buf = String::new();
            for (depth, &f) in fields.iter().enumerate() {
                if depth > 0 {
                    key_buf.push('\x1f');
                }
                match row.get(f) {
                    Some(c) if !c.is_null() => append_canonical_cell(&mut key_buf, c),
                    _ => key_buf.push_str("null"),
                }
                *prefix_maps[depth].entry(key_buf.clone()).or_insert(0) += 1;
            }
        }
        let prefixes: Vec<usize> = prefix_maps
            .iter()
            .map(|m| scale_distinct_count(m, sample_n, total_n.max(self.row_count)))
            .collect();
        if let Some(full_map) = prefix_maps.last() {
            let fanout = build_fanout_histogram(full_map, sample_n);
            self.index_fanouts.insert(label.to_string(), fanout);
        }
        self.composite_prefixes.insert(label.to_string(), prefixes);
    }
}

/// Versioned, memory-bounded catalog of collection statistics (`LIN-0569`..`LIN-0572`, `LIN-0592`..`LIN-0599`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StatsCatalog {
    pub version: u32,
    pub max_stats_bytes: usize,
    pub stale_fraction_threshold: f64,
    tables: BTreeMap<String, TableStats>,
}

impl Default for StatsCatalog {
    fn default() -> Self {
        Self::new()
    }
}

impl StatsCatalog {
    pub fn new() -> Self {
        Self {
            version: STATS_VERSION,
            max_stats_bytes: DEFAULT_MAX_STATS_BYTES,
            stale_fraction_threshold: 0.20,
            tables: BTreeMap::new(),
        }
    }

    pub fn with_max_bytes(max_stats_bytes: usize) -> Self {
        Self {
            max_stats_bytes,
            ..Self::new()
        }
    }

    /// Approximate memory currently retained by this statistics catalog (`LIN-0598`).
    pub fn used_bytes(&self) -> usize {
        let mut total = std::mem::size_of::<Self>();
        for (name, stats) in &self.tables {
            total += std::mem::size_of::<String>() + name.len() + stats.estimate_bytes();
        }
        total
    }

    /// Analyze an in-memory slice of rows with an optional deterministic sample cap (`LIN-0569`, `LIN-0570`).
    pub fn analyze_rows(
        &mut self,
        collection: &str,
        rows: &[Row],
        sample_limit: Option<usize>,
    ) -> Result<&TableStats, Error> {
        self.analyze_rows_at_gen(collection, rows, sample_limit, 0, &[])
    }

    /// Analyze rows at a specific store generation, optionally computing composite index prefix stats.
    pub fn analyze_rows_at_gen(
        &mut self,
        collection: &str,
        rows: &[Row],
        sample_limit: Option<usize>,
        analyzed_at_gen: u64,
        composite_indexes: &[(String, Vec<String>)],
    ) -> Result<&TableStats, Error> {
        let mut table_stats = compute_table_stats(rows, sample_limit, analyzed_at_gen);
        for (label, fields) in composite_indexes {
            let refs: Vec<&str> = fields.iter().map(String::as_str).collect();
            table_stats.record_composite_index(label, &refs, rows);
        }

        let candidate_bytes = self.projected_bytes_with(collection, &table_stats);
        if candidate_bytes > self.max_stats_bytes {
            return Err(Error::runtime(format!(
                "statistics memory budget exceeded for `{collection}`: {candidate_bytes} > {} bytes",
                self.max_stats_bytes
            )));
        }
        self.tables.insert(collection.to_string(), table_stats);
        Ok(self.tables.get(collection).expect("just inserted"))
    }

    /// Analyze a live `Db` collection, including any catalog-declared indexes on it.
    pub fn analyze_db_collection(
        &mut self,
        db: &crate::Db,
        collection: &str,
        sample_limit: Option<usize>,
    ) -> Result<&TableStats, Error> {
        let rows = db.store.collection(collection);
        let composites: Vec<(String, Vec<String>)> = db
            .catalog
            .indexes_on(collection)
            .into_iter()
            .map(|idx| (idx.label(), idx.fields.clone()))
            .collect();
        self.analyze_rows_at_gen(collection, rows, sample_limit, db.store.r#gen, &composites)
    }

    /// Incrementally record row insertions/updates/deletions for staleness detection (`LIN-0571`).
    pub fn record_mutations(&mut self, collection: &str, delta_rows: usize) {
        if let Some(ts) = self.tables.get_mut(collection) {
            ts.mutations_since_analyze = ts.mutations_since_analyze.saturating_add(delta_rows);
        }
    }

    /// Whether statistics for `collection` are missing or have exceeded the mutation threshold (`LIN-0572`).
    pub fn is_stale(&self, collection: &str) -> bool {
        let Some(ts) = self.tables.get(collection) else {
            return true;
        };
        if ts.row_count == 0 {
            return ts.mutations_since_analyze > 0;
        }
        let threshold = ((ts.row_count as f64) * self.stale_fraction_threshold)
            .ceil()
            .max(1.0) as usize;
        ts.mutations_since_analyze >= threshold
    }

    /// Drop statistics for a single collection (`LIN-0599`).
    pub fn reset(&mut self, collection: &str) -> bool {
        self.tables.remove(collection).is_some()
    }

    /// Drop all collected statistics (`LIN-0599`).
    pub fn reset_all(&mut self) {
        self.tables.clear();
    }

    /// Inspect collected statistics for `collection` (`LIN-0592`).
    pub fn inspect(&self, collection: &str) -> Option<&TableStats> {
        self.tables.get(collection)
    }

    /// Mutable access for schema migration or composite index registration (`LIN-0596`).
    pub fn inspect_mut(&mut self, collection: &str) -> Option<&mut TableStats> {
        self.tables.get_mut(collection)
    }

    /// All analyzed collection names in deterministic order.
    pub fn collections(&self) -> impl Iterator<Item = &str> {
        self.tables.keys().map(String::as_str)
    }

    /// Reconcile persisted statistics after a schema change (`LIN-0596`): drops removed
    /// collections and prunes dropped columns/indexes while keeping valid column stats intact.
    pub fn reconcile_with_catalog(&mut self, cat: &Catalog) {
        self.tables
            .retain(|col_name, _| cat.collection(col_name).is_some());
        for (col_name, ts) in &mut self.tables {
            if let Some(col_def) = cat.collection(col_name) {
                ts.columns.retain(|f, _| col_def.field(f).is_some());
                ts.index_fanouts
                    .retain(|k, _| col_def.field(k).is_some() || cat.indexes.contains_key(k));
                ts.composite_prefixes
                    .retain(|k, _| cat.indexes.contains_key(k));
            }
        }
    }

    /// Serialize to JSON with `STATS_VERSION` envelope (`LIN-0593`, `LIN-0594`).
    pub fn to_json_bytes(&self) -> Result<Vec<u8>, Error> {
        serde_json::to_vec(self).map_err(|e| Error::runtime(format!("stats encode error: {e}")))
    }

    /// Deserialize from JSON and validate `STATS_VERSION` (`LIN-0593`, `LIN-0594`).
    pub fn from_json_bytes(bytes: &[u8]) -> Result<Self, Error> {
        let cat: Self = serde_json::from_slice(bytes)
            .map_err(|e| Error::runtime(format!("stats decode error: {e}")))?;
        if cat.version != STATS_VERSION {
            return Err(Error::runtime(format!(
                "unsupported statistics version {}, expected {}",
                cat.version, STATS_VERSION
            )));
        }
        if cat.used_bytes() > cat.max_stats_bytes {
            return Err(Error::runtime(format!(
                "persisted statistics exceed memory budget ({} > {} bytes)",
                cat.used_bytes(),
                cat.max_stats_bytes
            )));
        }
        Ok(cat)
    }

    /// Serialize to compact MessagePack bytes (`LIN-0594`).
    pub fn to_msgpack_bytes(&self) -> Result<Vec<u8>, Error> {
        rmp_serde::to_vec_named(self)
            .map_err(|e| Error::runtime(format!("stats msgpack encode error: {e}")))
    }

    /// Deserialize from MessagePack bytes and validate `STATS_VERSION` + budget (`LIN-0593`, `LIN-0598`).
    pub fn from_msgpack_bytes(bytes: &[u8]) -> Result<Self, Error> {
        let cat: Self = rmp_serde::from_slice(bytes)
            .map_err(|e| Error::runtime(format!("stats msgpack decode error: {e}")))?;
        if cat.version != STATS_VERSION {
            return Err(Error::runtime(format!(
                "unsupported statistics version {}, expected {}",
                cat.version, STATS_VERSION
            )));
        }
        if cat.used_bytes() > cat.max_stats_bytes {
            return Err(Error::runtime(format!(
                "persisted statistics exceed memory budget ({} > {} bytes)",
                cat.used_bytes(),
                cat.max_stats_bytes
            )));
        }
        Ok(cat)
    }

    fn projected_bytes_with(&self, collection: &str, replacement: &TableStats) -> usize {
        let mut total = std::mem::size_of::<Self>();
        let mut replaced = false;
        for (name, stats) in &self.tables {
            if name == collection {
                total += std::mem::size_of::<String>() + name.len() + replacement.estimate_bytes();
                replaced = true;
            } else {
                total += std::mem::size_of::<String>() + name.len() + stats.estimate_bytes();
            }
        }
        if !replaced {
            total +=
                std::mem::size_of::<String>() + collection.len() + replacement.estimate_bytes();
        }
        total
    }
}

/// Calibrated physical cost weights (`LIN-0585`..`LIN-0588`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CostUnits {
    pub cpu_tuple_cost: f64,
    pub mem_byte_penalty: f64,
    pub gpu_transfer_byte_cost: f64,
    pub cold_page_cost: f64,
}

impl Default for CostUnits {
    fn default() -> Self {
        Self {
            cpu_tuple_cost: 0.01,
            mem_byte_penalty: 0.000_001,
            gpu_transfer_byte_cost: 0.000_005,
            cold_page_cost: 1.5,
        }
    }
}

/// Estimated row count paired with statistical confidence in `[0.0, 1.0]` (`LIN-0591`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CardinalityEstimate {
    pub rows: usize,
    pub confidence: f64,
}

/// Estimated join cardinality, hash-table build memory, and build-side choice (`LIN-0577`, `LIN-0578`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct JoinCostEstimate {
    pub output_rows: usize,
    pub build_bytes: usize,
    pub recommended_build_right: bool,
    pub total_cost: f64,
}

/// Feedback receipt comparing estimated vs actual execution cardinality (`LIN-0581`, `LIN-0589`, `LIN-0590`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SelectivityFeedbackReceipt {
    pub estimated: usize,
    pub actual: usize,
    pub q_error: f64,
    pub underestimated: bool,
}

/// Compare estimated vs actual row counts and compute standard `q_error >= 1.0` (`LIN-0589`, `LIN-0590`).
pub fn compare_estimate_actual(estimated: usize, actual: usize) -> SelectivityFeedbackReceipt {
    let q_error = if estimated == actual {
        1.0
    } else {
        let e = estimated.max(1) as f64;
        let a = actual.max(1) as f64;
        if e >= a { e / a } else { a / e }
    };
    SelectivityFeedbackReceipt {
        estimated,
        actual,
        q_error,
        underestimated: actual > estimated,
    }
}

/// Calibrated cost & cardinality estimator (`LIN-0573`..`LIN-0591`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CostEstimator {
    pub units: CostUnits,
}

impl Default for CostEstimator {
    fn default() -> Self {
        Self::new(CostUnits::default())
    }
}

impl CostEstimator {
    pub const fn new(units: CostUnits) -> Self {
        Self { units }
    }

    /// Estimate cardinality of an index seek (`field op val`) against `TableStats` (`LIN-0573`).
    pub fn estimate_index_seek(
        &self,
        stats: &TableStats,
        field: &str,
        op: CmpOp,
        val: &Value,
    ) -> CardinalityEstimate {
        if stats.row_count == 0 {
            return CardinalityEstimate {
                rows: 0,
                confidence: 1.0,
            };
        }
        let Some(col) = stats.columns.get(field) else {
            return CardinalityEstimate {
                rows: 0,
                confidence: 0.25,
            };
        };
        let non_null_rows = ((1.0 - col.null_fraction).clamp(0.0, 1.0) * stats.row_count as f64)
            .round() as usize;
        if non_null_rows == 0 {
            let rows = if op == CmpOp::Ne { stats.row_count } else { 0 };
            return CardinalityEstimate {
                rows,
                confidence: col.confidence,
            };
        }

        // Unbound runtime parameter: use uniform/range priors with discounted confidence.
        if matches!(val, Value::Param(_) | Value::Now | Value::NowMinus(_)) {
            let rows = match op {
                CmpOp::Eq => (non_null_rows / col.distinct_estimate.max(1)).max(1),
                CmpOp::Ne => stats
                    .row_count
                    .saturating_sub(non_null_rows / col.distinct_estimate.max(1)),
                CmpOp::Gt | CmpOp::Ge | CmpOp::Lt | CmpOp::Le => {
                    ((non_null_rows as f64) * 0.33).round() as usize
                }
            };
            return CardinalityEstimate {
                rows: rows.min(stats.row_count),
                confidence: (col.confidence * 0.65).clamp(0.1, 1.0),
            };
        }

        match op {
            CmpOp::Eq => {
                if let Some(num_val) = value_as_f64(val)
                    && let Some(hist) = &col.numeric_hist
                {
                    if num_val.is_nan() {
                        return CardinalityEstimate {
                            rows: 0,
                            confidence: col.confidence,
                        };
                    }
                    if hist.buckets.iter().sum::<usize>() == 0
                        || num_val < hist.min
                        || num_val > hist.max
                    {
                        return CardinalityEstimate {
                            rows: 0,
                            confidence: col.confidence,
                        };
                    }
                }
                if let Some(key) = canonical_value_key(val) {
                    if let Some(&(_, frac)) = col.mcv.iter().find(|(k, _)| k == &key) {
                        return CardinalityEstimate {
                            rows: ((frac * stats.row_count as f64).round() as usize)
                                .min(non_null_rows),
                            confidence: col.confidence,
                        };
                    }
                    if !col.mcv.is_empty() {
                        let mcv_frac_sum: f64 = col.mcv.iter().map(|(_, f)| *f).sum();
                        let tail_frac = (1.0 - col.null_fraction - mcv_frac_sum).max(0.0);
                        let tail_distinct = col.distinct_estimate.saturating_sub(col.mcv.len());
                        if tail_distinct == 0 || tail_frac <= 1e-12 {
                            return CardinalityEstimate {
                                rows: 0,
                                confidence: col.confidence,
                            };
                        }
                        let est = ((tail_frac * stats.row_count as f64) / tail_distinct as f64)
                            .round() as usize;
                        return CardinalityEstimate {
                            rows: est.max(1).min(non_null_rows),
                            confidence: col.confidence,
                        };
                    }
                }
                let est = (non_null_rows as f64 / col.distinct_estimate.max(1) as f64).round()
                    as usize;
                CardinalityEstimate {
                    rows: est.max(1).min(non_null_rows),
                    confidence: col.confidence,
                }
            }
            CmpOp::Ne => {
                let eq = self.estimate_index_seek(stats, field, CmpOp::Eq, val);
                CardinalityEstimate {
                    rows: stats.row_count.saturating_sub(eq.rows),
                    confidence: eq.confidence,
                }
            }
            CmpOp::Gt | CmpOp::Ge | CmpOp::Lt | CmpOp::Le => {
                if let Some(num_val) = value_as_f64(val) {
                    if num_val.is_nan() {
                        return CardinalityEstimate {
                            rows: 0,
                            confidence: col.confidence,
                        };
                    }
                    if let Some(hist) = &col.numeric_hist {
                        let finite_sample: usize = hist.buckets.iter().sum();
                        let finite_ratio = if col.non_null_count > 0 {
                            finite_sample as f64 / col.non_null_count as f64
                        } else {
                            0.0
                        };
                        let frac = hist.estimate_range_fraction(op, num_val);
                        let rows = ((non_null_rows as f64) * finite_ratio * frac).round() as usize;
                        return CardinalityEstimate {
                            rows: rows.min(non_null_rows),
                            confidence: col.confidence,
                        };
                    }
                }
                CardinalityEstimate {
                    rows: (((non_null_rows as f64) * 0.33).round() as usize).min(non_null_rows),
                    confidence: (col.confidence * 0.5).clamp(0.1, 1.0),
                }
            }
        }
    }

    /// Estimate selectivity of an arbitrary residual filter predicate (`LIN-0574`, `LIN-0583`, `LIN-0584`).
    pub fn estimate_residual_filter(&self, stats: &TableStats, pred: &Pred) -> CardinalityEstimate {
        if stats.row_count == 0 {
            return CardinalityEstimate {
                rows: 0,
                confidence: 1.0,
            };
        }
        let (sel, conf) = self.estimate_pred_selectivity(stats, pred);
        let rows = ((sel.clamp(0.0, 1.0) * stats.row_count as f64).round() as usize)
            .min(stats.row_count);
        CardinalityEstimate {
            rows,
            confidence: conf.clamp(0.1, 1.0),
        }
    }

    /// Estimate candidate count for substring/token search (`~ "needle"` / `has "needle"`, `LIN-0575`).
    pub fn estimate_substring(
        &self,
        stats: &TableStats,
        field: &str,
        needle_len: usize,
    ) -> CardinalityEstimate {
        if stats.row_count == 0 {
            return CardinalityEstimate {
                rows: 0,
                confidence: 1.0,
            };
        }
        let Some(col) = stats.columns.get(field) else {
            return CardinalityEstimate {
                rows: 0,
                confidence: 0.2,
            };
        };
        let non_null_rows = ((1.0 - col.null_fraction).clamp(0.0, 1.0) * stats.row_count as f64)
            .round() as usize;
        if non_null_rows == 0 {
            return CardinalityEstimate {
                rows: 0,
                confidence: col.confidence,
            };
        }
        if needle_len == 0 {
            return CardinalityEstimate {
                rows: non_null_rows,
                confidence: col.confidence,
            };
        }
        let Some(th) = &col.text_hist else {
            let fallback = ((non_null_rows as f64) / (1.0 + needle_len as f64 * 0.5)).round()
                as usize;
            return CardinalityEstimate {
                rows: fallback.min(non_null_rows),
                confidence: (col.confidence * 0.5).clamp(0.1, 1.0),
            };
        };
        if needle_len > th.max_len {
            return CardinalityEstimate {
                rows: 0,
                confidence: col.confidence,
            };
        }
        let total_bucket_rows: usize = th.buckets.iter().sum();
        if total_bucket_rows == 0 {
            return CardinalityEstimate {
                rows: 0,
                confidence: col.confidence,
            };
        }
        let mut eligible = 0.0f64;
        for (b, &cnt) in th.buckets.iter().enumerate() {
            if cnt == 0 {
                continue;
            }
            let (lo, hi) = TextLengthHistogram::bucket_bounds(b);
            if hi < needle_len {
                continue;
            }
            if lo >= needle_len {
                eligible += cnt as f64;
            } else {
                let effective_hi = hi.min(th.max_len);
                let span = (effective_hi - lo + 1).max(1) as f64;
                let fit = (effective_hi.saturating_sub(needle_len) + 1) as f64;
                eligible += cnt as f64 * (fit / span).clamp(0.0, 1.0);
            }
        }
        let len_eligible_frac = (eligible / total_bucket_rows as f64).clamp(0.0, 1.0);
        let decay = 1.0 / (1.0 + (needle_len as f64) * 0.35);
        let rows = ((non_null_rows as f64) * len_eligible_frac * decay).round() as usize;
        CardinalityEstimate {
            rows: rows.min(non_null_rows),
            confidence: (col.confidence * 0.8).clamp(0.1, 1.0),
        }
    }

    /// Estimate multi-hop graph frontier expansion with explicit result cap (`LIN-0576`).
    pub fn estimate_graph_expansion(
        &self,
        seed_rows: usize,
        avg_degree: f64,
        depth: usize,
        max_cap: usize,
    ) -> CardinalityEstimate {
        if seed_rows == 0 || depth == 0 || max_cap == 0 || avg_degree <= 0.0 || avg_degree.is_nan()
        {
            return CardinalityEstimate {
                rows: 0,
                confidence: 0.9,
            };
        }
        let mut frontier = seed_rows as f64;
        let mut visited = 0.0f64;
        for _ in 0..depth {
            frontier *= avg_degree;
            visited += frontier;
            if visited >= max_cap as f64 {
                visited = max_cap as f64;
                break;
            }
        }
        let rows = (visited.round() as usize).min(max_cap);
        let confidence = (0.9_f64.powi(depth.min(16) as i32)).clamp(0.1, 1.0);
        CardinalityEstimate { rows, confidence }
    }

    /// Estimate equi-join output cardinality, build-side memory, and side recommendation (`LIN-0577`, `LIN-0578`).
    pub fn estimate_join(
        &self,
        left: &TableStats,
        right: &TableStats,
        left_key: &str,
        right_key: &str,
    ) -> JoinCostEstimate {
        let left_row_bytes = left.avg_row_bytes.max(64);
        let right_row_bytes = right.avg_row_bytes.max(64);
        let left_bytes = left.row_count.saturating_mul(left_row_bytes);
        let right_bytes = right.row_count.saturating_mul(right_row_bytes);
        let recommended_build_right = right_bytes <= left_bytes;
        let build_bytes = if recommended_build_right {
            right_bytes
        } else {
            left_bytes
        };

        if left.row_count == 0 || right.row_count == 0 {
            return JoinCostEstimate {
                output_rows: 0,
                build_bytes,
                recommended_build_right,
                total_cost: (left.row_count + right.row_count) as f64 * self.units.cpu_tuple_cost
                    + build_bytes as f64 * self.units.mem_byte_penalty,
            };
        }

        let (l_non_null, l_ndv) = match left.columns.get(left_key) {
            Some(c) => (
                ((1.0 - c.null_fraction).clamp(0.0, 1.0) * left.row_count as f64),
                c.distinct_estimate.max(1),
            ),
            None => (left.row_count as f64, left.row_count.max(1)),
        };
        let (r_non_null, r_ndv) = match right.columns.get(right_key) {
            Some(c) => (
                ((1.0 - c.null_fraction).clamp(0.0, 1.0) * right.row_count as f64),
                c.distinct_estimate.max(1),
            ),
            None => (right.row_count as f64, right.row_count.max(1)),
        };
        let denom = l_ndv.max(r_ndv).max(1) as f64;
        let output_rows = ((l_non_null * r_non_null) / denom).round() as usize;
        let total_cost = (left.row_count + right.row_count + output_rows) as f64
            * self.units.cpu_tuple_cost
            + build_bytes as f64 * self.units.mem_byte_penalty;
        JoinCostEstimate {
            output_rows,
            build_bytes,
            recommended_build_right,
            total_cost,
        }
    }

    /// Estimate sort working-set bytes, accounting for bounded `Top-K` heaps (`LIN-0579`).
    pub fn estimate_sort_bytes(
        &self,
        rows: usize,
        avg_row_bytes: usize,
        top_k: Option<usize>,
    ) -> usize {
        if rows == 0 {
            return 0;
        }
        let retained = match top_k {
            Some(k) => rows.min(k),
            None => rows,
        };
        let per_entry = avg_row_bytes.max(32).saturating_add(16);
        retained.saturating_mul(per_entry)
    }

    /// Estimate hash-aggregation group count over `group_fields` (`LIN-0580`).
    pub fn estimate_agg_groups(&self, stats: &TableStats, group_fields: &[&str]) -> usize {
        if stats.row_count == 0 {
            return 0;
        }
        if group_fields.is_empty() {
            return 1;
        }
        // Check exact/sampled composite prefix statistics first when available.
        for (label, prefixes) in &stats.composite_prefixes {
            if let Some(open) = label.find('[')
                && label.ends_with(']')
            {
                let idx_fields: Vec<&str> = label[open + 1..label.len() - 1].split(',').collect();
                if idx_fields.len() >= group_fields.len()
                    && idx_fields[..group_fields.len()] == *group_fields
                    && let Some(&est) = prefixes.get(group_fields.len() - 1)
                {
                    return est.clamp(1, stats.row_count);
                }
            }
        }
        let mut product = 1.0f64;
        for (i, &field) in group_fields.iter().enumerate() {
            let field_ndv = match stats.columns.get(field) {
                Some(c) => {
                    let has_null_group = usize::from(c.null_fraction > 0.0);
                    (c.distinct_estimate + has_null_group).max(1)
                }
                None => 1,
            };
            // Mild correlation damping for secondary grouping keys so multi-key NDVs stay realistic.
            let factor = if i == 0 {
                field_ndv as f64
            } else {
                (field_ndv as f64).powf(0.85).max(1.0)
            };
            product *= factor;
            if product >= stats.row_count as f64 {
                return stats.row_count;
            }
        }
        (product.round() as usize).clamp(1, stats.row_count)
    }

    /// Compare estimated vs actual execution rows (`LIN-0581`, `LIN-0589`, `LIN-0590`).
    pub fn compare_estimate_actual(
        &self,
        estimated: usize,
        actual: usize,
    ) -> SelectivityFeedbackReceipt {
        compare_estimate_actual(estimated, actual)
    }

    /// Calibrated CPU vs GPU scan/filter cost comparison (`LIN-0585`, `LIN-0587`).
    pub fn estimate_gpu_scan_cost(&self, rows: usize, transfer_bytes: usize) -> f64 {
        (transfer_bytes as f64) * self.units.gpu_transfer_byte_cost
            + (rows as f64) * (self.units.cpu_tuple_cost * 0.15)
    }

    /// Calibrated cold-disk scan cost (`LIN-0588`).
    pub fn estimate_cold_scan_cost(&self, pages: usize, rows: usize) -> f64 {
        (pages as f64) * self.units.cold_page_cost + (rows as f64) * self.units.cpu_tuple_cost
    }

    fn estimate_pred_selectivity(&self, stats: &TableStats, pred: &Pred) -> (f64, f64) {
        let total = stats.row_count.max(1) as f64;
        match pred {
            Pred::Cmp { field, op, value } => {
                let est = self.estimate_index_seek(stats, field.as_cow_str().as_ref(), *op, value);
                (est.rows as f64 / total, est.confidence)
            }
            Pred::IsNull { field, negated } => {
                let fname = field.as_cow_str();
                match stats.columns.get(fname.as_ref()) {
                    Some(col) => {
                        let frac = if *negated {
                            (1.0 - col.null_fraction).clamp(0.0, 1.0)
                        } else {
                            col.null_fraction.clamp(0.0, 1.0)
                        };
                        (frac, col.confidence)
                    }
                    None => (if *negated { 0.0 } else { 1.0 }, 0.8),
                }
            }
            Pred::In { field, values } => {
                let fname = field.as_cow_str();
                let mut sum_rows = 0usize;
                let mut conf = 1.0f64;
                let mut seen = Vec::with_capacity(values.len().min(32));
                for v in values {
                    if seen.len() < 64 && seen.contains(&v) {
                        continue;
                    }
                    if seen.len() < 64 {
                        seen.push(v);
                    }
                    let est = self.estimate_index_seek(stats, fname.as_ref(), CmpOp::Eq, v);
                    sum_rows = sum_rows.saturating_add(est.rows);
                    conf = conf.min(est.confidence);
                }
                let max_non_null = stats
                    .columns
                    .get(fname.as_ref())
                    .map(|c| ((1.0 - c.null_fraction) * total).round() as usize)
                    .unwrap_or(stats.row_count);
                (sum_rows.min(max_non_null) as f64 / total, conf)
            }
            Pred::Contains { field, needle } | Pred::Has { field, needle, .. } => {
                let est =
                    self.estimate_substring(stats, field.as_cow_str().as_ref(), needle.len());
                (est.rows as f64 / total, est.confidence)
            }
            Pred::Regex { field, .. } => {
                let fname = field.as_cow_str();
                let non_null_frac = stats
                    .columns
                    .get(fname.as_ref())
                    .map(|c| (1.0 - c.null_fraction).clamp(0.0, 1.0))
                    .unwrap_or(0.0);
                (non_null_frac * 0.15, 0.4)
            }
            Pred::Not(inner) => {
                let (s, c) = self.estimate_pred_selectivity(stats, inner);
                ((1.0 - s).clamp(0.0, 1.0), c)
            }
            Pred::And(a, b) => {
                // Same-field range intersection: `x >= lo and x <= hi`
                if let Some((sel, conf)) = self.try_same_field_and_selectivity(stats, a, b) {
                    return (sel, conf);
                }
                // Composite-prefix correlation awareness (`LIN-0583`): if `a` and `b` are
                // equality predicates covered by a composite index prefix, use its joint NDV!
                if let Some((sel, conf)) = self.try_composite_eq_selectivity(stats, pred) {
                    return (sel, conf);
                }
                let (sa, ca) = self.estimate_pred_selectivity(stats, a);
                let (sb, cb) = self.estimate_pred_selectivity(stats, b);
                if sa <= 0.0 || sb <= 0.0 {
                    return (0.0, ca.min(cb));
                }
                // Exponential backoff (`s_min * sqrt(s_max)`) resists catastrophic underestimation
                // on positively correlated attributes (`LIN-0583`) while staying <= min(sa, sb).
                let (s_min, s_max) = if sa <= sb { (sa, sb) } else { (sb, sa) };
                let combined = (s_min * s_max.sqrt()).clamp(0.0, s_min);
                (combined, (ca.min(cb) * 0.9).clamp(0.1, 1.0))
            }
            Pred::Or(a, b) => {
                if a == b {
                    return self.estimate_pred_selectivity(stats, a);
                }
                let (sa, ca) = self.estimate_pred_selectivity(stats, a);
                let (sb, cb) = self.estimate_pred_selectivity(stats, b);
                let union = (sa + sb - sa * sb).clamp(0.0, 1.0);
                (union, ca.min(cb))
            }
        }
    }

    fn try_same_field_and_selectivity(
        &self,
        stats: &TableStats,
        a: &Pred,
        b: &Pred,
    ) -> Option<(f64, f64)> {
        let (
            Pred::Cmp {
                field: fa,
                op: opa,
                value: va,
            },
            Pred::Cmp {
                field: fb,
                op: opb,
                value: vb,
            },
        ) = (a, b)
        else {
            return None;
        };
        if fa != fb {
            return None;
        }
        let fname = fa.as_cow_str();
        let col = stats.columns.get(fname.as_ref())?;
        let hist = col.numeric_hist.as_ref()?;
        let na = value_as_f64(va)?;
        let nb = value_as_f64(vb)?;
        if na.is_nan() || nb.is_nan() {
            return Some((0.0, col.confidence));
        }
        if *opa == CmpOp::Eq && *opb == CmpOp::Eq {
            if (na - nb).abs() > f64::EPSILON {
                return Some((0.0, col.confidence));
            }
            let est = self.estimate_index_seek(stats, fname.as_ref(), CmpOp::Eq, va);
            return Some((est.rows as f64 / stats.row_count.max(1) as f64, est.confidence));
        }
        let lo_bound = match (opa, opb) {
            (CmpOp::Gt | CmpOp::Ge, CmpOp::Lt | CmpOp::Le) => Some((*opa, na, *opb, nb)),
            (CmpOp::Lt | CmpOp::Le, CmpOp::Gt | CmpOp::Ge) => Some((*opb, nb, *opa, na)),
            _ => None,
        }?;
        let (lo_op, lo_v, hi_op, hi_v) = lo_bound;
        if lo_v > hi_v || (lo_v == hi_v && (lo_op == CmpOp::Gt || hi_op == CmpOp::Lt)) {
            return Some((0.0, col.confidence));
        }
        let non_null_frac = (1.0 - col.null_fraction).clamp(0.0, 1.0);
        let cdf_hi = hist.estimate_range_fraction(hi_op, hi_v);
        let cdf_lo = 1.0 - hist.estimate_range_fraction(lo_op, lo_v);
        let interval = (cdf_hi - cdf_lo).clamp(0.0, 1.0);
        Some((non_null_frac * interval, col.confidence))
    }

    fn try_composite_eq_selectivity(
        &self,
        stats: &TableStats,
        pred: &Pred,
    ) -> Option<(f64, f64)> {
        if stats.composite_prefixes.is_empty() {
            return None;
        }
        let mut eq_fields: Vec<(&str, &Value)> = Vec::new();
        collect_eq_conjuncts(pred, &mut eq_fields)?;
        if eq_fields.len() < 2 {
            return None;
        }
        for (label, prefixes) in &stats.composite_prefixes {
            let Some(open) = label.find('[') else {
                continue;
            };
            if !label.ends_with(']') {
                continue;
            }
            let idx_fields: Vec<&str> = label[open + 1..label.len() - 1].split(',').collect();
            if idx_fields.len() < eq_fields.len() {
                continue;
            }
            let prefix_slice = &idx_fields[..eq_fields.len()];
            if prefix_slice
                .iter()
                .all(|f| eq_fields.iter().any(|(ef, _)| ef == f))
            {
                // Check exact heavy-hitter match on full composite key when fanout histogram exists.
                if idx_fields.len() == eq_fields.len()
                    && let Some(fanout) = stats.index_fanouts.get(label)
                {
                    let mut composite_key = String::new();
                    let mut all_encodable = true;
                    for (i, f) in idx_fields.iter().enumerate() {
                        if i > 0 {
                            composite_key.push('\x1f');
                        }
                        let v = eq_fields.iter().find(|(ef, _)| ef == f).unwrap().1;
                        if let Some(vk) = canonical_value_key(v) {
                            composite_key.push_str(&vk);
                        } else {
                            all_encodable = false;
                            break;
                        }
                    }
                    if all_encodable
                        && stats.sampled_rows > 0
                        && let Some(&(_, cnt)) =
                            fanout.heavy_hitters.iter().find(|(k, _)| k == &composite_key)
                    {
                        return Some((cnt as f64 / stats.sampled_rows as f64, 0.95));
                    }
                }
                let joint_ndv = (*prefixes.get(eq_fields.len() - 1)?).max(1) as f64;
                let min_non_null = eq_fields
                    .iter()
                    .filter_map(|(f, _)| stats.columns.get(*f))
                    .map(|c| (1.0 - c.null_fraction).clamp(0.0, 1.0))
                    .fold(1.0f64, f64::min);
                return Some(((min_non_null / joint_ndv).clamp(0.0, 1.0), 0.9));
            }
        }
        None
    }
}

fn collect_eq_conjuncts<'a>(pred: &'a Pred, out: &mut Vec<(&'a str, &'a Value)>) -> Option<()> {
    match pred {
        Pred::And(a, b) => {
            collect_eq_conjuncts(a, out)?;
            collect_eq_conjuncts(b, out)
        }
        Pred::Cmp {
            field,
            op: CmpOp::Eq,
            value,
        } => {
            let leaf = field.leaf()?;
            out.push((leaf, value));
            Some(())
        }
        _ => None,
    }
}

fn sample_indices(total: usize, sample_n: usize) -> impl Iterator<Item = usize> {
    let take_n = sample_n.min(total);
    (0..take_n).map(move |i| {
        if take_n == total {
            i
        } else {
            ((i as u128 * total as u128) / take_n as u128) as usize
        }
    })
}

struct ColAccum {
    non_null: usize,
    distinct_counts: HashMap<String, usize>,
    num_min: f64,
    num_max: f64,
    num_nan: usize,
    num_vals: Vec<f64>,
    has_num: bool,
    text_min: usize,
    text_max: usize,
    text_sum: usize,
    text_cnt: usize,
    text_buckets: [usize; 8],
    vec_min: usize,
    vec_max: usize,
    vec_cnt: usize,
}

impl ColAccum {
    fn new() -> Self {
        Self {
            non_null: 0,
            distinct_counts: HashMap::new(),
            num_min: f64::INFINITY,
            num_max: f64::NEG_INFINITY,
            num_nan: 0,
            num_vals: Vec::new(),
            has_num: false,
            text_min: usize::MAX,
            text_max: 0,
            text_sum: 0,
            text_cnt: 0,
            text_buckets: [0; 8],
            vec_min: usize::MAX,
            vec_max: 0,
            vec_cnt: 0,
        }
    }

    fn observe(&mut self, cell: &Cell) {
        if cell.is_null() {
            return;
        }
        self.non_null += 1;
        let mut key = String::with_capacity(24);
        append_canonical_cell(&mut key, cell);
        *self.distinct_counts.entry(key).or_insert(0) += 1;

        match cell {
            Cell::Int(n) => self.observe_f64(*n as f64),
            Cell::Time(t) => self.observe_f64(*t as f64),
            Cell::Float(f) => self.observe_f64(*f),
            Cell::Text(s) => self.observe_str(s.as_ref()),
            Cell::SmallText(s) => self.observe_str(s.as_str()),
            Cell::Vec(v) => {
                let d = v.len();
                self.vec_min = self.vec_min.min(d);
                self.vec_max = self.vec_max.max(d);
                self.vec_cnt += 1;
            }
            Cell::Bool(_) | Cell::Null => {}
        }
    }

    fn observe_f64(&mut self, v: f64) {
        self.has_num = true;
        if v.is_nan() {
            self.num_nan += 1;
            return;
        }
        if v.is_finite() {
            self.num_min = self.num_min.min(v);
            self.num_max = self.num_max.max(v);
            self.num_vals.push(v);
        }
    }

    fn observe_str(&mut self, s: &str) {
        let len = s.len();
        self.text_min = self.text_min.min(len);
        self.text_max = self.text_max.max(len);
        self.text_sum = self.text_sum.saturating_add(len);
        self.text_cnt += 1;
        let b = TextLengthHistogram::bucket_for_len(len);
        self.text_buckets[b] += 1;
    }
}

fn compute_table_stats(
    rows: &[Row],
    sample_limit: Option<usize>,
    analyzed_at_gen: u64,
) -> TableStats {
    let row_count = rows.len();
    let sampled_rows = match sample_limit {
        Some(limit) => limit.min(row_count),
        None => row_count,
    };
    if row_count == 0 || sampled_rows == 0 {
        return TableStats {
            row_count,
            sampled_rows,
            avg_row_bytes: 0,
            mutations_since_analyze: 0,
            analyzed_at_gen,
            columns: BTreeMap::new(),
            index_fanouts: BTreeMap::new(),
            composite_prefixes: BTreeMap::new(),
        };
    }

    let confidence = if sampled_rows == row_count {
        1.0
    } else {
        (0.1 + 0.9 * (sampled_rows as f64 / row_count as f64).sqrt()).clamp(0.1, 0.99)
    };

    let mut accums: BTreeMap<String, ColAccum> = BTreeMap::new();
    let mut total_bytes = 0usize;

    for idx in sample_indices(row_count, sampled_rows) {
        let row = &rows[idx];
        total_bytes = total_bytes.saturating_add(row_byte_size(row));
        for (k, v) in row {
            accums
                .entry(k.clone())
                .or_insert_with(ColAccum::new)
                .observe(v);
        }
    }

    let avg_row_bytes = (total_bytes / sampled_rows).max(1);
    let mut columns = BTreeMap::new();
    let mut index_fanouts = BTreeMap::new();

    for (field, acc) in accums {
        let null_count = sampled_rows.saturating_sub(acc.non_null);
        let null_fraction = (null_count as f64 / sampled_rows as f64).clamp(0.0, 1.0);
        let distinct_estimate = scale_distinct_count(&acc.distinct_counts, sampled_rows, row_count);

        let numeric_hist = if acc.has_num {
            let mut buckets = [0usize; 16];
            let (min, max) = if acc.num_vals.is_empty() {
                (0.0, 0.0)
            } else {
                (acc.num_min, acc.num_max)
            };
            let mut hist = NumericRangeHistogram {
                min,
                max,
                nan_count: acc.num_nan,
                buckets: [0; 16],
            };
            for &v in &acc.num_vals {
                let b = hist.bucket_index(v);
                buckets[b] += 1;
            }
            hist.buckets = buckets;
            Some(hist)
        } else {
            None
        };

        let text_hist = if acc.text_cnt > 0 {
            Some(TextLengthHistogram {
                min_len: acc.text_min,
                max_len: acc.text_max,
                avg_len: acc.text_sum as f64 / acc.text_cnt as f64,
                buckets: acc.text_buckets,
            })
        } else {
            None
        };

        let vec_stats = if acc.vec_cnt > 0 {
            Some(VectorDimStats {
                min_dim: acc.vec_min,
                max_dim: acc.vec_max,
                non_null_vecs: acc.vec_cnt,
            })
        } else {
            None
        };

        let mut sorted_freqs: Vec<(&String, &usize)> = acc.distinct_counts.iter().collect();
        sorted_freqs.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
        let mcv: Vec<(String, f64)> = sorted_freqs
            .iter()
            .take(MAX_MCV_ENTRIES)
            .map(|(k, c)| ((*k).clone(), **c as f64 / sampled_rows as f64))
            .collect();

        let fanout = build_fanout_histogram(&acc.distinct_counts, sampled_rows);
        index_fanouts.insert(field.clone(), fanout);

        columns.insert(
            field,
            ColumnStats {
                non_null_count: if sampled_rows == row_count {
                    acc.non_null
                } else {
                    ((acc.non_null as f64 / sampled_rows as f64) * row_count as f64).round()
                        as usize
                },
                null_fraction,
                distinct_estimate,
                confidence,
                numeric_hist,
                text_hist,
                vec_stats,
                mcv,
            },
        );
    }

    TableStats {
        row_count,
        sampled_rows,
        avg_row_bytes,
        mutations_since_analyze: 0,
        analyzed_at_gen,
        columns,
        index_fanouts,
        composite_prefixes: BTreeMap::new(),
    }
}

fn scale_distinct_count(
    counts: &HashMap<String, usize>,
    sampled_rows: usize,
    row_count: usize,
) -> usize {
    let d_sample = counts.len();
    if d_sample == 0 {
        return 0;
    }
    if sampled_rows >= row_count {
        return d_sample;
    }
    let non_null_sample: usize = counts.values().sum();
    if non_null_sample == 0 {
        return 0;
    }
    let singletons = counts.values().filter(|&&c| c == 1).count();
    let singleton_ratio = singletons as f64 / non_null_sample as f64;
    let est_non_null_total =
        ((non_null_sample as f64 / sampled_rows as f64) * row_count as f64).round() as usize;
    let unseen = est_non_null_total.saturating_sub(non_null_sample) as f64;
    let est = (d_sample as f64 + singleton_ratio * unseen).round() as usize;
    est.clamp(d_sample, est_non_null_total.max(d_sample))
}

fn build_fanout_histogram(
    counts: &HashMap<String, usize>,
    sampled_rows: usize,
) -> PostingFanoutHistogram {
    if counts.is_empty() {
        return PostingFanoutHistogram {
            distinct_keys: 0,
            total_postings: 0,
            min_fanout: 0,
            max_fanout: 0,
            avg_fanout: 0.0,
            max_key_fraction: 0.0,
            buckets: [0; 8],
            heavy_hitters: Vec::new(),
        };
    }
    let mut min_f = usize::MAX;
    let mut max_f = 0usize;
    let mut total = 0usize;
    let mut buckets = [0usize; 8];
    let mut entries: Vec<(&String, usize)> = Vec::with_capacity(counts.len());
    for (k, &c) in counts {
        min_f = min_f.min(c);
        max_f = max_f.max(c);
        total = total.saturating_add(c);
        buckets[PostingFanoutHistogram::bucket_for_fanout(c)] += 1;
        entries.push((k, c));
    }
    entries.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
    let heavy_hitters = entries
        .into_iter()
        .take(MAX_MCV_ENTRIES)
        .map(|(k, c)| (k.clone(), c))
        .collect();
    let denom = sampled_rows.max(total).max(1) as f64;
    PostingFanoutHistogram {
        distinct_keys: counts.len(),
        total_postings: total,
        min_fanout: min_f,
        max_fanout: max_f,
        avg_fanout: total as f64 / counts.len() as f64,
        max_key_fraction: (max_f as f64 / denom).clamp(0.0, 1.0),
        buckets,
        heavy_hitters,
    }
}

fn append_canonical_cell(out: &mut String, cell: &Cell) {
    use std::fmt::Write as _;
    match cell {
        Cell::Null => out.push_str("null"),
        Cell::Bool(b) => out.push_str(if *b { "b:1" } else { "b:0" }),
        Cell::Int(n) => {
            let _ = write!(out, "n:{n}");
        }
        Cell::Float(f) => {
            if f.is_nan() {
                out.push_str("f:nan");
            } else if f.is_finite()
                && f.fract() == 0.0
                && *f >= (i64::MIN as f64)
                && *f < (i64::MAX as f64)
            {
                let _ = write!(out, "n:{}", *f as i64);
            } else {
                let bits = if *f == 0.0 { 0u64 } else { f.to_bits() };
                let _ = write!(out, "f:{bits:016x}");
            }
        }
        Cell::Time(ms) => {
            let _ = write!(out, "t:{ms}");
        }
        Cell::Text(s) => {
            out.push_str("s:");
            if s.len() <= 96 {
                out.push_str(s);
            } else {
                out.push_str(&s[..s.floor_char_boundary(64)]);
                let _ = write!(out, "#{}", s.len());
            }
        }
        Cell::SmallText(s) => {
            out.push_str("s:");
            out.push_str(s.as_str());
        }
        Cell::Vec(v) => {
            let _ = write!(out, "v:{}", v.len());
            for x in v.iter().take(4) {
                let bits = if *x == 0.0 { 0u32 } else { x.to_bits() };
                let _ = write!(out, ":{bits:08x}");
            }
        }
    }
}

fn canonical_value_key(val: &Value) -> Option<String> {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(24);
    match val {
        Value::Bool(b) => out.push_str(if *b { "b:1" } else { "b:0" }),
        Value::Int(n) => {
            let _ = write!(out, "n:{n}");
        }
        Value::Float(f) => {
            if f.is_nan() {
                out.push_str("f:nan");
            } else if f.is_finite()
                && f.fract() == 0.0
                && *f >= (i64::MIN as f64)
                && *f < (i64::MAX as f64)
            {
                let _ = write!(out, "n:{}", *f as i64);
            } else {
                let bits = if *f == 0.0 { 0u64 } else { f.to_bits() };
                let _ = write!(out, "f:{bits:016x}");
            }
        }
        Value::Timestamp(ms) => {
            let _ = write!(out, "t:{ms}");
        }
        Value::Duration(d) => {
            let _ = write!(out, "n:{}", d.as_millis());
        }
        Value::String(s) => {
            out.push_str("s:");
            if s.len() <= 96 {
                out.push_str(s);
            } else {
                out.push_str(&s[..s.floor_char_boundary(64)]);
                let _ = write!(out, "#{}", s.len());
            }
        }
        Value::Name(s) => {
            out.push_str("s:");
            out.push_str(s);
        }
        Value::Now | Value::NowMinus(_) | Value::Param(_) => return None,
    }
    Some(out)
}

fn value_as_f64(val: &Value) -> Option<f64> {
    match val {
        Value::Int(n) => Some(*n as f64),
        Value::Float(f) => Some(*f),
        Value::Timestamp(ms) => Some(*ms as f64),
        Value::Duration(d) => Some(d.as_millis() as f64),
        _ => None,
    }
}
