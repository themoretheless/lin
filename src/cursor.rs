//! Pull-based row cursor for Lin queries.
//!
//! **Lazy** pipelines (no full result `Vec`):
//! - `collection | filter? | project? | skip* | take?`
//! - `collection | filter? | join|left_join right on f | project? | skip* | take?`
//!   (nested-loop: left scan/index + point lookup on the right via FK)
//! - Hot SoA: `orders [| total >] | join users on user_id | { id, users.email, total }`
//! - `collection | filter? | hop rel [1..=3]` `| project? | skip* | take?`
//! - `collection | search lex|hybrid|vec "…" | project? | skip* | take?` (FTS / vector candidates)
//!
//! graph / match / sort / union / multi-join → **buffered**.

use std::cell::RefCell;
use std::collections::{BTreeSet, VecDeque};
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use crate::ast::{CmpOp, Field, Pred, Query, SearchMode, Source, Step, Stmt, Value};
use crate::error::Error;
use crate::eval::OwnedPredSlots;
use crate::exec::{
    Db, JoinFieldPlan, PredSlots, ReadDb, emit_join_row, field_names, plan_join_fields,
    pred_total_gt, project_join_fields,
};
use crate::query::Queryable;
use crate::store::{
    Cell, OwnedRowProjector, Row, Store, cell_from_arc_inline, now_ms, project_fields, row_text,
};
use rustc_hash::{FxHashMap, FxHashSet, FxHasher};
use smallvec::{SmallVec, smallvec};

thread_local! {
    static CURSOR_VALIDATED_QUERIES: RefCell<FxHashSet<u64>> = RefCell::new(FxHashSet::default());
}

/// Sync pull cursor over query results.
pub struct QueryCursor<'a> {
    db: CursorDb<'a>,
    state: CursorState<'a>,
}

/// Compact cursor projection: one shared field schema plus row-local cells.
///
/// Use [`QueryCursor::next_projected`] to avoid allocating a
/// `BTreeMap<String, Cell>` for every projected row.
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectedRow {
    fields: Arc<[String]>,
    cells: SmallVec<[Cell; 4]>,
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
        if self.fields.is_empty() || self.cells.is_empty() {
            return Row::new();
        }
        if self.fields.len() == self.cells.len()
            && self.fields.windows(2).all(|w| w[0] < w[1])
        {
            return Row::from_sorted(self.fields.iter().map(String::as_str), self.cells);
        }
        let w = self.fields.len();
        if w == self.cells.len()
            && w <= 8
            && (0..w).all(|i| (i + 1..w).all(|j| self.fields[i] != self.fields[j]))
        {
            let shape = Row::shape_of(&self.fields);
            let mut src_cells = self.cells;
            let mut ordered: SmallVec<[Cell; 8]> = SmallVec::with_capacity(w);
            for target in shape.names() {
                let pos = self
                    .fields
                    .iter()
                    .position(|f| f == target)
                    .unwrap_or(0);
                ordered.push(std::mem::replace(&mut src_cells[pos], Cell::Null));
            }
            return Row::from_parts(shape, ordered);
        }
        let mut row = crate::store::acquire_row();
        for (f, c) in self.fields.iter().zip(self.cells) {
            row.insert(f.as_str(), c);
        }
        row
    }

    fn from_row(row: Row) -> Self {
        let mut fields = Vec::with_capacity(row.len());
        let mut cells = SmallVec::with_capacity(row.len());
        for (f, c) in row {
            fields.push(f);
            cells.push(c);
        }
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

impl<'a> CursorDb<'a> {
    fn inner(&self) -> &'a Db {
        match self {
            Self::Writer(db) => db,
            Self::Reader(db) => db.as_db(),
        }
    }

    fn store_catalog(&self) -> (&'a Store, &'a crate::catalog::Catalog) {
        let inner = self.inner();
        (&inner.store, &inner.catalog)
    }

    fn run_query(&self, q: &Query) -> Result<Vec<Row>, Error> {
        match self {
            CursorDb::Writer(db) => db.run_query_readonly(q),
            CursorDb::Reader(db) => Ok(db.run_stmt(Stmt::Query(q.clone()))?.into_rows()),
        }
    }

    fn search_fts_indices(
        &self,
        collection: &str,
        mode: SearchMode,
        query: &str,
        rank_limit: Option<usize>,
    ) -> Result<Vec<usize>, Error> {
        self.inner()
            .search_fts_indices_ranked(collection, mode, query, rank_limit)
    }

    fn vector_search_indices(
        &self,
        rows: &[Row],
        query: &str,
        rank_limit: Option<usize>,
    ) -> Result<Vec<(f64, usize)>, Error> {
        self.inner().vector_search_indices(rows, query, rank_limit)
    }

    fn prepare_query(&self, q: &Query) -> Result<(), Error> {
        let inner = self.inner();
        let max_ast_bytes = inner.quotas.max_ast_bytes;
        let mut hasher = FxHasher::default();
        inner.catalog.hash_u64().hash(&mut hasher);
        max_ast_bytes.hash(&mut hasher);
        q.hash(&mut hasher);
        let key = hasher.finish();

        let hit = CURSOR_VALIDATED_QUERIES.with(|cache| cache.borrow().contains(&key));
        if hit {
            return Ok(());
        }

        let ast_bytes = q.estimate_bytes() + 2 * std::mem::size_of::<Stmt>();
        if ast_bytes > max_ast_bytes {
            return Err(Error::new(format!(
                "prepared AST size {} exceeds max_ast_bytes quota {}",
                ast_bytes, max_ast_bytes
            )));
        }
        crate::check::check_query_only(q, &inner.catalog)?;

        CURSOR_VALIDATED_QUERIES.with(|cache| {
            let mut set = cache.borrow_mut();
            if set.len() >= 2048 {
                set.clear();
            }
            set.insert(key);
        });
        Ok(())
    }
}

#[allow(clippy::large_enum_variant)]
enum CursorState<'a> {
    #[cfg(feature = "gpu")]
    GpuStream(GpuStreamCursor),
    Lazy(LazyCursor),
    LazyJoin(LazyJoinCursor),
    /// Hot SoA: `orders [| total >] | join users on user_id | { id, users.email, total }`
    LazyJoinSoa(LazyJoinSoa<'a>),
    /// Cold collection pulled from the disk engine chunk by chunk.
    DiskStream(DiskStreamCursor),
    /// Cold search / hop cursor pulling ranked or reached keys lazily via point reads.
    DiskIds(DiskIdsCursor),
    /// Join whose left side streams from the disk engine and whose right side is probed
    /// by `id`, without materializing either collection.
    DiskJoin(DiskJoinCursor),
    Buffered(std::vec::IntoIter<Row>),
    Done,
}

struct DiskIdsCursor {
    collection: String,
    resolve_doc_uri: bool,
    fallback_col: Option<String>,
    ids: Vec<Arc<str>>,
    pos: usize,
    resolved_left: Option<usize>,
    project: Option<Arc<[String]>>,
    skip_left: usize,
    take_left: Option<usize>,
}

