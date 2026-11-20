//! Horizontal sharding, Jump Consistent Hash routing, predicate shard pruning,
//! columnar `insert_batch` scattering, and two-phase distributed Kusto query execution.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Instant;

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

/// Zero-allocation canonical 64-bit hash for `col[row_idx]` without cloning `Cell` or `Arc<str>`.
#[inline]
fn shard_hash_col_row(col: &Column, row_idx: usize) -> u64 {
    match col {
        Column::Nulls { .. } => hash_bytes_tagged(6, &[]),
        Column::Int { vals, valid } => {
            if valid.is_valid(row_idx) {
                hash_i64(vals[row_idx])
            } else {
                hash_bytes_tagged(6, &[])
            }
        }
        Column::Time { vals, valid } => {
            if valid.is_valid(row_idx) {
                hash_u64_tagged(3, vals[row_idx] as u64)
            } else {
                hash_bytes_tagged(6, &[])
            }
        }
        Column::Float { vals, valid } => {
            if valid.is_valid(row_idx) {
                hash_f64_canonical(vals[row_idx])
            } else {
                hash_bytes_tagged(6, &[])
            }
        }
        Column::Bool { vals, valid } => {
            if valid.is_valid(row_idx) {
                hash_u64_tagged(4, u64::from(vals[row_idx]))
            } else {
                hash_bytes_tagged(6, &[])
            }
        }
        Column::Text { .. } | Column::Dict { .. } => match col.text_at(row_idx) {
            Some(s) => hash_bytes_tagged(5, s.as_bytes()),
            None => hash_bytes_tagged(6, &[]),
        },
        Column::Cells(cells) => shard_hash_cell(&cells[row_idx]),
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
    match prune_set(p, key, num_shards, strategy) {
        Some(set) => set.into_iter().collect(),
        None => (0..num_shards).collect(),
    }
}

// =========================================================================
// Lossless AST -> LinQL formatter (no truncation, float `.0` preserved)
// =========================================================================

fn fmt_float_literal(n: f64) -> String {
    if n.is_nan() {
        return "0.0".to_string();
    }
    if n == 0.0 {
        return "0.0".to_string();
    }
    let s = format!("{n}");
    if s.contains('.') || s.contains('e') || s.contains('E') {
        s
    } else {
        format!("{s}.0")
    }
}

fn fmt_value(v: &Value) -> String {
    match v {
        Value::String(s) => format!("{s:?}"),
        Value::Int(n) => format!("{n}"),
        Value::Float(n) => fmt_float_literal(*n),
        Value::Bool(b) => format!("{b}"),
        Value::Timestamp(ms) => format!("timestamp({ms})"),
        Value::Now => "now".to_string(),
        Value::NowMinus(d) => format!("now - {d}"),
        Value::Duration(d) => format!("{d}"),
        Value::Name(n) => n.clone(),
        Value::Param(p) => format!("${p}"),
    }
}

fn fmt_pred(pred: &Pred) -> String {
    match pred {
        Pred::And(a, b) => {
            let sa = if matches!(**a, Pred::Or(..)) {
                format!("({})", fmt_pred(a))
            } else {
                fmt_pred(a)
            };
            let sb = if matches!(**b, Pred::Or(..)) {
                format!("({})", fmt_pred(b))
            } else {
                fmt_pred(b)
            };
            format!("{sa} and {sb}")
        }
        Pred::Or(a, b) => format!("{} or {}", fmt_pred(a), fmt_pred(b)),
        Pred::Not(inner) => format!("not ({})", fmt_pred(inner)),
        Pred::Cmp { field, op, value } => {
            format!("{field} {} {}", op.as_str(), fmt_value(value))
        }
        Pred::IsNull { field, negated } => {
            if *negated {
                format!("{field} is not null")
            } else {
                format!("{field} is null")
            }
        }
        Pred::In { field, values } => {
            let mut out = format!("{field} in [");
            for (i, v) in values.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                out.push_str(&fmt_value(v));
            }
            out.push(']');
            out
        }
        Pred::Has { field, ci, needle } => {
            if *ci {
                format!("{field} has ci {needle:?}")
            } else {
                format!("{field} has {needle:?}")
            }
        }
        Pred::Contains { field, needle } => format!("{field} ~ {needle:?}"),
        Pred::Regex {
            field,
            pattern,
            flags,
        } => format!("{field} ~ /{pattern}/{flags}"),
    }
}

