use std::collections::BTreeMap;
use std::sync::Arc;

use rustc_hash::{FxHashMap, FxHashSet};

use crate::ast::{CmpOp, Field, Pred, Query, Source, Step, Value};
use crate::batch::{Column, RecordBatch};
use crate::error::Error;
use crate::store::{Cell, Row, acquire_row};
use crate::window::WindowFunction;

use super::agg::{cell_key, cell_key_ref};
use super::{Db, field_names};

/// Output field for a projected FK join: left column or right probe slot.
#[derive(Debug, Clone)]
pub(crate) enum JoinFieldPlan {
    Left(String),
    Right { out: String, idx: usize },
}

enum JoinLeftSource {
    Idxs(Vec<usize>),
    Scan { len: usize },
}

enum OrdersWinOut {
    Floats(Vec<f64>),
    Ints(Vec<i64>),
}

#[derive(Copy, Clone)]
enum OrdersOrderKey {
    Id,
    Total,
}

#[inline]
fn cmp_f64_sort(a: f64, b: f64) -> std::cmp::Ordering {
    match a.partial_cmp(&b) {
        Some(ord) => ord,
        None => match (a.is_nan(), b.is_nan()) {
            (true, true) => std::cmp::Ordering::Equal,
            (true, false) => std::cmp::Ordering::Greater,
            (false, true) => std::cmp::Ordering::Less,
            (false, false) => std::cmp::Ordering::Equal,
        },
    }
}

#[inline]
fn cmp_col_rows(col: &Column, r1: usize, r2: usize) -> std::cmp::Ordering {
    match col {
        Column::Nulls { .. } => std::cmp::Ordering::Equal,
        Column::Int { vals, valid } | Column::Time { vals, valid } => {
            if valid.null_count() == 0 {
                vals[r1].cmp(&vals[r2])
            } else {
                match (valid.is_valid(r1), valid.is_valid(r2)) {
                    (true, true) => vals[r1].cmp(&vals[r2]),
                    (false, false) => std::cmp::Ordering::Equal,
                    _ => super::eval::cmp_sort(&col.get(r1), &col.get(r2)),
                }
            }
        }
        Column::Float { vals, valid } => {
            if valid.null_count() == 0 {
                cmp_f64_sort(vals[r1], vals[r2])
            } else {
                match (valid.is_valid(r1), valid.is_valid(r2)) {
                    (true, true) => cmp_f64_sort(vals[r1], vals[r2]),
                    (false, false) => std::cmp::Ordering::Equal,
                    _ => super::eval::cmp_sort(&col.get(r1), &col.get(r2)),
                }
            }
        }
        Column::Bool { vals, valid } => {
            if valid.null_count() == 0 {
                vals[r1].cmp(&vals[r2])
            } else {
                match (valid.is_valid(r1), valid.is_valid(r2)) {
                    (true, true) => vals[r1].cmp(&vals[r2]),
                    (false, false) => std::cmp::Ordering::Equal,
                    _ => super::eval::cmp_sort(&col.get(r1), &col.get(r2)),
                }
            }
        }
        Column::Text { .. } | Column::Dict { .. } => match (col.text_at(r1), col.text_at(r2)) {
            (Some(sa), Some(sb)) => sa.cmp(sb),
            (None, None) => std::cmp::Ordering::Equal,
            _ => super::eval::cmp_sort(&col.get(r1), &col.get(r2)),
        },
        Column::Cells(c) => super::eval::cmp_sort(&c[r1], &c[r2]),
    }
}

#[inline]
fn sort_indices_cmp(part: &mut [u32], mut cmp: impl FnMut(u32, u32) -> std::cmp::Ordering) {
    if part.len() <= 32 {
        for i in 1..part.len() {
            let cur = part[i];
            let mut j = i;
            while j > 0 && cmp(part[j - 1], cur) == std::cmp::Ordering::Greater {
                part[j] = part[j - 1];
                j -= 1;
            }
            part[j] = cur;
        }
    } else {
        part.sort_by(|&a, &b| cmp(a, b));
    }
}

#[inline]
fn sort_orders_partition(
    part: &mut [u32],
    order_key: OrdersOrderKey,
    desc: bool,
    orders_id: &[Arc<str>],
    orders_total: &[f64],
) {
    if part.len() <= 1 {
        return;
    }
    match (order_key, desc) {
        (OrdersOrderKey::Id, false) => sort_indices_cmp(part, |a, b| {
            orders_id[a as usize]
                .as_ref()
                .cmp(orders_id[b as usize].as_ref())
        }),
        (OrdersOrderKey::Id, true) => sort_indices_cmp(part, |a, b| {
            orders_id[b as usize]
                .as_ref()
                .cmp(orders_id[a as usize].as_ref())
        }),
        (OrdersOrderKey::Total, false) => sort_indices_cmp(part, |a, b| {
            cmp_f64_sort(orders_total[a as usize], orders_total[b as usize])
        }),
        (OrdersOrderKey::Total, true) => sort_indices_cmp(part, |a, b| {
            cmp_f64_sort(orders_total[b as usize], orders_total[a as usize])
        }),
    }
}

/// Sets `prefix.key` on `row`. The name is assembled in a reused buffer: joins call this
/// once per right-hand cell of every output row, and `Row::insert` only borrows the key.
#[inline]
pub(crate) fn insert_prefixed(row: &mut Row, prefix: &str, key: &str, value: Cell) {
    thread_local! {
        static NAME: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
    }
    NAME.with(|name| {
        let mut name = name.borrow_mut();
        name.clear();
        name.reserve(prefix.len() + 1 + key.len());
        name.push_str(prefix);
        name.push('.');
        name.push_str(key);
        row.insert(name.as_str(), value);
    });
}

#[derive(Copy, Clone)]
enum MergeCellSrc {
    Left(usize),
    Right(usize),
}

struct CachedJoinMerge {
    left_shape: Arc<crate::store::RowShape>,
    right_shape: Arc<crate::store::RowShape>,
    merged_shape: Arc<crate::store::RowShape>,
    sources: Vec<MergeCellSrc>,
}

struct JoinRowMerger<'a> {
    right_col: &'a str,
    cache: smallvec::SmallVec<[CachedJoinMerge; 2]>,
    buf: smallvec::SmallVec<[Cell; 8]>,
}

impl<'a> JoinRowMerger<'a> {
    #[inline]
    fn new(right_col: &'a str) -> Self {
        Self {
            right_col,
            cache: smallvec::SmallVec::new(),
            buf: smallvec::SmallVec::new(),
        }
    }

    #[inline]
    fn merge(&mut self, mut left: Row, right: &Row) -> Row {
        let (Some(ls), Some(rs)) = (left.shape_arc().cloned(), right.shape_arc()) else {
            return merge_join_row(left, right, self.right_col);
        };
        let pos = if let Some(idx) = self
            .cache
            .iter()
            .position(|c| Arc::ptr_eq(&c.left_shape, &ls) && Arc::ptr_eq(&c.right_shape, rs))
        {
            idx
        } else {
            let mut map: BTreeMap<String, MergeCellSrc> = BTreeMap::new();
            for (li, k) in ls.names().iter().enumerate() {
                map.insert(k.clone(), MergeCellSrc::Left(li));
            }
            for (ri, k) in rs.names().iter().enumerate() {
                let mut pref = String::with_capacity(self.right_col.len() + 1 + k.len());
                pref.push_str(self.right_col);
                pref.push('.');
                pref.push_str(k);
                map.insert(pref, MergeCellSrc::Right(ri));
            }
            let sorted_names: Vec<String> = map.keys().cloned().collect();
            let sources: Vec<MergeCellSrc> = map.into_values().collect();
            let merged_shape = Row::shape_of(&sorted_names);
            self.cache.push(CachedJoinMerge {
                left_shape: ls,
                right_shape: Arc::clone(rs),
                merged_shape,
                sources,
            });
            self.cache.len() - 1
        };
        let entry = &self.cache[pos];
        let right_cells = right.cells_slice();
        let left_cells = left.cells_mut();
        self.buf.clear();
        self.buf.reserve(entry.sources.len());
        for &src in &entry.sources {
            match src {
                MergeCellSrc::Left(li) => {
                    self.buf
                        .push(std::mem::replace(&mut left_cells[li], Cell::Null));
                }
                MergeCellSrc::Right(ri) => {
                    self.buf.push(right_cells[ri].clone_for_borrow());
                }
            }
        }
        left.reset_from_parts(Arc::clone(&entry.merged_shape), self.buf.drain(..));
        left
    }
}

#[inline]
fn batch_max_needed(skip_n: usize, explicit_take: Option<Option<i64>>) -> Option<usize> {
    match explicit_take {
        Some(Some(t)) => Some(skip_n.saturating_add(t.max(0) as usize)),
        Some(None) => None,
        None => Some(skip_n.saturating_add(50)),
    }
}

pub(crate) fn merge_join_row(mut left: Row, right: &Row, right_col: &str) -> Row {
    for (k, v) in right {
        insert_prefixed(&mut left, right_col, k, v.clone_for_borrow());
    }
    left
}

pub(crate) fn push_project_batch_row(batch: &mut RecordBatch, row: &Row, names: &[String]) {
    for (col_i, name) in names.iter().enumerate() {
        batch.cols[col_i].push(
            row.get(name)
                .map(Cell::clone_for_borrow)
                .unwrap_or(Cell::Null),
        );
    }
}

#[inline]
fn push_project_batch_row_slots(
    batch: &mut RecordBatch,
    row: &Row,
    slots: &mut [crate::store::row::FieldSlot<'_>],
) {
    for (col_i, slot) in slots.iter_mut().enumerate() {
        batch.cols[col_i].push(
            slot.get(row)
                .map(Cell::clone_for_borrow)
                .unwrap_or(Cell::Null),
        );
    }
}

enum JoinBatchSlot<'a> {
    Left(crate::store::row::FieldSlot<'a>),
    Right(usize),
}

#[inline]
fn build_join_batch_slots(plan: &[JoinFieldPlan]) -> Vec<JoinBatchSlot<'_>> {
    plan.iter()
        .map(|p| match p {
            JoinFieldPlan::Left(k) => JoinBatchSlot::Left(crate::store::row::FieldSlot::new(k)),
            JoinFieldPlan::Right { idx, .. } => JoinBatchSlot::Right(*idx),
        })
        .collect()
}

#[inline]
fn push_join_batch_row_slots(
    batch: &mut RecordBatch,
    left: &Row,
    right: Option<&[Cell]>,
    slots: &mut [JoinBatchSlot<'_>],
) {
    for (col_i, s) in slots.iter_mut().enumerate() {
        let cell = match s {
            JoinBatchSlot::Left(slot) => slot
                .get(left)
                .map(Cell::clone_for_borrow)
                .unwrap_or(Cell::Null),
            JoinBatchSlot::Right(idx) => right
                .and_then(|r| r.get(*idx).map(Cell::clone_for_borrow))
                .unwrap_or(Cell::Null),
        };
        batch.cols[col_i].push(cell);
    }
}

pub(crate) fn finish_batch(
    batch: &mut RecordBatch,
    skip_n: usize,
    explicit_take: Option<Option<i64>>,
) {
    let n = batch.n();
    if skip_n >= n {
        batch.truncate(0);
    } else if skip_n > 0 {
        batch.drain_prefix(skip_n);
    }
    match explicit_take {
        Some(Some(t)) => {
            let t = t.max(0) as usize;
            if batch.n() > t {
                batch.truncate(t);
            }
        }
        Some(None) => {}
        None if batch.n() > 50 => batch.truncate(50),
        None => {}
    }
}

/// `orders | total > N` — used by SoA join / cursor hot paths.
pub(crate) fn pred_total_gt(pred: &Pred) -> Option<f64> {
    match pred {
        Pred::Cmp {
            field,
            op: CmpOp::Gt,
            value,
        } if field.leaf() == Some("total") => match value {
            Value::Float(n) => Some(*n),
            Value::Int(n) => Some(*n as f64),
            _ => None,
        },
        _ => None,
    }
}

pub(crate) fn rows_to_rough_batch(rows: &[Row]) -> RecordBatch {
    RecordBatch::from_rows(rows)
}

pub(crate) fn plan_join_fields(
    right_col: &str,
    names: &[String],
) -> (Vec<String>, Vec<JoinFieldPlan>) {
    let mut right_fields = Vec::new();
    let mut plan = Vec::with_capacity(names.len());
    for f in names {
        if let Some(rest) = f.strip_prefix(right_col).and_then(|s| s.strip_prefix('.')) {
            let idx = if let Some(i) = right_fields.iter().position(|x| x == rest) {
                i
            } else {
                right_fields.push(rest.to_string());
                right_fields.len() - 1
            };
            plan.push(JoinFieldPlan::Right {
                out: f.clone(),
                idx,
            });
        } else {
            plan.push(JoinFieldPlan::Left(f.clone()));
        }
    }
    (right_fields, plan)
}

pub(crate) fn build_right_probe<'a>(
    rights: &'a [Row],
    right_fields: &[String],
) -> FxHashMap<&'a str, Vec<Cell>> {
    let mut probe = FxHashMap::default();
    probe.reserve(rights.len());
    let mut id_slot = crate::store::row::FieldSlot::new("id");
    let mut field_slots: Vec<crate::store::row::FieldSlot<'_>> = right_fields
        .iter()
        .map(|f| crate::store::row::FieldSlot::new(f.as_str()))
        .collect();
    for r in rights {
        let Some(id) = id_slot.get(r).and_then(Cell::text) else {
            continue;
        };
        let cells = field_slots
            .iter_mut()
            .map(|s| s.get(r).map(Cell::clone_for_borrow).unwrap_or(Cell::Null))
            .collect();
        probe.insert(id, cells);
    }
    probe
}

pub(crate) fn emit_join_row(left: &Row, right: Option<&[Cell]>, plan: &[JoinFieldPlan]) -> Row {
    let mut out = acquire_row();
    for p in plan {
        match p {
            JoinFieldPlan::Left(k) => {
                let cell = left
                    .get(k)
                    .map(Cell::clone_for_borrow)
                    .unwrap_or(Cell::Null);
                out.insert(k.as_str(), cell);
            }
            JoinFieldPlan::Right { out: k, idx } => {
                let cell = right
                    .and_then(|r| r.get(*idx))
                    .map(Cell::clone_for_borrow)
                    .unwrap_or(Cell::Null);
                out.insert(k.as_str(), cell);
            }
        }
    }
    out
}

/// Project join output without building a full merged left∪right map.
pub(crate) fn project_join_fields(
    left: &Row,
    right: Option<&Row>,
    right_col: &str,
    fields: &[String],
) -> Row {
    let mut out = acquire_row();
    for f in fields {
        let cell = if let Some(rest) = f.strip_prefix(right_col).and_then(|s| s.strip_prefix('.')) {
            right
                .and_then(|r| r.get(rest))
                .map(Cell::clone_for_borrow)
                .unwrap_or(Cell::Null)
        } else {
            left.get(f)
                .map(Cell::clone_for_borrow)
                .unwrap_or(Cell::Null)
        };
        out.insert(f.as_str(), cell);
    }
    out
}

impl Db {
    /// `col | pred | { fields } | take …` — project from hot columns when possible.
    pub(super) fn try_filter_project(
        &self,
        q: &Query,
        bindings: &BTreeMap<String, Vec<Row>>,
        now: i64,
    ) -> Option<Vec<Row>> {
        let Source::Collection(name) = &q.source else {
            return None;
        };
        if bindings.get(name).is_some() {
            return None;
        }
        if self.store.get_col_count(name) > self.quotas.max_execution_work {
            return None;
        }
        let mut filter: Option<&Pred> = None;
        let mut fields: Option<&[Field]> = None;
        let mut explicit_take: Option<Option<i64>> = None;
        let mut skip_n: usize = 0;
        let mut phase = 0;
        for step in &q.steps {
            let next_phase = match step {
                Step::Filter(_) => 0,
                Step::Project(_) => 1,
                Step::Skip { .. } => 2,
                Step::Take { .. } => 3,
                _ => return None,
            };
            if next_phase < phase || (next_phase == 3 && explicit_take.is_some()) {
                return None;
            }
            phase = next_phase;
            match step {
                Step::Filter(p) => {
                    if filter.is_some() {
                        return None;
                    }
                    filter = Some(p);
                }
                Step::Project(f) => {
                    if fields.is_some() {
                        return None;
                    }
                    fields = Some(f.as_slice());
                }
                Step::Skip { n } => {
                    skip_n = skip_n.saturating_add((*n).max(0) as usize);
                }
                Step::Take { n } => {
                    explicit_take = Some(*n);
                }
                _ => return None,
            }
        }
        let pred = filter?;
        let fields = fields?;
        let names = field_names(fields);
        if names.is_empty() {
            return None;
        }

        let dict_limit = match explicit_take {
            Some(Some(n)) => Some(skip_n.saturating_add(n.max(0) as usize)),
            Some(None) => None,
            None => Some(skip_n.saturating_add(50)),
        };

        if !self.store.is_disk_backed(name) && self.has_matching_interval_index(name, pred, now) {
            return None;
        }
        let mut rows = {
            if self.store.is_disk_backed(name)
                && let Some(eng) = self.store.disk_engine_read()
            {
                let proj_refs: Vec<&str> = names.iter().map(String::as_str).collect();
                let mut r = eng
                    .query_filter(name, Some(pred), dict_limit, Some(&proj_refs), now)
                    .ok()?;
                for row in &mut r {
                    if row.len() < names.len() {
                        for f in &names {
                            if !row.contains_key(f.as_str()) {
                                row.insert(f.clone(), Cell::Null);
                            }
                        }
                    }
                }
                r
            } else if name == "docs"
                && let Pred::Contains { field, needle } = pred
                && field.leaf() == Some("title")
            {
                self.store
                    .scan_docs_hot_project(&names, Some(needle.as_str()))?
            } else if name == "docs"
                && let Some(rows) =
                    self.try_docs_direct_id_title(pred, None, false, now, &names, dict_limit)
            {
                rows
            } else if name == "docs"
                && let Pred::Cmp {
                    field,
                    op: CmpOp::Eq,
                    value: Value::String(target),
                } = pred
                && let Some(col_field @ ("wing" | "layer")) = field.leaf()
                && let Some(rows) =
                    self.store
                        .project_docs_dict_eq(col_field, target.as_ref(), &names, dict_limit)
            {
                rows
            } else if let Some((uses, covered)) =
                crate::index::pick_index_with_coverage(&self.catalog, name, pred)
            {
                if name == "docs"
                    && covered
                    && let Some(rows) = self.try_docs_direct_id_title(
                        pred,
                        Some(&uses),
                        true,
                        now,
                        &names,
                        dict_limit,
                    )
                {
                    rows
                } else if let Some(idxs) = self.store.index_seek(name, &uses, now) {
                    if name == "docs" && covered {
                        if let Some(rows) = self.store.project_docs_hot(&idxs, &names) {
                            rows
                        } else {
                            self.fetch_idxs_filtered(name, &idxs, pred, now, Some(fields), covered)
                        }
                    } else {
                        self.fetch_idxs_filtered(name, &idxs, pred, now, Some(fields), covered)
                    }
                } else if let Some(idxs) = self.mirror_select(name, pred, now) {
                    let start = skip_n.min(idxs.len());
                    let take_n = match explicit_take {
                        Some(Some(n)) => Some(n.max(0) as usize),
                        Some(None) => None,
                        None => Some(50),
                    };
                    let end = match take_n {
                        Some(t) => start.saturating_add(t).min(idxs.len()),
                        None => idxs.len(),
                    };
                    return Some(self.fetch_idxs_filtered(
                        name,
                        &idxs[start..end],
                        pred,
                        now,
                        Some(fields),
                        true,
                    ));
                } else {
                    self.filter_scan(name, pred, now, Some(fields))
                }
            } else if let Some(idxs) = self.mirror_select(name, pred, now) {
                let start = skip_n.min(idxs.len());
                let take_n = match explicit_take {
                    Some(Some(n)) => Some(n.max(0) as usize),
                    Some(None) => None,
                    None => Some(50),
                };
                let end = match take_n {
                    Some(t) => start.saturating_add(t).min(idxs.len()),
                    None => idxs.len(),
                };
                return Some(self.fetch_idxs_filtered(
                    name,
                    &idxs[start..end],
                    pred,
                    now,
                    Some(fields),
                    true,
                ));
            } else {
                self.filter_scan(name, pred, now, Some(fields))
            }
        };

        if skip_n >= rows.len() {
            rows.clear();
        } else if skip_n > 0 {
            rows.drain(0..skip_n);
        }

        match explicit_take {
            Some(Some(n)) => {
                let n = n.max(0) as usize;
                if rows.len() > n {
                    rows.truncate(n);
                }
            }
            Some(None) => {}
            None if rows.len() > 50 => rows.truncate(50),
            None => {}
        }
        Some(rows)
    }

    /// `[filter?] | join | { fields } | take…` — row-API wrapper over columnar join.
    pub(super) fn try_join_project(
        &self,
        q: &Query,
        bindings: &BTreeMap<String, Vec<Row>>,
        now: i64,
    ) -> Option<Vec<Row>> {
        Some(self.try_join_project_batch(q, bindings, now)?.into_rows())
    }

    /// Columnar `filter? | window+ | sort? | project | skip* | take`: only the columns the
    /// windows, the sort and the projection need are read (one batch scan), partitions and ordering are computed on
    /// row indices, and no per-row map exists until the caller asks for rows.
    ///
    /// Produces exactly what the row pipeline does, in the same order (partitions in
    /// first-seen order, each in window order). Shapes whose semantics depend on details the
    /// row pipeline owns are left to it: no explicit `take` (implicit limits), a window
    /// reading another window's output, nested fields, an output name shadowing a column.
    pub(super) fn try_window_project(
        &self,
        q: &Query,
        bindings: &BTreeMap<String, Vec<Row>>,
        now: i64,
    ) -> Option<Vec<Row>> {
        if let Some(rows) = self.try_orders_single_window_soa(q, bindings, |fields, as_field, flat, out| {
            let n = flat.len();
            let orders_id = &self.store.orders_id;
            if fields.len() == 2
                && let (Some(f0), Some(f1)) = (fields[0].leaf(), fields[1].leaf())
                && ((f0 == "id" && f1 == as_field) || (f1 == "id" && f0 == as_field))
            {
                let shape = Row::shape_of(&[f0, f1]);
                let swap = shape.names()[0].as_str() != "id";
                let mut rows = if n >= 1024 {
                    crate::store::take_general_rows().unwrap_or_else(|| Vec::with_capacity(n))
                } else {
                    Vec::with_capacity(n)
                };
                if rows.capacity() < n {
                    rows.reserve(n - rows.len());
                }
                let reuse_len = rows.len().min(n);
                match out {
                    OrdersWinOut::Floats(vals) => {
                        for i in 0..reuse_len {
                            let c_id = Cell::from_arc_str(&orders_id[flat[i] as usize]);
                            let c_win = Cell::Float(vals[i]);
                            let (a, b) = if swap { (c_win, c_id) } else { (c_id, c_win) };
                            rows[i].set_pair_with_shape(&shape, a, b);
                        }
                        for i in reuse_len..n {
                            let c_id = Cell::from_arc_str(&orders_id[flat[i] as usize]);
                            let c_win = Cell::Float(vals[i]);
                            let (a, b) = if swap { (c_win, c_id) } else { (c_id, c_win) };
                            rows.push(Row::from_parts(Arc::clone(&shape), [a, b]));
                        }
                    }
                    OrdersWinOut::Ints(vals) => {
                        for i in 0..reuse_len {
                            let c_id = Cell::from_arc_str(&orders_id[flat[i] as usize]);
                            let c_win = Cell::Int(vals[i]);
                            let (a, b) = if swap { (c_win, c_id) } else { (c_id, c_win) };
                            rows[i].set_pair_with_shape(&shape, a, b);
                        }
                        for i in reuse_len..n {
                            let c_id = Cell::from_arc_str(&orders_id[flat[i] as usize]);
                            let c_win = Cell::Int(vals[i]);
                            let (a, b) = if swap { (c_win, c_id) } else { (c_id, c_win) };
                            rows.push(Row::from_parts(Arc::clone(&shape), [a, b]));
                        }
                    }
                }
                rows.truncate(n);
                return rows;
            }
            self.build_orders_window_batch(fields, as_field, flat, out).into_rows()
        }) {
            return Some(rows);
        }
        if let Some(rows) = self.try_window_sparse_rows(q, bindings, now) {
            return Some(rows);
        }
        self.try_window_project_batch(q, bindings, now)
            .map(RecordBatch::into_rows)
    }

    fn build_orders_window_batch(
        &self,
        fields: &[Field],
        as_field: &str,
        flat: &[u32],
        out: OrdersWinOut,
    ) -> RecordBatch {
        let orders_id = &self.store.orders_id;
        let orders_uid = &self.store.orders_user_id;
        let orders_total = &self.store.orders_total;
        let mut out_opt = Some(out);
        let mut win_refs = fields
            .iter()
            .filter(|f| f.leaf() == Some(as_field))
            .count();
        let mut out_cols = Vec::with_capacity(fields.len());
        for f in fields {
            match f.leaf().unwrap_or("") {
                "id" => out_cols.push(Column::Cells(
                    flat.iter()
                        .map(|&r| Cell::from_arc_str(&orders_id[r as usize]))
                        .collect(),
                )),
                "user_id" => out_cols.push(Column::Cells(
                    flat.iter()
                        .map(|&r| Cell::from_arc_str(&orders_uid[r as usize]))
                        .collect(),
                )),
                "total" => out_cols.push(Column::from_floats(
                    flat.iter().map(|&r| orders_total[r as usize]).collect(),
                )),
                leaf if leaf == as_field => {
                    win_refs -= 1;
                    if win_refs == 0 {
                        let col = match out_opt.take().unwrap() {
                            OrdersWinOut::Floats(v) => Column::from_floats(v),
                            OrdersWinOut::Ints(v) => Column::from_ints(v),
                        };
                        out_cols.push(col);
                    } else {
                        let col = match out_opt.as_ref().unwrap() {
                            OrdersWinOut::Floats(v) => Column::from_floats(v.clone()),
                            OrdersWinOut::Ints(v) => Column::from_ints(v.clone()),
                        };
                        out_cols.push(col);
                    }
                }
                _ => unreachable!(),
            }
        }
        RecordBatch {
            names: field_names(fields).into(),
            cols: out_cols,
        }
    }

