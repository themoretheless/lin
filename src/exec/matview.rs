//! Incremental materialized `summarize` views and augmented time-range interval overlap indexes (`P2.3`).
//!
//! - [`MaterializedSummarizeView`]: maintains exact partial `summarize` accumulators
//!   (`count`, `sum`, `avg`, `min`, `max`, `dcount`, `percentile`, `arg_max`, `arg_min`, `bin(...)`)
//!   over a hot collection, folding newly appended rows (`&rows[cursor_len..]`) in `O(Δrows)`
//!   whenever `RowMap::rewrite_epoch` is unchanged, and rebuilding cleanly after in-place
//!   updates, deletes, or rollbacks.
//! - [`IntervalOverlapIndex`]: block-partitioned sorted-start interval index augmented with a
//!   segment tree of `max(end)` bounds plus an `O(1)` amortized append tail, accelerating
//!   `start <=/< q_end and end >=/> q_start` time-range overlap queries in `O(log B + K)`.

use std::sync::{Arc, Mutex};

use crate::ast::{AggSpec, CmpOp, GroupKey, Pred, Query, Source, Step, Stmt, Value};
use crate::catalog::{Catalog, Type};
use crate::error::Error;
use crate::store::row::FieldSlot;
use crate::store::{Cell, Row, Store};

use super::agg::{
    PartialSummarizeState, apply_coordinator_post_steps, finish_partial_state, fold_summarize_rows,
    merge_partial_states,
};

/// Runtime statistics of a [`MaterializedSummarizeView`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MaterializedViewStats {
    pub cursor_len: usize,
    pub incremental_refreshes: u64,
    pub full_rebuilds: u64,
    pub cache_hits: u64,
}

struct MatViewRuntime {
    epoch: u64,
    rewrite_epoch: u64,
    cursor_len: usize,
    partial: Option<PartialSummarizeState>,
    cached_rows: Arc<[Row]>,
    stats: MaterializedViewStats,
}

pub(crate) struct MaterializedSummarizeView {
    pub(crate) collection: String,
    pub(crate) canonical_query: Query,
    pre_filter: Option<Pred>,
    specs: Vec<AggSpec>,
    by: Vec<GroupKey>,
    sum_f64_flags: Vec<bool>,
    post_steps: Vec<Step>,
    has_explicit_take: bool,
    rt: Mutex<MatViewRuntime>,
}

fn pred_is_static(p: &Pred) -> bool {
    match p {
        Pred::And(a, b) | Pred::Or(a, b) => pred_is_static(a) && pred_is_static(b),
        Pred::Not(inner) => pred_is_static(inner),
        Pred::Cmp { value, .. } => !matches!(
            value,
            Value::Now | Value::NowMinus(_) | Value::Param(_)
        ),
        Pred::In { values, .. } => values
            .iter()
            .all(|v| !matches!(v, Value::Now | Value::NowMinus(_) | Value::Param(_))),
        Pred::IsNull { .. } | Pred::Has { .. } | Pred::Contains { .. } | Pred::Regex { .. } => true,
    }
}

