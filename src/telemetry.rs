//! Structured runtime telemetry, latency histograms, slow-query ring buffer, and Prometheus/OTel exporters (`D41`: `LIN-1601`..`LIN-1640`, `D35`: `LIN-1385`..`LIN-1386`).

use std::collections::VecDeque;
use std::fmt::Write as _;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};

use rustc_hash::{FxHashMap, FxHashSet};
use serde::{Deserialize, Serialize};

use crate::error::{Error, redact_credentials};

/// Schema version for exported telemetry snapshots, OTel attributes, and Prometheus series (`LIN-1637`).
pub const OBSERVABILITY_SCHEMA_VERSION: u32 = 1;

static NEXT_QUERY_EXEC_ID: AtomicU64 = AtomicU64::new(1);

/// Lock-free monotonically increasing per-query execution identifier (`LIN-1601`).
#[inline]
pub fn next_query_execution_id() -> u64 {
    NEXT_QUERY_EXEC_ID.fetch_add(1, Ordering::Relaxed)
}

/// Deterministic 64-bit FNV-1a plan fingerprint for cross-execution correlation (`LIN-1624`).
pub fn compute_plan_hash(plan_repr: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in plan_repr.as_bytes() {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash
}

// ---------------------------------------------------------------------------
// Fixed-bucket Atomic Latency Histogram (`LIN-1605`..`LIN-1608`)
// ---------------------------------------------------------------------------

/// Number of log-scale buckets in [`LatencyHistogram`]: 23 finite upper bounds (`1 µs` .. `8 s`) + 1 overflow (`> 8 s`).
pub const HISTOGRAM_NUM_BUCKETS: usize = 24;

/// Upper bounds in nanoseconds for the first 23 buckets; bucket 23 holds `> 8_000_000_000 ns` (`+Inf`).
pub const HISTOGRAM_UPPER_BOUNDS_NS: [u64; HISTOGRAM_NUM_BUCKETS - 1] = [
    1_000,         // <= 1 µs
    2_000,         // <= 2 µs
    4_000,         // <= 4 µs
    8_000,         // <= 8 µs
    16_000,        // <= 16 µs
    32_000,        // <= 32 µs
    64_000,        // <= 64 µs
    128_000,       // <= 128 µs
    256_000,       // <= 256 µs
    512_000,       // <= 512 µs
    1_000_000,     // <= 1 ms
    2_000_000,     // <= 2 ms
    4_000_000,     // <= 4 ms
    8_000_000,     // <= 8 ms
    16_000_000,    // <= 16 ms
    32_000_000,    // <= 32 ms
    64_000_000,    // <= 64 ms
    128_000_000,   // <= 128 ms
    256_000_000,   // <= 256 ms
    512_000_000,   // <= 512 ms
    1_000_000_000, // <= 1 s
    4_000_000_000, // <= 4 s
    8_000_000_000, // <= 8 s
];

/// Lock-free, zero-allocation fixed-bucket latency histogram.
#[derive(Debug)]
pub struct LatencyHistogram {
    buckets: [AtomicU64; HISTOGRAM_NUM_BUCKETS],
    count: AtomicU64,
    sum_ns: AtomicU64,
    min_ns: AtomicU64,
    max_ns: AtomicU64,
}

impl Default for LatencyHistogram {
    fn default() -> Self {
        Self::new()
    }
}

impl LatencyHistogram {
    pub const fn new() -> Self {
        Self {
            buckets: [
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
            ],
            count: AtomicU64::new(0),
            sum_ns: AtomicU64::new(0),
            min_ns: AtomicU64::new(u64::MAX),
            max_ns: AtomicU64::new(0),
        }
    }

    #[inline]
    fn bucket_index(ns: u64) -> usize {
        match HISTOGRAM_UPPER_BOUNDS_NS.binary_search(&ns) {
            Ok(idx) | Err(idx) => idx,
        }
    }

    /// Records a single observation in nanoseconds with zero heap allocations.
    #[inline]
    pub fn record_ns(&self, ns: u64) {
        let idx = Self::bucket_index(ns);
        self.buckets[idx].fetch_add(1, Ordering::Relaxed);
        self.count.fetch_add(1, Ordering::Relaxed);
        self.sum_ns.fetch_add(ns, Ordering::Relaxed);
        self.min_ns.fetch_min(ns, Ordering::Relaxed);
        self.max_ns.fetch_max(ns, Ordering::Relaxed);
    }

    #[inline]
    pub fn count(&self) -> u64 {
        self.count.load(Ordering::Relaxed)
    }

    #[inline]
    pub fn sum_ns(&self) -> u64 {
        self.sum_ns.load(Ordering::Relaxed)
    }

    #[inline]
    pub fn min_ns(&self) -> u64 {
        let min = self.min_ns.load(Ordering::Relaxed);
        if min == u64::MAX { 0 } else { min }
    }

    #[inline]
    pub fn max_ns(&self) -> u64 {
        self.max_ns.load(Ordering::Relaxed)
    }

    pub fn bucket_counts(&self) -> [u64; HISTOGRAM_NUM_BUCKETS] {
        let mut out = [0u64; HISTOGRAM_NUM_BUCKETS];
        for (i, slot) in self.buckets.iter().enumerate() {
            out[i] = slot.load(Ordering::Relaxed);
        }
        out
    }

    /// Estimates the `q` quantile (`0.0..=1.0`) in nanoseconds using linear interpolation inside the target bucket,
    /// clamped to `[min_ns, max_ns]`.
    pub fn quantile_ns(&self, q: f64) -> u64 {
        let counts = self.bucket_counts();
        let total: u64 = counts.iter().sum();
        if total == 0 {
            return 0;
        }
        let min = self.min_ns.load(Ordering::Relaxed);
        let max = self.max_ns.load(Ordering::Relaxed);
        let clamped_q = q.clamp(0.0, 1.0);
        if clamped_q <= 0.0 {
            return if min == u64::MAX { 0 } else { min };
        }
        if clamped_q >= 1.0 {
            return max;
        }

        let rank_f = clamped_q * (total as f64);
        let target = (rank_f.ceil() as u64).max(1);
        let mut cumulative = 0u64;
        for (idx, &bucket_count) in counts.iter().enumerate() {
            let prev_cum = cumulative;
            cumulative += bucket_count;
            if cumulative >= target && bucket_count > 0 {
                let lower = if idx == 0 {
                    0
                } else {
                    HISTOGRAM_UPPER_BOUNDS_NS[idx - 1]
                };
                let upper = if idx < HISTOGRAM_UPPER_BOUNDS_NS.len() {
                    HISTOGRAM_UPPER_BOUNDS_NS[idx]
                } else {
                    max.max(lower)
                };
                let frac = ((rank_f - (prev_cum as f64)) / (bucket_count as f64)).clamp(0.0, 1.0);
                let est = lower + (((upper.saturating_sub(lower)) as f64) * frac).round() as u64;
                let lo = if min == u64::MAX { 0 } else { min };
                let hi = max.max(lo);
                return est.clamp(lo, hi);
            }
        }
        max
    }

    #[inline]
    pub fn p50_ns(&self) -> u64 {
        self.quantile_ns(0.50)
    }

    #[inline]
    pub fn p95_ns(&self) -> u64 {
        self.quantile_ns(0.95)
    }

    #[inline]
    pub fn p99_ns(&self) -> u64 {
        self.quantile_ns(0.99)
    }

    pub fn reset(&self) {
        for slot in &self.buckets {
            slot.store(0, Ordering::Relaxed);
        }
        self.count.store(0, Ordering::Relaxed);
        self.sum_ns.store(0, Ordering::Relaxed);
        self.min_ns.store(u64::MAX, Ordering::Relaxed);
        self.max_ns.store(0, Ordering::Relaxed);
    }
}

// ---------------------------------------------------------------------------
// Query Phase Timing Schema & Anomaly Detection (`LIN-1602`..`LIN-1604`, `LIN-1623`, `LIN-1624`)
// ---------------------------------------------------------------------------

/// Per-execution phase timing, operator row/byte counters, redacted query text, and anomaly hint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct QueryPhaseRecord {
    pub exec_id: u64,
    pub plan_hash: u64,
    pub prepare_ns: u64,
    pub execute_ns: u64,
    pub commit_ns: u64,
    pub queue_ns: u64,
    pub rows_in: u64,
    pub rows_out: u64,
    pub bytes_allocated: u64,
    pub redacted_query: String,
    pub anomaly_hint: Option<&'static str>,
}

