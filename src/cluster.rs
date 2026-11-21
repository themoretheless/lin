//! Horizontal sharding, Jump Consistent Hash routing, predicate shard pruning,
//! columnar `insert_batch` scattering, and two-phase distributed Kusto query execution.

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Instant;

use smallvec::SmallVec;

use crate::ast::{
    AggFunc, AggSpec, BinStep, CmpOp, Decl, EdgeTarget, GroupExpr, GroupKey, MatchHop, Pred,
    Query, Record, SearchMode, Source, Step, Stmt, TypeExpr, Value,
};
use crate::batch::{Column, RecordBatch};
use crate::error::Error;
use crate::exec::{
    Db, OpenOpts, Quotas, apply_coordinator_post_steps, coordinator_agg_arg_extremum,
    empty_partial_summarize_wire, merge_partial_summarize_wire, rows_to_batch_for_query,
};
use crate::server::{
    ClientOpts, ExecutionReceipt, LinServer, ServerClient, ServerConfig, ServerHandle,
};
use crate::store::{Cell, Row};

/// Partitioning algorithm mapping a 64-bit canonical key hash to a shard index.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ShardStrategy {
    #[default]
    JumpConsistentHash,
    HashMod,
}

/// Lamping & Veach (2014) Jump Consistent Hash: minimal key movement on resharding.
pub fn jump_consistent_hash(mut key: u64, num_buckets: usize) -> usize {
    if num_buckets <= 1 {
        return 0;
    }
    let buckets = num_buckets.min(i32::MAX as usize) as i64;
    let mut b: i64 = -1;
    let mut j: i64 = 0;
    while j < buckets {
        b = j;
        key = key.wrapping_mul(2_862_933_555_777_941_757).wrapping_add(1);
        j = ((b.wrapping_add(1) as f64)
            * (((1i64 << 31) as f64) / (((key >> 33).wrapping_add(1)) as f64))) as i64;
    }
    b.max(0) as usize
}

#[inline]
fn fmix64(mut h: u64) -> u64 {
    h ^= h >> 33;
    h = h.wrapping_mul(0xff51_afd7_ed55_8ccd);
    h ^= h >> 33;
    h = h.wrapping_mul(0xc4ce_b9fe_1a85_ec53);
    h ^ (h >> 33)
}

#[inline]
fn hash_u64_tagged(tag: u8, bits: u64) -> u64 {
    fmix64(((tag as u64) << 56) ^ bits ^ 0x9e37_79b9_7f4a_7c15)
}

#[inline]
fn hash_bytes_tagged(tag: u8, bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325 ^ (tag as u64);
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    fmix64(h ^ ((bytes.len() as u64) << 32))
}

#[inline]
fn hash_i64(n: i64) -> u64 {
    hash_u64_tagged(1, n as u64)
}

#[inline]
fn hash_f64_canonical(f: f64) -> u64 {
    if f.is_nan() {
        return hash_u64_tagged(2, u64::MAX);
    }
    let f = if f == 0.0 { 0.0 } else { f };
    if f.is_finite()
        && f.fract() == 0.0
        && f >= i64::MIN as f64
        && f < 9_223_372_036_854_775_808.0
    {
        let as_i = f as i64;
        if (as_i as f64) == f {
            return hash_i64(as_i);
        }
    }
    hash_u64_tagged(2, f.to_bits())
}

/// Deterministic canonical 64-bit hash for a [`Cell`] shard key.
pub fn shard_hash_cell(cell: &Cell) -> u64 {
    match cell {
        Cell::Null => hash_bytes_tagged(6, &[]),
        Cell::Bool(b) => hash_u64_tagged(4, u64::from(*b)),
        Cell::Int(i) => hash_i64(*i),
        Cell::Float(f) => hash_f64_canonical(*f),
        Cell::Time(ms) => hash_u64_tagged(3, *ms as u64),
        Cell::Text(s) => hash_bytes_tagged(5, s.as_bytes()),
        Cell::SmallText(s) => hash_bytes_tagged(5, s.as_str().as_bytes()),
        Cell::Vec(v) => {
            let mut h = hash_u64_tagged(7, v.len() as u64);
            for &x in v.iter() {
                h = fmix64(h ^ hash_f64_canonical(x as f64));
            }
            h
        }
    }
}

/// Deterministic canonical 64-bit hash for an AST [`Value`] literal (`None` for dynamic `now`/`$param`).
pub fn shard_hash_value(val: &Value) -> Option<u64> {
    match val {
        Value::String(s) => Some(hash_bytes_tagged(5, s.as_bytes())),
        Value::Name(s) => Some(hash_bytes_tagged(5, s.as_bytes())),
        Value::Int(i) => Some(hash_i64(*i)),
        Value::Float(f) => Some(hash_f64_canonical(*f)),
        Value::Bool(b) => Some(hash_u64_tagged(4, u64::from(*b))),
        Value::Timestamp(ms) => Some(hash_u64_tagged(3, *ms as u64)),
        Value::Duration(d) => Some(hash_i64(d.as_millis())),
        Value::Now | Value::NowMinus(_) | Value::Param(_) => None,
    }
}

#[inline]
pub fn pick_shard(hash: u64, num_shards: usize, strategy: ShardStrategy) -> usize {
    if num_shards <= 1 {
        return 0;
    }
    match strategy {
        ShardStrategy::JumpConsistentHash => jump_consistent_hash(hash, num_shards),
        ShardStrategy::HashMod => (hash % (num_shards as u64)) as usize,
    }
}

/// Execution telemetry for a distributed cluster operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClusterStats {
    pub shards_queried: usize,
    pub total_shards: usize,
    pub max_gen: u64,
    pub elapsed_us: u64,
}

/// Configuration for [`ShardRouter`].
#[derive(Clone, Debug)]
pub struct ShardConfig {
    pub shard_addrs: Vec<String>,
    pub shard_keys: BTreeMap<String, String>,
    pub default_shard_key: Option<String>,
    pub strategy: ShardStrategy,
    pub client: ClientOpts,
    pub id_prefix: String,
}

impl Default for ShardConfig {
    fn default() -> Self {
        Self {
            shard_addrs: Vec::new(),
            shard_keys: BTreeMap::new(),
            default_shard_key: None,
            strategy: ShardStrategy::JumpConsistentHash,
            client: ClientOpts::default(),
            id_prefix: "lin_s".to_string(),
        }
    }
}

impl ShardConfig {
    pub fn shard_key_for(&self, collection: &str) -> Option<&str> {
        self.shard_keys
            .get(collection)
            .map(String::as_str)
            .or(self.default_shard_key.as_deref())
    }
}

// =========================================================================
// Predicate shard pruning
// =========================================================================

fn prune_mask(pred: &Pred, shard_key: &str, num_shards: usize, strategy: ShardStrategy) -> Option<u64> {
    match pred {
        Pred::Cmp {
            field,
            op: CmpOp::Eq,
            value,
        } if field.as_str() == shard_key => {
            shard_hash_value(value).map(|h| 1u64 << pick_shard(h, num_shards, strategy))
        }
        Pred::In { field, values } if field.as_str() == shard_key => {
            let mut mask = 0u64;
            for v in values {
                let h = shard_hash_value(v)?;
                mask |= 1u64 << pick_shard(h, num_shards, strategy);
            }
            Some(mask)
        }
        Pred::And(a, b) => match (
            prune_mask(a, shard_key, num_shards, strategy),
            prune_mask(b, shard_key, num_shards, strategy),
        ) {
            (Some(ma), Some(mb)) => Some(ma & mb),
            (Some(ma), None) => Some(ma),
            (None, Some(mb)) => Some(mb),
            (None, None) => None,
        },
        Pred::Or(a, b) => match (
            prune_mask(a, shard_key, num_shards, strategy),
            prune_mask(b, shard_key, num_shards, strategy),
        ) {
            (Some(ma), Some(mb)) => Some(ma | mb),
            _ => None,
        },
        _ => None,
    }
}

fn prune_set(pred: &Pred, shard_key: &str, num_shards: usize, strategy: ShardStrategy) -> Option<BTreeSet<usize>> {
    match pred {
        Pred::Cmp {
            field,
            op: CmpOp::Eq,
            value,
        } if field.as_str() == shard_key => {
            shard_hash_value(value).map(|h| BTreeSet::from([pick_shard(h, num_shards, strategy)]))
        }
        Pred::In { field, values } if field.as_str() == shard_key => {
            let mut set = BTreeSet::new();
            for v in values {
                let h = shard_hash_value(v)?;
                set.insert(pick_shard(h, num_shards, strategy));
            }
            Some(set)
        }
        Pred::And(a, b) => match (
            prune_set(a, shard_key, num_shards, strategy),
            prune_set(b, shard_key, num_shards, strategy),
        ) {
            (Some(sa), Some(sb)) => Some(sa.intersection(&sb).copied().collect()),
            (Some(sa), None) => Some(sa),
            (None, Some(sb)) => Some(sb),
            (None, None) => None,
        },
        Pred::Or(a, b) => match (
            prune_set(a, shard_key, num_shards, strategy),
            prune_set(b, shard_key, num_shards, strategy),
        ) {
            (Some(sa), Some(sb)) => Some(sa.union(&sb).copied().collect()),
            _ => None,
        },
        _ => None,
    }
}