fn fmt_record(rec: &Record) -> String {
    let mut out = String::from("{ ");
    for (i, (k, v)) in rec.fields.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(k);
        out.push_str(": ");
        out.push_str(&fmt_value(v));
    }
    out.push_str(" }");
    out
}

fn fmt_bin_step(step: &BinStep) -> String {
    match step {
        BinStep::Duration(ms) => {
            if *ms % 86_400_000 == 0 && *ms >= 86_400_000 {
                format!("{}d", ms / 86_400_000)
            } else if *ms % 3_600_000 == 0 && *ms >= 3_600_000 {
                format!("{}h", ms / 3_600_000)
            } else if *ms % 60_000 == 0 && *ms >= 60_000 {
                format!("{}m", ms / 60_000)
            } else if *ms % 1_000 == 0 && *ms >= 1_000 {
                format!("{}s", ms / 1_000)
            } else {
                format!("{ms}")
            }
        }
        BinStep::Int(n) => format!("{n}"),
        BinStep::Float(f) => fmt_float_literal(*f),
    }
}

fn fmt_group_key(gk: &GroupKey) -> String {
    match &gk.expr {
        GroupExpr::Field(f) => {
            if gk.alias == f.as_str() {
                format!("{f}")
            } else {
                format!("{}: {f}", gk.alias)
            }
        }
        GroupExpr::Bin { field, step } => {
            format!("{}: bin({field}, {})", gk.alias, fmt_bin_step(step))
        }
    }
}

fn fmt_agg_spec(spec: &AggSpec) -> String {
    let body = match &spec.func {
        AggFunc::Count => "count()".to_string(),
        AggFunc::Sum(f) => format!("sum({f})"),
        AggFunc::Avg(f) => format!("avg({f})"),
        AggFunc::Min(f) => format!("min({f})"),
        AggFunc::Max(f) => format!("max({f})"),
        AggFunc::DCount(f) => format!("dcount({f})"),
        AggFunc::Percentile { field, pct } => {
            format!("percentile({field}, {})", fmt_float_literal(*pct))
        }
        AggFunc::ArgMax { order, value } => format!("arg_max({order}, {value})"),
        AggFunc::ArgMin { order, value } => format!("arg_min({order}, {value})"),
    };
    format!("{}: {body}", spec.alias)
}

fn fmt_by_list(by: &[GroupKey]) -> String {
    if by.is_empty() {
        return String::new();
    }
    let mut out = String::from(" by [");
    for (i, g) in by.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(&fmt_group_key(g));
    }
    out.push(']');
    out
}

fn fmt_hop(h: &MatchHop) -> String {
    let edge = h
        .edge
        .as_deref()
        .map_or_else(|| h.rel.clone(), |e| format!("{e}:{}", h.rel));
    let depth = if h.min_depth == 1 && h.max_depth == 1 {
        String::new()
    } else if h.min_depth == h.max_depth {
        format!("*{}", h.min_depth)
    } else {
        format!("*{}..{}", h.min_depth, h.max_depth)
    };
    if h.reverse {
        format!("<-[{edge}{depth}]- {}", h.bind)
    } else {
        format!("-[{edge}{depth}]-> {}", h.bind)
    }
}