impl QueryPhaseRecord {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        exec_id: u64,
        plan_hash: u64,
        prepare_ns: u64,
        execute_ns: u64,
        commit_ns: u64,
        queue_ns: u64,
        rows_in: u64,
        rows_out: u64,
        bytes_allocated: u64,
        raw_or_redacted_query: &str,
    ) -> Self {
        let mut rec = Self {
            exec_id,
            plan_hash,
            prepare_ns,
            execute_ns,
            commit_ns,
            queue_ns,
            rows_in,
            rows_out,
            bytes_allocated,
            redacted_query: redact_query_text(raw_or_redacted_query),
            anomaly_hint: None,
        };
        rec.anomaly_hint = rec.detect_phase_anomaly();
        rec
    }

    #[inline]
    pub fn total_ns(&self) -> u64 {
        self.prepare_ns
            .saturating_add(self.execute_ns)
            .saturating_add(self.commit_ns)
            .saturating_add(self.queue_ns)
    }

    /// Detects dominant latency skew, unselective scans, or high allocation pressure (`LIN-1623`).
    pub fn detect_phase_anomaly(&self) -> Option<&'static str> {
        if self.queue_ns >= 5_000_000 && self.queue_ns > self.execute_ns.saturating_mul(2) {
            return Some("queue_contention: admission/lock wait dominates execution phase");
        }
        if self.commit_ns >= 10_000_000 && self.commit_ns > self.execute_ns.saturating_mul(2) {
            return Some("commit_stall: WAL fsync/commit latency dominates execution phase");
        }
        if self.prepare_ns >= 5_000_000 && self.prepare_ns > self.execute_ns.saturating_mul(2) {
            return Some("prepare_overhead: parse/compile dominates execution; cache `Prepared`");
        }
        if self.rows_in >= 10_000
            && self.rows_out <= 10
            && self.rows_in > self.rows_out.saturating_mul(100)
        {
            return Some("unselective_scan: high rows_in/rows_out ratio; add secondary index");
        }
        if self.bytes_allocated >= 64 * 1024 * 1024 {
            return Some("high_memory_allocation: query allocated >= 64 MiB");
        }
        if self.execute_ns >= 500_000_000 {
            return Some("slow_execution: execution phase exceeded 500 ms");
        }
        None
    }
}

// ---------------------------------------------------------------------------
// Query Literal Redaction & Cardinality Guardrails (`LIN-1618`..`LIN-1620`)
// ---------------------------------------------------------------------------

