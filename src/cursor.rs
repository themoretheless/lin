//! Pull-based row cursor for Lin queries.
//!
//! **Lazy** pipelines (no full result `Vec`):
//! - `collection | filter? | project? | skip* | take?`
//! - `collection | filter? | join|left_join right on f | project? | skip* | take?`
//!   (nested-loop: left scan/index + point lookup on the right via FK)
//!
//! hop / graph / match / sort / union / search / multi-join → **buffered** materialize.

use std::collections::VecDeque;

use crate::ast::{Pred, Query, Source, Stmt, Step};
use crate::error::Error;
use crate::exec::{
    Db, ReadDb, JoinFieldPlan, build_right_probe_owned, emit_join_row, eval_pred, field_names,
    plan_join_fields, project_join_fields,
};
use crate::query::Queryable;
use crate::store::{Cell, Row, Store, now_ms, project_fields};
use rustc_hash::FxHashMap;

/// Sync pull cursor over query results.
pub struct QueryCursor<'a> {
    db: CursorDb<'a>,
    state: CursorState,
}

enum CursorDb<'a> {
    Writer(&'a Db),
    Reader(&'a ReadDb),
}

impl CursorDb<'_> {
    fn store_catalog(&self) -> (&Store, &crate::catalog::Catalog) {
        match self {
            CursorDb::Writer(db) => (&db.store, &db.catalog),
            CursorDb::Reader(db) => {
                let inner = db.as_db();
                (&inner.store, &inner.catalog)
            }
        }
    }

    fn run_query(&self, q: &Query) -> Result<Vec<Row>, Error> {
        match self {
            CursorDb::Writer(db) => db.run_query_readonly(q),
            CursorDb::Reader(db) => Ok(db.run_stmt(Stmt::Query(q.clone()))?.rows),
        }
    }

    fn prepare_query(&self, q: &Query) -> Result<(), Error> {
        let stmt = Stmt::Query(q.clone());
        match self {
            CursorDb::Writer(db) => {
                db.prepare_stmt(stmt)?;
            }
            CursorDb::Reader(db) => {
                db.prepare_stmt(stmt)?;
            }
        }
        Ok(())
    }
}

enum CursorState {
    Lazy(LazyCursor),
    LazyJoin(LazyJoinCursor),
    Buffered(std::vec::IntoIter<Row>),
    Done,
}

struct LazyCursor {
    collection: String,
    source: RowSource,
    pos: usize,
    pred: Option<Pred>,
    need_filter: bool,
    project: Option<Vec<String>>,
    skip_left: usize,
    take_left: Option<usize>,
    now: i64,
}

struct LazyJoinCursor {
    left: LazyCursor,
    right_col: String,
    on: String,
    to_field: String,
    left_join: bool,
    project: Option<Vec<String>>,
    /// When `to_field == "id"` and project is set: hash-built right cells.
    right_probe: Option<FxHashMap<String, Vec<Cell>>>,
    join_plan: Option<Vec<JoinFieldPlan>>,
    skip_left: usize,
    take_left: Option<usize>,
    pending: VecDeque<Row>,
}

enum RowSource {
    Idxs(Vec<usize>),
    Scan { len: usize },
}

impl<'a> QueryCursor<'a> {
    pub fn open_db(db: &'a Db, q: &Query) -> Result<Self, Error> {
        Self::open(CursorDb::Writer(db), q)
    }

    pub fn open_read(db: &'a ReadDb, q: &Query) -> Result<Self, Error> {
        Self::open(CursorDb::Reader(db), q)
    }

    fn open(db: CursorDb<'a>, q: &Query) -> Result<Self, Error> {
        db.prepare_query(q)?;

        if let Some(join) = try_lazy_join(&db, q)? {
            return Ok(Self {
                db,
                state: CursorState::LazyJoin(join),
            });
        }
        if let Some(lazy) = try_lazy_simple(&db, q)? {
            return Ok(Self {
                db,
                state: CursorState::Lazy(lazy),
            });
        }

        let rows = db.run_query(q)?;
        Ok(Self {
            db,
            state: CursorState::Buffered(rows.into_iter()),
        })
    }

