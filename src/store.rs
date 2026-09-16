use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use rustc_hash::{FxHashMap, FxHashSet};
use serde::{Deserialize, Serialize};

use crate::ast::{Decl, TypeExpr};
use crate::catalog::Catalog;
use crate::cold;
use crate::error::Error;
use crate::index::LiveIndex;
use crate::persist::{
    self, ColSnap, Head, IndexSnap, LogRecord, OpenMemOpts, Pack, Persist, RelSnap, Snapshot,
};

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct StoreOpenPhases {
    pub setup_ms: f64,
    pub snapshot_ms: f64,
    pub wal_ms: f64,
    pub metadata_ms: f64,
    pub indexes_ms: f64,
    pub row_maps_ms: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", content = "v")]
pub enum Cell {
    Null,
    /// Arc so projecting / cloning rows does not deep-copy large strings.
    Text(Arc<str>),
    Int(i64),
    Float(f64),
    Bool(bool),
    Time(i64),
    /// Dense embedding (L2-normalized by the active embedder).
    Vec(Arc<[f32]>),
}

impl Cell {
    pub fn text_arc(s: impl Into<Arc<str>>) -> Self {
        Cell::Text(s.into())
    }

    pub fn vec_arc(v: impl Into<Arc<[f32]>>) -> Self {
        Cell::Vec(v.into())
    }

    pub fn text(&self) -> Option<&str> {
        match self {
            Cell::Text(s) => Some(s.as_ref()),
            _ => None,
        }
    }

    pub fn text_shared(&self) -> Option<Arc<str>> {
        match self {
            Cell::Text(s) => Some(Arc::clone(s)),
            _ => None,
        }
    }

    pub fn as_vec(&self) -> Option<&[f32]> {
        match self {
            Cell::Vec(v) => Some(v.as_ref()),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Cell::Int(n) => Some(*n as f64),
            Cell::Float(n) => Some(*n),
            Cell::Time(n) => Some(*n as f64),
            _ => None,
        }
    }

    pub fn as_int(&self) -> Option<i64> {
        match self {
            Cell::Int(n) => Some(*n),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Cell::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_time(&self) -> Option<i64> {
        match self {
            Cell::Time(ms) => Some(*ms),
            _ => None,
        }
    }

    pub fn is_null(&self) -> bool {
        matches!(self, Cell::Null)
    }

    pub fn compact(&self) -> String {
        match self {
            Cell::Null => "null".into(),
            Cell::Text(s) => format!("{}", Quote(s.as_ref())),
            Cell::Int(n) => n.to_string(),
            Cell::Float(n) => n.to_string(),
            Cell::Bool(b) => b.to_string(),
            Cell::Time(ms) => fmt_iso_millis(*ms),
            Cell::Vec(v) => format!("vec[{}]", v.len()),
        }
    }
}

/// Debug-quote like `format!("{s:?}")` without allocating the cell wrapper.
struct Quote<'a>(&'a str);
impl std::fmt::Display for Quote<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.0)
    }
}

pub type Row = BTreeMap<String, Cell>;

/// Ergonomics for `match row.cell("col") { Cell::Text(s) => …, Cell::Null => … }`.
pub trait RowExt {
    /// Column value, or [`Cell::Null`] if the key is missing.
    fn cell(&self, key: &str) -> &Cell;
}

impl RowExt for Row {
    fn cell(&self, key: &str) -> &Cell {
        static NULL: Cell = Cell::Null;
        self.get(key).unwrap_or(&NULL)
    }
}

pub fn compact_row(row: &Row) -> String {
    let parts: Vec<String> = row
        .iter()
        .filter(|(_, v)| !matches!(v, Cell::Null))
        .map(|(k, v)| format!("{k}: {}", v.compact()))
        .collect();
    format!("{{{}}}", parts.join(", "))
}

pub fn row_text<'a>(row: &'a Row, key: &str) -> Option<&'a str> {
    row.get(key).and_then(Cell::text)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Edge {
    pub rel: String,
    pub from: String,
    pub to: String,
}

#[derive(Debug)]
pub struct Store {
    pub r#gen: u64,
    pub embed_id: String,
    pub next_id: u64,
    pub collections: BTreeMap<String, Vec<Row>>,
    pub edges: Vec<Edge>,
    pub extra_collections: BTreeMap<String, ColSnap>,
    pub extra_rels: BTreeMap<String, RelSnap>,
    pub extra_indexes: BTreeMap<String, IndexSnap>,
    pub indexes: BTreeMap<String, LiveIndex>,
    /// collection → FTS postings (fields with catalog `fts`).
    pub(crate) fts: BTreeMap<String, crate::fts::FtsIndex>,
    /// collection → id → row index (O(1) Get / IndexSeek fetch).
    by_id: BTreeMap<String, FxHashMap<String, usize>>,
    /// docs uri → row index.
    docs_by_uri: FxHashMap<String, usize>,
    /// facts (s,p,o) → row index — O(1) append idempotency.
    pub(crate) facts_by_spo: FxHashMap<(Arc<str>, Arc<str>, Arc<str>), usize>,
    /// edge (rel,from,to) set — O(1) append/delete edge.
    pub(crate) edge_keys: FxHashSet<(Arc<str>, Arc<str>, Arc<str>)>,
    /// Parallel Arc columns for docs — contains scans + projected materialize
    /// without cloning full `BTreeMap` rows.
    docs_id: Vec<Arc<str>>,
    docs_title: Vec<Arc<str>>,
    docs_layer: Vec<Arc<str>>,
    docs_wing: Vec<Arc<str>>,
    /// SoA for orders / users — FK join + OLAP project without per-row maps.
    orders_id: Vec<Arc<str>>,
    orders_user_id: Vec<Arc<str>>,
    orders_total: Vec<f64>,
    users_id: Vec<Arc<str>>,
    users_email: Vec<Arc<str>>,
    /// Columnar facts (append hot path). `collections["facts"]` may lag until
    /// [`Self::ensure_facts_rows`].
    facts_s: Vec<Arc<str>>,
    facts_p: Vec<Arc<str>>,
    facts_o: Vec<Arc<str>>,
    /// When true, columnar facts are ahead of `collections["facts"]`.
    facts_rows_stale: bool,
    /// mmap cold collections not yet promoted into `collections`.
    cold: BTreeMap<String, crate::cold::ColdCol>,
}

/// Cheap undo for append-only packs (insert / append) — no full store clone.
#[derive(Debug, Clone)]
pub struct AppendMark {
    next_id: u64,
    col_lens: BTreeMap<String, usize>,
    edge_len: usize,
}

impl Store {
    pub fn empty(embed_id: impl Into<String>) -> Self {
        let mut collections = BTreeMap::new();
        for name in ["docs", "users", "orders", "facts"] {
            collections.insert(name.into(), Vec::new());
        }
        Self {
            r#gen: 0,
            embed_id: embed_id.into(),
            next_id: 1,
            collections,
            edges: Vec::new(),
            extra_collections: BTreeMap::new(),
            extra_rels: BTreeMap::new(),
            extra_indexes: BTreeMap::new(),
            indexes: BTreeMap::new(),
            fts: BTreeMap::new(),
            by_id: BTreeMap::new(),
            docs_by_uri: FxHashMap::default(),
            facts_by_spo: FxHashMap::default(),
            edge_keys: FxHashSet::default(),
            docs_id: Vec::new(),
            docs_title: Vec::new(),
            docs_layer: Vec::new(),
            docs_wing: Vec::new(),
            orders_id: Vec::new(),
            orders_user_id: Vec::new(),
            orders_total: Vec::new(),
            users_id: Vec::new(),
            users_email: Vec::new(),
            facts_s: Vec::new(),
            facts_p: Vec::new(),
            facts_o: Vec::new(),
            facts_rows_stale: false,
            cold: BTreeMap::new(),
        }
    }

    /// Deep-copy the memory image (no durable handle). Used for read snapshots.
    pub fn clone_mem(&mut self) -> Self {
        let mut s = Self::empty(self.embed_id.clone());
        s.mem_restore(self.mem_backup());
        s.fts = self.fts.clone();
        s
    }