#[inline]
fn is_ident_continue(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Replaces string literals, numeric literals, duration literals, and credentials with `?`
/// so PII and secrets never enter telemetry buffers or span attributes (`LIN-1619`).
pub fn redact_query_text(q: &str) -> String {
    let bytes = q.as_bytes();
    let mut out = String::with_capacity(bytes.len());
    let mut i = 0;

    while i < bytes.len() {
        let b = bytes[i];

        // Double-quoted or single-quoted string literal -> `?`
        if b == b'"' || b == b'\'' {
            let quote = b;
            i += 1;
            while i < bytes.len() {
                if bytes[i] == b'\\' {
                    i = (i + 2).min(bytes.len());
                    continue;
                }
                if bytes[i] == quote {
                    if i + 1 < bytes.len() && bytes[i + 1] == quote {
                        i += 2;
                        continue;
                    }
                    i += 1;
                    break;
                }
                let ch = q[i..].chars().next().unwrap();
                i += ch.len_utf8();
            }
            out.push('?');
            continue;
        }

        // Negative numeric literal `-42` when `-` is not preceded by an identifier/number/closing paren
        if b == b'-' && i + 1 < bytes.len() && bytes[i + 1].is_ascii_digit() {
            let prev_non_ws = out.as_bytes().iter().rev().copied().find(|c| !c.is_ascii_whitespace());
            let is_binary_minus = matches!(
                prev_non_ws,
                Some(c) if c.is_ascii_alphanumeric() || c == b'_' || c == b')' || c == b']' || c == b'?'
            );
            if !is_binary_minus {
                i += 1; // consume leading unary `-` as part of numeric literal
            } else {
                out.push('-');
                i += 1;
                continue;
            }
        }

        // Numeric / hex / float / duration literal (only when NOT preceded by identifier char)
        if bytes[i].is_ascii_digit() {
            let prev_is_ident = i > 0 && is_ident_continue(bytes[i - 1]);
            if !prev_is_ident {
                if bytes[i] == b'0'
                    && i + 1 < bytes.len()
                    && (bytes[i + 1] == b'x' || bytes[i + 1] == b'X')
                {
                    i += 2;
                    while i < bytes.len() && (bytes[i].is_ascii_hexdigit() || bytes[i] == b'_') {
                        i += 1;
                    }
                } else {
                    while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'_') {
                        i += 1;
                    }
                    if i + 1 < bytes.len() && bytes[i] == b'.' && bytes[i + 1].is_ascii_digit() {
                        i += 2;
                        while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'_') {
                            i += 1;
                        }
                    }
                    if i < bytes.len() && (bytes[i] == b'e' || bytes[i] == b'E') {
                        let mut exp = i + 1;
                        if exp < bytes.len() && (bytes[exp] == b'+' || bytes[exp] == b'-') {
                            exp += 1;
                        }
                        if exp < bytes.len() && bytes[exp].is_ascii_digit() {
                            i = exp + 1;
                            while i < bytes.len() && bytes[i].is_ascii_digit() {
                                i += 1;
                            }
                        }
                    }
                    // Consume trailing duration unit (`7d`, `15m`, `100ms`, `50us`, `10ns`)
                    if i < bytes.len() && bytes[i].is_ascii_alphabetic() {
                        let unit_start = i;
                        while i < bytes.len() && is_ident_continue(bytes[i]) {
                            i += 1;
                        }
                        let unit = &q[unit_start..i];
                        if !matches!(unit, "ns" | "us" | "ms" | "s" | "m" | "h" | "d" | "w") {
                            i = unit_start;
                        }
                    }
                }
                out.push('?');
                continue;
            }
        }

        // `$param` parameter placeholders remain `$param`
        let ch = q[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }

    redact_credentials(&out)
}

const FORBIDDEN_HIGH_CARDINALITY_KEYS: &[&str] = &[
    "query_id",
    "exec_id",
    "execution_id",
    "user_id",
    "row_id",
    "doc_id",
    "entity_id",
    "request_id",
    "trace_id",
    "span_id",
    "session_id",
    "client_ip",
    "ip",
    "email",
    "uuid",
    "query",
    "raw_query",
    "sql",
    "statement",
    "token",
    "password",
    "secret",
    "timestamp",
    "ts",
];

fn looks_like_uuid_or_hex_id(val: &str) -> bool {
    let b = val.as_bytes();
    // Standard 36-char UUID 8-4-4-4-12
    if b.len() == 36
        && b[8] == b'-'
        && b[13] == b'-'
        && b[18] == b'-'
        && b[23] == b'-'
        && b.iter()
            .enumerate()
            .all(|(idx, &ch)| matches!(idx, 8 | 13 | 18 | 23) || ch.is_ascii_hexdigit())
    {
        return true;
    }
    // 16+ consecutive hex characters (hash/trace/object IDs)
    let mut hex_run = 0usize;
    for &ch in b {
        if ch.is_ascii_hexdigit() {
            hex_run += 1;
            if hex_run >= 16 {
                return true;
            }
        } else {
            hex_run = 0;
        }
    }
    false
}

