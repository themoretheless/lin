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
use crate::exec::{Db, JoinFieldPlan, PredSlots, ReadDb, field_names, plan_join_fields, pred_total_gt};
use crate::query::Queryable;
use crate::store::{
    Cell, OwnedRowProjector, Row, Store, cell_from_arc_inline, now_ms, project_fields, row_text,
};
use rustc_hash::{FxHashMap, FxHashSet, FxHasher};
use smallvec::{SmallVec, smallvec};

thread_local! {
    static CURSOR_VALIDATED_QUERIES: RefCell<FxHashSet<u64>> = RefCell::new(FxHashSet::default());
}

fn empty_fields_arc() -> Arc<[String]> {
    static EMPTY: std::sync::OnceLock<Arc<[String]>> = std::sync::OnceLock::new();
    Arc::clone(EMPTY.get_or_init(|| Vec::<String>::new().into()))
}

fn soa_join_shape() -> &'static Arc<crate::store::RowShape> {
    static SOA_JOIN_SHAPE: std::sync::OnceLock<Arc<crate::store::RowShape>> =
        std::sync::OnceLock::new();
    SOA_JOIN_SHAPE.get_or_init(|| Row::shape_of(&["id", "total", "users.email"]))
}

fn soa_join_fields() -> &'static Arc<[String]> {
    static SOA_JOIN_FIELDS: std::sync::OnceLock<Arc<[String]>> = std::sync::OnceLock::new();
    SOA_JOIN_FIELDS.get_or_init(|| {
        vec![
            String::from("id"),
            String::from("users.email"),
            String::from("total"),
        ]
        .into()
    })
}

enum JoinSlotSource {
    Left(crate::store::row::OwnedFieldSlot),
    Right(crate::store::row::OwnedFieldSlot),
}

struct CompiledJoinProjector {
    shape: Option<Arc<crate::store::RowShape>>,
    sorted_slots: SmallVec<[JoinSlotSource; 8]>,
}

impl CompiledJoinProjector {
    fn new(right_col: &str, fields: &[String]) -> (Self, Vec<String>) {
        let (right_fields, plan) = plan_join_fields(right_col, fields);
        if fields.is_empty() {
            return (
                Self {
                    shape: None,
                    sorted_slots: SmallVec::new(),
                },
                right_fields,
            );
        }
        let shape = Row::shape_of(fields);
        let sorted_slots = shape
            .names()
            .iter()
            .map(|target| {
                // Find the matching entry in `plan` (last occurrence wins if duplicated).
                let matched = plan.iter().rfind(|p| match p {
                    JoinFieldPlan::Left(k) => k == target,
                    JoinFieldPlan::Right { out, .. } => out == target,
                });
                match matched {
                    Some(JoinFieldPlan::Right { idx, .. }) => JoinSlotSource::Right(
                        crate::store::row::OwnedFieldSlot::new(right_fields[*idx].as_str()),
                    ),
                    _ => JoinSlotSource::Left(crate::store::row::OwnedFieldSlot::new(
                        target.as_str(),
                    )),
                }
            })
            .collect();
        (
            Self {
                shape: Some(shape),
                sorted_slots,
            },
            right_fields,
        )
    }

    #[inline]
    fn project(&mut self, left: &Row, right: Option<&Row>) -> Row {
        let Some(ref shape) = self.shape else {
            return Row::new();
        };
        Row::from_parts(
            Arc::clone(shape),
            self.sorted_slots.iter_mut().map(|src| match src {
                JoinSlotSource::Left(slot) => slot
                    .get(left)
                    .map(Cell::clone_for_borrow)
                    .unwrap_or(Cell::Null),
                JoinSlotSource::Right(slot) => right
                    .and_then(|r| slot.get(r))
                    .map(Cell::clone_for_borrow)
                    .unwrap_or(Cell::Null),
            }),
        )
    }
}

fn build_hot_right_index(
    store: &Store,
    right_col: &str,
    to_field: &str,
) -> FxHashMap<String, SmallVec<[usize; 2]>> {
    let rows = store.collection(right_col);
    let mut slot = crate::store::row::FieldSlot::new(to_field);
    let mut map: FxHashMap<String, SmallVec<[usize; 2]>> = FxHashMap::default();
    map.reserve(rows.len());
    for (idx, r) in rows.iter().enumerate() {
        if let Some(rk) = slot.get(r).and_then(crate::exec::cell_key_ref) {
            if let Some(list) = map.get_mut(rk.as_ref()) {
                list.push(idx);
            } else {
                map.insert(rk.into_owned(), smallvec![idx]);
            }
        }
    }
    map
}

/// Sync pull cursor over query results.
pub struct QueryCursor<'a> {
    db: CursorDb<'a>,
    state: CursorState<'a>,
    last_proj_schema: Option<(Arc<crate::store::RowShape>, Arc<[String]>)>,
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

    fn from_row_cached(
        row: Row,
        cache: &mut Option<(Arc<crate::store::RowShape>, Arc<[String]>)>,
    ) -> Self {
        let Some(shape_ref) = row.shape_arc() else {
            return Self {
                fields: empty_fields_arc(),
                cells: SmallVec::new(),
            };
        };
        let fields = if let Some((cached_shape, cached_fields)) = cache.as_ref()
            && Arc::ptr_eq(cached_shape, shape_ref)
        {
            Arc::clone(cached_fields)
        } else {
            let shape_cloned = Arc::clone(shape_ref);
            let f_arc: Arc<[String]> = shape_cloned.names().to_vec().into();
            *cache = Some((shape_cloned, Arc::clone(&f_arc)));
            f_arc
        };
        let cells = row.into_values().collect();
        Self { fields, cells }
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
    projector: Option<OwnedRowProjector>,
    target_shape: Option<Arc<crate::store::RowShape>>,
    skip_left: usize,
    take_left: Option<usize>,
}