    fn try_orders_single_window_soa<R>(
        &self,
        q: &Query,
        bindings: &BTreeMap<String, Vec<Row>>,
        build: impl Fn(&[Field], &str, &[u32], OrdersWinOut) -> R,
    ) -> Option<R> {
        #[derive(Default)]
        struct OrdersWinScratch {
            slots: Vec<u32>,
            counts: Vec<u32>,
            bounds: Vec<u32>,
            next_at: Vec<u32>,
            flat: Vec<u32>,
        }
        thread_local! {
            static SCRATCH: std::cell::RefCell<OrdersWinScratch> =
                std::cell::RefCell::new(OrdersWinScratch::default());
        }
        let Source::Collection(name) = &q.source else {
            return None;
        };
        if name != "orders" || bindings.get("orders").is_some() || self.store.is_disk_backed("orders")
        {
            return None;
        }
        if self.store.get_col_count("orders") > self.quotas.max_execution_work {
            return None;
        }
        let mut window_step: Option<&Step> = None;
        let mut fields: Option<&[Field]> = None;
        let mut skip_n = 0usize;
        let mut take: Option<Option<i64>> = None;
        for step in &q.steps {
            match step {
                Step::Window { .. } if window_step.is_none() && fields.is_none() => {
                    window_step = Some(step)
                }
                Step::Project(f)
                    if window_step.is_some() && fields.is_none() && skip_n == 0 && take.is_none() =>
                {
                    fields = Some(f.as_slice())
                }
                Step::Skip { n } if window_step.is_some() && take.is_none() => {
                    skip_n = skip_n.saturating_add((*n).max(0) as usize);
                }
                Step::Take { n } if window_step.is_some() && take.is_none() => take = Some(*n),
                _ => return None,
            }
        }
        let Some(Step::Window {
            func,
            field,
            by,
            order,
            frame: None,
            as_field,
        }) = window_step
        else {
            return None;
        };
        if skip_n != 0 || !matches!(take, Some(None)) {
            return None;
        }
        let col_rows = self.store.collection("orders");
        let n = col_rows.len();
        if n == 0 {
            return None;
        }
        let synth_fields: Option<[Field; 4]> = if fields.is_none() {
            let s = col_rows[0].shape_arc()?;
            if s.names().len() != 3
                || !col_rows
                    .iter()
                    .all(|r| r.shape_arc().is_some_and(|rs| Arc::ptr_eq(rs, s)))
            {
                return None;
            }
            let mut ns = ["id", "total", "user_id", as_field.as_str()];
            ns.sort_unstable();
            Some(ns.map(Field::name))
        } else {
            None
        };
        let fields: &[Field] = match (fields, &synth_fields) {
            (Some(f), _) => f,
            (None, Some(sf)) => sf.as_slice(),
            (None, None) => unreachable!(),
        };
        let orders_id = &self.store.orders_id;
        let orders_uid = &self.store.orders_user_id;
        let orders_total = &self.store.orders_total;
        let orders_tag = &self.store.orders_total_tag;
        if orders_id.len() != n
            || orders_uid.len() != n
            || orders_total.len() != n
            || orders_tag.len() != n
        {
            return None;
        }
        if by.len() != 1 || by[0].leaf() != Some("user_id") || order.len() != 1 {
            return None;
        }
        if matches!(as_field.as_str(), "id" | "user_id" | "total") {
            return None;
        }
        let ord_desc = order[0].desc;
        let order_key = match order[0].field.leaf()? {
            "id" => OrdersOrderKey::Id,
            "total" => OrdersOrderKey::Total,
            _ => return None,
        };
        let is_sum_total = matches!(func, WindowFunction::Sum)
            && field.as_ref().and_then(Field::leaf) == Some("total");
        if !is_sum_total
            && !matches!(
                func,
                WindowFunction::Rank | WindowFunction::DenseRank | WindowFunction::RowNumber
            )
        {
            return None;
        }
        let mut need_id = matches!(order_key, OrdersOrderKey::Id);
        let mut need_total = is_sum_total || matches!(order_key, OrdersOrderKey::Total);
        for f in fields {
            let leaf = f.leaf()?;
            match leaf {
                "id" => need_id = true,
                "user_id" => {}
                "total" => need_total = true,
                s if s == as_field => {}
                _ => return None,
            }
        }
        if !orders_uid.iter().all(|s| !s.is_empty())
            || (need_id && !orders_id.iter().all(|s| !s.is_empty()))
            || (need_total && !orders_tag.iter().all(|&t| t == 1))
        {
            return None;
        }

        let run_with_scratch = |scratch: &mut OrdersWinScratch| -> R {
            scratch.slots.clear();
            scratch.slots.reserve(n);
            scratch.counts.clear();
            let mut slot_of: FxHashMap<&str, u32> =
                FxHashMap::with_capacity_and_hasher((n / 8).clamp(16, 4096), Default::default());
            for uid in orders_uid {
                let next = scratch.counts.len() as u32;
                let slot = *slot_of.entry(uid.as_ref()).or_insert_with(|| {
                    scratch.counts.push(0);
                    next
                });
                scratch.counts[slot as usize] += 1;
                scratch.slots.push(slot);
            }
            scratch.bounds.clear();
            scratch.bounds.reserve(scratch.counts.len() + 1);
            scratch.next_at.clear();
            scratch.next_at.reserve(scratch.counts.len());
            let mut acc = 0u32;
            for &c in &scratch.counts {
                scratch.bounds.push(acc);
                scratch.next_at.push(acc);
                acc += c;
            }
            scratch.bounds.push(acc);
            scratch.flat.resize(n, 0);
            for (pos, &slot) in scratch.slots.iter().enumerate() {
                let at = &mut scratch.next_at[slot as usize];
                scratch.flat[*at as usize] = pos as u32;
                *at += 1;
            }

            let out = if is_sum_total {
                let mut out_f64: Vec<f64> = Vec::with_capacity(n);
                for range in scratch.bounds.windows(2) {
                    let part = &mut scratch.flat[range[0] as usize..range[1] as usize];
                    sort_orders_partition(part, order_key, ord_desc, orders_id, orders_total);
                    let mut running = 0.0f64;
                    for &r in part.iter() {
                        running += orders_total[r as usize];
                        out_f64.push(running);
                    }
                }
                OrdersWinOut::Floats(out_f64)
            } else {
                let mut out_i64: Vec<i64> = Vec::with_capacity(n);
                for range in scratch.bounds.windows(2) {
                    let part = &mut scratch.flat[range[0] as usize..range[1] as usize];
                    if part.is_empty() {
                        continue;
                    }
                    sort_orders_partition(part, order_key, ord_desc, orders_id, orders_total);
                    match func {
                        WindowFunction::RowNumber => {
                            out_i64.extend(1..=part.len() as i64);
                        }
                        WindowFunction::Rank | WindowFunction::DenseRank => {
                            let is_rank = matches!(func, WindowFunction::Rank);
                            let mut rank = 1i64;
                            out_i64.push(1);
                            for i in 1..part.len() {
                                let a = part[i - 1] as usize;
                                let b = part[i] as usize;
                                let tied = match order_key {
                                    OrdersOrderKey::Id => {
                                        orders_id[a].as_ref() == orders_id[b].as_ref()
                                    }
                                    OrdersOrderKey::Total => {
                                        cmp_f64_sort(orders_total[a], orders_total[b])
                                            == std::cmp::Ordering::Equal
                                    }
                                };
                                if !tied {
                                    rank = if is_rank { (i + 1) as i64 } else { rank + 1 };
                                }
                                out_i64.push(rank);
                            }
                        }
                        _ => unreachable!(),
                    }
                }
                OrdersWinOut::Ints(out_i64)
            };
            build(fields, as_field.as_str(), &scratch.flat, out)
        };
        if let Ok(Some(res)) = SCRATCH.try_with(|c| {
            c.try_borrow_mut()
                .ok()
                .map(|mut guard| run_with_scratch(&mut guard))
        }) {
            return Some(res);
        }
        let mut fallback_scratch = OrdersWinScratch::default();
        Some(run_with_scratch(&mut fallback_scratch))
    }