fn fmt_step(step: &Step) -> String {
    match step {
        Step::Filter(p) => format!("where {}", fmt_pred(p)),
        Step::Project(fields) => {
            let list: Vec<String> = fields.iter().map(ToString::to_string).collect();
            format!("pick {{ {} }}", list.join(", "))
        }
        Step::Without(fields) => {
            let list: Vec<String> = fields.iter().map(ToString::to_string).collect();
            format!("without {{ {} }}", list.join(", "))
        }
        Step::Join {
            left,
            collection,
            on,
        } => {
            if *left {
                format!("join left {collection} on {on}")
            } else {
                format!("join {collection} on {on}")
            }
        }
        Step::Hop { rel, depth } => match depth {
            Some(d) => format!("hop {rel} depth={d}"),
            None => format!("hop {rel}"),
        },
        Step::Graph { rel, depth } => match depth {
            Some(d) => format!("graph {rel} depth={d}"),
            None => format!("graph {rel}"),
        },
        Step::Match { start, hops } => {
            let mut out = String::from("match ");
            if let Some(s) = start {
                out.push_str(s);
                out.push(' ');
            }
            for (i, h) in hops.iter().enumerate() {
                if i > 0 {
                    out.push(' ');
                }
                out.push_str(&fmt_hop(h));
            }
            out
        }
        Step::Search { mode, query } => match mode {
            SearchMode::Hybrid => format!("search {query:?}"),
            SearchMode::Lex => format!("search lex {query:?}"),
            SearchMode::Vec => format!("search vec {query:?}"),
        },
        Step::Count { by } => match by {
            Some(f) => format!("count by {f}"),
            None => "count".to_string(),
        },
        Step::Sum { field, by } => match by {
            Some(b) => format!("sum {field} by {b}"),
            None => format!("sum {field}"),
        },
        Step::Min { field, by } => match by {
            Some(b) => format!("min {field} by {b}"),
            None => format!("min {field}"),
        },
        Step::Max { field, by } => match by {
            Some(b) => format!("max {field} by {b}"),
            None => format!("max {field}"),
        },
        Step::Avg { field, by } => match by {
            Some(b) => format!("avg {field} by {b}"),
            None => format!("avg {field}"),
        },
        Step::Summarize { specs, by } => {
            let items: Vec<String> = specs.iter().map(fmt_agg_spec).collect();
            format!("summarize {{ {} }}{}", items.join(", "), fmt_by_list(by))
        }
        Step::ArgExtremum { is_max, order, by } => {
            let kw = if *is_max { "arg_max" } else { "arg_min" };
            format!("{kw}({order}, *){}", fmt_by_list(by))
        }
        Step::Sort { field, desc } => {
            if *desc {
                format!("sort {field} desc")
            } else {
                format!("sort {field} asc")
            }
        }
        Step::Window {
            func,
            field,
            by,
            order,
            frame,
            as_field,
        } => {
            let mut out = format!("window {func}");
            if let Some(f) = field {
                out.push_str(&format!(" ({f})"));
            } else if matches!(func, crate::window::WindowFunction::Count) {
                out.push_str(" (*)");
            }
            if !by.is_empty() {
                let b: Vec<String> = by.iter().map(ToString::to_string).collect();
                out.push_str(&format!(" by [{}]", b.join(", ")));
            }
            if !order.is_empty() {
                let ord: Vec<String> = order
                    .iter()
                    .map(|o| {
                        format!(
                            "{} {}",
                            o.field,
                            if o.desc { "desc" } else { "asc" }
                        )
                    })
                    .collect();
                out.push_str(&format!(" sort [{}]", ord.join(", ")));
            }
            if let Some(fr) = frame {
                out.push_str(&format!(" {fr}"));
            }
            out.push_str(&format!(" as {as_field}"));
            out
        }
        Step::Skip { n } => format!("skip {n}"),
        Step::Take { n } => match n {
            Some(k) => format!("take {k}"),
            None => "take all".to_string(),
        },
        Step::Union(rhs) => format!("union ({})", fmt_query(rhs)),
    }
}

fn fmt_query(q: &Query) -> String {
    let mut out = match &q.source {
        Source::Collection(c) => c.clone(),
        Source::Page(p) => format!("page {p:?}"),
        Source::Catalog => "catalog".to_string(),
    };
    if q.ignore_filter {
        out.push_str(" all");
    }
    for s in &q.steps {
        out.push_str(" | ");
        out.push_str(&fmt_step(s));
    }
    out
}

fn fmt_type_expr(t: &TypeExpr) -> String {
    match t {
        TypeExpr::Named(n) => n.clone(),
        TypeExpr::Vec { dim, model } => {
            if model.is_empty() {
                format!("vec[{dim}]")
            } else {
                format!("vec[{dim}, {model:?}]")
            }
        }
    }
}