    /// Load snapshot+log into memory (no writer lock / no Persist).
    /// Does not truncate the log or rewrite head — safe alongside a writer
    /// only if the caller coordinates; prefer [`crate::exec::Db::reader`] for
    /// in-process shared snapshots.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, Error> {
        let cat = crate::catalog::fixture();
        Ok(Self::open_with(path, &cat, OpenMemOpts::default())?.0)
    }

    /// Open durable data dir: memory image + exclusive Persist for writes.
    pub fn open_with(
        path: impl AsRef<Path>,
        cat: &Catalog,
        opts: OpenMemOpts,
    ) -> Result<(Self, Persist), Error> {
        let (store, persist, _) = Self::open_with_profiled(path, cat, opts)?;
        Ok((store, persist))
    }

    pub(crate) fn open_with_profiled(
        path: impl AsRef<Path>,
        cat: &Catalog,
        opts: OpenMemOpts,
    ) -> Result<(Self, Persist, StoreOpenPhases), Error> {
        let mut phases = StoreOpenPhases::default();
        let phase = Instant::now();
        let dir = path.as_ref();
        persist::ensure_dir(dir)?;
        let lock = persist::acquire_writer_lock(dir)?;
        let fixture_hash = catalog_hash(cat);
        phases.setup_ms = phase.elapsed().as_secs_f64() * 1000.0;

        let phase = Instant::now();
        let snap = persist::read_snapshot(dir)?;
        let mut store = match &snap {
            Some(s) => Self::from_snapshot_unindexed(dir, s)?,
            None => {
                let embed = persist::read_head(dir)?
                    .map(|h| h.embed_id)
                    .unwrap_or_else(|| cat.embed_id.clone());
                Self::empty(embed)
            }
        };
        phases.snapshot_ms = phase.elapsed().as_secs_f64() * 1000.0;

        let phase = Instant::now();
        store.rebuild_indexes();
        phases.indexes_ms = phase.elapsed().as_secs_f64() * 1000.0;
        let phase = Instant::now();
        store.rebuild_row_maps();
        phases.row_maps_ms = phase.elapsed().as_secs_f64() * 1000.0;

        let phase = Instant::now();
        let mut log = persist::open_log(dir)?;
        let min_gen = store.r#gen;
        let start = snap.as_ref().map(|s| s.log_offset).unwrap_or(0);
        let end = persist::replay_log(&mut log, min_gen, start, |rec| {
            store.apply_record(&rec);
            Ok(())
        })?;
        persist::truncate_log(&mut log, end)?;
        phases.wal_ms = phase.elapsed().as_secs_f64() * 1000.0;

        let phase = Instant::now();
        let mut live = cat.clone();
        store.merge_extras_into(&mut live);
        let live_hash = catalog_hash(&live);

        if let Some(head) = persist::read_head(dir)?
            && head.catalog_hash != live_hash
        {
            return Err(Error::runtime(format!(
                "catalog_hash mismatch: store={} catalog={}",
                head.catalog_hash, live_hash
            )));
        }
        if let Some(s) = &snap
            && s.catalog_hash != live_hash
            && s.extra_collections.is_empty()
            && s.extra_rels.is_empty()
            && s.extra_indexes.is_empty()
            && s.catalog_hash != fixture_hash
        {
            return Err(Error::runtime(format!(
                "catalog_hash mismatch: snapshot={} catalog={}",
                s.catalog_hash, live_hash
            )));
        }

        persist::write_head(
            dir,
            &Head {
                r#gen: store.r#gen,
                catalog_hash: live_hash.clone(),
                embed_id: store.embed_id.clone(),
            },
        )?;
        phases.metadata_ms = phase.elapsed().as_secs_f64() * 1000.0;

        let persist = Persist {
            dir: dir.to_path_buf(),
            log,
            lock,
            catalog_hash: live_hash,
            writes_since_snapshot: 0,
            encode_buf: Vec::with_capacity(64 * 1024),
            sync: opts.sync,
            cold: opts.cold,
            log_bytes: persist::log_len(dir).unwrap_or(0),
        };
        Ok((store, persist, phases))
    }

    /// Read-only open: replay into memory, do not truncate log or write head.
    /// Returns `(store, shared_lock)`.
    pub fn open_read_with(
        path: impl AsRef<Path>,
        cat: &Catalog,
    ) -> Result<(Self, std::fs::File), Error> {
        let (store, lock, _) = Self::open_read_with_profiled(path, cat)?;
        Ok((store, lock))
    }

    pub(crate) fn open_read_with_profiled(
        path: impl AsRef<Path>,
        cat: &Catalog,
    ) -> Result<(Self, std::fs::File, StoreOpenPhases), Error> {
        let mut phases = StoreOpenPhases::default();
        let phase = Instant::now();
        let dir = path.as_ref();
        if !dir.exists() {
            return Err(Error::runtime(format!(
                "data dir not found: {}",
                dir.display()
            )));
        }
        let lock = persist::acquire_reader_lock(dir)?;
        phases.setup_ms = phase.elapsed().as_secs_f64() * 1000.0;
        let phase = Instant::now();
        let snap = persist::read_snapshot(dir)?;
        let mut store = match &snap {
            Some(s) => Self::from_snapshot_unindexed(dir, s)?,
            None => {
                let embed = persist::read_head(dir)?
                    .map(|h| h.embed_id)
                    .unwrap_or_else(|| cat.embed_id.clone());
                Self::empty(embed)
            }
        };
        phases.snapshot_ms = phase.elapsed().as_secs_f64() * 1000.0;
        let phase = Instant::now();
        store.rebuild_indexes();
        phases.indexes_ms = phase.elapsed().as_secs_f64() * 1000.0;
        let phase = Instant::now();
        store.rebuild_row_maps();
        phases.row_maps_ms = phase.elapsed().as_secs_f64() * 1000.0;
        let phase = Instant::now();
        let mut log = persist::open_log_read(dir)?;
        let min_gen = store.r#gen;
        let start = snap.as_ref().map(|s| s.log_offset).unwrap_or(0);
        if let Some(log) = log.as_mut() {
            let _ = persist::replay_log(log, min_gen, start, |rec| {
                store.apply_record(&rec);
                Ok(())
            })?;
        }
        phases.wal_ms = phase.elapsed().as_secs_f64() * 1000.0;
        Ok((store, lock, phases))
    }

    pub(crate) fn from_snapshot(dir: &Path, s: &Snapshot) -> Result<Self, Error> {
        let mut store = Self::from_snapshot_unindexed(dir, s)?;
        store.rebuild_indexes();
        store.rebuild_row_maps();
        Ok(store)
    }

    fn from_snapshot_unindexed(dir: &Path, s: &Snapshot) -> Result<Self, Error> {
        let mut store = Self {
            r#gen: s.r#gen,
            embed_id: s.embed_id.clone(),
            next_id: s.next_id,
            collections: s.collections.clone(),
            edges: s.edges.clone(),
            extra_collections: s.extra_collections.clone(),
            extra_rels: s.extra_rels.clone(),
            extra_indexes: s.extra_indexes.clone(),
            indexes: BTreeMap::new(),
            fts: BTreeMap::new(),
            by_id: BTreeMap::new(),
            docs_by_uri: FxHashMap::default(),
            facts_by_spo: FxHashMap::default(),
            edge_keys: FxHashSet::default(),
            docs_id: Vec::new(),
            docs_title: Vec::new(),
            docs_layer: Vec::new(),
            docs_wing: Vec::new(),
            orders_id: Vec::new(),
            orders_user_id: Vec::new(),
            orders_total: Vec::new(),
            users_id: Vec::new(),
            users_email: Vec::new(),
            facts_s: Vec::new(),
            facts_p: Vec::new(),
            facts_o: Vec::new(),
            facts_rows_stale: false,
            cold: BTreeMap::new(),
        };
        for name in store.extra_collections.keys() {
            store.collections.entry(name.clone()).or_default();
        }
        for name in &s.cold_collections {
            if crate::cold::cold_path(dir, name).exists() {
                // Lazy: mmap only; rows page-in on first collection() access.
                let col = cold::map_cold(dir, name)?;
                store.collections.insert(name.clone(), Vec::new());
                store.cold.insert(name.clone(), col);
            }
        }
        Ok(store)
    }

    /// Capture current memory image (for backup / checkpoint payload).
    pub fn capture_snapshot(&mut self, catalog_hash: impl Into<String>) -> Snapshot {
        self.capture_snapshot_at(catalog_hash, 0)
    }

    pub fn capture_snapshot_at(
        &mut self,
        catalog_hash: impl Into<String>,
        log_offset: u64,
    ) -> Snapshot {
        self.ensure_facts_rows();
        let cold_names: Vec<String> = self.cold.keys().cloned().collect();
        for name in cold_names {
            self.promote_cold(&name);
        }
        Snapshot {
            r#gen: self.r#gen,
            embed_id: self.embed_id.clone(),
            next_id: self.next_id,
            catalog_hash: catalog_hash.into(),
            log_offset,
            collections: self.collections.clone(),
            edges: self.edges.clone(),
            extra_collections: self.extra_collections.clone(),
            extra_rels: self.extra_rels.clone(),
            extra_indexes: self.extra_indexes.clone(),
            cold_collections: Vec::new(),
        }
    }

    pub fn merge_extras_into(&self, cat: &mut Catalog) {
        for c in self.extra_collections.values() {
            let fields = c
                .fields
                .iter()
                .map(|(n, t)| (n.clone(), TypeExpr::Named(t.clone())))
                .collect();
            let _ = cat.apply_decl(&Decl::Col {
                name: c.name.clone(),
                append: c.append_only,
                fields,
            });
        }
        for r in self.extra_rels.values() {
            let decl = if let Some(of) = &r.reverse_of {
                Decl::RelReverse {
                    name: r.name.clone(),
                    of: of.clone(),
                }
            } else {
                Decl::Rel {
                    name: r.name.clone(),
                    stub: r.stub,
                }
            };
            let _ = cat.apply_decl(&decl);
        }
        for idx in self.extra_indexes.values() {
            let _ = cat.apply_decl(&Decl::Index {
                collection: idx.collection.clone(),
                unique: idx.unique,
                fields: idx.fields.clone(),
            });
        }
    }

    /// Flush one committed pack to the log (data sync). Head is updated durably
    /// on checkpoint/close only (soft head skipped on the hot path).
    pub fn durable_commit(&self, persist: &mut Persist, pack: Pack) -> Result<(), Error> {
        let rec = LogRecord {
            r#gen: self.r#gen + 1,
            next_id: self.next_id,
            pack,
        };
        let sync = persist.sync;
        {
            let Persist {
                log,
                encode_buf,
                log_bytes,
                ..
            } = persist;
            persist::append_record(log, &rec, encode_buf, sync, log_bytes)?;
        }
        persist.writes_since_snapshot += 1;
        Ok(())
    }

    pub fn maybe_checkpoint(&mut self, persist: &mut Persist) -> Result<(), Error> {
        if persist.writes_since_snapshot >= persist::SNAPSHOT_EVERY {
            self.write_snapshot(persist)?;
        }
        Ok(())
    }

    /// Force a durable checkpoint (snapshot + head fsync).
    pub fn checkpoint(&mut self, persist: &mut Persist) -> Result<(), Error> {
        self.write_snapshot(persist)
    }

    pub fn write_snapshot(&mut self, persist: &mut Persist) -> Result<(), Error> {
        // Exclude open_read processes while publishing snapshot + compacting log.
        let _fence = persist::acquire_fence_exclusive(&persist.dir)?;
        // Ensure log content is fully durable before publishing snapshot/head.
        // Normal mode: this is the durability point for prior commits.
        persist::durable_sync(&persist.log)
            .or_else(|_| persist.log.sync_all())
            .map_err(persist::io_err)?;
        let mut snap = self.capture_snapshot_at(persist.catalog_hash.clone(), 0);
        if persist.cold {
            // Cold bins are a decode cache; snapshot stays self-contained (rows kept).
            let mut cold_names = Vec::new();
            for (name, rows) in &snap.collections {
                if rows.len() >= cold::COLD_MIN_ROWS {
                    cold::write_cold(&persist.dir, name, rows)?;
                    cold_names.push(name.clone());
                }
            }
            cold::prune_cold(&persist.dir, &cold_names)?;
            snap.cold_collections = cold_names;
        }
        let head = Head {
            r#gen: self.r#gen,
            catalog_hash: persist.catalog_hash.clone(),
            embed_id: self.embed_id.clone(),
        };
        persist::write_snapshot(&persist.dir, &snap)?;
        persist::write_head(&persist.dir, &head)?;
        persist::compact_log(&mut persist.log)?;
        persist.writes_since_snapshot = 0;
        persist.log_bytes = 0;
        Ok(())
    }

    fn apply_record(&mut self, rec: &LogRecord) {
        self.apply_pack(&rec.pack);
        self.next_id = rec.next_id;
        self.r#gen = rec.r#gen;
    }

    pub fn apply_pack(&mut self, pack: &Pack) {
        match pack {
            Pack::Insert {
                collection,
                row,
                edges,
            } => {
                self.collection_mut(collection).push(row.clone());
                let idx = self.collection(collection).len() - 1;
                self.row_maps_register(collection, idx);
                let _ = self.index_insert_at(collection, idx);
                self.fts_insert_at(collection, idx);
                for e in edges {
                    let _ = self.append_edge_parts(&e.rel, &e.from, &e.to);
                }
            }
            Pack::InsertBulk {
                collection,
                rows,
                edges,
            } => {
                let start = self.collection(collection).len();
                self.collection_mut(collection).reserve(rows.len());
                self.row_maps_reserve(collection, rows.len());
                let _ = self.index_insert_slab(collection, start, rows);
                self.fts_insert_slab(collection, start, rows);
                self.row_maps_register_slab(collection, start, rows);
                self.collection_mut(collection).extend(rows.iter().cloned());
                for e in edges {
                    let _ = self.append_edge_parts(&e.rel, &e.from, &e.to);
                }
            }
            Pack::InsertCols {
                collection,
                fields,
                cols,
                n,
                edges,
            } => {
                let n = *n as usize;
                let rows = persist::cols_to_rows(fields, cols, n);
                let start = self.collection(collection).len();
                self.collection_mut(collection).reserve(rows.len());
                self.row_maps_reserve(collection, rows.len());
                let _ = self.index_insert_slab(collection, start, &rows);
                self.fts_insert_slab(collection, start, &rows);
                self.row_maps_register_slab(collection, start, &rows);
                self.collection_mut(collection).extend(rows);
                for e in edges {
                    let _ = self.append_edge_parts(&e.rel, &e.from, &e.to);
                }
            }
            Pack::AppendFact { row } => {
                let _ = self.append_fact_row(row.clone());
            }
            Pack::AppendFactsBulk { s, p, o } => {
                let _ = self.append_facts_spo_bulk(s, p, o, false);
            }
            Pack::AppendEdge { rel, from, to } => {
                let _ = self.append_edge_parts(rel, from, to);
            }
            Pack::AppendEdgesBulk { rel, from, to } => {
                let n = rel.len().min(from.len()).min(to.len());
                self.edges.reserve(n);
                self.edge_keys.reserve(n);
                for i in 0..n {
                    let _ = self.append_edge_parts(&rel[i], &from[i], &to[i]);
                }
            }
            Pack::Update { collection, rows } => {
                for new in rows {
                    let id = row_text(new, "id").map(str::to_string);
                    let idx = id.as_ref().and_then(|id| self.row_index(collection, id));
                    if let Some(i) = idx {
                        self.index_remove_at(collection, i);
                        self.fts_remove_at(collection, i);
                        self.collection_mut(collection)[i] = new.clone();
                        self.row_maps_reregister(collection, i);
                        let _ = self.index_insert_at(collection, i);
                        self.fts_insert_at(collection, i);
                        continue;
                    }
                    self.collection_mut(collection).push(new.clone());
                    let i = self.collection(collection).len() - 1;
                    self.row_maps_register(collection, i);
                    let _ = self.index_insert_at(collection, i);
                    self.fts_insert_at(collection, i);
                }
            }
            Pack::Reembed => {}
            Pack::Delete { collection, rows } => {
                self.collection_mut(collection)
                    .retain(|r| !rows.iter().any(|d| same_row_key(collection, r, d)));
                self.rebuild_row_maps_collection(collection);
                self.rebuild_indexes_collection(collection);
                self.rebuild_fts_inplace(collection);
            }
            Pack::DeleteEdge { rel, from, to } => {
                self.remove_edge(rel, from, to);
            }
            Pack::SchemaCol {
                name,
                append,
                fields,
            } => {
                self.extra_collections.insert(
                    name.clone(),
                    ColSnap {
                        name: name.clone(),
                        append_only: *append,
                        fields: fields.clone(),
                    },
                );
                self.collections.entry(name.clone()).or_default();
            }
            Pack::SchemaRel {
                name,
                stub,
                reverse_of,
            } => {
                self.extra_rels.insert(
                    name.clone(),
                    RelSnap {
                        name: name.clone(),
                        stub: *stub,
                        reverse_of: reverse_of.clone(),
                    },
                );
            }
            Pack::SchemaIndex {
                collection,
                unique,
                fields,
            } => {
                let snap = IndexSnap {
                    collection: collection.clone(),
                    unique: *unique,
                    fields: fields.clone(),
                };
                let label = format!("{}[{}]", collection, fields.join(","));
                self.extra_indexes.insert(label.clone(), snap);
                let def = crate::catalog::IndexDef {
                    collection: collection.clone(),
                    unique: *unique,
                    fields: fields.clone(),
                };
                let mut live = LiveIndex::new(def);
                for (i, row) in self.collection(collection).iter().enumerate() {
                    let _ = live.insert_at(i, row);
                }
                self.indexes.insert(label, live);
            }
            Pack::Batch { packs } => {
                for p in packs {
                    self.apply_pack(p);
                }
            }
        }
    }

    pub fn mem_backup(&mut self) -> MemBackup {
        self.ensure_facts_rows();
        let cold_names: Vec<String> = self.cold.keys().cloned().collect();
        for name in cold_names {
            self.promote_cold(&name);
        }
        MemBackup {
            r#gen: self.r#gen,
            embed_id: self.embed_id.clone(),
            next_id: self.next_id,
            collections: self.collections.clone(),
            edges: self.edges.clone(),
            extra_collections: self.extra_collections.clone(),
            extra_rels: self.extra_rels.clone(),
            extra_indexes: self.extra_indexes.clone(),
        }
    }

    pub fn mem_restore(&mut self, b: MemBackup) {
        self.r#gen = b.r#gen;
        self.embed_id = b.embed_id;
        self.next_id = b.next_id;
        self.collections = b.collections;
        self.edges = b.edges;
        self.extra_collections = b.extra_collections;
        self.extra_rels = b.extra_rels;
        self.extra_indexes = b.extra_indexes;
        self.rebuild_indexes();
        self.rebuild_row_maps();
        self.rebuild_all_fts_inplace();
    }

    /// Mark lengths for append-only rollback (insert/append packs).
    pub fn append_mark(&self) -> AppendMark {
        let mut col_lens = BTreeMap::new();
        for (name, rows) in &self.collections {
            col_lens.insert(name.clone(), rows.len());
        }
        AppendMark {
            next_id: self.next_id,
            col_lens,
            edge_len: self.edges.len(),
        }
    }

    pub fn append_rollback(&mut self, mark: AppendMark) {
        self.next_id = mark.next_id;
        self.edges.truncate(mark.edge_len);
        for (name, rows) in self.collections.iter_mut() {
            let len = mark.col_lens.get(name).copied().unwrap_or(0);
            rows.truncate(len);
        }
        // Drop collections created after the mark (schema col during append-only is rare;
        // full backup covers schema). Keep extras as-is for append-only path.
        self.rebuild_indexes();
        self.rebuild_row_maps();
        self.rebuild_all_fts_inplace();
    }

    fn rebuild_all_fts_inplace(&mut self) {
        let names = self.fts.keys().cloned().collect::<Vec<_>>();
        for name in names {
            self.rebuild_fts_inplace(&name);
        }
    }

    pub fn fixture(cat: &Catalog) -> Self {
        let mut s = Self::empty(cat.embed_id.clone());
        let now = now_ms();
        let overview = "e7c98d54-b4d6-4165-86e9-9b999e7ce9c3";
        let wal = "d-wal";
        let facts = "d-facts";

        s.collections.insert(
            "docs".into(),
            vec![
                doc_row(DocSeed {
                    id: overview,
                    uri: "wiki://rag-overview",
                    title: "RAG overview",
                    wing: "rag",
                    room: "wiki",
                    layer: "wiki",
                    body: "hybrid search and embedding identity",
                    snippet: "overview of rag",
                    ts: now - 86_400_000,
                }),
                doc_row(DocSeed {
                    id: wal,
                    uri: "raw://n/wal",
                    title: "write ahead wal",
                    wing: "rag",
                    room: "inbox",
                    layer: "raw",
                    body: "wal notes for the reducer",
                    snippet: "wal snippet",
                    ts: now - 7_200_000,
                }),
                doc_row(DocSeed {
                    id: facts,
                    uri: "wiki://lin-facts",
                    title: "Lin facts",
                    wing: "sys",
                    room: "meta",
                    layer: "wiki",
                    body: "catalog and facts",
                    snippet: "lin facts",
                    ts: now - 3 * 86_400_000,
                }),
            ],
        );
        s.collections.insert(
            "users".into(),
            vec![
                user_row("u1", "alice@lin.dev"),
                user_row("u2", "bob@lin.dev"),
            ],
        );
        s.collections.insert(
            "orders".into(),
            vec![
                order_row("o1", "u1", 150.0, now - 86_400_000),
                order_row("o2", "u2", 40.0, now - 2 * 86_400_000),
            ],
        );
        s.edges.push(Edge {
            rel: "wikilink".into(),
            from: wal.into(),
            to: overview.into(),
        });
        s.edges.push(Edge {
            rel: "tagged".into(),
            from: overview.into(),
            to: facts.into(),
        });
        s.next_id = 10;
        s.rebuild_row_maps();
        s
    }

    pub fn alloc_id(&mut self) -> String {
        let id = format!("id-{}", self.next_id);
        self.next_id += 1;
        id
    }

    /// Facts row count (columnar or materialized).
    pub fn facts_len(&self) -> usize {
        self.facts_s
            .len()
            .max(self.collections.get("facts").map(|c| c.len()).unwrap_or(0))
    }

    /// Row count including columnar facts not yet materialized into `collections`.
    pub fn row_count(&self) -> usize {
        let mut n = 0usize;
        for (name, rows) in &self.collections {
            if name == "facts" && self.facts_rows_stale {
                n += self.facts_s.len();
            } else if rows.is_empty() {
                if let Some(c) = self.cold.get(name) {
                    n += c.rows().map(|r| r.len()).unwrap_or(0);
                }
            } else {
                n += rows.len();
            }
        }
        n
    }

    /// Materialize columnar facts into `collections["facts"]` (no-op if fresh).
    pub fn ensure_facts_rows(&mut self) {
        if !self.facts_rows_stale {
            return;
        }
        let n = self.facts_s.len();
        let facts = self.collections.entry("facts".into()).or_default();
        facts.clear();
        facts.reserve(n);
        for i in 0..n {
            let mut row = BTreeMap::new();
            row.insert("s".into(), Cell::Text(Arc::clone(&self.facts_s[i])));
            row.insert("p".into(), Cell::Text(Arc::clone(&self.facts_p[i])));
            row.insert("o".into(), Cell::Text(Arc::clone(&self.facts_o[i])));
            facts.push(row);
        }
        self.facts_rows_stale = false;
    }

    /// Promote a cold mmap collection into `collections` (idempotent).
    pub fn promote_cold(&mut self, name: &str) {
        let Some(col) = self.cold.remove(name) else {
            return;
        };
        if let Ok(rows) = col.into_rows() {
            self.collections.insert(name.to_string(), rows);
        }
    }

    pub fn collection(&self, name: &str) -> &[Row] {
        if let Some(c) = self.cold.get(name) {
            if let Ok(rows) = c.rows() {
                return rows;
            }
        }
        self.collections
            .get(name)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    pub fn collection_mut(&mut self, name: &str) -> &mut Vec<Row> {
        if name == "facts" {
            self.ensure_facts_rows();
        }
        self.promote_cold(name);
        if let Some(v) = self.collections.get_mut(name) {
            return v;
        }
        self.collections.entry(name.to_string()).or_default()
    }

    pub fn find_doc_key(&self, key: &str) -> Option<&Row> {
        self.get_by_id("docs", key).or_else(|| self.get_by_uri(key))
    }

    pub fn get_by_id(&self, collection: &str, id: &str) -> Option<&Row> {
        let idx = self.row_index(collection, id)?;
        self.collections.get(collection)?.get(idx)
    }

    pub fn get_by_uri(&self, uri: &str) -> Option<&Row> {
        let idx = *self.docs_by_uri.get(uri)?;
        self.collections.get("docs")?.get(idx)
    }

    pub fn row_index(&self, collection: &str, id: &str) -> Option<usize> {
        self.by_id.get(collection)?.get(id).copied()
    }

    pub fn get_by_idx(&self, collection: &str, idx: usize) -> Option<&Row> {
        self.collections.get(collection)?.get(idx)
    }

    pub fn rows_by_idxs(&self, collection: &str, idxs: &[usize]) -> Vec<Row> {
        let mut out = Vec::with_capacity(idxs.len());
        for &i in idxs {
            if let Some(row) = self.get_by_idx(collection, i) {
                out.push(row.clone());
            }
        }
        out
    }

    pub fn rows_by_ids(&self, collection: &str, ids: &[String]) -> Vec<Row> {
        let mut out = Vec::with_capacity(ids.len());
        for id in ids {
            if let Some(row) = self.get_by_id(collection, id) {
                out.push(row.clone());
            }
        }
        out
    }

    pub fn project_by_id(&self, collection: &str, id: &str, fields: &[String]) -> Option<Row> {
        let row = self.get_by_id(collection, id)?;
        Some(project_fields(row, fields))
    }

    pub fn project_by_key(
        &self,
        collection: &str,
        field: &str,
        key: &str,
        fields: Option<&[String]>,
    ) -> Option<Row> {
        // docs hot project: columnar Arc clones, skip full BTreeMap row.
        if collection == "docs"
            && let Some(fs) = fields
            && !fs.is_empty()
            && fs.iter().all(|f| docs_hot_field(f))
        {
            let idx = match field {
                "id" => self.row_index("docs", key)?,
                "uri" => *self.docs_by_uri.get(key)?,
                _ => return None,
            };
            return self.project_docs_hot_idx(idx, fs);
        }
        let row = match field {
            "id" => self.get_by_id(collection, key)?,
            "uri" if collection == "docs" => self.get_by_uri(key)?,
            _ => return None,
        };
        Some(match fields {
            Some(fs) if !fs.is_empty() => project_fields(row, fs),
            _ => row.clone(),
        })
    }

    /// Single-row hot projection from parallel docs columns.
    pub fn project_docs_hot_idx(&self, i: usize, fields: &[String]) -> Option<Row> {
        if i >= self.docs_id.len() {
            return None;
        }
        if fields.len() == 2 && fields[0] == "id" && fields[1] == "title" {
            return Some(row_id_title(&self.docs_id[i], &self.docs_title[i]));
        }
        let mut row = BTreeMap::new();
        for f in fields {
            let cell = match f.as_str() {
                "id" => Cell::Text(Arc::clone(&self.docs_id[i])),
                "title" => Cell::Text(Arc::clone(&self.docs_title[i])),
                "layer" => Cell::Text(Arc::clone(&self.docs_layer[i])),
                "wing" => Cell::Text(Arc::clone(&self.docs_wing[i])),
                _ => return None,
            };
            row.insert(f.clone(), cell);
        }
        Some(row)
    }

    pub fn rebuild_row_maps(&mut self) {
        self.ensure_facts_rows();
        self.by_id.clear();
        self.docs_by_uri.clear();
        self.facts_by_spo.clear();
        self.edge_keys.clear();
        let names: Vec<String> = self.collections.keys().cloned().collect();
        for name in names {
            self.rebuild_row_maps_collection(&name);
        }
        self.rebuild_edge_keys();
        // Keep columnar facts aligned with materialized rows.
        self.facts_s.clear();
        self.facts_p.clear();
        self.facts_o.clear();
        if let Some(facts) = self.collections.get("facts") {
            self.facts_s.reserve(facts.len());
            self.facts_p.reserve(facts.len());
            self.facts_o.reserve(facts.len());
            for row in facts {
                if let Some((s, p, o)) = spo_key(row) {
                    self.facts_s.push(s);
                    self.facts_p.push(p);
                    self.facts_o.push(o);
                } else {
                    self.facts_s.push(Arc::from(""));
                    self.facts_p.push(Arc::from(""));
                    self.facts_o.push(Arc::from(""));
                }
            }
        }
        self.facts_rows_stale = false;
    }

    fn rebuild_edge_keys(&mut self) {
        self.edge_keys.clear();
        self.edge_keys.reserve(self.edges.len());
        for e in &self.edges {
            self.edge_keys.insert((
                Arc::<str>::from(e.rel.as_str()),
                Arc::<str>::from(e.from.as_str()),
                Arc::<str>::from(e.to.as_str()),
            ));
        }
    }

    /// Idempotent fact insert. Returns `(row, changed)`.
    pub fn append_fact_row(&mut self, row: Row) -> (Row, bool) {
        let Some(key) = spo_key(&row) else {
            self.ensure_facts_rows();
            self.collection_mut("facts").push(row);
            let row = self.collection("facts").last().unwrap().clone();
            return (row, true);
        };
        if let Some(&idx) = self.facts_by_spo.get(&key) {
            self.ensure_facts_rows();
            return (self.collection("facts")[idx].clone(), false);
        }
        let sa = Arc::clone(&key.0);
        let pa = Arc::clone(&key.1);
        let oa = Arc::clone(&key.2);
        let idx = self.facts_s.len();
        self.facts_by_spo.insert(key, idx);
        self.facts_s.push(sa);
        self.facts_p.push(pa);
        self.facts_o.push(oa);
        self.facts_rows_stale = true;
        self.ensure_facts_rows();
        let out = self.collection("facts")[idx].clone();
        (out, true)
    }

    /// Bulk append facts from columnar s/p/o.
    /// When `want_rows` is false, skips `BTreeMap` row builds (materialize later via
    /// [`Self::ensure_facts_rows`]) — keeps durable_append sync off the alloc path.
    pub fn append_facts_spo_bulk(
        &mut self,
        s: &[String],
        p: &[String],
        o: &[String],
        want_rows: bool,
    ) -> (Vec<Row>, usize) {
        let n = s.len().min(p.len()).min(o.len());
        self.facts_by_spo.reserve(n);
        self.facts_s.reserve(n);
        self.facts_p.reserve(n);
        self.facts_o.reserve(n);
        let mut rows = if want_rows {
            Vec::with_capacity(n)
        } else {
            Vec::new()
        };
        let mut changed = 0usize;
        for i in 0..n {
            let sa: Arc<str> = Arc::from(s[i].as_str());
            let pa: Arc<str> = Arc::from(p[i].as_str());
            let oa: Arc<str> = Arc::from(o[i].as_str());
            let key = (Arc::clone(&sa), Arc::clone(&pa), Arc::clone(&oa));
            if let Some(&idx) = self.facts_by_spo.get(&key) {
                if want_rows {
                    self.ensure_facts_rows();
                    rows.push(self.collections["facts"][idx].clone());
                }
                continue;
            }
            let idx = self.facts_s.len();
            self.facts_by_spo.insert(key, idx);
            self.facts_s.push(sa);
            self.facts_p.push(pa);
            self.facts_o.push(oa);
            self.facts_rows_stale = true;
            changed += 1;
            if want_rows {
                // Build only the returned row; full vec sync via ensure later.
                let mut row = BTreeMap::new();
                row.insert("s".into(), Cell::Text(Arc::clone(&self.facts_s[idx])));
                row.insert("p".into(), Cell::Text(Arc::clone(&self.facts_p[idx])));
                row.insert("o".into(), Cell::Text(Arc::clone(&self.facts_o[idx])));
                rows.push(row);
            }
        }
        if want_rows && changed > 0 {
            self.ensure_facts_rows();
        }
        (rows, changed)
    }

    pub fn append_edge_parts(&mut self, rel: &str, from: &str, to: &str) -> bool {
        let key = (
            Arc::<str>::from(rel),
            Arc::<str>::from(from),
            Arc::<str>::from(to),
        );
        if !self.edge_keys.insert(key.clone()) {
            return false;
        }
        self.edges.push(Edge {
            rel: key.0.as_ref().to_owned(),
            from: key.1.as_ref().to_owned(),
            to: key.2.as_ref().to_owned(),
        });
        true
    }

    pub fn has_edge(&self, rel: &str, from: &str, to: &str) -> bool {
        self.edge_keys.contains(&(
            Arc::<str>::from(rel),
            Arc::<str>::from(from),
            Arc::<str>::from(to),
        ))
    }

    pub fn remove_edge(&mut self, rel: &str, from: &str, to: &str) -> bool {
        let key = (
            Arc::<str>::from(rel),
            Arc::<str>::from(from),
            Arc::<str>::from(to),
        );
        if !self.edge_keys.remove(&key) {
            return false;
        }
        self.edges
            .retain(|e| !(e.rel == rel && e.from == from && e.to == to));
        true
    }

    pub fn rebuild_row_maps_collection(&mut self, collection: &str) {
        self.by_id
            .entry(collection.to_string())
            .or_default()
            .clear();
        if collection == "docs" {
            self.docs_by_uri.clear();
            self.docs_id.clear();
            self.docs_title.clear();
            self.docs_layer.clear();
            self.docs_wing.clear();
        }
        if collection == "orders" {
            self.orders_id.clear();
            self.orders_user_id.clear();
            self.orders_total.clear();
        }
        if collection == "users" {
            self.users_id.clear();
            self.users_email.clear();
        }
        if collection == "facts" {
            self.facts_by_spo.clear();
        }
        let Some(rows) = self.collections.get(collection) else {
            return;
        };
        let n = rows.len();
        if collection == "docs" {
            self.docs_id.reserve(n);
            self.docs_title.reserve(n);
            self.docs_layer.reserve(n);
            self.docs_wing.reserve(n);
        }
        if collection == "orders" {
            self.orders_id.reserve(n);
            self.orders_user_id.reserve(n);
            self.orders_total.reserve(n);
        }
        if collection == "users" {
            self.users_id.reserve(n);
            self.users_email.reserve(n);
        }
        // Snapshot first so we can mutate maps without overlapping borrows.
        let snaps: Vec<(
            Option<String>,
            Option<String>,
            Arc<str>,
            Arc<str>,
            Arc<str>,
            Arc<str>,
            Option<(Arc<str>, Arc<str>, Arc<str>)>,
            Arc<str>, // user_id / email extra
            f64,      // total
        )> = rows
            .iter()
            .map(|row| {
                (
                    row_text(row, "id").map(str::to_string),
                    row_text(row, "uri").map(str::to_string),
                    row.get("id")
                        .and_then(Cell::text_shared)
                        .unwrap_or_default(),
                    row.get("title")
                        .and_then(Cell::text_shared)
                        .unwrap_or_default(),
                    row.get("layer")
                        .and_then(Cell::text_shared)
                        .unwrap_or_default(),
                    row.get("wing")
                        .and_then(Cell::text_shared)
                        .unwrap_or_default(),
                    if collection == "facts" {
                        spo_key(row)
                    } else {
                        None
                    },
                    if collection == "orders" {
                        row.get("user_id")
                            .and_then(Cell::text_shared)
                            .unwrap_or_default()
                    } else if collection == "users" {
                        row.get("email")
                            .and_then(Cell::text_shared)
                            .unwrap_or_default()
                    } else {
                        Arc::<str>::from("")
                    },
                    if collection == "orders" {
                        row.get("total").and_then(Cell::as_f64).unwrap_or(0.0)
                    } else {
                        0.0
                    },
                )
            })
            .collect();
        for (i, (id, uri, did, title, layer, wing, spo, extra, total)) in
            snaps.into_iter().enumerate()
        {
            if let Some(id) = id {
                self.by_id
                    .get_mut(collection)
                    .expect("by_id entry")
                    .insert(id, i);
            }
            if let Some(spo) = spo {
                self.facts_by_spo.insert(spo, i);
            }
            if collection == "docs" {
                if let Some(uri) = uri {
                    self.docs_by_uri.insert(uri, i);
                }
                self.docs_id.push(did);
                self.docs_title.push(title);
                self.docs_layer.push(layer);
                self.docs_wing.push(wing);
            } else if collection == "orders" {
                self.orders_id.push(did);
                self.orders_user_id.push(extra);
                self.orders_total.push(total);
            } else if collection == "users" {
                self.users_id.push(did);
                self.users_email.push(extra);
            }
        }
    }

    pub fn row_maps_register(&mut self, collection: &str, idx: usize) {
        let snaps = self
            .collections
            .get(collection)
            .and_then(|c| c.get(idx))
            .map(|row| {
                (
                    row.get("id").and_then(Cell::text_shared),
                    row.get("uri").and_then(Cell::text_shared),
                    row.get("title").and_then(Cell::text_shared),
                    row.get("layer").and_then(Cell::text_shared),
                    row.get("wing").and_then(Cell::text_shared),
                )
            });
        let Some((id, uri, title, layer, wing)) = snaps else {
            return;
        };
        if let Some(ref id) = id {
            self.by_id
                .entry(collection.to_string())
                .or_default()
                .insert(id.as_ref().to_owned(), idx);
        }
        if collection == "docs" {
            let did = id.unwrap_or_default();
            let title = title.unwrap_or_default();
            let layer = layer.unwrap_or_default();
            let wing = wing.unwrap_or_default();
            if let Some(uri) = uri {
                self.docs_by_uri.insert(uri.as_ref().to_owned(), idx);
            }
            if idx == self.docs_id.len() {
                self.docs_id.push(did);
                self.docs_title.push(title);
                self.docs_layer.push(layer);
                self.docs_wing.push(wing);
            } else if idx < self.docs_id.len() {
                self.docs_id[idx] = did;
                self.docs_title[idx] = title;
                self.docs_layer[idx] = layer;
                self.docs_wing[idx] = wing;
            } else {
                self.rebuild_row_maps_collection("docs");
            }
        }
        if collection == "orders" || collection == "users" {
            self.row_maps_register_soa(collection, idx);
        }
    }

    fn row_maps_register_soa(&mut self, collection: &str, idx: usize) {
        let Some(row) = self.collections.get(collection).and_then(|c| c.get(idx)) else {
            return;
        };
        let id = row
            .get("id")
            .and_then(Cell::text_shared)
            .unwrap_or_default();
        if collection == "orders" {
            let uid = row
                .get("user_id")
                .and_then(Cell::text_shared)
                .unwrap_or_default();
            let total = row.get("total").and_then(Cell::as_f64).unwrap_or(0.0);
            if idx == self.orders_id.len() {
                self.orders_id.push(id);
                self.orders_user_id.push(uid);
                self.orders_total.push(total);
            } else if idx < self.orders_id.len() {
                self.orders_id[idx] = id;
                self.orders_user_id[idx] = uid;
                self.orders_total[idx] = total;
            } else {
                self.rebuild_row_maps_collection("orders");
            }
        } else if collection == "users" {
            let email = row
                .get("email")
                .and_then(Cell::text_shared)
                .unwrap_or_default();
            if idx == self.users_id.len() {
                self.users_id.push(id);
                self.users_email.push(email);
            } else if idx < self.users_id.len() {
                self.users_id[idx] = id;
                self.users_email[idx] = email;
            } else {
                self.rebuild_row_maps_collection("users");
            }
        }
    }

    /// Register maps/columns from an already-built row (avoids re-fetch).
    pub fn row_maps_register_row(&mut self, collection: &str, idx: usize, row: &Row) {
        let id = row.get("id").and_then(Cell::text_shared);
        let uri = row.get("uri").and_then(Cell::text_shared);
        if let Some(ref id) = id {
            self.by_id
                .entry(collection.to_string())
                .or_default()
                .insert(id.as_ref().to_owned(), idx);
        }
        if collection == "docs" {
            let title = row
                .get("title")
                .and_then(Cell::text_shared)
                .unwrap_or_default();
            let layer = row
                .get("layer")
                .and_then(Cell::text_shared)
                .unwrap_or_default();
            let wing = row
                .get("wing")
                .and_then(Cell::text_shared)
                .unwrap_or_default();
            let did = id.clone().unwrap_or_default();
            if let Some(uri) = uri {
                self.docs_by_uri.insert(uri.as_ref().to_owned(), idx);
            }
            if idx == self.docs_id.len() {
                self.docs_id.push(did);
                self.docs_title.push(title);
                self.docs_layer.push(layer);
                self.docs_wing.push(wing);
            } else if idx < self.docs_id.len() {
                self.docs_id[idx] = did;
                self.docs_title[idx] = title;
                self.docs_layer[idx] = layer;
                self.docs_wing[idx] = wing;
            } else {
                self.rebuild_row_maps_collection("docs");
            }
        }
        if collection == "orders" {
            let uid = row
                .get("user_id")
                .and_then(Cell::text_shared)
                .unwrap_or_default();
            let total = row.get("total").and_then(Cell::as_f64).unwrap_or(0.0);
            let did = id.clone().unwrap_or_default();
            if idx == self.orders_id.len() {
                self.orders_id.push(did);
                self.orders_user_id.push(uid);
                self.orders_total.push(total);
            } else if idx < self.orders_id.len() {
                self.orders_id[idx] = did;
                self.orders_user_id[idx] = uid;
                self.orders_total[idx] = total;
            } else {
                self.rebuild_row_maps_collection("orders");
            }
        }
        if collection == "users" {
            let email = row
                .get("email")
                .and_then(Cell::text_shared)
                .unwrap_or_default();
            let did = id.clone().unwrap_or_default();
            if idx == self.users_id.len() {
                self.users_id.push(did);
                self.users_email.push(email);
            } else if idx < self.users_id.len() {
                self.users_id[idx] = did;
                self.users_email[idx] = email;
            } else {
                self.rebuild_row_maps_collection("users");
            }
        }
    }

    /// Append-only slab: register `[start, start+rows.len())` from the given rows.
    pub fn row_maps_register_slab(&mut self, collection: &str, start: usize, rows: &[Row]) {
        if rows.is_empty() {
            return;
        }
        self.row_maps_reserve(collection, rows.len());
        if collection == "docs" {
            if self.docs_id.len() != start {
                self.rebuild_row_maps_collection("docs");
                if self.docs_id.len() != start {
                    for (i, row) in rows.iter().enumerate() {
                        self.row_maps_register_row(collection, start + i, row);
                    }
                    return;
                }
            }
            for (i, row) in rows.iter().enumerate() {
                let idx = start + i;
                let id = row.get("id").and_then(Cell::text_shared);
                let uri = row.get("uri").and_then(Cell::text_shared);
                if let Some(ref id) = id {
                    self.by_id
                        .entry(collection.to_string())
                        .or_default()
                        .insert(id.as_ref().to_owned(), idx);
                }
                if let Some(uri) = uri {
                    self.docs_by_uri.insert(uri.as_ref().to_owned(), idx);
                }
                self.docs_id.push(id.unwrap_or_default());
                self.docs_title.push(
                    row.get("title")
                        .and_then(Cell::text_shared)
                        .unwrap_or_default(),
                );
                self.docs_layer.push(
                    row.get("layer")
                        .and_then(Cell::text_shared)
                        .unwrap_or_default(),
                );
                self.docs_wing.push(
                    row.get("wing")
                        .and_then(Cell::text_shared)
                        .unwrap_or_default(),
                );
            }
            return;
        }
        for (i, row) in rows.iter().enumerate() {
            self.row_maps_register_row(collection, start + i, row);
        }
    }

    /// Reserve row-map capacity before a bulk insert.
    pub fn row_maps_reserve(&mut self, collection: &str, additional: usize) {
        self.by_id
            .entry(collection.to_string())
            .or_default()
            .reserve(additional);
        if collection == "docs" {
            self.docs_by_uri.reserve(additional);
            self.docs_id.reserve(additional);
            self.docs_title.reserve(additional);
            self.docs_layer.reserve(additional);
            self.docs_wing.reserve(additional);
        }
        if collection == "orders" {
            self.orders_id.reserve(additional);
            self.orders_user_id.reserve(additional);
            self.orders_total.reserve(additional);
        }
        if collection == "users" {
            self.users_id.reserve(additional);
            self.users_email.reserve(additional);
        }
    }

    pub fn row_maps_reregister(&mut self, collection: &str, idx: usize) {
        // uri may have changed; safest is rebuild one collection (small for updates).
        self.rebuild_row_maps_collection(collection);
        let _ = idx;
    }

    pub fn index_insert_at(&mut self, collection: &str, row_idx: usize) -> Result<(), Error> {
        let Some(row) = self
            .collections
            .get(collection)
            .and_then(|c| c.get(row_idx))
            .cloned()
        else {
            return Ok(());
        };
        self.index_insert_row(collection, row_idx, &row)
    }

    pub fn index_insert_row(
        &mut self,
        collection: &str,
        row_idx: usize,
        row: &Row,
    ) -> Result<(), Error> {
        let labels: Vec<String> = self
            .indexes
            .iter()
            .filter(|(_, idx)| idx.def.collection == collection)
            .map(|(k, _)| k.clone())
            .collect();
        for label in labels {
            if let Some(idx) = self.indexes.get_mut(&label)
                && let Err(e) = idx.insert_at(row_idx, row)
            {
                return Err(Error::runtime(e));
            }
        }
        Ok(())
    }

    /// Batch secondary-index inserts for a freshly appended slab `[start, start+rows.len())`.
    pub fn index_insert_slab(
        &mut self,
        collection: &str,
        start: usize,
        rows: &[Row],
    ) -> Result<(), Error> {
        let labels: Vec<String> = self
            .indexes
            .iter()
            .filter(|(_, idx)| idx.def.collection == collection)
            .map(|(k, _)| k.clone())
            .collect();
        if labels.is_empty() {
            return Ok(());
        }
        for (i, row) in rows.iter().enumerate() {
            let row_idx = start + i;
            for label in &labels {
                if let Some(idx) = self.indexes.get_mut(label)
                    && let Err(e) = idx.insert_at_new(row_idx, row)
                {
                    return Err(Error::runtime(e));
                }
            }
        }
        Ok(())
    }

    pub fn index_remove_at(&mut self, collection: &str, row_idx: usize) {
        for idx in self.indexes.values_mut() {
            if idx.def.collection == collection {
                idx.remove_at(row_idx);
            }
        }
    }

    pub fn rebuild_indexes(&mut self) {
        self.indexes.clear();
        let snaps = self.extra_indexes.clone();
        for (label, snap) in snaps {
            let def = crate::catalog::IndexDef {
                collection: snap.collection.clone(),
                unique: snap.unique,
                fields: snap.fields.clone(),
            };
            let mut live = LiveIndex::new(def);
            for (i, row) in self.collection(&snap.collection).iter().enumerate() {
                let _ = live.insert_at(i, row);
            }
            self.indexes.insert(label, live);
        }
    }

    /// Rebuild FTS postings from catalog `fts` fields (rebuild-on-open / after writes).
    pub fn rebuild_fts(&mut self, catalog: &crate::catalog::Catalog) {
        self.fts.clear();
        for (cname, cdef) in &catalog.collections {
            let fields: Vec<String> = cdef
                .fields
                .iter()
                .filter(|(_, f)| f.fts)
                .map(|(n, _)| n.clone())
                .collect();
            if fields.is_empty() {
                continue;
            }
            let rows = self.collection(cname);
            self.fts
                .insert(cname.clone(), crate::fts::FtsIndex::build(rows, &fields));
        }
    }

    pub fn rebuild_fts_collection(&mut self, catalog: &crate::catalog::Catalog, collection: &str) {
        let Some(cdef) = catalog.collections.get(collection) else {
            self.fts.remove(collection);
            return;
        };
        let fields: Vec<String> = cdef
            .fields
            .iter()
            .filter(|(_, f)| f.fts)
            .map(|(n, _)| n.clone())
            .collect();
        if fields.is_empty() {
            self.fts.remove(collection);
            return;
        }
        let rows = self.collection(collection);
        self.fts.insert(
            collection.to_string(),
            crate::fts::FtsIndex::build(rows, &fields),
        );
    }

    pub fn fts_insert_at(&mut self, collection: &str, row_idx: usize) {
        let Some(row) = self
            .collections
            .get(collection)
            .and_then(|c| c.get(row_idx))
            .cloned()
        else {
            return;
        };
        if let Some(fts) = self.fts.get_mut(collection) {
            fts.insert_row(row_idx, &row);
        }
    }

    pub fn fts_insert_slab(&mut self, collection: &str, start: usize, rows: &[Row]) {
        let Some(fts) = self.fts.get_mut(collection) else {
            return;
        };
        for (i, row) in rows.iter().enumerate() {
            fts.insert_row(start + i, row);
        }
    }

    pub fn fts_remove_at(&mut self, collection: &str, row_idx: usize) {
        if let Some(fts) = self.fts.get_mut(collection) {
            fts.remove_row_idx(row_idx);
        }
    }

    /// Rebuild one collection's FTS using existing field list (after delete / compact).
    pub fn rebuild_fts_inplace(&mut self, collection: &str) {
        let Some(fields) = self.fts.get(collection).map(|f| f.fields.clone()) else {
            return;
        };
        let rows = self.collection(collection);
        self.fts.insert(
            collection.to_string(),
            crate::fts::FtsIndex::build(rows, &fields),
        );
    }

    pub fn rebuild_indexes_collection(&mut self, collection: &str) {
        let labels: Vec<String> = self
            .indexes
            .iter()
            .filter(|(_, idx)| idx.def.collection == collection)
            .map(|(k, _)| k.clone())
            .collect();
        for label in labels {
            let Some(snap) = self.extra_indexes.get(&label).cloned() else {
                continue;
            };
            let def = crate::catalog::IndexDef {
                collection: snap.collection.clone(),
                unique: snap.unique,
                fields: snap.fields.clone(),
            };
            let mut live = LiveIndex::new(def);
            for (i, row) in self.collection(collection).iter().enumerate() {
                let _ = live.insert_at(i, row);
            }
            self.indexes.insert(label, live);
        }
    }

    pub fn index_seek(
        &self,
        collection: &str,
        uses: &[crate::index::IndexUse],
        now: i64,
    ) -> Option<Vec<usize>> {
        if uses.is_empty() {
            return None;
        }
        if uses.len() == 1 {
            let use_ = &uses[0];
            let idx = self.indexes.get(&use_.def.label())?;
            if idx.def.collection != collection {
                return None;
            }
            return Some(idx.seek_idxs(use_, now));
        }
        let mut out = Vec::new();
        let mut seen = FxHashSet::default();
        for use_ in uses {
            let idx = self.indexes.get(&use_.def.label())?;
            if idx.def.collection != collection {
                return None;
            }
            for i in idx.seek_idxs(use_, now) {
                if seen.insert(i) {
                    out.push(i);
                }
            }
        }
        Some(out)
    }

    pub fn index_seek_count(
        &self,
        collection: &str,
        uses: &[crate::index::IndexUse],
        now: i64,
    ) -> Option<usize> {
        if uses.is_empty() {
            return None;
        }
        if uses.len() == 1 {
            let use_ = &uses[0];
            let idx = self.indexes.get(&use_.def.label())?;
            if idx.def.collection != collection {
                return None;
            }
            return Some(idx.seek_count(use_, now));
        }
        // OR branches may overlap — dedupe via seek.
        Some(self.index_seek(collection, uses, now)?.len())
    }

    /// Count `docs.title ~ needle` (no group) using columnar titles + memchr.
    pub fn docs_title_contains_count(&self, needle: &str) -> i64 {
        let finder = memchr::memmem::Finder::new(needle.as_bytes());
        let mut n = 0i64;
        for title in &self.docs_title {
            if finder.find(title.as_bytes()).is_some() {
                n += 1;
            }
        }
        n
    }

    /// Count `docs.title ~ needle` grouped by layer using columnar titles (no row maps).
    pub fn docs_title_contains_count_by_layer(&self, needle: &str) -> BTreeMap<Arc<str>, i64> {
        let finder = memchr::memmem::Finder::new(needle.as_bytes());
        let mut map: BTreeMap<Arc<str>, i64> = BTreeMap::new();
        let n = self.docs_title.len().min(self.docs_layer.len());
        let titles = &self.docs_title[..n];
        let layers = &self.docs_layer[..n];
        for i in 0..n {
            if finder.find(titles[i].as_bytes()).is_none() {
                continue;
            }
            let layer = &layers[i];
            // Borrow as str so hits reuse the existing key without Arc clone.
            if let Some(c) = map.get_mut(layer.as_ref()) {
                *c += 1;
            } else {
                map.insert(Arc::clone(layer), 1);
            }
        }
        map
    }

    /// Project hot docs fields from parallel columns — Arc clone only, no full row clone.
    /// True when orders SoA matches `collections["orders"]` length.
    pub fn orders_soa_ready(&self) -> bool {
        let n = self.collection("orders").len();
        self.orders_id.len() == n && self.orders_user_id.len() == n && self.orders_total.len() == n
    }

    /// True when users SoA matches `collections["users"]` length.
    pub fn users_soa_ready(&self) -> bool {
        let n = self.collection("users").len();
        self.users_id.len() == n && self.users_email.len() == n
    }

    pub fn orders_id(&self) -> &[Arc<str>] {
        &self.orders_id
    }
    pub fn orders_user_id(&self) -> &[Arc<str>] {
        &self.orders_user_id
    }
    pub fn orders_total(&self) -> &[f64] {
        &self.orders_total
    }
    pub fn users_id(&self) -> &[Arc<str>] {
        &self.users_id
    }
    pub fn users_email(&self) -> &[Arc<str>] {
        &self.users_email
    }

    /// Returns None if any requested field is outside the hot set.
    pub fn project_docs_hot(&self, idxs: &[usize], fields: &[String]) -> Option<Vec<Row>> {
        if fields.is_empty() || !fields.iter().all(|f| docs_hot_field(f)) {
            return None;
        }
        // Hot path: { id, title } — compact 2-entry maps.
        if fields.len() == 2 && fields[0] == "id" && fields[1] == "title" {
            let mut out = Vec::with_capacity(idxs.len());
            for &i in idxs {
                if i >= self.docs_id.len() {
                    continue;
                }
                out.push(row_id_title(&self.docs_id[i], &self.docs_title[i]));
            }
            return Some(out);
        }
        let mut out = Vec::with_capacity(idxs.len());
        for &i in idxs {
            if i >= self.docs_id.len() {
                continue;
            }
            let mut row = BTreeMap::new();
            for f in fields {
                let cell = match f.as_str() {
                    "id" => Cell::Text(Arc::clone(&self.docs_id[i])),
                    "title" => Cell::Text(Arc::clone(&self.docs_title[i])),
                    "layer" => Cell::Text(Arc::clone(&self.docs_layer[i])),
                    "wing" => Cell::Text(Arc::clone(&self.docs_wing[i])),
                    _ => Cell::Null,
                };
                row.insert(f.clone(), cell);
            }
            out.push(row);
        }
        Some(out)
    }

    /// Full docs scan projecting only hot columns when filter is title-contains or always-true path.
    pub fn scan_docs_hot_project(
        &self,
        fields: &[String],
        title_contains: Option<&str>,
    ) -> Option<Vec<Row>> {
        if fields.is_empty() || !fields.iter().all(|f| docs_hot_field(f)) {
            return None;
        }
        let n = self.docs_id.len();
        let mut out = Vec::new();
        let finder = title_contains.map(|n| memchr::memmem::Finder::new(n.as_bytes()));
        let id_title = fields.len() == 2 && fields[0] == "id" && fields[1] == "title";
        for i in 0..n {
            if let Some(f) = &finder
                && f.find(self.docs_title[i].as_bytes()).is_none()
            {
                continue;
            }
            if id_title {
                out.push(row_id_title(&self.docs_id[i], &self.docs_title[i]));
                continue;
            }
            let mut row = BTreeMap::new();
            for f in fields {
                let cell = match f.as_str() {
                    "id" => Cell::Text(Arc::clone(&self.docs_id[i])),
                    "title" => Cell::Text(Arc::clone(&self.docs_title[i])),
                    "layer" => Cell::Text(Arc::clone(&self.docs_layer[i])),
                    "wing" => Cell::Text(Arc::clone(&self.docs_wing[i])),
                    _ => Cell::Null,
                };
                row.insert(f.clone(), cell);
            }
            out.push(row);
        }
        Some(out)
    }
}

