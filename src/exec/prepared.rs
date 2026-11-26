use std::sync::Arc;

use smallvec::SmallVec;

use crate::ast::{CmpOp, Field, Pred, Query, Record, Source, Step, Stmt, Value};
use crate::batch::RecordBatch;
use crate::catalog::Catalog;
use crate::error::Error;
use crate::index::{IndexKey, IndexUse};
use crate::plan::Plan;
use crate::store::{Cell, Row, Store, now_ms, row_text};

use super::eval::{
    bind_stmt_params, eval_pred, point_key, pred_has_param, pred_needs_now, project,
    record_has_param, record_needs_now, value_cell, value_needs_now, value_to_arc_str,
};
use super::{Db, Done, ReadDb, field_names};

#[derive(Debug, Clone)]
pub(crate) struct PointGetPlan {
    pub collection: String,
    pub field: String,
    pub key: String,
    pub fields: Option<Vec<String>>,
    pub projector: Option<crate::store::OwnedRowProjector>,
    pub is_simple_eq: bool,
    pub pred_slots: Option<crate::eval::OwnedPredSlots>,
    pub needs_now: bool,
    pub is_docs_id_title: bool,
    pub is_docs_hot_proj: bool,
    pub prepared_docs_epoch: u64,
    pub prepared_rewrite_epoch: u64,
    pub prepared_idx: Option<usize>,
}

#[derive(Debug, Clone)]
pub(crate) struct ScanProjectPlan {
    pub collection: String,
    pub fields: Option<Vec<String>>,
    pub projector: Option<crate::store::OwnedRowProjector>,
    pub is_docs_hot: bool,
    pub skip: usize,
    pub take: Option<usize>,
}

#[derive(Debug, Clone)]
pub(crate) struct PointDeletePlan {
    pub collection: String,
    pub field: String,
    pub key: String,
    pub cas: Option<String>,
    pub cas_each: bool,
    pub append_only: bool,
    pub prepared_gen: u64,
    pub prepared_docs_epoch: u64,
    pub prepared_idx: Option<usize>,
    pub prepared_kd_kt: Option<(IndexKey, IndexKey)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WingIdxKind {
    /// No index on `"docs"` touches `"wing"` (nor any partial index on `"docs"`).
    None,
    /// Only `index docs[wing]` (non-partial).
    WingOnly,
    /// `index docs[wing, ts]` (non-partial), possibly alongside `index docs[wing]`.
    WingTs,
    /// Partial index or composite index involving `"wing"` other than `[wing]` / `[wing, ts]`.
    Complex,
}

#[derive(Debug, Clone)]
pub(crate) struct PointUpdatePlan {
    pub collection: String,
    pub field: String,
    pub key: String,
    pub cas: Option<String>,
    pub cas_each: bool,
    pub record: Record,
    pub is_docs_wing_only: bool,
    pub const_wing: Option<(Cell, Arc<str>)>,
    pub schema_fast_eligible: bool,
    pub has_unique_idx: bool,
    pub wing_idx_kind: WingIdxKind,
    pub prepared_cat_hash: u64,
    pub prepared_indexes_len: usize,
    pub prepared_docs_epoch: u64,
    pub prepared_rewrite_epoch: u64,
    pub prepared_idx: Option<usize>,
    pub prepared_shape: Option<Arc<crate::store::RowShape>>,
    pub hash_slot: Option<u8>,
    pub wing_slot: Option<u8>,
    pub ts_slot: Option<u8>,
}

#[derive(Debug, Clone)]
pub(crate) enum AggKind {
    Count { by: Option<Field> },
    Sum { field: Field, by: Option<Field> },
    Avg { field: Field, by: Option<Field> },
    Min { field: Field, by: Option<Field> },
    Max { field: Field, by: Option<Field> },
}

#[derive(Debug, Clone)]
pub(crate) struct AggPlan {
    pub collection: String,
    pub filter: Option<Pred>,
    pub pred_slots: Option<crate::eval::OwnedPredSlots>,
    pub value_slot: Option<crate::store::row::OwnedFieldSlot>,
    pub kind: AggKind,
    pub needs_now: bool,
    pub index_plan: Option<(Vec<IndexUse>, bool)>,
    /// Trailing `sort` / `skip` / `take N` steps, applied in order to the groups the plan returns.
    pub tail: Vec<AggTail>,
}

#[derive(Debug, Clone)]
pub(crate) enum AggTail {
    Skip(usize),
    Take(usize),
    Sort { field: Field, desc: bool },
}

#[derive(Debug, Clone)]
pub(crate) struct PreparedAppendFacts {
    pub s: Vec<Arc<str>>,
    pub p: Vec<Arc<str>>,
    pub o: Vec<Arc<str>>,
    pub encoded_spo: Vec<u8>,
}

#[derive(Debug, Clone)]
pub(crate) struct PointGraphPlan {
    pub collection: String,
    pub field: String,
    pub key: String,
    pub edge_rel: String,
    pub rel_cell: Cell,
    pub reverse: bool,
    pub depth: usize,
    pub is_hop: bool,
    pub skip: usize,
    pub limit: Option<usize>,
    pub step_count: usize,
    pub id_slot: crate::store::row::OwnedFieldSlot,
    pub uri_slot: crate::store::row::OwnedFieldSlot,
    pub prepared_docs_epoch: u64,
    pub prepared_rewrite_epoch: u64,
    pub prepared_idx: Option<usize>,
}

/// Cheap-to-clone execution payload of [`Prepared`] (all `Arc` handles + `Copy` flags,
/// without cloning `src: String`) for zero-allocation `plan_cache` hits on `&mut Db` and
/// `&mut Transaction`.
#[derive(Debug, Clone)]
pub(crate) struct ExecPlan {
    pub stmts: Arc<[Stmt]>,
    pub plan: Arc<Plan>,
    pub writes: bool,
    pub append_only: bool,
    pub schema: bool,
    pub point_get: Option<Arc<PointGetPlan>>,
    pub point_graph: Option<Arc<PointGraphPlan>>,
    pub scan_project: Option<Arc<ScanProjectPlan>>,
    pub agg_plan: Option<Arc<AggPlan>>,
    pub point_delete: Option<Arc<PointDeletePlan>>,
    pub point_update: Option<Arc<PointUpdatePlan>>,
    pub append_facts: Option<Arc<PreparedAppendFacts>>,
}

/// Compiled statement: parse + typecheck + plan once, run many times.
#[derive(Debug, Clone)]
pub struct Prepared {
    pub src: String,
    pub(crate) stmts: Arc<[Stmt]>,
    pub plan: Arc<Plan>,
    /// True if any statement may mutate catalog or store.
    pub(crate) writes: bool,
    /// Append-only writes (insert/append) — cheap rollback without full clone.
    pub(crate) append_only: bool,
    /// Schema (col/rel/index) — clears plan cache after success.
    pub(crate) schema: bool,
    /// True if any statement contains `$param` placeholders.
    pub(crate) has_params: bool,
    pub(crate) point_get: Option<Arc<PointGetPlan>>,
    pub(crate) point_graph: Option<Arc<PointGraphPlan>>,
    pub(crate) scan_project: Option<Arc<ScanProjectPlan>>,
    pub(crate) agg_plan: Option<Arc<AggPlan>>,
    pub(crate) point_delete: Option<Arc<PointDeletePlan>>,
    pub(crate) point_update: Option<Arc<PointUpdatePlan>>,
    pub(crate) append_facts: Option<Arc<PreparedAppendFacts>>,
}

impl Prepared {
    #[inline]
    pub(crate) fn exec_plan(&self) -> ExecPlan {
        ExecPlan {
            stmts: Arc::clone(&self.stmts),
            plan: Arc::clone(&self.plan),
            writes: self.writes,
            append_only: self.append_only,
            schema: self.schema,
            point_get: self.point_get.as_ref().map(Arc::clone),
            point_graph: self.point_graph.as_ref().map(Arc::clone),
            scan_project: self.scan_project.as_ref().map(Arc::clone),
            agg_plan: self.agg_plan.as_ref().map(Arc::clone),
            point_delete: self.point_delete.as_ref().map(Arc::clone),
            point_update: self.point_update.as_ref().map(Arc::clone),
            append_facts: self.append_facts.as_ref().map(Arc::clone),
        }
    }