    pub(super) fn try_window_project_batch(
        &self,
        q: &Query,
        bindings: &BTreeMap<String, Vec<Row>>,
        now: i64,
    ) -> Option<RecordBatch> {
        if let Some(batch) = self.try_orders_single_window_soa(q, bindings, |fields, as_field, flat, out| {
            self.build_orders_window_batch(fields, as_field, flat, out)
        }) {
            return Some(batch);
        }
        let Source::Collection(name) = &q.source else {
            return None;
        };
        if bindings.get(name).is_some() {
            return None;
        }
        if self.store.get_col_count(name) > self.quotas.max_execution_work {
            return None;
        }
        let mut filter: Option<&Pred> = None;
        let mut windows: Vec<&Step> = Vec::new();
        let mut post_filters: Vec<&Pred> = Vec::new();
        let mut group_step: Option<&Step> = None;
        let mut without: Vec<&Field> = Vec::new();
        let mut fields: Option<&[Field]> = None;
        let mut sorts: Vec<(&Field, bool)> = Vec::new();
        let mut skip_n = 0usize;
        let mut take: Option<Option<i64>> = None;
        for step in &q.steps {
            match step {
                Step::Filter(p) if windows.is_empty() && filter.is_none() => filter = Some(p),
                Step::Window { .. }
                    if post_filters.is_empty()
                        && group_step.is_none()
                        && without.is_empty()
                        && fields.is_none()
                        && sorts.is_empty()
                        && skip_n == 0
                        && take.is_none() =>
                {
                    windows.push(step)
                }
                Step::Filter(p)
                    if !windows.is_empty()
                        && group_step.is_none()
                        && without.is_empty()
                        && fields.is_none()
                        && sorts.is_empty()
                        && skip_n == 0
                        && take.is_none() =>
                {
                    post_filters.push(p)
                }
                Step::Count { by: Some(_) }
                | Step::Sum { by: Some(_), .. }
                | Step::Min { by: Some(_), .. }
                | Step::Max { by: Some(_), .. }
                | Step::Avg { by: Some(_), .. }
                    if !windows.is_empty()
                        && group_step.is_none()
                        && without.is_empty()
                        && fields.is_none()
                        && sorts.is_empty()
                        && skip_n == 0
                        && take.is_none() =>
                {
                    #[cfg(feature = "gpu")]
                    if self.gpu.is_some() {
                        return None;
                    }
                    group_step = Some(step);
                }
                Step::Without(wf)
                    if !windows.is_empty()
                        && group_step.is_none()
                        && fields.is_none()
                        && sorts.is_empty()
                        && skip_n == 0
                        && take.is_none() =>
                {
                    if wf.iter().any(|f| f.leaf().is_none()) {
                        return None;
                    }
                    without.extend(wf.iter());
                }
                Step::Sort { field, desc }
                    if !windows.is_empty()
                        && fields
                            .is_none_or(|fs| fs.iter().any(|pf| pf.leaf() == field.leaf()))
                        && skip_n == 0
                        && take.is_none() =>
                {
                    sorts.push((field, *desc));
                }
                Step::Project(f)
                    if !windows.is_empty() && fields.is_none() && skip_n == 0 && take.is_none() =>
                {
                    fields = Some(f.as_slice())
                }
                Step::Skip { n } if !windows.is_empty() && take.is_none() => {
                    skip_n = skip_n.saturating_add((*n).max(0) as usize);
                }
                Step::Take { n } if !windows.is_empty() && take.is_none() => take = Some(*n),
                _ => return None,
            }
        }
        if windows.is_empty() {
            return None;
        }
        // Grouped queries default to `take all` when `take` is omitted; ungrouped window
        // queries require an explicit `take` on the columnar batch path unless grouped.
        let take: Option<i64> = match (group_step.is_some(), take) {
            (_, Some(t)) => t,
            (true, None) => None,
            (false, None) => return None,
        };
        let unprojected = fields.is_none() && group_step.is_none();
        let synth_fields: Option<Vec<Field>> = if unprojected {
            if self.store.is_disk_backed(name) {
                let Some(eng) = self.store.disk_engine_read() else {
                    return None;
                };
                let col_names = eng.uniform_non_null_columns(name)?;
                if col_names.is_empty() {
                    return Some(RecordBatch::empty(Arc::<[String]>::from(
                        Vec::<String>::new(),
                    )));
                }
                let mut out_names: Vec<String> = col_names;
                for w in &windows {
                    if let Step::Window { as_field, .. } = w {
                        if as_field.contains('.') {
                            return None;
                        }
                        out_names.push(as_field.clone());
                    }
                }
                out_names.retain(|nm| !without.iter().any(|wf| wf.leaf() == Some(nm.as_str())));
                out_names.sort_unstable();
                out_names.dedup();
                Some(out_names.into_iter().map(Field::name).collect())
            } else {
                let col_rows = self.store.collection(name);
                if col_rows.is_empty() {
                    return Some(RecordBatch::empty(Arc::<[String]>::from(
                        Vec::<String>::new(),
                    )));
                }
                // Collect distinct shapes across `col_rows` so mixed-sparsity collections
                // work on the columnar `RecordBatch` path too!
                let mut seen_shapes: smallvec::SmallVec<[Arc<crate::store::RowShape>; 8]> =
                    smallvec::SmallVec::new();
                for r in col_rows {
                    let rs = r.shape_arc()?;
                    if !seen_shapes.iter().any(|s| Arc::ptr_eq(s, rs)) {
                        if rs.names().iter().any(|n| n.contains('.')) {
                            return None;
                        }
                        seen_shapes.push(Arc::clone(rs));
                    }
                }
                let mut out_names: Vec<&str> = Vec::new();
                for s in &seen_shapes {
                    for nm in s.names() {
                        out_names.push(nm.as_str());
                    }
                }
                for w in &windows {
                    if let Step::Window { as_field, .. } = w {
                        if as_field.contains('.') {
                            return None;
                        }
                        out_names.push(as_field.as_str());
                    }
                }
                out_names.retain(|nm| !without.iter().any(|wf| wf.leaf() == Some(*nm)));
                out_names.sort_unstable();
                out_names.dedup();
                Some(out_names.into_iter().map(Field::name).collect())
            }
        } else {
            None
        };
        let post_group_fields = fields;
        let fields: &[Field] = if group_step.is_some() {
            &[]
        } else {
            match (fields, &synth_fields) {
                (Some(f), _) => f,
                (None, Some(sf)) => sf.as_slice(),
                (None, None) => unreachable!(),
            }
        };

        // Columns to read: everything the windows, post-filters, group step, sorts, and
        // projected base columns consume.
        let mut base: Vec<Field> = Vec::new();
        let mut win_names: Vec<&str> = Vec::new();
        let want = |f: &Field, win_names: &[&str], base: &mut Vec<Field>| -> Option<()> {
            let leaf = f.leaf()?;
            if win_names.contains(&leaf) {
                return None;
            }
            if !base.iter().any(|b| b.leaf() == Some(leaf)) {
                base.push(f.clone());
            }
            Some(())
        };
        for w in &windows {
            let Step::Window {
                field,
                by,
                order,
                as_field,
                ..
            } = w
            else {
                return None;
            };
            for f in by.iter().chain(field.iter()).chain(order.iter().map(|o| &o.field)) {
                want(f, &win_names, &mut base)?;
            }
            if win_names.contains(&as_field.as_str()) {
                return None;
            }
            win_names.push(as_field.as_str());
        }
        let mut pf_fields: Vec<&Field> = Vec::new();
        for pf in &post_filters {
            pf.walk_fields(&mut pf_fields);
        }
        for pf_f in &pf_fields {
            let leaf = pf_f.leaf()?;
            if !win_names.contains(&leaf) {
                want(pf_f, &[], &mut base)?;
            }
        }
        if let Some(gs) = group_step {
            let (by_f, val_f) = match gs {
                Step::Count { by: Some(by) } => (by, None),
                Step::Sum { field, by: Some(by) }
                | Step::Min { field, by: Some(by) }
                | Step::Max { field, by: Some(by) }
                | Step::Avg { field, by: Some(by) } => (by, Some(field)),
                _ => return None,
            };
            let by_leaf = by_f.leaf()?;
            if !win_names.contains(&by_leaf) {
                want(by_f, &[], &mut base)?;
            }
            if let Some(vf) = val_f {
                let v_leaf = vf.leaf()?;
                if !win_names.contains(&v_leaf) {
                    want(vf, &[], &mut base)?;
                }
            }
        } else {
            for f in fields.iter().chain(sorts.iter().map(|(f, _)| *f)) {
                let leaf = f.leaf()?;
                if !win_names.contains(&leaf) {
                    want(f, &[], &mut base)?;
                }
            }
        }
        if base.iter().any(|b| b.leaf().is_some_and(|l| win_names.contains(&l))) {
            return None;
        }

        let mut steps = Vec::with_capacity(3);
        if let Some(p) = filter {
            steps.push(Step::Filter(p.clone()));
        }
        steps.push(Step::Project(base.clone()));
        steps.push(Step::Take { n: None });
        let base_q = Query {
            source: q.source.clone(),
            steps,
            explain: None,
            ignore_filter: q.ignore_filter,
        };

#[inline]
fn mapped_row(seq_rows: Option<&[u32]>, pos: u32) -> usize {
    match seq_rows {
        Some(s) => s[pos as usize] as usize,
        None => pos as usize,
    }
}

fn sort_part_by_col(part: &mut [u32], col: &Column, desc: bool, seq_rows: Option<&[u32]>) {
    match col {
        Column::Nulls { .. } => {}
        Column::Int { vals, valid } | Column::Time { vals, valid } if valid.null_count() == 0 => {
            if desc {
                sort_indices_cmp(part, |a, b| {
                    vals[mapped_row(seq_rows, b)].cmp(&vals[mapped_row(seq_rows, a)])
                });
            } else {
                sort_indices_cmp(part, |a, b| {
                    vals[mapped_row(seq_rows, a)].cmp(&vals[mapped_row(seq_rows, b)])
                });
            }
        }
        Column::Float { vals, valid } if valid.null_count() == 0 => {
            if desc {
                sort_indices_cmp(part, |a, b| {
                    cmp_f64_sort(vals[mapped_row(seq_rows, b)], vals[mapped_row(seq_rows, a)])
                });
            } else {
                sort_indices_cmp(part, |a, b| {
                    cmp_f64_sort(vals[mapped_row(seq_rows, a)], vals[mapped_row(seq_rows, b)])
                });
            }
        }
        Column::Dict {
            values,
            codes,
            valid,
        } if valid.null_count() == 0 => {
            let mut order: Vec<u32> = (0..values.len() as u32).collect();
            order.sort_unstable_by(|&a, &b| values[a as usize].cmp(&values[b as usize]));
            let mut rank = vec![0u32; values.len()];
            for (r, &code) in order.iter().enumerate() {
                rank[code as usize] = r as u32;
            }
            if desc {
                sort_indices_cmp(part, |a, b| {
                    rank[codes[mapped_row(seq_rows, b)] as usize]
                        .cmp(&rank[codes[mapped_row(seq_rows, a)] as usize])
                });
            } else {
                sort_indices_cmp(part, |a, b| {
                    rank[codes[mapped_row(seq_rows, a)] as usize]
                        .cmp(&rank[codes[mapped_row(seq_rows, b)] as usize])
                });
            }
        }
        Column::Text { valid, .. } if valid.null_count() == 0 => {
            if desc {
                sort_indices_cmp(part, |a, b| {
                    col.text_at(mapped_row(seq_rows, b))
                        .cmp(&col.text_at(mapped_row(seq_rows, a)))
                });
            } else {
                sort_indices_cmp(part, |a, b| {
                    col.text_at(mapped_row(seq_rows, a))
                        .cmp(&col.text_at(mapped_row(seq_rows, b)))
                });
            }
        }
        Column::Cells(cells) => {
            #[inline(always)]
            fn cmp_cell_fast(a: &Cell, b: &Cell) -> std::cmp::Ordering {
                match (a.text(), b.text()) {
                    (Some(sa), Some(sb)) => sa.cmp(sb),
                    _ => super::eval::cmp_sort(a, b),
                }
            }
            match (seq_rows, desc) {
                (None, false) => sort_indices_cmp(part, |a, b| {
                    cmp_cell_fast(&cells[a as usize], &cells[b as usize])
                }),
                (None, true) => sort_indices_cmp(part, |a, b| {
                    cmp_cell_fast(&cells[b as usize], &cells[a as usize])
                }),
                (Some(sr), false) => sort_indices_cmp(part, |a, b| {
                    cmp_cell_fast(
                        &cells[sr[a as usize] as usize],
                        &cells[sr[b as usize] as usize],
                    )
                }),
                (Some(sr), true) => sort_indices_cmp(part, |a, b| {
                    cmp_cell_fast(
                        &cells[sr[b as usize] as usize],
                        &cells[sr[a as usize] as usize],
                    )
                }),
            }
        }
        _ => {
            if desc {
                sort_indices_cmp(part, |a, b| {
                    cmp_col_rows(col, mapped_row(seq_rows, b), mapped_row(seq_rows, a))
                });
            } else {
                sort_indices_cmp(part, |a, b| {
                    cmp_col_rows(col, mapped_row(seq_rows, a), mapped_row(seq_rows, b))
                });
            }
        }
    }
}

fn sort_part(
    part: &mut [u32],
    cols: &[Column],
    order_cols: &[(usize, bool)],
    seq_rows: Option<&[u32]>,
) {
    if part.len() <= 1 || order_cols.is_empty() {
        return;
    }
    if let [(c, desc)] = *order_cols {
        sort_part_by_col(part, &cols[c], desc, seq_rows);
        return;
    }
    // Stable, like the row sort.
    part.sort_by(|&a, &b| {
        let ra = mapped_row(seq_rows, a);
        let rb = mapped_row(seq_rows, b);
        for &(c, desc) in order_cols {
            let ord = cmp_col_rows(&cols[c], ra, rb);
            let ord = if desc { ord.reverse() } else { ord };
            if ord != std::cmp::Ordering::Equal {
                return ord;
            }
        }
        std::cmp::Ordering::Equal
    });
}

fn partition_into(
    cols: &[Column],
    by_cols: &[usize],
    seq_rows: Option<&[u32]>,
    n: usize,
    bounds: &mut Vec<u32>,
    flat: &mut Vec<u32>,
) {
    if by_cols.is_empty() {
        flat.extend(0..n as u32);
        bounds.extend([0, n as u32]);
        return;
    }
    if n == 0 {
        bounds.push(0);
        return;
    }
    if let [c] = *by_cols
        && matches!(&cols[c], Column::Nulls { .. })
    {
        flat.extend(0..n as u32);
        bounds.extend([0, n as u32]);
        return;
    }

    let mut slots: Vec<u32> = Vec::with_capacity(n);
    let mut counts: Vec<u32> = Vec::new();
    let mut assign = |slot: u32, counts: &mut Vec<u32>| {
        if slot as usize == counts.len() {
            counts.push(0);
        }
        counts[slot as usize] += 1;
        slots.push(slot);
    };
    let map_hint = (n / 8).clamp(16, 4096);

    if let [c] = *by_cols {
        match &cols[c] {
            Column::Cells(cells) => {
                if cells.iter().all(|c| c.text().is_some()) {
                    let mut slot_of: FxHashMap<&str, u32> =
                        FxHashMap::with_capacity_and_hasher(map_hint, Default::default());
                    for pos in 0..n {
                        let r = mapped_row(seq_rows, pos as u32);
                        let next = slot_of.len() as u32;
                        let slot = *slot_of.entry(cells[r].text().unwrap()).or_insert(next);
                        assign(slot, &mut counts);
                    }
                } else {
                    let mut slot_of: FxHashMap<&Cell, u32> =
                        FxHashMap::with_capacity_and_hasher(map_hint, Default::default());
                    for pos in 0..n {
                        let r = mapped_row(seq_rows, pos as u32);
                        let next = slot_of.len() as u32;
                        let slot = *slot_of.entry(&cells[r]).or_insert(next);
                        assign(slot, &mut counts);
                    }
                }
            }
            Column::Int { vals, valid } | Column::Time { vals, valid } => {
                if valid.null_count() == 0 {
                    let mut slot_of: FxHashMap<i64, u32> =
                        FxHashMap::with_capacity_and_hasher(map_hint, Default::default());
                    for pos in 0..n {
                        let r = mapped_row(seq_rows, pos as u32);
                        let next = slot_of.len() as u32;
                        let slot = *slot_of.entry(vals[r]).or_insert(next);
                        assign(slot, &mut counts);
                    }
                } else {
                    let mut slot_of: FxHashMap<Option<i64>, u32> =
                        FxHashMap::with_capacity_and_hasher(map_hint, Default::default());
                    for pos in 0..n {
                        let idx = mapped_row(seq_rows, pos as u32);
                        let key = valid.is_valid(idx).then_some(vals[idx]);
                        let next = slot_of.len() as u32;
                        let slot = *slot_of.entry(key).or_insert(next);
                        assign(slot, &mut counts);
                    }
                }
            }
            Column::Bool { vals, valid } => {
                let mut map = [u32::MAX; 3];
                for pos in 0..n {
                    let idx = mapped_row(seq_rows, pos as u32);
                    let bucket = if !valid.is_valid(idx) {
                        0
                    } else if vals[idx] {
                        2
                    } else {
                        1
                    };
                    if map[bucket] == u32::MAX {
                        map[bucket] = counts.len() as u32;
                    }
                    assign(map[bucket], &mut counts);
                }
            }
            Column::Dict {
                values,
                codes,
                valid,
            } => {
                let mut map = vec![u32::MAX; values.len() + 1];
                let null_bucket = values.len();
                for pos in 0..n {
                    let idx = mapped_row(seq_rows, pos as u32);
                    let bucket = if valid.is_valid(idx) {
                        codes[idx] as usize
                    } else {
                        null_bucket
                    };
                    if map[bucket] == u32::MAX {
                        map[bucket] = counts.len() as u32;
                    }
                    assign(map[bucket], &mut counts);
                }
            }
            col @ Column::Text { valid, .. } if valid.null_count() == 0 => {
                let mut slot_of: FxHashMap<&str, u32> =
                    FxHashMap::with_capacity_and_hasher(map_hint, Default::default());
                for pos in 0..n {
                    let r = mapped_row(seq_rows, pos as u32);
                    let next = slot_of.len() as u32;
                    let slot = *slot_of.entry(col.text_at(r).unwrap_or("")).or_insert(next);
                    assign(slot, &mut counts);
                }
            }
            col => {
                let mut slot_of: FxHashMap<Cell, u32> =
                    FxHashMap::with_capacity_and_hasher(map_hint, Default::default());
                for pos in 0..n {
                    let r = mapped_row(seq_rows, pos as u32);
                    let next = slot_of.len() as u32;
                    let slot = *slot_of.entry(col.get(r)).or_insert(next);
                    assign(slot, &mut counts);
                }
            }
        }
    } else if by_cols
        .iter()
        .all(|&c| matches!(&cols[c], Column::Cells(_)))
    {
        let mut slot_of: FxHashMap<smallvec::SmallVec<[&Cell; 2]>, u32> =
            FxHashMap::with_capacity_and_hasher(map_hint, Default::default());
        for pos in 0..n {
            let idx = mapped_row(seq_rows, pos as u32);
            let key: smallvec::SmallVec<[&Cell; 2]> = by_cols
                .iter()
                .map(|&c| match &cols[c] {
                    Column::Cells(cv) => &cv[idx],
                    _ => unreachable!(),
                })
                .collect();
            let next = slot_of.len() as u32;
            let slot = *slot_of.entry(key).or_insert(next);
            assign(slot, &mut counts);
        }
    } else {
        let mut slot_of: FxHashMap<smallvec::SmallVec<[Cell; 2]>, u32> =
            FxHashMap::with_capacity_and_hasher(map_hint, Default::default());
        for pos in 0..n {
            let idx = mapped_row(seq_rows, pos as u32);
            let key: smallvec::SmallVec<[Cell; 2]> =
                by_cols.iter().map(|&c| cols[c].get(idx)).collect();
            let next = slot_of.len() as u32;
            let slot = *slot_of.entry(key).or_insert(next);
            assign(slot, &mut counts);
        }
    }

    bounds.reserve(counts.len() + 1);
    let mut acc = 0u32;
    for &c in &counts {
        bounds.push(acc);
        acc += c;
    }
    bounds.push(acc);
    let mut next_at: Vec<u32> = bounds[..counts.len()].to_vec();
    flat.resize(n, 0);
    for (pos, &slot) in slots.iter().enumerate() {
        let at = &mut next_at[slot as usize];
        flat[*at as usize] = pos as u32;
        *at += 1;
    }
}

        let batch = self.try_filter_project_batch(&base_q, bindings, now)?;
        let mut n = batch.n();
        let mut cols: Vec<Column> = batch.cols;
        let col_of = |f: &Field| -> Option<usize> {
            let leaf = f.leaf()?;
            base.iter().position(|b| b.leaf() == Some(leaf))
        };

        static NULL: Cell = Cell::Null;
        // Current row sequence -> row of the base batch; starts identity (`None` = `0..n`).
        let mut seq_rows: Option<Vec<u32>> = None;
        let mut win_cols: Vec<Column> = Vec::with_capacity(windows.len());
        for w in &windows {
            let Step::Window {
                func,
                field,
                by,
                order,
                frame,
                ..
            } = w
            else {
                return None;
            };
            let by_cols: Vec<usize> = by.iter().map(col_of).collect::<Option<_>>()?;
            let order_cols: Vec<(usize, bool)> = order
                .iter()
                .map(|o| col_of(&o.field).map(|c| (c, o.desc)))
                .collect::<Option<_>>()?;
            let target_col = match field {
                Some(f) => Some(col_of(f)?),
                None => None,
            };

            let mut bounds: Vec<u32> = Vec::new();
            let mut flat: Vec<u32> = Vec::with_capacity(n);
            partition_into(&cols, &by_cols, seq_rows.as_deref(), n, &mut bounds, &mut flat);

            let has_order = !order_cols.is_empty();
            let win_col = if matches!(func, WindowFunction::RowNumber)
                || (matches!(func, WindowFunction::Rank | WindowFunction::DenseRank) && has_order)
            {
                let mut out_i64: Vec<i64> = Vec::with_capacity(n);
                for range in bounds.windows(2) {
                    let part = &mut flat[range[0] as usize..range[1] as usize];
                    sort_part(part, &cols, &order_cols, seq_rows.as_deref());
                    if part.is_empty() {
                        continue;
                    }
                    match func {
                        WindowFunction::RowNumber => {
                            out_i64.extend(1..=part.len() as i64);
                        }
                        WindowFunction::Rank => {
                            let mut rank = 1i64;
                            out_i64.push(1);
                            for i in 1..part.len() {
                                let r_prev = mapped_row(seq_rows.as_deref(), part[i - 1]);
                                let r_cur = mapped_row(seq_rows.as_deref(), part[i]);
                                let tied = order_cols.iter().all(|&(c, _)| {
                                    cmp_col_rows(&cols[c], r_prev, r_cur)
                                        == std::cmp::Ordering::Equal
                                });
                                if !tied {
                                    rank = (i + 1) as i64;
                                }
                                out_i64.push(rank);
                            }
                        }
                        WindowFunction::DenseRank => {
                            let mut dense = 1i64;
                            out_i64.push(1);
                            for i in 1..part.len() {
                                let r_prev = mapped_row(seq_rows.as_deref(), part[i - 1]);
                                let r_cur = mapped_row(seq_rows.as_deref(), part[i]);
                                let tied = order_cols.iter().all(|&(c, _)| {
                                    cmp_col_rows(&cols[c], r_prev, r_cur)
                                        == std::cmp::Ordering::Equal
                                });
                                if !tied {
                                    dense += 1;
                                }
                                out_i64.push(dense);
                            }
                        }
                        _ => unreachable!(),
                    }
                }
                Column::from_ints(out_i64)
            } else if matches!(func, WindowFunction::Sum)
                && frame.is_none()
                && n > 0
                && let Some(c) = target_col
                && let Column::Float { vals, valid } = &cols[c]
                && valid.null_count() == 0
            {
                let mut out_f64: Vec<f64> = Vec::with_capacity(n);
                for range in bounds.windows(2) {
                    let part = &mut flat[range[0] as usize..range[1] as usize];
                    sort_part(part, &cols, &order_cols, seq_rows.as_deref());
                    if has_order {
                        let mut acc = 0.0f64;
                        for &pos in part.iter() {
                            acc += vals[mapped_row(seq_rows.as_deref(), pos)];
                            out_f64.push(acc);
                        }
                    } else {
                        let mut acc = 0.0f64;
                        for &pos in part.iter() {
                            acc += vals[mapped_row(seq_rows.as_deref(), pos)];
                        }
                        out_f64.resize(out_f64.len() + part.len(), acc);
                    }
                }
                Column::from_floats(out_f64)
            } else {
                let owned_target: Option<Vec<Cell>> = match target_col {
                    Some(c) if !matches!(&cols[c], Column::Cells(_)) => {
                        Some((0..n).map(|i| cols[c].get(i)).collect())
                    }
                    _ => None,
                };
                let mut out: Vec<Cell> = Vec::with_capacity(n);
                let mut ties: Vec<bool> = Vec::new();
                let mut tv: Vec<&Cell> = Vec::new();
                for range in bounds.windows(2) {
                    let part = &mut flat[range[0] as usize..range[1] as usize];
                    sort_part(part, &cols, &order_cols, seq_rows.as_deref());
                    ties.clear();
                    if has_order && part.len() > 1 {
                        ties.push(false);
                        for w2 in part.windows(2) {
                            let r1 = mapped_row(seq_rows.as_deref(), w2[0]);
                            let r2 = mapped_row(seq_rows.as_deref(), w2[1]);
                            ties.push(order_cols.iter().all(|&(c, _)| {
                                cmp_col_rows(&cols[c], r1, r2) == std::cmp::Ordering::Equal
                            }));
                        }
                    }
                    tv.clear();
                    match target_col {
                        Some(c) => match &cols[c] {
                            Column::Cells(cv) => {
                                tv.extend(
                                    part.iter()
                                        .map(|&pos| &cv[mapped_row(seq_rows.as_deref(), pos)]),
                                );
                            }
                            _ => {
                                let cv = owned_target.as_deref().unwrap();
                                tv.extend(
                                    part.iter()
                                        .map(|&pos| &cv[mapped_row(seq_rows.as_deref(), pos)]),
                                );
                            }
                        },
                        None => tv.resize(part.len(), &NULL),
                    }
                    out.extend(super::eval::window_values(
                        func,
                        &tv,
                        &ties,
                        has_order,
                        target_col.is_some(),
                        frame.as_ref(),
                    ));
                }
                Column::Cells(out)
            };

            let new_seq = flat;
            // Re-sequence: earlier window outputs move with their rows.
            for wc in &mut win_cols {
                wc.permute_in_place(&new_seq);
            }
            seq_rows = Some(match seq_rows {
                Some(prev) => new_seq.iter().map(|&pos| prev[pos as usize]).collect(),
                None => new_seq,
            });
            win_cols.push(win_col);
        }

        // Post-window `Filter` steps: evaluate over one reused scratch `Row` without
        // materializing `Vec<Row>`.
        if !post_filters.is_empty() && n > 0 {
            let mut pf_uniq: Vec<&str> = pf_fields.iter().filter_map(|f| f.leaf()).collect();
            pf_uniq.sort_unstable();
            pf_uniq.dedup();
            enum PfSrc {
                Win(usize),
                Base(usize),
            }
            let pf_srcs: Vec<PfSrc> = pf_uniq
                .iter()
                .map(|&leaf| {
                    if let Some(wi) = win_names.iter().position(|w| *w == leaf) {
                        Some(PfSrc::Win(wi))
                    } else {
                        base.iter().position(|b| b.leaf() == Some(leaf)).map(PfSrc::Base)
                    }
                })
                .collect::<Option<_>>()?;
            let pf_shape = Row::shape_of(&pf_uniq);
            let mut scratch = crate::store::acquire_row();
            let mut slots: Vec<super::eval::PredSlots<'_>> =
                post_filters.iter().map(|p| super::eval::PredSlots::new(p)).collect();
            let mut keep: Vec<u32> = Vec::with_capacity(n);
            for pos in 0..n {
                let raw_pos = mapped_row(seq_rows.as_deref(), pos as u32);
                scratch.reset_with_shape_ref(
                    &pf_shape,
                    pf_srcs.iter().map(|src| match src {
                        PfSrc::Win(wi) => win_cols[*wi].get(pos),
                        PfSrc::Base(ci) => cols[*ci].get(raw_pos),
                    }),
                );
                if slots.iter_mut().all(|s| s.eval(&scratch, now)) {
                    keep.push(pos as u32);
                }
            }
            if keep.len() < n {
                for wc in &mut win_cols {
                    let old = std::mem::replace(wc, Column::new());
                    *wc = old.gather_unique_owned(&keep);
                }
                seq_rows = Some(match seq_rows {
                    Some(prev) => keep.iter().map(|&p| prev[p as usize]).collect(),
                    None => keep,
                });
                n = win_cols.first().map_or(0, Column::len);
            }
        }

        // Grouped aggregation between window and projection (`count/sum/min/max/avg ... by k`).
        if let Some(gs) = group_step {
            let resolve_src = |f: &Field| -> Option<(&Column, Option<&[u32]>)> {
                let leaf = f.leaf()?;
                if let Some(wi) = win_names.iter().position(|w| *w == leaf) {
                    Some((&win_cols[wi], None))
                } else {
                    let ci = col_of(f)?;
                    Some((&cols[ci], seq_rows.as_deref()))
                }
            };
            #[inline]
            fn key_cost(c: &Cell) -> usize {
                match c {
                    Cell::Text(s) => s.len() + 32,
                    _ => 16,
                }
            }
            let max_bytes = self.quotas.max_aggregate_bytes;
            let mut grouped: RecordBatch = match gs {
                Step::Count { by: Some(by) } => {
                    let by_name = by.leaf()?;
                    let (kcol, kseq) = resolve_src(by)?;
                    let mut entries: Vec<(Cell, i64)> = match kcol {
                        Column::Dict {
                            values,
                            codes,
                            valid,
                        } => {
                            let null_bucket = values.len();
                            let mut counts = vec![0i64; values.len() + 1];
                            for pos in 0..n {
                                let idx = mapped_row(kseq, pos as u32);
                                let b = if valid.is_valid(idx) {
                                    codes[idx] as usize
                                } else {
                                    null_bucket
                                };
                                counts[b] += 1;
                            }
                            let mut state_bytes = 0usize;
                            let mut out = Vec::new();
                            for (code, &cnt) in counts.iter().enumerate() {
                                if cnt == 0 {
                                    continue;
                                }
                                let cell = if code < null_bucket {
                                    Cell::from_arc_str(&values[code])
                                } else {
                                    Cell::Null
                                };
                                state_bytes = state_bytes.saturating_add(key_cost(&cell) + 64);
                                if state_bytes > max_bytes {
                                    return None;
                                }
                                out.push((cell, cnt));
                            }
                            out
                        }
                        Column::Cells(kcells) => {
                            let mut map: FxHashMap<Cell, i64> = FxHashMap::default();
                            let mut state_bytes = 0usize;
                            for pos in 0..n {
                                let kref = &kcells[mapped_row(kseq, pos as u32)];
                                if let Some(cnt) = map.get_mut(kref) {
                                    *cnt += 1;
                                } else {
                                    state_bytes = state_bytes.saturating_add(key_cost(kref) + 64);
                                    if state_bytes > max_bytes {
                                        return None;
                                    }
                                    map.insert(kref.clone_for_borrow(), 1);
                                }
                            }
                            map.into_iter().collect()
                        }
                        _ => {
                            let mut map: FxHashMap<Cell, i64> = FxHashMap::default();
                            let mut state_bytes = 0usize;
                            for pos in 0..n {
                                let kcell = kcol.get(mapped_row(kseq, pos as u32));
                                if let Some(cnt) = map.get_mut(&kcell) {
                                    *cnt += 1;
                                } else {
                                    state_bytes = state_bytes.saturating_add(key_cost(&kcell) + 64);
                                    if state_bytes > max_bytes {
                                        return None;
                                    }
                                    map.insert(kcell, 1);
                                }
                            }
                            map.into_iter().collect()
                        }
                    };
                    entries.sort_unstable_by(|(a, _), (b, _)| super::eval::cmp_sort(a, b));
                    if by_name == "hits" {
                        RecordBatch {
                            names: Arc::from([String::from("hits")]),
                            cols: vec![Column::from_ints(
                                entries.into_iter().map(|(_, c)| c).collect(),
                            )],
                        }
                    } else {
                        let mut kcol_out = Column::with_capacity(entries.len());
                        let mut hvals = Vec::with_capacity(entries.len());
                        for (k, c) in entries {
                            kcol_out.push(k);
                            hvals.push(c);
                        }
                        RecordBatch {
                            names: Arc::from([by_name.to_string(), String::from("hits")]),
                            cols: vec![kcol_out, Column::from_ints(hvals)],
                        }
                    }
                }
                Step::Sum {
                    field: vf,
                    by: Some(by),
                } => {
                    let by_name = by.leaf()?;
                    let val_name = vf.leaf()?;
                    if by_name == val_name {
                        return None;
                    }
                    let (kcol, kseq) = resolve_src(by)?;
                    let (vcol, vseq) = resolve_src(vf)?;
                    let mut map: FxHashMap<Cell, crate::exact_sum::ExactSum> = FxHashMap::default();
                    let mut state_bytes = 0usize;
                    if let Column::Cells(kcells) = kcol {
                        for pos in 0..n {
                            let kref = &kcells[mapped_row(kseq, pos as u32)];
                            let vcell = vcol.get(mapped_row(vseq, pos as u32));
                            if let Some(acc) = map.get_mut(kref) {
                                acc.add_cell(Some(&vcell));
                            } else {
                                state_bytes = state_bytes.saturating_add(key_cost(kref) + 64);
                                if state_bytes > max_bytes {
                                    return None;
                                }
                                let mut acc = crate::exact_sum::ExactSum::default();
                                acc.add_cell(Some(&vcell));
                                map.insert(kref.clone_for_borrow(), acc);
                            }
                        }
                    } else {
                        for pos in 0..n {
                            let kcell = kcol.get(mapped_row(kseq, pos as u32));
                            let vcell = vcol.get(mapped_row(vseq, pos as u32));
                            if let Some(acc) = map.get_mut(&kcell) {
                                acc.add_cell(Some(&vcell));
                            } else {
                                state_bytes = state_bytes.saturating_add(key_cost(&kcell) + 64);
                                if state_bytes > max_bytes {
                                    return None;
                                }
                                let mut acc = crate::exact_sum::ExactSum::default();
                                acc.add_cell(Some(&vcell));
                                map.insert(kcell, acc);
                            }
                        }
                    }
                    let mut entries: Vec<(Cell, f64)> =
                        map.into_iter().map(|(k, s)| (k, s.value())).collect();
                    entries.sort_unstable_by(|(a, _), (b, _)| super::eval::cmp_sort(a, b));
                    let mut kcol_out = Column::with_capacity(entries.len());
                    let mut fvals = Vec::with_capacity(entries.len());
                    for (k, v) in entries {
                        kcol_out.push(k);
                        fvals.push(v);
                    }
                    RecordBatch {
                        names: Arc::from([by_name.to_string(), val_name.to_string()]),
                        cols: vec![kcol_out, Column::from_floats(fvals)],
                    }
                }
                Step::Avg {
                    field: vf,
                    by: Some(by),
                } => {
                    let by_name = by.leaf()?;
                    let val_name = vf.leaf()?;
                    if by_name == val_name {
                        return None;
                    }
                    let (kcol, kseq) = resolve_src(by)?;
                    let (vcol, vseq) = resolve_src(vf)?;
                    let mut map: FxHashMap<Cell, (crate::exact_sum::ExactSum, usize)> =
                        FxHashMap::default();
                    let mut state_bytes = 0usize;
                    if let Column::Cells(kcells) = kcol {
                        for pos in 0..n {
                            let kref = &kcells[mapped_row(kseq, pos as u32)];
                            let vcell = vcol.get(mapped_row(vseq, pos as u32));
                            if let Some(entry) = map.get_mut(kref) {
                                if entry.0.add_numeric(Some(&vcell)) {
                                    entry.1 += 1;
                                }
                            } else {
                                state_bytes = state_bytes.saturating_add(key_cost(kref) + 64);
                                if state_bytes > max_bytes {
                                    return None;
                                }
                                let mut acc = (crate::exact_sum::ExactSum::default(), 0usize);
                                if acc.0.add_numeric(Some(&vcell)) {
                                    acc.1 = 1;
                                }
                                map.insert(kref.clone_for_borrow(), acc);
                            }
                        }
                    } else {
                        for pos in 0..n {
                            let kcell = kcol.get(mapped_row(kseq, pos as u32));
                            let vcell = vcol.get(mapped_row(vseq, pos as u32));
                            if let Some(entry) = map.get_mut(&kcell) {
                                if entry.0.add_numeric(Some(&vcell)) {
                                    entry.1 += 1;
                                }
                            } else {
                                state_bytes = state_bytes.saturating_add(key_cost(&kcell) + 64);
                                if state_bytes > max_bytes {
                                    return None;
                                }
                                let mut acc = (crate::exact_sum::ExactSum::default(), 0usize);
                                if acc.0.add_numeric(Some(&vcell)) {
                                    acc.1 = 1;
                                }
                                map.insert(kcell, acc);
                            }
                        }
                    }
                    let mut entries: Vec<(Cell, f64)> = map
                        .into_iter()
                        .map(|(k, (s, cnt))| (k, if cnt == 0 { 0.0 } else { s.value() / cnt as f64 }))
                        .collect();
                    entries.sort_unstable_by(|(a, _), (b, _)| super::eval::cmp_sort(a, b));
                    let mut kcol_out = Column::with_capacity(entries.len());
                    let mut fvals = Vec::with_capacity(entries.len());
                    for (k, v) in entries {
                        kcol_out.push(k);
                        fvals.push(v);
                    }
                    RecordBatch {
                        names: Arc::from([by_name.to_string(), val_name.to_string()]),
                        cols: vec![kcol_out, Column::from_floats(fvals)],
                    }
                }
                Step::Min {
                    field: vf,
                    by: Some(by),
                }
                | Step::Max {
                    field: vf,
                    by: Some(by),
                } => {
                    let is_max = matches!(gs, Step::Max { .. });
                    let is_better = |cand: &Cell, prev: &Cell| -> bool {
                        let ord = super::eval::cmp_sort(cand, prev);
                        if is_max { ord.is_gt() } else { ord.is_lt() }
                    };
                    let by_name = by.leaf()?;
                    let val_name = vf.leaf()?;
                    if by_name == val_name {
                        return None;
                    }
                    let (kcol, kseq) = resolve_src(by)?;
                    let (vcol, vseq) = resolve_src(vf)?;
                    let mut map: FxHashMap<Cell, Option<Cell>> = FxHashMap::default();
                    let mut state_bytes = 0usize;
                    if let Column::Cells(kcells) = kcol {
                        for pos in 0..n {
                            let kref = &kcells[mapped_row(kseq, pos as u32)];
                            let vcell = vcol.get(mapped_row(vseq, pos as u32));
                            if let Some(slot) = map.get_mut(kref) {
                                if !matches!(vcell, Cell::Null)
                                    && slot.as_ref().is_none_or(|prev| is_better(&vcell, prev))
                                {
                                    *slot = Some(vcell);
                                }
                            } else {
                                state_bytes = state_bytes.saturating_add(key_cost(kref) + 64);
                                if state_bytes > max_bytes {
                                    return None;
                                }
                                let init = (!matches!(vcell, Cell::Null)).then_some(vcell);
                                map.insert(kref.clone_for_borrow(), init);
                            }
                        }
                    } else {
                        for pos in 0..n {
                            let kcell = kcol.get(mapped_row(kseq, pos as u32));
                            let vcell = vcol.get(mapped_row(vseq, pos as u32));
                            if let Some(slot) = map.get_mut(&kcell) {
                                if !matches!(vcell, Cell::Null)
                                    && slot.as_ref().is_none_or(|prev| is_better(&vcell, prev))
                                {
                                    *slot = Some(vcell);
                                }
                            } else {
                                state_bytes = state_bytes.saturating_add(key_cost(&kcell) + 64);
                                if state_bytes > max_bytes {
                                    return None;
                                }
                                let init = (!matches!(vcell, Cell::Null)).then_some(vcell);
                                map.insert(kcell, init);
                            }
                        }
                    }
                    let mut entries: Vec<(Cell, Cell)> = map
                        .into_iter()
                        .map(|(k, opt)| (k, opt.unwrap_or(Cell::Null)))
                        .collect();
                    entries.sort_unstable_by(|(a, _), (b, _)| super::eval::cmp_sort(a, b));
                    let mut kcol_out = Column::with_capacity(entries.len());
                    let mut vcol_out = Column::with_capacity(entries.len());
                    for (k, v) in entries {
                        kcol_out.push(k);
                        vcol_out.push(v);
                    }
                    RecordBatch {
                        names: Arc::from([by_name.to_string(), val_name.to_string()]),
                        cols: vec![kcol_out, vcol_out],
                    }
                }
                _ => return None,
            };
            let gn = grouped.n();
            if !sorts.is_empty() && gn > 1 {
                let mut gperm: Vec<u32> = (0..gn as u32).collect();
                for (sf, desc) in sorts {
                    let leaf = sf.leaf()?;
                    let ci = grouped.names.iter().position(|nm| nm == leaf)?;
                    sort_part_by_col(&mut gperm, &grouped.cols[ci], desc, None);
                }
                for gc in &mut grouped.cols {
                    gc.permute_in_place(&gperm);
                }
            }
            if let Some(pfs) = post_group_fields {
                let mut proj_cols = Vec::with_capacity(pfs.len());
                let mut proj_names = Vec::with_capacity(pfs.len());
                for pf in pfs {
                    let leaf = pf.leaf()?;
                    proj_names.push(leaf.to_string());
                    if let Some(ci) = grouped.names.iter().position(|nm| nm == leaf) {
                        proj_cols.push(grouped.cols[ci].clone());
                    } else {
                        proj_cols.push(Column::Nulls { n: gn, cap: 0 });
                    }
                }
                grouped = RecordBatch {
                    names: proj_names.into(),
                    cols: proj_cols,
                };
            }
            finish_batch(&mut grouped, skip_n, Some(take));
            return Some(grouped);
        }

        // `sort` steps after the windows: sequential stable sorts of the sequence, where
        // each key may be a base column or a window output. Ties keep the prior order.
        for (sort_field, desc) in sorts {
            let leaf = sort_field.leaf()?;
            let win_key = win_names.iter().position(|w| *w == leaf);
            let base_key = match win_key {
                Some(_) => None,
                None => Some(col_of(sort_field)?),
            };
            let mut perm: Vec<u32> = (0..n as u32).collect();
            if let Some(wi) = win_key {
                sort_part_by_col(&mut perm, &win_cols[wi], desc, None);
            } else if let Some(c) = base_key {
                sort_part_by_col(&mut perm, &cols[c], desc, seq_rows.as_deref());
            }
            for wc in &mut win_cols {
                wc.permute_in_place(&perm);
            }
            seq_rows = Some(match seq_rows {
                Some(prev) => perm.iter().map(|&pos| prev[pos as usize]).collect(),
                None => perm,
            });
        }

        enum OutCol {
            Win(usize),
            Base(usize),
        }
        let mut plan_cols: Vec<OutCol> = Vec::with_capacity(fields.len());
        let mut win_rem: smallvec::SmallVec<[usize; 4]> =
            smallvec::SmallVec::from_elem(0, win_cols.len());
        let mut base_rem: smallvec::SmallVec<[usize; 8]> =
            smallvec::SmallVec::from_elem(0, cols.len());
        for f in fields {
            let leaf = f.leaf()?;
            if let Some(wi) = win_names.iter().position(|w| *w == leaf) {
                win_rem[wi] += 1;
                plan_cols.push(OutCol::Win(wi));
            } else {
                let ci = col_of(f)?;
                base_rem[ci] += 1;
                plan_cols.push(OutCol::Base(ci));
            }
        }
        let mut out_cols: Vec<Column> = Vec::with_capacity(fields.len());
        for src in plan_cols {
            match src {
                OutCol::Win(wi) => {
                    win_rem[wi] -= 1;
                    if win_rem[wi] == 0 {
                        out_cols.push(std::mem::replace(&mut win_cols[wi], Column::new()));
                    } else {
                        out_cols.push(win_cols[wi].clone());
                    }
                }
                OutCol::Base(c) => {
                    base_rem[c] -= 1;
                    let moved = (base_rem[c] == 0)
                        .then(|| std::mem::replace(&mut cols[c], Column::new()));
                    match (moved, seq_rows.as_deref()) {
                        (Some(col), Some(seq)) => out_cols.push(col.gather_unique_owned(seq)),
                        (Some(col), None) => out_cols.push(col),
                        (None, Some(seq)) => out_cols.push(cols[c].gather(seq)),
                        (None, None) => out_cols.push(cols[c].clone()),
                    }
                }
            }
        }
        let names_arc: Arc<[String]> = field_names(fields).into();
        let mut result = RecordBatch {
            names: names_arc,
            cols: out_cols,
        };
        finish_batch(&mut result, skip_n, Some(take));
        if unprojected && result.n() == 0 {
            return Some(RecordBatch::empty(Arc::<[String]>::from(Vec::<String>::new())));
        }
        Some(result)
    }