fn docs_hot_field(f: &str) -> bool {
    matches!(f, "id" | "title" | "layer" | "wing")
}

#[inline]
fn row_id_title(id: &Arc<str>, title: &Arc<str>) -> Row {
    BTreeMap::from([
        (String::from("id"), Cell::Text(Arc::clone(id))),
        (String::from("title"), Cell::Text(Arc::clone(title))),
    ])
}

fn spo_key(row: &Row) -> Option<(Arc<str>, Arc<str>, Arc<str>)> {
    Some((
        row.get("s").and_then(Cell::text_shared)?,
        row.get("p").and_then(Cell::text_shared)?,
        row.get("o").and_then(Cell::text_shared)?,
    ))
}

pub fn project_fields(row: &Row, fields: &[String]) -> Row {
    let mut out = BTreeMap::new();
    for f in fields {
        out.insert(f.clone(), row.get(f).cloned().unwrap_or(Cell::Null));
    }
    out
}

#[derive(Clone)]
pub struct MemBackup {
    r#gen: u64,
    embed_id: String,
    next_id: u64,
    collections: BTreeMap<String, Vec<Row>>,
    edges: Vec<Edge>,
    extra_collections: BTreeMap<String, ColSnap>,
    extra_rels: BTreeMap<String, RelSnap>,
    extra_indexes: BTreeMap<String, IndexSnap>,
}