    #[inline]
    pub fn run(&self, db: &mut Db) -> Result<super::Handle, Error> {
        db.run_prepared(self)
    }

    pub fn run_read(&self, db: &ReadDb) -> Result<super::Handle, Error> {
        db.run_prepared(self)
    }

    /// Execute with bound `$param` values without reparsing or replanning.
    pub fn run_with(&self, db: &mut Db, params: &[(&str, Cell)]) -> Result<super::Handle, Error> {
        if !self.has_params || params.is_empty() {
            return db.run_prepared(self);
        }
        let bound: SmallVec<[Stmt; 2]> = self
            .stmts
            .iter()
            .map(|s| bind_stmt_params(s, params))
            .collect();
        db.run_bound_stmts(
            &bound,
            &self.plan,
            self.writes,
            self.append_only,
            self.schema,
            true,
        )
    }

    /// Execute with bound `$param` values on a read-only snapshot.
    pub fn run_read_with(
        &self,
        db: &ReadDb,
        params: &[(&str, Cell)],
    ) -> Result<super::Handle, Error> {
        if !self.has_params || params.is_empty() {
            return db.run_prepared(self);
        }
        if self.writes {
            return Err(Error::runtime("read-only snapshot: writes not allowed"));
        }
        let bound: SmallVec<[Stmt; 2]> = self
            .stmts
            .iter()
            .map(|s| bind_stmt_params(s, params))
            .collect();
        db.as_db().run_bound_stmts_readonly(&bound, &self.plan)
    }

    /// Columnar OLAP path when the statement is a supported batch query (FK join+project).
    /// Falls back to row materialization for other statements.
    pub fn run_batch(&self, db: &mut Db) -> Result<RecordBatch, Error> {
        db.run_prepared_batch(self)
    }

    pub fn run_batch_read(&self, db: &ReadDb) -> Result<RecordBatch, Error> {
        db.run_prepared_batch(self)
    }

    pub fn execute(&self, db: &mut Db) -> Result<Done, Error> {
        db.execute_prepared(self)
    }

    pub fn execute_with(&self, db: &mut Db, params: &[(&str, Cell)]) -> Result<Done, Error> {
        if !self.has_params || params.is_empty() {
            return db.execute_prepared(self);
        }
        let bound: SmallVec<[Stmt; 2]> = self
            .stmts
            .iter()
            .map(|s| bind_stmt_params(s, params))
            .collect();
        Ok(db
            .run_bound_stmts(
                &bound,
                &self.plan,
                self.writes,
                self.append_only,
                self.schema,
                false,
            )?
            .done)
    }

    pub fn scalar(&self, db: &mut Db) -> Result<Cell, Error> {
        self.run(db)?.scalar()
    }

    pub fn scalar_with(&self, db: &mut Db, params: &[(&str, Cell)]) -> Result<Cell, Error> {
        self.run_with(db, params)?.scalar()
    }

