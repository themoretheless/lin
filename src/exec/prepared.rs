use std::sync::Arc;

use crate::ast::{CmpOp, Field, Pred, Query, Record, Source, Step, Stmt, Value};
use crate::batch::RecordBatch;
use crate::catalog::Catalog;
use crate::error::Error;
use crate::index::{IndexKey, IndexUse};
use crate::plan::Plan;
use crate::store::{Cell, Row, Store, now_ms, row_text};

use super::eval::{
    bind_stmt_params, eval_pred, point_key, pred_has_param, pred_needs_now, project,
    record_has_param, value_cell, value_needs_now,
};
use super::{Db, Done, ReadDb, field_names};

#[derive(Debug, Clone)]
pub(crate) struct PointGetPlan {
    pub collection: String,
    pub field: String,
    pub key: String,
    pub fields: Option<Vec<String>>,
    pub is_simple_eq: bool,
    pub pred: Option<Pred>,
    pub needs_now: bool,
    pub is_docs_id_title: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct PointDeletePlan {
    pub collection: String,
    pub field: String,
    pub key: String,
    pub cas: Option<String>,
    pub append_only: bool,
    pub prepared_gen: u64,
    pub prepared_docs_epoch: u64,
    pub prepared_idx: Option<usize>,
    pub prepared_kd_kt: Option<(IndexKey, IndexKey)>,
}

#[derive(Debug, Clone)]
pub(crate) struct PointUpdatePlan {
    pub collection: String,
    pub field: String,
    pub key: String,
    pub cas: Option<String>,
    pub record: Record,
    pub is_docs_wing_only: bool,
    pub const_wing: Option<(Cell, Arc<str>)>,
    pub prepared_gen: u64,
    pub prepared_docs_epoch: u64,
    pub prepared_idx: Option<usize>,
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
    pub kind: AggKind,
    pub needs_now: bool,
    pub index_plan: Option<(Vec<IndexUse>, bool)>,
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
    pub(crate) agg_plan: Option<Arc<AggPlan>>,
    pub(crate) point_delete: Option<Arc<PointDeletePlan>>,
    pub(crate) point_update: Option<Arc<PointUpdatePlan>>,
}

impl Prepared {
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
        let bound: Vec<Stmt> = self
            .stmts
            .iter()
            .map(|s| bind_stmt_params(s, params))
            .collect();
        db.run_bound_stmts(&bound, &self.plan, self.writes, self.append_only, self.schema, true)
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
        let bound: Vec<Stmt> = self
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
        let bound: Vec<Stmt> = self
            .stmts
            .iter()
            .map(|s| bind_stmt_params(s, params))
            .collect();
        Ok(db
            .run_bound_stmts(&bound, &self.plan, self.writes, self.append_only, self.schema, false)?
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

impl Db {
    pub(crate) fn try_extract_point_get(q: &Query, cat: &Catalog) -> Option<PointGetPlan> {
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
        if !matches!(q.steps.first(), Some(Step::Filter(_))) {
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
        if pred_has_param(pred) {
            return None;
        }
        let (field, key) = point_key(pred)?;
        if field == "uri" && name != "docs" {
            return None;
        }
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
        let needs_now = !simple_eq && pred_needs_now(pred);
        Some(PointGetPlan {
            collection: name,
            field: field.to_string(),
            key: key.to_string(),
            fields,
            is_simple_eq: simple_eq,
            pred: if simple_eq { None } else { Some(pred.clone()) },
            needs_now,
            is_docs_id_title,
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
            cas_each: false,
        } = stmt
        else {
            return None;
        };
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
                let cas_ok = if append_only {
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
            append_only,
            prepared_gen: store.r#gen,
            prepared_docs_epoch: store.docs_epoch,
            prepared_idx,
            prepared_kd_kt,
        })
    }

    pub(crate) fn try_extract_point_update(
        stmt: &Stmt,
        store: &Store,
    ) -> Option<PointUpdatePlan> {
        let Stmt::Update {
            collection,
            pred: Some(pred),
            cas,
            cas_each: false,
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
        let mut prepared_idx = None;
        if collection == "docs" && !store.is_disk_backed("docs") {
            let docs = store.collection("docs");
            if let Some(idx) = store.point_index("docs", field, key)
                && idx < docs.len()
            {
                prepared_idx = Some(idx);
            }
        }
        Some(PointUpdatePlan {
            collection: collection.clone(),
            field: field.to_string(),
            key: key.to_string(),
            cas: cas.clone(),
            record: record.clone(),
            is_docs_wing_only,
            const_wing,
            prepared_gen: store.r#gen,
            prepared_docs_epoch: store.docs_epoch,
            prepared_idx,
        })
    }

    #[inline]
    pub(crate) fn execute_point_get(&self, pg: &PointGetPlan) -> Option<Row> {
        if pg.is_docs_id_title {
            self.store.point_get_docs_id_title(&pg.key)
        } else if pg.is_simple_eq {
            self.store
                .project_by_key(&pg.collection, &pg.field, &pg.key, pg.fields.as_deref())
        } else {
            let full = self
                .store
                .project_by_key(&pg.collection, &pg.field, &pg.key, None)?;
            let now = if pg.needs_now { now_ms() } else { 0 };
            if let Some(pred) = &pg.pred
                && !eval_pred(pred, &full, now)
            {
                return None;
            }
            if let Some(fs) = &pg.fields {
                Some(
                    self.store
                        .project_by_key(&pg.collection, &pg.field, &pg.key, Some(fs))
                        .unwrap_or(full),
                )
            } else {
                Some(full)
            }
        }
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
                let now = now_ms();
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
        if !matches!(q.steps.first(), Some(Step::Filter(_))) {
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
                Step::Take { .. } => {}
                Step::Skip { .. } => {}
                _ => return None,
            }
        }
        let pred = filter?;
        let (field, key) = point_key(pred)?;
        if q.steps.iter().any(|step| {
            matches!(step, Step::Skip { n } if *n > 0)
                || matches!(step, Step::Take { n: Some(n) } if *n <= 0)
        }) {
            return Some(Vec::new());
        }

        let simple_eq = matches!(
            pred,
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
        let now = if pred_needs_now(pred) { now_ms() } else { 0 };
        let Some(full) = self.store.project_by_key(name, field, key, None) else {
            return Some(Vec::new());
        };
        if !eval_pred(pred, &full, now) {
            return Some(Vec::new());
        }
        Some(vec![
            self.store
                .project_by_key(name, field, key, fields.as_deref())
                .unwrap_or(full),
        ])
    }
}