fn same_row_key(collection: &str, a: &Row, b: &Row) -> bool {
    if collection == "facts" {
        return row_text(a, "s") == row_text(b, "s")
            && row_text(a, "p") == row_text(b, "p")
            && row_text(a, "o") == row_text(b, "o");
    }
    match (row_text(a, "id"), row_text(b, "id")) {
        (Some(x), Some(y)) => x == y,
        _ => row_text(a, "uri")
            .zip(row_text(b, "uri"))
            .is_some_and(|(x, y)| x == y),
    }
}

pub fn catalog_hash(cat: &Catalog) -> String {
    let mut buf = String::new();
    buf.push_str(&cat.embed_id);
    for (n, c) in &cat.collections {
        buf.push_str(n);
        buf.push(if c.append_only { 'A' } else { 'M' });
        for (f, info) in &c.fields {
            buf.push_str(f);
            buf.push_str(info.ty.name());
        }
    }
    for (n, r) in &cat.rels {
        buf.push_str(n);
        if let Some(of) = &r.reverse_of {
            buf.push_str(of);
        }
    }
    for (n, idx) in &cat.indexes {
        buf.push_str(n);
        if idx.unique {
            buf.push('U');
        }
        for f in &idx.fields {
            buf.push_str(f);
        }
    }
    format!("h:{:016x}", fnv1a64(buf.as_bytes()))
}

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[allow(dead_code)]
pub fn content_hash(body: &str) -> String {
    content_hash_arc(body).as_ref().to_owned()
}