    /// Whether this cursor pulls lazily (not a pre-built `Vec`).
    pub fn is_lazy(&self) -> bool {
        matches!(
            self.state,
            CursorState::Lazy(_) | CursorState::LazyJoin(_)
        )
    }
}

impl Iterator for QueryCursor<'_> {
    type Item = Result<Row, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        if matches!(self.state, CursorState::Done) {
            return None;
        }
        if matches!(self.state, CursorState::Buffered(_)) {
            let CursorState::Buffered(it) = &mut self.state else {
                unreachable!()
            };
            return it.next().map(Ok);
        }

        let mut state = std::mem::replace(&mut self.state, CursorState::Done);
        let (store, _) = self.db.store_catalog();

        let item = match &mut state {
            CursorState::Lazy(lazy) => next_lazy(store, lazy),
            CursorState::LazyJoin(join) => next_lazy_join(store, join),
            _ => None,
        };

        if item.is_some() {
            self.state = state;
        }
        item
    }
}

fn next_lazy(store: &Store, lazy: &mut LazyCursor) -> Option<Result<Row, Error>> {
    loop {
        if lazy.take_left == Some(0) {
            return None;
        }
        let idx = match &lazy.source {
            RowSource::Idxs(idxs) => {
                if lazy.pos >= idxs.len() {
                    return None;
                }
                let i = idxs[lazy.pos];
                lazy.pos += 1;
                i
            }
            RowSource::Scan { len } => {
                if lazy.pos >= *len {
                    return None;
                }
                let i = lazy.pos;
                lazy.pos += 1;
                i
            }
        };
        let Some(row) = store.get_by_idx(&lazy.collection, idx) else {
            continue;
        };
        if lazy.need_filter
            && let Some(pred) = &lazy.pred
            && !eval_pred(pred, row, lazy.now)
        {
            continue;
        }
        if lazy.skip_left > 0 {
            lazy.skip_left -= 1;
            continue;
        }
        if let Some(t) = lazy.take_left.as_mut() {
            *t = t.saturating_sub(1);
        }
        let out = if let Some(fields) = &lazy.project {
            project_fields(row, fields)
        } else {
            row.clone()
        };
        return Some(Ok(out));
    }
}

fn next_lazy_join(store: &Store, join: &mut LazyJoinCursor) -> Option<Result<Row, Error>> {
    loop {
        if join.take_left == Some(0) {
            return None;
        }
        if let Some(row) = join.pending.pop_front() {
            if join.skip_left > 0 {
                join.skip_left -= 1;
                continue;
            }
            if let Some(t) = join.take_left.as_mut() {
                *t = t.saturating_sub(1);
            }
            return Some(Ok(row));
        }

        let left_row = next_lazy_raw(store, &mut join.left)?;

        let Ok(left_row) = left_row else {
            return Some(left_row);
        };

        let key = left_row.get(&join.on).and_then(Cell::text);

        // Hash-built projected right side (FK → id + project).
        if let (Some(probe), Some(plan)) = (&join.right_probe, &join.join_plan) {
            let right_cells = key.and_then(|k| probe.get(k).map(|v| v.as_slice()));
            if right_cells.is_none() && !join.left_join {
                continue;
            }
            let out = emit_join_row(&left_row, right_cells, plan);
            if join.skip_left > 0 {
                join.skip_left -= 1;
                continue;
            }
            if let Some(t) = join.take_left.as_mut() {
                *t = t.saturating_sub(1);
            }
            return Some(Ok(out));
        }

        let right = if join.to_field == "id" {
            key.and_then(|k| store.get_by_id(&join.right_col, k))
        } else {
            None
        };

        if join.to_field == "id" {
            if right.is_none() && !join.left_join {
                continue;
            }
            let out = if let Some(fields) = join.project.as_deref() {
                project_join_fields(&left_row, right, &join.right_col, fields)
            } else if let Some(r) = right {
                let mut merged = left_row;
                for (k, v) in r {
                    merged.insert(format!("{}.{}", join.right_col, k), v.clone());
                }
                merged
            } else {
                left_row
            };
            if join.skip_left > 0 {
                join.skip_left -= 1;
                continue;
            }
            if let Some(t) = join.take_left.as_mut() {
                *t = t.saturating_sub(1);
            }
            return Some(Ok(out));
        }

        // Non-id: clone via project_by_key (rare for catalog FKs).
        let mut hits = Vec::new();
        if let Some(k) = key {
            if let Some(r) = store.project_by_key(&join.right_col, &join.to_field, k, None) {
                hits.push(r);
            }
        }

        if hits.is_empty() {
            if join.left_join {
                let out = apply_project(&left_row, join.project.as_deref());
                join.pending.push_back(out);
            }
            continue;
        }

        for r in hits {
            let mut merged = left_row.clone();
            for (k, v) in r {
                merged.insert(format!("{}.{}", join.right_col, k), v);
            }
            let out = apply_project(&merged, join.project.as_deref());
            join.pending.push_back(out);
        }
    }
}

