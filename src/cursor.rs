//! Pull-based row cursor for Lin queries.
//!
//! **Lazy** pipelines (no full result `Vec`):
//! - `collection | filter? | project? | skip* | take?`
//! - `collection | filter? | join|left_join right on f | project? | skip* | take?`
//!   (nested-loop: left scan/index + point lookup on the right via FK)
//! - Hot SoA: `orders [| total >] | join users on user_id | { id, users.email, total }`
//! - `collection | filter? | hop rel` (depth 1) `| project? | skip* | take?`
//! - `collection | search lex|hybrid "…" | project? | skip* | take?` (FTS candidates)
//!
//! graph / match / sort / union / search vec / hop depth>1 / multi-join → **buffered**.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::Arc;

use crate::ast::{Pred, Query, SearchMode, Source, Step, Stmt};
use crate::error::Error;
use crate::exec::{
    Db, JoinFieldPlan, ReadDb, build_right_probe_owned, emit_join_row, eval_pred, field_names,
    plan_join_fields, pred_total_gt, project_join_fields,
};
use crate::query::Queryable;
use crate::store::{Cell, Row, Store, now_ms, project_fields, row_text};
use rustc_hash::FxHashMap;

/// Sync pull cursor over query results.
pub struct QueryCursor<'a> {
    db: CursorDb<'a>,
    state: CursorState,
}

/// Compact cursor projection: one shared field schema plus row-local cells.
///
/// Use [`QueryCursor::next_projected`] to avoid allocating a
/// `BTreeMap<String, Cell>` for every projected row.
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectedRow {
    fields: Arc<[String]>,
    cells: Vec<Cell>,
}

impl ProjectedRow {
    pub fn fields(&self) -> &[String] {
        &self.fields
    }

    pub fn cells(&self) -> &[Cell] {
        &self.cells
    }

    #[inline]
    pub fn get(&self, field: &str) -> Option<&Cell> {
        self.fields
            .iter()
            .position(|candidate| candidate == field)
            .and_then(|i| self.cells.get(i))
    }

    pub fn into_row(self) -> Row {
        self.fields
            .iter()
            .cloned()
            .zip(self.cells)
            .collect::<BTreeMap<_, _>>()
    }

    fn from_row(row: Row) -> Self {
        let (fields, cells): (Vec<_>, Vec<_>) = row.into_iter().unzip();
        Self {
            fields: fields.into(),
            cells,
        }
    }
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