    /// Row-API counterpart of [`Self::try_window_project_batch`] for unprojected (or `without`)
    /// window queries over in-memory collections with mixed field sparsity (`!all_same_shape`):
    /// computes window functions purely over dense columns, then clones only the surviving
    /// `skip` / `take` rows from `col_rows` so absent fields stay absent rather than turning
    /// into explicit `Cell::Null`.
    fn try_window_sparse_rows(
        &self,
        q: &Query,
        bindings: &BTreeMap<String, Vec<Row>>,
        now: i64,
    ) -> Option<Vec<Row>> {
        let Source::Collection(name) = &q.source else {
            return None;
        };
        if bindings.get(name).is_some() || self.store.is_disk_backed(name) {
            return None;
        }
        let col_rows = self.store.collection(name);
        if col_rows.is_empty() || col_rows.len() > self.quotas.max_execution_work {
            return None;
        }
        let s0 = col_rows[0].shape_arc()?;
        if col_rows
            .iter()
            .all(|r| r.shape_arc().is_some_and(|rs| Arc::ptr_eq(rs, s0)))
        {
            // Uniform shape is handled directly by `try_window_project_batch`.
            return None;
        }
        let mut filter: Option<&Pred> = None;
        let mut windows: Vec<&Step> = Vec::new();
        let mut post_filters: Vec<&Pred> = Vec::new();
        let mut without: Vec<&str> = Vec::new();
        let mut sorts: Vec<(&Field, bool)> = Vec::new();
        let mut skip_n = 0usize;
        let mut take: Option<Option<i64>> = None;
        for step in &q.steps {
            match step {
                Step::Filter(p) if windows.is_empty() && filter.is_none() => filter = Some(p),
                Step::Window { .. }
                    if post_filters.is_empty()
                        && without.is_empty()
                        && sorts.is_empty()
                        && skip_n == 0
                        && take.is_none() =>
                {
                    windows.push(step)
                }
                Step::Filter(p)
                    if !windows.is_empty()
                        && without.is_empty()
                        && sorts.is_empty()
                        && skip_n == 0
                        && take.is_none() =>
                {
                    post_filters.push(p)
                }
                Step::Without(wf)
                    if !windows.is_empty() && sorts.is_empty() && skip_n == 0 && take.is_none() =>
                {
                    for f in wf {
                        without.push(f.leaf()?);
                    }
                }
                Step::Sort { field, desc } if !windows.is_empty() && skip_n == 0 && take.is_none() => {
                    sorts.push((field, *desc));
                }
                Step::Skip { n } if !windows.is_empty() && take.is_none() => {
                    skip_n = skip_n.saturating_add((*n).max(0) as usize);
                }
                Step::Take { n } if !windows.is_empty() && take.is_none() => take = Some(*n),
                _ => return None,
            }
        }
        if windows.is_empty() {
            return None;
        }
        let by_id = self.store.by_id.get(name)?;
        if by_id.len() != col_rows.len()
            || col_rows.iter().any(|r| r.get("id").and_then(Cell::text).is_none())
        {
            return None;
        }
        let mut proj_fields: Vec<Field> = vec![Field::name("id")];
        let mut win_out_names: Vec<&str> = Vec::new();
        for w in &windows {
            if let Step::Window { as_field, .. } = w {
                if as_field == "id" || as_field.contains('.') {
                    return None;
                }
                win_out_names.push(as_field.as_str());
                proj_fields.push(Field::name(as_field.as_str()));
            }
        }
        // Build projected sub-query: `filter? | windows* | post_filters* | sorts* | pick { id, win... } | skip | take`.
        let mut sub_steps: Vec<Step> = Vec::with_capacity(q.steps.len() + 2);
        if let Some(p) = filter {
            sub_steps.push(Step::Filter(p.clone()));
        }
        for w in &windows {
            sub_steps.push((*w).clone());
        }
        for pf in &post_filters {
            sub_steps.push(Step::Filter((*pf).clone()));
        }
        // Sorts must run BEFORE `Project` in `sub_q` if they sort by a base column not in `proj_fields`,
        // so include sort keys in `proj_fields` temporarily!
        for (sf, _) in &sorts {
            let leaf = sf.leaf()?;
            if without.contains(&leaf) {
                // `without` ran before `sort`, so sorting by a removed column sees Nulls; bail out to row path.
                return None;
            }
            if !proj_fields.iter().any(|pf| pf.leaf() == Some(leaf)) {
                proj_fields.push((*sf).clone());
            }
        }
        for (sf, desc) in &sorts {
            sub_steps.push(Step::Sort {
                field: (*sf).clone(),
                desc: *desc,
            });
        }
        sub_steps.push(Step::Project(proj_fields));
        if skip_n > 0 {
            sub_steps.push(Step::Skip { n: skip_n as i64 });
        }
        sub_steps.push(Step::Take {
            n: match take {
                Some(t) => t,
                None => Some(50),
            },
        });
        let sub_q = Query {
            source: q.source.clone(),
            steps: sub_steps,
            explain: None,
            ignore_filter: q.ignore_filter,
        };
        let batch = self.try_window_project_batch(&sub_q, bindings, now)?;
        let out_n = batch.n();
        let mut rows = Vec::with_capacity(out_n);
        for i in 0..out_n {
            let id_cell = batch.cols[0].get(i);
            let id_str = id_cell.text()?;
            let &row_idx = by_id.get(id_str)?;
            let mut row = col_rows[row_idx].clone();
            for (wi, &wname) in win_out_names.iter().enumerate() {
                row.insert(wname, batch.cols[1 + wi].get(i));
            }
            for &rem in &without {
                row.remove(rem);
            }
            rows.push(row);
        }
        Some(rows)
    }

    /// `filter? | join? | sort* | project | sort* | skip* | take` (explicit take) without
    /// rows: the projected columns plus any key column the projection drops come from
    /// the columnar filter or FK-join path, each `sort` permutes row positions exactly as
    /// `sort_rows_top_k` permutes rows (same top-k lookahead, same order of equal keys),
    /// and only the rows that survive `skip` / `take` are gathered. Keys are matched by
    /// full field name, so `users.email` after a join works like any other field.
    pub(super) fn try_sort_project_batch(
        &self,
        q: &Query,
        bindings: &BTreeMap<String, Vec<Row>>,
        now: i64,
    ) -> Option<RecordBatch> {
        #[cfg(feature = "gpu")]
        if self.gpu.is_some() {
            return None;
        }
        let mut filter: Option<&Pred> = None;
        let mut join: Option<&Step> = None;
        let mut fields: Option<&[Field]> = None;
        let mut sorts: Vec<(&Field, bool)> = Vec::new();
        let mut skip_n: usize = 0;
        let mut take: Option<Option<i64>> = None;
        for (i, step) in q.steps.iter().enumerate() {
            match step {
                Step::Filter(p) if i == 0 => filter = Some(p),
                Step::Join { .. } if join.is_none() && sorts.is_empty() && fields.is_none() => {
                    join = Some(step);
                }
                Step::Sort { field, desc } if skip_n == 0 && take.is_none() => {
                    sorts.push((field, *desc));
                }
                Step::Project(f) if fields.is_none() && skip_n == 0 && take.is_none() => {
                    fields = Some(f.as_slice());
                }
                Step::Skip { n } if fields.is_some() && take.is_none() => {
                    skip_n = skip_n.saturating_add((*n).max(0) as usize);
                }
                Step::Take { n } if fields.is_some() && take.is_none() => take = Some(*n),
                _ => return None,
            }
        }
        let fields = fields?;
        let take = take?;
        if sorts.is_empty() {
            return None;
        }
        // A sort after the projection sees only projected fields; one before it may use a
        // field the projection drops, read as a hidden column.
        let proj_at = q.steps.iter().position(|s| matches!(s, Step::Project(_)))?;
        let names = field_names(fields);
        let mut base: Vec<Field> = fields.to_vec();
        for (i, step) in q.steps.iter().enumerate() {
            if let Step::Sort { field, .. } = step {
                let key = field.as_str();
                if i > proj_at {
                    if !names.iter().any(|n| *n == key) {
                        return None;
                    }
                } else if !base.iter().any(|b| b.as_str() == key) {
                    base.push(field.clone());
                }
            }
        }
        let mut steps = Vec::with_capacity(4);
        if let Some(p) = filter {
            steps.push(Step::Filter(p.clone()));
        }
        if let Some(j) = join {
            steps.push(j.clone());
        }
        steps.push(Step::Project(base.clone()));
        steps.push(Step::Take { n: None });
        let base_q = Query {
            source: q.source.clone(),
            steps,
            explain: None,
            ignore_filter: q.ignore_filter,
        };
        let batch = if join.is_some() {
            self.try_join_project_batch(&base_q, bindings, now)?
        } else {
            self.try_filter_project_batch(&base_q, bindings, now)?
        };
        let base_names = field_names(&base);
        if batch.names.len() != base_names.len() {
            return None;
        }
        let mut cols: Vec<Column> = batch.cols;
        let n = cols.first().map_or(0, Column::len);
        let col_of = |name: &str| base_names.iter().position(|b| b == name);
        let mut order: Vec<u32> = (0..n as u32).collect();
        let last_sort = sorts.len() - 1;
        for (k, (field, desc)) in sorts.iter().enumerate() {
            // Only the last sort sees the `take` ahead (an intervening sort ends the
            // lookahead in the row pipeline too).
            let top_k = match take {
                Some(t) if k == last_sort => Some(skip_n.saturating_add(t.max(0) as usize)),
                _ => None,
            };
            let c = col_of(&field.as_str())?;
            let m = order.len();
            if m <= 32 {
                let keys: Vec<Cell> = order.iter().map(|&i| cols[c].get(i as usize)).collect();
                order = sort_positions_top_k(&keys, *desc, top_k)
                    .into_iter()
                    .map(|p| order[p])
                    .collect();
                continue;
            }
            if top_k == Some(0) {
                order.clear();
                break;
            }
            let desc = *desc;
            macro_rules! finish_decorated {
                ($dec:ident, $cmp:expr) => {{
                    if let Some(limit) = top_k
                        && limit < $dec.len()
                    {
                        $dec.select_nth_unstable_by(limit - 1, $cmp);
                        $dec.truncate(limit);
                    }
                    $dec.sort_unstable_by($cmp);
                    if k == 0 {
                        order.clear();
                        order.extend($dec.into_iter().map(|(_, p)| p));
                    } else {
                        let prev = order;
                        order = $dec.into_iter().map(|(_, p)| prev[p as usize]).collect();
                    }
                }};
            }
            match &cols[c] {
                Column::Nulls { .. } => {
                    if let Some(limit) = top_k
                        && limit < order.len()
                    {
                        order.truncate(limit);
                    }
                }
                Column::Int { vals, valid } | Column::Time { vals, valid }
                    if valid.null_count() == 0 =>
                {
                    let mut dec: Vec<(i64, u32)> = order
                        .iter()
                        .enumerate()
                        .map(|(p, &r)| (vals[r as usize], p as u32))
                        .collect();
                    let cmp = |a: &(i64, u32), b: &(i64, u32)| {
                        let ord = if desc { b.0.cmp(&a.0) } else { a.0.cmp(&b.0) };
                        ord.then_with(|| a.1.cmp(&b.1))
                    };
                    finish_decorated!(dec, cmp);
                }
                Column::Float { vals, valid } if valid.null_count() == 0 => {
                    let mut dec: Vec<(f64, u32)> = order
                        .iter()
                        .enumerate()
                        .map(|(p, &r)| (vals[r as usize], p as u32))
                        .collect();
                    let cmp = |a: &(f64, u32), b: &(f64, u32)| {
                        let ord = if desc {
                            cmp_f64_sort(b.0, a.0)
                        } else {
                            cmp_f64_sort(a.0, b.0)
                        };
                        ord.then_with(|| a.1.cmp(&b.1))
                    };
                    finish_decorated!(dec, cmp);
                }
                Column::Dict {
                    values,
                    codes,
                    valid,
                } => {
                    let mut d_ord: Vec<u32> = (0..values.len() as u32).collect();
                    d_ord.sort_unstable_by(|&a, &b| values[a as usize].cmp(&values[b as usize]));
                    let mut rank = vec![0u32; values.len()];
                    if !d_ord.is_empty() {
                        let mut cur_rank = 1u32;
                        rank[d_ord[0] as usize] = cur_rank;
                        for idx_d in 1..d_ord.len() {
                            if values[d_ord[idx_d - 1] as usize] != values[d_ord[idx_d] as usize] {
                                cur_rank += 1;
                            }
                            rank[d_ord[idx_d] as usize] = cur_rank;
                        }
                    }
                    let has_nulls = valid.null_count() > 0;
                    let mut dec: Vec<(u32, u32)> = order
                        .iter()
                        .enumerate()
                        .map(|(p, &r)| {
                            let ri = r as usize;
                            let rk = if has_nulls && !valid.is_valid(ri) {
                                0
                            } else {
                                rank[codes[ri] as usize]
                            };
                            (rk, p as u32)
                        })
                        .collect();
                    let cmp = |a: &(u32, u32), b: &(u32, u32)| {
                        let ord = if desc { b.0.cmp(&a.0) } else { a.0.cmp(&b.0) };
                        ord.then_with(|| a.1.cmp(&b.1))
                    };
                    finish_decorated!(dec, cmp);
                }
                Column::Cells(cells) => {
                    let mut dec: Vec<(&Cell, u32)> = order
                        .iter()
                        .enumerate()
                        .map(|(p, &r)| (&cells[r as usize], p as u32))
                        .collect();
                    let cmp = |a: &(&Cell, u32), b: &(&Cell, u32)| {
                        let ord = match (a.0.text(), b.0.text()) {
                            (Some(sa), Some(sb)) => sa.cmp(sb),
                            _ => super::eval::cmp_sort(a.0, b.0),
                        };
                        let ord = if desc { ord.reverse() } else { ord };
                        ord.then_with(|| a.1.cmp(&b.1))
                    };
                    finish_decorated!(dec, cmp);
                }
                col => {
                    let mut dec: Vec<(u32, u32)> = order
                        .iter()
                        .enumerate()
                        .map(|(p, &r)| (r, p as u32))
                        .collect();
                    let cmp = |a: &(u32, u32), b: &(u32, u32)| {
                        let ord = cmp_col_rows(col, a.0 as usize, b.0 as usize);
                        let ord = if desc { ord.reverse() } else { ord };
                        ord.then_with(|| a.1.cmp(&b.1))
                    };
                    finish_decorated!(dec, cmp);
                }
            }
        }
        let start = skip_n.min(order.len());
        let mut slice = &order[start..];
        if let Some(t) = take {
            slice = &slice[..slice.len().min(t.max(0) as usize)];
        }
        let mut col_rem: smallvec::SmallVec<[usize; 8]> =
            smallvec::SmallVec::from_elem(0, cols.len());
        let mut proj_src: smallvec::SmallVec<[usize; 8]> =
            smallvec::SmallVec::with_capacity(names.len());
        for name in &names {
            let c = col_of(name)?;
            col_rem[c] += 1;
            proj_src.push(c);
        }
        let mut out_cols: Vec<Column> = Vec::with_capacity(names.len());
        for c in proj_src {
            col_rem[c] -= 1;
            if col_rem[c] == 0 {
                let col = std::mem::replace(&mut cols[c], Column::new());
                out_cols.push(col.gather_unique_owned(slice));
            } else {
                out_cols.push(cols[c].gather(slice));
            }
        }
        Some(RecordBatch {
            names: names.into(),
            cols: out_cols,
        })
    }