fn fmt_decl(d: &Decl) -> String {
    match d {
        Decl::Col {
            name,
            append,
            fields,
        } => {
            let kw = if *append { "append col" } else { "col" };
            let cols: Vec<String> = fields
                .iter()
                .map(|(k, t)| format!("{k}: {}", fmt_type_expr(t)))
                .collect();
            format!("{kw} {name} {{ {} }}", cols.join(", "))
        }
        Decl::Rel { name, stub } => {
            if *stub {
                format!("rel stub {name}")
            } else {
                format!("rel {name}")
            }
        }
        Decl::RelReverse { name, of } => format!("rel {name} reverse {of}"),
        Decl::Fk {
            from_col,
            from_field,
            to_col,
            to_field,
        } => format!("fk {from_col}.{from_field} -> {to_col}.{to_field}"),
        Decl::Guard {
            collection,
            field,
            pred,
        } => format!("guard {collection}.{field} [{}]", fmt_pred(pred)),
        Decl::Filter {
            collection,
            pred: _,
            src,
        } => {
            if src.is_empty() {
                format!("filter {collection} off")
            } else {
                format!("filter {collection} [{src}]")
            }
        }
        Decl::Index {
            collection,
            unique,
            fields,
            pred,
        } => {
            let uniq = if *unique { "unique " } else { "" };
            let p = pred
                .as_ref()
                .map_or_else(String::new, |p| format!(" where {}", fmt_pred(p)));
            format!("index {uniq}{collection} [{}]{p}", fields.join(", "))
        }
        Decl::Owned { name, fields } => {
            let cols: Vec<String> = fields
                .iter()
                .map(|(k, t)| format!("{k}: {}", fmt_type_expr(t)))
                .collect();
            format!("owned {name} {{ {} }}", cols.join(", "))
        }
    }
}

fn fmt_cas(cas: Option<&str>, cas_each: bool) -> String {
    if cas_each {
        " cas each".to_string()
    } else if let Some(c) = cas {
        format!(" cas {c:?}")
    } else {
        String::new()
    }
}

fn fmt_insert(collection: &str, records: &[Record]) -> String {
    let items: Vec<String> = records.iter().map(fmt_record).collect();
    format!("insert {collection} [{}]", items.join(", "))
}

fn fmt_update(
    collection: &str,
    pred: Option<&Pred>,
    cas: Option<&str>,
    cas_each: bool,
    record: &Record,
) -> String {
    let p = pred
        .map_or_else(String::new, |p| format!(" [{}]", fmt_pred(p)));
    format!(
        "update {collection}{p}{} {}",
        fmt_cas(cas, cas_each),
        fmt_record(record)
    )
}