    pub fn scalar_read(&self, db: &ReadDb) -> Result<Cell, Error> {
        self.run_read(db)?.scalar()
    }
}

pub(crate) fn stmt_has_param(stmt: &Stmt) -> bool {
    let val_has = |v: &Value| matches!(v, Value::Param(_));
    let query_has = |q: &Query| {
        q.steps.iter().any(|step| match step {
            Step::Filter(p) => pred_has_param(p),
            _ => false,
        })
    };
    match stmt {
        Stmt::Query(q) | Stmt::Let { query: q, .. } | Stmt::IdbSlice { query: q } => query_has(q),
        Stmt::Insert { records, edges, .. } => {
            records.iter().any(record_has_param)
                || edges.iter().any(|e| match &e.target {
                    crate::ast::EdgeTarget::Value(v) => val_has(v),
                    crate::ast::EdgeTarget::Page(_) => false,
                })
        }
        Stmt::AppendFacts { records } => records.iter().any(record_has_param),
        Stmt::AppendEdges { edges } => edges.iter().any(|e| val_has(&e.from) || val_has(&e.to)),
        Stmt::Update { pred, record, .. } => {
            pred.as_ref().is_some_and(pred_has_param) || record_has_param(record)
        }
        Stmt::Delete { pred, .. } => pred_has_param(pred),
        Stmt::DeleteEdge { from, to, .. } => val_has(from) || val_has(to),
        _ => false,
    }
}

pub(crate) fn classify_docs_wing_indexes(
    indexes: &std::collections::BTreeMap<String, crate::index::LiveIndex>,
    collection: &str,
) -> (bool, WingIdxKind) {
    let mut has_unique = false;
    let mut has_wing_only = false;
    let mut has_wing_ts = false;
    let mut has_complex = false;
    for live_idx in indexes.values() {
        if live_idx.def.collection == collection {
            if live_idx.def.unique {
                has_unique = true;
            }
            if live_idx.def.pred.is_none()
                && live_idx.def.fields.len() == 1
                && live_idx.def.fields[0] == "wing"
            {
                has_wing_only = true;
            } else if live_idx.def.pred.is_none()
                && live_idx.def.fields.len() == 2
                && live_idx.def.fields[0] == "wing"
                && live_idx.def.fields[1] == "ts"
            {
                has_wing_ts = true;
            } else if live_idx.def.pred.is_some()
                || live_idx.def.fields.iter().any(|f| f == "wing")
            {
                has_complex = true;
            }
        }
    }
    let kind = if has_complex {
        WingIdxKind::Complex
    } else if has_wing_ts {
        WingIdxKind::WingTs
    } else if has_wing_only {
        WingIdxKind::WingOnly
    } else {
        WingIdxKind::None
    };
    (has_unique, kind)
}

impl Db {
    pub(crate) fn try_extract_append_facts(stmt: &Stmt) -> Option<PreparedAppendFacts> {
        let Stmt::AppendFacts { records } = stmt else {
            return None;
        };
        if records.is_empty()
            || records.iter().any(record_needs_now)
            || records
                .iter()
                .any(|r| r.fields.iter().any(|(k, _)| !matches!(k.as_str(), "s" | "p" | "o")))
        {
            return None;
        }
        static EMPTY_ARC: std::sync::LazyLock<Arc<str>> =
            std::sync::LazyLock::new(|| Arc::from(""));
        let n = records.len();
        let mut s = Vec::with_capacity(n);
        let mut p = Vec::with_capacity(n);
        let mut o = Vec::with_capacity(n);
        let mut encoded_spo = Vec::with_capacity(n * 48);
        let mut last_s_val: Option<&Value> = None;
        let mut last_s_arc: Option<Arc<str>> = None;
        let mut last_p_val: Option<&Value> = None;
        let mut last_p_arc: Option<Arc<str>> = None;
        for record in records {
            if record.fields.len() == 3
                && record.fields[0].0 == "s"
                && record.fields[1].0 == "p"
                && record.fields[2].0 == "o"
            {
                let sv = &record.fields[0].1;
                let sa = if last_s_val == Some(sv)
                    && let Some(cached) = &last_s_arc
                {
                    Arc::clone(cached)
                } else {
                    let arc = value_to_arc_str(sv, 0);
                    last_s_arc = Some(Arc::clone(&arc));
                    last_s_val = Some(sv);
                    arc
                };
                let pv = &record.fields[1].1;
                let pa = if last_p_val == Some(pv)
                    && let Some(cached) = &last_p_arc
                {
                    Arc::clone(cached)
                } else {
                    let arc = value_to_arc_str(pv, 0);
                    last_p_arc = Some(Arc::clone(&arc));
                    last_p_val = Some(pv);
                    arc
                };
                let oa = value_to_arc_str(&record.fields[2].1, 0);
                crate::persist::put_str(&mut encoded_spo, &sa);
                crate::persist::put_str(&mut encoded_spo, &pa);
                crate::persist::put_str(&mut encoded_spo, &oa);
                s.push(sa);
                p.push(pa);
                o.push(oa);
            } else {
                let mut sr = None;
                let mut pr = None;
                let mut or = None;
                for (k, v) in &record.fields {
                    match k.as_str() {
                        "s" => sr = Some(value_to_arc_str(v, 0)),
                        "p" => pr = Some(value_to_arc_str(v, 0)),
                        "o" => or = Some(value_to_arc_str(v, 0)),
                        _ => {}
                    }
                }
                let sa = sr.unwrap_or_else(|| Arc::clone(&EMPTY_ARC));
                let pa = pr.unwrap_or_else(|| Arc::clone(&EMPTY_ARC));
                let oa = or.unwrap_or_else(|| Arc::clone(&EMPTY_ARC));
                crate::persist::put_str(&mut encoded_spo, &sa);
                crate::persist::put_str(&mut encoded_spo, &pa);
                crate::persist::put_str(&mut encoded_spo, &oa);
                s.push(sa);
                p.push(pa);
                o.push(oa);
            }
        }
        Some(PreparedAppendFacts {
            s,
            p,
            o,
            encoded_spo,
        })
    }