/// Validates metric label key/value pairs and rejects unbounded high-cardinality dimensions (`LIN-1618`, `LIN-1620`).
pub fn validate_metric_label(key: &str, val: &str) -> Result<(), Error> {
    if key.is_empty() || key.len() > 64 {
        return Err(Error::resource_limit(
            "metric label key must be 1..=64 ASCII characters",
        ));
    }
    if key.starts_with("__") {
        return Err(Error::permission_denied(format!(
            "metric label key `{key}` uses reserved Prometheus `__` prefix"
        )));
    }
    let kb = key.as_bytes();
    if !(kb[0].is_ascii_alphabetic() || kb[0] == b'_')
        || !kb[1..].iter().all(|&c| c.is_ascii_alphanumeric() || c == b'_')
    {
        return Err(Error::resource_limit(format!(
            "invalid metric label key `{key}`: must match [a-zA-Z_][a-zA-Z0-9_]*"
        )));
    }
    if FORBIDDEN_HIGH_CARDINALITY_KEYS
        .iter()
        .any(|k| key.eq_ignore_ascii_case(k))
    {
        return Err(Error::resource_limit(format!(
            "high-cardinality metric label key `{key}` rejected"
        )));
    }

    if val.is_empty() || val.len() > 64 {
        return Err(Error::resource_limit(
            "metric label value must be 1..=64 bytes",
        ));
    }
    if val.chars().any(|c| c.is_control()) {
        return Err(Error::resource_limit(
            "metric label value must not contain control characters",
        ));
    }
    if val.contains("://") || val.contains('@') {
        return Err(Error::resource_limit(
            "high-cardinality URL or email metric label value rejected",
        ));
    }
    let digit_count = val.bytes().filter(|b| b.is_ascii_digit()).count();
    if digit_count > 8 || looks_like_uuid_or_hex_id(val) {
        return Err(Error::resource_limit(
            "high-cardinality identifier/UUID/numeric metric label value rejected",
        ));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Metrics Sink Trait & Slow-Query Ring (`LIN-1614`, `LIN-1621`, `LIN-1622`)
// ---------------------------------------------------------------------------

/// External metrics observer invoked inside `catch_unwind` isolation (`LIN-1614`, `LIN-1635`).
pub trait MetricsSink: Send + Sync + 'static {
    fn on_query(&self, record: &QueryPhaseRecord);
    fn on_counter(&self, name: &str, delta: u64);
    fn on_gauge(&self, name: &str, value: i64);
}

/// Bounded ring buffer of slow and deterministically sampled queries (`LIN-1621`, `LIN-1622`).
#[derive(Debug)]
pub struct SlowQueryRing {
    capacity: usize,
    threshold_ns: AtomicU64,
    sample_every_n: AtomicU64,
    sample_seq: AtomicU64,
    evicted_count: AtomicU64,
    entries: Mutex<VecDeque<QueryPhaseRecord>>,
}

impl SlowQueryRing {
    pub fn new(capacity: usize, threshold_ns: u64, sample_every_n: u64) -> Self {
        let cap = capacity.max(1);
        Self {
            capacity: cap,
            threshold_ns: AtomicU64::new(threshold_ns),
            sample_every_n: AtomicU64::new(sample_every_n),
            sample_seq: AtomicU64::new(0),
            evicted_count: AtomicU64::new(0),
            entries: Mutex::new(VecDeque::with_capacity(cap.min(256))),
        }
    }

    #[inline]
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    #[inline]
    pub fn set_threshold_ns(&self, threshold_ns: u64) {
        self.threshold_ns.store(threshold_ns, Ordering::Relaxed);
    }

    #[inline]
    pub fn set_sample_every_n(&self, sample_every_n: u64) {
        self.sample_every_n.store(sample_every_n, Ordering::Relaxed);
    }

    #[inline]
    pub fn evicted_count(&self) -> u64 {
        self.evicted_count.load(Ordering::Relaxed)
    }

    pub fn observe(&self, record: &QueryPhaseRecord) -> bool {
        let total = record.total_ns();
        let threshold = self.threshold_ns.load(Ordering::Relaxed);
        let is_slow = total >= threshold;
        let sample_n = self.sample_every_n.load(Ordering::Relaxed);
        let is_sampled = if sample_n == 0 {
            false
        } else if sample_n == 1 {
            true
        } else {
            let seq = self.sample_seq.fetch_add(1, Ordering::Relaxed);
            seq % sample_n == 0
        };

        if !is_slow && !is_sampled {
            return false;
        }

        let mut guard = self
            .entries
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if guard.len() >= self.capacity {
            guard.pop_front();
            self.evicted_count.fetch_add(1, Ordering::Relaxed);
        }
        guard.push_back(record.clone());
        true
    }

    pub fn snapshot(&self) -> Vec<QueryPhaseRecord> {
        let guard = self
            .entries
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        guard.iter().cloned().collect()
    }

    pub fn len(&self) -> usize {
        let guard = self
            .entries
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        guard.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn clear(&self) {
        let mut guard = self
            .entries
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        guard.clear();
        self.evicted_count.store(0, Ordering::Relaxed);
    }
}

// ---------------------------------------------------------------------------
// Health, Readiness, Per-Collection Stats & Provenance (`LIN-1629`..`LIN-1640`)
// ---------------------------------------------------------------------------

/// Point-in-time liveness and operational health snapshot (`LIN-1629`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HealthStatusSnapshot {
    pub healthy: bool,
    pub shutting_down: bool,
    pub wal_pending_bytes: i64,
    pub checkpoint_debt_bytes: i64,
    pub embedding_failures: u64,
    pub schema_version: u32,
}

/// Traffic-routing readiness state (`LIN-1630`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReadinessStatus {
    Ready,
    CatchingUp { lag_bytes: u64 },
    Draining,
    Degraded { reason: String },
}

/// Per-collection exported operational statistics (`LIN-1631`).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct CollectionStatsExport {
    pub collection: String,
    pub live_rows: u64,
    pub retained_bytes: u64,
    pub scans: u64,
    pub mutations: u64,
}

/// Trace-to-benchmark provenance descriptor (`LIN-1640`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraceBenchmarkProvenance {
    pub exec_id: u64,
    pub plan_hash: u64,
    pub benchmark_suite: String,
    pub git_commit: String,
}

/// Profiler command recipe entry (`LIN-1639`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProfilerCookbookRecipe {
    pub platform: &'static str,
    pub goal: &'static str,
    pub command: &'static str,
}

pub const PROFILER_COMMAND_COOKBOOK: &[ProfilerCookbookRecipe] = &[
    ProfilerCookbookRecipe {
        platform: "macos",
        goal: "CPU sample flamegraph on release bench",
        command: "xcrun xctrace record --template 'Time Profiler' --launch -- ./target/release/deps/compare",
    },
    ProfilerCookbookRecipe {
        platform: "linux",
        goal: "Hardware cycle & cache-miss profile",
        command: "perf record -F 999 -g -- ./target/release/deps/compare && perf script",
    },
    ProfilerCookbookRecipe {
        platform: "cross-platform",
        goal: "Export live Prometheus metrics snapshot",
        command: "lin --metrics-prometheus",
    },
];

// ---------------------------------------------------------------------------
// Telemetry Registry (`LIN-1605`..`LIN-1638`)
// ---------------------------------------------------------------------------

const DEFAULT_MAX_LABEL_SERIES_PER_METRIC: usize = 256;
const DEFAULT_MAX_CATCHUP_LAG_BYTES: i64 = 16 * 1024 * 1024;
const DEFAULT_DEGRADED_WAL_BYTES: i64 = 512 * 1024 * 1024;

/// Central lock-free runtime observability registry.
pub struct TelemetryRegistry {
    enabled: AtomicBool,
    has_sink: AtomicBool,
    shutting_down: AtomicBool,
    sink: RwLock<Option<Arc<dyn MetricsSink>>>,
    sink_failures: AtomicU64,