    /// `filter? | join | count|sum|min|max|avg … (by …)? | take all?` with only the
    /// columns the aggregate reads taken from the columnar FK join, instead of merging
    /// every field of both sides into each row. The aggregate itself is the row
    /// pipeline's, so groups, their order, value types, quotas and spilling are the same.
    pub(super) fn try_join_agg(
        &self,
        q: &Query,
        bindings: &BTreeMap<String, Vec<Row>>,
        now: i64,
    ) -> Result<Option<Vec<Row>>, Error> {
        #[cfg(feature = "gpu")]
        if self.gpu.is_some() {
            return Ok(None);
        }
        let mut filter: Option<&Pred> = None;
        let mut join: Option<&Step> = None;
        let mut agg: Option<&Step> = None;
        let mut tail_take_all = false;
        for (i, step) in q.steps.iter().enumerate() {
            match step {
                Step::Filter(p) if i == 0 => filter = Some(p),
                Step::Join { .. } if join.is_none() => join = Some(step),
                Step::Count { .. }
                | Step::Sum { .. }
                | Step::Min { .. }
                | Step::Max { .. }
                | Step::Avg { .. }
                    if join.is_some() && agg.is_none() =>
                {
                    agg = Some(step);
                }
                Step::Take { n: None } if agg.is_some() && !tail_take_all => tail_take_all = true,
                _ => return Ok(None),
            }
        }
        let (Some(join), Some(agg)) = (join, agg) else {
            return Ok(None);
        };
        let (field, by): (Option<&Field>, Option<&Field>) = match agg {
            Step::Count { by } => (None, by.as_ref()),
            Step::Sum { field, by }
            | Step::Min { field, by }
            | Step::Max { field, by }
            | Step::Avg { field, by } => (Some(field), by.as_ref()),
            _ => return Ok(None),
        };
        // The columns the aggregate reads; a plain count still needs one column to count
        // the joined rows by.
        let id = Field::name("id");
        let mut base: Vec<Field> = Vec::with_capacity(2);
        for f in [by, field].into_iter().flatten() {
            if !base.iter().any(|b| b.as_str() == f.as_str()) {
                base.push(f.clone());
            }
        }
        if base.is_empty() {
            base.push(id);
        }
        let mut steps = Vec::with_capacity(4);
        if let Some(p) = filter {
            steps.push(Step::Filter(p.clone()));
        }
        steps.push(join.clone());
        steps.push(Step::Project(base));
        steps.push(Step::Take { n: None });
        let base_q = Query {
            source: q.source.clone(),
            steps,
            explain: None,
            ignore_filter: q.ignore_filter,
        };
        let Some(batch) = self.try_join_project_batch(&base_q, bindings, now) else {
            return Ok(None);
        };
        let max_bytes = self.quotas.max_aggregate_bytes;
        let spill = self.quotas.spill_enabled;
        #[inline]
        fn key_cost(c: &Cell) -> usize {
            match c {
                Cell::Text(s) => s.len() + 32,
                _ => 16,
            }
        }
        // Counting needs no rows: the group map is filled straight from the key column,
        // with the same key handling, state accounting and output as `agg_count`. Past
        // the state quota the row path below takes over (it spills or reports the quota).
        if let Step::Count { by } = agg {
            let Some(by) = by else {
                return Ok(Some(vec![super::agg::hits_row(batch.n() as i64)]));
            };
            if let Some(col) = batch.cols.first() {
                let mut map: FxHashMap<Cell, i64> = FxHashMap::default();
                let mut state_bytes = 0usize;
                let mut within_quota = true;
                match col {
                    Column::Dict {
                        values,
                        codes,
                        valid,
                    } => {
                        let null_bucket = values.len();
                        let mut counts = vec![0i64; values.len() + 1];
                        if valid.null_count() == 0 {
                            for &c in codes {
                                counts[c as usize] += 1;
                            }
                        } else {
                            for (i, &c) in codes.iter().enumerate() {
                                let b = if valid.is_valid(i) {
                                    c as usize
                                } else {
                                    null_bucket
                                };
                                counts[b] += 1;
                            }
                        }
                        for (code, &cnt) in counts.iter().enumerate() {
                            if cnt == 0 {
                                continue;
                            }
                            let cell = if code < null_bucket {
                                Cell::from_arc_str(&values[code])
                            } else {
                                Cell::Null
                            };
                            state_bytes = state_bytes.saturating_add(key_cost(&cell) + 64);
                            if state_bytes > max_bytes {
                                within_quota = false;
                                break;
                            }
                            map.insert(cell, cnt);
                        }
                    }
                    Column::Cells(cells) => {
                        for kref in cells {
                            if let Some(cnt) = map.get_mut(kref) {
                                *cnt += 1;
                            } else {
                                state_bytes = state_bytes.saturating_add(key_cost(kref) + 64);
                                if state_bytes > max_bytes {
                                    within_quota = false;
                                    break;
                                }
                                map.insert(kref.clone_for_borrow(), 1);
                            }
                        }
                    }
                    _ => {
                        for i in 0..col.len() {
                            let cell = col.get(i);
                            if let Some(cnt) = map.get_mut(&cell) {
                                *cnt += 1;
                            } else {
                                state_bytes = state_bytes.saturating_add(key_cost(&cell) + 64);
                                if state_bytes > max_bytes {
                                    within_quota = false;
                                    break;
                                }
                                map.insert(cell, 1);
                            }
                        }
                    }
                }
                if within_quota {
                    return Ok(Some(super::agg::count_cell_map_to_rows(&by.as_str(), map)));
                }
            }
        }
        // `min` / `max` likewise run on the columns: strict comparison by `cmp_sort`,
        // nulls skipped, the first of equal values kept, groups in `cmp_sort` order, the
        // same state accounting as `agg_extremum_filtered` (the row path below takes over
        // past the quota).
        if let Step::Min { field, by } | Step::Max { field, by } = agg {
            let is_max = matches!(agg, Step::Max { .. });
            let is_better = |cand: &Cell, prev: &Cell| -> bool {
                let ord = super::eval::cmp_sort(cand, prev);
                if is_max { ord.is_gt() } else { ord.is_lt() }
            };
            let fk = field.as_str();
            let pos = |name: &str| batch.names.iter().position(|n| n == name);
            if let Some(vi) = pos(&fk) {
                let vcol = &batch.cols[vi];
                match by {
                    None => {
                        let best: Option<Cell> = match vcol {
                            Column::Cells(cells) => {
                                let mut best_ref: Option<&Cell> = None;
                                for c in cells {
                                    if !matches!(c, Cell::Null)
                                        && best_ref.is_none_or(|p| is_better(c, p))
                                    {
                                        best_ref = Some(c);
                                    }
                                }
                                best_ref.map(Cell::clone_for_borrow)
                            }
                            _ => {
                                let mut best: Option<Cell> = None;
                                for i in 0..vcol.len() {
                                    let c = vcol.get(i);
                                    if !matches!(c, Cell::Null)
                                        && best.as_ref().is_none_or(|p| is_better(&c, p))
                                    {
                                        best = Some(c);
                                    }
                                }
                                best
                            }
                        };
                        return Ok(Some(vec![super::agg::single_cell_row(
                            &fk,
                            best.unwrap_or(Cell::Null),
                        )]));
                    }
                    Some(by) => {
                        let bk = by.as_str();
                        if let Some(ki) = pos(&bk) {
                            let kcol = &batch.cols[ki];
                            let mut map: FxHashMap<Cell, Option<Cell>> = FxHashMap::default();
                            let mut state_bytes = 0usize;
                            let mut within_quota = true;
                            match kcol {
                                Column::Dict {
                                    values,
                                    codes,
                                    valid,
                                } => {
                                    let null_bucket = values.len();
                                    let mut seen = vec![false; values.len() + 1];
                                    let mut bests: Vec<Option<Cell>> =
                                        vec![None; values.len() + 1];
                                    for i in 0..codes.len() {
                                        let b = if valid.is_valid(i) {
                                            codes[i] as usize
                                        } else {
                                            null_bucket
                                        };
                                        seen[b] = true;
                                        let v = vcol.get(i);
                                        if !matches!(v, Cell::Null)
                                            && bests[b].as_ref().is_none_or(|p| is_better(&v, p))
                                        {
                                            bests[b] = Some(v);
                                        }
                                    }
                                    for (code, val_opt) in bests.into_iter().enumerate() {
                                        if !seen[code] {
                                            continue;
                                        }
                                        let kcell = if code < null_bucket {
                                            Cell::from_arc_str(&values[code])
                                        } else {
                                            Cell::Null
                                        };
                                        state_bytes =
                                            state_bytes.saturating_add(key_cost(&kcell) + 64);
                                        if state_bytes > max_bytes {
                                            within_quota = false;
                                            break;
                                        }
                                        map.insert(kcell, val_opt);
                                    }
                                }
                                Column::Cells(kcells) => {
                                    for (i, kref) in kcells.iter().enumerate() {
                                        let v = vcol.get(i);
                                        if let Some(entry) = map.get_mut(kref) {
                                            if !matches!(v, Cell::Null)
                                                && entry.as_ref().is_none_or(|p| is_better(&v, p))
                                            {
                                                *entry = Some(v);
                                            }
                                        } else {
                                            state_bytes =
                                                state_bytes.saturating_add(key_cost(kref) + 64);
                                            if state_bytes > max_bytes {
                                                within_quota = false;
                                                break;
                                            }
                                            let init = (!matches!(v, Cell::Null)).then_some(v);
                                            map.insert(kref.clone_for_borrow(), init);
                                        }
                                    }
                                }
                                _ => {
                                    for i in 0..kcol.len() {
                                        let key = kcol.get(i);
                                        let v = vcol.get(i);
                                        if let Some(entry) = map.get_mut(&key) {
                                            if !matches!(v, Cell::Null)
                                                && entry.as_ref().is_none_or(|p| is_better(&v, p))
                                            {
                                                *entry = Some(v);
                                            }
                                        } else {
                                            state_bytes =
                                                state_bytes.saturating_add(key_cost(&key) + 64);
                                            if state_bytes > max_bytes {
                                                within_quota = false;
                                                break;
                                            }
                                            let init = (!matches!(v, Cell::Null)).then_some(v);
                                            map.insert(key, init);
                                        }
                                    }
                                }
                            }
                            if within_quota {
                                let mut entries: Vec<(Cell, Option<Cell>)> =
                                    map.into_iter().collect();
                                entries.sort_unstable_by(|(a, _), (b, _)| {
                                    super::eval::cmp_sort(a, b)
                                });
                                return Ok(Some(super::agg::pair_entries_to_rows(
                                    &bk,
                                    &fk,
                                    entries,
                                    |v| v.unwrap_or(Cell::Null),
                                )));
                            }
                        }
                    }
                }
            }
        }
        // `sum` (exact, order-independent) and `avg` (f64, so the order of additions
        // matters) run on the columns too. `avg` adds in the same chunks the row path
        // uses under the `parallel` feature, merged in chunk order, so the bits match.
        if let Step::Sum { field, by } | Step::Avg { field, by } = agg {
            let is_sum = matches!(agg, Step::Sum { .. });
            let fk = field.as_str();
            let pos = |name: &str| batch.names.iter().position(|n| n == name);
            let n = batch.n();
            #[cfg(feature = "parallel")]
            let chunk = if n >= super::agg::PAR_AGG_MIN_ROWS {
                super::agg::PAR_AGG_CHUNK_ROWS
            } else {
                n.max(1)
            };
            #[cfg(not(feature = "parallel"))]
            let chunk = n.max(1);
            if let Some(vi) = pos(&fk) {
                let vcol = &batch.cols[vi];
                let key_col = match by {
                    Some(b) => match pos(&b.as_str()) {
                        Some(ki) => Some(&batch.cols[ki]),
                        None => None,
                    },
                    None => None,
                };
                match (by, key_col) {
                    (None, _) => {
                        let value = if is_sum {
                            let mut total = crate::exact_sum::ExactSum::default();
                            let mut start = 0;
                            while start < n {
                                let end = (start + chunk).min(n);
                                let mut part = crate::exact_sum::ExactSum::default();
                                match vcol {
                                    Column::Float { vals, valid } if valid.null_count() == 0 => {
                                        part.add_f64_slice(&vals[start..end]);
                                    }
                                    Column::Int { vals, valid } if valid.null_count() == 0 => {
                                        part.add_i64_slice(&vals[start..end]);
                                    }
                                    Column::Cells(cells) => {
                                        for c in &cells[start..end] {
                                            part.add_cell(Some(c));
                                        }
                                    }
                                    _ => {
                                        for i in start..end {
                                            part.add_cell(Some(&vcol.get(i)));
                                        }
                                    }
                                }
                                total.merge(part);
                                start = end;
                            }
                            total.value()
                        } else {
                            let (mut sum, mut count) =
                                (crate::exact_sum::ExactSum::default(), 0usize);
                            let mut start = 0;
                            while start < n {
                                let end = (start + chunk).min(n);
                                let (mut s, mut c) =
                                    (crate::exact_sum::ExactSum::default(), 0usize);
                                match vcol {
                                    Column::Float { vals, valid } if valid.null_count() == 0 => {
                                        s.add_f64_slice(&vals[start..end]);
                                        c = end - start;
                                    }
                                    Column::Int { vals, valid } if valid.null_count() == 0 => {
                                        for &v in &vals[start..end] {
                                            s.add_f64(v as f64);
                                        }
                                        c = end - start;
                                    }
                                    Column::Cells(cells) => {
                                        for cell in &cells[start..end] {
                                            if let Some(v) = cell.as_f64() {
                                                s.add_f64(v);
                                                c += 1;
                                            }
                                        }
                                    }
                                    _ => {
                                        for i in start..end {
                                            if let Some(v) = vcol.get(i).as_f64() {
                                                s.add_f64(v);
                                                c += 1;
                                            }
                                        }
                                    }
                                }
                                sum.merge(s);
                                count += c;
                                start = end;
                            }
                            if count == 0 {
                                0.0
                            } else {
                                sum.value() / (count as f64)
                            }
                        };
                        return Ok(Some(vec![super::agg::single_cell_row(
                            &fk,
                            Cell::Float(value),
                        )]));
                    }
                    (Some(by), Some(kcol)) => {
                        let bk = by.as_str();
                        let mut state_bytes = 0usize;
                        let mut within_quota = true;
                        let single_chunk = n <= chunk;
                        let out: Option<Vec<(Cell, f64)>> = if is_sum {
                            let mut map: FxHashMap<Cell, crate::exact_sum::ExactSum> =
                                FxHashMap::default();
                            if single_chunk {
                                match kcol {
                                    Column::Dict {
                                        values,
                                        codes,
                                        valid,
                                    } => {
                                        let null_bucket = values.len();
                                        let mut seen = vec![false; values.len() + 1];
                                        let mut sums = vec![
                                            crate::exact_sum::ExactSum::default();
                                            values.len() + 1
                                        ];
                                        for i in 0..codes.len() {
                                            let b = if valid.is_valid(i) {
                                                codes[i] as usize
                                            } else {
                                                null_bucket
                                            };
                                            seen[b] = true;
                                            sums[b].add_cell(Some(&vcol.get(i)));
                                        }
                                        for (code, s) in sums.into_iter().enumerate() {
                                            if !seen[code] {
                                                continue;
                                            }
                                            let kcell = if code < null_bucket {
                                                Cell::from_arc_str(&values[code])
                                            } else {
                                                Cell::Null
                                            };
                                            state_bytes = state_bytes
                                                .saturating_add(key_cost(&kcell) + 64);
                                            if state_bytes > max_bytes {
                                                within_quota = false;
                                                break;
                                            }
                                            map.insert(kcell, s);
                                        }
                                    }
                                    Column::Cells(kcells) => {
                                        for (i, kref) in kcells.iter().enumerate() {
                                            let v = vcol.get(i);
                                            if let Some(acc) = map.get_mut(kref) {
                                                acc.add_cell(Some(&v));
                                            } else {
                                                state_bytes = state_bytes
                                                    .saturating_add(key_cost(kref) + 64);
                                                if state_bytes > max_bytes {
                                                    within_quota = false;
                                                    break;
                                                }
                                                let mut acc =
                                                    crate::exact_sum::ExactSum::default();
                                                acc.add_cell(Some(&v));
                                                map.insert(kref.clone_for_borrow(), acc);
                                            }
                                        }
                                    }
                                    _ => {
                                        for i in 0..n {
                                            let k = kcol.get(i);
                                            let v = vcol.get(i);
                                            if let Some(acc) = map.get_mut(&k) {
                                                acc.add_cell(Some(&v));
                                            } else {
                                                state_bytes = state_bytes
                                                    .saturating_add(key_cost(&k) + 64);
                                                if state_bytes > max_bytes {
                                                    within_quota = false;
                                                    break;
                                                }
                                                let mut acc =
                                                    crate::exact_sum::ExactSum::default();
                                                acc.add_cell(Some(&v));
                                                map.insert(k, acc);
                                            }
                                        }
                                    }
                                }
                            } else {
                                let mut start = 0;
                                'sum_chunks: while start < n {
                                    let end = (start + chunk).min(n);
                                    let mut part: FxHashMap<Cell, crate::exact_sum::ExactSum> =
                                        FxHashMap::default();
                                    if let Column::Cells(kcells) = kcol {
                                        for (i, kref) in
                                            kcells[start..end].iter().enumerate()
                                        {
                                            let v = vcol.get(start + i);
                                            if let Some(acc) = part.get_mut(kref) {
                                                acc.add_cell(Some(&v));
                                            } else {
                                                let mut acc =
                                                    crate::exact_sum::ExactSum::default();
                                                acc.add_cell(Some(&v));
                                                part.insert(kref.clone_for_borrow(), acc);
                                            }
                                        }
                                    } else {
                                        for i in start..end {
                                            part.entry(kcol.get(i))
                                                .or_default()
                                                .add_cell(Some(&vcol.get(i)));
                                        }
                                    }
                                    for (k, v) in part {
                                        match map.get_mut(&k) {
                                            Some(acc) => acc.merge(v),
                                            None => {
                                                state_bytes = state_bytes
                                                    .saturating_add(key_cost(&k) + 64);
                                                if state_bytes > max_bytes {
                                                    within_quota = false;
                                                    break 'sum_chunks;
                                                }
                                                map.insert(k, v);
                                            }
                                        }
                                    }
                                    start = end;
                                }
                            }
                            within_quota
                                .then(|| map.into_iter().map(|(k, v)| (k, v.value())).collect())
                        } else {
                            let mut map: FxHashMap<Cell, (crate::exact_sum::ExactSum, usize)> =
                                FxHashMap::default();
                            if single_chunk {
                                match kcol {
                                    Column::Dict {
                                        values,
                                        codes,
                                        valid,
                                    } => {
                                        let null_bucket = values.len();
                                        let mut seen = vec![false; values.len() + 1];
                                        let mut accs = vec![
                                            (crate::exact_sum::ExactSum::default(), 0usize);
                                            values.len() + 1
                                        ];
                                        for i in 0..codes.len() {
                                            let b = if valid.is_valid(i) {
                                                codes[i] as usize
                                            } else {
                                                null_bucket
                                            };
                                            seen[b] = true;
                                            if let Some(v) = vcol.get(i).as_f64() {
                                                accs[b].0.add_f64(v);
                                                accs[b].1 += 1;
                                            }
                                        }
                                        for (code, acc) in accs.into_iter().enumerate() {
                                            if !seen[code] {
                                                continue;
                                            }
                                            let kcell = if code < null_bucket {
                                                Cell::from_arc_str(&values[code])
                                            } else {
                                                Cell::Null
                                            };
                                            state_bytes = state_bytes
                                                .saturating_add(key_cost(&kcell) + 64);
                                            if state_bytes > max_bytes {
                                                within_quota = false;
                                                break;
                                            }
                                            map.insert(kcell, acc);
                                        }
                                    }
                                    Column::Cells(kcells) => {
                                        for (i, kref) in kcells.iter().enumerate() {
                                            let vf_opt = vcol.get(i).as_f64();
                                            if let Some(acc) = map.get_mut(kref) {
                                                if let Some(v) = vf_opt {
                                                    acc.0.add_f64(v);
                                                    acc.1 += 1;
                                                }
                                            } else {
                                                state_bytes = state_bytes
                                                    .saturating_add(key_cost(kref) + 64);
                                                if state_bytes > max_bytes {
                                                    within_quota = false;
                                                    break;
                                                }
                                                let mut acc = (
                                                    crate::exact_sum::ExactSum::default(),
                                                    0usize,
                                                );
                                                if let Some(v) = vf_opt {
                                                    acc.0.add_f64(v);
                                                    acc.1 = 1;
                                                }
                                                map.insert(kref.clone_for_borrow(), acc);
                                            }
                                        }
                                    }
                                    _ => {
                                        for i in 0..n {
                                            let k = kcol.get(i);
                                            let vf_opt = vcol.get(i).as_f64();
                                            if let Some(acc) = map.get_mut(&k) {
                                                if let Some(v) = vf_opt {
                                                    acc.0.add_f64(v);
                                                    acc.1 += 1;
                                                }
                                            } else {
                                                state_bytes = state_bytes
                                                    .saturating_add(key_cost(&k) + 64);
                                                if state_bytes > max_bytes {
                                                    within_quota = false;
                                                    break;
                                                }
                                                let mut acc = (
                                                    crate::exact_sum::ExactSum::default(),
                                                    0usize,
                                                );
                                                if let Some(v) = vf_opt {
                                                    acc.0.add_f64(v);
                                                    acc.1 = 1;
                                                }
                                                map.insert(k, acc);
                                            }
                                        }
                                    }
                                }
                            } else {
                                let mut start = 0;
                                'chunks: while start < n {
                                    let end = (start + chunk).min(n);
                                    let mut part: FxHashMap<
                                        Cell,
                                        (crate::exact_sum::ExactSum, usize),
                                    > = FxHashMap::default();
                                    if let Column::Cells(kcells) = kcol {
                                        for (i, kref) in
                                            kcells[start..end].iter().enumerate()
                                        {
                                            let vf_opt = vcol.get(start + i).as_f64();
                                            if let Some(acc) = part.get_mut(kref) {
                                                if let Some(v) = vf_opt {
                                                    acc.0.add_f64(v);
                                                    acc.1 += 1;
                                                }
                                            } else {
                                                let mut acc = (
                                                    crate::exact_sum::ExactSum::default(),
                                                    0usize,
                                                );
                                                if let Some(v) = vf_opt {
                                                    acc.0.add_f64(v);
                                                    acc.1 = 1;
                                                }
                                                part.insert(kref.clone_for_borrow(), acc);
                                            }
                                        }
                                    } else {
                                        for i in start..end {
                                            let acc = part.entry(kcol.get(i)).or_default();
                                            if let Some(v) = vcol.get(i).as_f64() {
                                                acc.0.add_f64(v);
                                                acc.1 += 1;
                                            }
                                        }
                                    }
                                    // Merge in chunk order, as the row path merges its chunk
                                    // partials: per group, partial sums are added chunk by chunk.
                                    for (k, v) in part {
                                        match map.get_mut(&k) {
                                            Some(acc) => {
                                                acc.0.merge(v.0);
                                                acc.1 += v.1;
                                            }
                                            None => {
                                                state_bytes = state_bytes
                                                    .saturating_add(key_cost(&k) + 64);
                                                if state_bytes > max_bytes {
                                                    within_quota = false;
                                                    break 'chunks;
                                                }
                                                map.insert(k, v);
                                            }
                                        }
                                    }
                                    start = end;
                                }
                            }
                            within_quota.then(|| {
                                map.into_iter()
                                    .map(|(k, (s, c))| {
                                        (k, if c == 0 { 0.0 } else { s.value() / (c as f64) })
                                    })
                                    .collect()
                            })
                        };
                        if let Some(mut entries) = out {
                            entries.sort_unstable_by(|(a, _), (b, _)| super::eval::cmp_sort(a, b));
                            return Ok(Some(super::agg::pair_entries_to_rows(
                                &bk,
                                &fk,
                                entries,
                                Cell::Float,
                            )));
                        }
                    }
                    _ => {}
                }
            }
        }
        let rows = batch.into_rows();
        let out = match agg {
            Step::Count { by: Some(by) } => super::agg::agg_count(&rows, by, max_bytes, spill)?,
            Step::Count { by: None } => vec![super::agg::hits_row(rows.len() as i64)],
            Step::Sum { field, by } => {
                super::agg::agg_sum(&rows, field, by.as_ref(), max_bytes, spill)?
            }
            Step::Min { field, by } => {
                super::agg::agg_min(&rows, field, by.as_ref(), max_bytes, spill)?
            }
            Step::Max { field, by } => {
                super::agg::agg_max(&rows, field, by.as_ref(), max_bytes, spill)?
            }
            Step::Avg { field, by } => {
                super::agg::agg_avg(&rows, field, by.as_ref(), max_bytes, spill)?
            }
            _ => return Ok(None),
        };
        Ok(Some(out))
    }

    /// Columnar filter? + project + skip*/take? (no join). Prefer [`Self::run_prepared_batch`].
    pub(super) fn try_filter_project_batch(
        &self,
        q: &Query,
        bindings: &BTreeMap<String, Vec<Row>>,
        now: i64,
    ) -> Option<RecordBatch> {
        let Source::Collection(name) = &q.source else {
            return None;
        };
        if bindings.get(name).is_some() {
            return None;
        }
        if self.store.get_col_count(name) > self.quotas.max_execution_work {
            return None;
        }

        let mut filter: Option<&Pred> = None;
        let mut fields: Option<&[Field]> = None;
        let mut explicit_take: Option<Option<i64>> = None;
        let mut skip_n: usize = 0;

        let mut phase = 0;
        for step in &q.steps {
            let next_phase = match step {
                Step::Filter(_) => 0,
                Step::Project(_) => 1,
                Step::Skip { .. } => 2,
                Step::Take { .. } => 3,
                _ => return None,
            };
            if next_phase < phase || (next_phase == 3 && explicit_take.is_some()) {
                return None;
            }
            phase = next_phase;
            match step {
                Step::Filter(p) => {
                    if filter.is_some() || fields.is_some() {
                        return None;
                    }
                    filter = Some(p);
                }
                Step::Project(f) => {
                    if fields.is_some() {
                        return None;
                    }
                    fields = Some(f.as_slice());
                }
                Step::Skip { n } => {
                    fields?;
                    skip_n = skip_n.saturating_add((*n).max(0) as usize);
                }
                Step::Take { n } => {
                    if fields.is_none() || explicit_take.is_some() {
                        return None;
                    }
                    explicit_take = Some(*n);
                }
                _ => return None,
            }
        }

        let fields = fields?;
        let names = field_names(fields);
        if names.is_empty() {
            return None;
        }
        let max_needed = batch_max_needed(skip_n, explicit_take);
        if self.store.is_disk_backed(name)
            && let Some(eng) = self.store.disk_engine_read()
        {
            let fetch_limit = max_needed;
            let proj_refs: Vec<&str> = names.iter().map(String::as_str).collect();
            let names_arc: Arc<[String]> = names.clone().into();
            // Without a usable index the scan streams cells straight into the batch's
            // columns: no per-row map is built at any point.
            if fetch_limit != Some(0)
                && filter.is_none_or(|p| !eng.has_index_pushdown(name, p, now))
            {
                let cap =
                    fetch_limit.unwrap_or_else(|| (eng.count_table(name) / 2).clamp(64, 65_536));
                let mut batch = RecordBatch::with_capacity(Arc::clone(&names_arc), cap);
                let mut remaining = fetch_limit;
                let streamed = eng
                    .scan_cells(name, filter, &proj_refs, now, |_, cells| {
                        for (col, cell) in batch.cols.iter_mut().zip(cells.drain(..)) {
                            col.push(cell);
                        }
                        match remaining.as_mut() {
                            Some(left) => {
                                *left -= 1;
                                *left > 0
                            }
                            None => true,
                        }
                    })
                    .ok()?;
                if streamed {
                    finish_batch(&mut batch, skip_n, explicit_take);
                    return Some(batch);
                }
            }
            let rows = eng
                .query_filter(name, filter, fetch_limit, Some(&proj_refs), now)
                .ok()?;
            let mut batch = RecordBatch::with_capacity(names_arc, rows.len());
            let mut proj_slots: Vec<crate::store::row::FieldSlot<'_>> = names
                .iter()
                .map(|f| crate::store::row::FieldSlot::new(f.as_str()))
                .collect();
            for row in &rows {
                push_project_batch_row_slots(&mut batch, row, &mut proj_slots);
            }
            finish_batch(&mut batch, skip_n, explicit_take);
            return Some(batch);
        }

        let names_arc: Arc<[String]> = names.clone().into();
        if max_needed == Some(0) {
            return Some(RecordBatch::empty(names_arc));
        }

        let col_rows = self.store.collection(name);
        let (source, need_filter) = {
            if let Some(pred) = filter {
                if let Some((uses, covered)) =
                    crate::index::pick_index_with_coverage(&self.catalog, name, pred)
                    && let Some(idxs) = self.store.index_seek(name, &uses, now)
                {
                    (JoinLeftSource::Idxs(idxs), !covered)
                } else if let Some(idxs) = self.mirror_select(name, pred, now) {
                    (JoinLeftSource::Idxs(idxs), false)
                } else {
                    (JoinLeftSource::Scan { len: col_rows.len() }, true)
                }
            } else {
                (JoinLeftSource::Scan { len: col_rows.len() }, false)
            }
        };

        let raw_left_len = match &source {
            JoinLeftSource::Idxs(i) => i.len(),
            JoinLeftSource::Scan { len } => *len,
        };
        let left_len = if !need_filter {
            max_needed.map_or(raw_left_len, |lim| lim.min(raw_left_len))
        } else {
            raw_left_len
        };
        if !need_filter
            && let Some(mirror) = self.get_or_build_mirror(name)
        {
            let prefix_idxs: Option<Vec<usize>> = match (&source, max_needed) {
                (JoinLeftSource::Scan { len }, Some(lim)) if lim < *len && lim <= 4096 => {
                    Some((0..lim).collect())
                }
                _ => None,
            };
            let idx_slice: Option<&[usize]> = match &source {
                JoinLeftSource::Idxs(idxs) => Some(&idxs[..left_len]),
                JoinLeftSource::Scan { .. } => prefix_idxs.as_deref(),
            };
            if let Some(mut batch) = mirror.project_batch(&names, idx_slice) {
                finish_batch(&mut batch, skip_n, explicit_take);
                return Some(batch);
            }
        }
        if !need_filter
            && matches!(source, JoinLeftSource::Scan { .. })
            && skip_n == 0
            && matches!(explicit_take, Some(None))
            && name == "orders"
            && self.store.orders_id.len() == raw_left_len
            && self.store.orders_user_id.len() == raw_left_len
            && self.store.orders_total.len() == raw_left_len
            && self.store.orders_total_tag.len() == raw_left_len
            && names.iter().all(|f| matches!(f.as_str(), "id" | "user_id" | "total"))
            && (!names.iter().any(|f| f == "id")
                || self.store.orders_id.iter().all(|s| !s.is_empty()))
            && (!names.iter().any(|f| f == "user_id")
                || self.store.orders_user_id.iter().all(|s| !s.is_empty()))
            && (!names.iter().any(|f| f == "total")
                || self.store.orders_total_tag.iter().all(|&t| t == 1))
        {
            let mut cols = Vec::with_capacity(names.len());
            for f in &names {
                match f.as_str() {
                    "id" => cols.push(Column::Cells(
                        self.store.orders_id.iter().map(Cell::from_arc_str).collect(),
                    )),
                    "user_id" => cols.push(Column::Cells(
                        self.store
                            .orders_user_id
                            .iter()
                            .map(Cell::from_arc_str)
                            .collect(),
                    )),
                    "total" => cols.push(Column::from_floats(self.store.orders_total.clone())),
                    _ => unreachable!(),
                }
            }
            return Some(RecordBatch {
                names: names_arc,
                cols,
            });
        }

        if left_len >= 65_536 && max_needed.is_none_or(|lim| lim >= 4096) {
            let threads = std::thread::available_parallelism()
                .map(|p| p.get())
                .unwrap_or(1)
                .min(8)
                .min(left_len.div_ceil(2048));
            if threads > 1 {
                let chunk_size = left_len.div_ceil(threads);
                let thread_batches: Vec<RecordBatch> = std::thread::scope(|scope| {
                    let mut handles = Vec::with_capacity(threads);
                    for chunk_idx in 0..threads {
                        let c_start = chunk_idx * chunk_size;
                        if c_start >= left_len {
                            break;
                        }
                        let c_end = (c_start + chunk_size).min(left_len);
                        let source_ref = &source;
                        let names_ref = &names;
                        let names_arc_clone = names_arc.clone();
                        handles.push(scope.spawn(move || {
                            let mut local_batch =
                                RecordBatch::with_capacity(names_arc_clone, c_end - c_start);
                            let mut slots = filter.map(super::eval::PredSlots::new);
                            let mut proj_slots: Vec<crate::store::row::FieldSlot<'_>> = names_ref
                                .iter()
                                .map(|f| crate::store::row::FieldSlot::new(f.as_str()))
                                .collect();
                            for pos in c_start..c_end {
                                let idx = match source_ref {
                                    JoinLeftSource::Idxs(idxs) => idxs[pos],
                                    JoinLeftSource::Scan { .. } => pos,
                                };
                                let Some(row) = col_rows.get(idx) else {
                                    continue;
                                };
                                if need_filter
                                    && let Some(slots) = slots.as_mut()
                                    && !slots.eval(row, now)
                                {
                                    continue;
                                }
                                push_project_batch_row_slots(
                                    &mut local_batch,
                                    row,
                                    &mut proj_slots,
                                );
                            }
                            local_batch
                        }));
                    }
                    handles.into_iter().map(|h| h.join().unwrap()).collect()
                });
                let mut batch = RecordBatch::empty(names_arc);
                let total_rows: usize = thread_batches.iter().map(|b| b.n()).sum();
                for col_i in 0..batch.cols.len() {
                    batch.cols[col_i].reserve(total_rows);
                }
                for mut thread_batch in thread_batches {
                    for (col_i, col) in thread_batch.cols.drain(..).enumerate() {
                        batch.cols[col_i].extend(col);
                    }
                }
                finish_batch(&mut batch, skip_n, explicit_take);
                return Some(batch);
            }
        }

        let init_cap = max_needed.map_or(left_len, |lim| lim.min(left_len));
        let mut batch = RecordBatch::with_capacity(names_arc, init_cap);
        let mut slots = filter.map(super::eval::PredSlots::new);
        let mut proj_slots: Vec<crate::store::row::FieldSlot<'_>> = names
            .iter()
            .map(|f| crate::store::row::FieldSlot::new(f.as_str()))
            .collect();
        let mut pos = 0usize;
        let mut emitted = 0usize;
        while pos < left_len {
            let idx = match &source {
                JoinLeftSource::Idxs(idxs) => idxs[pos],
                JoinLeftSource::Scan { .. } => pos,
            };
            pos += 1;
            let Some(row) = col_rows.get(idx) else {
                continue;
            };
            if need_filter
                && let Some(slots) = slots.as_mut()
                && !slots.eval(row, now)
            {
                continue;
            }
            push_project_batch_row_slots(&mut batch, row, &mut proj_slots);
            emitted += 1;
            if max_needed.is_some_and(|lim| emitted >= lim) {
                break;
            }
        }

        finish_batch(&mut batch, skip_n, explicit_take);
        Some(batch)
    }

    /// Columnar FK join+project (OLAP). Prefer [`Self::run_prepared_batch`].
    pub(super) fn try_join_project_batch(
        &self,
        q: &Query,
        bindings: &BTreeMap<String, Vec<Row>>,
        now: i64,
    ) -> Option<RecordBatch> {
        let Source::Collection(left_name) = &q.source else {
            return None;
        };
        if bindings.get(left_name).is_some() {
            return None;
        }

        let mut filter: Option<&Pred> = None;
        let mut join: Option<(bool, &str, &str)> = None;
        let mut fields: Option<&[Field]> = None;
        let mut explicit_take: Option<Option<i64>> = None;
        let mut skip_n: usize = 0;
        let mut saw_join = false;

        for step in &q.steps {
            match step {
                Step::Filter(p) if !saw_join => {
                    if filter.is_some() {
                        return None;
                    }
                    filter = Some(p);
                }
                Step::Join {
                    left,
                    collection,
                    on,
                } if !saw_join => {
                    saw_join = true;
                    join = Some((*left, collection.as_str(), on.as_str()));
                }
                Step::Project(f) if saw_join => {
                    if fields.is_some() {
                        return None;
                    }
                    fields = Some(f.as_slice());
                }
                Step::Skip { n } if saw_join => {
                    skip_n = skip_n.saturating_add((*n).max(0) as usize);
                }
                Step::Take { n } if saw_join => {
                    if explicit_take.is_some() {
                        return None;
                    }
                    explicit_take = Some(*n);
                }
                Step::Project(_) | Step::Skip { .. } | Step::Take { .. } if !saw_join => {
                    return None;
                }
                _ => return None,
            }
        }

        let (left_join, right_col, on) = join?;
        let fields = fields?;
        let names = field_names(fields);
        if names.is_empty() {
            return None;
        }

        let to_field = self
            .catalog
            .find_fk(left_name, on, right_col)
            .map(|fk| fk.to_field.as_str())
            .unwrap_or("id");
        if to_field != "id" {
            return None;
        }

        if left_name == "orders"
            && right_col == "users"
            && on == "user_id"
            && self.store.orders_soa_ready()
            && self.store.users_soa_ready()
        {
            return self.join_orders_users_soa_batch(
                filter,
                left_join,
                &names,
                skip_n,
                explicit_take,
                now,
            );
        }

        if self.store.is_disk_backed(left_name) || self.store.is_disk_backed(right_col) {
            return self.join_disk_batch(
                left_name,
                right_col,
                on,
                filter,
                left_join,
                &names,
                skip_n,
                explicit_take,
                now,
            );
        }

        let max_needed = batch_max_needed(skip_n, explicit_take);
        let names_arc: Arc<[String]> = names.clone().into();
        if max_needed == Some(0) {
            return Some(RecordBatch::empty(names_arc));
        }

        let (right_fields, plan) = plan_join_fields(right_col, &names);
        let left_rows_col = self.store.collection(left_name);
        let rights = self.store.collection(right_col);

        let (source, need_filter) = {
            if let Some(pred) = filter {
                if let Some((uses, covered)) =
                    crate::index::pick_index_with_coverage(&self.catalog, left_name, pred)
                    && let Some(idxs) = self.store.index_seek(left_name, &uses, now)
                {
                    (JoinLeftSource::Idxs(idxs), !covered)
                } else if let Some(idxs) = self.mirror_select(left_name, pred, now) {
                    (JoinLeftSource::Idxs(idxs), false)
                } else {
                    (
                        JoinLeftSource::Scan {
                            len: left_rows_col.len(),
                        },
                        true,
                    )
                }
            } else {
                (
                    JoinLeftSource::Scan {
                        len: left_rows_col.len(),
                    },
                    false,
                )
            }
        };

        let left_len = match &source {
            JoinLeftSource::Idxs(i) => i.len(),
            JoinLeftSource::Scan { len } => *len,
        };

        let right_by_id = self.store.by_id.get(right_col);
        let store_has_all_ids = right_by_id.is_some_and(|m| m.len() == rights.len());
        let fast_right_by_id = if right_col != "docs" && store_has_all_ids {
            right_by_id
        } else {
            None
        };

        let est_probes = match max_needed {
            Some(lim) if !need_filter && left_join => lim.min(left_len),
            Some(lim) if !need_filter => lim.saturating_mul(2).min(left_len),
            _ => left_len,
        };
        let use_direct_lookup = store_has_all_ids && est_probes < rights.len();
        let probe = if use_direct_lookup {
            None
        } else if store_has_all_ids || left_len * 4 >= rights.len() {
            Some(build_right_probe(rights, &right_fields))
        } else {
            let mut needed: FxHashSet<&str> = FxHashSet::default();
            let mut slots = filter.map(super::eval::PredSlots::new);
            let mut on_slot = crate::store::row::FieldSlot::new(on);
            for i in 0..left_len {
                let idx = match &source {
                    JoinLeftSource::Idxs(idxs) => idxs[i],
                    JoinLeftSource::Scan { .. } => i,
                };
                if let Some(row) = left_rows_col.get(idx)
                    && (!need_filter || slots.as_mut().is_none_or(|s| s.eval(row, now)))
                    && let Some(k) = on_slot.get(row).and_then(Cell::text)
                {
                    needed.insert(k);
                }
            }
            let mut p = FxHashMap::default();
            p.reserve(needed.len());
            let mut id_slot = crate::store::row::FieldSlot::new("id");
            let mut rf_slots: Vec<crate::store::row::FieldSlot<'_>> = right_fields
                .iter()
                .map(|f| crate::store::row::FieldSlot::new(f.as_str()))
                .collect();
            for r in rights {
                if let Some(id) = id_slot.get(r).and_then(Cell::text)
                    && needed.contains(id)
                {
                    let cells = rf_slots
                        .iter_mut()
                        .map(|s| s.get(r).map(Cell::clone_for_borrow).unwrap_or(Cell::Null))
                        .collect();
                    p.insert(id, cells);
                }
            }
            Some(p)
        };

        let init_cap = max_needed.map_or(left_len, |lim| lim.min(left_len));
        let mut batch = RecordBatch::with_capacity(names_arc, init_cap);
        let mut cell_buf: smallvec::SmallVec<[Cell; 8]> = smallvec::SmallVec::new();
        let mut slots = filter.map(super::eval::PredSlots::new);
        let mut on_slot = crate::store::row::FieldSlot::new(on);
        let mut join_slots = build_join_batch_slots(&plan);
        let mut rf_slots: Vec<crate::store::row::FieldSlot<'_>> = right_fields
            .iter()
            .map(|f| crate::store::row::FieldSlot::new(f.as_str()))
            .collect();
        let mut pos = 0usize;
        let mut emitted = 0usize;
        while pos < left_len {
            let idx = match &source {
                JoinLeftSource::Idxs(idxs) => idxs[pos],
                JoinLeftSource::Scan { .. } => pos,
            };
            pos += 1;
            let Some(left_row) = left_rows_col.get(idx) else {
                continue;
            };
            if need_filter
                && let Some(slots) = slots.as_mut()
                && !slots.eval(left_row, now)
            {
                continue;
            }

            if let Some(ref probe_map) = probe {
                let right_cells = on_slot
                    .get(left_row)
                    .and_then(Cell::text)
                    .and_then(|k| probe_map.get(k).map(|v| v.as_slice()));
                if right_cells.is_none() && !left_join {
                    continue;
                }
                push_join_batch_row_slots(&mut batch, left_row, right_cells, &mut join_slots);
            } else {
                let right_row = on_slot.get(left_row).and_then(Cell::text).and_then(|k| {
                    if let Some(m) = fast_right_by_id {
                        m.get(k).and_then(|&ri| rights.get(ri))
                    } else {
                        self.store.get_by_id(right_col, k)
                    }
                });
                if right_row.is_none() && !left_join {
                    continue;
                }
                if let Some(r) = right_row {
                    cell_buf.clear();
                    for s in &mut rf_slots {
                        cell_buf.push(s.get(r).map(Cell::clone_for_borrow).unwrap_or(Cell::Null));
                    }
                    push_join_batch_row_slots(
                        &mut batch,
                        left_row,
                        Some(&cell_buf),
                        &mut join_slots,
                    );
                } else {
                    push_join_batch_row_slots(&mut batch, left_row, None, &mut join_slots);
                }
            }
            emitted += 1;
            if max_needed.is_some_and(|lim| emitted >= lim) {
                break;
            }
        }

        finish_batch(&mut batch, skip_n, explicit_take);
        Some(batch)
    }

    /// FK join + project where at least one side is disk-backed, without materializing a
    /// cold collection in memory: a cold left side is read through the filtered, projected
    /// engine scan, and a cold right side is probed by primary key with only the projected
    /// columns decoded, once per distinct key.
    #[allow(clippy::too_many_arguments)]
    fn join_disk_batch(
        &self,
        left_name: &str,
        right_col: &str,
        on: &str,
        filter: Option<&Pred>,
        left_join: bool,
        names: &[String],
        skip_n: usize,
        explicit_take: Option<Option<i64>>,
        now: i64,
    ) -> Option<RecordBatch> {
        let max_needed = batch_max_needed(skip_n, explicit_take);
        let names_arc: Arc<[String]> = names.to_vec().into();
        if max_needed == Some(0) {
            return Some(RecordBatch::empty(names_arc));
        }

        let (right_fields, plan) = plan_join_fields(right_col, names);
        let eng = self.store.disk_engine_read()?;

        let right_on_disk = self.store.is_disk_backed(right_col);
        let right_refs: Vec<&str> = right_fields.iter().map(String::as_str).collect();
        let preloaded = if right_on_disk {
            eng.get_or_preload_join_dim(right_col, &right_refs)
        } else {
            None
        };

        let mut rf_slots: Vec<crate::store::row::FieldSlot<'_>> = right_fields
            .iter()
            .map(|f| crate::store::row::FieldSlot::new(f.as_str()))
            .collect();

        let mut left_owned: Option<Vec<Row>> = None;
        if self.store.is_disk_backed(left_name) {
            let est_cap = max_needed
                .unwrap_or_else(|| eng.count_table(left_name))
                .clamp(64, 65_536);
            if let Some(ref pre) = preloaded {
                let col_sources: smallvec::SmallVec<[Result<&str, usize>; 8]> = plan
                    .iter()
                    .map(|p| match p {
                        JoinFieldPlan::Left(f) => Ok(f.as_str()),
                        JoinFieldPlan::Right { idx, .. } => Err(*idx),
                    })
                    .collect();
                if let Some(mut batch) = eng
                    .join_lookup_preloaded_batch(
                        left_name,
                        filter,
                        on,
                        left_join,
                        Arc::clone(&names_arc),
                        &col_sources,
                        right_col,
                        &right_refs,
                        pre,
                        est_cap,
                        now,
                    )
                    .ok()
                    .flatten()
                {
                    finish_batch(&mut batch, skip_n, explicit_take);
                    return Some(batch);
                }
            }
            // Join key first, then every projected left column.
            let mut cols: Vec<&str> = vec![on];
            for p in &plan {
                if let JoinFieldPlan::Left(f) = p
                    && !cols.contains(&f.as_str())
                {
                    cols.push(f.as_str());
                }
            }
            // Output column -> index into the streamed left cells, or the right-side slot.
            let slots: Vec<Result<usize, usize>> = plan
                .iter()
                .map(|p| match p {
                    JoinFieldPlan::Left(f) => {
                        Ok(cols.iter().position(|c| *c == f.as_str()).unwrap_or(0))
                    }
                    JoinFieldPlan::Right { idx, .. } => Err(*idx),
                })
                .collect();
            let mut seen_left = smallvec::SmallVec::<[bool; 8]>::from_elem(false, cols.len());
            let mut can_take_left = true;
            for s in &slots {
                if let Ok(i) = *s {
                    if seen_left[i] {
                        can_take_left = false;
                        break;
                    }
                    seen_left[i] = true;
                }
            }
            let mut batch = RecordBatch::with_capacity(Arc::clone(&names_arc), est_cap);
            let mut memo: FxHashMap<String, Option<smallvec::SmallVec<[Cell; 8]>>> =
                FxHashMap::default();
            let mut failed = false;
            if right_on_disk && preloaded.is_none() {
                let chunk_cap = max_needed.map_or(4096, |lim| lim.clamp(64, 4096));
                let mut buffered: Vec<smallvec::SmallVec<[Cell; 8]>> =
                    Vec::with_capacity(chunk_cap);
                let mut flush_chunk =
                    |buf: &mut Vec<smallvec::SmallVec<[Cell; 8]>>,
                     batch: &mut RecordBatch,
                     rf_slots: &mut [crate::store::row::FieldSlot<'_>]|
                     -> bool {
                        let missing: Vec<&str> = buf
                            .iter()
                            .filter_map(|cells| cells[0].text())
                            .filter(|k| !memo.contains_key(*k))
                            .collect();
                        if !missing.is_empty() {
                            let fetched = match eng.get_many_projected_by_id(
                                right_col,
                                missing.iter().copied(),
                                &right_refs,
                            ) {
                                Ok(m) => m,
                                Err(_) => return false,
                            };
                            for k in missing {
                                if !memo.contains_key(k) {
                                    let val = fetched.get(k).map(|r| {
                                        rf_slots
                                            .iter_mut()
                                            .map(|s| {
                                                s.get(r)
                                                    .map(Cell::clone_for_borrow)
                                                    .unwrap_or(Cell::Null)
                                            })
                                            .collect()
                                    });
                                    memo.insert(k.to_string(), val);
                                }
                            }
                        }
                        for mut cells in buf.drain(..) {
                            let right_cells = cells[0]
                                .text()
                                .and_then(|k| memo.get(k).and_then(|v| v.as_deref()));
                            if right_cells.is_none() && !left_join {
                                continue;
                            }
                            for (col_i, slot) in slots.iter().enumerate() {
                                let cell = match slot {
                                    Ok(i) => {
                                        if can_take_left {
                                            std::mem::replace(&mut cells[*i], Cell::Null)
                                        } else {
                                            cells[*i].clone_for_borrow()
                                        }
                                    }
                                    Err(idx) => right_cells
                                        .and_then(|r| r.get(*idx).map(Cell::clone_for_borrow))
                                        .unwrap_or(Cell::Null),
                                };
                                batch.cols[col_i].push(cell);
                            }
                            if max_needed.is_some_and(|lim| batch.n() >= lim) {
                                break;
                            }
                        }
                        true
                    };
                let streamed = eng
                    .scan_cells(left_name, filter, &cols, now, |_, cells| {
                        buffered.push(cells.drain(..).collect());
                        if buffered.len() >= chunk_cap {
                            if !flush_chunk(&mut buffered, &mut batch, &mut rf_slots) {
                                failed = true;
                                return false;
                            }
                            if max_needed.is_some_and(|lim| batch.n() >= lim) {
                                return false;
                            }
                        }
                        true
                    })
                    .ok()?;
                if !failed
                    && streamed
                    && !buffered.is_empty()
                    && max_needed.is_none_or(|lim| batch.n() < lim)
                    && !flush_chunk(&mut buffered, &mut batch, &mut rf_slots)
                {
                    failed = true;
                }
                if failed {
                    return None;
                }
                if streamed {
                    finish_batch(&mut batch, skip_n, explicit_take);
                    return Some(batch);
                }
            } else {
                let streamed = eng
                    .scan_cells(left_name, filter, &cols, now, |_, cells| {
                        let right_cells = match cells[0].text() {
                            None => None,
                            Some(k) => {
                                if let Some(ref pre) = preloaded {
                                    pre.get(k).map(|v| v.as_slice())
                                } else {
                                    if !memo.contains_key(k) {
                                        let found = self.store.get_by_id(right_col, k);
                                        memo.insert(
                                            k.to_string(),
                                            found.map(|r| {
                                                rf_slots
                                                    .iter_mut()
                                                    .map(|s| {
                                                        s.get(r)
                                                            .map(Cell::clone_for_borrow)
                                                            .unwrap_or(Cell::Null)
                                                    })
                                                    .collect()
                                            }),
                                        );
                                    }
                                    memo.get(k).and_then(|v| v.as_deref())
                                }
                            }
                        };
                        if right_cells.is_none() && !left_join {
                            return true;
                        }
                        for (col_i, slot) in slots.iter().enumerate() {
                            let cell = match slot {
                                Ok(i) => {
                                    if can_take_left {
                                        std::mem::replace(&mut cells[*i], Cell::Null)
                                    } else {
                                        cells[*i].clone_for_borrow()
                                    }
                                }
                                Err(idx) => right_cells
                                    .and_then(|r| r.get(*idx).map(Cell::clone_for_borrow))
                                    .unwrap_or(Cell::Null),
                            };
                            batch.cols[col_i].push(cell);
                        }
                        max_needed.is_none_or(|lim| batch.n() < lim)
                    })
                    .ok()?;
                if failed {
                    return None;
                }
                if streamed {
                    finish_batch(&mut batch, skip_n, explicit_take);
                    return Some(batch);
                }
            }
            // Predicate on a nested field: read projected rows through the row scan.
            left_owned = Some(
                eng.query_filter(left_name, filter, None, Some(&cols), now)
                    .ok()?,
            );
        }
        let left_mem: &[Row] = if left_owned.is_none() {
            self.store.collection(left_name)
        } else {
            &[]
        };
        let left_rows = left_owned.as_deref().unwrap_or(left_mem);
        // Rows from the engine scan are already filtered.
        let row_filter = if left_owned.is_some() { None } else { filter };
        let mut join_slots = build_join_batch_slots(&plan);
        let init_cap = max_needed.map_or(left_rows.len(), |lim| lim.min(left_rows.len()));

        if let Some(ref pre) = preloaded {
            let mut batch = RecordBatch::with_capacity(names_arc, init_cap);
            let mut slots = row_filter.map(super::eval::PredSlots::new);
            let mut on_slot = crate::store::row::FieldSlot::new(on);
            for left_row in left_rows {
                if let Some(slots) = slots.as_mut()
                    && !slots.eval(left_row, now)
                {
                    continue;
                }
                let right_cells = on_slot
                    .get(left_row)
                    .and_then(Cell::text)
                    .and_then(|k| pre.get(k).map(|v| v.as_slice()));
                if right_cells.is_none() && !left_join {
                    continue;
                }
                push_join_batch_row_slots(&mut batch, left_row, right_cells, &mut join_slots);
                if max_needed.is_some_and(|lim| batch.n() >= lim) {
                    break;
                }
            }
            finish_batch(&mut batch, skip_n, explicit_take);
            return Some(batch);
        }

        if right_on_disk {
            let mut matched: Vec<(&Row, Option<&str>)> = Vec::with_capacity(left_rows.len());
            let mut slots = row_filter.map(super::eval::PredSlots::new);
            let mut on_slot = crate::store::row::FieldSlot::new(on);
            for left_row in left_rows {
                if let Some(slots) = slots.as_mut()
                    && !slots.eval(left_row, now)
                {
                    continue;
                }
                let key = on_slot.get(left_row).and_then(Cell::text);
                if key.is_none() && !left_join {
                    continue;
                }
                matched.push((left_row, key));
                if left_join && max_needed.is_some_and(|lim| matched.len() >= lim) {
                    break;
                }
            }
            let fetched = eng
                .get_many_projected_by_id(
                    right_col,
                    matched.iter().filter_map(|&(_, k)| k),
                    &right_refs,
                )
                .ok()?;
            let mut memo: FxHashMap<&str, smallvec::SmallVec<[Cell; 8]>> =
                FxHashMap::with_capacity_and_hasher(fetched.len(), Default::default());
            for (k, r) in fetched {
                memo.insert(
                    k,
                    rf_slots
                        .iter_mut()
                        .map(|s| s.get(&r).map(Cell::clone_for_borrow).unwrap_or(Cell::Null))
                        .collect(),
                );
            }
            let mut batch = RecordBatch::with_capacity(
                names_arc,
                max_needed.map_or(matched.len(), |lim| lim.min(matched.len())),
            );
            for (left_row, key) in matched {
                let right_cells = key.and_then(|k| memo.get(k).map(|v| v.as_slice()));
                if right_cells.is_none() && !left_join {
                    continue;
                }
                push_join_batch_row_slots(&mut batch, left_row, right_cells, &mut join_slots);
                if max_needed.is_some_and(|lim| batch.n() >= lim) {
                    break;
                }
            }
            finish_batch(&mut batch, skip_n, explicit_take);
            return Some(batch);
        }

        let mut memo: FxHashMap<&str, Option<smallvec::SmallVec<[Cell; 8]>>> = FxHashMap::default();

        let mut batch = RecordBatch::with_capacity(names_arc, init_cap);
        let mut slots = row_filter.map(super::eval::PredSlots::new);
        let mut on_slot = crate::store::row::FieldSlot::new(on);
        for left_row in left_rows {
            if let Some(slots) = slots.as_mut()
                && !slots.eval(left_row, now)
            {
                continue;
            }
            let right_cells = match on_slot.get(left_row).and_then(Cell::text) {
                None => None,
                Some(k) => {
                    if !memo.contains_key(k) {
                        let found = self.store.get_by_id(right_col, k);
                        memo.insert(
                            k,
                            found.map(|r| {
                                rf_slots
                                    .iter_mut()
                                    .map(|s| {
                                        s.get(r)
                                            .map(Cell::clone_for_borrow)
                                            .unwrap_or(Cell::Null)
                                    })
                                    .collect()
                            }),
                        );
                    }
                    memo.get(k).and_then(|v| v.as_deref())
                }
            };
            if right_cells.is_none() && !left_join {
                continue;
            }
            push_join_batch_row_slots(&mut batch, left_row, right_cells, &mut join_slots);
            if max_needed.is_some_and(|lim| batch.n() >= lim) {
                break;
            }
        }
        finish_batch(&mut batch, skip_n, explicit_take);
        Some(batch)
    }

    /// orders ⋈ users using parallel columns + hash probe on users.
    fn join_orders_users_soa_batch(
        &self,
        filter: Option<&Pred>,
        left_join: bool,
        names: &[String],
        skip_n: usize,
        explicit_take: Option<Option<i64>>,
        now: i64,
    ) -> Option<RecordBatch> {
        let max_needed = batch_max_needed(skip_n, explicit_take);
        let names_arc: Arc<[String]> = names.to_vec().into();
        if max_needed == Some(0) {
            return Some(RecordBatch::empty(names_arc));
        }

        let (right_fields, plan) = plan_join_fields("users", names);
        let users_id = self.store.users_id();
        let users_email = self.store.users_email();
        let store_users_by_id = self
            .store
            .by_id
            .get("users")
            .filter(|m| m.len() == users_id.len());
        let mut fallback_probe: FxHashMap<&str, usize> = FxHashMap::default();
        if store_users_by_id.is_none() {
            fallback_probe.reserve(users_id.len());
            for (i, user_id) in users_id.iter().enumerate() {
                fallback_probe.insert(user_id.as_ref(), i);
            }
        }
        let lookup_user = |uid: &str| -> Option<usize> {
            if let Some(m) = store_users_by_id {
                m.get(uid).copied()
            } else {
                fallback_probe.get(uid).copied()
            }
        };

        let orders_id = self.store.orders_id();
        let orders_uid = self.store.orders_user_id();
        let orders_total = self.store.orders_total();
        let orders_total_tag = self.store.orders_total_tag();
        let orders_rows = self.store.collection("orders");
        let n = orders_id.len();

        let total_gt = filter.and_then(pred_total_gt);
        let (source, need_row_filter) = if total_gt.is_some() {
            (JoinLeftSource::Scan { len: n }, false)
        } else if let Some(pred) = filter {
            if let Some((uses, covered)) =
                crate::index::pick_index_with_coverage(&self.catalog, "orders", pred)
                && let Some(idxs) = self.store.index_seek("orders", &uses, now)
            {
                (JoinLeftSource::Idxs(idxs), !covered)
            } else {
                (JoinLeftSource::Scan { len: n }, true)
            }
        } else {
            (JoinLeftSource::Scan { len: n }, false)
        };

        let fast_id_email_total = plan.len() == 3
            && matches!(plan[0], JoinFieldPlan::Left(ref k) if k == "id")
            && matches!(
                plan[1],
                JoinFieldPlan::Right { ref out, idx: 0 } if out == "users.email"
                    || right_fields.first().is_some_and(|f| f == "email")
            )
            && matches!(plan[2], JoinFieldPlan::Left(ref k) if k == "total");

        let left_len = match &source {
            JoinLeftSource::Idxs(idxs) => idxs.len(),
            JoinLeftSource::Scan { len } => *len,
        };

        let use_parallel = left_len >= super::agg::PAR_AGG_MIN_ROWS
            && max_needed
                .is_none_or(|lim| lim >= 4096 || total_gt.is_some() || need_row_filter);

        if use_parallel {
            let threads = std::thread::available_parallelism()
                .map(|p| p.get())
                .unwrap_or(1)
                .min(8)
                .min(left_len.div_ceil(super::agg::PAR_AGG_CHUNK_ROWS.max(8192)));
            if threads > 1 {
                let chunk_size = left_len.div_ceil(threads);
                if fast_id_email_total {
                    enum WorkerTotalCol {
                        Int(Vec<i64>),
                        Float(Vec<f64>),
                        Cells(Vec<Cell>),
                    }
                    let uniform_tag = if !orders_total_tag.is_empty() {
                        let t0 = orders_total_tag[0];
                        if (t0 == 0 || t0 == 1) && orders_total_tag.iter().all(|&t| t == t0) {
                            Some(t0)
                        } else {
                            None
                        }
                    } else {
                        None
                    };
                    let parts: Vec<(Vec<Cell>, Vec<Cell>, WorkerTotalCol)> =
                        std::thread::scope(|scope| {
                            let mut handles = Vec::with_capacity(threads);
                            for chunk_idx in 0..threads {
                                let c_start = chunk_idx * chunk_size;
                                if c_start >= left_len {
                                    break;
                                }
                                let c_end = (c_start + chunk_size).min(left_len);
                                let source_ref = &source;
                                let lookup_user_ref = &lookup_user;
                                handles.push(scope.spawn(move || {
                                    let cap = c_end - c_start;
                                    let mut c0 = Vec::with_capacity(cap);
                                    let mut c1 = Vec::with_capacity(cap);
                                    let mut c2 = match uniform_tag {
                                        Some(0) => WorkerTotalCol::Int(Vec::with_capacity(cap)),
                                        Some(1) => WorkerTotalCol::Float(Vec::with_capacity(cap)),
                                        _ => WorkerTotalCol::Cells(Vec::with_capacity(cap)),
                                    };
                                    let mut slots = filter.map(super::eval::PredSlots::new);
                                    let mut total_slot =
                                        crate::store::row::FieldSlot::new("total");
                                    for pos in c_start..c_end {
                                        let idx = match source_ref {
                                            JoinLeftSource::Idxs(idxs) => idxs[pos],
                                            JoinLeftSource::Scan { .. } => pos,
                                        };
                                        if idx >= n {
                                            continue;
                                        }
                                        if let Some(min) = total_gt {
                                            if orders_total[idx] <= min {
                                                continue;
                                            }
                                        } else if need_row_filter
                                            && let Some(slots) = slots.as_mut()
                                        {
                                            let Some(left_row) = orders_rows.get(idx) else {
                                                continue;
                                            };
                                            if !slots.eval(left_row, now) {
                                                continue;
                                            }
                                        }
                                        let uid = orders_uid[idx].as_ref();
                                        let Some(ui) = lookup_user_ref(uid) else {
                                            if left_join {
                                                c0.push(Cell::Text(Arc::clone(&orders_id[idx])));
                                                c1.push(Cell::Null);
                                                match &mut c2 {
                                                    WorkerTotalCol::Int(v) => {
                                                        v.push(orders_total[idx] as i64)
                                                    }
                                                    WorkerTotalCol::Float(v) => {
                                                        v.push(orders_total[idx])
                                                    }
                                                    WorkerTotalCol::Cells(v) => {
                                                        v.push(order_total_cell(
                                                            &mut total_slot,
                                                            orders_rows,
                                                            orders_total,
                                                            orders_total_tag,
                                                            idx,
                                                        ))
                                                    }
                                                }
                                            }
                                            continue;
                                        };
                                        c0.push(Cell::Text(Arc::clone(&orders_id[idx])));
                                        c1.push(Cell::Text(Arc::clone(&users_email[ui])));
                                        match &mut c2 {
                                            WorkerTotalCol::Int(v) => {
                                                v.push(orders_total[idx] as i64)
                                            }
                                            WorkerTotalCol::Float(v) => v.push(orders_total[idx]),
                                            WorkerTotalCol::Cells(v) => v.push(order_total_cell(
                                                &mut total_slot,
                                                orders_rows,
                                                orders_total,
                                                orders_total_tag,
                                                idx,
                                            )),
                                        }
                                    }
                                    (c0, c1, c2)
                                }));
                            }
                            handles.into_iter().map(|h| h.join().unwrap()).collect()
                        });
                    let total_rows: usize = parts.iter().map(|(c0, _, _)| c0.len()).sum();
                    if total_rows == 0 {
                        return Some(RecordBatch::empty(names_arc));
                    }
                    let mut out0 = Vec::with_capacity(total_rows);
                    let mut out1 = Vec::with_capacity(total_rows);
                    let out2 = match uniform_tag {
                        Some(0) => {
                            let mut vals = Vec::with_capacity(total_rows);
                            for (mut c0, mut c1, c2) in parts {
                                out0.append(&mut c0);
                                out1.append(&mut c1);
                                if let WorkerTotalCol::Int(mut v) = c2 {
                                    vals.append(&mut v);
                                }
                            }
                            Column::Int {
                                vals,
                                valid: crate::batch::Validity::all_valid(total_rows),
                            }
                        }
                        Some(1) => {
                            let mut vals = Vec::with_capacity(total_rows);
                            for (mut c0, mut c1, c2) in parts {
                                out0.append(&mut c0);
                                out1.append(&mut c1);
                                if let WorkerTotalCol::Float(mut v) = c2 {
                                    vals.append(&mut v);
                                }
                            }
                            Column::Float {
                                vals,
                                valid: crate::batch::Validity::all_valid(total_rows),
                            }
                        }
                        _ => {
                            let mut col = Column::with_capacity(total_rows);
                            for (mut c0, mut c1, c2) in parts {
                                out0.append(&mut c0);
                                out1.append(&mut c1);
                                if let WorkerTotalCol::Cells(v) = c2 {
                                    for cell in v {
                                        col.push(cell);
                                    }
                                }
                            }
                            col
                        }
                    };
                    let mut batch = RecordBatch {
                        names: names_arc,
                        cols: vec![Column::Cells(out0), Column::Cells(out1), out2],
                    };
                    finish_batch(&mut batch, skip_n, explicit_take);
                    return Some(batch);
                }

                let thread_batches: Vec<RecordBatch> = std::thread::scope(|scope| {
                    let mut handles = Vec::with_capacity(threads);
                    for chunk_idx in 0..threads {
                        let c_start = chunk_idx * chunk_size;
                        if c_start >= left_len {
                            break;
                        }
                        let c_end = (c_start + chunk_size).min(left_len);
                        let source_ref = &source;
                        let lookup_user_ref = &lookup_user;
                        let plan_ref = &plan;
                        let names_arc_clone = names_arc.clone();
                        let right_fields_ref = &right_fields;
                        handles.push(scope.spawn(move || {
                            let mut local_batch =
                                RecordBatch::with_capacity(names_arc_clone, c_end - c_start);
                            let mut slots = filter.map(super::eval::PredSlots::new);
                            let mut total_slot = crate::store::row::FieldSlot::new("total");
                            for pos in c_start..c_end {
                                let idx = match source_ref {
                                    JoinLeftSource::Idxs(idxs) => idxs[pos],
                                    JoinLeftSource::Scan { .. } => pos,
                                };
                                if idx >= n {
                                    continue;
                                }
                                if let Some(min) = total_gt {
                                    if orders_total[idx] <= min {
                                        continue;
                                    }
                                } else if need_row_filter && let Some(slots) = slots.as_mut() {
                                    let Some(left_row) = orders_rows.get(idx) else {
                                        continue;
                                    };
                                    if !slots.eval(left_row, now) {
                                        continue;
                                    }
                                }
                                let uid = orders_uid[idx].as_ref();
                                let right_i = lookup_user_ref(uid);
                                if right_i.is_none() && !left_join {
                                    continue;
                                }
                                for (col_i, p) in plan_ref.iter().enumerate() {
                                    let cell = match p {
                                        JoinFieldPlan::Left(k) => match k.as_str() {
                                            "id" => Cell::Text(Arc::clone(&orders_id[idx])),
                                            "user_id" => Cell::Text(Arc::clone(&orders_uid[idx])),
                                            "total" => order_total_cell(
                                                &mut total_slot,
                                                orders_rows,
                                                orders_total,
                                                orders_total_tag,
                                                idx,
                                            ),
                                            _ => orders_rows
                                                .get(idx)
                                                .and_then(|r| r.get(k).map(Cell::clone_for_borrow))
                                                .unwrap_or(Cell::Null),
                                        },
                                        JoinFieldPlan::Right { out: _, idx: r_idx } => {
                                            match right_i {
                                                Some(ui) => match right_fields_ref
                                                    .get(*r_idx)
                                                    .map(String::as_str)
                                                {
                                                    Some("id") => {
                                                        Cell::Text(Arc::clone(&users_id[ui]))
                                                    }
                                                    Some("email") => {
                                                        Cell::Text(Arc::clone(&users_email[ui]))
                                                    }
                                                    _ => Cell::Null,
                                                },
                                                None => Cell::Null,
                                            }
                                        }
                                    };
                                    local_batch.cols[col_i].push(cell);
                                }
                            }
                            local_batch
                        }));
                    }
                    handles.into_iter().map(|h| h.join().unwrap()).collect()
                });
                let mut batch = RecordBatch::empty(names_arc);
                let total_rows: usize = thread_batches.iter().map(|b| b.n()).sum();
                for col_i in 0..batch.cols.len() {
                    batch.cols[col_i].reserve(total_rows);
                }
                for mut thread_batch in thread_batches {
                    for (col_i, col) in thread_batch.cols.drain(..).enumerate() {
                        batch.cols[col_i].extend(col);
                    }
                }
                finish_batch(&mut batch, skip_n, explicit_take);
                return Some(batch);
            }
        }

        let init_cap = max_needed.map_or(left_len, |lim| lim.min(left_len));
        let mut slots = filter.map(super::eval::PredSlots::new);
        let mut total_slot = crate::store::row::FieldSlot::new("total");
        let mut pos = 0usize;
        if fast_id_email_total {
            enum TotalCol {
                Int(Vec<i64>),
                Float(Vec<f64>),
                Cells(Column),
            }
            let uniform_tag = if !orders_total_tag.is_empty() {
                let t0 = orders_total_tag[0];
                let check_len = if matches!(source, JoinLeftSource::Scan { .. })
                    && total_gt.is_none()
                    && !need_row_filter
                    && left_join
                {
                    init_cap.min(orders_total_tag.len())
                } else {
                    orders_total_tag.len()
                };
                if (t0 == 0 || t0 == 1)
                    && orders_total_tag[..check_len].iter().all(|&t| t == t0)
                {
                    Some(t0)
                } else {
                    None
                }
            } else {
                None
            };
            let mut col0 = Vec::with_capacity(init_cap);
            let mut col1 = Vec::with_capacity(init_cap);
            let mut total_col = match uniform_tag {
                Some(0) => TotalCol::Int(Vec::with_capacity(init_cap)),
                Some(1) => TotalCol::Float(Vec::with_capacity(init_cap)),
                _ => TotalCol::Cells(Column::with_capacity(init_cap)),
            };
            while pos < left_len {
                let idx = match &source {
                    JoinLeftSource::Idxs(idxs) => idxs[pos],
                    JoinLeftSource::Scan { .. } => pos,
                };
                pos += 1;
                if idx >= n {
                    continue;
                }
                if let Some(min) = total_gt {
                    if orders_total[idx] <= min {
                        continue;
                    }
                } else if need_row_filter && let Some(slots) = slots.as_mut() {
                    let Some(left_row) = orders_rows.get(idx) else {
                        continue;
                    };
                    if !slots.eval(left_row, now) {
                        continue;
                    }
                }
                let uid = orders_uid[idx].as_ref();
                let Some(ui) = lookup_user(uid) else {
                    if left_join {
                        col0.push(Cell::Text(Arc::clone(&orders_id[idx])));
                        col1.push(Cell::Null);
                        match &mut total_col {
                            TotalCol::Int(vals) => vals.push(orders_total[idx] as i64),
                            TotalCol::Float(vals) => vals.push(orders_total[idx]),
                            TotalCol::Cells(col) => col.push(order_total_cell(
                                &mut total_slot,
                                orders_rows,
                                orders_total,
                                orders_total_tag,
                                idx,
                            )),
                        }
                        if max_needed.is_some_and(|lim| col0.len() >= lim) {
                            break;
                        }
                    }
                    continue;
                };
                col0.push(Cell::Text(Arc::clone(&orders_id[idx])));
                col1.push(Cell::Text(Arc::clone(&users_email[ui])));
                match &mut total_col {
                    TotalCol::Int(vals) => vals.push(orders_total[idx] as i64),
                    TotalCol::Float(vals) => vals.push(orders_total[idx]),
                    TotalCol::Cells(col) => col.push(order_total_cell(
                        &mut total_slot,
                        orders_rows,
                        orders_total,
                        orders_total_tag,
                        idx,
                    )),
                }
                if max_needed.is_some_and(|lim| col0.len() >= lim) {
                    break;
                }
            }
            if col0.is_empty() {
                return Some(RecordBatch::empty(names_arc));
            }
            let n_out = col0.len();
            let c2 = match total_col {
                TotalCol::Int(vals) => crate::batch::Column::Int {
                    vals,
                    valid: crate::batch::Validity::all_valid(n_out),
                },
                TotalCol::Float(vals) => crate::batch::Column::Float {
                    vals,
                    valid: crate::batch::Validity::all_valid(n_out),
                },
                TotalCol::Cells(col) => col,
            };
            let mut batch = RecordBatch {
                names: names_arc,
                cols: vec![
                    crate::batch::Column::Cells(col0),
                    crate::batch::Column::Cells(col1),
                    c2,
                ],
            };
            finish_batch(&mut batch, skip_n, explicit_take);
            return Some(batch);
        }

        let mut batch = RecordBatch::with_capacity(names_arc, init_cap);
        let mut emitted = 0usize;
        while pos < left_len {
            let idx = match &source {
                JoinLeftSource::Idxs(idxs) => idxs[pos],
                JoinLeftSource::Scan { .. } => pos,
            };
            pos += 1;
            if idx >= n {
                continue;
            }
            if let Some(min) = total_gt {
                if orders_total[idx] <= min {
                    continue;
                }
            } else if need_row_filter && let Some(slots) = slots.as_mut() {
                let Some(left_row) = orders_rows.get(idx) else {
                    continue;
                };
                if !slots.eval(left_row, now) {
                    continue;
                }
            }

            let uid = orders_uid[idx].as_ref();
            let right_i = lookup_user(uid);
            if right_i.is_none() && !left_join {
                continue;
            }

            for (col_i, p) in plan.iter().enumerate() {
                let cell = match p {
                    JoinFieldPlan::Left(k) => match k.as_str() {
                        "id" => Cell::Text(Arc::clone(&orders_id[idx])),
                        "user_id" => Cell::Text(Arc::clone(&orders_uid[idx])),
                        "total" => order_total_cell(
                            &mut total_slot,
                            orders_rows,
                            orders_total,
                            orders_total_tag,
                            idx,
                        ),
                        _ => orders_rows
                            .get(idx)
                            .and_then(|r| r.get(k).map(Cell::clone_for_borrow))
                            .unwrap_or(Cell::Null),
                    },
                    JoinFieldPlan::Right { out: _, idx: ri } => {
                        let Some(ui) = right_i else {
                            batch.cols[col_i].push(Cell::Null);
                            continue;
                        };
                        let fname = right_fields.get(*ri).map(|s| s.as_str()).unwrap_or("");
                        match fname {
                            "id" => Cell::Text(Arc::clone(&users_id[ui])),
                            "email" => Cell::Text(Arc::clone(&users_email[ui])),
                            _ => Cell::Null,
                        }
                    }
                };
                batch.cols[col_i].push(cell);
            }
            emitted += 1;
            if max_needed.is_some_and(|lim| emitted >= lim) {
                break;
            }
        }

        finish_batch(&mut batch, skip_n, explicit_take);
        Some(batch)
    }

    pub(super) fn join_rows(
        &self,
        left_col: &str,
        left: Vec<Row>,
        right_col: &str,
        on: &str,
        left_join: bool,
    ) -> Result<Vec<Row>, Error> {
        let to_field = self
            .catalog
            .find_fk(left_col, on, right_col)
            .map(|fk| fk.to_field.as_str())
            .unwrap_or("id");

        if left.is_empty() {
            return Ok(Vec::new());
        }

        let mut on_slot = crate::store::row::FieldSlot::new(on);
        let mut merger = JoinRowMerger::new(right_col);

        // Cold right side: probe the disk engine instead of materializing the whole
        // collection in memory.
        if self.store.is_disk_backed(right_col)
            && let Some(eng) = self.store.disk_engine_read()
        {
            let over_quota = |n: usize| -> Result<(), Error> {
                if n > self.quotas.max_join_expansion {
                    return Err(Error::runtime(format!(
                        "Cartesian join expansion {} exceeds max_join_expansion quota {}",
                        n, self.quotas.max_join_expansion
                    )));
                }
                Ok(())
            };
            if to_field == "id" {
                // Every distinct key is looked up once, in one batch ordered by position
                // on disk.
                let keys: Vec<Arc<str>> = left
                    .iter()
                    .filter_map(|l| on_slot.get(l).and_then(Cell::text).map(Arc::from))
                    .collect();
                let found = eng.get_many_by_id(right_col, keys.iter().map(|k| &**k))?;
                let mut out = Vec::with_capacity(left.len());
                for l in left {
                    let right = on_slot.get(&l).and_then(Cell::text).and_then(|k| found.get(k));
                    match right {
                        Some(r) => out.push(merger.merge(l, r)),
                        None if left_join => out.push(l),
                        None => {}
                    }
                    over_quota(out.len())?;
                }
                return Ok(out);
            }
            // Non-id key: one streaming pass keeps only the right rows some left row needs.
            let needed_keys: FxHashSet<String> = left
                .iter()
                .filter_map(|l| on_slot.get(l).and_then(cell_key))
                .collect();
            if needed_keys.is_empty() {
                return Ok(if left_join { left } else { Vec::new() });
            }
            let mut idx: FxHashMap<String, Vec<Row>> = FxHashMap::default();
            let mut to_slot = crate::store::row::FieldSlot::new(to_field);
            let mut keep = |r: &Row| {
                if let Some(k) = cell_key_ref(to_slot.get(r).unwrap_or(&Cell::Null))
                    && needed_keys.contains(k.as_ref())
                {
                    match idx.get_mut(k.as_ref()) {
                        Some(rs) => rs.push(r.clone()),
                        None => {
                            idx.insert(k.into_owned(), vec![r.clone()]);
                        }
                    }
                }
            };
            // Text keys in a bounded number: push `key in [...]` into the scan, so segments
            // are filtered on the key column (dictionary codes, secondary index) and only
            // matching rows are materialized.
            const MAX_PUSHED_KEYS: usize = 4096;
            let text_keys: Option<Vec<crate::ast::Value>> = (needed_keys.len() <= MAX_PUSHED_KEYS
                && left.iter().all(|l| match on_slot.get(l) {
                    None | Some(Cell::Null | Cell::Text(_) | Cell::SmallText(_)) => true,
                    Some(_) => false,
                }))
            .then(|| {
                needed_keys
                    .iter()
                    .map(|k| crate::ast::Value::String(Arc::from(k.as_str())))
                    .collect()
            });
            match text_keys {
                Some(values) => {
                    let pred = Pred::In {
                        field: Field::name(to_field),
                        values,
                    };
                    for r in eng.query_filter(right_col, Some(&pred), None, None, 0)? {
                        keep(&r);
                    }
                }
                // Any other key set: one pass over the key column alone, materializing
                // only the rows some left row joins to.
                None => eng.scan_by_key_set(right_col, to_field, &needed_keys, |r| {
                    keep(&r);
                    Ok(true)
                })?,
            }
            let mut out = Vec::with_capacity(left.len());
            for mut l in left {
                let hits = on_slot
                    .get(&l)
                    .and_then(cell_key_ref)
                    .and_then(|k| idx.get(k.as_ref()));
                match hits {
                    Some(rs) => {
                        for (i, r) in rs.iter().enumerate() {
                            let base = if i + 1 == rs.len() {
                                std::mem::take(&mut l)
                            } else {
                                l.clone()
                            };
                            out.push(merger.merge(base, r));
                            over_quota(out.len())?;
                        }
                    }
                    None if left_join => out.push(l),
                    None => {}
                }
            }
            return Ok(out);
        }

        if to_field == "id" {
            let right = self.store.collection(right_col);
            let right_by_id = self.store.by_id.get(right_col);
            let store_has_all_ids = right_by_id.is_some_and(|m| m.len() == right.len());
            if store_has_all_ids {
                let fast_right_by_id = (right_col != "docs").then_some(right_by_id).flatten();
                let mut out = Vec::with_capacity(left.len());
                for l in left {
                    let right_row = on_slot.get(&l).and_then(Cell::text).and_then(|k| {
                        if let Some(m) = fast_right_by_id {
                            m.get(k).and_then(|&ri| right.get(ri))
                        } else {
                            self.store.get_by_id(right_col, k)
                        }
                    });
                    match right_row {
                        Some(r) => out.push(merger.merge(l, r)),
                        None if left_join => out.push(l),
                        None => {}
                    }
                    if out.len() > self.quotas.max_join_expansion {
                        return Err(Error::runtime(format!(
                            "Cartesian join expansion {} exceeds max_join_expansion quota {}",
                            out.len(),
                            self.quotas.max_join_expansion
                        )));
                    }
                }
                return Ok(out);
            }

            let needed: Option<FxHashSet<&str>> = if left.len() * 4 < right.len() {
                Some(
                    left.iter()
                        .filter_map(|l| on_slot.get(l).and_then(Cell::text))
                        .collect(),
                )
            } else {
                None
            };

            let mut by_id: FxHashMap<&str, &Row> = FxHashMap::default();
            let mut id_slot = crate::store::row::FieldSlot::new("id");
            if let Some(ref needed_set) = needed {
                by_id.reserve(needed_set.len());
                for r in right {
                    if let Some(id) = id_slot.get(r).and_then(Cell::text)
                        && needed_set.contains(id)
                    {
                        by_id.insert(id, r);
                    }
                }
            } else {
                by_id.reserve(right.len());
                for r in right {
                    if let Some(id) = id_slot.get(r).and_then(Cell::text) {
                        by_id.insert(id, r);
                    }
                }
            }

            let mut out = Vec::with_capacity(left.len());
            for l in left {
                let right_row = on_slot
                    .get(&l)
                    .and_then(Cell::text)
                    .and_then(|k| by_id.get(k).copied());
                match right_row {
                    Some(r) => out.push(merger.merge(l, r)),
                    None if left_join => out.push(l),
                    None => {}
                }
            }
            return Ok(out);
        }

        let right = self.store.collection(right_col);
        let needed_keys: FxHashSet<String> = left
            .iter()
            .filter_map(|l| on_slot.get(l).and_then(cell_key))
            .collect();
        if needed_keys.is_empty() {
            return Ok(if left_join { left } else { Vec::new() });
        }

        let mut idx: FxHashMap<String, Vec<&Row>> = FxHashMap::default();
        idx.reserve(needed_keys.len());
        let mut to_slot = crate::store::row::FieldSlot::new(to_field);
        for r in right {
            if let Some(k) = cell_key_ref(to_slot.get(r).unwrap_or(&Cell::Null))
                && needed_keys.contains(k.as_ref())
            {
                match idx.get_mut(k.as_ref()) {
                    Some(rs) => rs.push(r),
                    None => {
                        idx.insert(k.into_owned(), vec![r]);
                    }
                }
            }
        }
        let mut out = Vec::with_capacity(left.len());
        for mut l in left {
            let hits = on_slot
                .get(&l)
                .and_then(cell_key_ref)
                .and_then(|k| idx.get(k.as_ref()));
            if let Some(rs) = hits {
                for (i, r) in rs.iter().enumerate() {
                    let base = if i + 1 == rs.len() {
                        std::mem::take(&mut l)
                    } else {
                        l.clone()
                    };
                    out.push(merger.merge(base, r));
                    if out.len() > self.quotas.max_join_expansion {
                        return Err(Error::runtime(format!(
                            "Cartesian join expansion {} exceeds max_join_expansion quota {}",
                            out.len(),
                            self.quotas.max_join_expansion
                        )));
                    }
                }
            } else if left_join {
                out.push(l);
            }
        }
        Ok(out)
    }

    /// Vectorized `RecordBatch` pipeline over post-scan / post-join `Filter` (conjunctions of
    /// scalar `Pred::Cmp`), `Project`, `Without`, `Sort`, `Skip`, and `Take` using
    /// [`RecordBatch::eval_cmp_indices`], [`RecordBatch::filter_indices`],
    /// [`RecordBatch::into_filtered_indices`], [`RecordBatch::project_cols`],
    /// [`RecordBatch::into_projected_cols`], [`RecordBatch::without_cols`], and
    /// [`RecordBatch::sort_by_col`] without materializing `Vec<Row>`.
    pub(super) fn try_vectorized_pipeline_batch(
        &self,
        q: &Query,
        bindings: &BTreeMap<String, Vec<Row>>,
        now: i64,
    ) -> Option<RecordBatch> {
        #[cfg(feature = "gpu")]
        if self.gpu.is_some() {
            return None;
        }
        let Source::Collection(left_name) = &q.source else {
            return None;
        };
        if left_name == "catalog" || bindings.contains_key(left_name) {
            return None;
        }
        if self.store.get_col_count(left_name) > self.quotas.max_execution_work {
            return None;
        }

        #[inline]
        fn parse_field_str(s: &str) -> Field {
            Field {
                parts: s.split('.').map(str::to_string).collect(),
            }
        }

        fn collect_conjunctive_cmp_fields(pred: &Pred, out: &mut Vec<String>) -> bool {
            match pred {
                Pred::And(a, b) => {
                    collect_conjunctive_cmp_fields(a, out) && collect_conjunctive_cmp_fields(b, out)
                }
                Pred::Cmp { field, value, .. } => {
                    if matches!(value, Value::Param(_)) {
                        return false;
                    }
                    let name = field.as_str();
                    if !out.contains(&name) {
                        out.push(name);
                    }
                    true
                }
                _ => false,
            }
        }

        fn eval_conjunctive_cmp(
            batch: &RecordBatch,
            pred: &Pred,
            now: i64,
            scratch: &mut Vec<u32>,
            acc: &mut Option<Vec<u32>>,
        ) -> bool {
            match pred {
                Pred::And(a, b) => {
                    if !eval_conjunctive_cmp(batch, a, now, scratch, acc) {
                        return false;
                    }
                    eval_conjunctive_cmp(batch, b, now, scratch, acc)
                }
                Pred::Cmp { field, op, value } => {
                    let lit = super::eval::value_cell(value, now);
                    if matches!(lit, Cell::Null | Cell::Vec(_)) {
                        return false;
                    }
                    let fname = field.as_str();
                    scratch.clear();
                    if let Some(col_idx) = batch.names.iter().position(|n| n == &fname) {
                        if !batch.eval_cmp_indices(col_idx, *op, &lit, scratch) {
                            return false;
                        }
                    } else if matches!(op, CmpOp::Ne) {
                        let n = batch.n();
                        if n > u32::MAX as usize {
                            return false;
                        }
                        scratch.extend(0..n as u32);
                    }
                    match acc {
                        None => {
                            *acc = Some(std::mem::take(scratch));
                        }
                        Some(cur) => {
                            if cur.is_empty() {
                                return true;
                            }
                            if scratch.is_empty() {
                                cur.clear();
                                return true;
                            }
                            let mut w = 0usize;
                            let mut j = 0usize;
                            for r in 0..cur.len() {
                                let target = cur[r];
                                while j < scratch.len() && scratch[j] < target {
                                    j += 1;
                                }
                                if j < scratch.len() && scratch[j] == target {
                                    cur[w] = target;
                                    w += 1;
                                    j += 1;
                                }
                            }
                            cur.truncate(w);
                        }
                    }
                    true
                }
                _ => false,
            }
        }

        let mut join_idx: Option<usize> = None;
        for (i, step) in q.steps.iter().enumerate() {
            match step {
                Step::Join { .. } => {
                    if join_idx.is_some() {
                        return None;
                    }
                    if i > 1 || (i == 1 && !matches!(q.steps[0], Step::Filter(_))) {
                        return None;
                    }
                    join_idx = Some(i);
                }
                Step::Filter(_)
                | Step::Project(_)
                | Step::Without(_)
                | Step::Sort { .. }
                | Step::Skip { .. }
                | Step::Take { .. } => {}
                _ => return None,
            }
        }

        let post_start: usize;
        let base_steps_prefix: Vec<Step>;
        let uniform_all_cols: Option<Vec<String>>;

        if let Some(j_pos) = join_idx {
            let Step::Join {
                collection: right_col,
                ..
            } = &q.steps[j_pos]
            else {
                unreachable!()
            };
            if bindings.contains_key(right_col) {
                return None;
            }
            post_start = j_pos + 1;
            let mut prefix = Vec::with_capacity(j_pos + 2);
            for s in &q.steps[..=j_pos] {
                prefix.push(s.clone());
            }
            base_steps_prefix = prefix;

            let has_post_project = q.steps[post_start..]
                .iter()
                .any(|s| matches!(s, Step::Project(_)));
            if has_post_project {
                uniform_all_cols = None;
            } else if !self.store.is_disk_backed(left_name) && !self.store.is_disk_backed(right_col)
            {
                let l_rows = self.store.collection(left_name);
                let r_rows = self.store.collection(right_col);
                if l_rows.is_empty() {
                    return Some(RecordBatch::empty(Arc::<[String]>::from(Vec::<String>::new())));
                }
                let ls = l_rows[0].shape_arc()?;
                if !l_rows
                    .iter()
                    .all(|r| r.shape_arc().is_some_and(|s| Arc::ptr_eq(s, ls)))
                {
                    return None;
                }
                let mut merged_names: Vec<String> = ls.names().to_vec();
                if !r_rows.is_empty() {
                    let rs = r_rows[0].shape_arc()?;
                    if !r_rows
                        .iter()
                        .all(|r| r.shape_arc().is_some_and(|s| Arc::ptr_eq(s, rs)))
                    {
                        return None;
                    }
                    if let Step::Join { left: true, .. } = &q.steps[j_pos] {
                        return None;
                    }
                    for rn in rs.names() {
                        merged_names.push(format!("{right_col}.{rn}"));
                    }
                    merged_names.sort_unstable();
                    merged_names.dedup();
                }
                uniform_all_cols = Some(merged_names);
            } else {
                return None;
            }
        } else {
            let has_pre_filter = matches!(q.steps.first(), Some(Step::Filter(_)));
            post_start = usize::from(has_pre_filter);
            let mut prefix = Vec::with_capacity(2);
            if has_pre_filter {
                prefix.push(q.steps[0].clone());
            }
            base_steps_prefix = prefix;

            let has_post_project = q.steps[post_start..]
                .iter()
                .any(|s| matches!(s, Step::Project(_)));
            if has_post_project {
                uniform_all_cols = None;
            } else if self.store.is_disk_backed(left_name) {
                let eng = self.store.disk_engine_read()?;
                let mut cols = eng.uniform_non_null_columns(left_name)?;
                cols.sort_unstable();
                cols.dedup();
                uniform_all_cols = Some(cols);
            } else {
                let l_rows = self.store.collection(left_name);
                if l_rows.is_empty() {
                    return Some(RecordBatch::empty(Arc::<[String]>::from(Vec::<String>::new())));
                }
                let ls = l_rows[0].shape_arc()?;
                if !l_rows
                    .iter()
                    .all(|r| r.shape_arc().is_some_and(|s| Arc::ptr_eq(s, ls)))
                {
                    return None;
                }
                uniform_all_cols = Some(ls.names().to_vec());
            }
        }

        let mut needed: Vec<String> = match uniform_all_cols {
            Some(cols) => cols,
            None => {
                let mut cols: Vec<String> = Vec::new();
                let mut saw_first_proj = false;
                for step in &q.steps[post_start..] {
                    match step {
                        Step::Filter(p) => {
                            if !collect_conjunctive_cmp_fields(p, &mut cols) {
                                return None;
                            }
                        }
                        Step::Sort { field, .. } => {
                            let nm = field.as_str();
                            if !saw_first_proj && !cols.contains(&nm) {
                                cols.push(nm);
                            }
                        }
                        Step::Project(fs) => {
                            if !saw_first_proj {
                                for f in fs {
                                    let nm = f.as_str();
                                    if !cols.contains(&nm) {
                                        cols.push(nm);
                                    }
                                }
                                saw_first_proj = true;
                            }
                        }
                        Step::Without(fs) => {
                            if !saw_first_proj {
                                for f in fs {
                                    if f.leaf().is_none() && !f.as_str().contains('.') {
                                        return None;
                                    }
                                }
                            }
                        }
                        Step::Skip { .. } | Step::Take { .. } => {}
                        _ => return None,
                    }
                }
                cols
            }
        };
        if needed.is_empty() {
            return None;
        }

        let needed_fields: Vec<Field> = needed.drain(..).map(|s| parse_field_str(&s)).collect();
        let mut base_steps = base_steps_prefix;
        base_steps.push(Step::Project(needed_fields));
        base_steps.push(Step::Take { n: None });
        let base_q = Query {
            source: q.source.clone(),
            steps: base_steps,
            explain: None,
            ignore_filter: q.ignore_filter,
        };

        let mut batch = if join_idx.is_some() {
            self.try_join_project_batch(&base_q, bindings, now)?
        } else {
            self.try_filter_project_batch(&base_q, bindings, now)?
        };

        let mut active_projection = false;
        let mut implicit_take = true;
        let mut cmp_scratch: Vec<u32> = Vec::new();
        let post_steps = &q.steps[post_start..];
        let mut idx = 0usize;

        while idx < post_steps.len() {
            match &post_steps[idx] {
                Step::Filter(pred) => {
                    if !batch.is_empty() {
                        let mut keep: Option<Vec<u32>> = None;
                        if !eval_conjunctive_cmp(&batch, pred, now, &mut cmp_scratch, &mut keep) {
                            return None;
                        }
                        if let Some(indices) = keep
                            && indices.len() < batch.n()
                        {
                            batch = if indices.is_empty() {
                                batch.filter_indices(&[])
                            } else {
                                batch.into_filtered_indices(&indices)
                            };
                        }
                    }
                    idx += 1;
                }
                Step::Project(fields) => {
                    let out_names = field_names(fields);
                    let row_n = batch.n();
                    if out_names
                        .iter()
                        .all(|nm| batch.names.iter().any(|bn| bn == nm))
                    {
                        let col_idxs: Vec<usize> = out_names
                            .iter()
                            .map(|nm| batch.names.iter().position(|bn| bn == nm).unwrap())
                            .collect();
                        batch = if row_n == 0 {
                            batch.project_cols(&col_idxs, out_names.into())
                        } else {
                            batch.into_projected_cols(&col_idxs, out_names.into())
                        };
                    } else {
                        let mut rem: smallvec::SmallVec<[usize; 16]> =
                            smallvec::SmallVec::from_elem(0, batch.cols.len());
                        let mut mapping: smallvec::SmallVec<[Option<usize>; 16]> =
                            smallvec::SmallVec::with_capacity(out_names.len());
                        for nm in &out_names {
                            let pos = batch.names.iter().position(|bn| bn == nm);
                            if let Some(p) = pos {
                                rem[p] += 1;
                            }
                            mapping.push(pos);
                        }
                        let mut cols = Vec::with_capacity(out_names.len());
                        for pos_opt in mapping {
                            match pos_opt {
                                Some(p) => {
                                    rem[p] -= 1;
                                    if rem[p] == 0 {
                                        cols.push(std::mem::take(&mut batch.cols[p]));
                                    } else {
                                        cols.push(batch.cols[p].clone());
                                    }
                                }
                                None => cols.push(Column::Nulls {
                                    n: row_n,
                                    cap: 0,
                                }),
                            }
                        }
                        batch = RecordBatch {
                            names: out_names.into(),
                            cols,
                        };
                    }
                    active_projection = true;
                    idx += 1;
                }
                Step::Without(fields) => {
                    let drop_owned = field_names(fields);
                    let drop_refs: smallvec::SmallVec<[&str; 8]> =
                        drop_owned.iter().map(String::as_str).collect();
                    batch = batch.without_cols(&drop_refs);
                    active_projection = false;
                    idx += 1;
                }
                Step::Sort { field, desc } => {
                    let key_name = field.as_str();
                    let mut tail_only = true;
                    let mut look_skip = 0usize;
                    let mut look_take: Option<Option<i64>> = None;
                    for s in &post_steps[idx + 1..] {
                        match s {
                            Step::Skip { n } if look_take.is_none() => {
                                look_skip = look_skip.saturating_add((*n).max(0) as usize);
                            }
                            Step::Take { n } if look_take.is_none() => {
                                look_take = Some(*n);
                            }
                            _ => {
                                tail_only = false;
                                break;
                            }
                        }
                    }
                    if tail_only {
                        let eff_take: Option<usize> = match look_take {
                            Some(Some(t)) => {
                                implicit_take = false;
                                Some(t.max(0) as usize)
                            }
                            Some(None) => {
                                implicit_take = false;
                                None
                            }
                            None => implicit_take.then_some(50),
                        };
                        if eff_take.is_some() {
                            implicit_take = false;
                        }
                        if let Some(col_idx) = batch.names.iter().position(|n| n == &key_name) {
                            batch = batch.sort_by_col(col_idx, *desc, look_skip, eff_take);
                        } else {
                            finish_batch(
                                &mut batch,
                                look_skip,
                                Some(eff_take.map(|t| t as i64)),
                            );
                        }
                        break;
                    }
                    if let Some(col_idx) = batch.names.iter().position(|n| n == &key_name) {
                        batch = batch.sort_by_col(col_idx, *desc, 0, None);
                    }
                    idx += 1;
                }
                Step::Skip { n } => {
                    let s = (*n).max(0) as usize;
                    if s >= batch.n() {
                        batch.truncate(0);
                    } else if s > 0 {
                        batch.drain_prefix(s);
                    }
                    idx += 1;
                }
                Step::Take { n } => {
                    implicit_take = false;
                    if let Some(t) = n {
                        batch.truncate((*t).max(0) as usize);
                    }
                    idx += 1;
                }
                _ => return None,
            }
        }

        if implicit_take && batch.n() > 50 {
            batch.truncate(50);
        }

        if !active_projection {
            if batch.is_empty() {
                return Some(RecordBatch::empty(Arc::<[String]>::from(Vec::<String>::new())));
            }
            let is_sorted_unique = batch
                .names
                .windows(2)
                .all(|w| w[0].as_str() < w[1].as_str());
            if !is_sorted_unique {
                let mut map: BTreeMap<String, usize> = BTreeMap::new();
                for (i, nm) in batch.names.iter().enumerate() {
                    map.insert(nm.clone(), i);
                }
                let sorted_names: Vec<String> = map.keys().cloned().collect();
                let idxs: Vec<usize> = map.into_values().collect();
                batch = batch.into_projected_cols(&idxs, sorted_names.into());
            }
        }

        Some(batch)
    }
}