/// Pull next matching left row without output project/skip/take (join owns those).
fn next_lazy_raw(store: &Store, lazy: &mut LazyCursor) -> Option<Result<Row, Error>> {
    // Temporarily clear output limits on left — left was built without them for join.
    loop {
        let idx = match &lazy.source {
            RowSource::Idxs(idxs) => {
                if lazy.pos >= idxs.len() {
                    return None;
                }
                let i = idxs[lazy.pos];
                lazy.pos += 1;
                i
            }
            RowSource::Scan { len } => {
                if lazy.pos >= *len {
                    return None;
                }
                let i = lazy.pos;
                lazy.pos += 1;
                i
            }
        };
        let Some(row) = store.get_by_idx(&lazy.collection, idx) else {
            continue;
        };
        if lazy.need_filter
            && let Some(pred) = &lazy.pred
            && !eval_pred(pred, row, lazy.now)
        {
            continue;
        }
        return Some(Ok(row.clone()));
    }
}

fn apply_project(row: &Row, project: Option<&[String]>) -> Row {
    match project {
        Some(fields) => project_fields(row, fields),
        None => row.clone(),
    }
}

fn build_left_source(
    store: &Store,
    catalog: &crate::catalog::Catalog,
    name: &str,
    filter: Option<&Pred>,
    now: i64,
) -> (RowSource, bool) {
    if let Some(pred) = filter {
        if let Some(uses) = crate::index::pick_index(catalog, name, pred)
            && let Some(idxs) = store.index_seek(name, &uses, now)
        {
            let covered = crate::index::index_covers_pred(pred, &uses);
            return (RowSource::Idxs(idxs), !covered);
        }
        return (
            RowSource::Scan {
                len: store.collection(name).len(),
            },
            true,
        );
    }
    (
        RowSource::Scan {
            len: store.collection(name).len(),
        },
        false,
    )
}

fn try_lazy_simple(db: &CursorDb<'_>, q: &Query) -> Result<Option<LazyCursor>, Error> {
    let Source::Collection(name) = &q.source else {
        return Ok(None);
    };
    let mut filter: Option<&Pred> = None;
    let mut project: Option<Vec<String>> = None;
    let mut skip_n: usize = 0;
    let mut take_n: Option<Option<i64>> = None;
    let mut saw_take = false;

    for step in &q.steps {
        match step {
            Step::Filter(p) => {
                if filter.is_some() {
                    return Ok(None);
                }
                filter = Some(p);
            }
            Step::Project(f) => {
                if project.is_some() {
                    return Ok(None);
                }
                project = Some(field_names(f));
            }
            Step::Skip { n } => {
                skip_n = skip_n.saturating_add((*n).max(0) as usize);
            }
            Step::Take { n } => {
                if saw_take {
                    return Ok(None);
                }
                saw_take = true;
                take_n = Some(*n);
            }
            _ => return Ok(None),
        }
    }

    let (store, catalog) = db.store_catalog();
    let now = now_ms();
    let take_left = match take_n {
        Some(Some(n)) => Some(n.max(0) as usize),
        Some(None) => None,
        None => Some(50),
    };
    let (source, need_filter) = build_left_source(store, catalog, name, filter, now);

    Ok(Some(LazyCursor {
        collection: name.clone(),
        source,
        pos: 0,
        pred: filter.cloned(),
        need_filter,
        project,
        skip_left: skip_n,
        take_left,
        now,
    }))
}