    // Phase latency histograms (`LIN-1605`..`LIN-1608`)
    pub prepare_latency: LatencyHistogram,
    pub execute_latency: LatencyHistogram,
    pub commit_latency: LatencyHistogram,
    pub queue_latency: LatencyHistogram,

    // Atomic gauges (`LIN-1609`..`LIN-1612`, `LIN-1385`, `LIN-1386`)
    pub wal_pending_bytes: AtomicI64,
    pub checkpoint_debt_bytes: AtomicI64,
    pub pool_retained_bytes: AtomicI64,
    pub gpu_resident_bytes: AtomicI64,
    pub active_queries: AtomicI64,
    pub replica_lag_ms: AtomicI64,
    pub replica_lag_bytes: AtomicI64,

    // Operational counters (`LIN-1603`, `LIN-1604`, `LIN-1625`..`LIN-1628`)
    pub operator_rows_in: AtomicU64,
    pub operator_rows_out: AtomicU64,
    pub operator_bytes_allocated: AtomicU64,
    pub replica_reconnects: AtomicU64,
    pub stale_snapshots: AtomicU64,
    pub embedding_failures: AtomicU64,
    pub compaction_pauses: AtomicU64,
    pub compaction_pause_ns: AtomicU64,

    pub slow_queries: SlowQueryRing,
    collection_stats: Mutex<FxHashMap<String, CollectionStatsExport>>,
    label_series: Mutex<FxHashMap<String, FxHashSet<(String, String)>>>,
    max_series_per_metric: usize,
    catchup_lag_threshold_bytes: AtomicI64,
    degraded_wal_threshold_bytes: AtomicI64,
}

impl Default for TelemetryRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl TelemetryRegistry {
    pub fn new() -> Self {
        Self::with_slow_ring(128, 10_000_000, 0)
    }

    pub fn with_slow_ring(
        slow_capacity: usize,
        slow_threshold_ns: u64,
        sample_every_n: u64,
    ) -> Self {
        Self {
            enabled: AtomicBool::new(true),
            has_sink: AtomicBool::new(false),
            shutting_down: AtomicBool::new(false),
            sink: RwLock::new(None),
            sink_failures: AtomicU64::new(0),
            prepare_latency: LatencyHistogram::new(),
            execute_latency: LatencyHistogram::new(),
            commit_latency: LatencyHistogram::new(),
            queue_latency: LatencyHistogram::new(),
            wal_pending_bytes: AtomicI64::new(0),
            checkpoint_debt_bytes: AtomicI64::new(0),
            pool_retained_bytes: AtomicI64::new(0),
            gpu_resident_bytes: AtomicI64::new(0),
            active_queries: AtomicI64::new(0),
            replica_lag_ms: AtomicI64::new(0),
            replica_lag_bytes: AtomicI64::new(0),
            operator_rows_in: AtomicU64::new(0),
            operator_rows_out: AtomicU64::new(0),
            operator_bytes_allocated: AtomicU64::new(0),
            replica_reconnects: AtomicU64::new(0),
            stale_snapshots: AtomicU64::new(0),
            embedding_failures: AtomicU64::new(0),
            compaction_pauses: AtomicU64::new(0),
            compaction_pause_ns: AtomicU64::new(0),
            slow_queries: SlowQueryRing::new(slow_capacity, slow_threshold_ns, sample_every_n),
            collection_stats: Mutex::new(FxHashMap::default()),
            label_series: Mutex::new(FxHashMap::default()),
            max_series_per_metric: DEFAULT_MAX_LABEL_SERIES_PER_METRIC,
            catchup_lag_threshold_bytes: AtomicI64::new(DEFAULT_MAX_CATCHUP_LAG_BYTES),
            degraded_wal_threshold_bytes: AtomicI64::new(DEFAULT_DEGRADED_WAL_BYTES),
        }
    }

    /// Disabled registry whose `record_*` fast path executes a single `Ordering::Relaxed` atomic load (`LIN-1617`).
    pub fn disabled() -> Self {
        let reg = Self::new();
        reg.enabled.store(false, Ordering::Relaxed);
        reg
    }