    fn search_fts(
        &self,
        collection: &str,
        mode: SearchMode,
        query: &str,
        rank_limit: Option<usize>,
    ) -> Result<Vec<Row>, Error> {
        match self {
            CursorDb::Writer(db) => db.search_fts(collection, mode, query, rank_limit),
            CursorDb::Reader(db) => db.as_db().search_fts(collection, mode, query, rank_limit),
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

#[allow(clippy::large_enum_variant)]
enum CursorState {
    Lazy(LazyCursor),
    LazyJoin(LazyJoinCursor),
    /// Hot SoA: `orders [| total >] | join users on user_id | { id, users.email, total }`
    LazyJoinSoa(LazyJoinSoa),
    Buffered(std::vec::IntoIter<Row>),
    Done,
}

struct LazyCursor {
    collection: String,
    source: RowSource,
    pos: usize,
    pred: Option<Pred>,
    need_filter: bool,
    project: Option<Arc<[String]>>,
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
    left_fields: Option<Vec<String>>,
    skip_left: usize,
    take_left: Option<usize>,
    pending: VecDeque<Row>,
}

/// Columnar left + user-index probe; emits compact 3-field rows.
struct LazyJoinSoa {
    source: RowSource,
    pos: usize,
    left_join: bool,
    /// When set, filter `orders.total > min` without row maps.
    total_gt: Option<f64>,
    /// user_id → index into users SoA columns.
    probe: FxHashMap<Arc<str>, usize>,
    skip_left: usize,
    take_left: Option<usize>,
    fields: Arc<[String]>,
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
        let q = {
            let (_, cat) = db.store_catalog();
            crate::catalog::with_catalog_filter(q, cat)
        };
        let q = &q;
        db.prepare_query(q)?;

        if let Some(join) = try_lazy_join_soa(&db, q)? {
            return Ok(Self {
                db,
                state: CursorState::LazyJoinSoa(join),
            });
        }
        if let Some(join) = try_lazy_join(&db, q)? {
            return Ok(Self {
                db,
                state: CursorState::LazyJoin(join),
            });
        }
        if let Some(lazy) = try_lazy_search(&db, q)? {
            return Ok(Self {
                db,
                state: CursorState::Lazy(lazy),
            });
        }
        if let Some(lazy) = try_lazy_hop(&db, q)? {
            return Ok(Self {
                db,
                state: CursorState::Lazy(lazy),
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
            CursorState::Lazy(_) | CursorState::LazyJoin(_) | CursorState::LazyJoinSoa(_)
        )
    }

    /// Pull a compact projected row. Projection schemas are shared across rows;
    /// non-projected or buffered plans fall back to converting the regular row.
    pub fn next_projected(&mut self) -> Option<Result<ProjectedRow, Error>> {
        let native = matches!(&self.state, CursorState::Lazy(lazy) if lazy.project.is_some())
            || matches!(self.state, CursorState::LazyJoinSoa(_));
        if !native {
            return self.next().map(|row| row.map(ProjectedRow::from_row));
        }

        let (store, _) = self.db.store_catalog();
        let item = match &mut self.state {
            CursorState::Lazy(lazy) => next_lazy_projected(store, lazy),
            CursorState::LazyJoinSoa(join) => next_lazy_join_soa_projected(store, join),
            _ => unreachable!("native projected state checked above"),
        };
        if item.is_none() {
            self.state = CursorState::Done;
        }
        item
    }

    /// Map the next row through [`crate::row::FromRow`].
    pub fn next_typed<T: crate::row::FromRow>(&mut self) -> Option<Result<T, Error>> {
        self.next().map(|row| row.and_then(|row| T::from_row(&row)))
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

        let (store, _) = self.db.store_catalog();

        let item = match &mut self.state {
            CursorState::Lazy(lazy) => next_lazy(store, lazy),
            CursorState::LazyJoin(join) => next_lazy_join(store, join),
            CursorState::LazyJoinSoa(join) => next_lazy_join_soa(store, join),
            _ => None,
        };

        if item.is_none() {
            self.state = CursorState::Done;
        }
        item
    }
}

fn next_lazy(store: &Store, lazy: &mut LazyCursor) -> Option<Result<Row, Error>> {
    let idx = next_lazy_match_idx(store, lazy)?;
    let row = store.get_by_idx(&lazy.collection, idx)?;
    let out = if let Some(fields) = &lazy.project {
        project_fields(row, fields)
    } else {
        row.clone()
    };
    Some(Ok(out))
}

#[inline]
fn next_lazy_projected(
    store: &Store,
    lazy: &mut LazyCursor,
) -> Option<Result<ProjectedRow, Error>> {
    let idx = next_lazy_match_idx(store, lazy)?;
    let row = store.get_by_idx(&lazy.collection, idx)?;
    let fields = lazy.project.as_ref().expect("projected state checked");
    let cells = fields
        .iter()
        .map(|field| row.get(field).cloned().unwrap_or(Cell::Null))
        .collect();
    Some(Ok(ProjectedRow {
        fields: Arc::clone(fields),
        cells,
    }))
}

#[inline]
fn next_lazy_match_idx(store: &Store, lazy: &mut LazyCursor) -> Option<usize> {
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
        return Some(idx);
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

        let left_row = if let Some(fields) = &join.left_fields {
            next_lazy_raw_projected(store, &mut join.left, fields)?
        } else {
            next_lazy_raw(store, &mut join.left)?
        };

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
        if let Some(k) = key
            && let Some(r) = store.project_by_key(&join.right_col, &join.to_field, k, None)
        {
            hits.push(r);
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

fn next_lazy_raw_projected(
    store: &Store,
    lazy: &mut LazyCursor,
    fields: &[String],
) -> Option<Result<Row, Error>> {
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
        return Some(Ok(project_fields(row, fields)));
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
        project: project.map(Into::into),
        skip_left: skip_n,
        take_left,
        now,
    }))
}

/// `col | search lex|hybrid … | project? | skip* | take?` via FTS-ordered idxs.
fn try_lazy_search(db: &CursorDb<'_>, q: &Query) -> Result<Option<LazyCursor>, Error> {
    let Source::Collection(name) = &q.source else {
        return Ok(None);
    };
    let mut mode: Option<SearchMode> = None;
    let mut query: Option<&str> = None;
    let mut project: Option<Vec<String>> = None;
    let mut skip_n: usize = 0;
    let mut take_n: Option<Option<i64>> = None;
    let mut saw_take = false;

    for step in &q.steps {
        match step {
            Step::Search { mode: m, query: qq } => {
                if mode.is_some() {
                    return Ok(None);
                }
                if !matches!(m, SearchMode::Lex | SearchMode::Hybrid) {
                    return Ok(None);
                }
                mode = Some(*m);
                query = Some(qq.as_str());
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

    let (Some(mode), Some(query)) = (mode, query) else {
        return Ok(None);
    };
    let (store, _) = db.store_catalog();
    if !store.fts.contains_key(name) {
        return Ok(None);
    }

    let take_left = match take_n {
        Some(Some(n)) => Some(n.max(0) as usize),
        Some(None) => None,
        None => Some(50),
    };
    let rank_limit = if mode == SearchMode::Lex {
        take_left.map(|n| n.saturating_add(skip_n))
    } else {
        None
    };
    let ranked = db.search_fts(name, mode, query, rank_limit)?;
    let mut idxs = Vec::with_capacity(ranked.len());
    for r in &ranked {
        let Some(id) = row_text(r, "id") else {
            continue;
        };
        if let Some(i) = store.row_index(name, id) {
            idxs.push(i);
        }
    }

    Ok(Some(LazyCursor {
        collection: name.clone(),
        source: RowSource::Idxs(idxs),
        pos: 0,
        pred: None,
        need_filter: false,
        project: project.map(Into::into),
        skip_left: skip_n,
        take_left,
        now: now_ms(),
    }))
}

/// `col | filter? | hop rel` (depth 1) `| project? | skip* | take?`.
fn try_lazy_hop(db: &CursorDb<'_>, q: &Query) -> Result<Option<LazyCursor>, Error> {
    let Source::Collection(name) = &q.source else {
        return Ok(None);
    };
    let mut filter: Option<&Pred> = None;
    let mut hop_rel: Option<&str> = None;
    let mut project: Option<Vec<String>> = None;
    let mut skip_n: usize = 0;
    let mut take_n: Option<Option<i64>> = None;
    let mut saw_take = false;

    for step in &q.steps {
        match step {
            Step::Filter(p) => {
                if filter.is_some() || hop_rel.is_some() {
                    return Ok(None);
                }
                filter = Some(p);
            }
            Step::Hop { rel, depth } => {
                if hop_rel.is_some() {
                    return Ok(None);
                }
                if let Some(d) = depth
                    && *d != 1
                {
                    return Ok(None);
                }
                hop_rel = Some(rel.as_str());
            }
            Step::Project(f) => {
                if hop_rel.is_none() || project.is_some() {
                    return Ok(None);
                }
                project = Some(field_names(f));
            }
            Step::Skip { n } => {
                if hop_rel.is_none() {
                    return Ok(None);
                }
                skip_n = skip_n.saturating_add((*n).max(0) as usize);
            }
            Step::Take { n } => {
                if hop_rel.is_none() || saw_take {
                    return Ok(None);
                }
                saw_take = true;
                take_n = Some(*n);
            }
            _ => return Ok(None),
        }
    }

    let Some(rel) = hop_rel else {
        return Ok(None);
    };

    let (store, catalog) = db.store_catalog();
    let now = now_ms();
    let (source, need_filter) = build_left_source(store, catalog, name, filter, now);

    let (edge_rel, reverse) = match catalog.rel(rel) {
        Some(r) if let Some(of) = &r.reverse_of => (of.as_str(), true),
        _ => (rel, false),
    };

    let mut frontier: BTreeSet<String> = BTreeSet::new();
    collect_seed_keys(
        store,
        name,
        &source,
        filter,
        need_filter,
        now,
        &mut frontier,
    );

    let mut seen = frontier.clone();
    let mut reached: BTreeSet<String> = BTreeSet::new();
    for e in &store.edges {
        if e.rel != edge_rel {
            continue;
        }
        let (src, dst) = if reverse {
            (e.to.as_str(), e.from.as_str())
        } else {
            (e.from.as_str(), e.to.as_str())
        };
        if frontier.contains(src) && seen.insert(dst.to_string()) {
            reached.insert(dst.to_string());
            if reached.len() >= 300 {
                break;
            }
        }
    }

    let out_collection = if name == "docs" || store.collections.contains_key("docs") {
        // hop resolves via find_doc_key → docs first
        "docs"
    } else {
        name.as_str()
    };
    let mut idxs = Vec::new();
    for key in reached {
        if let Some(i) = neighbor_idx(store, name, out_collection, &key) {
            idxs.push(i);
        }
        if idxs.len() >= 300 {
            break;
        }
    }

    let take_left = match take_n {
        Some(Some(n)) => Some(n.max(0) as usize),
        Some(None) => None,
        None => Some(50),
    };

    Ok(Some(LazyCursor {
        collection: out_collection.to_string(),
        source: RowSource::Idxs(idxs),
        pos: 0,
        pred: None,
        need_filter: false,
        project: project.map(Into::into),
        skip_left: skip_n,
        take_left,
        now,
    }))
}

fn neighbor_idx(store: &Store, primary: &str, out_collection: &str, key: &str) -> Option<usize> {
    if let Some(row) = store.find_doc_key(key) {
        let id = row_text(row, "id")?;
        return store.row_index(out_collection, id);
    }
    if primary != "docs" {
        return store.row_index(primary, key);
    }
    None
}

fn collect_seed_keys(
    store: &Store,
    collection: &str,
    source: &RowSource,
    pred: Option<&Pred>,
    need_filter: bool,
    now: i64,
    out: &mut BTreeSet<String>,
) {
    let rows = store.collection(collection);
    let push = |row: &Row, out: &mut BTreeSet<String>| {
        if let Some(id) = row_text(row, "id") {
            out.insert(id.to_string());
        }
        if let Some(uri) = row_text(row, "uri") {
            out.insert(uri.to_string());
        }
    };
    match source {
        RowSource::Idxs(idxs) => {
            for &i in idxs {
                let Some(row) = rows.get(i) else {
                    continue;
                };
                if need_filter
                    && let Some(p) = pred
                    && !eval_pred(p, row, now)
                {
                    continue;
                }
                push(row, out);
            }
        }
        RowSource::Scan { len } => {
            for i in 0..*len {
                let Some(row) = rows.get(i) else {
                    continue;
                };
                if let Some(p) = pred
                    && !eval_pred(p, row, now)
                {
                    continue;
                }
                push(row, out);
            }
        }
    }
}

fn next_lazy_join_soa(store: &Store, join: &mut LazyJoinSoa) -> Option<Result<Row, Error>> {
    next_lazy_join_soa_projected(store, join).map(|row| row.map(ProjectedRow::into_row))
}

#[inline]
fn next_lazy_join_soa_projected(
    store: &Store,
    join: &mut LazyJoinSoa,
) -> Option<Result<ProjectedRow, Error>> {
    let orders_id = store.orders_id();
    let orders_uid = store.orders_user_id();
    let orders_total = store.orders_total();
    let users_email = store.users_email();
    let n = orders_id.len();

    loop {
        if join.take_left == Some(0) {
            return None;
        }
        let idx = match &join.source {
            RowSource::Idxs(idxs) => {
                if join.pos >= idxs.len() {
                    return None;
                }
                let i = idxs[join.pos];
                join.pos += 1;
                i
            }
            RowSource::Scan { len } => {
                if join.pos >= *len {
                    return None;
                }
                let i = join.pos;
                join.pos += 1;
                i
            }
        };
        if idx >= n {
            continue;
        }
        debug_assert_eq!(orders_id.len(), orders_uid.len());
        debug_assert_eq!(orders_id.len(), orders_total.len());
        // SAFETY: `LazyJoinSoa` is constructed only after `orders_soa_ready()`;
        // `idx < orders_id.len()` was checked above, and the cursor's shared Db
        // borrow prevents mutation while these parallel columns are consumed.
        let (order_id, uid, total) = unsafe {
            (
                orders_id.get_unchecked(idx),
                orders_uid.get_unchecked(idx).as_ref(),
                *orders_total.get_unchecked(idx),
            )
        };
        if let Some(min) = join.total_gt
            && total.partial_cmp(&min) != Some(std::cmp::Ordering::Greater)
        {
            continue;
        }
        let right = join.probe.get(uid).copied();
        if right.is_none() && !join.left_join {
            continue;
        }
        if join.skip_left > 0 {
            join.skip_left -= 1;
            continue;
        }
        if let Some(t) = join.take_left.as_mut() {
            *t = t.saturating_sub(1);
        }

        let cells = vec![
            Cell::Text(Arc::clone(order_id)),
            match right {
                Some(ui) => {
                    debug_assert!(ui < users_email.len());
                    // SAFETY: probe indices are built from aligned users SoA
                    // columns after `users_soa_ready()` and cannot mutate here.
                    Cell::Text(Arc::clone(unsafe { users_email.get_unchecked(ui) }))
                }
                None => Cell::Null,
            },
            Cell::Float(total),
        ];
        return Some(Ok(ProjectedRow {
            fields: Arc::clone(&join.fields),
            cells,
        }));
    }
}

fn is_orders_users_id_email_total(fields: &[String]) -> bool {
    fields.len() == 3 && fields[0] == "id" && fields[1] == "users.email" && fields[2] == "total"
}

fn try_lazy_join_soa(db: &CursorDb<'_>, q: &Query) -> Result<Option<LazyJoinSoa>, Error> {
    let Source::Collection(name) = &q.source else {
        return Ok(None);
    };
    if name != "orders" {
        return Ok(None);
    }

    let mut filter: Option<&Pred> = None;
    let mut join: Option<(bool, &str, &str)> = None;
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
                join = Some((*left, collection.as_str(), on.as_str()));
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
            Step::Project(_) | Step::Skip { .. } | Step::Take { .. } if !saw_join => {
                return Ok(None);
            }
            _ => return Ok(None),
        }
    }

    let Some((left_join, right_col, on)) = join else {
        return Ok(None);
    };
    if right_col != "users" || on != "user_id" {
        return Ok(None);
    }
    let Some(fields) = project.as_ref() else {
        return Ok(None);
    };
    if !is_orders_users_id_email_total(fields) {
        return Ok(None);
    }

    let (store, catalog) = db.store_catalog();
    if !store.orders_soa_ready() || !store.users_soa_ready() {
        return Ok(None);
    }
    let to_field = catalog
        .find_fk(name, on, right_col)
        .map(|fk| fk.to_field.as_str())
        .unwrap_or("id");
    if to_field != "id" {
        return Ok(None);
    }

    let total_gt = filter.and_then(pred_total_gt);
    if filter.is_some() && total_gt.is_none() {
        // Non-SoA filter — fall back to generic lazy join.
        return Ok(None);
    }

    let take_left = match take_n {
        Some(Some(n)) => Some(n.max(0) as usize),
        Some(None) => None,
        None => Some(50),
    };

    let n = store.orders_id().len();
    let source = RowSource::Scan { len: n };

    let users_id = store.users_id();
    let mut probe: FxHashMap<Arc<str>, usize> = FxHashMap::default();
    probe.reserve(users_id.len());
    for (i, id) in users_id.iter().enumerate() {
        probe.insert(Arc::clone(id), i);
    }

    Ok(Some(LazyJoinSoa {
        source,
        pos: 0,
        left_join,
        total_gt,
        probe,
        skip_left: skip_n,
        take_left,
        fields: vec![
            String::from("id"),
            String::from("users.email"),
            String::from("total"),
        ]
        .into(),
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

    let (right_probe, join_plan, left_fields) = if to_field == "id" {
        if let Some(ref fields) = project {
            let (right_fields, plan) = plan_join_fields(&right_col, fields);
            let probe = build_right_probe_owned(store.collection(&right_col), &right_fields);
            let left_fields = plan
                .iter()
                .filter_map(|p| match p {
                    JoinFieldPlan::Left(name) => Some(name.clone()),
                    JoinFieldPlan::Right { .. } => None,
                })
                .collect();
            (Some(probe), Some(plan), Some(left_fields))
        } else {
            // Full-row probe by id index into collection — build id→row clone map would be heavy;
            // keep point-get via get_by_id in next (no owned full-row hash).
            (None, None, None)
        }
    } else {
        (None, None, None)
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
        left_fields,
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