impl MaterializedSummarizeView {
    pub(crate) fn compile(src: &str, cat: &Catalog) -> Result<Self, Error> {
        let stmt = crate::parse::parse(src)?;
        let Stmt::Query(raw_q) = stmt else {
            return Err(Error::new(
                "materialized view requires a read query ending with `summarize`",
            ));
        };
        let _ = crate::check::check_program(std::slice::from_ref(&Stmt::Query(raw_q.clone())), cat)?;
        let mut q = crate::catalog::with_catalog_filter(&raw_q, cat).into_owned();
        if q.needs_simplify() {
            q.simplify_predicates();
        }
        let Source::Collection(ref col_name) = q.source else {
            return Err(Error::new("materialized view source must be a collection"));
        };
        let mut idx = 0usize;
        let mut pre_filter: Option<Pred> = None;
        while let Some(Step::Filter(p)) = q.steps.get(idx) {
            if !pred_is_static(p) {
                return Err(Error::new(
                    "materialized view pre-filter cannot depend on `now` or `$param`",
                ));
            }
            pre_filter = Some(match pre_filter {
                None => p.clone(),
                Some(acc) => acc.and(p.clone()),
            });
            idx += 1;
        }
        let Some(Step::Summarize { specs, by }) = q.steps.get(idx).cloned() else {
            return Err(Error::new(
                "materialized view query must have `summarize { ... }` after optional static filters",
            ));
        };
        let post_steps: Vec<Step> = q.steps[idx + 1..].to_vec();
        for ps in &post_steps {
            match ps {
                Step::Filter(p) if pred_is_static(p) => {}
                Step::Project(_)
                | Step::Without(_)
                | Step::Sort { .. }
                | Step::Skip { .. }
                | Step::Take { .. } => {}
                _ => {
                    return Err(Error::new(
                        "unsupported post-summarize step in materialized view",
                    ));
                }
            }
        }
        let has_explicit_take = post_steps.iter().any(|s| matches!(s, Step::Take { .. }));
        let col_def = cat.collection(col_name);
        let sum_f64_flags: Vec<bool> = specs
            .iter()
            .map(|sp| match &sp.func {
                crate::ast::AggFunc::Sum(f) => f
                    .leaf()
                    .and_then(|lf| col_def.and_then(|cd| cd.fields.get(lf)))
                    .is_some_and(|fi| fi.ty == Type::F64),
                _ => false,
            })
            .collect();

        Ok(Self {
            collection: col_name.clone(),
            canonical_query: q,
            pre_filter,
            specs,
            by,
            sum_f64_flags,
            post_steps,
            has_explicit_take,
            rt: Mutex::new(MatViewRuntime {
                epoch: 0,
                rewrite_epoch: 0,
                cursor_len: 0,
                partial: None,
                cached_rows: Arc::from([]),
                stats: MaterializedViewStats {
                    cursor_len: 0,
                    incremental_refreshes: 0,
                    full_rebuilds: 0,
                    cache_hits: 0,
                },
            }),
        })
    }

    pub(crate) fn matches_query(&self, q: &Query) -> bool {
        &self.canonical_query == q
    }

    pub(crate) fn stats(&self) -> MaterializedViewStats {
        self.rt
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .stats
    }

    pub(crate) fn evaluate(
        &self,
        store: &Store,
        quotas: &crate::Quotas,
        allow_implicit_take: bool,
    ) -> Result<Option<Vec<Row>>, Error> {
        if store.is_disk_backed(&self.collection)
            || (self.collection == "facts" && store.facts_rows_stale)
            || (self.collection == "docs" && store.has_deferred_doc_ops())
        {
            return Ok(None);
        }
        let empty_rows: Vec<Row> = Vec::new();
        let rows = store.collections.get(&self.collection).unwrap_or(&empty_rows);
        let cur_epoch = store.collections.epoch();
        let cur_rewrite = store.collections.rewrite_epoch();
        let max_bytes = quotas.max_aggregate_bytes;

        let mut rt = self.rt.lock().unwrap_or_else(|e| e.into_inner());
        let base_rows: Arc<[Row]> = if rt.partial.is_some()
            && rt.epoch == cur_epoch
            && rt.cursor_len == rows.len()
        {
            rt.stats.cache_hits += 1;
            Arc::clone(&rt.cached_rows)
        } else if rt.partial.is_some()
            && rt.rewrite_epoch == cur_rewrite
            && rows.len() >= rt.cursor_len
        {
            if rows.len() > rt.cursor_len {
                let delta = fold_summarize_rows(
                    &rows[rt.cursor_len..],
                    self.pre_filter.as_ref(),
                    0,
                    &self.specs,
                    &self.by,
                    &self.sum_f64_flags,
                    max_bytes,
                )?;
                let partial = rt.partial.as_mut().unwrap();
                merge_partial_states(partial, delta, self.specs.len(), max_bytes)?;
                let finished =
                    finish_partial_state(partial.clone(), &self.by, &self.specs);
                let post = apply_coordinator_post_steps(finished, &self.post_steps, 0, quotas)?;
                rt.cached_rows = Arc::from(post);
                rt.cursor_len = rows.len();
                rt.stats.cursor_len = rows.len();
                rt.stats.incremental_refreshes += 1;
            } else {
                rt.stats.cache_hits += 1;
            }
            rt.epoch = cur_epoch;
            Arc::clone(&rt.cached_rows)
        } else {
            let full = fold_summarize_rows(
                rows,
                self.pre_filter.as_ref(),
                0,
                &self.specs,
                &self.by,
                &self.sum_f64_flags,
                max_bytes,
            )?;
            let finished = finish_partial_state(full.clone(), &self.by, &self.specs);
            let post = apply_coordinator_post_steps(finished, &self.post_steps, 0, quotas)?;
            rt.partial = Some(full);
            rt.cached_rows = Arc::from(post);
            rt.epoch = cur_epoch;
            rt.rewrite_epoch = cur_rewrite;
            rt.cursor_len = rows.len();
            rt.stats.cursor_len = rows.len();
            rt.stats.full_rebuilds += 1;
            Arc::clone(&rt.cached_rows)
        };
        drop(rt);

        let mut out: Vec<Row> = base_rows.iter().cloned().collect();
        if allow_implicit_take && !self.has_explicit_take && out.len() > 50 {
            out.truncate(50);
        }
        Ok(Some(out))
    }
}