fn target_shards_for_pred(
    pred: Option<&Pred>,
    shard_key: Option<&str>,
    num_shards: usize,
    strategy: ShardStrategy,
) -> Vec<usize> {
    if num_shards <= 1 {
        return vec![0];
    }
    let Some(key) = shard_key else {
        return (0..num_shards).collect();
    };
    let Some(p) = pred else {
        return (0..num_shards).collect();
    };
    if num_shards <= 64 {
        return match prune_mask(p, key, num_shards, strategy) {
            Some(mut m) => {
                let mut out = Vec::with_capacity(m.count_ones() as usize);
                while m != 0 {
                    let s = m.trailing_zeros() as usize;
                    out.push(s);
                    m &= m - 1;
                }
                out
            }
            None => (0..num_shards).collect(),
        };
    }
    match prune_set(p, key, num_shards, strategy) {
        Some(set) => set.into_iter().collect(),
        None => (0..num_shards).collect(),
    }
}

// =========================================================================
// Lossless AST -> LinQL formatter (single-String buffer, float `.0` preserved)
// =========================================================================

fn write_float_literal(out: &mut String, n: f64) {
    if n.is_nan() || n == 0.0 {
        out.push_str("0.0");
        return;
    }
    let start = out.len();
    let _ = write!(out, "{n}");
    if !out[start..]
        .bytes()
        .any(|b| b == b'.' || b == b'e' || b == b'E')
    {
        out.push_str(".0");
    }
}

fn write_value(out: &mut String, v: &Value) {
    match v {
        Value::String(s) => {
            let _ = write!(out, "{s:?}");
        }
        Value::Int(n) => {
            let _ = write!(out, "{n}");
        }
        Value::Float(n) => write_float_literal(out, *n),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Timestamp(ms) => {
            let _ = write!(out, "timestamp({ms})");
        }
        Value::Now => out.push_str("now"),
        Value::NowMinus(d) => {
            let _ = write!(out, "now - {d}");
        }
        Value::Duration(d) => {
            let _ = write!(out, "{d}");
        }
        Value::Name(n) => out.push_str(n),
        Value::Param(p) => {
            out.push('$');
            out.push_str(p);
        }
    }
}

fn write_pred(out: &mut String, pred: &Pred) {
    match pred {
        Pred::And(a, b) => {
            if matches!(**a, Pred::Or(..)) {
                out.push('(');
                write_pred(out, a);
                out.push(')');
            } else {
                write_pred(out, a);
            }
            out.push_str(" and ");
            if matches!(**b, Pred::Or(..)) {
                out.push('(');
                write_pred(out, b);
                out.push(')');
            } else {
                write_pred(out, b);
            }
        }
        Pred::Or(a, b) => {
            write_pred(out, a);
            out.push_str(" or ");
            write_pred(out, b);
        }
        Pred::Not(inner) => {
            out.push_str("not (");
            write_pred(out, inner);
            out.push(')');
        }
        Pred::Cmp { field, op, value } => {
            let _ = write!(out, "{field} {} ", op.as_str());
            write_value(out, value);
        }
        Pred::IsNull { field, negated } => {
            if *negated {
                let _ = write!(out, "{field} is not null");
            } else {
                let _ = write!(out, "{field} is null");
            }
        }
        Pred::In { field, values } => {
            let _ = write!(out, "{field} in [");
            for (i, v) in values.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                write_value(out, v);
            }
            out.push(']');
        }
        Pred::Has { field, ci, needle } => {
            if *ci {
                let _ = write!(out, "{field} has ci {needle:?}");
            } else {
                let _ = write!(out, "{field} has {needle:?}");
            }
        }
        Pred::Contains { field, needle } => {
            let _ = write!(out, "{field} ~ {needle:?}");
        }
        Pred::Regex {
            field,
            pattern,
            flags,
        } => {
            let _ = write!(out, "{field} ~ /{pattern}/{flags}");
        }
    }
}

fn write_record(out: &mut String, rec: &Record) {
    out.push_str("{ ");
    for (i, (k, v)) in rec.fields.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(k);
        out.push_str(": ");
        write_value(out, v);
    }
    out.push_str(" }");
}

fn write_bin_step(out: &mut String, step: &BinStep) {
    match step {
        BinStep::Duration(ms) => {
            if *ms % 86_400_000 == 0 && *ms >= 86_400_000 {
                let _ = write!(out, "{}d", ms / 86_400_000);
            } else if *ms % 3_600_000 == 0 && *ms >= 3_600_000 {
                let _ = write!(out, "{}h", ms / 3_600_000);
            } else if *ms % 60_000 == 0 && *ms >= 60_000 {
                let _ = write!(out, "{}m", ms / 60_000);
            } else if *ms % 1_000 == 0 && *ms >= 1_000 {
                let _ = write!(out, "{}s", ms / 1_000);
            } else {
                let _ = write!(out, "{ms}");
            }
        }
        BinStep::Int(n) => {
            let _ = write!(out, "{n}");
        }
        BinStep::Float(f) => write_float_literal(out, *f),
    }
}

fn write_group_key(out: &mut String, gk: &GroupKey) {
    match &gk.expr {
        GroupExpr::Field(f) => {
            if gk.alias == f.as_str() {
                let _ = write!(out, "{f}");
            } else {
                let _ = write!(out, "{}: {f}", gk.alias);
            }
        }
        GroupExpr::Bin { field, step } => {
            let _ = write!(out, "{}: bin({field}, ", gk.alias);
            write_bin_step(out, step);
            out.push(')');
        }
    }
}

fn write_agg_spec(out: &mut String, spec: &AggSpec) {
    out.push_str(&spec.alias);
    out.push_str(": ");
    match &spec.func {
        AggFunc::Count => out.push_str("count()"),
        AggFunc::Sum(f) => {
            let _ = write!(out, "sum({f})");
        }
        AggFunc::Avg(f) => {
            let _ = write!(out, "avg({f})");
        }
        AggFunc::Min(f) => {
            let _ = write!(out, "min({f})");
        }
        AggFunc::Max(f) => {
            let _ = write!(out, "max({f})");
        }
        AggFunc::DCount(f) => {
            let _ = write!(out, "dcount({f})");
        }
        AggFunc::Percentile { field, pct } => {
            let _ = write!(out, "percentile({field}, ");
            write_float_literal(out, *pct);
            out.push(')');
        }
        AggFunc::ArgMax { order, value } => {
            let _ = write!(out, "arg_max({order}, {value})");
        }
        AggFunc::ArgMin { order, value } => {
            let _ = write!(out, "arg_min({order}, {value})");
        }
    }
}

fn write_by_list(out: &mut String, by: &[GroupKey]) {
    if by.is_empty() {
        return;
    }
    out.push_str(" by [");
    for (i, g) in by.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        write_group_key(out, g);
    }
    out.push(']');
}

fn write_hop(out: &mut String, h: &MatchHop) {
    out.push_str(if h.reverse { "<-[" } else { "-[" });
    if let Some(e) = h.edge.as_deref() {
        out.push_str(e);
        out.push(':');
    }
    out.push_str(&h.rel);
    if !(h.min_depth == 1 && h.max_depth == 1) {
        if h.min_depth == h.max_depth {
            let _ = write!(out, "*{}", h.min_depth);
        } else {
            let _ = write!(out, "*{}..{}", h.min_depth, h.max_depth);
        }
    }
    out.push_str(if h.reverse { "]- " } else { "]-> " });
    out.push_str(&h.bind);
}