    #[inline]
    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    pub fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::Relaxed);
    }

    pub fn set_sink(&self, sink: Option<Arc<dyn MetricsSink>>) {
        let present = sink.is_some();
        let mut guard = self
            .sink
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *guard = sink;
        self.has_sink.store(present, Ordering::Release);
    }

    #[inline]
    pub fn sink_failure_count(&self) -> u64 {
        self.sink_failures.load(Ordering::Relaxed)
    }

    pub fn set_readiness_thresholds(&self, catchup_lag_bytes: i64, degraded_wal_bytes: i64) {
        self.catchup_lag_threshold_bytes
            .store(catchup_lag_bytes.max(0), Ordering::Relaxed);
        self.degraded_wal_threshold_bytes
            .store(degraded_wal_bytes.max(1), Ordering::Relaxed);
    }

    #[inline]
    fn with_isolated_sink(&self, f: impl FnOnce(&dyn MetricsSink)) {
        // Fast path: 1 relaxed atomic load when no sink is attached or after shutdown (`LIN-1617`, `LIN-1636`).
        if !self.has_sink.load(Ordering::Acquire) || self.shutting_down.load(Ordering::Relaxed) {
            return;
        }
        let sink_opt = {
            let guard = self
                .sink
                .read()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            guard.clone()
        };
        if let Some(sink) = sink_opt {
            let res = catch_unwind(AssertUnwindSafe(|| f(sink.as_ref())));
            if res.is_err() {
                self.sink_failures.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    /// Records a completed query phase profile across histograms, counters, slow-query ring, and sink.
    pub fn record_query(&self, mut record: QueryPhaseRecord) {
        if !self.enabled.load(Ordering::Relaxed) {
            return;
        }
        if record.anomaly_hint.is_none() {
            record.anomaly_hint = record.detect_phase_anomaly();
        }
        self.prepare_latency.record_ns(record.prepare_ns);
        self.execute_latency.record_ns(record.execute_ns);
        self.commit_latency.record_ns(record.commit_ns);
        self.queue_latency.record_ns(record.queue_ns);

        self.operator_rows_in
            .fetch_add(record.rows_in, Ordering::Relaxed);
        self.operator_rows_out
            .fetch_add(record.rows_out, Ordering::Relaxed);
        self.operator_bytes_allocated
            .fetch_add(record.bytes_allocated, Ordering::Relaxed);

        self.slow_queries.observe(&record);
        self.with_isolated_sink(|sink| sink.on_query(&record));
    }

    // Gauge update helpers (update atomic gauge + notify sink safely)
    pub fn set_wal_pending_bytes(&self, bytes: i64) {
        self.wal_pending_bytes.store(bytes, Ordering::Relaxed);
        self.with_isolated_sink(|s| s.on_gauge("wal_pending_bytes", bytes));
    }

    pub fn set_checkpoint_debt_bytes(&self, bytes: i64) {
        self.checkpoint_debt_bytes.store(bytes, Ordering::Relaxed);
        self.with_isolated_sink(|s| s.on_gauge("checkpoint_debt_bytes", bytes));
    }

    pub fn set_pool_retained_bytes(&self, bytes: i64) {
        self.pool_retained_bytes.store(bytes, Ordering::Relaxed);
        self.with_isolated_sink(|s| s.on_gauge("pool_retained_bytes", bytes));
    }

    pub fn set_gpu_resident_bytes(&self, bytes: i64) {
        self.gpu_resident_bytes.store(bytes, Ordering::Relaxed);
        self.with_isolated_sink(|s| s.on_gauge("gpu_resident_bytes", bytes));
    }

    pub fn set_active_queries(&self, count: i64) {
        self.active_queries.store(count, Ordering::Relaxed);
        self.with_isolated_sink(|s| s.on_gauge("active_queries", count));
    }

    pub fn set_replica_lag(&self, lag_ms: i64, lag_bytes: i64) {
        self.replica_lag_ms.store(lag_ms, Ordering::Relaxed);
        self.replica_lag_bytes.store(lag_bytes, Ordering::Relaxed);
        self.with_isolated_sink(|s| {
            s.on_gauge("replica_lag_ms", lag_ms);
            s.on_gauge("replica_lag_bytes", lag_bytes);
        });
    }

    /// RAII active-query guard safe across normal returns and stack unwinds (`LIN-1633`).
    pub fn enter_query(&self) -> ActiveQueryGuard<'_> {
        if self.enabled.load(Ordering::Relaxed) {
            let cur = self.active_queries.fetch_add(1, Ordering::Relaxed) + 1;
            self.with_isolated_sink(|s| s.on_gauge("active_queries", cur));
        }
        ActiveQueryGuard { registry: self }
    }

    // Operational counter helpers (`LIN-1625`..`LIN-1628`)
    pub fn record_operator_flow(&self, rows_in: u64, rows_out: u64, bytes_allocated: u64) {
        if !self.enabled.load(Ordering::Relaxed) {
            return;
        }
        self.operator_rows_in.fetch_add(rows_in, Ordering::Relaxed);
        self.operator_rows_out
            .fetch_add(rows_out, Ordering::Relaxed);
        self.operator_bytes_allocated
            .fetch_add(bytes_allocated, Ordering::Relaxed);
        self.with_isolated_sink(|s| {
            s.on_counter("operator_rows_in", rows_in);
            s.on_counter("operator_rows_out", rows_out);
            s.on_counter("operator_bytes_allocated", bytes_allocated);
        });
    }

    pub fn record_replica_reconnect(&self) {
        if !self.enabled.load(Ordering::Relaxed) {
            return;
        }
        self.replica_reconnects.fetch_add(1, Ordering::Relaxed);
        self.with_isolated_sink(|s| s.on_counter("replica_reconnects", 1));
    }

    pub fn record_stale_snapshot(&self) {
        if !self.enabled.load(Ordering::Relaxed) {
            return;
        }
        self.stale_snapshots.fetch_add(1, Ordering::Relaxed);
        self.with_isolated_sink(|s| s.on_counter("stale_snapshots", 1));
    }

    pub fn record_embedding_failure(&self) {
        if !self.enabled.load(Ordering::Relaxed) {
            return;
        }
        self.embedding_failures.fetch_add(1, Ordering::Relaxed);
        self.with_isolated_sink(|s| s.on_counter("embedding_failures", 1));
    }

    pub fn record_compaction_pause(&self, pause_ns: u64) {
        if !self.enabled.load(Ordering::Relaxed) {
            return;
        }
        self.compaction_pauses.fetch_add(1, Ordering::Relaxed);
        self.compaction_pause_ns
            .fetch_add(pause_ns, Ordering::Relaxed);
        self.with_isolated_sink(|s| {
            s.on_counter("compaction_pauses", 1);
            s.on_counter("compaction_pause_ns", pause_ns);
        });
    }

    /// Registers a validated metric label pair while enforcing per-metric series cardinality cap (`LIN-1618`).
    pub fn register_metric_label(&self, metric: &str, key: &str, val: &str) -> Result<(), Error> {
        validate_metric_label(key, val)?;
        let mut guard = self
            .label_series
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let set = guard.entry(metric.to_string()).or_default();
        let pair = (key.to_string(), val.to_string());
        if !set.contains(&pair) && set.len() >= self.max_series_per_metric {
            return Err(Error::resource_limit(format!(
                "metric `{metric}` exceeded max label cardinality ({})",
                self.max_series_per_metric
            )));
        }
        set.insert(pair);
        Ok(())
    }

    /// Updates exported per-collection operational stats (`LIN-1631`).
    pub fn update_collection_stats(&self, stats: CollectionStatsExport) {
        let mut guard = self
            .collection_stats
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        guard.insert(stats.collection.clone(), stats);
    }

    pub fn export_collection_stats(&self) -> Vec<CollectionStatsExport> {
        let guard = self
            .collection_stats
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut list: Vec<CollectionStatsExport> = guard.values().cloned().collect();
        list.sort_by(|a, b| a.collection.cmp(&b.collection));
        list
    }

    /// Atomically resets all operational counters (`LIN-1632`).
    pub fn reset_counters(&self) {
        self.operator_rows_in.store(0, Ordering::Relaxed);
        self.operator_rows_out.store(0, Ordering::Relaxed);
        self.operator_bytes_allocated.store(0, Ordering::Relaxed);
        self.replica_reconnects.store(0, Ordering::Relaxed);
        self.stale_snapshots.store(0, Ordering::Relaxed);
        self.embedding_failures.store(0, Ordering::Relaxed);
        self.compaction_pauses.store(0, Ordering::Relaxed);
        self.compaction_pause_ns.store(0, Ordering::Relaxed);
        self.sink_failures.store(0, Ordering::Relaxed);
    }

    pub fn reset_histograms(&self) {
        self.prepare_latency.reset();
        self.execute_latency.reset();
        self.commit_latency.reset();
        self.queue_latency.reset();
    }

    /// Marks the registry as shutting down (`LIN-1636`).
    /// Subsequent instrumentation calls and exports remain 100% memory-safe and panic-free.
    pub fn mark_shutdown(&self) {
        self.shutting_down.store(true, Ordering::Release);
        self.has_sink.store(false, Ordering::Release);
        let mut guard = self
            .sink
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *guard = None;
    }

    #[inline]
    pub fn is_shutting_down(&self) -> bool {
        self.shutting_down.load(Ordering::Acquire)
    }

    /// Point-in-time health snapshot (`LIN-1629`).
    pub fn health_snapshot(&self) -> HealthStatusSnapshot {
        let shutting_down = self.is_shutting_down();
        let wal_pending = self.wal_pending_bytes.load(Ordering::Relaxed);
        let checkpoint_debt = self.checkpoint_debt_bytes.load(Ordering::Relaxed);
        let embed_failures = self.embedding_failures.load(Ordering::Relaxed);
        let degraded_wal = self.degraded_wal_threshold_bytes.load(Ordering::Relaxed);
        let healthy = !shutting_down && wal_pending < degraded_wal;
        HealthStatusSnapshot {
            healthy,
            shutting_down,
            wal_pending_bytes: wal_pending,
            checkpoint_debt_bytes: checkpoint_debt,
            embedding_failures: embed_failures,
            schema_version: OBSERVABILITY_SCHEMA_VERSION,
        }
    }

    /// Computes readiness status for load balancers and replica pools (`LIN-1630`).
    pub fn readiness_status(&self) -> ReadinessStatus {
        if self.is_shutting_down() {
            return ReadinessStatus::Draining;
        }
        let wal_pending = self.wal_pending_bytes.load(Ordering::Relaxed);
        let degraded_wal = self.degraded_wal_threshold_bytes.load(Ordering::Relaxed);
        if wal_pending >= degraded_wal {
            return ReadinessStatus::Degraded {
                reason: format!(
                    "wal_pending_bytes ({wal_pending}) >= degraded threshold ({degraded_wal})"
                ),
            };
        }
        let lag_bytes = self.replica_lag_bytes.load(Ordering::Relaxed);
        let catchup_limit = self.catchup_lag_threshold_bytes.load(Ordering::Relaxed);
        if lag_bytes > catchup_limit && lag_bytes > 0 {
            return ReadinessStatus::CatchingUp {
                lag_bytes: lag_bytes as u64,
            };
        }
        ReadinessStatus::Ready
    }

    /// Exports all histograms, gauges, and counters in valid Prometheus 0.0.4 text exposition format (`LIN-1616`).
    pub fn export_prometheus_text(&self) -> String {
        let mut out = String::with_capacity(4096);

        let _ = writeln!(
            out,
            "# HELP lin_observability_info Observability schema version metadata."
        );
        let _ = writeln!(out, "# TYPE lin_observability_info gauge");
        let _ = writeln!(
            out,
            "lin_observability_info{{schema_version=\"{OBSERVABILITY_SCHEMA_VERSION}\"}} 1"
        );

        write_prom_histogram(
            &mut out,
            "lin_prepare_latency_seconds",
            "Query parse, check, and plan latency in seconds.",
            &self.prepare_latency,
        );
        write_prom_histogram(
            &mut out,
            "lin_execute_latency_seconds",
            "Query operator execution latency in seconds.",
            &self.execute_latency,
        );
        write_prom_histogram(
            &mut out,
            "lin_commit_latency_seconds",
            "Transaction WAL commit/fsync latency in seconds.",
            &self.commit_latency,
        );
        write_prom_histogram(
            &mut out,
            "lin_queue_latency_seconds",
            "Query admission and lock queue wait latency in seconds.",
            &self.queue_latency,
        );

        write_prom_gauge(
            &mut out,
            "lin_wal_pending_bytes",
            "Uncheckpointed WAL bytes pending on disk.",
            self.wal_pending_bytes.load(Ordering::Relaxed),
        );
        write_prom_gauge(
            &mut out,
            "lin_checkpoint_debt_bytes",
            "Accumulated checkpoint compaction debt in bytes.",
            self.checkpoint_debt_bytes.load(Ordering::Relaxed),
        );
        write_prom_gauge(
            &mut out,
            "lin_pool_retained_bytes",
            "Memory pool bytes retained by execution arenas.",
            self.pool_retained_bytes.load(Ordering::Relaxed),
        );
        write_prom_gauge(
            &mut out,
            "lin_gpu_resident_bytes",
            "Resident GPU device buffer bytes.",
            self.gpu_resident_bytes.load(Ordering::Relaxed),
        );
        write_prom_gauge(
            &mut out,
            "lin_active_queries",
            "Currently executing queries.",
            self.active_queries.load(Ordering::Relaxed),
        );
        write_prom_gauge(
            &mut out,
            "lin_replica_lag_ms",
            "Replication apply lag in milliseconds.",
            self.replica_lag_ms.load(Ordering::Relaxed),
        );
        write_prom_gauge(
            &mut out,
            "lin_replica_lag_bytes",
            "Replication WAL byte lag behind primary.",
            self.replica_lag_bytes.load(Ordering::Relaxed),
        );

        write_prom_counter(
            &mut out,
            "lin_operator_rows_in_total",
            "Total input rows consumed by query operators.",
            self.operator_rows_in.load(Ordering::Relaxed),
        );
        write_prom_counter(
            &mut out,
            "lin_operator_rows_out_total",
            "Total output rows produced by query operators.",
            self.operator_rows_out.load(Ordering::Relaxed),
        );
        write_prom_counter(
            &mut out,
            "lin_operator_bytes_allocated_total",
            "Total scratch bytes allocated by query operators.",
            self.operator_bytes_allocated.load(Ordering::Relaxed),
        );
        write_prom_counter(
            &mut out,
            "lin_replica_reconnects_total",
            "Total replica stream reconnect attempts.",
            self.replica_reconnects.load(Ordering::Relaxed),
        );
        write_prom_counter(
            &mut out,
            "lin_stale_snapshots_total",
            "Total stale snapshot read rejections.",
            self.stale_snapshots.load(Ordering::Relaxed),
        );
        write_prom_counter(
            &mut out,
            "lin_embedding_failures_total",
            "Total embedding provider invocations that failed.",
            self.embedding_failures.load(Ordering::Relaxed),
        );
        write_prom_counter(
            &mut out,
            "lin_compaction_pauses_total",
            "Total background compaction pause events.",
            self.compaction_pauses.load(Ordering::Relaxed),
        );
        write_prom_counter(
            &mut out,
            "lin_compaction_pause_ns_total",
            "Total nanoseconds spent paused for compaction.",
            self.compaction_pause_ns.load(Ordering::Relaxed),
        );

        out
    }
}

pub struct ActiveQueryGuard<'a> {
    registry: &'a TelemetryRegistry,
}