impl DiskIdsCursor {
    #[allow(clippy::too_many_arguments)]
    fn new(
        collection: String,
        resolve_doc_uri: bool,
        fallback_col: Option<String>,
        ids: Vec<Arc<str>>,
        resolved_left: Option<usize>,
        project: Option<Arc<[String]>>,
        skip_left: usize,
        take_left: Option<usize>,
    ) -> Self {
        let projector = project.as_deref().map(OwnedRowProjector::new);
        let target_shape = project
            .as_deref()
            .filter(|fs| !fs.is_empty())
            .map(Row::shape_of);
        Self {
            collection,
            resolve_doc_uri,
            fallback_col,
            ids,
            pos: 0,
            resolved_left,
            project,
            projector,
            target_shape,
            skip_left,
            take_left,
        }
    }
}

fn next_disk_ids(store: &Store, cur: &mut DiskIdsCursor) -> Option<Result<Row, Error>> {
    loop {
        if cur.take_left == Some(0) || cur.resolved_left == Some(0) || cur.pos >= cur.ids.len() {
            return None;
        }
        let key = &cur.ids[cur.pos];
        cur.pos += 1;
        let proj_slice = cur.project.as_deref();

        if cur.resolve_doc_uri && !store.is_disk_backed("docs") {
            if let Some(doc_ref) = store.find_doc_key(key) {
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
                let out = match cur.projector.as_mut() {
                    Some(proj) => proj.project(doc_ref),
                    None => doc_ref.clone(),
                };
                return Some(Ok(out));
            }
            let Some(fb) = cur.fallback_col.as_deref() else {
                continue;
            };
            let Some(row) = store.project_by_key(fb, "id", key, proj_slice) else {
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
            return Some(Ok(row));
        }

        let row = if cur.resolve_doc_uri {
            let doc_row = store
                .project_by_key("docs", "id", key, proj_slice)
                .or_else(|| store.project_by_key("docs", "uri", key, proj_slice));
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
            let fetched = match proj_slice {
                Some(fs) => {
                    let refs: SmallVec<[&str; 8]> = fs.iter().map(String::as_str).collect();
                    eng.get_projected_by_id(&cur.collection, key, &refs)
                }
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
        let out = match cur.projector.as_mut() {
            Some(proj) => {
                let already_shaped = row
                    .shape_arc()
                    .zip(cur.target_shape.as_ref())
                    .is_some_and(|(a, b)| Arc::ptr_eq(a, b));
                if already_shaped {
                    row
                } else {
                    proj.project(&row)
                }
            }
            None => row,
        };
        return Some(Ok(out));
    }
}

/// `col | filter? | project? | skip* | take?` over a disk-backed collection, read through
/// [`crate::disk::DiskEngine::scan_chunk`] without materializing the collection.
struct DiskStreamCursor {
    collection: String,
    pred: Option<Pred>,
    project: Option<Vec<String>>,
    projector: Option<OwnedRowProjector>,
    target_shape: Option<Arc<crate::store::RowShape>>,
    skip_left: usize,
    take_left: Option<usize>,
    now: i64,
    pos: crate::disk::DiskScanPos,
    pending: VecDeque<Row>,
}

impl DiskStreamCursor {
    fn new(
        collection: String,
        pred: Option<Pred>,
        project: Option<Vec<String>>,
        skip_left: usize,
        take_left: Option<usize>,
        now: i64,
    ) -> Self {
        let projector = project.as_deref().map(OwnedRowProjector::new);
        let target_shape = project
            .as_deref()
            .filter(|fs| !fs.is_empty())
            .map(Row::shape_of);
        Self {
            collection,
            pred,
            project,
            projector,
            target_shape,
            skip_left,
            take_left,
            now,
            pos: crate::disk::DiskScanPos::default(),
            pending: VecDeque::new(),
        }
    }
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
    on_slot: crate::store::row::OwnedFieldSlot,
    to_field: String,
    left_join: bool,
    join_projector: Option<CompiledJoinProjector>,
    right_proj_fields: Option<Vec<String>>,
    skip_left: usize,
    take_left: Option<usize>,
    emitted: usize,
    max_expansion: usize,
    memo: FxHashMap<String, Option<Row>>,
    multi_memo: FxHashMap<String, Vec<Row>>,
    hot_right_index: Option<FxHashMap<String, SmallVec<[usize; 2]>>>,
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
            let right_on_disk = store.is_disk_backed(&cur.right_col);
            if !right_on_disk {
                let right = cur
                    .on_slot
                    .get(&left)
                    .and_then(Cell::text)
                    .and_then(|key| store.get_by_id(&cur.right_col, key));
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
                let row = match cur.join_projector.as_mut() {
                    Some(proj) => proj.project(&left, right),
                    None => match right {
                        Some(r) => crate::exec::merge_join_row(left, r, &cur.right_col),
                        None => left,
                    },
                };
                return Some(Ok(row));
            }

            let right = match cur.on_slot.get(&left).and_then(Cell::text) {
                None => None,
                Some(key) => {
                    if cur.memo.len() >= 8192 && !cur.memo.contains_key(key) {
                        cur.memo.clear();
                    }
                    if !cur.memo.contains_key(key) {
                        let found = match store.disk_engine_read() {
                            Some(eng) => match &cur.right_proj_fields {
                                Some(rfs) => {
                                    let refs: SmallVec<[&str; 8]> =
                                        rfs.iter().map(String::as_str).collect();
                                    match eng.get_projected_by_id(&cur.right_col, key, &refs) {
                                        Ok(row) => row,
                                        Err(e) => return Some(Err(e)),
                                    }
                                }
                                None => match eng.get_by_id(&cur.right_col, key) {
                                    Ok(row) => row,
                                    Err(e) => return Some(Err(e)),
                                },
                            },
                            None => None,
                        };
                        cur.memo.entry(key.to_string()).or_insert(found).as_ref()
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
            let row = match cur.join_projector.as_mut() {
                Some(proj) => proj.project(&left, right),
                None => match right {
                    Some(r) => crate::exec::merge_join_row(left, r, &cur.right_col),
                    None => left,
                },
            };
            return Some(Ok(row));
        }

        // Non-id foreign key: match `cell_key_ref` (`Text` / `SmallText` / `Int`).
        let right_on_disk = store.is_disk_backed(&cur.right_col);
        if !right_on_disk {
            let hit_idxs: &[usize] = match cur
                .on_slot
                .get(&left)
                .and_then(crate::exec::cell_key_ref)
            {
                None => &[],
                Some(k_cow) => {
                    let idx_map = cur.hot_right_index.get_or_insert_with(|| {
                        build_hot_right_index(store, &cur.right_col, &cur.to_field)
                    });
                    idx_map
                        .get(k_cow.as_ref())
                        .map(SmallVec::as_slice)
                        .unwrap_or(&[])
                }
            };
            if hit_idxs.is_empty() {
                if cur.left_join {
                    cur.emitted += 1;
                    if cur.emitted > cur.max_expansion {
                        cur.take_left = Some(0);
                        return Some(Err(Error::runtime(format!(
                            "Cartesian join expansion {} exceeds max_join_expansion quota {}",
                            cur.emitted, cur.max_expansion
                        ))));
                    }
                    let out = match cur.join_projector.as_mut() {
                        Some(proj) => proj.project(&left, None),
                        None => left,
                    };
                    cur.pending.push_back(out);
                }
                continue;
            }
            let right_rows = store.collection(&cur.right_col);
            let last = hit_idxs.len() - 1;
            let mut left_opt = Some(left);
            for (i, &r_idx) in hit_idxs.iter().enumerate() {
                let r = &right_rows[r_idx];
                cur.emitted += 1;
                if cur.emitted > cur.max_expansion {
                    cur.take_left = Some(0);
                    return Some(Err(Error::runtime(format!(
                        "Cartesian join expansion {} exceeds max_join_expansion quota {}",
                        cur.emitted, cur.max_expansion
                    ))));
                }
                let out = match cur.join_projector.as_mut() {
                    Some(proj) => proj.project(left_opt.as_ref().unwrap(), Some(r)),
                    None => {
                        let base = if i == last {
                            left_opt.take().unwrap()
                        } else {
                            left_opt.as_ref().unwrap().clone()
                        };
                        crate::exec::merge_join_row(base, r, &cur.right_col)
                    }
                };
                cur.pending.push_back(out);
            }
            continue;
        }

        let key_opt = cur
            .on_slot
            .get(&left)
            .and_then(crate::exec::cell_key_ref)
            .map(|c| c.into_owned());
        let hits: Vec<Row> = match key_opt {
            None => Vec::new(),
            Some(k) => {
                if let Some(cached) = cur.multi_memo.get(k.as_str()) {
                    cached.clone()
                } else {
                    let fetched = match fetch_cold_non_id_right_rows(
                        store,
                        &cur.right_col,
                        &cur.to_field,
                        &k,
                        cur.right_proj_fields.as_deref(),
                    ) {
                        Ok(rows) => rows,
                        Err(e) => {
                            cur.take_left = Some(0);
                            return Some(Err(e));
                        }
                    };
                    if cur.multi_memo.len() < 4096 {
                        cur.multi_memo.insert(k, fetched.clone());
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
                let out = match cur.join_projector.as_mut() {
                    Some(proj) => proj.project(&left, None),
                    None => left,
                };
                cur.pending.push_back(out);
            }
            continue;
        }
        let last = hits.len() - 1;
        let mut left_opt = Some(left);
        for (i, r) in hits.iter().enumerate() {
            cur.emitted += 1;
            if cur.emitted > cur.max_expansion {
                cur.take_left = Some(0);
                return Some(Err(Error::runtime(format!(
                    "Cartesian join expansion {} exceeds max_join_expansion quota {}",
                    cur.emitted, cur.max_expansion
                ))));
            }
            let out = match cur.join_projector.as_mut() {
                Some(proj) => proj.project(left_opt.as_ref().unwrap(), Some(r)),
                None => {
                    let base = if i == last {
                        left_opt.take().unwrap()
                    } else {
                        left_opt.as_ref().unwrap().clone()
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
    if !store.is_disk_backed(name) {
        return None;
    }
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
            Step::Project(f)
                if join.is_some() && project.is_none() && skip_n == 0 && take_n.is_none() =>
            {
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
    let to_field = catalog
        .find_fk(name, on, right_col)
        .map(|fk| fk.to_field.as_str())
        .unwrap_or("id");
    let right_on_disk = store.is_disk_backed(right_col);
    let (join_projector, right_proj_fields) = match project {
        Some(ref fs) => {
            let (proj, rfs) = CompiledJoinProjector::new(right_col, fs);
            (Some(proj), right_on_disk.then_some(rfs))
        }
        None => (None, None),
    };
    Some(DiskJoinCursor {
        left: DiskStreamCursor::new(name.clone(), filter.cloned(), None, 0, None, now_ms()),
        right_col: right_col.to_string(),
        on_slot: crate::store::row::OwnedFieldSlot::new(on),
        to_field: to_field.to_string(),
        left_join,
        join_projector,
        right_proj_fields,
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
        hot_right_index: None,
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
            let out = match cur.projector.as_mut() {
                // Absent fields come back as nulls, as on the in-memory path.
                Some(proj) => {
                    let already_shaped = row
                        .shape_arc()
                        .zip(cur.target_shape.as_ref())
                        .is_some_and(|(a, b)| Arc::ptr_eq(a, b));
                    if already_shaped {
                        row
                    } else {
                        proj.project(&row)
                    }
                }
                None => row,
            };
            return Some(Ok(out));
        }
        let eng = store.disk_engine_read()?;
        let want = cur
            .take_left
            .map_or(DISK_STREAM_CHUNK, |n| n.saturating_add(cur.skip_left))
            .clamp(1, DISK_STREAM_CHUNK);
        let fields_buf: SmallVec<[&str; 8]>;
        let fields_ref: Option<&[&str]> = match &cur.project {
            Some(fs) => {
                fields_buf = fs.iter().map(String::as_str).collect();
                Some(fields_buf.as_slice())
            }
            None => None,
        };
        let chunk = eng.scan_chunk(
            &cur.collection,
            cur.pred.as_ref(),
            fields_ref,
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
    on_slot: crate::store::row::OwnedFieldSlot,
    to_field: String,
    left_join: bool,
    join_projector: Option<CompiledJoinProjector>,
    right_on_disk: bool,
    right_proj_fields: Option<Vec<String>>,
    disk_right_memo: FxHashMap<String, Option<Row>>,
    multi_right_memo: FxHashMap<String, Vec<Row>>,
    hot_right_index: Option<FxHashMap<String, SmallVec<[usize; 2]>>>,
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

const SOA_JOIN_BATCH: usize = 64;
const UNMATCHED_USER_IDX: u32 = u32::MAX;

/// Columnar left + user-index probe with 64-row resolved prefetch batching; emits compact 3-field rows.
struct LazyJoinSoa<'a> {
    source: RowSource,
    pos: usize,
    left_join: bool,
    /// When set, filter `orders.total > min` without row maps.
    total_gt: Option<f64>,
    orders_id: &'a [Arc<str>],
    orders_uid: &'a [Arc<str>],
    orders_total: &'a [f64],
    orders_total_tag: &'a [u8],
    /// Fallback rows behind the SoA buffers (only consulted when `orders_total_tag` == `2`).
    orders_rows: &'a [Row],
    total_slot: crate::store::row::FieldSlot<'static>,
    users_id: &'a [Arc<str>],
    users_email: &'a [Arc<str>],
    unique_users: bool,
    probe: UserProbe<'a>,
    skip_left: usize,
    take_left: Option<usize>,
    out_shape: Arc<crate::store::RowShape>,
    proj_fields: Arc<[String]>,
    buf_idx: [u32; SOA_JOIN_BATCH],
    buf_ui: [u32; SOA_JOIN_BATCH],
    buf_pos: u8,
    buf_len: u8,
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
                last_proj_schema: None,
            });
        }
        if let Some(join) = try_disk_join(&db, q) {
            return Ok(Self {
                db,
                state: CursorState::DiskJoin(join),
                last_proj_schema: None,
            });
        }
        if let Some(join) = try_lazy_join_soa(&db, q)? {
            return Ok(Self {
                db,
                state: CursorState::LazyJoinSoa(join),
                last_proj_schema: None,
            });
        }
        if let Some(join) = try_lazy_join(&db, q)? {
            return Ok(Self {
                db,
                state: CursorState::LazyJoin(join),
                last_proj_schema: None,
            });
        }
        if let Some(state) = try_lazy_search(&db, q)? {
            return Ok(Self {
                db,
                state,
                last_proj_schema: None,
            });
        }
        if let Some(state) = try_lazy_hop(&db, q)? {
            return Ok(Self {
                db,
                state,
                last_proj_schema: None,
            });
        }
        if let Some(lazy) = try_lazy_simple(&db, q)? {
            return Ok(Self {
                db,
                state: CursorState::Lazy(lazy),
                last_proj_schema: None,
            });
        }
        if let Some(stream) = try_disk_stream(&db, q) {
            return Ok(Self {
                db,
                state: CursorState::DiskStream(stream),
                last_proj_schema: None,
            });
        }

        let rows = db.run_query(q)?;
        Ok(Self {
            db,
            state: CursorState::Buffered(rows.into_iter()),
            last_proj_schema: None,
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
    #[inline]
    pub fn next_projected(&mut self) -> Option<Result<ProjectedRow, Error>> {
        if let CursorState::LazyJoinSoa(join) = &mut self.state {
            let item = next_lazy_join_soa_projected(join);
            if item.is_none() {
                self.state = CursorState::Done;
            }
            return item;
        }
        if matches!(self.state, CursorState::Done) {
            return None;
        }
        let native = matches!(&self.state, CursorState::Lazy(lazy) if lazy.project.is_some());
        #[cfg(feature = "gpu")]
        let native = native
            || matches!(&self.state, CursorState::GpuStream(stream) if stream.project.is_some());
        if !native {
            let item = self.next()?;
            return Some(item.map(|row| ProjectedRow::from_row_cached(row, &mut self.last_proj_schema)));
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

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if let CursorState::LazyJoinSoa(join) = &mut self.state {
            let item = next_lazy_join_soa(join);
            if item.is_none() {
                self.state = CursorState::Done;
            }
            return item;
        }
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
            _ => None,
        };

        if item.is_none() {
            self.state = CursorState::Done;
        }
        item
    }
}

fn next_lazy(store: &Store, lazy: &mut LazyCursor) -> Option<Result<Row, Error>> {
    let rows = store.collection(&lazy.collection);
    let row = next_lazy_match_row(rows, lazy)?;
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
    let rows = store.collection(&lazy.collection);
    let row = next_lazy_match_row(rows, lazy)?;
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
fn next_lazy_match_row<'s>(rows: &'s [Row], lazy: &mut LazyCursor) -> Option<&'s Row> {
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
        let Some(row) = rows.get(idx) else {
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
        return Some(row);
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

        let left_ref = next_lazy_raw_ref(store, &mut join.left)?;

        if join.to_field == "id" && !join.right_on_disk {
            let right = join
                .on_slot
                .get(left_ref)
                .and_then(Cell::text)
                .and_then(|k| store.get_by_id(&join.right_col, k));
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
            if join.skip_left > 0 {
                join.skip_left -= 1;
                continue;
            }
            if let Some(t) = join.take_left.as_mut() {
                *t = t.saturating_sub(1);
            }
            let out = if let Some(proj) = join.join_projector.as_mut() {
                proj.project(left_ref, right)
            } else if let Some(r) = right {
                crate::exec::merge_join_row(left_ref.clone(), r, &join.right_col)
            } else {
                left_ref.clone()
            };
            return Some(Ok(out));
        }

        if join.to_field == "id" && join.right_on_disk {
            let right_ref: Option<&Row> = match join.on_slot.get(left_ref).and_then(Cell::text) {
                None => None,
                Some(k) => {
                    if join.disk_right_memo.len() >= 8192 && !join.disk_right_memo.contains_key(k) {
                        join.disk_right_memo.clear();
                    }
                    if !join.disk_right_memo.contains_key(k) {
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
                        join.disk_right_memo
                            .entry(k.to_string())
                            .or_insert(row_opt)
                            .as_ref()
                    } else {
                        join.disk_right_memo.get(k).and_then(Option::as_ref)
                    }
                }
            };
            if right_ref.is_none() && !join.left_join {
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
            if join.skip_left > 0 {
                join.skip_left -= 1;
                continue;
            }
            if let Some(t) = join.take_left.as_mut() {
                *t = t.saturating_sub(1);
            }
            let out = if let Some(proj) = join.join_projector.as_mut() {
                proj.project(left_ref, right_ref)
            } else if let Some(r) = right_ref {
                crate::exec::merge_join_row(left_ref.clone(), r, &join.right_col)
            } else {
                left_ref.clone()
            };
            return Some(Ok(out));
        }

        // Non-id join key (cold or hot right side): match `cell_key_ref` (`Text`/`SmallText`/`Int`).
        if !join.right_on_disk {
            let hit_idxs: &[usize] = match join
                .on_slot
                .get(left_ref)
                .and_then(crate::exec::cell_key_ref)
            {
                None => &[],
                Some(k_cow) => {
                    let idx_map = join.hot_right_index.get_or_insert_with(|| {
                        build_hot_right_index(store, &join.right_col, &join.to_field)
                    });
                    idx_map
                        .get(k_cow.as_ref())
                        .map(SmallVec::as_slice)
                        .unwrap_or(&[])
                }
            };
            if hit_idxs.is_empty() {
                if join.left_join {
                    join.emitted += 1;
                    if join.emitted > join.max_expansion {
                        join.take_left = Some(0);
                        return Some(Err(Error::runtime(format!(
                            "Cartesian join expansion {} exceeds max_join_expansion quota {}",
                            join.emitted, join.max_expansion
                        ))));
                    }
                    let out = match join.join_projector.as_mut() {
                        Some(proj) => proj.project(left_ref, None),
                        None => left_ref.clone(),
                    };
                    join.pending.push_back(out);
                }
                continue;
            }
            let right_rows = store.collection(&join.right_col);
            for &r_idx in hit_idxs {
                let r = &right_rows[r_idx];
                join.emitted += 1;
                if join.emitted > join.max_expansion {
                    join.take_left = Some(0);
                    return Some(Err(Error::runtime(format!(
                        "Cartesian join expansion {} exceeds max_join_expansion quota {}",
                        join.emitted, join.max_expansion
                    ))));
                }
                let out = match join.join_projector.as_mut() {
                    Some(proj) => proj.project(left_ref, Some(r)),
                    None => crate::exec::merge_join_row(left_ref.clone(), r, &join.right_col),
                };
                join.pending.push_back(out);
            }
            continue;
        }

        let non_id_key = join
            .on_slot
            .get(left_ref)
            .and_then(crate::exec::cell_key_ref)
            .map(|c| c.into_owned());
        let hits: Vec<Row> = match non_id_key {
            None => Vec::new(),
            Some(k) => {
                if let Some(cached) = join.multi_right_memo.get(k.as_str()) {
                    cached.clone()
                } else {
                    let fetched = match fetch_cold_non_id_right_rows(
                        store,
                        &join.right_col,
                        &join.to_field,
                        &k,
                        join.right_proj_fields.as_deref(),
                    ) {
                        Ok(rows) => rows,
                        Err(e) => {
                            join.take_left = Some(0);
                            return Some(Err(e));
                        }
                    };
                    if join.multi_right_memo.len() < 4096 {
                        join.multi_right_memo.insert(k, fetched.clone());
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
                let out = match join.join_projector.as_mut() {
                    Some(proj) => proj.project(left_ref, None),
                    None => left_ref.clone(),
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
            let out = match join.join_projector.as_mut() {
                Some(proj) => proj.project(left_ref, Some(r)),
                None => crate::exec::merge_join_row(left_ref.clone(), r, &join.right_col),
            };
            join.pending.push_back(out);
        }
    }
}

/// Pull next matching left row by reference without cloning or projecting prematurely.
#[inline]
fn next_lazy_raw_ref<'s>(store: &'s Store, lazy: &mut LazyCursor) -> Option<&'s Row> {
    let rows = store.collection(&lazy.collection);
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
        let Some(row) = rows.get(idx) else {
            continue;
        };
        if lazy.need_filter
            && let Some(slots) = lazy.pred_slots.as_mut()
            && !slots.eval(row, lazy.now)
        {
            continue;
        }
        return Some(row);
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
    let take_left = match take_n {
        Some(Some(n)) => Some(n.max(0) as usize),
        Some(None) => None,
        None => Some(50),
    };
    Some(DiskStreamCursor::new(
        name.clone(),
        filter.cloned(),
        project,
        skip_n,
        take_left,
        now_ms(),
    ))
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
        return Ok(Some(CursorState::DiskIds(DiskIdsCursor::new(
            name.clone(),
            false,
            None,
            ids,
            None,
            project.map(Into::into),
            skip_n,
            take_left,
        ))));
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
        let reached = bfs_hop_keys(
            store,
            edge_rel,
            reverse,
            hop_depth,
            db.inner().quotas.max_graph_expansion,
            frontier,
            &mut seen,
        )?;
        let ids: Vec<Arc<str>> = reached.into_iter().collect();
        return Ok(Some(CursorState::DiskIds(DiskIdsCursor::new(
            "docs".to_string(),
            true,
            (name != "docs").then(|| name.clone()),
            ids,
            Some(300),
            project.map(Into::into),
            skip_n,
            take_left,
        ))));
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
    let reached = bfs_hop_keys(
        store,
        edge_rel,
        reverse,
        hop_depth,
        db.inner().quotas.max_graph_expansion,
        frontier,
        &mut seen,
    )?;

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

fn bfs_hop_keys(
    store: &Store,
    edge_rel: &str,
    reverse: bool,
    hop_depth: i64,
    max_graph_expansion: usize,
    mut frontier: Vec<Arc<str>>,
    seen: &mut FxHashSet<Arc<str>>,
) -> Result<BTreeSet<Arc<str>>, Error> {
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
        if seen.len() > max_graph_expansion {
            return Err(Error::runtime(format!(
                "graph expansion count {} exceeds max_graph_expansion quota {}",
                seen.len(),
                max_graph_expansion
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
    Ok(reached)
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
    let mut id_slot = crate::store::row::FieldSlot::new("id");
    let mut uri_slot = crate::store::row::FieldSlot::new("uri");
    let mut push_cell = |c: Option<&Cell>| match c {
        Some(Cell::Text(a)) if !a.is_empty() => {
            if !seen.contains(a.as_ref()) {
                seen.insert(Arc::clone(a));
                out.push(Arc::clone(a));
            }
        }
        Some(other) => {
            if let Some(s) = other.text().filter(|s| !s.is_empty())
                && !seen.contains(s)
            {
                let a: Arc<str> = Arc::from(s);
                seen.insert(Arc::clone(&a));
                out.push(a);
            }
        }
        None => {}
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
                push_cell(id_slot.get(row));
                push_cell(uri_slot.get(row));
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
                push_cell(id_slot.get(row));
                push_cell(uri_slot.get(row));
            }
        }
    }
}

#[inline(always)]
fn resolve_user_idx(
    uid: &str,
    users_id: &[Arc<str>],
    unique_users: bool,
    probe: &FxHashMap<Arc<str>, usize>,
) -> Option<usize> {
    if unique_users {
        let b = uid.as_bytes();
        if b.first() == Some(&b'u') {
            let digits = if b.get(1) == Some(&b'-') {
                &b[2..]
            } else {
                &b[1..]
            };
            if !digits.is_empty() && digits.len() <= 9 {
                let mut val = 0usize;
                let mut all_digits = true;
                for &d in digits {
                    let dig = d.wrapping_sub(b'0');
                    if dig > 9 {
                        all_digits = false;
                        break;
                    }
                    val = val * 10 + dig as usize;
                }
                if all_digits {
                    if let Some(id_at) = users_id.get(val)
                        && id_at.as_ref() == uid
                    {
                        return Some(val);
                    }
                    if val > 0
                        && let Some(id_prev) = users_id.get(val - 1)
                        && id_prev.as_ref() == uid
                    {
                        return Some(val - 1);
                    }
                }
            }
        }
    }
    probe.get(uid).copied()
}

#[inline(always)]
fn inline_cells3(c0: Cell, c1: Cell, c2: Cell) -> SmallVec<[Cell; 4]> {
    let mut cells = SmallVec::<[Cell; 4]>::from_buf([c0, c1, c2, Cell::Null]);
    cells.truncate(3);
    cells
}

impl<'a> LazyJoinSoa<'a> {
    #[inline(never)]
    fn refill_batch(&mut self) -> bool {
        if self.take_left == Some(0) {
            return false;
        }
        let max_emit = self
            .take_left
            .map_or(SOA_JOIN_BATCH, |t| t.min(SOA_JOIN_BATCH));
        let probe = self.probe.map();
        let n = self.orders_id.len();
        let users_id = self.users_id;
        let unique_users = self.unique_users;
        let orders_uid = self.orders_uid;
        let orders_total = self.orders_total;
        debug_assert_eq!(n, orders_uid.len());
        debug_assert_eq!(n, orders_total.len());

        let mut k = 0usize;

        match (&self.source, self.skip_left == 0, self.left_join, self.total_gt) {
            (RowSource::Scan { len }, true, false, None) => {
                let end = (*len).min(n);
                let mut pos = self.pos;
                while pos < end && k < max_emit {
                    // SAFETY: `pos < end <= n == orders_uid.len()`.
                    let uid = unsafe { orders_uid.get_unchecked(pos).as_ref() };
                    pos += 1;
                    if let Some(ui) = resolve_user_idx(uid, users_id, unique_users, probe) {
                        self.buf_idx[k] = (pos - 1) as u32;
                        self.buf_ui[k] = ui as u32;
                        k += 1;
                    }
                }
                self.pos = pos;
            }
            (RowSource::Scan { len }, true, false, Some(min)) => {
                let end = (*len).min(n);
                let mut pos = self.pos;
                while pos < end && k < max_emit {
                    // SAFETY: `pos < end <= n == orders_total.len() == orders_uid.len()`.
                    let total = unsafe { *orders_total.get_unchecked(pos) };
                    let idx = pos;
                    pos += 1;
                    if !(total > min) {
                        continue;
                    }
                    let uid = unsafe { orders_uid.get_unchecked(idx).as_ref() };
                    if let Some(ui) = resolve_user_idx(uid, users_id, unique_users, probe) {
                        self.buf_idx[k] = idx as u32;
                        self.buf_ui[k] = ui as u32;
                        k += 1;
                    }
                }
                self.pos = pos;
            }
            _ => {
                loop {
                    if k >= max_emit {
                        break;
                    }
                    let idx = match &self.source {
                        RowSource::Idxs(idxs) => {
                            if self.pos >= idxs.len() {
                                break;
                            }
                            let i = idxs[self.pos];
                            self.pos += 1;
                            i
                        }
                        RowSource::Scan { len } => {
                            if self.pos >= *len {
                                break;
                            }
                            let i = self.pos;
                            self.pos += 1;
                            i
                        }
                    };
                    if idx >= n {
                        continue;
                    }
                    // SAFETY: `idx < n == orders_uid.len() == orders_total.len()`.
                    let (uid, total) = unsafe {
                        (
                            orders_uid.get_unchecked(idx).as_ref(),
                            *orders_total.get_unchecked(idx),
                        )
                    };
                    if let Some(min) = self.total_gt
                        && !(total > min)
                    {
                        continue;
                    }
                    let right = resolve_user_idx(uid, users_id, unique_users, probe);
                    if right.is_none() && !self.left_join {
                        continue;
                    }
                    if self.skip_left > 0 {
                        self.skip_left -= 1;
                        continue;
                    }
                    self.buf_idx[k] = idx as u32;
                    self.buf_ui[k] = match right {
                        Some(ui) => ui as u32,
                        None => UNMATCHED_USER_IDX,
                    };
                    k += 1;
                }
            }
        }

        if k == 0 {
            return false;
        }
        if let Some(t) = self.take_left.as_mut() {
            *t = t.saturating_sub(k);
        }
        self.buf_pos = 0;
        self.buf_len = k as u8;
        true
    }

    #[inline(always)]
    fn pop_pair(&mut self) -> Option<(usize, u32)> {
        if self.buf_pos >= self.buf_len && !self.refill_batch() {
            return None;
        }
        let p = self.buf_pos as usize;
        self.buf_pos += 1;
        // SAFETY: `p < self.buf_len <= SOA_JOIN_BATCH`.
        unsafe {
            Some((
                *self.buf_idx.get_unchecked(p) as usize,
                *self.buf_ui.get_unchecked(p),
            ))
        }
    }

    #[cold]
    #[inline(never)]
    fn total_cell_fallback(&mut self, idx: usize) -> Cell {
        crate::exec::order_total_cell(
            &mut self.total_slot,
            self.orders_rows,
            self.orders_total,
            self.orders_total_tag,
            idx,
        )
    }

    #[inline(always)]
    fn total_cell_at(&mut self, idx: usize) -> Cell {
        // SAFETY: `idx < self.orders_id.len() == self.orders_total.len()`.
        let total = unsafe { *self.orders_total.get_unchecked(idx) };
        match self.orders_total_tag.get(idx).copied() {
            Some(1) => Cell::Float(total),
            Some(0) => Cell::Int(total as i64),
            _ => self.total_cell_fallback(idx),
        }
    }
}

#[inline]
fn next_lazy_join_soa(join: &mut LazyJoinSoa<'_>) -> Option<Result<Row, Error>> {
    let (idx, ui) = join.pop_pair()?;
    let total = join.total_cell_at(idx);
    // SAFETY: `idx < orders_id.len()` and `ui < users_email.len()` (when `!= UNMATCHED_USER_IDX`)
    // were validated when filling `buf_idx` / `buf_ui`.
    let id_cell = cell_from_arc_inline(unsafe { join.orders_id.get_unchecked(idx) });
    let email_cell = if ui == UNMATCHED_USER_IDX {
        Cell::Null
    } else {
        debug_assert!((ui as usize) < join.users_email.len());
        cell_from_arc_inline(unsafe { join.users_email.get_unchecked(ui as usize) })
    };
    // Sorted key order: "id" < "total" < "users.email".
    let row = Row::from_parts(
        Arc::clone(&join.out_shape),
        [id_cell, total, email_cell],
    );
    Some(Ok(row))
}

#[inline]
fn next_lazy_join_soa_projected(join: &mut LazyJoinSoa<'_>) -> Option<Result<ProjectedRow, Error>> {
    let (idx, ui) = join.pop_pair()?;
    let total = join.total_cell_at(idx);
    // SAFETY: `idx < orders_id.len()` and `ui < users_email.len()` (when `!= UNMATCHED_USER_IDX`)
    // were validated when filling `buf_idx` / `buf_ui`.
    let id_cell = cell_from_arc_inline(unsafe { join.orders_id.get_unchecked(idx) });
    let email_cell = if ui == UNMATCHED_USER_IDX {
        Cell::Null
    } else {
        debug_assert!((ui as usize) < join.users_email.len());
        cell_from_arc_inline(unsafe { join.users_email.get_unchecked(ui as usize) })
    };
    Some(Ok(ProjectedRow {
        fields: Arc::clone(&join.proj_fields),
        cells: inline_cells3(id_cell, email_cell, total),
    }))
}

fn is_orders_users_id_email_total_fields(fields: &[Field]) -> bool {
    fields.len() == 3
        && fields[0].leaf() == Some("id")
        && ((fields[1].parts.len() == 2
            && fields[1].parts[0] == "users"
            && fields[1].parts[1] == "email")
            || fields[1].leaf() == Some("users.email"))
        && fields[2].leaf() == Some("total")
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
    let mut project: Option<&[Field]> = None;
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
                project = Some(f.as_slice());
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
    let Some(fields) = project else {
        return Ok(None);
    };
    if !is_orders_users_id_email_total_fields(fields) {
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
    if n > u32::MAX as usize || users_email.len() >= UNMATCHED_USER_IDX as usize {
        return Ok(None);
    }
    let source = RowSource::Scan { len: n };

    let (probe, unique_users) = if let Some(by_id) = store.by_id.get("users")
        && by_id.len() == users_id.len()
    {
        (UserProbe::Borrowed(by_id), true)
    } else {
        let mut map: FxHashMap<Arc<str>, usize> = FxHashMap::default();
        map.reserve(users_id.len());
        for (i, id) in users_id.iter().enumerate() {
            map.insert(Arc::clone(id), i);
        }
        let uniq = map.len() == users_id.len();
        (UserProbe::Owned(map), uniq)
    };

    Ok(Some(LazyJoinSoa {
        source,
        pos: 0,
        left_join,
        total_gt,
        orders_id,
        orders_uid,
        orders_total,
        orders_total_tag: store.orders_total_tag(),
        orders_rows: store.collection("orders"),
        total_slot: crate::store::row::FieldSlot::new("total"),
        users_id,
        users_email,
        unique_users,
        probe,
        skip_left: skip_n,
        take_left,
        out_shape: Arc::clone(soa_join_shape()),
        proj_fields: Arc::clone(soa_join_fields()),
        buf_idx: [0; SOA_JOIN_BATCH],
        buf_ui: [0; SOA_JOIN_BATCH],
        buf_pos: 0,
        buf_len: 0,
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

    // Compile projected join schema once and push right-side column projection into cold reads.
    let (join_projector, right_proj_fields) = match project {
        Some(ref fields) => {
            let (proj, right_fields) = CompiledJoinProjector::new(&right_col, fields);
            (Some(proj), right_on_disk.then_some(right_fields))
        }
        None => (None, None),
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
        on_slot: crate::store::row::OwnedFieldSlot::new(on),
        to_field,
        left_join,
        join_projector,
        right_on_disk,
        right_proj_fields,
        disk_right_memo: FxHashMap::default(),
        multi_right_memo: FxHashMap::default(),
        hot_right_index: None,
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

#[cfg(test)]
mod soa_batch_tests {
    use super::*;

    #[test]
    fn lazy_join_soa_64_batch_matches_run_across_boundaries() {
        let mut db = Db::empty();
        let mut u_stmt = String::from("insert users [");
        for i in 0..40 {
            if i > 0 {
                u_stmt.push(',');
            }
            let uid = if i < 30 {
                format!("u-{i}")
            } else {
                format!("custom_{i}")
            };
            u_stmt.push_str(&format!(r#"{{ id: "{uid}", email: "e{i}@x.dev" }}"#));
        }
        u_stmt.push(']');
        db.run(&u_stmt).unwrap();

        let mut o_stmt = String::from("insert orders [");
        for i in 0..210 {
            if i > 0 {
                o_stmt.push(',');
            }
            let total = (i % 150) as f64 + 0.5;
            if i % 17 == 0 {
                o_stmt.push_str(&format!(r#"{{ id: "o-{i}", total: {total} }}"#));
            } else {
                let uid = if i % 5 == 0 {
                    format!("custom_{}", 30 + (i % 10))
                } else {
                    format!("u-{}", i % 30)
                };
                o_stmt.push_str(&format!(
                    r#"{{ id: "o-{i}", user_id: "{uid}", total: {total} }}"#
                ));
            }
        }
        o_stmt.push(']');
        db.run(&o_stmt).unwrap();

        let cases = [
            r#"orders | join users on user_id | { id, users.email, total } | take all"#,
            r#"orders | total > 75.0 | join users on user_id | { id, users.email, total } | take all"#,
            r#"orders | join left users on user_id | { id, users.email, total } | take all"#,
            r#"orders | join users on user_id | { id, users.email, total } | skip 70 | take 80"#,
            r#"orders | join left users on user_id | { id, users.email, total } | skip 10 | take 5"#,
        ];

        for src in cases {
            let want = db.run(src).unwrap().rows.clone();
            let Stmt::Query(q) = crate::parse(src).unwrap() else {
                unreachable!()
            };
            let cur = db.cursor(&q).unwrap();
            assert!(matches!(cur.state, CursorState::LazyJoinSoa(_)), "{src}");
            let got_rows: Vec<Row> = cur.map(|r| r.unwrap()).collect();
            assert_eq!(got_rows, want, "row cursor mismatch for {src}");

            let mut pcur = db.cursor(&q).unwrap();
            let mut got_proj = Vec::new();
            while let Some(pr) = pcur.next_projected() {
                let pr = pr.unwrap();
                assert_eq!(pr.fields(), &["id", "users.email", "total"]);
                got_proj.push(pr.into_row());
            }
            assert_eq!(got_proj, want, "projected cursor mismatch for {src}");
        }
    }
}