fn write_step(out: &mut String, step: &Step) {
    match step {
        Step::Filter(p) => {
            out.push_str("where ");
            write_pred(out, p);
        }
        Step::Project(fields) => {
            out.push_str("pick { ");
            for (i, f) in fields.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                let _ = write!(out, "{f}");
            }
            out.push_str(" }");
        }
        Step::Without(fields) => {
            out.push_str("without { ");
            for (i, f) in fields.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                let _ = write!(out, "{f}");
            }
            out.push_str(" }");
        }
        Step::Join {
            left,
            collection,
            on,
        } => {
            if *left {
                let _ = write!(out, "join left {collection} on {on}");
            } else {
                let _ = write!(out, "join {collection} on {on}");
            }
        }
        Step::Hop { rel, depth } => match depth {
            Some(d) => {
                let _ = write!(out, "hop {rel} depth={d}");
            }
            None => {
                let _ = write!(out, "hop {rel}");
            }
        },
        Step::Graph { rel, depth } => match depth {
            Some(d) => {
                let _ = write!(out, "graph {rel} depth={d}");
            }
            None => {
                let _ = write!(out, "graph {rel}");
            }
        },
        Step::Match { start, hops } => {
            out.push_str("match ");
            if let Some(s) = start {
                out.push_str(s);
                out.push(' ');
            }
            for (i, h) in hops.iter().enumerate() {
                if i > 0 {
                    out.push(' ');
                }
                write_hop(out, h);
            }
        }
        Step::Search { mode, query } => match mode {
            SearchMode::Hybrid => {
                let _ = write!(out, "search {query:?}");
            }
            SearchMode::Lex => {
                let _ = write!(out, "search lex {query:?}");
            }
            SearchMode::Vec => {
                let _ = write!(out, "search vec {query:?}");
            }
        },
        Step::Count { by } => match by {
            Some(f) => {
                let _ = write!(out, "count by {f}");
            }
            None => out.push_str("count"),
        },
        Step::Sum { field, by } => match by {
            Some(b) => {
                let _ = write!(out, "sum {field} by {b}");
            }
            None => {
                let _ = write!(out, "sum {field}");
            }
        },
        Step::Min { field, by } => match by {
            Some(b) => {
                let _ = write!(out, "min {field} by {b}");
            }
            None => {
                let _ = write!(out, "min {field}");
            }
        },
        Step::Max { field, by } => match by {
            Some(b) => {
                let _ = write!(out, "max {field} by {b}");
            }
            None => {
                let _ = write!(out, "max {field}");
            }
        },
        Step::Avg { field, by } => match by {
            Some(b) => {
                let _ = write!(out, "avg {field} by {b}");
            }
            None => {
                let _ = write!(out, "avg {field}");
            }
        },
        Step::Summarize { specs, by } => {
            out.push_str("summarize { ");
            for (i, spec) in specs.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                write_agg_spec(out, spec);
            }
            out.push_str(" }");
            write_by_list(out, by);
        }
        Step::ArgExtremum { is_max, order, by } => {
            let kw = if *is_max { "arg_max" } else { "arg_min" };
            let _ = write!(out, "{kw}({order}, *)");
            write_by_list(out, by);
        }
        Step::Sort { field, desc } => {
            let dir = if *desc { "desc" } else { "asc" };
            let _ = write!(out, "sort {field} {dir}");
        }
        Step::Window {
            func,
            field,
            by,
            order,
            frame,
            as_field,
        } => {
            let _ = write!(out, "window {func}");
            if let Some(f) = field {
                let _ = write!(out, " ({f})");
            } else if matches!(func, crate::window::WindowFunction::Count) {
                out.push_str(" (*)");
            }
            if !by.is_empty() {
                out.push_str(" by [");
                for (i, b) in by.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    let _ = write!(out, "{b}");
                }
                out.push(']');
            }
            if !order.is_empty() {
                out.push_str(" sort [");
                for (i, o) in order.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    let _ = write!(out, "{} {}", o.field, if o.desc { "desc" } else { "asc" });
                }
                out.push(']');
            }
            if let Some(fr) = frame {
                let _ = write!(out, " {fr}");
            }
            let _ = write!(out, " as {as_field}");
        }
        Step::Skip { n } => {
            let _ = write!(out, "skip {n}");
        }
        Step::Take { n } => match n {
            Some(k) => {
                let _ = write!(out, "take {k}");
            }
            None => out.push_str("take all"),
        },
        Step::Union(rhs) => {
            out.push_str("union (");
            write_query(out, rhs);
            out.push(')');
        }
    }
}

fn write_query(out: &mut String, q: &Query) {
    match &q.source {
        Source::Collection(c) => out.push_str(c),
        Source::Page(p) => {
            let _ = write!(out, "page {p:?}");
        }
        Source::Catalog => out.push_str("catalog"),
    }
    if q.ignore_filter {
        out.push_str(" all");
    }
    for s in &q.steps {
        out.push_str(" | ");
        write_step(out, s);
    }
}

fn fmt_query(q: &Query) -> String {
    let mut out = String::with_capacity(64 + q.steps.len() * 24);
    write_query(&mut out, q);
    out
}

fn write_type_expr(out: &mut String, t: &TypeExpr) {
    match t {
        TypeExpr::Named(n) => out.push_str(n),
        TypeExpr::Vec { dim, model } => {
            if model.is_empty() {
                let _ = write!(out, "vec[{dim}]");
            } else {
                let _ = write!(out, "vec[{dim}, {model:?}]");
            }
        }
    }
}

fn fmt_decl(d: &Decl) -> String {
    let mut out = String::with_capacity(64);
    match d {
        Decl::Col {
            name,
            append,
            fields,
        } => {
            let kw = if *append { "append col" } else { "col" };
            let _ = write!(out, "{kw} {name} {{ ");
            for (i, (k, t)) in fields.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                out.push_str(k);
                out.push_str(": ");
                write_type_expr(&mut out, t);
            }
            out.push_str(" }");
        }
        Decl::Rel { name, stub } => {
            if *stub {
                let _ = write!(out, "rel stub {name}");
            } else {
                let _ = write!(out, "rel {name}");
            }
        }
        Decl::RelReverse { name, of } => {
            let _ = write!(out, "rel {name} reverse {of}");
        }
        Decl::Fk {
            from_col,
            from_field,
            to_col,
            to_field,
        } => {
            let _ = write!(out, "fk {from_col}.{from_field} -> {to_col}.{to_field}");
        }
        Decl::Guard {
            collection,
            field,
            pred,
        } => {
            let _ = write!(out, "guard {collection}.{field} [");
            write_pred(&mut out, pred);
            out.push(']');
        }
        Decl::Filter {
            collection,
            pred: _,
            src,
        } => {
            if src.is_empty() {
                let _ = write!(out, "filter {collection} off");
            } else {
                let _ = write!(out, "filter {collection} [{src}]");
            }
        }
        Decl::Index {
            collection,
            unique,
            fields,
            pred,
        } => {
            let uniq = if *unique { "unique " } else { "" };
            let _ = write!(out, "index {uniq}{collection} [");
            for (i, f) in fields.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                out.push_str(f);
            }
            out.push(']');
            if let Some(p) = pred {
                out.push_str(" where ");
                write_pred(&mut out, p);
            }
        }
        Decl::Owned { name, fields } => {
            let _ = write!(out, "owned {name} {{ ");
            for (i, (k, t)) in fields.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                out.push_str(k);
                out.push_str(": ");
                write_type_expr(&mut out, t);
            }
            out.push_str(" }");
        }
    }
    out
}

fn write_cas(out: &mut String, cas: Option<&str>, cas_each: bool) {
    if cas_each {
        out.push_str(" cas each");
    } else if let Some(c) = cas {
        let _ = write!(out, " cas {c:?}");
    }
}

fn fmt_insert(collection: &str, records: &[Record]) -> String {
    let mut out = String::with_capacity(16 + collection.len() + records.len() * 32);
    out.push_str("insert ");
    out.push_str(collection);
    out.push_str(" [");
    for (i, rec) in records.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        write_record(&mut out, rec);
    }
    out.push(']');
    out
}

fn fmt_insert_cow(collection: &str, records: &[Cow<'_, Record>]) -> String {
    let mut out = String::with_capacity(16 + collection.len() + records.len() * 32);
    out.push_str("insert ");
    out.push_str(collection);
    out.push_str(" [");
    for (i, rec) in records.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        write_record(&mut out, rec.as_ref());
    }
    out.push(']');
    out
}

fn fmt_update(
    collection: &str,
    pred: Option<&Pred>,
    cas: Option<&str>,
    cas_each: bool,
    record: &Record,
) -> String {
    let mut out = String::with_capacity(48 + collection.len() + record.fields.len() * 24);
    out.push_str("update ");
    out.push_str(collection);
    if let Some(p) = pred {
        out.push_str(" [");
        write_pred(&mut out, p);
        out.push(']');
    }
    write_cas(&mut out, cas, cas_each);
    out.push(' ');
    write_record(&mut out, record);
    out
}

fn fmt_delete(collection: &str, pred: &Pred, cas: Option<&str>, cas_each: bool) -> String {
    let mut out = String::with_capacity(32 + collection.len());
    out.push_str("delete ");
    out.push_str(collection);
    out.push_str(" [");
    write_pred(&mut out, pred);
    out.push(']');
    write_cas(&mut out, cas, cas_each);
    out
}

// =========================================================================
// ShardRouter: DDL broadcast, DML routing, columnar insert_batch, query plans A–D
// =========================================================================

/// Parallel coordinator routing LinQL DDL/DML and Kusto/OLAP queries across N shards.
pub struct ShardRouter {
    config: ShardConfig,
    shards: Vec<ServerClient>,
    coord_db: Mutex<Db>,
    quotas: Quotas,
    next_id: AtomicU64,
}

impl ShardRouter {
    pub fn connect(config: ShardConfig) -> Result<Self, Error> {
        if config.shard_addrs.is_empty() {
            return Err(Error::runtime("ShardConfig requires at least one shard address"));
        }
        let mut shards = Vec::with_capacity(config.shard_addrs.len());
        for addr in &config.shard_addrs {
            shards.push(ServerClient::connect(addr.clone(), config.client.clone())?);
        }
        Ok(Self {
            config,
            shards,
            coord_db: Mutex::new(Db::empty()),
            quotas: Quotas::default(),
            next_id: AtomicU64::new(1),
        })
    }

    pub fn num_shards(&self) -> usize {
        self.shards.len()
    }

    pub fn shard_clients(&self) -> &[ServerClient] {
        &self.shards
    }