const IV_BLOCK: usize = 64;
const IV_TAIL_COMPACT: usize = 512;

#[derive(Default)]
struct IntervalRuntime {
    epoch: u64,
    rewrite_epoch: u64,
    cursor_len: usize,
    built: bool,
    sorted: Vec<(i64, i64, u32)>,
    block_min_start: Vec<i64>,
    tree_max_end: Vec<i64>,
    tree_base: usize,
    tail: Vec<(i64, i64, u32)>,
}

impl IntervalRuntime {
    fn extract_cell_i64(c: Option<&Cell>) -> Option<i64> {
        match c? {
            Cell::Time(t) => Some(*t),
            Cell::Int(n) => Some(*n),
            _ => None,
        }
    }

    fn rebuild(&mut self, rows: &[Row], start_field: &str, end_field: &str) {
        self.sorted.clear();
        self.tail.clear();
        let mut s_slot = FieldSlot::new(start_field);
        let mut e_slot = FieldSlot::new(end_field);
        for (idx, r) in rows.iter().enumerate() {
            if let (Some(s), Some(e)) = (
                Self::extract_cell_i64(s_slot.get(r)),
                Self::extract_cell_i64(e_slot.get(r)),
            ) {
                self.sorted.push((s, e, idx as u32));
            }
        }
        self.rebuild_tree();
        self.cursor_len = rows.len();
        self.built = true;
    }

    fn append_delta(&mut self, rows: &[Row], from: usize, start_field: &str, end_field: &str) {
        let mut s_slot = FieldSlot::new(start_field);
        let mut e_slot = FieldSlot::new(end_field);
        for (off, r) in rows[from..].iter().enumerate() {
            if let (Some(s), Some(e)) = (
                Self::extract_cell_i64(s_slot.get(r)),
                Self::extract_cell_i64(e_slot.get(r)),
            ) {
                self.tail.push((s, e, (from + off) as u32));
            }
        }
        self.cursor_len = rows.len();
        if self.tail.len() >= IV_TAIL_COMPACT {
            self.sorted.append(&mut self.tail);
            self.rebuild_tree();
        }
    }