fn fmt_delete(collection: &str, pred: &Pred, cas: Option<&str>, cas_each: bool) -> String {
    format!(
        "delete {collection} [{}]{}",
        fmt_pred(pred),
        fmt_cas(cas, cas_each)
    )
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
                for row_idx in 0..n {
                    let h = shard_hash_col_row(col, row_idx);
                    let shard = pick_shard(h, num_shards, strategy);
                    per_shard[shard].push(row_idx as u32);
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

        let results: Vec<Result<ExecutionReceipt, Error>> = thread::scope(|scope| {
            let mut handles = Vec::with_capacity(shards_queried);
            for (shard_idx, idx) in &non_empty {
                let client = &self.shards[*shard_idx];
                handles.push(scope.spawn(move || {
                    let cols: Vec<Column> = batch.cols.iter().map(|c| c.gather(idx)).collect();
                    let sub = RecordBatch {
                        names: Arc::clone(&batch.names),
                        cols,
                    };
                    client.insert_batch(collection, &sub)
                }));
            }
            handles
                .into_iter()
                .map(|h| {
                    h.join()
                        .unwrap_or_else(|_| Err(Error::runtime("shard insert_batch worker panicked")))
                })
                .collect()
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
                        let target = match &e.target {
                            EdgeTarget::Page(p) => format!("{p:?}"),
                            EdgeTarget::Value(v) => fmt_value(v),
                        };
                        out.push_str(&format!(" -> {} {target}", e.rel));
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

    fn exec_sharded_insert(
        &self,
        collection: &str,
        records: &[Record],
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
        let mut buckets: Vec<Vec<Record>> = vec![Vec::new(); num_shards];

        for (idx, rec) in records.iter().enumerate() {
            let mut rec_owned = rec.clone();
            let shard = if let Some(sk) = shard_key {
                if let Some((_, val)) = rec_owned.fields.iter().find(|(k, _)| k == sk) {
                    match shard_hash_value(val) {
                        Some(h) => pick_shard(h, num_shards, self.config.strategy),
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
                    rec_owned.fields.push(("id".to_string(), val));
                    pick_shard(h, num_shards, self.config.strategy)
                } else {
                    return Err(Error::runtime(format!(
                        "insert missing shard key field {sk:?} for collection {collection:?}"
                    )));
                }
            } else if let Some((_, val)) = rec_owned.fields.iter().find(|(k, _)| k == "id") {
                match shard_hash_value(val) {
                    Some(h) => pick_shard(h, num_shards, self.config.strategy),
                    None => idx % num_shards,
                }
            } else {
                idx % num_shards
            };
            buckets[shard].push(rec_owned);
        }

        let work: Vec<(usize, String)> = buckets
            .into_iter()
            .enumerate()
            .filter(|(_, recs)| !recs.is_empty())
            .map(|(s, recs)| (s, fmt_insert(collection, &recs)))
            .collect();

        let results: Vec<Result<ExecutionReceipt, Error>> = thread::scope(|scope| {
            let mut handles = Vec::with_capacity(work.len());
            for (shard_idx, dsl) in &work {
                let client = &self.shards[*shard_idx];
                handles.push(scope.spawn(move || client.execute(dsl)));
            }
            handles
                .into_iter()
                .map(|h| {
                    h.join()
                        .unwrap_or_else(|_| Err(Error::runtime("shard insert worker panicked")))
                })
                .collect()
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
        let results: Vec<Result<ExecutionReceipt, Error>> = thread::scope(|scope| {
            let mut handles = Vec::with_capacity(targets.len());
            for &shard_idx in targets {
                let client = &self.shards[shard_idx];
                handles.push(scope.spawn(move || client.execute(dsl)));
            }
            handles
                .into_iter()
                .map(|h| {
                    h.join()
                        .unwrap_or_else(|_| Err(Error::runtime("shard write worker panicked")))
                })
                .collect()
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
        let (rows, stats, post_query) = self.exec_query_internal(q)?;
        let batch = rows_to_batch_for_query(&rows, &post_query);
        Ok((batch, stats))
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
        let results: Vec<Result<(u64, Vec<u8>), Error>> = thread::scope(|scope| {
            let mut handles = Vec::with_capacity(targets.len());
            for &shard_idx in targets {
                let client = &self.shards[shard_idx];
                handles.push(scope.spawn(move || client.partial_summarize(dsl, consistency)));
            }
            handles
                .into_iter()
                .map(|h| {
                    h.join()
                        .unwrap_or_else(|_| Err(Error::runtime("partial summarize worker panicked")))
                })
                .collect()
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

    fn fanout_query_rows(
        &self,
        targets: &[usize],
        dsl: &str,
    ) -> Result<(u64, Vec<Row>), Error> {
        let consistency = self.config.client.consistency;
        let results: Vec<Result<(RecordBatch, ExecutionReceipt), Error>> = thread::scope(|scope| {
            let mut handles = Vec::with_capacity(targets.len());
            for &shard_idx in targets {
                let client = &self.shards[shard_idx];
                handles.push(scope.spawn(move || client.query_with_consistency(dsl, consistency)));
            }
            handles
                .into_iter()
                .map(|h| {
                    h.join()
                        .unwrap_or_else(|_| Err(Error::runtime("shard query worker panicked")))
                })
                .collect()
        });

        let mut max_gen = 0u64;
        let mut all_rows = Vec::new();
        for res in results {
            let (batch, receipt) = res?;
            max_gen = max_gen.max(receipt.r#gen);
            all_rows.extend(batch.to_rows());
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