/// `total` of the order at position `idx`, reconstructed directly from `(totals, tags)`
/// without touching `rows[idx]` whenever `tags[idx]` is `0` (`Int` <= 2^53) or `1` (`Float`),
/// falling back to `rows[idx]` only for `Null`/absent/large-integer edge cases (`tag == 2`).
#[inline]
pub(crate) fn order_total_cell(
    slot: &mut crate::store::row::FieldSlot<'static>,
    rows: &[Row],
    totals: &[f64],
    tags: &[u8],
    idx: usize,
) -> Cell {
    match tags.get(idx).copied() {
        Some(0) => Cell::Int(totals[idx] as i64),
        Some(1) => Cell::Float(totals[idx]),
        _ => match rows.get(idx) {
            Some(row) => slot.get(row).map(Cell::clone_for_borrow).unwrap_or(Cell::Null),
            None => Cell::Float(totals[idx]),
        },
    }
}

/// Positions of `keys` in the order [`super::eval::sort_rows_top_k`] would put rows with
/// these keys, equal keys included. The standard sorts pick their strategy by element
/// type, so equal keys are only placed the same way when the same element type is
/// sorted with the same comparator: up to 32 rows this sorts small `Row`s with
/// `sort_rows_top_k` itself, above that the `(key, position)` pairs it sorts there.
pub(crate) fn sort_positions_top_k(keys: &[Cell], desc: bool, top_k: Option<usize>) -> Vec<usize> {
    use super::eval::cmp_sort;
    let n = keys.len();
    if n <= 32 {
        static IK_SHAPE: std::sync::OnceLock<Arc<crate::store::RowShape>> =
            std::sync::OnceLock::new();
        let shape = IK_SHAPE.get_or_init(|| Row::shape_of(&["i", "k"]));
        let mut rows: Vec<Row> = keys
            .iter()
            .enumerate()
            .map(|(i, k)| {
                Row::from_parts(
                    Arc::clone(shape),
                    [Cell::Int(i as i64), k.clone_for_borrow()],
                )
            })
            .collect();
        super::eval::sort_rows_top_k(&mut rows, &Field::name("k"), desc, top_k);
        return rows
            .iter()
            .map(|r| match r.cells_slice().first() {
                Some(Cell::Int(i)) => *i as usize,
                _ => unreachable!("position column"),
            })
            .collect();
    }
    if top_k == Some(0) {
        return Vec::new();
    }
    let mut decorated: Vec<(&Cell, usize)> = keys
        .iter()
        .enumerate()
        .map(|(i, k)| (k, i))
        .collect();
    let cmp_keys = |a: &(&Cell, usize), b: &(&Cell, usize)| {
        let ord = cmp_sort(a.0, b.0);
        let ord = if desc { ord.reverse() } else { ord };
        ord.then_with(|| a.1.cmp(&b.1))
    };
    if let Some(limit) = top_k
        && limit < decorated.len()
    {
        decorated.select_nth_unstable_by(limit - 1, cmp_keys);
        decorated.truncate(limit);
    }
    decorated.sort_unstable_by(cmp_keys);
    decorated.into_iter().map(|(_, i)| i).collect()
}