    pub(crate) fn try_extract_point_graph(
        q: &Query,
        cat: &Catalog,
        store: &Store,
    ) -> Option<PointGraphPlan> {
        if q.explain.is_some() {
            return None;
        }
        let Source::Collection(collection) = &q.source else {
            return None;
        };
        if !q.ignore_filter && cat.collection(collection).is_some_and(|c| c.filter.is_some()) {
            return None;
        }
        let Some(Step::Filter(first_pred)) = q.steps.first() else {
            return None;
        };
        if pred_has_param(first_pred) {
            return None;
        }
        if !matches!(
            first_pred,
            Pred::Cmp {
                op: CmpOp::Eq,
                value: Value::String(_),
                ..
            }
        ) {
            return None;
        }
        let (field, key) = point_key(first_pred)?;
        if field != "id" && !(field == "uri" && collection == "docs") {
            return None;
        }
        let (rel, depth, is_hop) = match q.steps.get(1)? {
            Step::Graph { rel, depth } => (rel.as_str(), depth.unwrap_or(1).clamp(1, 3) as usize, false),
            Step::Hop { rel, depth } => (rel.as_str(), depth.unwrap_or(1).clamp(1, 3) as usize, true),
            _ => return None,
        };
        let mut skip = 0usize;
        let mut take_step: Option<Option<i64>> = None;
        let mut saw_take = false;
        for step in &q.steps[2..] {
            match step {
                Step::Skip { n } => {
                    if saw_take {
                        return None;
                    }
                    skip = skip.saturating_add((*n).max(0) as usize);
                }
                Step::Take { n } => {
                    if saw_take {
                        return None;
                    }
                    saw_take = true;
                    take_step = Some(*n);
                }
                _ => return None,
            }
        }
        let limit = match take_step {
            Some(Some(n)) => Some(n.max(0) as usize),
            Some(None) => None,
            None => Some(50),
        };
        let (edge_rel, reverse) = match cat.rel(rel) {
            Some(r) if let Some(of) = &r.reverse_of => (of.as_str(), true),
            _ => (rel, false),
        };
        let prepared_idx = if !store.is_disk_backed(collection) {
            store.point_index(collection, field, key)
        } else {
            None
        };
        Some(PointGraphPlan {
            collection: collection.clone(),
            field: field.to_string(),
            key: key.to_string(),
            edge_rel: edge_rel.to_string(),
            rel_cell: Cell::from_text(edge_rel),
            reverse,
            depth,
            is_hop,
            skip,
            limit,
            step_count: q.steps.len(),
            id_slot: crate::store::row::OwnedFieldSlot::new("id"),
            uri_slot: crate::store::row::OwnedFieldSlot::new("uri"),
            prepared_docs_epoch: store.docs_epoch,
            prepared_rewrite_epoch: store.collections.rewrite_epoch(),
            prepared_idx,
        })
    }

    pub(crate) fn execute_point_graph(
        &self,
        pg: &PointGraphPlan,
        plan: &Arc<Plan>,
        want_rows: bool,
    ) -> Option<Result<super::Handle, Error>> {
        #[cfg(feature = "gpu")]
        if self.gpu.is_some() {
            return None;
        }
        if self.store.is_disk_backed(&pg.collection)
            || (pg.is_hop && self.store.is_disk_backed("docs"))
            || ((pg.collection == "docs" || pg.is_hop) && self.store.has_deferred_doc_ops())
            || (pg.collection == "facts" && self.store.facts_rows_stale)
        {
            return None;
        }
        let col = self.store.collection(&pg.collection);
        let idx_opt = if let Some(i) = pg.prepared_idx
            && i < col.len()
            && self.store.fts_deferred_doc_swaps.is_empty()
            && ((self.store.collections.rewrite_epoch() == pg.prepared_rewrite_epoch
                && (pg.collection != "docs" || self.store.docs_epoch == pg.prepared_docs_epoch))
                || (pg.collection == "docs"
                    && pg.field == "id"
                    && self
                        .store
                        .docs_id
                        .get(i)
                        .is_some_and(|id| id.as_ref() == pg.key.as_str())))
        {
            Some(i)
        } else {
            self.store
                .point_index(&pg.collection, &pg.field, &pg.key)
                .filter(|&i| i < col.len())
        };

        let Some(seed_idx) = idx_opt else {
            return Some(Ok(super::Handle {
                plan: Arc::clone(plan),
                rows: Vec::new(),
                done: Done {
                    r#gen: self.store.r#gen,
                    n: 0,
                },
                message: None,
                ms: 0.0,
            }));
        };

        let mut work = 3usize;
        if work > self.quotas.max_execution_work {
            return Some(Err(Error::runtime(format!(
                "query execution work {} exceeds max_execution_work quota {}",
                work, self.quotas.max_execution_work
            ))));
        }

        let seed_row = &col[seed_idx];
        let mut seeds: SmallVec<[&str; 2]> = SmallVec::new();
        if let Some(id_s) = pg.id_slot.get_sync(seed_row).and_then(Cell::text) {
            seeds.push(id_s);
        }
        if let Some(uri_s) = pg.uri_slot.get_sync(seed_row).and_then(Cell::text)
            && !seeds.contains(&uri_s)
        {
            seeds.push(uri_s);
        }

        let walk_res = if pg.is_hop {
            self.hop_seeds(
                &pg.collection,
                &seeds,
                &pg.edge_rel,
                pg.reverse,
                pg.depth as i64,
            )
        } else {
            self.graph_edges_seeds(
                &seeds,
                &pg.edge_rel,
                &pg.rel_cell,
                pg.reverse,
                pg.depth as i64,
            )
        };
        let mut rows = match walk_res {
            Ok(r) => r,
            Err(e) => return Some(Err(e)),
        };

        for _ in 2..pg.step_count {
            work = work.saturating_add(rows.len());
            if work > self.quotas.max_execution_work {
                return Some(Err(Error::runtime(format!(
                    "query execution work {} exceeds max_execution_work quota {}",
                    work, self.quotas.max_execution_work
                ))));
            }
        }

        if pg.skip >= rows.len() {
            rows.clear();
        } else if pg.skip > 0 {
            rows.drain(0..pg.skip);
        }
        if let Some(lim) = pg.limit
            && rows.len() > lim
        {
            rows.truncate(lim);
        }

        if !rows.is_empty()
            && let Err(e) = self.check_output_quota(&rows)
        {
            return Some(Err(e));
        }

        let n = rows.len();
        let out_rows = if want_rows { rows } else { Vec::new() };
        Some(Ok(super::Handle {
            plan: Arc::clone(plan),
            rows: out_rows,
            done: Done {
                r#gen: self.store.r#gen,
                n,
            },
            message: None,
            ms: 0.0,
        }))
    }

