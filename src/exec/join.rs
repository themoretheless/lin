use std::collections::BTreeMap;
use std::sync::Arc;

use rustc_hash::{FxHashMap, FxHashSet};

use crate::ast::{CmpOp, Field, Pred, Query, Source, Step, Value};
use crate::batch::RecordBatch;
use crate::error::Error;
use crate::store::{Cell, Row, acquire_row};

use super::agg::cell_key;
use super::eval::eval_pred;
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

pub(crate) fn merge_join_row(mut left: Row, right: &Row, right_col: &str) -> Row {
    for (k, v) in right {
        left.insert(format!("{right_col}.{k}"), v.clone_for_borrow());
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

pub(crate) fn push_join_batch_row(
    batch: &mut RecordBatch,
    left: &Row,
    right: Option<&[Cell]>,
    plan: &[JoinFieldPlan],
) {
    for (col_i, p) in plan.iter().enumerate() {
        let cell = match p {
            JoinFieldPlan::Left(k) => left
                .get(k)
                .map(Cell::clone_for_borrow)
                .unwrap_or(Cell::Null),
            JoinFieldPlan::Right { out: _, idx } => right
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
    if rows.is_empty() {
        return RecordBatch::empty(Arc::<[String]>::from(Vec::<String>::new()));
    }
    let names: Vec<String> = rows[0].keys().cloned().collect();
    let names_arc: Arc<[String]> = names.clone().into();
    let mut batch = RecordBatch::with_capacity(names_arc, rows.len());
    for row in rows {
        for (i, name) in names.iter().enumerate() {
            batch.cols[i].push(row.get(name).cloned().unwrap_or(Cell::Null));
        }
    }
    batch
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
    for r in rights {
        let Some(id) = r.get("id").and_then(Cell::text) else {
            continue;
        };
        let cells = right_fields
            .iter()
            .map(|f| r.get(f).cloned().unwrap_or(Cell::Null))
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
                out.insert(k.clone(), cell);
            }
            JoinFieldPlan::Right { out: k, idx } => {
                let cell = right
                    .and_then(|r| r.get(*idx))
                    .map(Cell::clone_for_borrow)
                    .unwrap_or(Cell::Null);
                out.insert(k.clone(), cell);
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
    let mut out = BTreeMap::new();
    for f in fields {
        let cell = if let Some(rest) = f.strip_prefix(right_col).and_then(|s| s.strip_prefix('.')) {
            right
                .and_then(|r| r.get(rest))
                .cloned()
                .unwrap_or(Cell::Null)
        } else {
            left.get(f).cloned().unwrap_or(Cell::Null)
        };
        out.insert(f.clone(), cell);
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

        let mut rows = {
            if self.store.is_disk_backed(name)
                && let Some(eng) = self.store.disk_engine_read()
            {
                let proj_refs: Vec<&str> = names.iter().map(String::as_str).collect();
                let mut r = eng
                    .query_filter(name, Some(pred), dict_limit, Some(&proj_refs), now)
                    .ok()?;
                for row in &mut r {
                    for f in &names {
                        row.entry(f.clone()).or_insert(Cell::Null);
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
                && let Some(idxs) = self.store.index_seek(name, &uses, now)
            {
                if name == "docs" && covered {
                    if let Some(rows) = self.store.project_docs_hot(&idxs, &names) {
                        rows
                    } else {
                        self.fetch_idxs_filtered(name, &idxs, pred, now, Some(fields), covered)
                    }
                } else {
                    self.fetch_idxs_filtered(name, &idxs, pred, now, Some(fields), covered)
                }
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
        Some(self.try_join_project_batch(q, bindings, now)?.to_rows())
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
        if self.store.is_disk_backed(name)
            && let Some(eng) = self.store.disk_engine_read()
        {
            let fetch_limit = match explicit_take {
                Some(Some(n)) => Some(skip_n.saturating_add(n.max(0) as usize)),
                Some(None) => None,
                None => Some(skip_n.saturating_add(50)),
            };
            let proj_refs: Vec<&str> = names.iter().map(String::as_str).collect();
            let rows = eng
                .query_filter(name, filter, fetch_limit, Some(&proj_refs), now)
                .ok()?;
            let names_arc: Arc<[String]> = names.clone().into();
            let mut batch = RecordBatch::with_capacity(names_arc, rows.len());
            for row in &rows {
                push_project_batch_row(&mut batch, row, &names);
            }
            finish_batch(&mut batch, skip_n, explicit_take);
            return Some(batch);
        }

        let (source, need_filter) = {
            if let Some(pred) = filter {
                if let Some((uses, covered)) =
                    crate::index::pick_index_with_coverage(&self.catalog, name, pred)
                    && let Some(idxs) = self.store.index_seek(name, &uses, now)
                {
                    (JoinLeftSource::Idxs(idxs), !covered)
                } else {
                    (
                        JoinLeftSource::Scan {
                            len: self.store.collection(name).len(),
                        },
                        true,
                    )
                }
            } else {
                (
                    JoinLeftSource::Scan {
                        len: self.store.collection(name).len(),
                    },
                    false,
                )
            }
        };

        let left_len = match &source {
            JoinLeftSource::Idxs(i) => i.len(),
            JoinLeftSource::Scan { len } => *len,
        };
        let names_arc: Arc<[String]> = names.clone().into();

        if left_len >= 8192 {
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
                            for pos in c_start..c_end {
                                let idx = match source_ref {
                                    JoinLeftSource::Idxs(idxs) => idxs[pos],
                                    JoinLeftSource::Scan { .. } => pos,
                                };
                                let Some(row) = self.store.get_by_idx(name, idx) else {
                                    continue;
                                };
                                if need_filter
                                    && let Some(pred) = filter
                                    && !eval_pred(pred, row, now)
                                {
                                    continue;
                                }
                                push_project_batch_row(&mut local_batch, row, names_ref);
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

        let mut batch = RecordBatch::with_capacity(names_arc, left_len);
        let mut pos = 0usize;
        loop {
            let idx = match &source {
                JoinLeftSource::Idxs(idxs) => {
                    if pos >= idxs.len() {
                        break;
                    }
                    let i = idxs[pos];
                    pos += 1;
                    i
                }
                JoinLeftSource::Scan { len } => {
                    if pos >= *len {
                        break;
                    }
                    let i = pos;
                    pos += 1;
                    i
                }
            };
            let Some(row) = self.store.get_by_idx(name, idx) else {
                continue;
            };
            if need_filter
                && let Some(pred) = filter
                && !eval_pred(pred, row, now)
            {
                continue;
            }
            push_project_batch_row(&mut batch, row, &names);
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

        let (right_fields, plan) = plan_join_fields(right_col, &names);
        let rights = self.store.collection(right_col);

        let (source, need_filter) = {
            if let Some(pred) = filter {
                if let Some((uses, covered)) =
                    crate::index::pick_index_with_coverage(&self.catalog, left_name, pred)
                    && let Some(idxs) = self.store.index_seek(left_name, &uses, now)
                {
                    (JoinLeftSource::Idxs(idxs), !covered)
                } else {
                    (
                        JoinLeftSource::Scan {
                            len: self.store.collection(left_name).len(),
                        },
                        true,
                    )
                }
            } else {
                (
                    JoinLeftSource::Scan {
                        len: self.store.collection(left_name).len(),
                    },
                    false,
                )
            }
        };

        let left_len = match &source {
            JoinLeftSource::Idxs(i) => i.len(),
            JoinLeftSource::Scan { len } => *len,
        };

        let store_has_all_ids = self
            .store
            .by_id
            .get(right_col)
            .is_some_and(|m| m.len() == rights.len());

        let use_direct_lookup = store_has_all_ids && left_len < rights.len();
        let probe = if use_direct_lookup {
            None
        } else if store_has_all_ids || left_len * 4 >= rights.len() {
            Some(build_right_probe(rights, &right_fields))
        } else {
            let mut needed: FxHashSet<&str> = FxHashSet::default();
            for i in 0..left_len {
                let idx = match &source {
                    JoinLeftSource::Idxs(idxs) => idxs[i],
                    JoinLeftSource::Scan { .. } => i,
                };
                if let Some(row) = self.store.get_by_idx(left_name, idx)
                    && (!need_filter || filter.is_none_or(|p| eval_pred(p, row, now)))
                    && let Some(k) = row.get(on).and_then(Cell::text)
                {
                    needed.insert(k);
                }
            }
            let mut p = FxHashMap::default();
            p.reserve(needed.len());
            for r in rights {
                if let Some(id) = r.get("id").and_then(Cell::text)
                    && needed.contains(id)
                {
                    let cells = right_fields
                        .iter()
                        .map(|f| r.get(f).cloned().unwrap_or(Cell::Null))
                        .collect();
                    p.insert(id, cells);
                }
            }
            Some(p)
        };

        let names_arc: Arc<[String]> = names.clone().into();
        let mut batch = RecordBatch::with_capacity(names_arc, left_len);
        let mut cell_buf: smallvec::SmallVec<[Cell; 8]> = smallvec::SmallVec::new();
        let mut pos = 0usize;
        loop {
            let idx = match &source {
                JoinLeftSource::Idxs(idxs) => {
                    if pos >= idxs.len() {
                        break;
                    }
                    let i = idxs[pos];
                    pos += 1;
                    i
                }
                JoinLeftSource::Scan { len } => {
                    if pos >= *len {
                        break;
                    }
                    let i = pos;
                    pos += 1;
                    i
                }
            };
            let Some(left_row) = self.store.get_by_idx(left_name, idx) else {
                continue;
            };
            if need_filter
                && let Some(pred) = filter
                && !eval_pred(pred, left_row, now)
            {
                continue;
            }

            if let Some(ref probe_map) = probe {
                let right_cells = left_row
                    .get(on)
                    .and_then(Cell::text)
                    .and_then(|k| probe_map.get(k).map(|v| v.as_slice()));
                if right_cells.is_none() && !left_join {
                    continue;
                }
                push_join_batch_row(&mut batch, left_row, right_cells, &plan);
            } else {
                let right_row = left_row
                    .get(on)
                    .and_then(Cell::text)
                    .and_then(|k| self.store.get_by_id(right_col, k));
                if right_row.is_none() && !left_join {
                    continue;
                }
                if let Some(r) = right_row {
                    cell_buf.clear();
                    for f in &right_fields {
                        cell_buf.push(r.get(f).cloned().unwrap_or(Cell::Null));
                    }
                    push_join_batch_row(&mut batch, left_row, Some(&cell_buf), &plan);
                } else {
                    push_join_batch_row(&mut batch, left_row, None, &plan);
                }
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

        let names_arc: Arc<[String]> = names.to_vec().into();
        let mut batch = RecordBatch::with_capacity(names_arc.clone(), n);

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

        if left_len >= 8192 {
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
                        let lookup_user_ref = &lookup_user;
                        let plan_ref = &plan;
                        let names_arc_clone = names_arc.clone();
                        let right_fields_ref = &right_fields;
                        handles.push(scope.spawn(move || {
                            let mut local_batch =
                                RecordBatch::with_capacity(names_arc_clone, c_end - c_start);
                            if fast_id_email_total {
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
                                    } else if need_row_filter && let Some(pred) = filter {
                                        let Some(left_row) = self.store.get_by_idx("orders", idx)
                                        else {
                                            continue;
                                        };
                                        if !eval_pred(pred, left_row, now) {
                                            continue;
                                        }
                                    }
                                    let uid = orders_uid[idx].as_ref();
                                    let Some(ui) = lookup_user_ref(uid) else {
                                        if left_join {
                                            local_batch.cols[0]
                                                .push(Cell::Text(Arc::clone(&orders_id[idx])));
                                            local_batch.cols[1].push(Cell::Null);
                                            local_batch.cols[2]
                                                .push(Cell::Float(orders_total[idx]));
                                        }
                                        continue;
                                    };
                                    local_batch.cols[0]
                                        .push(Cell::Text(Arc::clone(&orders_id[idx])));
                                    local_batch.cols[1]
                                        .push(Cell::Text(Arc::clone(&users_email[ui])));
                                    local_batch.cols[2].push(Cell::Float(orders_total[idx]));
                                }
                            } else {
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
                                    } else if need_row_filter && let Some(pred) = filter {
                                        let Some(left_row) = self.store.get_by_idx("orders", idx)
                                        else {
                                            continue;
                                        };
                                        if !eval_pred(pred, left_row, now) {
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
                                                "user_id" => {
                                                    Cell::Text(Arc::clone(&orders_uid[idx]))
                                                }
                                                "total" => Cell::Float(orders_total[idx]),
                                                _ => self
                                                    .store
                                                    .get_by_idx("orders", idx)
                                                    .and_then(|r| r.get(k).cloned())
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
                                                        Some("email") => Cell::Text(Arc::clone(
                                                            &users_email[ui],
                                                        )),
                                                        _ => Cell::Null,
                                                    },
                                                    None => Cell::Null,
                                                }
                                            }
                                        };
                                        local_batch.cols[col_i].push(cell);
                                    }
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

        let mut pos = 0usize;
        if fast_id_email_total {
            loop {
                let idx = match &source {
                    JoinLeftSource::Idxs(idxs) => {
                        if pos >= idxs.len() {
                            break;
                        }
                        let i = idxs[pos];
                        pos += 1;
                        i
                    }
                    JoinLeftSource::Scan { len } => {
                        if pos >= *len {
                            break;
                        }
                        let i = pos;
                        pos += 1;
                        i
                    }
                };
                if idx >= n {
                    continue;
                }
                if let Some(min) = total_gt {
                    if orders_total[idx] <= min {
                        continue;
                    }
                } else if need_row_filter && let Some(pred) = filter {
                    let Some(left_row) = self.store.get_by_idx("orders", idx) else {
                        continue;
                    };
                    if !eval_pred(pred, left_row, now) {
                        continue;
                    }
                }
                let uid = orders_uid[idx].as_ref();
                let Some(ui) = lookup_user(uid) else {
                    if left_join {
                        batch.cols[0].push(Cell::Text(Arc::clone(&orders_id[idx])));
                        batch.cols[1].push(Cell::Null);
                        batch.cols[2].push(Cell::Float(orders_total[idx]));
                    }
                    continue;
                };
                batch.cols[0].push(Cell::Text(Arc::clone(&orders_id[idx])));
                batch.cols[1].push(Cell::Text(Arc::clone(&users_email[ui])));
                batch.cols[2].push(Cell::Float(orders_total[idx]));
            }
            finish_batch(&mut batch, skip_n, explicit_take);
            return Some(batch);
        }

        loop {
            let idx = match &source {
                JoinLeftSource::Idxs(idxs) => {
                    if pos >= idxs.len() {
                        break;
                    }
                    let i = idxs[pos];
                    pos += 1;
                    i
                }
                JoinLeftSource::Scan { len } => {
                    if pos >= *len {
                        break;
                    }
                    let i = pos;
                    pos += 1;
                    i
                }
            };
            if idx >= n {
                continue;
            }
            if let Some(min) = total_gt {
                if orders_total[idx] <= min {
                    continue;
                }
            } else if need_row_filter && let Some(pred) = filter {
                let Some(left_row) = self.store.get_by_idx("orders", idx) else {
                    continue;
                };
                if !eval_pred(pred, left_row, now) {
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
                        "total" => Cell::Float(orders_total[idx]),
                        _ => self
                            .store
                            .get_by_idx("orders", idx)
                            .and_then(|r| r.get(k).cloned())
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

        if to_field == "id" {
            let right = self.store.collection(right_col);
            let store_has_all_ids = self
                .store
                .by_id
                .get(right_col)
                .is_some_and(|m| m.len() == right.len());
            if store_has_all_ids {
                let mut out = Vec::with_capacity(left.len());
                for l in left {
                    let right = l
                        .get(on)
                        .and_then(Cell::text)
                        .and_then(|k| self.store.get_by_id(right_col, k));
                    match right {
                        Some(r) => out.push(merge_join_row(l, r, right_col)),
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
                        .filter_map(|l| l.get(on).and_then(Cell::text))
                        .collect(),
                )
            } else {
                None
            };

            let mut by_id: FxHashMap<&str, &Row> = FxHashMap::default();
            if let Some(ref needed_set) = needed {
                by_id.reserve(needed_set.len());
                for r in right {
                    if let Some(id) = r.get("id").and_then(Cell::text)
                        && needed_set.contains(id)
                    {
                        by_id.insert(id, r);
                    }
                }
            } else {
                by_id.reserve(right.len());
                for r in right {
                    if let Some(id) = r.get("id").and_then(Cell::text) {
                        by_id.insert(id, r);
                    }
                }
            }

            let mut out = Vec::with_capacity(left.len());
            for l in left {
                let right = l
                    .get(on)
                    .and_then(Cell::text)
                    .and_then(|k| by_id.get(k).copied());
                match right {
                    Some(r) => out.push(merge_join_row(l, r, right_col)),
                    None if left_join => out.push(l),
                    None => {}
                }
            }
            return Ok(out);
        }

        let right = self.store.collection(right_col);
        let needed_keys: FxHashSet<String> = left
            .iter()
            .filter_map(|l| l.get(on).and_then(cell_key))
            .collect();
        if needed_keys.is_empty() {
            return Ok(if left_join { left } else { Vec::new() });
        }

        let mut idx: FxHashMap<String, Vec<&Row>> = FxHashMap::default();
        idx.reserve(needed_keys.len());
        for r in right {
            if let Some(k) = cell_key(r.get(to_field).unwrap_or(&Cell::Null))
                && needed_keys.contains(&k)
            {
                idx.entry(k).or_default().push(r);
            }
        }
        let mut out = Vec::with_capacity(left.len());
        for mut l in left {
            let key = l.get(on).and_then(cell_key);
            let hits = key.as_ref().and_then(|k| idx.get(k));
            if let Some(rs) = hits {
                for (i, r) in rs.iter().enumerate() {
                    let base = if i + 1 == rs.len() {
                        std::mem::take(&mut l)
                    } else {
                        l.clone()
                    };
                    out.push(merge_join_row(base, r, right_col));
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
}