pub fn content_hash_arc(body: &str) -> Arc<str> {
    let h = fnv1a64(body.as_bytes());
    let mut buf = [0u8; 18];
    buf[0] = b'h';
    buf[1] = b':';
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for i in 0..16 {
        let shift = (15 - i) * 4;
        buf[2 + i] = HEX[((h >> shift) & 0xf) as usize];
    }
    // SAFETY: buf is always ASCII hex.
    Arc::from(std::str::from_utf8(&buf).unwrap())
}

pub fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut h = 0xcbf29ce484222325u64;
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

struct DocSeed<'a> {
    id: &'a str,
    uri: &'a str,
    title: &'a str,
    wing: &'a str,
    room: &'a str,
    layer: &'a str,
    body: &'a str,
    snippet: &'a str,
    ts: i64,
}

fn doc_row(d: DocSeed<'_>) -> Row {
    let mut r = BTreeMap::new();
    r.insert("id".into(), Cell::text_arc(d.id));
    r.insert("uri".into(), Cell::text_arc(d.uri));
    r.insert("title".into(), Cell::text_arc(d.title));
    r.insert("wing".into(), Cell::text_arc(d.wing));
    r.insert("room".into(), Cell::text_arc(d.room));
    r.insert("layer".into(), Cell::text_arc(d.layer));
    r.insert("body".into(), Cell::text_arc(d.body));
    r.insert("hash".into(), Cell::Text(content_hash_arc(d.body)));
    r.insert("snippet".into(), Cell::text_arc(d.snippet));
    r.insert("ts".into(), Cell::Time(d.ts));
    r
}

fn user_row(id: &str, email: &str) -> Row {
    let mut r = BTreeMap::new();
    r.insert("id".into(), Cell::text_arc(id));
    r.insert("email".into(), Cell::text_arc(email));
    r
}

fn order_row(id: &str, user_id: &str, total: f64, ts: i64) -> Row {
    let mut r = BTreeMap::new();
    r.insert("id".into(), Cell::text_arc(id));
    r.insert("user_id".into(), Cell::text_arc(user_id));
    r.insert("total".into(), Cell::Float(total));
    r.insert("ts".into(), Cell::Time(ts));
    r
}

fn fmt_iso_millis(ms: i64) -> String {
    let z = ms.div_euclid(1000);
    let millis = ms.rem_euclid(1000) as u32;
    let days = z.div_euclid(86_400);
    let tod = z.rem_euclid(86_400) as u32;
    let (y, m, d) = civil_from_days(days);
    let hh = tod / 3600;
    let mm = (tod % 3600) / 60;
    let ss = tod % 60;
    format!("{y:04}-{m:02}-{d:02}T{hh:02}:{mm:02}:{ss:02}.{millis:03}Z")
}

fn civil_from_days(days: i64) -> (i32, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y as i32, m, d)
}