fn next_disk_ids(store: &Store, cur: &mut DiskIdsCursor) -> Option<Result<Row, Error>> {
    loop {
        if cur.take_left == Some(0) || cur.resolved_left == Some(0) || cur.pos >= cur.ids.len() {
            return None;
        }
        let key = &cur.ids[cur.pos];
        cur.pos += 1;
        let proj_slice = cur.project.as_deref();
        let row = if cur.resolve_doc_uri {
            let doc_row = if store.is_disk_backed("docs") {
                store
                    .project_by_key("docs", "id", key, proj_slice)
                    .or_else(|| store.project_by_key("docs", "uri", key, proj_slice))
            } else {
                store.find_doc_key(key).map(|r| match &cur.project {
                    Some(fs) => project_fields(r, fs),
                    None => r.clone(),
                })
            };
            match doc_row {
                Some(r) => Some(r),
                None => match cur.fallback_col.as_deref() {
                    Some(fb) => store.project_by_key(fb, "id", key, proj_slice),
                    None => None,
                },
            }
        } else if store.is_disk_backed(&cur.collection) {
            let Some(eng) = store.disk_engine_read() else {
                return None;
            };
            let proj_refs: Option<Vec<&str>> =
                proj_slice.map(|fs| fs.iter().map(String::as_str).collect());
            let fetched = match &proj_refs {
                Some(fs) => eng.get_projected_by_id(&cur.collection, key, fs),
                None => eng.get_by_id(&cur.collection, key),
            };
            match fetched {
                Ok(r) => r,
                Err(e) => {
                    cur.take_left = Some(0);
                    return Some(Err(e));
                }
            }
        } else {
            store.project_by_key(&cur.collection, "id", key, proj_slice)
        };
        let Some(row) = row else {
            continue;
        };
        if let Some(rem) = cur.resolved_left.as_mut() {
            *rem -= 1;
        }
        if cur.skip_left > 0 {
            cur.skip_left -= 1;
            continue;
        }
        if let Some(n) = cur.take_left.as_mut() {
            *n -= 1;
        }
        return Some(Ok(match &cur.project {
            Some(fields) => project_fields(&row, fields),
            None => row,
        }));
    }
}

/// `col | filter? | project? | skip* | take?` over a disk-backed collection, read through
/// [`crate::disk::DiskEngine::scan_chunk`] without materializing the collection.
struct DiskStreamCursor {
    collection: String,
    pred: Option<Pred>,
    project: Option<Vec<String>>,
    skip_left: usize,
    take_left: Option<usize>,
    now: i64,
    pos: crate::disk::DiskScanPos,
    pending: VecDeque<Row>,
}

/// Rows fetched per engine call; small enough that `take 10` does not read a segment.
const DISK_STREAM_CHUNK: usize = 256;

/// Fetch all live rows in a cold `right_col` matching `key` on non-`id` `to_field`,
/// matching `eval_join`'s `cell_key` rules (`Text`/`SmallText` and decimal `Int`) without
/// hydrating the cold table.
fn fetch_cold_non_id_right_rows(
    store: &Store,
    right_col: &str,
    to_field: &str,
    key: &str,
    proj_fields: Option<&[String]>,
) -> Result<Vec<Row>, Error> {
    let Some(eng) = store.disk_engine_read() else {
        return Err(Error::runtime("disk engine unavailable"));
    };
    let can_use_text_eq = key.parse::<i64>().is_err()
        && eng.manifest.tables.get(right_col).is_some_and(|sch| {
            sch.columns.iter().all(|c| {
                c.name != to_field || !matches!(c.kind, crate::disk::DiskColKind::Int64)
            })
        });
    if can_use_text_eq {
        let pred = Pred::Cmp {
            field: Field::name(to_field),
            op: CmpOp::Eq,
            value: Value::String(Arc::from(key)),
        };
        let proj_refs: Option<Vec<&str>> =
            proj_fields.map(|fs| fs.iter().map(String::as_str).collect());
        return eng.query_filter(right_col, Some(&pred), None, proj_refs.as_deref(), 0);
    }
    let mut keys = FxHashSet::default();
    keys.insert(key.to_string());
    let mut hits = Vec::new();
    eng.scan_by_key_set(right_col, to_field, &keys, |r| {
        hits.push(match proj_fields {
            Some(fs) => project_fields(&r, fs),
            None => r,
        });
        Ok(true)
    })?;
    Ok(hits)
}

/// `left | filter? | join [left] right on f | project? | skip* | take?` with a cold left side:
/// left rows come from [`DiskStreamCursor`] (filter pushed into the scan, no projection),
/// right rows from point reads (`to_field == "id"`) or memoized key probes (`to_field != "id"`).
struct DiskJoinCursor {
    left: DiskStreamCursor,
    right_col: String,
    on: String,
    to_field: String,
    left_join: bool,
    project: Option<Vec<String>>,
    skip_left: usize,
    take_left: Option<usize>,
    emitted: usize,
    max_expansion: usize,
    memo: FxHashMap<String, Option<Row>>,
    multi_memo: FxHashMap<String, Vec<Row>>,
    pending: VecDeque<Row>,
}

fn next_disk_join(store: &Store, cur: &mut DiskJoinCursor) -> Option<Result<Row, Error>> {
    loop {
        if cur.take_left == Some(0) {
            return None;
        }
        if let Some(row) = cur.pending.pop_front() {
            if cur.skip_left > 0 {
                cur.skip_left -= 1;
                continue;
            }
            if let Some(n) = cur.take_left.as_mut() {
                *n -= 1;
            }
            return Some(Ok(row));
        }
        let left = match next_disk_stream(store, &mut cur.left)? {
            Ok(row) => row,
            Err(e) => return Some(Err(e)),
        };
        if cur.to_field == "id" {
            let right = match left.get(&cur.on).and_then(Cell::text) {
                None => None,
                Some(key) => {
                    if !cur.memo.contains_key(key) {
                        let found = if store.is_disk_backed(&cur.right_col) {
                            match store.disk_engine_read() {
                                Some(eng) => match eng.get_by_id(&cur.right_col, key) {
                                    Ok(row) => row,
                                    Err(e) => return Some(Err(e)),
                                },
                                None => None,
                            }
                        } else {
                            store.get_by_id(&cur.right_col, key).cloned()
                        };
                        if cur.memo.len() < 8192 {
                            cur.memo.insert(key.to_string(), found.clone());
                            cur.memo.get(key).and_then(Option::as_ref)
                        } else {
                            // Avoid unbounded memo growth on high-cardinality keys.
                            cur.memo.clear();
                            cur.memo.insert(key.to_string(), found);
                            cur.memo.get(key).and_then(Option::as_ref)
                        }
                    } else {
                        cur.memo.get(key).and_then(Option::as_ref)
                    }
                }
            };
            if right.is_none() && !cur.left_join {
                continue;
            }
            cur.emitted += 1;
            if cur.emitted > cur.max_expansion {
                cur.take_left = Some(0);
                return Some(Err(Error::runtime(format!(
                    "Cartesian join expansion {} exceeds max_join_expansion quota {}",
                    cur.emitted, cur.max_expansion
                ))));
            }
            if cur.skip_left > 0 {
                cur.skip_left -= 1;
                continue;
            }
            if let Some(n) = cur.take_left.as_mut() {
                *n -= 1;
            }
            let row = match &cur.project {
                Some(fields) => {
                    crate::exec::project_join_fields(&left, right, &cur.right_col, fields)
                }
                None => match right {
                    Some(r) => crate::exec::merge_join_row(left, r, &cur.right_col),
                    None => left,
                },
            };
            return Some(Ok(row));
        }

        // Non-id foreign key: match `cell_key_ref` (`Text` / `SmallText` / `Int`).
        let key_opt = left
            .get(&cur.on)
            .and_then(crate::exec::cell_key_ref)
            .map(|c| c.into_owned());
        let hits: Vec<Row> = match key_opt {
            None => Vec::new(),
            Some(ref k) => {
                if let Some(cached) = cur.multi_memo.get(k.as_str()) {
                    cached.clone()
                } else {
                    let fetched = if store.is_disk_backed(&cur.right_col) {
                        match fetch_cold_non_id_right_rows(
                            store,
                            &cur.right_col,
                            &cur.to_field,
                            k,
                            None,
                        ) {
                            Ok(rows) => rows,
                            Err(e) => {
                                cur.take_left = Some(0);
                                return Some(Err(e));
                            }
                        }
                    } else {
                        store
                            .collection(&cur.right_col)
                            .iter()
                            .filter(|r| {
                                r.get(&cur.to_field)
                                    .and_then(crate::exec::cell_key_ref)
                                    .is_some_and(|rk| rk.as_ref() == k.as_str())
                            })
                            .cloned()
                            .collect()
                    };
                    if cur.multi_memo.len() < 4096 {
                        cur.multi_memo.insert(k.clone(), fetched.clone());
                    }
                    fetched
                }
            }
        };
        if hits.is_empty() {
            if cur.left_join {
                cur.emitted += 1;
                if cur.emitted > cur.max_expansion {
                    cur.take_left = Some(0);
                    return Some(Err(Error::runtime(format!(
                        "Cartesian join expansion {} exceeds max_join_expansion quota {}",
                        cur.emitted, cur.max_expansion
                    ))));
                }
                let out = match &cur.project {
                    Some(fields) => {
                        crate::exec::project_join_fields(&left, None, &cur.right_col, fields)
                    }
                    None => left,
                };
                cur.pending.push_back(out);
            }
            continue;
        }
        for (i, r) in hits.iter().enumerate() {
            cur.emitted += 1;
            if cur.emitted > cur.max_expansion {
                cur.take_left = Some(0);
                return Some(Err(Error::runtime(format!(
                    "Cartesian join expansion {} exceeds max_join_expansion quota {}",
                    cur.emitted, cur.max_expansion
                ))));
            }
            let out = match &cur.project {
                Some(fields) => {
                    crate::exec::project_join_fields(&left, Some(r), &cur.right_col, fields)
                }
                None => {
                    let base = if i + 1 == hits.len() {
                        left.clone()
                    } else {
                        left.clone()
                    };
                    crate::exec::merge_join_row(base, r, &cur.right_col)
                }
            };
            cur.pending.push_back(out);
        }
    }
}