/// Output cardinality and build-side memory ceiling for explicit join operators (`D21`: `LIN-0821`..`LIN-0824`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JoinBudget {
    pub max_output_rows: usize,
    pub max_build_bytes: usize,
}

/// Physical join operator family (`D21`: `LIN-0813`..`LIN-0816`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum JoinAlgorithmKind {
    HashJoin,
    SortMergeJoin,
    HashSemiJoin,
    HashAntiJoin,
}

/// Execution telemetry and skew diagnostics for join operators (`D21`: `LIN-0837`..`LIN-0840`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JoinDiagnostics {
    pub algorithm: JoinAlgorithmKind,
    pub left_rows: usize,
    pub right_rows: usize,
    pub output_rows: usize,
    pub hot_keys: Vec<(String, usize)>,
    pub build_ns: u64,
    pub probe_ns: u64,
}

const EXACT_F64_INT_LIMIT: f64 = 9_007_199_254_740_992.0; // 2^53
const EXACT_I64_LIMIT_U64: u64 = 1u64 << 53;

/// Cross-numeric-type canonical join key (`D21`: `LIN-0804`, `LIN-0805`).
///
/// - `Cell::Null` and `Cell::Float(NaN)` map to `None` (`NULL != NULL` and `NaN != NaN`).
/// - Integral `Cell::Float(x)` with `|x| <= 2^53` normalizes to `NormalizedJoinKey::Int(x as i64)`
///   (`-0.0` and `+0.0` both normalize to `Int(0)`), so `Int(42)` and `Float(42.0)` hash and
///   compare identically without precision loss on large integers outside `[-2^53, 2^53]`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum NormalizedJoinKey {
    Int(i64),
    FloatBits(u64),
    Bool(bool),
    Time(i64),
    Text(crate::store::InlineStr),
    HeapText(Arc<str>),
}