impl Drop for ActiveQueryGuard<'_> {
    fn drop(&mut self) {
        if self.registry.enabled.load(Ordering::Relaxed) {
            let cur = self.registry.active_queries.fetch_sub(1, Ordering::Relaxed) - 1;
            self.registry
                .with_isolated_sink(|s| s.on_gauge("active_queries", cur));
        }
    }
}

fn format_le_seconds(ns: u64) -> String {
    let secs = (ns as f64) / 1_000_000_000.0;
    if ns.is_multiple_of(1_000_000_000) {
        format!("{:.1}", secs)
    } else if ns.is_multiple_of(1_000_000) {
        format!("{:.3}", secs)
    } else {
        format!("{:.6}", secs)
    }
}

fn write_prom_histogram(out: &mut String, name: &str, help: &str, hist: &LatencyHistogram) {
    let _ = writeln!(out, "# HELP {name} {help}");
    let _ = writeln!(out, "# TYPE {name} histogram");
    let counts = hist.bucket_counts();
    let mut cumulative = 0u64;
    for (idx, &bound_ns) in HISTOGRAM_UPPER_BOUNDS_NS.iter().enumerate() {
        cumulative = cumulative.saturating_add(counts[idx]);
        let le = format_le_seconds(bound_ns);
        let _ = writeln!(out, "{name}_bucket{{le=\"{le}\"}} {cumulative}");
    }
    cumulative = cumulative.saturating_add(counts[HISTOGRAM_NUM_BUCKETS - 1]);
    let _ = writeln!(out, "{name}_bucket{{le=\"+Inf\"}} {cumulative}");
    let sum_secs = (hist.sum_ns() as f64) / 1_000_000_000.0;
    let _ = writeln!(out, "{name}_sum {sum_secs:.9}");
    let _ = writeln!(out, "{name}_count {cumulative}");
}