    pub(crate) fn try_extract_scan_project(
        q: &Query,
        cat: &Catalog,
    ) -> Option<ScanProjectPlan> {
        let Source::Collection(collection) = &q.source else {
            return None;
        };
        if !q.ignore_filter && cat.collection(collection).is_some_and(|c| c.filter.is_some()) {
            return None;
        }
        let mut fields: Option<Vec<String>> = None;
        let mut skip: usize = 0;
        let mut take_step: Option<Option<i64>> = None;
        let mut saw_take = false;
        for step in &q.steps {
            match step {
                Step::Project(f) => {
                    if fields.is_some() || f.iter().any(|fld| fld.leaf().is_none()) {
                        return None;
                    }
                    fields = Some(field_names(f));
                }
                Step::Skip { n } => {
                    if saw_take {
                        return None;
                    }
                    skip = skip.saturating_add((*n).max(0) as usize);
                }
                Step::Take { n } => {
                    if saw_take {
                        return None;
                    }
                    saw_take = true;
                    take_step = Some(*n);
                }
                _ => return None,
            }
        }
        let take = match take_step {
            Some(Some(n)) => Some(n.max(0) as usize),
            Some(None) => None,
            None => Some(50),
        };
        let is_docs_hot = collection == "docs"
            && fields.as_ref().is_some_and(|fs| {
                !fs.is_empty()
                    && fs
                        .iter()
                        .all(|f| matches!(f.as_str(), "id" | "title" | "layer" | "wing"))
            });
        let projector = fields
            .as_deref()
            .map(crate::store::OwnedRowProjector::new);
        Some(ScanProjectPlan {
            collection: collection.clone(),
            fields,
            projector,
            is_docs_hot,
            skip,
            take,
        })
    }