fn try_disk_join(db: &CursorDb<'_>, q: &Query) -> Option<DiskJoinCursor> {
    let Source::Collection(name) = &q.source else {
        return None;
    };
    let (store, catalog) = db.store_catalog();
    let mut filter: Option<&Pred> = None;
    let mut join: Option<(bool, &str, &str)> = None;
    let mut project: Option<Vec<String>> = None;
    let mut skip_n: usize = 0;
    let mut take_n: Option<Option<i64>> = None;
    for step in &q.steps {
        match step {
            Step::Filter(p) if join.is_none() && filter.is_none() => filter = Some(p),
            Step::Join {
                left,
                collection,
                on,
            } if join.is_none() => join = Some((*left, collection.as_str(), on.as_str())),
            Step::Project(f) if join.is_some() && project.is_none() && skip_n == 0 && take_n.is_none() => {
                project = Some(field_names(f));
            }
            Step::Skip { n } if join.is_some() && take_n.is_none() => {
                skip_n = skip_n.saturating_add((*n).max(0) as usize);
            }
            Step::Take { n } if join.is_some() && take_n.is_none() => take_n = Some(*n),
            _ => return None,
        }
    }
    let (left_join, right_col, on) = join?;
    if !store.is_disk_backed(name) {
        return None;
    }
    let to_field = catalog
        .find_fk(name, on, right_col)
        .map(|fk| fk.to_field.as_str())
        .unwrap_or("id");
    Some(DiskJoinCursor {
        left: DiskStreamCursor {
            collection: name.clone(),
            pred: filter.cloned(),
            project: None,
            skip_left: 0,
            take_left: None,
            now: now_ms(),
            pos: crate::disk::DiskScanPos::default(),
            pending: VecDeque::new(),
        },
        right_col: right_col.to_string(),
        on: on.to_string(),
        to_field: to_field.to_string(),
        left_join,
        project,
        skip_left: skip_n,
        take_left: match take_n {
            Some(Some(n)) => Some(n.max(0) as usize),
            Some(None) => None,
            None => Some(50),
        },
        emitted: 0,
        max_expansion: db.inner().quotas.max_join_expansion,
        memo: FxHashMap::default(),
        multi_memo: FxHashMap::default(),
        pending: VecDeque::new(),
    })
}

fn next_disk_stream(store: &Store, cur: &mut DiskStreamCursor) -> Option<Result<Row, Error>> {
    loop {
        if cur.take_left == Some(0) {
            return None;
        }
        if let Some(row) = cur.pending.pop_front() {
            if cur.skip_left > 0 {
                cur.skip_left -= 1;
                continue;
            }
            if let Some(n) = cur.take_left.as_mut() {
                *n -= 1;
            }
            return Some(Ok(match &cur.project {
                // Absent fields come back as nulls, as on the in-memory path.
                Some(fields) => project_fields(&row, fields),
                None => row,
            }));
        }
        let eng = store.disk_engine_read()?;
        let want = cur
            .take_left
            .map_or(DISK_STREAM_CHUNK, |n| n.saturating_add(cur.skip_left))
            .clamp(1, DISK_STREAM_CHUNK);
        let fields: Option<Vec<&str>> = cur
            .project
            .as_ref()
            .map(|fs| fs.iter().map(String::as_str).collect());
        let chunk = eng.scan_chunk(
            &cur.collection,
            cur.pred.as_ref(),
            fields.as_deref(),
            cur.now,
            &mut cur.pos,
            want,
        );
        match chunk {
            Ok(rows) if rows.is_empty() => return None,
            Ok(rows) => cur.pending.extend(rows),
            Err(e) => {
                cur.take_left = Some(0);
                return Some(Err(e));
            }
        }
    }
}

struct LazyCursor {
    collection: String,
    source: RowSource,
    pos: usize,
    pred_slots: Option<OwnedPredSlots>,
    need_filter: bool,
    project: Option<Arc<[String]>>,
    projector: Option<OwnedRowProjector>,
    skip_left: usize,
    take_left: Option<usize>,
    now: i64,
}

impl LazyCursor {
    #[allow(clippy::too_many_arguments)]
    fn new(
        collection: String,
        source: RowSource,
        pred: Option<&Pred>,
        need_filter: bool,
        project: Option<Arc<[String]>>,
        skip_left: usize,
        take_left: Option<usize>,
        now: i64,
    ) -> Self {
        let pred_slots = if need_filter {
            pred.map(OwnedPredSlots::new)
        } else {
            None
        };
        let projector = project.as_deref().map(OwnedRowProjector::new);
        Self {
            collection,
            source,
            pos: 0,
            pred_slots,
            need_filter,
            project,
            projector,
            skip_left,
            take_left,
            now,
        }
    }
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
    left_projector: Option<OwnedRowProjector>,
    right_on_disk: bool,
    right_proj_fields: Option<Vec<String>>,
    disk_right_memo: FxHashMap<String, Option<Row>>,
    multi_right_memo: FxHashMap<String, Vec<Row>>,
    emitted: usize,
    max_expansion: usize,
    skip_left: usize,
    take_left: Option<usize>,
    pending: VecDeque<Row>,
}

enum UserProbe<'a> {
    Borrowed(&'a FxHashMap<Arc<str>, usize>),
    Owned(FxHashMap<Arc<str>, usize>),
}