    /// Columnar `insert_batch` scattering across shards via [`Column::gather`].
    pub fn insert_batch(
        &self,
        collection: &str,
        batch: &RecordBatch,
    ) -> Result<(ExecutionReceipt, ClusterStats), Error> {
        let t0 = Instant::now();
        let n = batch.n();
        if n == 0 {
            return Ok((
                ExecutionReceipt {
                    r#gen: self.max_shard_gen(),
                    affected: 0,
                    elapsed_us: 0,
                    group_size: 1,
                    batch: None,
                    message: None,
                },
                ClusterStats {
                    shards_queried: 0,
                    total_shards: self.shards.len(),
                    max_gen: self.max_shard_gen(),
                    elapsed_us: 0,
                },
            ));
        }

        let explicit_key = self.config.shard_key_for(collection);
        let key_col_idx = match explicit_key {
            Some(k) => Some(batch.names.iter().position(|name| name == k).ok_or_else(|| {
                Error::runtime(format!(
                    "insert_batch missing shard key column {k:?} for collection {collection:?}"
                ))
            })?),
            None => batch.names.iter().position(|name| name == "id"),
        };

        let num_shards = self.shards.len();
        if num_shards == 1 {
            let r = self.shards[0].insert_batch(collection, batch)?;
            let elapsed_us = u64::try_from(t0.elapsed().as_micros()).unwrap_or(u64::MAX);
            return Ok((
                ExecutionReceipt {
                    r#gen: r.r#gen,
                    affected: r.affected,
                    elapsed_us,
                    group_size: r.group_size,
                    batch: None,
                    message: None,
                },
                ClusterStats {
                    shards_queried: 1,
                    total_shards: 1,
                    max_gen: r.r#gen,
                    elapsed_us,
                },
            ));
        }

        let est_cap = (n / num_shards).saturating_add(16);
        let mut per_shard: Vec<Vec<u32>> = (0..num_shards)
            .map(|_| Vec::with_capacity(est_cap))
            .collect();
        match key_col_idx {
            Some(cidx) => {
                let col = &batch.cols[cidx];
                let strategy = self.config.strategy;
                let null_shard = pick_shard(hash_bytes_tagged(6, &[]), num_shards, strategy);
                match col {
                    Column::Nulls { .. } => {
                        let r = self.shards[null_shard].insert_batch(collection, batch)?;
                        let elapsed_us =
                            u64::try_from(t0.elapsed().as_micros()).unwrap_or(u64::MAX);
                        return Ok((
                            ExecutionReceipt {
                                r#gen: r.r#gen,
                                affected: r.affected,
                                elapsed_us,
                                group_size: r.group_size,
                                batch: None,
                                message: None,
                            },
                            ClusterStats {
                                shards_queried: 1,
                                total_shards: num_shards,
                                max_gen: r.r#gen,
                                elapsed_us,
                            },
                        ));
                    }
                    Column::Int { vals, valid } => {
                        if valid.null_count() == 0 {
                            for (row_idx, &v) in vals.iter().enumerate() {
                                per_shard[pick_shard(hash_i64(v), num_shards, strategy)]
                                    .push(row_idx as u32);
                            }
                        } else {
                            for (row_idx, &v) in vals.iter().enumerate() {
                                let s = if valid.is_valid(row_idx) {
                                    pick_shard(hash_i64(v), num_shards, strategy)
                                } else {
                                    null_shard
                                };
                                per_shard[s].push(row_idx as u32);
                            }
                        }
                    }
                    Column::Time { vals, valid } => {
                        if valid.null_count() == 0 {
                            for (row_idx, &v) in vals.iter().enumerate() {
                                per_shard
                                    [pick_shard(hash_u64_tagged(3, v as u64), num_shards, strategy)]
                                    .push(row_idx as u32);
                            }
                        } else {
                            for (row_idx, &v) in vals.iter().enumerate() {
                                let s = if valid.is_valid(row_idx) {
                                    pick_shard(hash_u64_tagged(3, v as u64), num_shards, strategy)
                                } else {
                                    null_shard
                                };
                                per_shard[s].push(row_idx as u32);
                            }
                        }
                    }
                    Column::Float { vals, valid } => {
                        if valid.null_count() == 0 {
                            for (row_idx, &v) in vals.iter().enumerate() {
                                per_shard[pick_shard(hash_f64_canonical(v), num_shards, strategy)]
                                    .push(row_idx as u32);
                            }
                        } else {
                            for (row_idx, &v) in vals.iter().enumerate() {
                                let s = if valid.is_valid(row_idx) {
                                    pick_shard(hash_f64_canonical(v), num_shards, strategy)
                                } else {
                                    null_shard
                                };
                                per_shard[s].push(row_idx as u32);
                            }
                        }
                    }
                    Column::Bool { vals, valid } => {
                        let bool_shards = [
                            pick_shard(hash_u64_tagged(4, 0), num_shards, strategy),
                            pick_shard(hash_u64_tagged(4, 1), num_shards, strategy),
                        ];
                        if valid.null_count() == 0 {
                            for (row_idx, &b) in vals.iter().enumerate() {
                                per_shard[bool_shards[usize::from(b)]].push(row_idx as u32);
                            }
                        } else {
                            for (row_idx, &b) in vals.iter().enumerate() {
                                let s = if valid.is_valid(row_idx) {
                                    bool_shards[usize::from(b)]
                                } else {
                                    null_shard
                                };
                                per_shard[s].push(row_idx as u32);
                            }
                        }
                    }
                    Column::Text {
                        offsets,
                        data,
                        valid,
                    } => {
                        if valid.null_count() == 0 {
                            for row_idx in 0..n {
                                let start = offsets[row_idx] as usize;
                                let end = offsets[row_idx + 1] as usize;
                                let s = pick_shard(
                                    hash_bytes_tagged(5, &data[start..end]),
                                    num_shards,
                                    strategy,
                                );
                                per_shard[s].push(row_idx as u32);
                            }
                        } else {
                            for row_idx in 0..n {
                                let s = if valid.is_valid(row_idx) {
                                    let start = offsets[row_idx] as usize;
                                    let end = offsets[row_idx + 1] as usize;
                                    pick_shard(
                                        hash_bytes_tagged(5, &data[start..end]),
                                        num_shards,
                                        strategy,
                                    )
                                } else {
                                    null_shard
                                };
                                per_shard[s].push(row_idx as u32);
                            }
                        }
                    }
                    Column::Dict {
                        values,
                        codes,
                        valid,
                    } => {
                        let dict_shards: SmallVec<[usize; 32]> = values
                            .iter()
                            .map(|s| {
                                pick_shard(hash_bytes_tagged(5, s.as_bytes()), num_shards, strategy)
                            })
                            .collect();
                        if valid.null_count() == 0 {
                            for (row_idx, &code) in codes.iter().enumerate() {
                                per_shard[dict_shards[code as usize]].push(row_idx as u32);
                            }
                        } else {
                            for (row_idx, &code) in codes.iter().enumerate() {
                                let s = if valid.is_valid(row_idx) {
                                    dict_shards[code as usize]
                                } else {
                                    null_shard
                                };
                                per_shard[s].push(row_idx as u32);
                            }
                        }
                    }
                    Column::Cells(cells) => {
                        for (row_idx, cell) in cells.iter().enumerate() {
                            let s = pick_shard(shard_hash_cell(cell), num_shards, strategy);
                            per_shard[s].push(row_idx as u32);
                        }
                    }
                }
            }
            None => {
                for row_idx in 0..n {
                    per_shard[row_idx % num_shards].push(row_idx as u32);
                }
            }
        }

        let non_empty: Vec<(usize, Vec<u32>)> = per_shard
            .into_iter()
            .enumerate()
            .filter(|(_, idx)| !idx.is_empty())
            .collect();
        let shards_queried = non_empty.len();

        // Single target shard fast path: all `n` rows map to this shard in order `0..n`,
        // so skip both `Column::gather` and `thread::scope`!
        if let [(shard_idx, _)] = non_empty.as_slice() {
            let r = self.shards[*shard_idx].insert_batch(collection, batch)?;
            let elapsed_us = u64::try_from(t0.elapsed().as_micros()).unwrap_or(u64::MAX);
            return Ok((
                ExecutionReceipt {
                    r#gen: r.r#gen,
                    affected: r.affected,
                    elapsed_us,
                    group_size: r.group_size,
                    batch: None,
                    message: None,
                },
                ClusterStats {
                    shards_queried: 1,
                    total_shards: num_shards,
                    max_gen: r.r#gen,
                    elapsed_us,
                },
            ));
        }

        let send_sub_batch = |shard_idx: usize, idx: &[u32]| -> Result<ExecutionReceipt, Error> {
            let cols: Vec<Column> = batch.cols.iter().map(|c| c.gather(idx)).collect();
            let sub = RecordBatch {
                names: Arc::clone(&batch.names),
                cols,
            };
            self.shards[shard_idx].insert_batch(collection, &sub)
        };

        let results: Vec<Result<ExecutionReceipt, Error>> = thread::scope(|scope| {
            let (last, rest) = non_empty.split_last().unwrap();
            let mut handles = Vec::with_capacity(rest.len());
            for (shard_idx, idx) in rest {
                let sidx = *shard_idx;
                handles.push(scope.spawn(move || send_sub_batch(sidx, idx)));
            }
            let last_res = send_sub_batch(last.0, &last.1);
            let mut out: Vec<Result<ExecutionReceipt, Error>> = handles
                .into_iter()
                .map(|h| {
                    h.join()
                        .unwrap_or_else(|_| Err(Error::runtime("shard insert_batch worker panicked")))
                })
                .collect();
            out.push(last_res);
            out
        });

        let mut total_affected = 0usize;
        let mut max_gen = 0u64;
        let mut max_group = 1usize;
        for res in results {
            let r = res?;
            total_affected = total_affected.saturating_add(r.affected);
            max_gen = max_gen.max(r.r#gen);
            max_group = max_group.max(r.group_size);
        }
        let elapsed_us = u64::try_from(t0.elapsed().as_micros()).unwrap_or(u64::MAX);
        Ok((
            ExecutionReceipt {
                r#gen: max_gen,
                affected: total_affected,
                elapsed_us,
                group_size: max_group,
                batch: None,
                message: None,
            },
            ClusterStats {
                shards_queried,
                total_shards: self.shards.len(),
                max_gen,
                elapsed_us,
            },
        ))
    }

    /// Execute a LinQL script or statement (DDL, DML, or distributed query) across shards.
    pub fn execute(&self, linql: &str) -> Result<ExecutionReceipt, Error> {
        let stmts = crate::parse::parse_program(linql)?;
        if stmts.len() == 1
            && let Stmt::Query(q) = &stmts[0]
        {
            let (batch, stats) = self.query_struct_with_stats(q)?;
            return Ok(ExecutionReceipt {
                r#gen: stats.max_gen,
                affected: 0,
                elapsed_us: stats.elapsed_us,
                group_size: 1,
                batch: Some(batch),
                message: None,
            });
        }

        let t0 = Instant::now();
        let mut last = ExecutionReceipt {
            r#gen: 0,
            affected: 0,
            elapsed_us: 0,
            group_size: 1,
            batch: None,
            message: None,
        };
        for stmt in &stmts {
            last = self.execute_single_stmt(stmt, (stmts.len() == 1).then_some(linql))?;
        }
        last.elapsed_us = u64::try_from(t0.elapsed().as_micros()).unwrap_or(u64::MAX);
        Ok(last)
    }

    /// Query across shards returning both the columnar [`RecordBatch`] and [`ClusterStats`].
    pub fn query_with_stats(&self, linql: &str) -> Result<(RecordBatch, ClusterStats), Error> {
        let stmt = crate::parse::parse(linql)?;
        let Stmt::Query(q) = stmt else {
            return Err(Error::runtime("query_with_stats requires a query statement"));
        };
        self.query_struct_with_stats(&q)
    }

    /// Execute a LinQL query across shards returning a columnar [`RecordBatch`].
    pub fn query(&self, linql: &str) -> Result<RecordBatch, Error> {
        Ok(self.query_with_stats(linql)?.0)
    }

    /// Execute a LinQL query across shards returning materialized [`Row`]s.
    pub fn query_rows(&self, linql: &str) -> Result<(Vec<Row>, ClusterStats), Error> {
        let stmt = crate::parse::parse(linql)?;
        let Stmt::Query(q) = stmt else {
            return Err(Error::runtime("query_rows requires a query statement"));
        };
        let (rows, stats, _) = self.exec_query_internal(&q)?;
        Ok((rows, stats))
    }

    fn max_shard_gen(&self) -> u64 {
        self.shards.iter().map(|s| s.session_gen()).max().unwrap_or(0)
    }

    fn execute_single_stmt(
        &self,
        stmt: &Stmt,
        raw_single: Option<&str>,
    ) -> Result<ExecutionReceipt, Error> {
        match stmt {
            Stmt::Decl(decl) => {
                let dsl = raw_single.map(str::to_string).unwrap_or_else(|| fmt_decl(decl));
                if let Ok(mut db) = self.coord_db.lock() {
                    let _ = db.run(&dsl);
                }
                self.broadcast_all(&dsl)
            }
            Stmt::Insert {
                collection,
                records,
                edges,
            } => {
                if !edges.is_empty() && self.shards.len() > 1 {
                    return Err(Error::runtime(
                        "sharded insert with graph edges across multiple shards is unsupported",
                    ));
                }
                if !edges.is_empty() {
                    let mut out = fmt_insert(collection, records);
                    for e in edges {
                        let _ = write!(out, " -> {} ", e.rel);
                        match &e.target {
                            EdgeTarget::Page(p) => {
                                let _ = write!(out, "{p:?}");
                            }
                            EdgeTarget::Value(v) => write_value(&mut out, v),
                        }
                    }
                    return self.shards[0].execute(&out);
                }
                self.exec_sharded_insert(collection, records)
            }
            Stmt::Update {
                collection,
                pred,
                cas,
                cas_each,
                record,
            } => {
                if let Some(sk) = self.config.shard_key_for(collection)
                    && record.fields.iter().any(|(k, _)| k == sk)
                {
                    return Err(Error::runtime(format!(
                        "cannot update shard key field {sk:?} on sharded collection {collection:?}"
                    )));
                }
                let target = target_shards_for_pred(
                    pred.as_ref(),
                    self.config.shard_key_for(collection),
                    self.shards.len(),
                    self.config.strategy,
                );
                let dsl = fmt_update(collection, pred.as_ref(), cas.as_deref(), *cas_each, record);
                self.fanout_write(&target, &dsl)
            }
            Stmt::Delete {
                collection,
                pred,
                cas,
                cas_each,
            } => {
                let target = target_shards_for_pred(
                    Some(pred),
                    self.config.shard_key_for(collection),
                    self.shards.len(),
                    self.config.strategy,
                );
                let dsl = fmt_delete(collection, pred, cas.as_deref(), *cas_each);
                self.fanout_write(&target, &dsl)
            }
            Stmt::Query(q) => {
                let (batch, stats) = self.query_struct_with_stats(q)?;
                Ok(ExecutionReceipt {
                    r#gen: stats.max_gen,
                    affected: 0,
                    elapsed_us: stats.elapsed_us,
                    group_size: 1,
                    batch: Some(batch),
                    message: None,
                })
            }
            other => {
                if self.shards.len() == 1 {
                    let dsl = raw_single.ok_or_else(|| {
                        Error::runtime(format!("unsupported sharded statement {other:?}"))
                    })?;
                    return self.shards[0].execute(dsl);
                }
                Err(Error::runtime(format!(
                    "statement {other:?} requires unsharded or single-shard target"
                )))
            }
        }
    }

    fn exec_sharded_insert<'a>(
        &self,
        collection: &str,
        records: &'a [Record],
    ) -> Result<ExecutionReceipt, Error> {
        if records.is_empty() {
            return Ok(ExecutionReceipt {
                r#gen: self.max_shard_gen(),
                affected: 0,
                elapsed_us: 0,
                group_size: 1,
                batch: None,
                message: None,
            });
        }
        let shard_key = self.config.shard_key_for(collection);
        let num_shards = self.shards.len();
        let est_cap = (records.len() / num_shards).saturating_add(4);
        let mut buckets: Vec<Vec<Cow<'a, Record>>> =
            (0..num_shards).map(|_| Vec::with_capacity(est_cap)).collect();

        for (idx, rec) in records.iter().enumerate() {
            let (shard, item): (usize, Cow<'a, Record>) = if let Some(sk) = shard_key {
                if let Some((_, val)) = rec.fields.iter().find(|(k, _)| k == sk) {
                    match shard_hash_value(val) {
                        Some(h) => (pick_shard(h, num_shards, self.config.strategy), Cow::Borrowed(rec)),
                        None => {
                            return Err(Error::runtime(format!(
                                "shard key field {sk:?} must have literal value on insert"
                            )));
                        }
                    }
                } else if sk == "id" {
                    let gen_id = format!(
                        "{}_{}",
                        self.config.id_prefix,
                        self.next_id.fetch_add(1, Ordering::Relaxed)
                    );
                    let val = Value::String(Arc::from(gen_id));
                    let h = shard_hash_value(&val).unwrap_or(0);
                    let mut rec_owned = rec.clone();
                    rec_owned.fields.push(("id".to_string(), val));
                    (
                        pick_shard(h, num_shards, self.config.strategy),
                        Cow::Owned(rec_owned),
                    )
                } else {
                    return Err(Error::runtime(format!(
                        "insert missing shard key field {sk:?} for collection {collection:?}"
                    )));
                }
            } else if let Some((_, val)) = rec.fields.iter().find(|(k, _)| k == "id") {
                let s = match shard_hash_value(val) {
                    Some(h) => pick_shard(h, num_shards, self.config.strategy),
                    None => idx % num_shards,
                };
                (s, Cow::Borrowed(rec))
            } else {
                (idx % num_shards, Cow::Borrowed(rec))
            };
            buckets[shard].push(item);
        }

        let work: Vec<(usize, String)> = buckets
            .into_iter()
            .enumerate()
            .filter(|(_, recs)| !recs.is_empty())
            .map(|(s, recs)| (s, fmt_insert_cow(collection, &recs)))
            .collect();

        if let [(shard_idx, dsl)] = work.as_slice() {
            let mut r = self.shards[*shard_idx].execute(dsl)?;
            r.elapsed_us = 0;
            r.batch = None;
            r.message = None;
            return Ok(r);
        }

        let results: Vec<Result<ExecutionReceipt, Error>> = thread::scope(|scope| {
            let (last, rest) = work.split_last().unwrap();
            let mut handles = Vec::with_capacity(rest.len());
            for (shard_idx, dsl) in rest {
                let client = &self.shards[*shard_idx];
                handles.push(scope.spawn(move || client.execute(dsl)));
            }
            let last_res = self.shards[last.0].execute(&last.1);
            let mut out: Vec<Result<ExecutionReceipt, Error>> = handles
                .into_iter()
                .map(|h| {
                    h.join()
                        .unwrap_or_else(|_| Err(Error::runtime("shard insert worker panicked")))
                })
                .collect();
            out.push(last_res);
            out
        });

        let mut total_affected = 0usize;
        let mut max_gen = 0u64;
        let mut max_group = 1usize;
        for res in results {
            let r = res?;
            total_affected = total_affected.saturating_add(r.affected);
            max_gen = max_gen.max(r.r#gen);
            max_group = max_group.max(r.group_size);
        }
        Ok(ExecutionReceipt {
            r#gen: max_gen,
            affected: total_affected,
            elapsed_us: 0,
            group_size: max_group,
            batch: None,
            message: None,
        })
    }

    fn broadcast_all(&self, dsl: &str) -> Result<ExecutionReceipt, Error> {
        let targets: Vec<usize> = (0..self.shards.len()).collect();
        self.fanout_write(&targets, dsl)
    }

    fn fanout_write(&self, targets: &[usize], dsl: &str) -> Result<ExecutionReceipt, Error> {
        if targets.is_empty() {
            return Ok(ExecutionReceipt {
                r#gen: self.max_shard_gen(),
                affected: 0,
                elapsed_us: 0,
                group_size: 1,
                batch: None,
                message: None,
            });
        }
        if let [only] = targets {
            let mut r = self.shards[*only].execute(dsl)?;
            r.elapsed_us = 0;
            r.batch = None;
            r.message = None;
            return Ok(r);
        }
        let results: Vec<Result<ExecutionReceipt, Error>> = thread::scope(|scope| {
            let (last, rest) = targets.split_last().unwrap();
            let mut handles = Vec::with_capacity(rest.len());
            for &shard_idx in rest {
                let client = &self.shards[shard_idx];
                handles.push(scope.spawn(move || client.execute(dsl)));
            }
            let last_res = self.shards[*last].execute(dsl);
            let mut out: Vec<Result<ExecutionReceipt, Error>> = handles
                .into_iter()
                .map(|h| {
                    h.join()
                        .unwrap_or_else(|_| Err(Error::runtime("shard write worker panicked")))
                })
                .collect();
            out.push(last_res);
            out
        });

        let mut total_affected = 0usize;
        let mut max_gen = 0u64;
        let mut max_group = 1usize;
        for res in results {
            let r = res?;
            total_affected = total_affected.saturating_add(r.affected);
            max_gen = max_gen.max(r.r#gen);
            max_group = max_group.max(r.group_size);
        }
        Ok(ExecutionReceipt {
            r#gen: max_gen,
            affected: total_affected,
            elapsed_us: 0,
            group_size: max_group,
            batch: None,
            message: None,
        })
    }

    fn query_struct_with_stats(&self, q: &Query) -> Result<(RecordBatch, ClusterStats), Error> {
        if let Some(res) = self.try_passthrough_batch_query(q)? {
            return Ok(res);
        }
        let (rows, stats, post_query) = self.exec_query_internal(q)?;
        let batch = rows_to_batch_for_query(&rows, &post_query);
        Ok((batch, stats))
    }

    /// Direct `RecordBatch` passthrough for Plan D queries when `post` is empty or only
    /// `Step::Skip` / `Step::Take`, avoiding `RecordBatch -> Vec<Row> -> RecordBatch` round-trips.
    fn try_passthrough_batch_query(
        &self,
        q: &Query,
    ) -> Result<Option<(RecordBatch, ClusterStats)>, Error> {
        let is_simple_pre_or_slice = q.steps.iter().all(|s| {
            matches!(
                s,
                Step::Filter(_)
                    | Step::Project(_)
                    | Step::Without(_)
                    | Step::Search { .. }
                    | Step::Skip { .. }
                    | Step::Take { .. }
            )
        });
        if !is_simple_pre_or_slice {
            return Ok(None);
        }
        let coord_idx = q
            .steps
            .iter()
            .position(|s| matches!(s, Step::Skip { .. } | Step::Take { .. }))
            .unwrap_or(q.steps.len());
        let pre = &q.steps[..coord_idx];
        let post = &q.steps[coord_idx..];
        if !post
            .iter()
            .all(|s| matches!(s, Step::Skip { .. } | Step::Take { .. }))
        {
            return Ok(None);
        }

        let t0 = Instant::now();
        let col_name = match &q.source {
            Source::Collection(c) => Some(c.as_str()),
            _ => None,
        };
        let shard_key = col_name.and_then(|c| self.config.shard_key_for(c));
        let mut combined_leading_filter: Option<Pred> = None;
        for step in &q.steps {
            match step {
                Step::Filter(p) => {
                    combined_leading_filter = Some(match combined_leading_filter {
                        Some(prev) => prev.and(p.clone()),
                        None => p.clone(),
                    });
                }
                _ => break,
            }
        }
        let target_shards = target_shards_for_pred(
            combined_leading_filter.as_ref(),
            shard_key,
            self.shards.len(),
            self.config.strategy,
        );

        let has_explicit_take = post.iter().any(|s| matches!(s, Step::Take { .. }));
        let mut total_skip: i64 = 0;
        let mut take_limit: Option<i64> = None;
        let mut pure_slice = !post.is_empty();
        for s in post {
            match s {
                Step::Skip { n } if take_limit.is_none() => {
                    total_skip = total_skip.saturating_add((*n).max(0));
                }
                Step::Take { n: Some(k) } => {
                    take_limit = Some(match take_limit {
                        Some(prev) => prev.min((*k).max(0)),
                        None => (*k).max(0),
                    });
                }
                _ => {
                    pure_slice = false;
                    break;
                }
            }
        }

        let shard_steps = if pure_slice {
            let mut s = pre.to_vec();
            if let Some(k) = take_limit {
                s.push(Step::Take {
                    n: Some(total_skip.saturating_add(k)),
                });
            } else if !has_explicit_take {
                s.push(Step::Take {
                    n: Some(total_skip.saturating_add(50)),
                });
            } else {
                s.push(Step::Take { n: None });
            }
            s
        } else if post.is_empty() && !has_explicit_take {
            let mut s = pre.to_vec();
            s.push(Step::Take { n: Some(50) });
            s
        } else {
            let mut s = pre.to_vec();
            s.push(Step::Take { n: None });
            s
        };

        let shard_q = Query {
            source: q.source.clone(),
            steps: shard_steps,
            explain: None,
            ignore_filter: q.ignore_filter,
        };

        let (max_gen, batches) = if target_shards.is_empty() {
            (self.max_shard_gen(), Vec::new())
        } else {
            self.fanout_query_batches(&target_shards, &fmt_query(&shard_q))?
        };

        let mut merged_batch = if batches.is_empty() {
            rows_to_batch_for_query(&[], q)
        } else {
            let non_empty_count = batches.iter().filter(|b| !b.is_empty()).count();
            if non_empty_count == 0 {
                batches
                    .into_iter()
                    .find(|b| !b.names.is_empty())
                    .unwrap_or_else(|| rows_to_batch_for_query(&[], q))
            } else if non_empty_count == 1 {
                batches.into_iter().find(|b| !b.is_empty()).unwrap()
            } else {
                let first_names = batches
                    .iter()
                    .find(|b| !b.is_empty())
                    .map(|b| &b.names)
                    .unwrap();
                let uniform_schema = batches
                    .iter()
                    .filter(|b| !b.is_empty())
                    .all(|b| b.names.as_ref() == first_names.as_ref());
                if uniform_schema {
                    let total_rows: usize = batches.iter().map(RecordBatch::n).sum();
                    let mut iter = batches.into_iter().filter(|b| !b.is_empty());
                    let mut base = iter.next().unwrap();
                    base.reserve_cols(total_rows);
                    for b in iter {
                        for (dst, src) in base.cols.iter_mut().zip(b.cols) {
                            dst.extend(src);
                        }
                    }
                    base
                } else {
                    let mut rows = Vec::new();
                    for b in batches {
                        rows.extend(b.into_rows());
                    }
                    rows_to_batch_for_query(&rows, q)
                }
            }
        };

        for s in post {
            match s {
                Step::Skip { n } => merged_batch.drain_prefix((*n).max(0) as usize),
                Step::Take { n: Some(k) } => merged_batch.truncate((*k).max(0) as usize),
                Step::Take { n: None } => {}
                _ => unreachable!(),
            }
        }
        if !has_explicit_take && merged_batch.n() > 50 {
            merged_batch.truncate(50);
        }

        let elapsed_us = u64::try_from(t0.elapsed().as_micros()).unwrap_or(u64::MAX);
        Ok(Some((
            merged_batch,
            ClusterStats {
                shards_queried: target_shards.len(),
                total_shards: self.shards.len(),
                max_gen,
                elapsed_us,
            },
        )))
    }

    fn exec_query_internal(&self, q: &Query) -> Result<(Vec<Row>, ClusterStats, Query), Error> {
        let t0 = Instant::now();
        let col_name = match &q.source {
            Source::Collection(c) => Some(c.as_str()),
            _ => None,
        };
        let shard_key = col_name.and_then(|c| self.config.shard_key_for(c));

        // Combine leading Filter steps for predicate shard pruning.
        let mut combined_leading_filter: Option<Pred> = None;
        for step in &q.steps {
            match step {
                Step::Filter(p) => {
                    combined_leading_filter = Some(match combined_leading_filter {
                        Some(prev) => prev.and(p.clone()),
                        None => p.clone(),
                    });
                }
                _ => break,
            }
        }
        let target_shards = target_shards_for_pred(
            combined_leading_filter.as_ref(),
            shard_key,
            self.shards.len(),
            self.config.strategy,
        );

        let now = crate::store::now_ms();

        // Check if query starts (after single-row local steps) with an aggregation or ArgExtremum.
        let agg_idx = q.steps.iter().position(|s| {
            matches!(
                s,
                Step::Summarize { .. }
                    | Step::Count { .. }
                    | Step::Sum { .. }
                    | Step::Min { .. }
                    | Step::Max { .. }
                    | Step::Avg { .. }
                    | Step::ArgExtremum { .. }
            )
        });

        let can_push_agg = agg_idx.is_some_and(|idx| {
            q.steps[..idx].iter().all(|s| {
                matches!(
                    s,
                    Step::Filter(_) | Step::Project(_) | Step::Without(_) | Step::Search { .. }
                )
            })
        });

        if can_push_agg && let Some(idx) = agg_idx {
            let pre_steps = &q.steps[..idx];
            let post_steps = &q.steps[idx + 1..];
            match &q.steps[idx] {
                Step::Summarize { specs, by } => {
                    let shard_q = Query {
                        source: q.source.clone(),
                        steps: {
                            let mut s = pre_steps.to_vec();
                            s.push(Step::Summarize {
                                specs: specs.clone(),
                                by: by.clone(),
                            });
                            s
                        },
                        explain: None,
                        ignore_filter: q.ignore_filter,
                    };
                    let sum_flags = self.sum_f64_flags(col_name, specs);
                    let (max_gen, payloads) = if target_shards.is_empty() {
                        (
                            self.max_shard_gen(),
                            vec![empty_partial_summarize_wire(specs, by, &sum_flags)?],
                        )
                    } else {
                        self.fanout_partial_summarize(&target_shards, &fmt_query(&shard_q))?
                    };
                    let mut merged = merge_partial_summarize_wire(
                        &payloads,
                        specs,
                        by,
                        &sum_flags,
                        self.quotas.max_aggregate_bytes,
                    )?;
                    merged =
                        apply_coordinator_post_steps(merged, post_steps, now, &self.quotas)?;
                    let elapsed_us = u64::try_from(t0.elapsed().as_micros()).unwrap_or(u64::MAX);
                    return Ok((
                        merged,
                        ClusterStats {
                            shards_queried: target_shards.len(),
                            total_shards: self.shards.len(),
                            max_gen,
                            elapsed_us,
                        },
                        q.clone(),
                    ));
                }
                Step::Count { by } => {
                    let specs = vec![AggSpec {
                        alias: "hits".to_string(),
                        func: AggFunc::Count,
                    }];
                    let by_keys: Vec<GroupKey> = by.iter().cloned().map(GroupKey::field).collect();
                    return self.run_lowered_summarize_plan(
                        q,
                        pre_steps,
                        post_steps,
                        &specs,
                        &by_keys,
                        &[false],
                        None,
                        &target_shards,
                        now,
                        t0,
                    );
                }
                Step::Sum { field, by } => {
                    let fname = field.as_str();
                    let specs = vec![AggSpec {
                        alias: fname.clone(),
                        func: AggFunc::Sum(field.clone()),
                    }];
                    let by_keys: Vec<GroupKey> = by.iter().cloned().map(GroupKey::field).collect();
                    return self.run_lowered_summarize_plan(
                        q,
                        pre_steps,
                        post_steps,
                        &specs,
                        &by_keys,
                        &[true],
                        Some(&fname),
                        &target_shards,
                        now,
                        t0,
                    );
                }
                Step::Min { field, by } => {
                    let fname = field.as_str();
                    let specs = vec![AggSpec {
                        alias: fname,
                        func: AggFunc::Min(field.clone()),
                    }];
                    let by_keys: Vec<GroupKey> = by.iter().cloned().map(GroupKey::field).collect();
                    return self.run_lowered_summarize_plan(
                        q,
                        pre_steps,
                        post_steps,
                        &specs,
                        &by_keys,
                        &[false],
                        None,
                        &target_shards,
                        now,
                        t0,
                    );
                }
                Step::Max { field, by } => {
                    let fname = field.as_str();
                    let specs = vec![AggSpec {
                        alias: fname,
                        func: AggFunc::Max(field.clone()),
                    }];
                    let by_keys: Vec<GroupKey> = by.iter().cloned().map(GroupKey::field).collect();
                    return self.run_lowered_summarize_plan(
                        q,
                        pre_steps,
                        post_steps,
                        &specs,
                        &by_keys,
                        &[false],
                        None,
                        &target_shards,
                        now,
                        t0,
                    );
                }
                Step::Avg { field, by } => {
                    let fname = field.as_str();
                    let specs = vec![AggSpec {
                        alias: fname,
                        func: AggFunc::Avg(field.clone()),
                    }];
                    let by_keys: Vec<GroupKey> = by.iter().cloned().map(GroupKey::field).collect();
                    return self.run_lowered_summarize_plan(
                        q,
                        pre_steps,
                        post_steps,
                        &specs,
                        &by_keys,
                        &[false],
                        None,
                        &target_shards,
                        now,
                        t0,
                    );
                }
                Step::ArgExtremum { is_max, order, by } => {
                    // Two-stage whole-row extremum: each shard runs local ArgExtremum | take all,
                    // then coordinator merges winners and runs trailing post_steps.
                    let shard_q = Query {
                        source: q.source.clone(),
                        steps: {
                            let mut s = pre_steps.to_vec();
                            s.push(Step::ArgExtremum {
                                is_max: *is_max,
                                order: order.clone(),
                                by: by.clone(),
                            });
                            s.push(Step::Take { n: None });
                            s
                        },
                        explain: None,
                        ignore_filter: q.ignore_filter,
                    };
                    let (max_gen, candidate_rows) = if target_shards.is_empty() {
                        (self.max_shard_gen(), Vec::new())
                    } else {
                        self.fanout_query_rows(&target_shards, &fmt_query(&shard_q))?
                    };
                    let winners = coordinator_agg_arg_extremum(
                        &candidate_rows,
                        *is_max,
                        order,
                        by,
                        self.quotas.max_aggregate_bytes,
                    )?;
                    let rows =
                        apply_coordinator_post_steps(winners, post_steps, now, &self.quotas)?;
                    let elapsed_us = u64::try_from(t0.elapsed().as_micros()).unwrap_or(u64::MAX);
                    return Ok((
                        rows,
                        ClusterStats {
                            shards_queried: target_shards.len(),
                            total_shards: self.shards.len(),
                            max_gen,
                            elapsed_us,
                        },
                        q.clone(),
                    ));
                }
                _ => {}
            }
        }

        // Plan D: Non-aggregated distributed execution (or complex pre-steps fallback).
        let can_split_sort_topk = q.steps.iter().all(|s| {
            matches!(
                s,
                Step::Filter(_)
                    | Step::Project(_)
                    | Step::Without(_)
                    | Step::Search { .. }
                    | Step::Sort { .. }
                    | Step::Skip { .. }
                    | Step::Take { .. }
                    | Step::Window { .. }
            )
        });

        if !can_split_sort_topk {
            return Err(Error::runtime(
                "distributed query shape with joins/graph/union requires shard-local or single-shard target",
            ));
        }

        let has_explicit_take = q.steps.iter().any(|s| matches!(s, Step::Take { .. }));
        let coord_idx = q
            .steps
            .iter()
            .position(|s| {
                matches!(
                    s,
                    Step::Sort { .. } | Step::Skip { .. } | Step::Take { .. } | Step::Window { .. }
                )
            })
            .unwrap_or(q.steps.len());

        let pre = &q.steps[..coord_idx];
        let post = &q.steps[coord_idx..];

        // Check for pure top-k pushdown: post is ONLY Sort + optional Skip + Take(Some(k)).
        let has_window = post.iter().any(|s| matches!(s, Step::Window { .. }));
        let mut sort_step: Option<&Step> = None;
        let mut total_skip: i64 = 0;
        let mut take_limit: Option<i64> = None;
        let mut pure_sort_topk = !has_window && !post.is_empty();
        for s in post {
            match s {
                Step::Sort { .. } if sort_step.is_none() && total_skip == 0 && take_limit.is_none() => {
                    sort_step = Some(s);
                }
                Step::Skip { n } if take_limit.is_none() => {
                    total_skip = total_skip.saturating_add((*n).max(0));
                }
                Step::Take { n: Some(k) } => {
                    take_limit = Some(match take_limit {
                        Some(prev) => prev.min((*k).max(0)),
                        None => (*k).max(0),
                    });
                }
                _ => {
                    pure_sort_topk = false;
                    break;
                }
            }
        }

        let shard_steps = if pure_sort_topk {
            let mut s = pre.to_vec();
            if let Some(sort) = sort_step {
                s.push(sort.clone());
            }
            if let Some(k) = take_limit {
                s.push(Step::Take {
                    n: Some(total_skip.saturating_add(k)),
                });
            } else if !has_explicit_take {
                s.push(Step::Take {
                    n: Some(total_skip.saturating_add(50)),
                });
            } else {
                s.push(Step::Take { n: None });
            }
            s
        } else if post.is_empty() && !has_explicit_take {
            let mut s = pre.to_vec();
            s.push(Step::Take { n: Some(50) });
            s
        } else {
            // Window or complex tail or explicit `take all`: pull all matching rows from shards!
            let mut s = pre.to_vec();
            s.push(Step::Take { n: None });
            s
        };

        let shard_q = Query {
            source: q.source.clone(),
            steps: shard_steps,
            explain: None,
            ignore_filter: q.ignore_filter,
        };
        let (max_gen, mut rows) = if target_shards.is_empty() {
            (self.max_shard_gen(), Vec::new())
        } else {
            self.fanout_query_rows(&target_shards, &fmt_query(&shard_q))?
        };

        rows = apply_coordinator_post_steps(rows, post, now, &self.quotas)?;
        if !has_explicit_take && rows.len() > 50 {
            rows.truncate(50);
        }

        let elapsed_us = u64::try_from(t0.elapsed().as_micros()).unwrap_or(u64::MAX);
        Ok((
            rows,
            ClusterStats {
                shards_queried: target_shards.len(),
                total_shards: self.shards.len(),
                max_gen,
                elapsed_us,
            },
            q.clone(),
        ))
    }

    #[allow(clippy::too_many_arguments)]
    fn run_lowered_summarize_plan(
        &self,
        orig_query: &Query,
        pre_steps: &[Step],
        post_steps: &[Step],
        specs: &[AggSpec],
        by_keys: &[GroupKey],
        sum_flags: &[bool],
        coerce_float_field: Option<&str>,
        target_shards: &[usize],
        now: i64,
        t0: Instant,
    ) -> Result<(Vec<Row>, ClusterStats, Query), Error> {
        let shard_q = Query {
            source: orig_query.source.clone(),
            steps: {
                let mut s = pre_steps.to_vec();
                s.push(Step::Summarize {
                    specs: specs.to_vec(),
                    by: by_keys.to_vec(),
                });
                s
            },
            explain: None,
            ignore_filter: orig_query.ignore_filter,
        };

        let (max_gen, payloads) = if target_shards.is_empty() {
            (
                self.max_shard_gen(),
                vec![empty_partial_summarize_wire(specs, by_keys, sum_flags)?],
            )
        } else {
            self.fanout_partial_summarize(target_shards, &fmt_query(&shard_q))?
        };

        let mut merged = merge_partial_summarize_wire(
            &payloads,
            specs,
            by_keys,
            sum_flags,
            self.quotas.max_aggregate_bytes,
        )?;
        if let Some(fname) = coerce_float_field {
            for r in &mut merged {
                if let Some(c) = r.get(fname) {
                    let f = c.as_f64().unwrap_or(0.0);
                    r.insert(fname.to_string(), Cell::Float(f));
                }
            }
        }
        let rows = apply_coordinator_post_steps(merged, post_steps, now, &self.quotas)?;
        let elapsed_us = u64::try_from(t0.elapsed().as_micros()).unwrap_or(u64::MAX);
        Ok((
            rows,
            ClusterStats {
                shards_queried: target_shards.len(),
                total_shards: self.shards.len(),
                max_gen,
                elapsed_us,
            },
            orig_query.clone(),
        ))
    }

    fn sum_f64_flags(&self, col_name: Option<&str>, specs: &[AggSpec]) -> Vec<bool> {
        let Ok(guard) = self.coord_db.lock() else {
            return vec![false; specs.len()];
        };
        let col_def = col_name.and_then(|c| guard.catalog.collections.get(c));
        crate::exec::sum_f64_flags_for(col_def, specs)
    }

    fn fanout_partial_summarize(
        &self,
        targets: &[usize],
        dsl: &str,
    ) -> Result<(u64, Vec<Vec<u8>>), Error> {
        let consistency = self.config.client.consistency;
        if let [only] = targets {
            let (g, p) = self.shards[*only].partial_summarize(dsl, consistency)?;
            return Ok((g, vec![p]));
        }
        let results: Vec<Result<(u64, Vec<u8>), Error>> = thread::scope(|scope| {
            let (last, rest) = targets.split_last().unwrap();
            let mut handles = Vec::with_capacity(rest.len());
            for &shard_idx in rest {
                let client = &self.shards[shard_idx];
                handles.push(scope.spawn(move || client.partial_summarize(dsl, consistency)));
            }
            let last_res = self.shards[*last].partial_summarize(dsl, consistency);
            let mut out: Vec<Result<(u64, Vec<u8>), Error>> = handles
                .into_iter()
                .map(|h| {
                    h.join()
                        .unwrap_or_else(|_| Err(Error::runtime("partial summarize worker panicked")))
                })
                .collect();
            out.push(last_res);
            out
        });

        let mut max_gen = 0u64;
        let mut payloads = Vec::with_capacity(results.len());
        for res in results {
            let (g, p) = res?;
            max_gen = max_gen.max(g);
            payloads.push(p);
        }
        Ok((max_gen, payloads))
    }

    fn fanout_query_batches(
        &self,
        targets: &[usize],
        dsl: &str,
    ) -> Result<(u64, Vec<RecordBatch>), Error> {
        let consistency = self.config.client.consistency;
        if let [only] = targets {
            let (batch, receipt) = self.shards[*only].query_with_consistency(dsl, consistency)?;
            return Ok((receipt.r#gen, vec![batch]));
        }
        let results: Vec<Result<(RecordBatch, ExecutionReceipt), Error>> = thread::scope(|scope| {
            let (last, rest) = targets.split_last().unwrap();
            let mut handles = Vec::with_capacity(rest.len());
            for &shard_idx in rest {
                let client = &self.shards[shard_idx];
                handles.push(scope.spawn(move || client.query_with_consistency(dsl, consistency)));
            }
            let last_res = self.shards[*last].query_with_consistency(dsl, consistency);
            let mut out: Vec<Result<(RecordBatch, ExecutionReceipt), Error>> = handles
                .into_iter()
                .map(|h| {
                    h.join()
                        .unwrap_or_else(|_| Err(Error::runtime("shard query worker panicked")))
                })
                .collect();
            out.push(last_res);
            out
        });

        let mut max_gen = 0u64;
        let mut batches = Vec::with_capacity(results.len());
        for res in results {
            let (batch, receipt) = res?;
            max_gen = max_gen.max(receipt.r#gen);
            batches.push(batch);
        }
        Ok((max_gen, batches))
    }

    fn fanout_query_rows(
        &self,
        targets: &[usize],
        dsl: &str,
    ) -> Result<(u64, Vec<Row>), Error> {
        let (max_gen, batches) = self.fanout_query_batches(targets, dsl)?;
        let total_rows: usize = batches.iter().map(RecordBatch::n).sum();
        let mut all_rows = Vec::with_capacity(total_rows);
        for batch in batches {
            all_rows.extend(batch.into_rows());
        }
        Ok((max_gen, all_rows))
    }
}

/// Embedded N-shard test/local cluster running N [`LinServer`] instances + a [`ShardRouter`].
pub struct ShardedCluster {
    handles: Vec<ServerHandle>,
    router: ShardRouter,
}

impl ShardedCluster {
    /// Start `num_shards` in-memory [`LinServer`] nodes and connect a [`ShardRouter`].
    pub fn start_in_memory(
        num_shards: usize,
        shard_keys: BTreeMap<String, String>,
        strategy: ShardStrategy,
    ) -> Result<Self, Error> {
        if num_shards == 0 {
            return Err(Error::runtime("ShardedCluster requires num_shards >= 1"));
        }
        let mut handles = Vec::with_capacity(num_shards);
        let mut addrs = Vec::with_capacity(num_shards);
        for _ in 0..num_shards {
            let h = LinServer::start(Db::empty(), ServerConfig::default())?;
            addrs.push(h.local_addr().to_string());
            handles.push(h);
        }
        let cfg = ShardConfig {
            shard_addrs: addrs,
            shard_keys,
            strategy,
            ..ShardConfig::default()
        };
        let router = ShardRouter::connect(cfg)?;
        Ok(Self { handles, router })
    }

    /// Start `num_shards` durable [`LinServer`] nodes under `root/shard_0..shard_{n-1}`.
    pub fn start_durable(
        root: impl AsRef<Path>,
        num_shards: usize,
        open_opts: OpenOpts,
        shard_keys: BTreeMap<String, String>,
        strategy: ShardStrategy,
    ) -> Result<Self, Error> {
        if num_shards == 0 {
            return Err(Error::runtime("ShardedCluster requires num_shards >= 1"));
        }
        let root = root.as_ref();
        std::fs::create_dir_all(root).map_err(|e| Error::runtime(format!("create cluster root: {e}")))?;
        let mut handles = Vec::with_capacity(num_shards);
        let mut addrs = Vec::with_capacity(num_shards);
        for idx in 0..num_shards {
            let dir = root.join(format!("shard_{idx}"));
            let db = Db::open_with(&dir, open_opts)?;
            let h = LinServer::start(db, ServerConfig::default())?;
            addrs.push(h.local_addr().to_string());
            handles.push(h);
        }
        let cfg = ShardConfig {
            shard_addrs: addrs,
            shard_keys,
            strategy,
            ..ShardConfig::default()
        };
        let router = ShardRouter::connect(cfg)?;
        Ok(Self { handles, router })
    }

    pub fn router(&self) -> &ShardRouter {
        &self.router
    }

    pub fn handles(&self) -> &[ServerHandle] {
        &self.handles
    }

    pub fn shard_addrs(&self) -> Vec<String> {
        self.handles.iter().map(|h| h.local_addr().to_string()).collect()
    }

    pub fn shutdown(self) -> Result<Vec<Db>, Error> {
        let Self { handles, router } = self;
        drop(router);
        let mut dbs = Vec::with_capacity(handles.len());
        for h in handles {
            dbs.push(h.shutdown()?);
        }
        Ok(dbs)
    }
}