fn try_lazy_join(db: &CursorDb<'_>, q: &Query) -> Result<Option<LazyJoinCursor>, Error> {
    let Source::Collection(name) = &q.source else {
        return Ok(None);
    };

    let mut filter: Option<&Pred> = None;
    let mut join: Option<(bool, String, String)> = None; // left_join, right, on
    let mut project: Option<Vec<String>> = None;
    let mut skip_n: usize = 0;
    let mut take_n: Option<Option<i64>> = None;
    let mut saw_take = false;
    let mut saw_join = false;

    for step in &q.steps {
        match step {
            Step::Filter(p) if !saw_join => {
                if filter.is_some() {
                    return Ok(None);
                }
                filter = Some(p);
            }
            Step::Join {
                left,
                collection,
                on,
            } if !saw_join => {
                saw_join = true;
                join = Some((*left, collection.clone(), on.clone()));
            }
            Step::Project(f) if saw_join => {
                if project.is_some() {
                    return Ok(None);
                }
                project = Some(field_names(f));
            }
            Step::Skip { n } if saw_join => {
                skip_n = skip_n.saturating_add((*n).max(0) as usize);
            }
            Step::Take { n } if saw_join => {
                if saw_take {
                    return Ok(None);
                }
                saw_take = true;
                take_n = Some(*n);
            }
            // Project/skip/take before join — not supported for lazy join path
            Step::Project(_) | Step::Skip { .. } | Step::Take { .. } if !saw_join => {
                return Ok(None);
            }
            _ => return Ok(None),
        }
    }

    let Some((left_join, right_col, on)) = join else {
        return Ok(None);
    };

    let (store, catalog) = db.store_catalog();
    let now = now_ms();
    let to_field = catalog
        .find_fk(name, &on, &right_col)
        .map(|fk| fk.to_field.clone())
        .unwrap_or_else(|| "id".into());

    let take_left = match take_n {
        Some(Some(n)) => Some(n.max(0) as usize),
        Some(None) => None,
        None => Some(50),
    };
    let (source, need_filter) = build_left_source(store, catalog, name, filter, now);

    let (right_probe, join_plan) = if to_field == "id" {
        if let Some(ref fields) = project {
            let (right_fields, plan) = plan_join_fields(&right_col, fields);
            let probe = build_right_probe_owned(store.collection(&right_col), &right_fields);
            (Some(probe), Some(plan))
        } else {
            // Full-row probe by id index into collection — build id→row clone map would be heavy;
            // keep point-get via get_by_id in next (no owned full-row hash).
            (None, None)
        }
    } else {
        (None, None)
    };

    Ok(Some(LazyJoinCursor {
        left: LazyCursor {
            collection: name.clone(),
            source,
            pos: 0,
            pred: filter.cloned(),
            need_filter,
            project: None,
            skip_left: 0,
            take_left: None,
            now,
        },
        right_col,
        on,
        to_field,
        left_join,
        project,
        right_probe,
        join_plan,
        skip_left: skip_n,
        take_left,
        pending: VecDeque::new(),
    }))
}

impl Db {
    /// Open a pull cursor for a query (lazy when the pipeline is simple / single FK join).
    pub fn cursor(&self, q: &Query) -> Result<QueryCursor<'_>, Error> {
        QueryCursor::open_db(self, q)
    }

    pub fn cursor_queryable(&self, q: &Queryable) -> Result<QueryCursor<'_>, Error> {
        self.cursor(q.query())
    }

    /// Read-only query materialization for cursor fallback (`&self`).
    pub(crate) fn run_query_readonly(&self, q: &Query) -> Result<Vec<Row>, Error> {
        self.exec_query(q, &Default::default())
    }
}

impl ReadDb {
    pub fn cursor(&self, q: &Query) -> Result<QueryCursor<'_>, Error> {
        QueryCursor::open_read(self, q)
    }

    pub fn cursor_queryable(&self, q: &Queryable) -> Result<QueryCursor<'_>, Error> {
        self.cursor(q.query())
    }
}

impl Queryable {
    pub fn cursor<'a>(&self, db: &'a Db) -> Result<QueryCursor<'a>, Error> {
        db.cursor(self.query())
    }

    pub fn cursor_read<'a>(&self, db: &'a ReadDb) -> Result<QueryCursor<'a>, Error> {
        db.cursor(self.query())
    }
}