    fn rebuild_tree(&mut self) {
        self.sorted.sort_unstable_by_key(|&(s, e, id)| (s, e, id));
        let n_blocks = self.sorted.len().div_ceil(IV_BLOCK);
        self.block_min_start.clear();
        self.block_min_start.reserve(n_blocks);
        if n_blocks == 0 {
            self.tree_max_end.clear();
            self.tree_base = 0;
            return;
        }
        let base = n_blocks.next_power_of_two();
        self.tree_base = base;
        self.tree_max_end.assign_len_fill(base * 2, i64::MIN);
        for b in 0..n_blocks {
            let chunk = &self.sorted[b * IV_BLOCK..((b + 1) * IV_BLOCK).min(self.sorted.len())];
            self.block_min_start.push(chunk[0].0);
            let max_e = chunk.iter().map(|x| x.1).max().unwrap_or(i64::MIN);
            self.tree_max_end[base + b] = max_e;
        }
        for node in (1..base).rev() {
            let l = self.tree_max_end[node * 2];
            let r = self.tree_max_end[node * 2 + 1];
            self.tree_max_end[node] = l.max(r);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn query_node(
        &self,
        node: usize,
        seg_lo: usize,
        seg_hi: usize,
        upper_block: usize,
        max_start: i64,
        start_strict: bool,
        min_end: i64,
        end_strict: bool,
        out: &mut Vec<usize>,
    ) {
        if seg_lo >= upper_block || node >= self.tree_max_end.len() {
            return;
        }
        let subtree_max = self.tree_max_end[node];
        if if end_strict {
            subtree_max <= min_end
        } else {
            subtree_max < min_end
        } {
            return;
        }
        if seg_hi - seg_lo == 1 {
            let start_idx = seg_lo * IV_BLOCK;
            let end_idx = (start_idx + IV_BLOCK).min(self.sorted.len());
            for &(s, e, rid) in &self.sorted[start_idx..end_idx] {
                let s_ok = if start_strict { s < max_start } else { s <= max_start };
                if !s_ok {
                    break;
                }
                let e_ok = if end_strict { e > min_end } else { e >= min_end };
                if e_ok {
                    out.push(rid as usize);
                }
            }
            return;
        }
        let mid = seg_lo + (seg_hi - seg_lo) / 2;
        self.query_node(
            node * 2,
            seg_lo,
            mid,
            upper_block,
            max_start,
            start_strict,
            min_end,
            end_strict,
            out,
        );
        if mid < upper_block {
            self.query_node(
                node * 2 + 1,
                mid,
                seg_hi,
                upper_block,
                max_start,
                start_strict,
                min_end,
                end_strict,
                out,
            );
        }
    }

    fn query_overlap(
        &self,
        max_start: i64,
        start_strict: bool,
        min_end: i64,
        end_strict: bool,
    ) -> Vec<usize> {
        let mut out = Vec::new();
        if !self.block_min_start.is_empty() && self.tree_base > 0 {
            let upper_block = self.block_min_start.partition_point(|&min_s| {
                if start_strict {
                    min_s < max_start
                } else {
                    min_s <= max_start
                }
            });
            if upper_block > 0 {
                self.query_node(
                    1,
                    0,
                    self.tree_base,
                    upper_block,
                    max_start,
                    start_strict,
                    min_end,
                    end_strict,
                    &mut out,
                );
            }
        }
        for &(s, e, rid) in &self.tail {
            let s_ok = if start_strict { s < max_start } else { s <= max_start };
            let e_ok = if end_strict { e > min_end } else { e >= min_end };
            if s_ok && e_ok {
                out.push(rid as usize);
            }
        }
        out.sort_unstable();
        out
    }
}

trait VecFillExt<T: Copy> {
    fn assign_len_fill(&mut self, len: usize, val: T);
}

impl<T: Copy> VecFillExt<T> for Vec<T> {
    fn assign_len_fill(&mut self, len: usize, val: T) {
        self.clear();
        self.resize(len, val);
    }
}

pub(crate) struct IntervalOverlapIndex {
    pub(crate) collection: String,
    pub(crate) start_field: String,
    pub(crate) end_field: String,
    rt: Mutex<IntervalRuntime>,
}

impl IntervalOverlapIndex {
    pub(crate) fn new(collection: String, start_field: String, end_field: String) -> Self {
        Self {
            collection,
            start_field,
            end_field,
            rt: Mutex::new(IntervalRuntime::default()),
        }
    }

    pub(crate) fn select_overlapping(
        &self,
        store: &Store,
        max_start: i64,
        start_strict: bool,
        min_end: i64,
        end_strict: bool,
    ) -> Option<Vec<usize>> {
        if store.is_disk_backed(&self.collection)
            || (self.collection == "facts" && store.facts_rows_stale)
            || (self.collection == "docs" && store.has_deferred_doc_ops())
        {
            return None;
        }
        let rows = store.collections.get(&self.collection)?;
        let cur_epoch = store.collections.epoch();
        let cur_rewrite = store.collections.rewrite_epoch();
        let mut rt = self.rt.lock().unwrap_or_else(|e| e.into_inner());
        if !rt.built || rt.rewrite_epoch != cur_rewrite || rows.len() < rt.cursor_len {
            rt.rebuild(rows, &self.start_field, &self.end_field);
            rt.epoch = cur_epoch;
            rt.rewrite_epoch = cur_rewrite;
        } else if rt.epoch != cur_epoch && rows.len() > rt.cursor_len {
            let from = rt.cursor_len;
            rt.append_delta(rows, from, &self.start_field, &self.end_field);
            rt.epoch = cur_epoch;
        }
        Some(rt.query_overlap(max_start, start_strict, min_end, end_strict))
    }
}

fn eval_bound_i64(v: &Value, now: i64) -> Option<i64> {
    match v {
        Value::Int(n) | Value::Timestamp(n) => Some(*n),
        Value::Now => Some(now),
        Value::NowMinus(d) => Some(now.saturating_sub(d.as_millis())),
        _ => None,
    }
}

fn collect_conjuncts<'a>(p: &'a Pred, out: &mut Vec<&'a Pred>) {
    match p {
        Pred::And(a, b) => {
            collect_conjuncts(a, out);
            collect_conjuncts(b, out);
        }
        other => out.push(other),
    }
}

/// Extract `(max_start, start_strict, min_end, end_strict)` when `pred` contains conjuncts
/// bounding `start_field <=/< max_start` and `end_field >=/> min_end` (or `==`).
pub(crate) fn extract_interval_bounds(
    pred: &Pred,
    start_field: &str,
    end_field: &str,
    now: i64,
) -> Option<(i64, bool, i64, bool)> {
    let mut leaves = Vec::new();
    collect_conjuncts(pred, &mut leaves);
    let mut upper: Option<(i64, bool)> = None;
    let mut lower: Option<(i64, bool)> = None;
    for leaf in leaves {
        let Pred::Cmp { field, op, value } = leaf else {
            continue;
        };
        let Some(fname) = field.leaf() else {
            continue;
        };
        let Some(bound) = eval_bound_i64(value, now) else {
            continue;
        };
        if fname == start_field {
            match op {
                CmpOp::Le | CmpOp::Eq => {
                    upper = Some(match upper {
                        None => (bound, false),
                        Some((prev, _)) if bound < prev => (bound, false),
                        Some(cur) => cur,
                    });
                    if *op == CmpOp::Eq {
                        lower = lower.or(None);
                    }
                }
                CmpOp::Lt => {
                    upper = Some(match upper {
                        None => (bound, true),
                        Some((prev, st)) if bound < prev || (bound == prev && !st) => (bound, true),
                        Some(cur) => cur,
                    });
                }
                _ => {}
            }
        }
        if fname == end_field {
            match op {
                CmpOp::Ge | CmpOp::Eq => {
                    lower = Some(match lower {
                        None => (bound, false),
                        Some((prev, _)) if bound > prev => (bound, false),
                        Some(cur) => cur,
                    });
                }
                CmpOp::Gt => {
                    lower = Some(match lower {
                        None => (bound, true),
                        Some((prev, st)) if bound > prev || (bound == prev && !st) => (bound, true),
                        Some(cur) => cur,
                    });
                }
                _ => {}
            }
        }
    }
    let (max_s, s_strict) = upper?;
    let (min_e, e_strict) = lower?;
    Some((max_s, s_strict, min_e, e_strict))
}