impl NormalizedJoinKey {
    fn display_label(&self) -> String {
        match self {
            Self::Int(i) => i.to_string(),
            Self::FloatBits(b) => f64::from_bits(*b).to_string(),
            Self::Bool(b) => b.to_string(),
            Self::Time(t) => format!("time({t})"),
            Self::Text(s) => s.as_str().to_owned(),
            Self::HeapText(s) => s.as_ref().to_owned(),
        }
    }

    fn approx_bytes(&self) -> usize {
        match self {
            Self::HeapText(s) => 40 + s.len(),
            _ => 32,
        }
    }
}

/// Normalize a scalar [`Cell`] into a canonical equi-join key (`None` for `Null`, `NaN`, or `Vec`).
#[inline]
pub fn normalize_join_key(cell: &Cell) -> Option<NormalizedJoinKey> {
    match cell {
        Cell::Null | Cell::Vec(_) => None,
        Cell::Int(i) => Some(NormalizedJoinKey::Int(*i)),
        Cell::Float(f) => {
            if f.is_nan() {
                return None;
            }
            if f.is_finite()
                && f.fract() == 0.0
                && (-EXACT_F64_INT_LIMIT..=EXACT_F64_INT_LIMIT).contains(f)
            {
                return Some(NormalizedJoinKey::Int(*f as i64));
            }
            let norm = if *f == 0.0 { 0.0f64 } else { *f };
            Some(NormalizedJoinKey::FloatBits(norm.to_bits()))
        }
        Cell::Bool(b) => Some(NormalizedJoinKey::Bool(*b)),
        Cell::Time(t) => Some(NormalizedJoinKey::Time(*t)),
        Cell::SmallText(s) => Some(NormalizedJoinKey::Text(*s)),
        Cell::Text(arc) => {
            if let Some(inline) = crate::store::InlineStr::try_new(arc.as_ref()) {
                Some(NormalizedJoinKey::Text(inline))
            } else {
                Some(NormalizedJoinKey::HeapText(Arc::clone(arc)))
            }
        }
    }
}

/// Test equi-join equality between two cells under cross-numeric normalization (`LIN-0804`).
#[inline]
pub fn join_cells_match(left: &Cell, right: &Cell) -> bool {
    match (normalize_join_key(left), normalize_join_key(right)) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    }
}

/// Strict total order over [`NormalizedJoinKey`] consistent with [`crate::eval::cmp_sort`] and
/// exact `Int`/`Float` equality within `[-2^53, 2^53]`.
fn cmp_normalized_join_keys(a: &NormalizedJoinKey, b: &NormalizedJoinKey) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    fn rank(k: &NormalizedJoinKey) -> u8 {
        match k {
            NormalizedJoinKey::Bool(_) => 1,
            NormalizedJoinKey::Int(_) | NormalizedJoinKey::FloatBits(_) => 2,
            NormalizedJoinKey::Time(_) => 3,
            NormalizedJoinKey::Text(_) | NormalizedJoinKey::HeapText(_) => 4,
        }
    }
    let ra = rank(a);
    let rb = rank(b);
    if ra != rb {
        return ra.cmp(&rb);
    }
    match (a, b) {
        (NormalizedJoinKey::Bool(x), NormalizedJoinKey::Bool(y)) => x.cmp(y),
        (NormalizedJoinKey::Time(x), NormalizedJoinKey::Time(y)) => x.cmp(y),
        (NormalizedJoinKey::Int(x), NormalizedJoinKey::Int(y)) => x.cmp(y),
        (NormalizedJoinKey::FloatBits(xb), NormalizedJoinKey::FloatBits(yb)) => {
            let xf = f64::from_bits(*xb);
            let yf = f64::from_bits(*yb);
            xf.partial_cmp(&yf).unwrap_or(Ordering::Equal)
        }
        (NormalizedJoinKey::Int(x), NormalizedJoinKey::FloatBits(yb)) => {
            let yf = f64::from_bits(*yb);
            match super::eval::cmp_int_float(*x, yf) {
                Ordering::Equal if x.unsigned_abs() > EXACT_I64_LIMIT_U64 => Ordering::Less,
                other => other,
            }
        }
        (NormalizedJoinKey::FloatBits(xb), NormalizedJoinKey::Int(y)) => {
            let xf = f64::from_bits(*xb);
            match super::eval::cmp_int_float(*y, xf).reverse() {
                Ordering::Equal if y.unsigned_abs() > EXACT_I64_LIMIT_U64 => Ordering::Greater,
                other => other,
            }
        }
        _ => {
            let sa = match a {
                NormalizedJoinKey::Text(s) => s.as_str(),
                NormalizedJoinKey::HeapText(s) => s.as_ref(),
                _ => "",
            };
            let sb = match b {
                NormalizedJoinKey::Text(s) => s.as_str(),
                NormalizedJoinKey::HeapText(s) => s.as_ref(),
                _ => "",
            };
            sa.cmp(sb)
        }
    }
}

fn summarize_hot_keys(counts: impl Iterator<Item = (NormalizedJoinKey, usize)>) -> Vec<(String, usize)> {
    let mut hot: Vec<(String, usize)> = counts
        .filter(|(_, c)| *c > 1)
        .map(|(k, c)| (k.display_label(), c))
        .collect();
    hot.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    hot.truncate(8);
    hot
}

/// Equi-join using a normalized hash table (`D21`: `LIN-0804`, `LIN-0805`, `LIN-0821`..`LIN-0840`).
pub fn hash_join_rows(
    left: &[Row],
    right: &[Row],
    left_key: &str,
    right_key: &str,
    right_prefix: &str,
    budget: &JoinBudget,
    mut is_cancelled: impl FnMut() -> bool,
) -> Result<(Vec<Row>, JoinDiagnostics), Error> {
    let build_start = std::time::Instant::now();
    let mut r_slot = crate::store::row::FieldSlot::new(right_key);
    let mut map: FxHashMap<NormalizedJoinKey, smallvec::SmallVec<[usize; 2]>> =
        FxHashMap::default();
    let mut build_bytes = 0usize;

    for (r_idx, r_row) in right.iter().enumerate() {
        if (r_idx & 63) == 0 && is_cancelled() {
            return Err(Error::canceled("hash join cancelled during build phase"));
        }
        let Some(key) = r_slot.get(r_row).and_then(normalize_join_key) else {
            continue;
        };
        let row_est = crate::store::row_byte_size(r_row);
        match map.entry(key) {
            std::collections::hash_map::Entry::Occupied(mut e) => {
                build_bytes = build_bytes.saturating_add(8 + row_est);
                if build_bytes > budget.max_build_bytes {
                    return Err(Error::resource_limit(format!(
                        "hash join build_bytes {build_bytes} exceeds max_build_bytes {}",
                        budget.max_build_bytes
                    )));
                }
                e.get_mut().push(r_idx);
            }
            std::collections::hash_map::Entry::Vacant(e) => {
                build_bytes = build_bytes.saturating_add(e.key().approx_bytes() + 24 + row_est);
                if build_bytes > budget.max_build_bytes {
                    return Err(Error::resource_limit(format!(
                        "hash join build_bytes {build_bytes} exceeds max_build_bytes {}",
                        budget.max_build_bytes
                    )));
                }
                e.insert(smallvec::smallvec![r_idx]);
            }
        }
    }
    let build_ns = build_start.elapsed().as_nanos() as u64;

    let probe_start = std::time::Instant::now();
    let mut l_slot = crate::store::row::FieldSlot::new(left_key);
    let mut merger = JoinRowMerger::new(right_prefix);
    let mut out = Vec::new();

    for (l_idx, l_row) in left.iter().enumerate() {
        if (l_idx & 63) == 0 && is_cancelled() {
            return Err(Error::canceled("hash join cancelled during probe phase"));
        }
        let Some(key) = l_slot.get(l_row).and_then(normalize_join_key) else {
            continue;
        };
        let Some(matches) = map.get(&key) else {
            continue;
        };
        for &r_idx in matches {
            if out.len() + 1 > budget.max_output_rows {
                return Err(Error::resource_limit(format!(
                    "hash join output rows {} exceeds max_output_rows {}",
                    out.len() + 1,
                    budget.max_output_rows
                )));
            }
            out.push(merger.merge(l_row.clone(), &right[r_idx]));
        }
    }
    let probe_ns = probe_start.elapsed().as_nanos() as u64;
    let hot_keys = summarize_hot_keys(map.into_iter().map(|(k, v)| (k, v.len())));

    let diag = JoinDiagnostics {
        algorithm: JoinAlgorithmKind::HashJoin,
        left_rows: left.len(),
        right_rows: right.len(),
        output_rows: out.len(),
        hot_keys,
        build_ns,
        probe_ns,
    };
    Ok((out, diag))
}

/// Sort-merge equi-join over inputs sorted ascending by `(left_key, right_key)` with full
/// duplicate peer-group handling, cross-numeric `Int`/`Float` matching, budget enforcement,
/// and cancellation (`D21`: `LIN-0813`, `LIN-0821`..`LIN-0840`).
pub fn merge_join_sorted_rows(
    left: &[Row],
    right: &[Row],
    left_key: &str,
    right_key: &str,
    right_prefix: &str,
    budget: &JoinBudget,
    mut is_cancelled: impl FnMut() -> bool,
) -> Result<(Vec<Row>, JoinDiagnostics), Error> {
    let build_start = std::time::Instant::now();
    let mut l_slot = crate::store::row::FieldSlot::new(left_key);
    let mut r_slot = crate::store::row::FieldSlot::new(right_key);

    let mut l_keys: Vec<Option<NormalizedJoinKey>> = Vec::with_capacity(left.len());
    let mut prev_l_idx: Option<usize> = None;
    for (idx, row) in left.iter().enumerate() {
        if (idx & 63) == 0 && is_cancelled() {
            return Err(Error::canceled("sort-merge join cancelled while scanning left keys"));
        }
        let k = l_slot.get(row).and_then(normalize_join_key);
        if let (Some(pi), Some(cur)) = (prev_l_idx, k.as_ref())
            && let Some(ref p) = l_keys[pi]
            && cmp_normalized_join_keys(p, cur) == std::cmp::Ordering::Greater
        {
            return Err(Error::runtime(
                "merge_join_sorted_rows: left input is not sorted ascending by join key",
            ));
        }
        if k.is_some() {
            prev_l_idx = Some(idx);
        }
        l_keys.push(k);
    }

    let mut r_keys: Vec<Option<NormalizedJoinKey>> = Vec::with_capacity(right.len());
    let mut prev_r_idx: Option<usize> = None;
    for (idx, row) in right.iter().enumerate() {
        if (idx & 63) == 0 && is_cancelled() {
            return Err(Error::canceled("sort-merge join cancelled while scanning right keys"));
        }
        let k = r_slot.get(row).and_then(normalize_join_key);
        if let (Some(pi), Some(cur)) = (prev_r_idx, k.as_ref())
            && let Some(ref p) = r_keys[pi]
            && cmp_normalized_join_keys(p, cur) == std::cmp::Ordering::Greater
        {
            return Err(Error::runtime(
                "merge_join_sorted_rows: right input is not sorted ascending by join key",
            ));
        }
        if k.is_some() {
            prev_r_idx = Some(idx);
        }
        r_keys.push(k);
    }
    let build_ns = build_start.elapsed().as_nanos() as u64;

    let probe_start = std::time::Instant::now();
    let mut merger = JoinRowMerger::new(right_prefix);
    let mut out = Vec::new();
    let mut hot_counts: Vec<(NormalizedJoinKey, usize)> = Vec::new();

    let mut li = 0usize;
    let mut ri = 0usize;
    while li < left.len() && ri < right.len() {
        if is_cancelled() {
            return Err(Error::canceled("sort-merge join cancelled during merge phase"));
        }
        let Some(ref lk) = l_keys[li] else {
            li += 1;
            continue;
        };
        let Some(ref rk) = r_keys[ri] else {
            ri += 1;
            continue;
        };
        match cmp_normalized_join_keys(lk, rk) {
            std::cmp::Ordering::Less => {
                li += 1;
            }
            std::cmp::Ordering::Greater => {
                ri += 1;
            }
            std::cmp::Ordering::Equal => {
                let mut l_end = li + 1;
                while l_end < left.len() {
                    match &l_keys[l_end] {
                        Some(next_lk)
                            if cmp_normalized_join_keys(lk, next_lk)
                                == std::cmp::Ordering::Equal =>
                        {
                            l_end += 1;
                        }
                        None => break,
                        _ => break,
                    }
                }
                let mut r_end = ri + 1;
                let mut group_build_bytes = crate::store::row_byte_size(&right[ri]);
                while r_end < right.len() {
                    match &r_keys[r_end] {
                        Some(next_rk)
                            if cmp_normalized_join_keys(rk, next_rk)
                                == std::cmp::Ordering::Equal =>
                        {
                            group_build_bytes = group_build_bytes
                                .saturating_add(crate::store::row_byte_size(&right[r_end]));
                            r_end += 1;
                        }
                        None => break,
                        _ => break,
                    }
                }
                if group_build_bytes > budget.max_build_bytes {
                    return Err(Error::resource_limit(format!(
                        "sort-merge join peer-group bytes {group_build_bytes} exceeds max_build_bytes {}",
                        budget.max_build_bytes
                    )));
                }
                let r_group_len = r_end - ri;
                if r_group_len > 1 {
                    hot_counts.push((rk.clone(), r_group_len));
                }
                for l_pos in li..l_end {
                    for r_pos in ri..r_end {
                        if ((out.len()) & 63) == 0 && is_cancelled() {
                            return Err(Error::canceled(
                                "sort-merge join cancelled during peer expansion",
                            ));
                        }
                        if out.len() + 1 > budget.max_output_rows {
                            return Err(Error::resource_limit(format!(
                                "sort-merge join output rows {} exceeds max_output_rows {}",
                                out.len() + 1,
                                budget.max_output_rows
                            )));
                        }
                        out.push(merger.merge(left[l_pos].clone(), &right[r_pos]));
                    }
                }
                li = l_end;
                ri = r_end;
            }
        }
    }
    let probe_ns = probe_start.elapsed().as_nanos() as u64;
    let hot_keys = summarize_hot_keys(hot_counts.into_iter());

    let diag = JoinDiagnostics {
        algorithm: JoinAlgorithmKind::SortMergeJoin,
        left_rows: left.len(),
        right_rows: right.len(),
        output_rows: out.len(),
        hot_keys,
        build_ns,
        probe_ns,
    };
    Ok((out, diag))
}

fn build_semi_anti_key_counts(
    right: &[Row],
    right_key: &str,
    budget: &JoinBudget,
    is_cancelled: &mut impl FnMut() -> bool,
    op_label: &str,
) -> Result<(FxHashMap<NormalizedJoinKey, usize>, u64), Error> {
    let start = std::time::Instant::now();
    let mut r_slot = crate::store::row::FieldSlot::new(right_key);
    let mut map: FxHashMap<NormalizedJoinKey, usize> = FxHashMap::default();
    let mut build_bytes = 0usize;

    for (r_idx, r_row) in right.iter().enumerate() {
        if (r_idx & 63) == 0 && is_cancelled() {
            return Err(Error::canceled(format!("{op_label} cancelled during build phase")));
        }
        let Some(key) = r_slot.get(r_row).and_then(normalize_join_key) else {
            continue;
        };
        if let Some(cnt) = map.get_mut(&key) {
            *cnt += 1;
        } else {
            build_bytes = build_bytes.saturating_add(key.approx_bytes() + 24);
            if build_bytes > budget.max_build_bytes {
                return Err(Error::resource_limit(format!(
                    "{op_label} build_bytes {build_bytes} exceeds max_build_bytes {}",
                    budget.max_build_bytes
                )));
            }
            map.insert(key, 1);
        }
    }
    Ok((map, start.elapsed().as_nanos() as u64))
}

/// Left semi-join (`EXISTS` semantics): emits each `left` row at most once when at least one
/// `right` row matches under cross-numeric key normalization (`D21`: `LIN-0815`).
pub fn semi_join_rows(
    left: &[Row],
    right: &[Row],
    left_key: &str,
    right_key: &str,
    budget: &JoinBudget,
    mut is_cancelled: impl FnMut() -> bool,
) -> Result<(Vec<Row>, JoinDiagnostics), Error> {
    let (map, build_ns) =
        build_semi_anti_key_counts(right, right_key, budget, &mut is_cancelled, "semi join")?;

    let probe_start = std::time::Instant::now();
    let mut l_slot = crate::store::row::FieldSlot::new(left_key);
    let mut out = Vec::new();

    for (l_idx, l_row) in left.iter().enumerate() {
        if (l_idx & 63) == 0 && is_cancelled() {
            return Err(Error::canceled("semi join cancelled during probe phase"));
        }
        let matched = l_slot
            .get(l_row)
            .and_then(normalize_join_key)
            .is_some_and(|k| map.contains_key(&k));
        if matched {
            if out.len() + 1 > budget.max_output_rows {
                return Err(Error::resource_limit(format!(
                    "semi join output rows {} exceeds max_output_rows {}",
                    out.len() + 1,
                    budget.max_output_rows
                )));
            }
            out.push(l_row.clone());
        }
    }
    let probe_ns = probe_start.elapsed().as_nanos() as u64;
    let hot_keys = summarize_hot_keys(map.into_iter());

    let diag = JoinDiagnostics {
        algorithm: JoinAlgorithmKind::HashSemiJoin,
        left_rows: left.len(),
        right_rows: right.len(),
        output_rows: out.len(),
        hot_keys,
        build_ns,
        probe_ns,
    };
    Ok((out, diag))
}

/// Left anti-join (`NOT EXISTS` semantics): emits each `left` row that has no matching
/// `right` row under cross-numeric key normalization (`D21`: `LIN-0816`).
pub fn anti_join_rows(
    left: &[Row],
    right: &[Row],
    left_key: &str,
    right_key: &str,
    budget: &JoinBudget,
    mut is_cancelled: impl FnMut() -> bool,
) -> Result<(Vec<Row>, JoinDiagnostics), Error> {
    let (map, build_ns) =
        build_semi_anti_key_counts(right, right_key, budget, &mut is_cancelled, "anti join")?;

    let probe_start = std::time::Instant::now();
    let mut l_slot = crate::store::row::FieldSlot::new(left_key);
    let mut out = Vec::new();

    for (l_idx, l_row) in left.iter().enumerate() {
        if (l_idx & 63) == 0 && is_cancelled() {
            return Err(Error::canceled("anti join cancelled during probe phase"));
        }
        let matched = l_slot
            .get(l_row)
            .and_then(normalize_join_key)
            .is_some_and(|k| map.contains_key(&k));
        if !matched {
            if out.len() + 1 > budget.max_output_rows {
                return Err(Error::resource_limit(format!(
                    "anti join output rows {} exceeds max_output_rows {}",
                    out.len() + 1,
                    budget.max_output_rows
                )));
            }
            out.push(l_row.clone());
        }
    }
    let probe_ns = probe_start.elapsed().as_nanos() as u64;
    let hot_keys = summarize_hot_keys(map.into_iter());

    let diag = JoinDiagnostics {
        algorithm: JoinAlgorithmKind::HashAntiJoin,
        left_rows: left.len(),
        right_rows: right.len(),
        output_rows: out.len(),
        hot_keys,
        build_ns,
        probe_ns,
    };
    Ok((out, diag))
}

#[cfg(test)]
mod sort_positions_tests {
    use super::sort_positions_top_k;
    use crate::ast::Field;
    use crate::store::{Cell, Row};

    /// The position permutation must reproduce the row sort exactly, ties included,
    /// on both sides of the 32-row switch and for every kind of `take`.
    #[test]
    fn matches_row_sort_including_ties() {
        let mut seed = 0x9e37_79b9_7f4a_7c15u64;
        let mut next = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        for n in 0..90usize {
            for round in 0..6 {
                let keys: Vec<Cell> = (0..n)
                    .map(|_| match next() % 7 {
                        0 => Cell::Null,
                        1 => Cell::Float(f64::NAN),
                        2 | 3 => Cell::Int((next() % 5) as i64),
                        4 => Cell::Float((next() % 5) as f64 + 0.5),
                        _ => Cell::from_text(["a", "b", "c"][(next() % 3) as usize]),
                    })
                    .collect();
                for desc in [false, true] {
                    for top_k in [None, Some(0), Some(1), Some(3), Some(n / 2 + 1), Some(n + 5)] {
                        let mut rows: Vec<Row> = keys
                            .iter()
                            .enumerate()
                            .map(|(i, k)| Row::from([("k", k.clone()), ("i", Cell::Int(i as i64))]))
                            .collect();
                        super::super::eval::sort_rows_top_k(&mut rows, &Field::name("k"), desc, top_k);
                        let want: Vec<usize> = rows
                            .iter()
                            .map(|r| match r.get("i") {
                                Some(Cell::Int(i)) => *i as usize,
                                other => panic!("{other:?}"),
                            })
                            .collect();
                        let got = sort_positions_top_k(&keys, desc, top_k);
                        assert_eq!(got, want, "n={n} round={round} desc={desc} top_k={top_k:?}");
                    }
                }
            }
        }
    }
}