fn write_prom_gauge(out: &mut String, name: &str, help: &str, val: i64) {
    let _ = writeln!(out, "# HELP {name} {help}");
    let _ = writeln!(out, "# TYPE {name} gauge");
    let _ = writeln!(out, "{name} {val}");
}

fn write_prom_counter(out: &mut String, name: &str, help: &str, val: u64) {
    let _ = writeln!(out, "# HELP {name} {help}");
    let _ = writeln!(out, "# TYPE {name} counter");
    let _ = writeln!(out, "{name} {val}");
}

/// Converts a [`QueryPhaseRecord`] into OpenTelemetry span key-value attributes (`LIN-1615`).
pub fn export_otel_span_attributes(record: &QueryPhaseRecord) -> Vec<(&'static str, String)> {
    let mut attrs = Vec::with_capacity(13);
    attrs.push(("db.system", "lin".to_string()));
    attrs.push((
        "lin.observability.schema_version",
        OBSERVABILITY_SCHEMA_VERSION.to_string(),
    ));
    attrs.push(("lin.query.exec_id", record.exec_id.to_string()));
    attrs.push(("lin.query.plan_hash", format!("{:016x}", record.plan_hash)));
    attrs.push(("db.statement", record.redacted_query.clone()));
    attrs.push(("lin.phase.prepare_ns", record.prepare_ns.to_string()));
    attrs.push(("lin.phase.execute_ns", record.execute_ns.to_string()));
    attrs.push(("lin.phase.commit_ns", record.commit_ns.to_string()));
    attrs.push(("lin.phase.queue_ns", record.queue_ns.to_string()));
    attrs.push(("lin.operator.rows_in", record.rows_in.to_string()));
    attrs.push(("lin.operator.rows_out", record.rows_out.to_string()));
    attrs.push((
        "lin.operator.bytes_allocated",
        record.bytes_allocated.to_string(),
    ));
    if let Some(hint) = record.anomaly_hint {
        attrs.push(("lin.phase.anomaly_hint", hint.to_string()));
    }
    attrs
}