impl UserProbe<'_> {
    #[inline]
    fn map(&self) -> &FxHashMap<Arc<str>, usize> {
        match self {
            Self::Borrowed(m) => m,
            Self::Owned(m) => m,
        }
    }
}

/// Columnar left + user-index probe; emits compact 3-field rows.
struct LazyJoinSoa<'a> {
    source: RowSource,
    pos: usize,
    left_join: bool,
    /// When set, filter `orders.total > min` without row maps.
    total_gt: Option<f64>,
    orders_id: &'a [Arc<str>],
    orders_uid: &'a [Arc<str>],
    orders_total: &'a [f64],
    /// Rows behind the SoA buffers: `total` is read from them to keep its stored type.
    orders_rows: &'a [Row],
    total_slot: crate::store::row::FieldSlot<'static>,
    users_email: &'a [Arc<str>],
    probe: UserProbe<'a>,
    skip_left: usize,
    take_left: Option<usize>,
    fields: Arc<[String]>,
    /// Pre-seeded row with sorted keys `"id"`, `"total"`, `"users.email"` for zero-search cloning.
    template_row: Row,
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

        #[cfg(feature = "gpu")]
        if let Some(stream) = try_gpu_stream(&db, q) {
            return Ok(Self {
                db,
                state: CursorState::GpuStream(stream),
            });
        }
        if let Some(join) = try_disk_join(&db, q) {
            return Ok(Self {
                db,
                state: CursorState::DiskJoin(join),
            });
        }
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
        if let Some(state) = try_lazy_search(&db, q)? {
            return Ok(Self { db, state });
        }
        if let Some(state) = try_lazy_hop(&db, q)? {
            return Ok(Self { db, state });
        }
        if let Some(lazy) = try_lazy_simple(&db, q)? {
            return Ok(Self {
                db,
                state: CursorState::Lazy(lazy),
            });
        }
        if let Some(stream) = try_disk_stream(&db, q) {
            return Ok(Self {
                db,
                state: CursorState::DiskStream(stream),
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
        match &self.state {
            #[cfg(feature = "gpu")]
            CursorState::GpuStream(_) => true,
            CursorState::Lazy(_)
            | CursorState::LazyJoin(_)
            | CursorState::LazyJoinSoa(_)
            | CursorState::DiskStream(_)
            | CursorState::DiskIds(_)
            | CursorState::DiskJoin(_) => true,
            _ => false,
        }
    }

    /// Pull a compact projected row. Projection schemas are shared across rows;
    /// non-projected or buffered plans fall back to converting the regular row.
    pub fn next_projected(&mut self) -> Option<Result<ProjectedRow, Error>> {
        let native = matches!(&self.state, CursorState::Lazy(lazy) if lazy.project.is_some())
            || matches!(self.state, CursorState::LazyJoinSoa(_));
        #[cfg(feature = "gpu")]
        let native = native
            || matches!(&self.state, CursorState::GpuStream(stream) if stream.project.is_some());
        if !native {
            return self.next().map(|row| row.map(ProjectedRow::from_row));
        }

        let (store, _) = self.db.store_catalog();
        let item = match &mut self.state {
            #[cfg(feature = "gpu")]
            CursorState::GpuStream(stream) => next_gpu_stream(&self.db, stream).map(|row| {
                row.map(|row| {
                    let fields = stream.project.as_ref().expect("native projection").clone();
                    let cells = fields
                        .iter()
                        .map(|name| row.get(name).cloned().unwrap_or(Cell::Null))
                        .collect();
                    ProjectedRow { fields, cells }
                })
            }),
            CursorState::Lazy(lazy) => next_lazy_projected(store, lazy),
            CursorState::LazyJoinSoa(join) => next_lazy_join_soa_projected(join),
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
            #[cfg(feature = "gpu")]
            CursorState::GpuStream(stream) => next_gpu_stream(&self.db, stream),
            CursorState::Lazy(lazy) => next_lazy(store, lazy),
            CursorState::DiskStream(stream) => next_disk_stream(store, stream),
            CursorState::DiskIds(ids) => next_disk_ids(store, ids),
            CursorState::DiskJoin(join) => next_disk_join(store, join),
            CursorState::LazyJoin(join) => next_lazy_join(store, join),
            CursorState::LazyJoinSoa(join) => next_lazy_join_soa(join),
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
    let out = if let Some(proj) = lazy.projector.as_mut() {
        proj.project(row)
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
    let fields = Arc::clone(lazy.project.as_ref().expect("projected state checked"));
    let cells = match lazy.projector.as_mut() {
        Some(proj) => proj.project_cells(row),
        None => fields
            .iter()
            .map(|f| row.get(f).map(Cell::clone_for_borrow).unwrap_or(Cell::Null))
            .collect(),
    };
    Some(Ok(ProjectedRow { fields, cells }))
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
            && let Some(slots) = lazy.pred_slots.as_mut()
            && !slots.eval(row, lazy.now)
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

        let left_row = if let Some(proj) = join.left_projector.as_mut() {
            next_lazy_raw_projected(store, &mut join.left, proj)?
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
            join.emitted += 1;
            if join.emitted > join.max_expansion {
                join.take_left = Some(0);
                return Some(Err(Error::runtime(format!(
                    "Cartesian join expansion {} exceeds max_join_expansion quota {}",
                    join.emitted, join.max_expansion
                ))));
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

        if join.to_field == "id" && join.right_on_disk {
            let owned_right: Option<Row> = match key {
                None => None,
                Some(k) => {
                    if let Some(cached) = join.disk_right_memo.get(k) {
                        cached.clone()
                    } else {
                        let Some(eng) = store.disk_engine_read() else {
                            return Some(Err(Error::runtime("disk engine unavailable")));
                        };
                        let fetched = match &join.right_proj_fields {
                            Some(rfs) => {
                                let refs: SmallVec<[&str; 8]> =
                                    rfs.iter().map(String::as_str).collect();
                                eng.get_projected_by_id(&join.right_col, k, &refs)
                            }
                            None => eng
                                .get_arc_by_id(&join.right_col, k)
                                .map(|opt| opt.map(|arc| (*arc).clone())),
                        };
                        let row_opt = match fetched {
                            Ok(r) => r,
                            Err(e) => return Some(Err(e)),
                        };
                        if join.disk_right_memo.len() < 8192 {
                            join.disk_right_memo.insert(k.to_string(), row_opt.clone());
                        }
                        row_opt
                    }
                }
            };
            if owned_right.is_none() && !join.left_join {
                continue;
            }
            join.emitted += 1;
            if join.emitted > join.max_expansion {
                join.take_left = Some(0);
                return Some(Err(Error::runtime(format!(
                    "Cartesian join expansion {} exceeds max_join_expansion quota {}",
                    join.emitted, join.max_expansion
                ))));
            }
            let out = if let Some(fields) = join.project.as_deref() {
                project_join_fields(&left_row, owned_right.as_ref(), &join.right_col, fields)
            } else if let Some(r) = owned_right {
                let mut merged = left_row;
                for (k, v) in r {
                    crate::exec::insert_prefixed(&mut merged, &join.right_col, &k, v);
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

        let right = if join.to_field == "id" {
            key.and_then(|k| store.get_by_id(&join.right_col, k))
        } else {
            None
        };

        if join.to_field == "id" {
            if right.is_none() && !join.left_join {
                continue;
            }
            join.emitted += 1;
            if join.emitted > join.max_expansion {
                join.take_left = Some(0);
                return Some(Err(Error::runtime(format!(
                    "Cartesian join expansion {} exceeds max_join_expansion quota {}",
                    join.emitted, join.max_expansion
                ))));
            }
            let out = if let Some(fields) = join.project.as_deref() {
                project_join_fields(&left_row, right, &join.right_col, fields)
            } else if let Some(r) = right {
                let mut merged = left_row;
                for (k, v) in r {
                    crate::exec::insert_prefixed(&mut merged, &join.right_col, k, v.clone());
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

        // Non-id join key (cold or hot right side): match `cell_key_ref` (`Text`/`SmallText`/`Int`).
        let non_id_key = left_row
            .get(&join.on)
            .and_then(crate::exec::cell_key_ref)
            .map(|c| c.into_owned());
        let hits: Vec<Row> = match non_id_key {
            None => Vec::new(),
            Some(ref k) => {
                if let Some(cached) = join.multi_right_memo.get(k.as_str()) {
                    cached.clone()
                } else {
                    let fetched = if join.right_on_disk {
                        match fetch_cold_non_id_right_rows(
                            store,
                            &join.right_col,
                            &join.to_field,
                            k,
                            join.right_proj_fields.as_deref(),
                        ) {
                            Ok(rows) => rows,
                            Err(e) => {
                                join.take_left = Some(0);
                                return Some(Err(e));
                            }
                        }
                    } else {
                        store
                            .collection(&join.right_col)
                            .iter()
                            .filter(|r| {
                                r.get(&join.to_field)
                                    .and_then(crate::exec::cell_key_ref)
                                    .is_some_and(|rk| rk.as_ref() == k.as_str())
                            })
                            .cloned()
                            .collect()
                    };
                    if join.multi_right_memo.len() < 4096 {
                        join.multi_right_memo.insert(k.clone(), fetched.clone());
                    }
                    fetched
                }
            }
        };

        if hits.is_empty() {
            if join.left_join {
                join.emitted += 1;
                if join.emitted > join.max_expansion {
                    join.take_left = Some(0);
                    return Some(Err(Error::runtime(format!(
                        "Cartesian join expansion {} exceeds max_join_expansion quota {}",
                        join.emitted, join.max_expansion
                    ))));
                }
                let out = match join.project.as_deref() {
                    Some(fields) => project_join_fields(&left_row, None, &join.right_col, fields),
                    None => left_row,
                };
                join.pending.push_back(out);
            }
            continue;
        }

        for r in &hits {
            join.emitted += 1;
            if join.emitted > join.max_expansion {
                join.take_left = Some(0);
                return Some(Err(Error::runtime(format!(
                    "Cartesian join expansion {} exceeds max_join_expansion quota {}",
                    join.emitted, join.max_expansion
                ))));
            }
            let out = match join.project.as_deref() {
                Some(fields) => project_join_fields(&left_row, Some(r), &join.right_col, fields),
                None => crate::exec::merge_join_row(left_row.clone(), r, &join.right_col),
            };
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
            && let Some(slots) = lazy.pred_slots.as_mut()
            && !slots.eval(row, lazy.now)
        {
            continue;
        }
        return Some(Ok(row.clone()));
    }
}

fn next_lazy_raw_projected(
    store: &Store,
    lazy: &mut LazyCursor,
    projector: &mut OwnedRowProjector,
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
            && let Some(slots) = lazy.pred_slots.as_mut()
            && !slots.eval(row, lazy.now)
        {
            continue;
        }
        return Some(Ok(projector.project(row)));
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
        if let Some((uses, covered)) = crate::index::pick_index_with_coverage(catalog, name, pred) {
            if let Some(idxs) = store.index_seek(name, &uses, now) {
                return (RowSource::Idxs(idxs), !covered);
            }
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
    // Cold collections stream from the disk engine (see `try_disk_stream`).
    if store.is_disk_backed(name) {
        return Ok(None);
    }
    let now = now_ms();
    let take_left = match take_n {
        Some(Some(n)) => Some(n.max(0) as usize),
        Some(None) => None,
        None => Some(50),
    };
    let (source, need_filter) = build_left_source(store, catalog, name, filter, now);

    Ok(Some(LazyCursor::new(
        name.clone(),
        source,
        filter,
        need_filter,
        project.map(Into::into),
        skip_n,
        take_left,
        now,
    )))
}

/// `col | filter? | project? | skip* | take?` over a cold collection.
fn try_disk_stream(db: &CursorDb<'_>, q: &Query) -> Option<DiskStreamCursor> {
    let Source::Collection(name) = &q.source else {
        return None;
    };
    let (store, _) = db.store_catalog();
    if !store.is_disk_backed(name) {
        return None;
    }
    let mut filter: Option<&Pred> = None;
    let mut project: Option<Vec<String>> = None;
    let mut skip_n: usize = 0;
    let mut take_n: Option<Option<i64>> = None;
    for step in &q.steps {
        match step {
            // The filter must come first: it runs inside the scan.
            Step::Filter(p)
                if filter.is_none() && project.is_none() && skip_n == 0 && take_n.is_none() =>
            {
                filter = Some(p);
            }
            Step::Project(f) if project.is_none() && skip_n == 0 && take_n.is_none() => {
                // Nested paths need the row evaluator.
                if f.iter().any(|field| field.leaf().is_none()) {
                    return None;
                }
                project = Some(field_names(f));
            }
            Step::Skip { n } if take_n.is_none() => {
                skip_n = skip_n.saturating_add((*n).max(0) as usize);
            }
            Step::Take { n } if take_n.is_none() => take_n = Some(*n),
            _ => return None,
        }
    }
    Some(DiskStreamCursor {
        collection: name.clone(),
        pred: filter.cloned(),
        project,
        skip_left: skip_n,
        take_left: match take_n {
            Some(Some(n)) => Some(n.max(0) as usize),
            Some(None) => None,
            None => Some(50),
        },
        now: now_ms(),
        pos: crate::disk::DiskScanPos::default(),
        pending: VecDeque::new(),
    })
}

/// `col | search vec|lex|hybrid … | project? | skip* | take?`.
fn try_lazy_search<'a>(db: &CursorDb<'a>, q: &Query) -> Result<Option<CursorState<'a>>, Error> {
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
                if mode.is_some() || project.is_some() || skip_n > 0 || saw_take {
                    return Ok(None);
                }
                mode = Some(*m);
                query = Some(qq.as_str());
            }
            Step::Project(f) => {
                if project.is_some() || f.iter().any(|field| field.leaf().is_none()) {
                    return Ok(None);
                }
                project = Some(field_names(f));
            }
            Step::Skip { n } => {
                if mode.is_none() || saw_take {
                    return Ok(None);
                }
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

    let take_left = match take_n {
        Some(Some(n)) => Some(n.max(0) as usize),
        Some(None) => None,
        None => Some(50),
    };
    let rank_limit = take_left.map(|n| n.saturating_add(skip_n));

    if store.is_disk_backed(name) {
        let Some(ids) = db
            .inner()
            .cold_search_ranked_ids(name, mode, query, rank_limit)?
        else {
            return Ok(None);
        };
        return Ok(Some(CursorState::DiskIds(DiskIdsCursor {
            collection: name.clone(),
            resolve_doc_uri: false,
            fallback_col: None,
            ids,
            pos: 0,
            resolved_left: None,
            project: project.map(Into::into),
            skip_left: skip_n,
            take_left,
        })));
    }

    if mode == SearchMode::Vec {
        let col = store.collection(name);
        let ranked = db.vector_search_indices(col, query, rank_limit)?;
        let idxs: Vec<usize> = ranked.into_iter().map(|(_, i)| i).collect();
        return Ok(Some(CursorState::Lazy(LazyCursor::new(
            name.clone(),
            RowSource::Idxs(idxs),
            None,
            false,
            project.map(Into::into),
            skip_n,
            take_left,
            now_ms(),
        ))));
    }

    if !store.fts_indexed(name) {
        return Ok(None);
    }

    let idxs = db.search_fts_indices(name, mode, query, rank_limit)?;

    Ok(Some(CursorState::Lazy(LazyCursor::new(
        name.clone(),
        RowSource::Idxs(idxs),
        None,
        false,
        project.map(Into::into),
        skip_n,
        take_left,
        now_ms(),
    ))))
}

/// `col | filter? | hop rel [1..=3]` `| project? | skip* | take?`.
fn try_lazy_hop<'a>(db: &CursorDb<'a>, q: &Query) -> Result<Option<CursorState<'a>>, Error> {
    let Source::Collection(name) = &q.source else {
        return Ok(None);
    };
    let mut filter: Option<&Pred> = None;
    let mut hop_rel: Option<(&str, i64)> = None;
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
                let d = depth.unwrap_or(1);
                if !(1..=3).contains(&d) {
                    return Ok(None);
                }
                hop_rel = Some((rel.as_str(), d));
            }
            Step::Project(f) => {
                if hop_rel.is_none()
                    || project.is_some()
                    || f.iter().any(|field| field.leaf().is_none())
                {
                    return Ok(None);
                }
                project = Some(field_names(f));
            }
            Step::Skip { n } => {
                if hop_rel.is_none() || saw_take {
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

    let Some((rel, hop_depth)) = hop_rel else {
        return Ok(None);
    };

    let (store, catalog) = db.store_catalog();
    let now = now_ms();

    let (edge_rel, reverse) = match catalog.rel(rel) {
        Some(r) if let Some(of) = &r.reverse_of => (of.as_str(), true),
        _ => (rel, false),
    };

    let take_left = match take_n {
        Some(Some(n)) => Some(n.max(0) as usize),
        Some(None) => None,
        None => Some(50),
    };

    // Cold hop: resolve seed keys without materializing the source table, walk in-memory
    // edges across `1..=3` hops, and pull reached rows lazily by `id` / `uri`.
    if store.is_disk_backed(name) || store.is_disk_backed("docs") {
        #[cfg(feature = "gpu")]
        if db.inner().gpu().is_some() {
            return Ok(None);
        }
        let mut frontier: Vec<Arc<str>> = Vec::new();
        let mut seen: FxHashSet<Arc<str>> = FxHashSet::default();
        if store.is_disk_backed(name) {
            let Some(eng) = store.disk_engine_read() else {
                return Ok(None);
            };
            let cols: &[&str] = if name == "docs" {
                &["id", "uri"]
            } else {
                &["id"]
            };
            let seed_rows = eng.query_filter(name, filter, None, Some(cols), now)?;
            for r in &seed_rows {
                if let Some(id) = r.get("id").and_then(Cell::text_shared)
                    && seen.insert(Arc::clone(&id))
                {
                    frontier.push(id);
                }
                if let Some(uri) = r.get("uri").and_then(Cell::text_shared)
                    && seen.insert(Arc::clone(&uri))
                {
                    frontier.push(uri);
                }
            }
        } else {
            let (source, need_filter) = build_left_source(store, catalog, name, filter, now);
            collect_seed_keys_arc(
                store,
                name,
                &source,
                filter,
                need_filter,
                now,
                &mut frontier,
                &mut seen,
            );
        }
        let rel_arc: Arc<str> = Arc::from(edge_rel);
        let mut reached: BTreeSet<Arc<str>> = BTreeSet::new();
        for _ in 0..hop_depth {
            let mut next: Vec<Arc<str>> = Vec::new();
            for src in &frontier {
                store.for_each_neighbor_arc(&rel_arc, src, reverse, |dst| {
                    if seen.insert(Arc::clone(dst)) {
                        next.push(Arc::clone(dst));
                        reached.insert(Arc::clone(dst));
                    }
                });
            }
            if seen.len() > db.inner().quotas.max_graph_expansion {
                return Err(Error::runtime(format!(
                    "graph expansion count {} exceeds max_graph_expansion quota {}",
                    seen.len(),
                    db.inner().quotas.max_graph_expansion
                )));
            }
            if reached.len() >= 300 {
                break;
            }
            frontier = next;
            if frontier.is_empty() {
                break;
            }
        }
        let ids: Vec<Arc<str>> = reached.into_iter().collect();
        return Ok(Some(CursorState::DiskIds(DiskIdsCursor {
            collection: "docs".to_string(),
            resolve_doc_uri: true,
            fallback_col: (name != "docs").then(|| name.clone()),
            ids,
            pos: 0,
            resolved_left: Some(300),
            project: project.map(Into::into),
            skip_left: skip_n,
            take_left,
        })));
    }

    let (source, need_filter) = build_left_source(store, catalog, name, filter, now);

    let out_collection = if name == "docs" || store.collections.contains_key("docs") {
        // hop resolves via find_doc_key → docs first
        "docs"
    } else {
        name.as_str()
    };

    #[cfg(feature = "gpu")]
    if db.inner().gpu().is_some() {
        if hop_depth != 1 {
            return Ok(None);
        }
        let mut frontier: BTreeSet<String> = BTreeSet::new();
        collect_gpu_seed_keys(
            db.inner(),
            name,
            &source,
            filter,
            need_filter,
            now,
            &mut frontier,
        )?;
        let reached: BTreeSet<String> = db
            .inner()
            .gpu_hop_keys(&frontier, edge_rel, reverse)?
            .into_iter()
            .collect();
        let mut idxs = Vec::new();
        for key in reached {
            if out_collection != name
                && store.find_doc_key(&key).is_none()
                && store.row_index(name, &key).is_some()
            {
                return Ok(None);
            }
            if let Some(i) = neighbor_idx(store, name, out_collection, &key) {
                idxs.push(i);
            }
            if idxs.len() >= 300 {
                break;
            }
        }
        return Ok(Some(CursorState::Lazy(LazyCursor::new(
            out_collection.to_string(),
            RowSource::Idxs(idxs),
            None,
            false,
            project.map(Into::into),
            skip_n,
            take_left,
            now,
        ))));
    }

    let mut frontier: Vec<Arc<str>> = Vec::new();
    let mut seen: FxHashSet<Arc<str>> = FxHashSet::default();
    collect_seed_keys_arc(
        store,
        name,
        &source,
        filter,
        need_filter,
        now,
        &mut frontier,
        &mut seen,
    );
    let rel_arc: Arc<str> = Arc::from(edge_rel);
    let mut reached: BTreeSet<Arc<str>> = BTreeSet::new();
    for _ in 0..hop_depth {
        let mut next: Vec<Arc<str>> = Vec::new();
        for src in &frontier {
            store.for_each_neighbor_arc(&rel_arc, src, reverse, |dst| {
                if seen.insert(Arc::clone(dst)) {
                    next.push(Arc::clone(dst));
                    reached.insert(Arc::clone(dst));
                }
            });
        }
        if seen.len() > db.inner().quotas.max_graph_expansion {
            return Err(Error::runtime(format!(
                "graph expansion count {} exceeds max_graph_expansion quota {}",
                seen.len(),
                db.inner().quotas.max_graph_expansion
            )));
        }
        if reached.len() >= 300 {
            break;
        }
        frontier = next;
        if frontier.is_empty() {
            break;
        }
    }

    let mut idxs = Vec::new();
    for key in reached {
        // A lazy source indexes one collection. Eager hop can resolve a mix
        // of docs and primary rows; primary indices cannot index docs.
        if out_collection != name
            && store.find_doc_key(&key).is_none()
            && store.row_index(name, &key).is_some()
        {
            return Ok(None);
        }
        if let Some(i) = neighbor_idx(store, name, out_collection, &key) {
            idxs.push(i);
        }
        if idxs.len() >= 300 {
            break;
        }
    }

    Ok(Some(CursorState::Lazy(LazyCursor::new(
        out_collection.to_string(),
        RowSource::Idxs(idxs),
        None,
        false,
        project.map(Into::into),
        skip_n,
        take_left,
        now,
    ))))
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

fn collect_seed_keys_arc(
    store: &Store,
    collection: &str,
    source: &RowSource,
    pred: Option<&Pred>,
    need_filter: bool,
    now: i64,
    out: &mut Vec<Arc<str>>,
    seen: &mut FxHashSet<Arc<str>>,
) {
    let rows = store.collection(collection);
    let mut push_cell = |c: Option<&Cell>| match c {
        Some(Cell::Text(a)) if !a.is_empty() => {
            if seen.insert(Arc::clone(a)) {
                out.push(Arc::clone(a));
            }
        }
        Some(other) => {
            if let Some(s) = other.text().filter(|s| !s.is_empty()) {
                let a: Arc<str> = Arc::from(s);
                if seen.insert(Arc::clone(&a)) {
                    out.push(a);
                }
            }
        }
        None => {}
    };
    let mut push = |row: &Row| {
        push_cell(row.get("id"));
        push_cell(row.get("uri"));
    };
    let mut slots = pred.map(PredSlots::new);
    match source {
        RowSource::Idxs(idxs) => {
            for &i in idxs {
                let Some(row) = rows.get(i) else {
                    continue;
                };
                if need_filter
                    && let Some(slots) = slots.as_mut()
                    && !slots.eval(row, now)
                {
                    continue;
                }
                push(row);
            }
        }
        RowSource::Scan { len } => {
            for i in 0..*len {
                let Some(row) = rows.get(i) else {
                    continue;
                };
                if let Some(slots) = slots.as_mut()
                    && !slots.eval(row, now)
                {
                    continue;
                }
                push(row);
            }
        }
    }
}

impl<'a> LazyJoinSoa<'a> {
    #[inline]
    fn next_matched(&mut self) -> Option<(&'a Arc<str>, Cell, Option<&'a Arc<str>>)> {
        let probe = self.probe.map();
        let n = self.orders_id.len();

        loop {
            if self.take_left == Some(0) {
                return None;
            }
            let idx = match &self.source {
                RowSource::Idxs(idxs) => {
                    if self.pos >= idxs.len() {
                        return None;
                    }
                    let i = idxs[self.pos];
                    self.pos += 1;
                    i
                }
                RowSource::Scan { len } => {
                    if self.pos >= *len {
                        return None;
                    }
                    let i = self.pos;
                    self.pos += 1;
                    i
                }
            };
            if idx >= n {
                continue;
            }
            debug_assert_eq!(self.orders_id.len(), self.orders_uid.len());
            debug_assert_eq!(self.orders_id.len(), self.orders_total.len());
            // SAFETY: `LazyJoinSoa` is constructed only after `orders_soa_ready()`;
            // `idx < orders_id.len()` was checked above, and `'a` borrows the immutable `Store`.
            let (order_id, uid, total) = unsafe {
                (
                    self.orders_id.get_unchecked(idx),
                    self.orders_uid.get_unchecked(idx).as_ref(),
                    *self.orders_total.get_unchecked(idx),
                )
            };
            if let Some(min) = self.total_gt
                && total.partial_cmp(&min) != Some(std::cmp::Ordering::Greater)
            {
                continue;
            }
            let right = probe.get(uid).copied();
            if right.is_none() && !self.left_join {
                continue;
            }
            if self.skip_left > 0 {
                self.skip_left -= 1;
                continue;
            }
            if let Some(t) = self.take_left.as_mut() {
                *t = t.saturating_sub(1);
            }
            // SAFETY: probe indices point into aligned `users_email` after `users_soa_ready()`.
            let email = right.map(|ui| {
                debug_assert!(ui < self.users_email.len());
                unsafe { self.users_email.get_unchecked(ui) }
            });
            let total = crate::exec::order_total_cell(
                &mut self.total_slot,
                self.orders_rows,
                self.orders_total,
                idx,
            );
            return Some((order_id, total, email));
        }
    }
}

fn next_lazy_join_soa(join: &mut LazyJoinSoa<'_>) -> Option<Result<Row, Error>> {
    let (order_id, total, email) = join.next_matched()?;
    let mut row = join.template_row.clone();
    let mut vals = row.values_mut();
    // Sorted key order: "id" < "total" < "users.email".
    if let Some(slot) = vals.next() {
        *slot = cell_from_arc_inline(order_id);
    }
    if let Some(slot) = vals.next() {
        *slot = total;
    }
    if let Some(slot) = vals.next() {
        *slot = match email {
            Some(e) => cell_from_arc_inline(e),
            None => Cell::Null,
        };
    }
    Some(Ok(row))
}

#[inline]
fn next_lazy_join_soa_projected(join: &mut LazyJoinSoa<'_>) -> Option<Result<ProjectedRow, Error>> {
    let (order_id, total, email) = join.next_matched()?;
    let cells = smallvec![
        cell_from_arc_inline(order_id),
        match email {
            Some(e) => cell_from_arc_inline(e),
            None => Cell::Null,
        },
        total,
    ];
    Some(Ok(ProjectedRow {
        fields: Arc::clone(&join.fields),
        cells,
    }))
}

fn is_orders_users_id_email_total(fields: &[String]) -> bool {
    fields.len() == 3 && fields[0] == "id" && fields[1] == "users.email" && fields[2] == "total"
}

fn try_lazy_join_soa<'a>(db: &CursorDb<'a>, q: &Query) -> Result<Option<LazyJoinSoa<'a>>, Error> {
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

    let orders_id = store.orders_id();
    let orders_uid = store.orders_user_id();
    let orders_total = store.orders_total();
    let users_id = store.users_id();
    let users_email = store.users_email();
    let n = orders_id.len();
    let source = RowSource::Scan { len: n };

    let probe = if let Some(by_id) = store.by_id.get("users")
        && by_id.len() == users_id.len()
    {
        UserProbe::Borrowed(by_id)
    } else {
        let mut map: FxHashMap<Arc<str>, usize> = FxHashMap::default();
        map.reserve(users_id.len());
        for (i, id) in users_id.iter().enumerate() {
            map.insert(Arc::clone(id), i);
        }
        UserProbe::Owned(map)
    };

    let mut template_row = crate::store::acquire_row();
    template_row.insert(String::from("id"), Cell::Null);
    template_row.insert(String::from("total"), Cell::Null);
    template_row.insert(String::from("users.email"), Cell::Null);

    Ok(Some(LazyJoinSoa {
        source,
        pos: 0,
        left_join,
        total_gt,
        orders_id,
        orders_uid,
        orders_total,
        orders_rows: store.collection("orders"),
        total_slot: crate::store::row::FieldSlot::new("total"),
        users_email,
        probe,
        skip_left: skip_n,
        take_left,
        fields: vec![
            String::from("id"),
            String::from("users.email"),
            String::from("total"),
        ]
        .into(),
        template_row,
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
    if store.is_disk_backed(name) {
        return Ok(None);
    }
    let right_on_disk = store.is_disk_backed(&right_col);
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

    // Push right-side column projection into cold reads for both `id` and non-`id` joins!
    let (right_probe, join_plan, left_projector, right_proj_fields) =
        if let Some(ref fields) = project {
            let (right_fields, plan) = plan_join_fields(&right_col, fields);
            let mut left_fields: Vec<String> = plan
                .iter()
                .filter_map(|p| match p {
                    JoinFieldPlan::Left(name) => Some(name.clone()),
                    JoinFieldPlan::Right { .. } => None,
                })
                .collect();
            if !left_fields.iter().any(|f| f == &on) {
                left_fields.push(on.clone());
            }
            let r_proj = right_on_disk.then_some(right_fields);
            (
                None,
                None,
                Some(OwnedRowProjector::new(&left_fields)),
                r_proj,
            )
        } else {
            (None, None, None, None)
        };

    Ok(Some(LazyJoinCursor {
        left: LazyCursor::new(
            name.clone(),
            source,
            filter,
            need_filter,
            None,
            0,
            None,
            now,
        ),
        right_col,
        on,
        to_field,
        left_join,
        project,
        right_probe,
        join_plan,
        left_projector,
        right_on_disk,
        right_proj_fields,
        disk_right_memo: FxHashMap::default(),
        multi_right_memo: FxHashMap::default(),
        emitted: 0,
        max_expansion: db.inner().quotas.max_join_expansion,
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
        self.ensure()?;
        db.cursor(self.query())
    }

    pub fn cursor_read<'a>(&self, db: &'a ReadDb) -> Result<QueryCursor<'a>, Error> {
        self.ensure()?;
        db.cursor(self.query())
    }
}

#[cfg(feature = "gpu")]
struct GpuStreamCursor {
    collection: String,
    pos: usize,
    len: usize,
    batch_rows: usize,
    steps: Vec<GpuStreamStep>,
    pending: VecDeque<Row>,
    stopped: bool,
    project: Option<Arc<[String]>>,
    now: i64,
}
#[cfg(feature = "gpu")]
enum GpuStreamStep {
    Filter(Pred),
    Project(Vec<String>),
    Join {
        left: bool,
        collection: String,
        on: String,
    },
    Skip(usize),
    Take(Option<usize>),
}
#[cfg(feature = "gpu")]
fn try_gpu_stream(db: &CursorDb<'_>, query: &Query) -> Option<GpuStreamCursor> {
    db.inner().gpu()?;
    let Source::Collection(collection) = &query.source else {
        return None;
    };
    let mut steps = Vec::new();
    let mut project = None;
    let mut explicit_take = false;
    let mut stopped = false;
    let mut batch_rows = 1024;
    for step in &query.steps {
        steps.push(match step {
            Step::Filter(pred) => GpuStreamStep::Filter(pred.clone()),
            Step::Project(fields) => {
                let names = field_names(fields);
                project = Some(Arc::from(names.clone()));
                GpuStreamStep::Project(names)
            }
            Step::Join {
                left,
                collection: right,
                on,
            } => {
                project = None;
                let to_field = db
                    .inner()
                    .catalog
                    .find_fk(collection, on, right)
                    .map(|fk| fk.to_field.as_str())
                    .unwrap_or("id");
                if to_field != "id" {
                    batch_rows = 1;
                }
                GpuStreamStep::Join {
                    left: *left,
                    collection: right.clone(),
                    on: on.clone(),
                }
            }
            Step::Skip { n } => GpuStreamStep::Skip((*n).max(0) as usize),
            Step::Take { n } => {
                explicit_take = true;
                let n = n.map(|n| n.max(0) as usize);
                stopped |= n == Some(0);
                GpuStreamStep::Take(n)
            }
            _ => return None,
        });
    }
    if !explicit_take {
        steps.push(GpuStreamStep::Take(Some(50)));
    }
    Some(GpuStreamCursor {
        collection: collection.clone(),
        pos: 0,
        len: db.inner().store.collection(collection).len(),
        batch_rows,
        steps,
        pending: VecDeque::new(),
        stopped,
        project,
        now: now_ms(),
    })
}
#[cfg(feature = "gpu")]
fn next_gpu_stream(db: &CursorDb<'_>, stream: &mut GpuStreamCursor) -> Option<Result<Row, Error>> {
    loop {
        if let Some(row) = stream.pending.pop_front() {
            return Some(Ok(row));
        }
        if stream.stopped || stream.pos >= stream.len {
            return None;
        }
        let end = stream.pos.saturating_add(stream.batch_rows).min(stream.len);
        let mut rows: Vec<Row> = (stream.pos..end)
            .filter_map(|i| db.inner().store.get_by_idx(&stream.collection, i).cloned())
            .collect();
        stream.pos = end;
        for step in &mut stream.steps {
            match step {
                GpuStreamStep::Filter(pred) => {
                    match db.inner().gpu_filter_mask(pred, &rows, stream.now) {
                        Ok(Some(mask)) => {
                            let mut mask = mask.into_iter();
                            rows.retain(|_| mask.next().expect("one mask per row"));
                        }
                        Ok(None) => {
                            let mut slots = PredSlots::new(pred);
                            rows.retain(|row| slots.eval(row, stream.now));
                        }
                        Err(error) => {
                            stream.stopped = true;
                            return Some(Err(error));
                        }
                    }
                }
                GpuStreamStep::Project(fields) => {
                    rows = rows.iter().map(|row| project_fields(row, fields)).collect()
                }
                GpuStreamStep::Join {
                    left,
                    collection,
                    on,
                } => {
                    match db
                        .inner()
                        .gpu_join_rows(&stream.collection, rows, collection, on, *left)
                    {
                        Ok(joined) => rows = joined,
                        Err(error) => {
                            stream.stopped = true;
                            return Some(Err(error));
                        }
                    }
                }
                GpuStreamStep::Skip(left) => {
                    let n = (*left).min(rows.len());
                    rows.drain(..n);
                    *left -= n;
                }
                GpuStreamStep::Take(Some(left)) => {
                    rows.truncate(*left);
                    *left -= rows.len();
                    if *left == 0 {
                        stream.stopped = true;
                    }
                }
                GpuStreamStep::Take(None) => {}
            }
        }
        stream.pending.extend(rows);
    }
}

#[cfg(feature = "gpu")]
fn collect_gpu_seed_keys(
    db: &Db,
    name: &str,
    source: &RowSource,
    pred: Option<&Pred>,
    need_filter: bool,
    now: i64,
    out: &mut BTreeSet<String>,
) -> Result<(), Error> {
    let len = match source {
        RowSource::Idxs(ids) => ids.len(),
        RowSource::Scan { len } => *len,
    };
    let pred = if need_filter || matches!(source, RowSource::Scan { .. }) {
        pred
    } else {
        None
    };
    for start in (0..len).step_by(1024) {
        let rows: Vec<Row> = (start..len.min(start + 1024))
            .filter_map(|i| {
                let index = match source {
                    RowSource::Idxs(ids) => ids[i],
                    RowSource::Scan { .. } => i,
                };
                db.store.get_by_idx(name, index).cloned()
            })
            .collect();
        let mask = if let Some(pred) = pred {
            db.gpu_filter_mask(pred, &rows, now)?
        } else {
            None
        };
        let mut slots = pred.map(PredSlots::new);
        for (i, row) in rows.iter().enumerate() {
            let hit = mask.as_ref().map_or_else(
                || slots.as_mut().is_none_or(|s| s.eval(row, now)),
                |mask| mask[i],
            );
            if hit {
                if let Some(id) = row_text(row, "id") {
                    out.insert(id.to_owned());
                }
                if let Some(uri) = row_text(row, "uri") {
                    out.insert(uri.to_owned());
                }
            }
        }
    }
    Ok(())
}