    pub(crate) fn try_extract_point_get(
        q: &Query,
        cat: &Catalog,
        store: &Store,
    ) -> Option<PointGetPlan> {
        if !matches!(q.source, Source::Collection(_)) {
            return None;
        }
        if !matches!(q.steps.first(), Some(Step::Filter(_))) {
            return None;
        }
        if q.steps.iter().any(|s| {
            !matches!(
                s,
                Step::Filter(_) | Step::Project(_) | Step::Take { .. } | Step::Skip { .. }
            )
        }) {
            return None;
        }
        let mut q = crate::catalog::with_catalog_filter(q, cat);
        if q.needs_simplify() {
            q.to_mut().simplify_predicates();
        }
        let q = &*q;
        let name = match &q.source {
            Source::Collection(n) => n.clone(),
            _ => return None,
        };
        let Some(Step::Filter(first_pred)) = q.steps.first() else {
            return None;
        };
        if pred_has_param(first_pred) {
            return None;
        }
        let (field, key) = point_key(first_pred)?;
        if field == "uri" && name != "docs" {
            return None;
        }
        let mut filter: Option<&Pred> = None;
        let mut fields: Option<Vec<String>> = None;
        for step in &q.steps {
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
                    fields = Some(field_names(f));
                }
                Step::Take { n: Some(n) } if *n <= 0 => return None,
                Step::Skip { n } if *n > 0 => return None,
                Step::Take { .. } | Step::Skip { .. } => {}
                _ => return None,
            }
        }
        let pred = filter?;
        let simple_eq = matches!(
            pred,
            Pred::Cmp {
                op: CmpOp::Eq,
                value: Value::String(_),
                ..
            }
        );
        let is_docs_id_title = simple_eq
            && name == "docs"
            && field == "id"
            && fields
                .as_deref()
                .map(|fs| fs.len() == 2 && fs[0] == "id" && fs[1] == "title")
                .unwrap_or(false);
        let is_docs_hot_proj = name == "docs"
            && fields.as_ref().is_some_and(|fs| {
                !fs.is_empty()
                    && fs
                        .iter()
                        .all(|f| matches!(f.as_str(), "id" | "title" | "layer" | "wing"))
            });
        let needs_now = !simple_eq && pred_needs_now(pred);
        let prepared_idx = if (field == "id" || (field == "uri" && name == "docs"))
            && !store.is_disk_backed(&name)
        {
            store.point_index(&name, field, key)
        } else {
            None
        };
        let prepared_docs_epoch = store.docs_epoch;
        let prepared_rewrite_epoch = store.collections.rewrite_epoch();
        let projector = fields
            .as_deref()
            .map(crate::store::OwnedRowProjector::new);
        let pred_slots = (!simple_eq).then(|| crate::eval::OwnedPredSlots::new(pred));
        Some(PointGetPlan {
            collection: name,
            field: field.to_string(),
            key: key.to_string(),
            fields,
            projector,
            is_simple_eq: simple_eq,
            pred_slots,
            needs_now,
            is_docs_id_title,
            is_docs_hot_proj,
            prepared_docs_epoch,
            prepared_rewrite_epoch,
            prepared_idx,
        })
    }

    pub(crate) fn try_extract_point_delete(
        stmt: &Stmt,
        catalog: &Catalog,
        store: &Store,
    ) -> Option<PointDeletePlan> {
        let Stmt::Delete {
            collection,
            pred,
            cas,
            cas_each,
        } = stmt
        else {
            return None;
        };
        // Deletes from a foreign-key target take the general path, which checks that no
        // row still references what is deleted.
        if catalog.fks.iter().any(|fk| fk.to_col == *collection) {
            return None;
        }
        if pred_has_param(pred) {
            return None;
        }
        let (field, key) = point_key(pred)?;
        if field != "id" && (field != "uri" || collection != "docs") {
            return None;
        }
        if !matches!(pred, Pred::Cmp { .. }) {
            return None;
        }
        let append_only = catalog
            .collection(collection)
            .is_some_and(|c| c.append_only);
        let mut prepared_idx = None;
        let mut prepared_kd_kt = None;
        if collection == "docs" && !store.is_disk_backed("docs") {
            let docs = store.collection("docs");
            if docs.len() >= 64
                && let Some(idx) = store.point_index("docs", field, key)
                && idx < docs.len()
            {
                let cas_ok = if append_only || *cas_each {
                    true
                } else if let Some(c) = cas {
                    row_text(&docs[idx], "hash") == Some(c.as_str())
                } else {
                    false
                };
                if cas_ok {
                    prepared_idx = Some(idx);
                    let last = docs.len() - 1;
                    if idx != last
                        && store.indexes.len() == 1
                        && let Some(live_idx) = store.indexes.values().next()
                        && live_idx.def.collection == "docs"
                        && live_idx.def.pred.is_none()
                    {
                        let kd = live_idx.key_of(&docs[idx]);
                        let kt = live_idx.key_of(&docs[last]);
                        prepared_kd_kt = Some((kd, kt));
                    }
                }
            }
        }
        Some(PointDeletePlan {
            collection: collection.clone(),
            field: field.to_string(),
            key: key.to_string(),
            cas: cas.clone(),
            cas_each: *cas_each,
            append_only,
            prepared_gen: store.r#gen,
            prepared_docs_epoch: store.docs_epoch,
            prepared_idx,
            prepared_kd_kt,
        })
    }

    pub(crate) fn try_extract_point_update(
        stmt: &Stmt,
        catalog: &Catalog,
        store: &Store,
    ) -> Option<PointUpdatePlan> {
        let Stmt::Update {
            collection,
            pred: Some(pred),
            cas,
            cas_each,
            record,
        } = stmt
        else {
            return None;
        };
        if pred_has_param(pred) || record_has_param(record) {
            return None;
        }
        let (field, key) = point_key(pred)?;
        if field != "id" && (field != "uri" || collection != "docs") {
            return None;
        }
        if !matches!(pred, Pred::Cmp { .. }) {
            return None;
        }
        let is_docs_wing_only =
            collection == "docs" && record.fields.len() == 1 && record.fields[0].0 == "wing";
        let const_wing = if is_docs_wing_only && !value_needs_now(&record.fields[0].1) {
            let cell = value_cell(&record.fields[0].1, 0);
            let arc = cell.text_shared().unwrap_or_default();
            Some((cell, arc))
        } else {
            None
        };
        let schema_fast_eligible = is_docs_wing_only
            && (cas.is_some() || *cas_each)
            && !catalog
                .guards
                .iter()
                .any(|g| g.collection == "docs" && g.field == "wing")
            && !catalog.fks.iter().any(|fk| fk.from_col == "docs")
            && !store
                .fts
                .get("docs")
                .is_some_and(|f| f.fields.iter().any(|fld| fld == "wing"));
        let (has_unique_idx, wing_idx_kind) = if is_docs_wing_only {
            classify_docs_wing_indexes(&store.indexes, collection)
        } else {
            (false, WingIdxKind::None)
        };
        let mut prepared_idx = None;
        let mut prepared_shape = None;
        let mut hash_slot = None;
        let mut wing_slot = None;
        let mut ts_slot = None;
        if collection == "docs" && !store.is_disk_backed("docs") {
            let docs = store.collection("docs");
            if let Some(idx) = store.point_index("docs", field, key)
                && idx < docs.len()
            {
                prepared_idx = Some(idx);
                if is_docs_wing_only && let Some(shape) = docs[idx].shape_arc() {
                    let names = shape.names();
                    let find_slot = |target: &str| -> Option<u8> {
                        names
                            .iter()
                            .position(|n| n == target)
                            .and_then(|pos| u8::try_from(pos).ok())
                    };
                    hash_slot = find_slot("hash");
                    wing_slot = find_slot("wing");
                    ts_slot = find_slot("ts");
                    prepared_shape = Some(Arc::clone(shape));
                }
            }
        }
        Some(PointUpdatePlan {
            collection: collection.clone(),
            field: field.to_string(),
            key: key.to_string(),
            cas: cas.clone(),
            cas_each: *cas_each,
            record: record.clone(),
            is_docs_wing_only,
            const_wing,
            schema_fast_eligible,
            has_unique_idx,
            wing_idx_kind,
            prepared_cat_hash: catalog.hash_u64(),
            prepared_indexes_len: store.indexes.len(),
            prepared_docs_epoch: store.docs_epoch,
            prepared_rewrite_epoch: store.collections.rewrite_epoch(),
            prepared_idx,
            prepared_shape,
            hash_slot,
            wing_slot,
            ts_slot,
        })
    }

    #[inline]
    pub(crate) fn execute_point_get(&self, pg: &PointGetPlan) -> Option<Row> {
        if pg.is_docs_id_title {
            if self.store.disk.is_none() && self.store.fts_deferred_doc_swaps.is_empty() {
                let idx = if let Some(i) = pg.prepared_idx
                    && i < self.store.docs_id.len()
                    && (self.store.docs_epoch == pg.prepared_docs_epoch
                        || self.store.docs_id[i].as_ref() == pg.key.as_str())
                {
                    i
                } else {
                    self.store.row_index("docs", &pg.key)?
                };
                if idx < self.store.docs_id.len() && idx < self.store.docs_title.len() {
                    return Some(Row::from_parts(
                        Arc::clone(crate::store::shape_id_title()),
                        [
                            Cell::Text(Arc::clone(&self.store.docs_id[idx])),
                            Cell::Text(Arc::clone(&self.store.docs_title[idx])),
                        ],
                    ));
                }
            }
            return self.store.point_get_docs_id_title(&pg.key);
        }

        if !self.store.is_disk_backed(&pg.collection)
            && !(pg.collection == "facts" && self.store.facts_rows_stale)
        {
            let idx = if let Some(i) = pg.prepared_idx
                && self.store.fts_deferred_doc_swaps.is_empty()
                && ((self.store.collections.rewrite_epoch() == pg.prepared_rewrite_epoch
                    && (pg.collection != "docs"
                        || self.store.docs_epoch == pg.prepared_docs_epoch))
                    || (pg.collection == "docs"
                        && pg.field == "id"
                        && self
                            .store
                            .docs_id
                            .get(i)
                            .is_some_and(|id| id.as_ref() == pg.key.as_str())))
            {
                i
            } else {
                self.store
                    .point_index(&pg.collection, &pg.field, &pg.key)?
            };
            if pg.is_simple_eq
                && pg.is_docs_hot_proj
                && self.store.fts_deferred_doc_swaps.is_empty()
                && self.store.deferred_doc_wing_updates.is_empty()
                && let Some(ref fs) = pg.fields
                && let Some(r) = self.store.project_docs_hot_idx(idx, fs)
            {
                return Some(r);
            }
            let row = self.store.get_by_idx(&pg.collection, idx)?;
            if !pg.is_simple_eq {
                let now = if pg.needs_now { now_ms() } else { 0 };
                if let Some(slots) = &pg.pred_slots
                    && !slots.eval_sync(row, now)
                {
                    return None;
                }
            }
            return Some(match &pg.projector {
                Some(proj) => proj.project_sync(row),
                None => row.clone(),
            });
        }

        if pg.is_simple_eq {
            self.store
                .project_by_key(&pg.collection, &pg.field, &pg.key, pg.fields.as_deref())
        } else {
            let full = self
                .store
                .project_by_key(&pg.collection, &pg.field, &pg.key, None)?;
            let now = if pg.needs_now { now_ms() } else { 0 };
            if let Some(slots) = &pg.pred_slots
                && !slots.eval_sync(&full, now)
            {
                return None;
            }
            if let Some(proj) = &pg.projector {
                Some(proj.project_sync(&full))
            } else {
                Some(full)
            }
        }
    }

    pub(crate) fn execute_scan_project(
        &self,
        sp: &ScanProjectPlan,
        want_rows: bool,
    ) -> Result<(Vec<Row>, usize), Error> {
        if sp.take == Some(0) {
            return Ok((Vec::new(), 0));
        }
        if self.store.is_disk_backed(&sp.collection)
            && let Some(eng) = self.store.disk_engine_read()
        {
            let proj_buf: SmallVec<[&str; 8]>;
            let proj_arg = match &sp.fields {
                Some(fs) => {
                    proj_buf = fs.iter().map(String::as_str).collect();
                    Some(proj_buf.as_slice())
                }
                None => None,
            };
            let mut skipped = 0usize;
            let mut count = 0usize;
            let cap = if want_rows {
                sp.take
                    .unwrap_or_else(|| self.store.get_col_count(&sp.collection).saturating_sub(sp.skip))
                    .min(self.store.get_col_count(&sp.collection))
            } else {
                0
            };
            let mut out = Vec::with_capacity(cap);
            eng.scan_projected(&sp.collection, proj_arg, |r| {
                if skipped < sp.skip {
                    skipped += 1;
                    return Ok(true);
                }
                if want_rows {
                    out.push(match &sp.projector {
                        Some(p) => p.project_sync(r),
                        None => r.clone(),
                    });
                }
                count += 1;
                if let Some(t) = sp.take
                    && count >= t
                {
                    return Ok(false);
                }
                Ok(true)
            })?;
            if !out.is_empty() {
                self.check_output_quota(&out)?;
            }
            return Ok((out, count));
        }

        let col = self.store.collection(&sp.collection);
        let end = match sp.take {
            Some(t) => sp.skip.saturating_add(t).min(col.len()),
            None => col.len(),
        };
        if end > self.quotas.max_execution_work {
            return Err(Error::runtime(format!(
                "query execution work {} exceeds max_execution_work quota {}",
                end, self.quotas.max_execution_work
            )));
        }
        if sp.skip >= end {
            return Ok((Vec::new(), 0));
        }
        let count = end - sp.skip;
        if !want_rows {
            return Ok((Vec::new(), count));
        }

        let out = if sp.is_docs_hot
            && self.store.fts_deferred_doc_swaps.is_empty()
            && self.store.deferred_doc_wing_updates.is_empty()
            && let Some(ref fs) = sp.fields
        {
            if count < 16384
                && fs.len() == 2
                && fs[0] == "id"
                && fs[1] == "title"
                && end <= self.store.docs_id.len()
                && end <= self.store.docs_title.len()
            {
                let shape = crate::store::shape_id_title();
                let mut rows = Vec::with_capacity(count);
                for i in sp.skip..end {
                    rows.push(Row::from_parts(
                        Arc::clone(shape),
                        [
                            Cell::Text(Arc::clone(&self.store.docs_id[i])),
                            Cell::Text(Arc::clone(&self.store.docs_title[i])),
                        ],
                    ));
                }
                rows
            } else if count <= 64 {
                let idxs: SmallVec<[usize; 64]> = (sp.skip..end).collect();
                self.store.project_docs_hot(&idxs, fs).unwrap_or_else(|| {
                    let proj = sp.projector.as_ref().unwrap();
                    col[sp.skip..end].iter().map(|r| proj.project_sync(r)).collect()
                })
            } else {
                let idxs: Vec<usize> = (sp.skip..end).collect();
                self.store.project_docs_hot(&idxs, fs).unwrap_or_else(|| {
                    let proj = sp.projector.as_ref().unwrap();
                    col[sp.skip..end].iter().map(|r| proj.project_sync(r)).collect()
                })
            }
        } else if let Some(ref proj) = sp.projector {
            let mut rows = Vec::with_capacity(count);
            for r in &col[sp.skip..end] {
                rows.push(proj.project_sync(r));
            }
            rows
        } else {
            col[sp.skip..end].to_vec()
        };

        self.check_output_quota(&out)?;
        Ok((out, count))
    }

    /// `docs | id == "…" | { … }` — direct map hit.
    pub(crate) fn try_point_get(&self, q: &Query) -> Option<Vec<Row>> {
        let name = match &q.source {
            Source::Collection(n) => n.as_str(),
            Source::Page(uri) => {
                if q.steps.iter().any(|step| {
                    !matches!(
                        step,
                        Step::Filter(_) | Step::Project(_) | Step::Skip { .. } | Step::Take { .. }
                    )
                }) {
                    return None;
                }
                let mut rows: Vec<Row> = self
                    .store
                    .project_by_key("docs", "uri", uri, None)
                    .into_iter()
                    .collect();
                let now = if !rows.is_empty()
                    && q.steps
                        .iter()
                        .any(|s| matches!(s, Step::Filter(p) if pred_needs_now(p)))
                {
                    now_ms()
                } else {
                    0
                };
                for step in &q.steps {
                    match step {
                        Step::Filter(pred) => rows.retain(|row| eval_pred(pred, row, now)),
                        Step::Project(fields) => rows = project(&rows, fields),
                        Step::Skip { n } if *n > 0 => rows.clear(),
                        Step::Take { n: Some(n) } if *n <= 0 => rows.clear(),
                        _ => {}
                    }
                }
                return Some(rows);
            }
            _ => return None,
        };
        let Step::Filter(first_pred) = q.steps.first()? else {
            return None;
        };
        let (field, key) = point_key(first_pred)?;
        if field == "uri" && name != "docs" {
            return None;
        }
        let mut seen_filter = false;
        let mut fields: Option<Vec<String>> = None;
        let mut empty_limit = false;
        for step in &q.steps {
            match step {
                Step::Filter(_) => {
                    if seen_filter {
                        return None;
                    }
                    seen_filter = true;
                }
                Step::Project(f) => {
                    if fields.is_some() {
                        return None;
                    }
                    fields = Some(field_names(f));
                }
                Step::Take { n: Some(n) } => {
                    if *n <= 0 {
                        empty_limit = true;
                    }
                }
                Step::Skip { n } => {
                    if *n > 0 {
                        empty_limit = true;
                    }
                }
                Step::Take { n: None } => {}
                _ => return None,
            }
        }
        if empty_limit {
            return Some(Vec::new());
        }

        let simple_eq = matches!(
            first_pred,
            Pred::Cmp {
                op: CmpOp::Eq,
                value: Value::String(_),
                ..
            }
        );
        if simple_eq {
            let row = match self
                .store
                .project_by_key(name, field, key, fields.as_deref())
            {
                Some(r) => vec![r],
                None => Vec::new(),
            };
            return Some(row);
        }
        let now = if pred_needs_now(first_pred) {
            now_ms()
        } else {
            0
        };
        let Some(full) = self.store.project_by_key(name, field, key, None) else {
            return Some(Vec::new());
        };
        if !eval_pred(first_pred, &full, now) {
            return Some(Vec::new());
        }
        Some(vec![
            self.store
                .project_by_key(name, field, key, fields.as_deref())
                .unwrap_or(full),
        ])
    }
}
