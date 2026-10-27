use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Arc, OnceLock};
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
    SnapshotRef,
};

thread_local! {
    static TLS_ROW_POOL: std::cell::RefCell<RowPool> = std::cell::RefCell::new(RowPool::new());
}

/// Global row pool for batch operations - reused across queries
#[allow(dead_code)]
static ROW_POOL_INIT: std::sync::OnceLock<std::sync::Mutex<Option<RowPool>>> =
    std::sync::OnceLock::new();

/// Get or create the global row pool (thread-safe)
#[allow(dead_code)]
pub fn get_row_pool() -> &'static std::sync::Mutex<Option<RowPool>> {
    ROW_POOL_INIT.get_or_init(|| std::sync::Mutex::new(None))
}

/// Initialize the row pool with a pre-sized capacity
#[allow(dead_code)]
pub fn init_row_pool(capacity: usize) {
    let pool = RowPool {
        cache: Vec::with_capacity(capacity),
    };
    if let Some(pool_mutex) = ROW_POOL_INIT.get() {
        *pool_mutex.lock().unwrap() = Some(pool);
    } else {
        // Fallback: direct assignment on first call
        let _ = ROW_POOL_INIT.set(std::sync::Mutex::new(Some(pool)));
    }
}

/// Object pool for frequently allocated BTreeMap rows — reduces allocations in hot paths
pub struct RowPool {
    /// Pre-allocated BTreeMap instances ready for reuse
    cache: Vec<BTreeMap<String, Cell>>,
}

impl RowPool {
    pub fn new() -> Self {
        Self {
            cache: Vec::with_capacity(64),
        }
    }

    #[inline]
    pub fn acquire(&mut self) -> BTreeMap<String, Cell> {
        self.cache.pop().unwrap_or_else(BTreeMap::new)
    }

    #[inline]
    pub fn release(&mut self, mut row: BTreeMap<String, Cell>) {
        // Clear Arc references but keep capacity (Arc is zero-copy drop)
        row.clear();
        if self.cache.len() < 256 {
            self.cache.push(row);
        }
    }

    /// Drain all pooled rows back (e.g., on reset/clear)
    #[allow(dead_code)]
    pub fn drain(&mut self) {
        self.cache.clear();
    }

    /// Check how many rows are currently pooled
    #[allow(dead_code)]
    pub fn pool_size(&self) -> usize {
        self.cache.len()
    }
}

/// Acquire a pooled BTreeMap row lock-free from the thread-local pool
#[inline]
pub fn acquire_row() -> Row {
    TLS_ROW_POOL.with(|p| p.borrow_mut().acquire()).into()
}

/// Release a BTreeMap row back to the thread-local pool for reuse
#[inline]
pub fn release_row(row: Row) {
    TLS_ROW_POOL.with(|p| p.borrow_mut().release(Arc::unwrap_or_clone(row.0)));
}

impl Default for RowPool {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct StoreOpenPhases {
    pub setup_ms: f64,
    pub snapshot_ms: f64,
    pub wal_ms: f64,
    pub metadata_ms: f64,
    pub indexes_ms: f64,
    pub row_maps_ms: f64,
    pub fts_ms: f64,
}

/// Compact inline string storage for strings up to 22 bytes.
///
/// Stores strings inline without heap allocation or atomic refcounts.
/// Combined with `Cell`'s 1-byte enum tag, keeps `size_of::<Cell>()` at exactly 24 bytes.
#[derive(Copy, Clone, Eq)]
pub struct InlineStr {
    len: u8,
    buf: [u8; 22],
}

impl InlineStr {
    pub const MAX_LEN: usize = 22;

    #[inline]
    pub fn try_new(s: &str) -> Option<Self> {
        let b = s.as_bytes();
        if b.len() > Self::MAX_LEN {
            return None;
        }
        let mut buf = [0u8; 22];
        buf[..b.len()].copy_from_slice(b);
        Some(Self {
            len: b.len() as u8,
            buf,
        })
    }

    #[inline]
    pub fn new_truncate(s: &str) -> Self {
        let b = s.as_bytes();
        let len = b.len().min(Self::MAX_LEN);
        let mut buf = [0u8; 22];
        buf[..len].copy_from_slice(&b[..len]);
        Self {
            len: len as u8,
            buf,
        }
    }

    #[inline]
    pub fn as_str(&self) -> &str {
        let slice = &self.buf[..self.len as usize];
        // SAFETY: Only constructed from valid UTF-8 strings
        unsafe { std::str::from_utf8_unchecked(slice) }
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.len as usize
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

impl std::ops::Deref for InlineStr {
    type Target = str;
    #[inline]
    fn deref(&self) -> &str {
        self.as_str()
    }
}

impl std::fmt::Debug for InlineStr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(self.as_str(), f)
    }
}

impl std::fmt::Display for InlineStr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(self.as_str(), f)
    }
}

impl PartialEq for InlineStr {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.as_str() == other.as_str()
    }
}

impl PartialEq<str> for InlineStr {
    #[inline]
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}

impl PartialEq<&str> for InlineStr {
    #[inline]
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

impl Ord for InlineStr {
    #[inline]
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.as_str().cmp(other.as_str())
    }
}

impl PartialOrd for InlineStr {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl std::hash::Hash for InlineStr {
    #[inline]
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.as_str().hash(state);
    }
}

impl Serialize for InlineStr {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for InlineStr {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = InlineStr;
            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a string up to 22 bytes")
            }
            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                InlineStr::try_new(v).ok_or_else(|| E::custom("string too long for InlineStr"))
            }
        }
        deserializer.deserialize_str(Visitor)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "t", content = "v")]
pub enum Cell {
    Null,
    /// Arc so projecting / cloning rows does not deep-copy large strings.
    Text(Arc<str>),
    /// Small String Optimization: inline storage for strings <= 22 bytes without heap allocation.
    SmallText(InlineStr),
    Int(i64),
    Float(f64),
    Bool(bool),
    Time(i64),
    /// Dense embedding (L2-normalized by the active embedder).
    Vec(Arc<[f32]>),
}

impl PartialEq for Cell {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Cell::Null, Cell::Null) => true,
            (Cell::Text(a), Cell::Text(b)) => a == b,
            (Cell::SmallText(a), Cell::SmallText(b)) => a == b,
            (Cell::Text(a), Cell::SmallText(b)) => a.as_ref() == b.as_str(),
            (Cell::SmallText(a), Cell::Text(b)) => a.as_str() == b.as_ref(),
            (Cell::Int(a), Cell::Int(b)) => a == b,
            (Cell::Float(a), Cell::Float(b)) => a.to_bits() == b.to_bits(),
            (Cell::Bool(a), Cell::Bool(b)) => a == b,
            (Cell::Time(a), Cell::Time(b)) => a == b,
            (Cell::Vec(a), Cell::Vec(b)) => a == b,
            _ => false,
        }
    }
}

impl Eq for Cell {}

impl std::hash::Hash for Cell {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        match self {
            Cell::Null => 0u8.hash(state),
            Cell::Text(s) => {
                1u8.hash(state);
                s.as_ref().hash(state);
            }
            Cell::SmallText(s) => {
                1u8.hash(state);
                s.as_str().hash(state);
            }
            Cell::Int(n) => {
                2u8.hash(state);
                n.hash(state);
            }
            Cell::Float(f) => {
                3u8.hash(state);
                f.to_bits().hash(state);
            }
            Cell::Bool(b) => {
                4u8.hash(state);
                b.hash(state);
            }
            Cell::Time(t) => {
                5u8.hash(state);
                t.hash(state);
            }
            Cell::Vec(v) => {
                6u8.hash(state);
                for x in v.as_ref() {
                    x.to_bits().hash(state);
                }
            }
        }
    }
}

impl Cell {
    pub fn text_arc(s: impl Into<Arc<str>>) -> Self {
        Cell::Text(s.into())
    }

    /// Construct a text cell with Small String Optimization (SSO).
    ///
    /// Strings up to 22 bytes are stored inline without heap allocations or atomic refcounts.
    /// Longer strings are stored in `Arc<str>`.
    pub fn from_text(s: impl AsRef<str>) -> Self {
        let s = s.as_ref();
        if let Some(inline) = InlineStr::try_new(s) {
            Cell::SmallText(inline)
        } else {
            Cell::Text(Arc::from(s))
        }
    }

    /// Construct an inline text cell if `s` fits within 22 bytes.
    #[inline]
    pub fn inline(s: &str) -> Option<Self> {
        InlineStr::try_new(s).map(Cell::SmallText)
    }

    pub fn vec_arc(v: impl Into<Arc<[f32]>>) -> Self {
        Cell::Vec(v.into())
    }

    pub fn text(&self) -> Option<&str> {
        match self {
            Cell::Text(s) => Some(s.as_ref()),
            Cell::SmallText(s) => Some(s.as_str()),
            _ => None,
        }
    }

    pub fn text_shared(&self) -> Option<Arc<str>> {
        match self {
            Cell::Text(s) => Some(Arc::clone(s)),
            Cell::SmallText(s) => Some(Arc::from(s.as_str())),
            _ => None,
        }
    }

    /// Zero-copy clone for borrow scenarios: just clone the Arc reference, don't copy data.
    /// For `SmallText`, this copies the 23-byte inline struct without any atomic refcounts.
    #[inline]
    pub fn clone_for_borrow(&self) -> Self {
        match self {
            Cell::Text(s) => Cell::Text(Arc::clone(s)),
            Cell::SmallText(s) => Cell::SmallText(*s),
            Cell::Vec(v) => Cell::Vec(Arc::clone(v)),
            // Other types are Copy, use standard clone
            Cell::Int(n) => Cell::Int(*n),
            Cell::Float(f) => Cell::Float(*f),
            Cell::Bool(b) => Cell::Bool(*b),
            Cell::Time(t) => Cell::Time(*t),
            Cell::Null => Cell::Null,
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

    #[inline]
    pub fn as_i64(&self) -> Option<i64> {
        self.as_int()
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

    pub fn compact_into(&self, out: &mut String) {
        use std::fmt::Write;
        match self {
            Cell::Null => out.push_str("null"),
            Cell::Text(s) => {
                let _ = write!(out, "{:?}", s.as_ref());
            }
            Cell::SmallText(s) => {
                let _ = write!(out, "{:?}", s.as_str());
            }
            Cell::Int(n) => {
                let _ = write!(out, "{n}");
            }
            Cell::Float(n) => {
                let _ = write!(out, "{n}");
            }
            Cell::Bool(b) => {
                out.push_str(if *b { "true" } else { "false" });
            }
            Cell::Time(ms) => {
                out.push_str(&fmt_iso_millis(*ms));
            }
            Cell::Vec(v) => {
                let _ = write!(out, "vec[{}]", v.len());
            }
        }
    }

    pub fn compact(&self) -> String {
        let mut out = String::with_capacity(16);
        self.compact_into(&mut out);
        out
    }
}

impl From<&str> for Cell {
    #[inline]
    fn from(s: &str) -> Self {
        Cell::from_text(s)
    }
}

impl From<String> for Cell {
    #[inline]
    fn from(s: String) -> Self {
        Cell::from_text(s)
    }
}

impl From<InlineStr> for Cell {
    #[inline]
    fn from(s: InlineStr) -> Self {
        Cell::SmallText(s)
    }
}

impl From<Arc<str>> for Cell {
    #[inline]
    fn from(s: Arc<str>) -> Self {
        Cell::Text(s)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Row(Arc<BTreeMap<String, Cell>>);
impl Row {
    pub fn new() -> Self { Self::default() }
}
impl std::ops::Deref for Row {
    type Target = BTreeMap<String, Cell>;
    fn deref(&self) -> &Self::Target { &self.0 }
}
impl std::ops::DerefMut for Row {
    fn deref_mut(&mut self) -> &mut Self::Target { Arc::make_mut(&mut self.0) }
}
impl FromIterator<(String, Cell)> for Row {
    fn from_iter<T: IntoIterator<Item=(String, Cell)>>(iter: T) -> Self {
        Self(Arc::new(iter.into_iter().collect()))
    }
}
impl<const N: usize> From<[(String, Cell); N]> for Row {
    fn from(value: [(String, Cell); N]) -> Self { value.into_iter().collect() }
}
impl From<BTreeMap<String, Cell>> for Row {
    fn from(value: BTreeMap<String, Cell>) -> Self { Self(Arc::new(value)) }
}
impl Extend<(String, Cell)> for Row {
    fn extend<T: IntoIterator<Item=(String, Cell)>>(&mut self, iter: T) { Arc::make_mut(&mut self.0).extend(iter); }
}
impl IntoIterator for Row {
    type Item=(String, Cell);
    type IntoIter=std::collections::btree_map::IntoIter<String, Cell>;
    fn into_iter(self) -> Self::IntoIter { Arc::unwrap_or_clone(self.0).into_iter() }
}
impl<'a> IntoIterator for &'a Row {
    type Item=(&'a String, &'a Cell);
    type IntoIter=std::collections::btree_map::Iter<'a,String,Cell>;
    fn into_iter(self) -> Self::IntoIter { self.0.iter() }
}
impl<'a> IntoIterator for &'a mut Row {
    type Item=(&'a String, &'a mut Cell);
    type IntoIter=std::collections::btree_map::IterMut<'a,String,Cell>;
    fn into_iter(self) -> Self::IntoIter { Arc::make_mut(&mut self.0).iter_mut() }
}


/// Estimated in-memory byte size of a row (keys + values + container overhead).
#[inline]
pub fn row_byte_size(row: &Row) -> usize {
    let mut total = std::mem::size_of::<Row>() + row.len() * 32;
    for (k, v) in row {
        total += k.len();
        total += match v {
            Cell::Null | Cell::Int(_) | Cell::Float(_) | Cell::Bool(_) | Cell::Time(_) => {
                std::mem::size_of::<Cell>()
            }
            Cell::SmallText(_) => std::mem::size_of::<Cell>(),
            Cell::Text(s) => std::mem::size_of::<Cell>() + s.len(),
            Cell::Vec(vec) => std::mem::size_of::<Cell>() + vec.len() * 4,
        };
    }
    total
}

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
    let mut out = String::with_capacity(64);
    out.push('{');
    let mut first = true;
    for (k, v) in row {
        if !matches!(v, Cell::Null) {
            if !first {
                out.push_str(", ");
            }
            first = false;
            out.push_str(k);
            out.push_str(": ");
            v.compact_into(&mut out);
        }
    }
    out.push('}');
    out
}

pub fn row_text<'a>(row: &'a Row, key: &str) -> Option<&'a str> {
    row.get(key).and_then(Cell::text)
}

/// Hot document row type for zero-copy projection (id, title, layer, wing columns)
#[derive(Clone)]
#[allow(dead_code)]
pub struct HotDocRow {
    pub id: Arc<str>,
    pub title: Arc<str>,
    pub layer: Arc<str>,
    pub wing: Arc<str>,
}

#[allow(dead_code)]
impl HotDocRow {
    #[inline]
    pub fn to_btree(self) -> BTreeMap<String, Cell> {
        BTreeMap::from([
            ("id".into(), Cell::Text(Arc::clone(&self.id))),
            ("title".into(), Cell::Text(Arc::clone(&self.title))),
            ("layer".into(), Cell::Text(Arc::clone(&self.layer))),
            ("wing".into(), Cell::Text(Arc::clone(&self.wing))),
        ])
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Edge {
    pub rel: String,
    pub from: String,
    pub to: String,
}

type FactKey = (Arc<str>, Arc<str>, Arc<str>);
type StoreRowSnap = (
    Option<Arc<str>>,
    Option<Arc<str>>,
    Arc<str>,
    Arc<str>,
    Arc<str>,
    Arc<str>,
    Option<(Arc<str>, Arc<str>, Arc<str>)>,
    Arc<str>,
    f64,
);

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
    pub extra_filters: BTreeMap<String, String>,
    pub extra_owned: BTreeMap<String, Vec<(String, String)>>,
    pub indexes: BTreeMap<String, LiveIndex>,
    /// collection → FTS postings (fields with catalog `fts`).
    pub(crate) fts: BTreeMap<String, crate::fts::FtsIndex>,
    /// collection → HNSW vector index
    pub(crate) hnsw: BTreeMap<String, crate::embed::HnswIndex>,
    /// collection → id → row index (O(1) Get / IndexSeek fetch).
    pub(crate) by_id: BTreeMap<String, FxHashMap<Arc<str>, usize>>,
    /// docs uri → row index.
    docs_by_uri: FxHashMap<Arc<str>, usize>,
    /// facts (s,p,o) → row index — O(1) append idempotency.
    pub(crate) facts_by_spo: FxHashMap<FactKey, usize>,
    /// edge (rel,from,to) set — O(1) append/delete edge.
    pub(crate) edge_keys: FxHashSet<(Arc<str>, Arc<str>, Arc<str>)>,
    /// (rel, from) → target node keys (outgoing adjacency index).
    pub(crate) edges_out: FxHashMap<(Arc<str>, Arc<str>), Vec<Arc<str>>>,
    /// (rel, to) → source node keys (incoming adjacency index).
    pub(crate) edges_in: FxHashMap<(Arc<str>, Arc<str>), Vec<Arc<str>>>,

    // Hot collection metadata — always accessed during scans
    /// Count of rows per collection for quick bounds checking
    pub(crate) col_counts: FxHashMap<String, usize>,

    /// Parallel Arc columns for docs — contains scans + projected materialize
    /// without cloning full `BTreeMap` rows. Organized SoA (struct-of-arrays) for cache locality.
    docs_id: Vec<Arc<str>>,
    docs_title: Vec<Arc<str>>,
    docs_title_packed: OnceLock<(Vec<u8>, Vec<usize>)>,
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
    facts_len: usize,
}

impl Store {
    pub fn empty(embed_id: impl Into<String>) -> Self {
        // ✅ Preallocate collections with expected count (4 typical tables)
        let mut collections = BTreeMap::new();
        for name in ["docs", "users", "orders", "facts"] {
            collections.insert(name.into(), Vec::new());
        }

        // Initialize global row pool for batch operations
        init_row_pool(32);

        // Cache-friendly: track row counts per collection for fast bounds checking
        let mut col_counts = FxHashMap::default();
        col_counts.insert("docs".to_string(), 0);
        col_counts.insert("users".to_string(), 0);
        col_counts.insert("orders".to_string(), 0);
        col_counts.insert("facts".to_string(), 0);

        Self {
            r#gen: 0,
            embed_id: embed_id.into(),
            next_id: 1,
            collections,
            edges: Vec::new(),
            extra_collections: BTreeMap::new(),
            extra_rels: BTreeMap::new(),
            extra_indexes: BTreeMap::new(),
            extra_filters: BTreeMap::new(),
            extra_owned: BTreeMap::new(),
            indexes: BTreeMap::new(),
            fts: BTreeMap::new(),
            hnsw: BTreeMap::new(),
            by_id: BTreeMap::new(),
            docs_by_uri: FxHashMap::default(),
            facts_by_spo: FxHashMap::default(),
            edge_keys: FxHashSet::default(),
            edges_out: FxHashMap::default(),
            edges_in: FxHashMap::default(),
            col_counts,
            docs_id: Vec::new(),
            docs_title: Vec::new(),
            docs_title_packed: OnceLock::new(),
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
        s.hnsw = self.hnsw.clone();
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
        let start = snap.as_ref().map(|s| s.log_offset).unwrap_or(0);
        let snap_catalog_hash = snap.as_ref().map(|s| s.catalog_hash.clone());
        let snap_has_extras = snap
            .as_ref()
            .map(|s| {
                !s.extra_collections.is_empty()
                    || !s.extra_rels.is_empty()
                    || !s.extra_indexes.is_empty()
                    || !s.extra_filters.is_empty()
                    || !s.extra_owned.is_empty()
            })
            .unwrap_or(false);

        let mut store = match snap {
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

        let mut fts_cat = cat.clone();
        store.merge_extras_into(&mut fts_cat);
        let fts_t0 = Instant::now();
        store.try_load_fts(dir, &fts_cat);
        let mut fts_ms = fts_t0.elapsed().as_secs_f64() * 1000.0;

        let phase = Instant::now();
        let mut log = persist::open_log(dir)?;
        let min_gen = store.r#gen;
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
        if let Some(snap_hash) = snap_catalog_hash
            && snap_hash != live_hash
            && !snap_has_extras
            && snap_hash != fixture_hash
        {
            return Err(Error::runtime(format!(
                "catalog_hash mismatch: snapshot={} catalog={}",
                snap_hash, live_hash
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

        let fts_t1 = Instant::now();
        store.ensure_fts(&live);
        fts_ms += fts_t1.elapsed().as_secs_f64() * 1000.0;
        phases.fts_ms = fts_ms;

        let persist = Persist {
            dir: dir.to_path_buf(),
            log,
            lock,
            catalog_hash: live_hash,
            writes_since_snapshot: 0,
            encode_buf: Vec::with_capacity(64 * 1024),
            encoder: None,
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
        persist::validate_data_dir(dir)?;
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
        let start = snap.as_ref().map(|s| s.log_offset).unwrap_or(0);
        let mut store = match snap {
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
        let mut fts_cat = cat.clone();
        store.merge_extras_into(&mut fts_cat);
        let fts_t0 = Instant::now();
        store.try_load_fts(dir, &fts_cat);
        let mut fts_ms = fts_t0.elapsed().as_secs_f64() * 1000.0;
        let phase = Instant::now();
        let mut log = persist::open_log_read(dir)?;
        let min_gen = store.r#gen;
        if let Some(log) = log.as_mut() {
            let _ = persist::replay_log(log, min_gen, start, |rec| {
                store.apply_record(&rec);
                Ok(())
            })?;
        }
        phases.wal_ms = phase.elapsed().as_secs_f64() * 1000.0;
        let mut live = cat.clone();
        store.merge_extras_into(&mut live);
        let fts_t1 = Instant::now();
        store.ensure_fts(&live);
        fts_ms += fts_t1.elapsed().as_secs_f64() * 1000.0;
        phases.fts_ms = fts_ms;
        Ok((store, lock, phases))
    }

    pub(crate) fn from_snapshot(dir: &Path, s: Snapshot) -> Result<Self, Error> {
        let mut store = Self::from_snapshot_unindexed(dir, s)?;
        store.rebuild_indexes();
        store.rebuild_row_maps();
        Ok(store)
    }

    #[allow(dead_code)]
    pub(crate) fn from_snapshot_ref(dir: &Path, s: &Snapshot) -> Result<Self, Error> {
        Self::from_snapshot(dir, s.clone())
    }

    fn from_snapshot_unindexed(dir: &Path, mut s: Snapshot) -> Result<Self, Error> {
        let col_counts = {
            let mut counts = FxHashMap::default();
            for (name, rows) in &s.collections {
                counts.insert(name.clone(), rows.len());
            }
            counts
        };
        let mut store = Self {
            r#gen: s.r#gen,
            embed_id: std::mem::take(&mut s.embed_id),
            next_id: s.next_id,
            collections: std::mem::take(&mut s.collections),
            edges: std::mem::take(&mut s.edges),
            extra_collections: std::mem::take(&mut s.extra_collections),
            extra_rels: std::mem::take(&mut s.extra_rels),
            extra_indexes: std::mem::take(&mut s.extra_indexes),
            extra_filters: std::mem::take(&mut s.extra_filters),
            extra_owned: std::mem::take(&mut s.extra_owned),
            indexes: BTreeMap::new(),
            fts: BTreeMap::new(),
            hnsw: BTreeMap::new(),
            by_id: BTreeMap::new(),
            docs_by_uri: FxHashMap::default(),
            facts_by_spo: FxHashMap::default(),
            edge_keys: FxHashSet::default(),
            edges_out: FxHashMap::default(),
            edges_in: FxHashMap::default(),
            docs_id: Vec::new(),
            docs_title: Vec::new(),
            docs_title_packed: OnceLock::new(),
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
            col_counts,
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
            extra_filters: self.extra_filters.clone(),
            extra_owned: self.extra_owned.clone(),
            cold_collections: Vec::new(),
        }
    }

    pub fn merge_extras_into(&self, cat: &mut Catalog) {
        for (name, fields) in &self.extra_owned {
            let fields = fields
                .iter()
                .map(|(n, t)| (n.clone(), TypeExpr::Named(t.clone())))
                .collect();
            let _ = cat.apply_decl(&Decl::Owned {
                name: name.clone(),
                fields,
            });
        }
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
            let pred = idx
                .pred_src
                .as_deref()
                .and_then(|src| crate::parse::parse_pred_src(src).ok());
            let _ = cat.apply_decl(&Decl::Index {
                collection: idx.collection.clone(),
                unique: idx.unique,
                fields: idx.fields.clone(),
                pred,
            });
        }
        for (collection, src) in &self.extra_filters {
            if let Ok(pred) = crate::parse::parse_pred_src(src) {
                let _ = cat.apply_decl(&Decl::Filter {
                    collection: collection.clone(),
                    pred: Some(pred),
                    src: src.clone(),
                });
            }
        }
    }

    /// Flush one committed pack to the log (data sync). Head is updated durably
    /// on checkpoint/close only (soft head skipped on the hot path).
    pub fn durable_commit(&self, persist: &mut Persist, pack: Pack) -> Result<(), Error> {
        self.durable_commit_prepared(persist, pack, None)
    }

    pub(crate) fn durable_commit_prepared(
        &self,
        persist: &mut Persist,
        pack: Pack,
        prepared: Option<crate::persist::PreparedEncoding>,
    ) -> Result<(), Error> {
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
            match prepared {
                Some(persist::PreparedEncoding::Ready(encoded)) => {
                    persist::append_prepared_record(
                        log, &rec, encoded, encode_buf, sync, log_bytes,
                    )?;
                }
                Some(persist::PreparedEncoding::Failed(error)) => return Err(error),
                Some(persist::PreparedEncoding::Panicked(payload)) => {
                    std::panic::resume_unwind(payload)
                }
                None => persist::append_record(log, &rec, encode_buf, sync, log_bytes)?,
            }
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
        self.ensure_facts_rows();
        let cold_names_to_promote: Vec<String> = self.cold.keys().cloned().collect();
        for name in cold_names_to_promote {
            self.promote_cold(&name);
        }
        let mut cold_names = Vec::new();
        if persist.cold {
            // Cold bins are a decode cache; snapshot stays self-contained (rows kept).
            for (name, rows) in &self.collections {
                if rows.len() >= cold::COLD_MIN_ROWS {
                    cold::write_cold(&persist.dir, name, rows)?;
                    cold_names.push(name.clone());
                }
            }
            cold::prune_cold(&persist.dir, &cold_names)?;
        }
        let head = Head {
            r#gen: self.r#gen,
            catalog_hash: persist.catalog_hash.clone(),
            embed_id: self.embed_id.clone(),
        };
        let snap_ref = SnapshotRef {
            r#gen: self.r#gen,
            embed_id: &self.embed_id,
            next_id: self.next_id,
            catalog_hash: &persist.catalog_hash,
            log_offset: 0,
            collections: &self.collections,
            edges: &self.edges,
            extra_collections: &self.extra_collections,
            extra_rels: &self.extra_rels,
            extra_indexes: &self.extra_indexes,
            extra_filters: &self.extra_filters,
            extra_owned: &self.extra_owned,
            cold_collections: &cold_names,
        };
        persist::write_snapshot_ref(&persist.dir, &snap_ref)?;
        persist::write_head(&persist.dir, &head)?;
        self.write_fts_files(&persist.dir)?;
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
                // Update cache-friendly count tracking
                self.update_col_count(collection, 1);

                // Update precomputed aggregates if any exist for this collection
                crate::exec::update_aggregate_for_insert(collection, row);

                // Update window function states if any exist for this collection
                crate::window::update_window_states(collection, idx, row);

                // Invalidate aggregate views on mutation (generation-based)
                crate::exec::invalidate_aggregates(self.r#gen);

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
                let n = rows.len();
                self.collection_mut(collection).reserve(rows.len());
                self.row_maps_reserve(collection, rows.len());
                let _ = self.index_insert_slab(collection, start, rows);
                self.fts_insert_slab(collection, start, rows);
                self.hnsw_insert_slab(collection, start, rows);
                self.row_maps_register_slab(collection, start, rows);
                self.collection_mut(collection).extend(rows.iter().cloned());
                // Update cache-friendly count tracking
                self.update_col_count(collection, n as isize);
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
                self.hnsw_insert_slab(collection, start, &rows);
                self.row_maps_register_slab(collection, start, &rows);
                self.collection_mut(collection).extend(rows.iter().cloned());
                // Update cache-friendly count tracking
                self.update_col_count(collection, n as isize);
                for e in edges {
                    let _ = self.append_edge_parts(&e.rel, &e.from, &e.to);
                }
            }
            Pack::AppendFact { row } => {
                // Zero-copy borrow: avoid unnecessary clone from owned Row
                let (result, _) = self.append_fact_row(&row);
                drop(result); // Drop the returned row since we already updated collection
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
                let mut new_rows = 0;
                for new in rows {
                    let id = row_text(new, "id").map(str::to_string);
                    let idx = id.as_ref().and_then(|id| self.row_index(collection, id));
                    if let Some(i) = idx {
                        let before = self.collection(collection)[i].clone();
                        self.index_remove_at(collection, i);
                        self.collection_mut(collection)[i] = new.clone();
                        self.row_maps_sync_updated(collection, i, &before, new);
                        let _ = self.index_insert_at(collection, i);
                        self.fts_sync_at(collection, i, &before);
                        continue;
                    }
                    self.collection_mut(collection).push(new.clone());
                    let i = self.collection(collection).len() - 1;
                    new_rows += 1;
                    self.row_maps_register(collection, i);
                    let _ = self.index_insert_at(collection, i);
                    self.fts_insert_at(collection, i);
                }
                // Update cache-friendly count tracking
                self.update_col_count(collection, new_rows as isize);
            }
            Pack::Reembed => {}
            Pack::Delete { collection, rows } => {
                let dead = self.dead_positions(collection, rows);
                self.delete_rows_at(collection, &dead);
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
                self.col_counts.entry(name.clone()).or_insert(0);
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
                pred_src,
            } => {
                let snap = IndexSnap {
                    collection: collection.clone(),
                    unique: *unique,
                    fields: fields.clone(),
                    pred_src: pred_src.clone(),
                };
                let label = format!("{}[{}]", collection, fields.join(","));
                self.extra_indexes.insert(label.clone(), snap);
                let pred = pred_src
                    .as_deref()
                    .and_then(|src| crate::parse::parse_pred_src(src).ok());
                let def = crate::catalog::IndexDef {
                    collection: collection.clone(),
                    unique: *unique,
                    fields: fields.clone(),
                    pred,
                    bitmap_card_field: None,
                };
                let mut live = LiveIndex::new(def);
                for (i, row) in self.collection(collection).iter().enumerate() {
                    let _ = live.insert_at(i, row);
                }
                self.indexes.insert(label, live);
                if fields.iter().any(|f| f == "embedding") {
                    self.build_hnsw(collection, 16, 64);
                }
            }
            Pack::SchemaFilter {
                collection,
                pred_src,
            } => match pred_src {
                Some(src) => {
                    self.extra_filters.insert(collection.clone(), src.clone());
                }
                None => {
                    self.extra_filters.remove(collection);
                }
            },
            Pack::SchemaOwned { name, fields } => {
                self.extra_owned.insert(name.clone(), fields.clone());
            }
            Pack::Batch { packs } => {
                self.apply_pack_batch(packs, 0);
            }
        }
    }

    fn apply_pack_batch(&mut self, packs: &[Pack], depth: usize) {
        if depth > 64 {
            return;
        }
        for p in packs {
            if let Pack::Batch { packs: sub } = p {
                self.apply_pack_batch(sub, depth + 1);
            } else {
                self.apply_pack(p);
            }
        }
    }

    pub fn mem_backup(&mut self) -> MemBackup {
        self.ensure_facts_rows();
        let cold_names: Vec<String> = self.cold.keys().cloned().collect();
        for name in cold_names {
            self.promote_cold(&name);
        }
        // Deletions bitmap is not persisted (snapshots are always compacted)
        MemBackup {
            r#gen: self.r#gen,
            embed_id: self.embed_id.clone(),
            next_id: self.next_id,
            collections: self.collections.clone(),
            edges: self.edges.clone(),
            extra_collections: self.extra_collections.clone(),
            extra_rels: self.extra_rels.clone(),
            extra_indexes: self.extra_indexes.clone(),
            extra_filters: self.extra_filters.clone(),
            extra_owned: self.extra_owned.clone(),
            hnsw: self.hnsw.clone(),
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
        self.extra_filters = b.extra_filters;
        self.extra_owned = b.extra_owned;
        self.hnsw = b.hnsw;
        self.facts_rows_stale = false;
        self.rebuild_indexes();
        self.rebuild_row_maps();
        self.rebuild_all_fts_inplace();
        self.refresh_col_counts();
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
            facts_len: self.facts_s.len(),
        }
    }

    pub fn append_rollback(&mut self, mark: AppendMark) {
        self.next_id = mark.next_id;
        self.edges.truncate(mark.edge_len);
        self.facts_s.truncate(mark.facts_len);
        self.facts_p.truncate(mark.facts_len);
        self.facts_o.truncate(mark.facts_len);
        self.facts_rows_stale = true;
        self.ensure_facts_rows();
        for (name, rows) in self.collections.iter_mut() {
            if name == "facts" {
                continue;
            }
            let len = mark.col_lens.get(name).copied().unwrap_or(0);
            rows.truncate(len);
        }
        // Drop collections created after the mark (schema col during append-only is rare;
        // full backup covers schema). Keep extras as-is for append-only path.
        self.rebuild_indexes();
        self.rebuild_row_maps();
        self.rebuild_all_fts_inplace();
        self.refresh_col_counts();
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
        s.refresh_col_counts();
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
            let mut row = Row::new();
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

    /// True when rows live in `collections` (not cold-only), so position-keyed
    /// journals can be replayed against it.
    pub fn has_collection(&self, name: &str) -> bool {
        self.collections.contains_key(name)
    }

    pub fn collection(&self, name: &str) -> &[Row] {
        if let Some(c) = self.cold.get(name)
            && let Ok(rows) = c.rows()
        {
            return rows;
        }
        self.collections
            .get(name)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// Cache-friendly row count lookup (O(1) via FxHashMap, avoids Vec::len iteration)
    #[inline]
    pub fn get_col_count(&self, collection: &str) -> usize {
        if collection == "facts" && self.facts_rows_stale {
            return self.facts_s.len();
        }
        if let Some(&c) = self.col_counts.get(collection)
            && c > 0
        {
            return c;
        }
        self.collection(collection).len()
    }

    /// Update cached count after single-row insert/delete
    #[inline]
    fn update_col_count(&mut self, collection: &str, delta: isize) {
        let current = *self
            .col_counts
            .get(collection)
            .unwrap_or(&self.collections.get(collection).map_or(0, Vec::len));
        if let Some(new_count) = current.checked_add_signed(delta) {
            self.col_counts.insert(collection.to_string(), new_count);
        }
    }

    /// Rebuild counts from scratch (used after bulk operations or snapshot restore)
    pub fn refresh_col_counts(&mut self) {
        self.col_counts.clear();
        for name in self.collections.keys() {
            let len = if name == "facts" && self.facts_rows_stale {
                self.facts_s.len()
            } else {
                self.collection(name).len()
            };
            self.col_counts.insert(name.clone(), len);
        }
    }

    /// Acquire a pooled BTreeMap for temporary batch operations
    /// Reuses previously released allocations to reduce heap pressure lock-free
    #[inline]
    pub fn acquire_pooled_row(&self) -> Row {
        acquire_row()
    }

    /// Return a pooled BTreeMap back to the thread-local pool
    #[inline]
    pub fn release_pooled_row(row: Row) {
        release_row(row);
    }

    pub fn collection_mut(&mut self, name: &str) -> &mut Vec<Row> {
        if name == "facts" {
            self.ensure_facts_rows();
        }
        self.promote_cold(name);
        // Keep the existing-key path allocation-free without an early returned
        // mutable borrow that older borrow checkers extend into the entry path.
        if self.collections.contains_key(name) {
            self.collections.get_mut(name).expect("existing collection")
        } else {
            self.collections.entry(name.to_string()).or_default()
        }
    }

    pub fn find_doc_key(&self, key: &str) -> Option<&Row> {
        self.get_by_id("docs", key).or_else(|| self.get_by_uri(key))
    }

    pub fn get_by_id(&self, collection: &str, id: &str) -> Option<&Row> {
        let idx = self.row_index(collection, id)?;
        self.get_by_idx(collection, idx)
    }

    pub fn get_by_uri(&self, uri: &str) -> Option<&Row> {
        let idx = *self.docs_by_uri.get(uri)?;
        self.get_by_idx("docs", idx)
    }

    pub fn row_index(&self, collection: &str, id: &str) -> Option<usize> {
        self.by_id.get(collection)?.get(id).copied()
    }

    /// Position of a row addressed by an `id` / `uri` point key, when the row
    /// maps cover that collection and field.
    pub fn point_index(&self, collection: &str, field: &str, key: &str) -> Option<usize> {
        match field {
            "id" => self.row_index(collection, key),
            "uri" if collection == "docs" => {
                let idx = *self.docs_by_uri.get(key)?;
                self.get_by_idx("docs", idx)?;
                Some(idx)
            }
            _ => None,
        }
    }

    pub fn get_by_idx(&self, collection: &str, idx: usize) -> Option<&Row> {
        self.collection(collection).get(idx)
    }

    pub fn rows_by_idxs(&self, collection: &str, idxs: &[usize]) -> Vec<Row> {
        let rows = self.collection(collection);
        let mut out = Vec::with_capacity(idxs.len());
        for &i in idxs {
            if let Some(row) = rows.get(i) {
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

    #[inline]
    pub fn point_get_docs_id_title(&self, id: &str) -> Option<Row> {
        let idx = self.row_index("docs", id)?;
        if idx < self.docs_id.len() {
            Some(row_id_title(&self.docs_id[idx], &self.docs_title[idx]))
        } else {
            None
        }
    }

    /// Single-row hot projection from parallel docs columns.
    pub fn project_docs_hot_idx(&self, i: usize, fields: &[String]) -> Option<Row> {
        if i >= self.docs_id.len() {
            return None;
        }
        if fields.len() == 2 && fields[0] == "id" && fields[1] == "title" {
            return Some(row_id_title(&self.docs_id[i], &self.docs_title[i]));
        }
        let mut row = Row::new();
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
        let spo_triples: Vec<(Arc<str>, Arc<str>, Arc<str>)> = self
            .collection("facts")
            .iter()
            .map(|row| {
                spo_key(row).unwrap_or_else(|| (Arc::from(""), Arc::from(""), Arc::from("")))
            })
            .collect();
        self.facts_s.reserve(spo_triples.len());
        self.facts_p.reserve(spo_triples.len());
        self.facts_o.reserve(spo_triples.len());
        for (s, p, o) in spo_triples {
            self.facts_s.push(s);
            self.facts_p.push(p);
            self.facts_o.push(o);
        }
        self.facts_rows_stale = false;
        self.refresh_col_counts();
    }

    fn rebuild_edge_keys(&mut self) {
        self.edge_keys.clear();
        self.edges_out.clear();
        self.edges_in.clear();
        self.edge_keys.reserve(self.edges.len());
        for e in &self.edges {
            let rel_arc = Arc::<str>::from(e.rel.as_str());
            let from_arc = Arc::<str>::from(e.from.as_str());
            let to_arc = Arc::<str>::from(e.to.as_str());
            self.edge_keys.insert((
                Arc::clone(&rel_arc),
                Arc::clone(&from_arc),
                Arc::clone(&to_arc),
            ));
            self.edges_out
                .entry((Arc::clone(&rel_arc), Arc::clone(&from_arc)))
                .or_default()
                .push(Arc::clone(&to_arc));
            self.edges_in
                .entry((rel_arc, to_arc))
                .or_default()
                .push(from_arc);
        }
    }

    /// Invoke closure for each outgoing or incoming neighbor key without allocating vectors.
    #[inline]
    pub fn for_each_neighbor(&self, rel: &str, node: &str, reverse: bool, mut f: impl FnMut(&str)) {
        let synced = self.edge_keys.len() == self.edges.len();
        if synced {
            let key = (Arc::<str>::from(rel), Arc::<str>::from(node));
            let map = if reverse {
                &self.edges_in
            } else {
                &self.edges_out
            };
            if let Some(list) = map.get(&key) {
                for dst in list {
                    f(dst.as_ref());
                }
                return;
            }
        }
        for e in &self.edges {
            if e.rel == rel {
                if reverse {
                    if e.to == node {
                        f(e.from.as_str());
                    }
                } else if e.from == node {
                    f(e.to.as_str());
                }
            }
        }
    }

    /// Outgoing or incoming neighbor keys for `(rel, node)` in $O(1)$ lookup.
    #[inline]
    pub fn edge_neighbors(&self, rel: &str, node: &str, reverse: bool) -> Vec<Arc<str>> {
        let synced = self.edge_keys.len() == self.edges.len();
        if !synced {
            return self
                .edges
                .iter()
                .filter_map(|e| {
                    if e.rel != rel {
                        return None;
                    }
                    if reverse {
                        (e.to == node).then(|| Arc::<str>::from(e.from.as_str()))
                    } else {
                        (e.from == node).then(|| Arc::<str>::from(e.to.as_str()))
                    }
                })
                .collect();
        }
        let key = (Arc::<str>::from(rel), Arc::<str>::from(node));
        let map = if reverse {
            &self.edges_in
        } else {
            &self.edges_out
        };
        map.get(&key).cloned().unwrap_or_default()
    }

    /// Idempotent fact insert. Returns `(row, changed)`.
    pub fn append_fact_row(&mut self, row: &Row) -> (Row, bool) {
        let Some(key) = spo_key(row) else {
            self.ensure_facts_rows();
            self.collection_mut("facts").push(row.clone());
            self.update_col_count("facts", 1);
            // Zero-copy: just clone when necessary (only for caller return)
            let row = self.collection("facts").last().unwrap().clone();
            return (row, true);
        };
        if let Some(&idx) = self.facts_by_spo.get(&key) {
            self.ensure_facts_rows();
            // Zero-copy borrow when fact already exists
            return (self.collection("facts")[idx].clone(), false);
        }
        let sa = Arc::clone(&key.0);
        let pa = Arc::clone(&key.1);
        let oa = Arc::clone(&key.2);
        let idx = self.facts_s.len();
        self.facts_by_spo.insert(key, idx);
        self.facts_s.push(Arc::clone(&sa));
        self.facts_p.push(Arc::clone(&pa));
        self.facts_o.push(Arc::clone(&oa));
        let mut built = Row::new();
        built.insert("s".into(), Cell::Text(sa));
        built.insert("p".into(), Cell::Text(pa));
        built.insert("o".into(), Cell::Text(oa));
        if self.facts_rows_stale {
            self.ensure_facts_rows();
        } else {
            let facts = self.collections.entry("facts".into()).or_default();
            if facts.len() == idx {
                facts.push(built.clone());
            } else {
                self.facts_rows_stale = true;
                self.ensure_facts_rows();
            }
        }
        self.update_col_count("facts", 1);
        (built, true)
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
                let mut row = Row::new();
                row.insert("s".into(), Cell::Text(Arc::clone(&self.facts_s[idx])));
                row.insert("p".into(), Cell::Text(Arc::clone(&self.facts_p[idx])));
                row.insert("o".into(), Cell::Text(Arc::clone(&self.facts_o[idx])));
                rows.push(row);
            }
        }
        if changed > 0 {
            self.col_counts.insert("facts".into(), self.facts_s.len());
        }
        if want_rows && changed > 0 {
            self.ensure_facts_rows();
        }
        (rows, changed)
    }

    pub fn append_edge_parts(&mut self, rel: &str, from: &str, to: &str) -> bool {
        let rel_arc = Arc::<str>::from(rel);
        let from_arc = Arc::<str>::from(from);
        let to_arc = Arc::<str>::from(to);
        let key = (
            Arc::clone(&rel_arc),
            Arc::clone(&from_arc),
            Arc::clone(&to_arc),
        );
        if !self.edge_keys.insert(key) {
            return false;
        }
        self.edges_out
            .entry((Arc::clone(&rel_arc), Arc::clone(&from_arc)))
            .or_default()
            .push(Arc::clone(&to_arc));
        self.edges_in
            .entry((rel_arc, to_arc))
            .or_default()
            .push(from_arc);
        self.edges.push(Edge {
            rel: rel.to_owned(),
            from: from.to_owned(),
            to: to.to_owned(),
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
        let rel_arc = Arc::<str>::from(rel);
        let from_arc = Arc::<str>::from(from);
        let to_arc = Arc::<str>::from(to);
        let key = (
            Arc::clone(&rel_arc),
            Arc::clone(&from_arc),
            Arc::clone(&to_arc),
        );
        if !self.edge_keys.remove(&key) {
            return false;
        }
        if let Some(list) = self
            .edges_out
            .get_mut(&(Arc::clone(&rel_arc), Arc::clone(&from_arc)))
        {
            list.retain(|x| x.as_ref() != to);
        }
        if let Some(list) = self.edges_in.get_mut(&(rel_arc, to_arc)) {
            list.retain(|x| x.as_ref() != from);
        }
        self.edges
            .retain(|e| !(e.rel == rel && e.from == from && e.to == to));
        true
    }

    /// Physical positions in `collection` whose row key matches any of `rows`.
    /// Resolves each target through the id/uri maps for an O(D) path, mirroring
    /// the Update write; falls back to a full scan only when a row's key can't be
    /// resolved through the maps (e.g. `facts` triples, or id-less rows).
    fn dead_positions(&self, collection: &str, rows: &[Row]) -> Vec<usize> {
        let n = self.collection(collection).len();
        let mut out: Vec<usize> = Vec::with_capacity(rows.len());
        let mut seen = rustc_hash::FxHashSet::default();
        for d in rows {
            let pos = row_text(d, "id")
                .and_then(|id| self.point_index(collection, "id", id))
                .or_else(|| {
                    row_text(d, "uri").and_then(|u| self.point_index(collection, "uri", u))
                });
            match pos {
                Some(p) if p < n && seen.insert(p) => out.push(p),
                Some(_) => {}
                None => {
                    for (i, r) in self.collection(collection).iter().enumerate() {
                        if same_row_key(collection, r, d) && seen.insert(i) {
                            out.push(i);
                        }
                    }
                }
            }
        }
        out
    }

    /// Swap-remove rows and update only the removed/moved index, FTS and map slots.
    pub fn delete_rows_at(&mut self, collection: &str, dead: &[usize]) {
 let _phase_probe=crate::phase_probe::Span::new(7);

        let old_n = self.collection_mut(collection).len();
        if old_n == 0 || dead.is_empty() {
            return;
        }

        let mut dead: Vec<usize> = dead.iter().copied().filter(|&d| d < old_n).collect();
        dead.sort_unstable();
        dead.dedup();
        if dead.is_empty() {
            return;
        }

        // Cache-friendly count update (decrement by number of deleted rows)
        self.update_col_count(collection, -(dead.len() as isize));

        // Swap-remove, highest slot first: the last live row moves into each vacated
        // slot, so the array stays dense and only the doomed row's and the moved
        // row's index / FTS / map entries change. Positions are derived (maps,
        // postings and columnar arrays are rebuilt from the rows on restore), so row
        // order is not a contract — `sort` is the explicit remedy when one is needed.

        for &p in dead.iter().rev() {
            let last = match self.collections.get(collection) {
                Some(rows) if p < rows.len() => rows.len() - 1,
                _ => break,
            };

            // Zero-copy: steal row with swap_remove instead of cloning
            let doomed = self.collections.get_mut(collection).unwrap().swap_remove(p);

            self.index_remove_row(collection, p, &doomed);
            self.fts_remove_row(collection, p, &doomed);
            if let Some(hnsw) = self.hnsw.get_mut(collection) {
                hnsw.remove(p);
            }
            self.row_maps_drop_keys(collection, &doomed);
            if p != last {
                self.move_deleted_tail(collection, last, p);
                continue;
            }
            self.swap_pop_columns(collection, p, last + 1);
        }

        let new_n = old_n - dead.len();
        let soa_len = match collection {
            "docs" => self.docs_id.len(),
            "orders" => self.orders_id.len(),
            "users" => self.users_id.len(),
            _ => new_n,
        };
        if soa_len != new_n {
            self.rebuild_row_maps_collection(collection);
        }
    }

    /// The collection already contains the moved row at `to`. Read it in place
    /// while editing disjoint derived indexes; retain only identity keys for
    /// the column/map step, avoiding a complete Row clone.
    fn move_deleted_tail(&mut self, collection: &str, from: usize, to: usize) {
 let _phase_probe=crate::phase_probe::Span::new(8);

        let row = &self.collections[collection][to];
        for index in self.indexes.values_mut() {
            if index.def.collection == collection {
                index.move_at(from, to);
            }
        }
        if let Some(fts) = self.fts.get_mut(collection) {
            fts.move_row(from, to, row);
        }
        if let Some(hnsw) = self.hnsw.get_mut(collection) {
            hnsw.move_id(from, to);
        }
        let id = row.get("id").and_then(Cell::text_shared);
        let uri = (collection == "docs")
            .then(|| row.get("uri").and_then(Cell::text_shared))
            .flatten();
        let spo = (collection == "facts").then(|| spo_key(row)).flatten();
        // Keep the original remove/register semantics for duplicate identities.
        if let Some(id) = id {
            self.by_id
                .entry(collection.to_string())
                .or_default()
                .insert(id, to);
        }
        if let Some(uri) = uri {
            self.docs_by_uri.insert(uri, to);
        }
        if let Some(spo) = spo {
            self.facts_by_spo.insert(spo, to);
        }
        // These arrays are known parallel; swap-pop already transfers every
        // mirrored value, so registering the row again would duplicate the work.
        self.swap_pop_columns(collection, to, from + 1);
    }

    /// Mirror [`swap_pop_at`] across the collection's columnar arrays, skipping
    /// columns that aren't parallel to the rows (the caller's length guard rebuilds).
    fn swap_pop_columns(&mut self, collection: &str, p: usize, len: usize) {
 let _phase_probe=crate::phase_probe::Span::new(10);

        if collection == "docs" {
            self.docs_title_packed.take();
        }
        match collection {
            "docs" => {
                swap_pop_at(&mut self.docs_id, p, len);
                swap_pop_at(&mut self.docs_title, p, len);
                swap_pop_at(&mut self.docs_layer, p, len);
                swap_pop_at(&mut self.docs_wing, p, len);
            }
            "orders" => {
                swap_pop_at(&mut self.orders_id, p, len);
                swap_pop_at(&mut self.orders_user_id, p, len);
                swap_pop_at(&mut self.orders_total, p, len);
            }
            "users" => {
                swap_pop_at(&mut self.users_id, p, len);
                swap_pop_at(&mut self.users_email, p, len);
            }
            "facts" => {
                swap_pop_at(&mut self.facts_s, p, len);
                swap_pop_at(&mut self.facts_p, p, len);
                swap_pop_at(&mut self.facts_o, p, len);
            }
            _ => {}
        }
    }

    /// Drop a row's `key → position` entries after its slot is vacated.
    fn row_maps_drop_keys(&mut self, collection: &str, row: &Row) {
 let _phase_probe=crate::phase_probe::Span::new(11);

        if let Some(id) = row_text(row, "id")
            && let Some(map) = self.by_id.get_mut(collection)
        {
            map.remove(id);
        }
        if collection == "docs"
            && let Some(uri) = row_text(row, "uri")
        {
            self.docs_by_uri.remove(uri);
        }
        if collection == "facts"
            && let Some(spo) = spo_key(row)
        {
            self.facts_by_spo.remove(&spo);
        }
    }

    /// [`Store::row_maps_register_row`] plus the `facts` key that helper ignores.
    fn swap_maps_register(&mut self, collection: &str, idx: usize, row: &Row) {
        // Inline: facts By_SPO lookup is only needed for triples
        if collection == "facts"
            && let Some(spo) = spo_key(row)
        {
            self.facts_by_spo.insert(spo, idx);
        } else {
            self.row_maps_register_row(collection, idx, row);
        }
    }

    /// Reverse in-place row mutations from a `Rows` undo journal. Positions never
    /// shift during an update, so each entry restores one row plus its own index /
    /// FTS / map slots — no whole-store clone, no per-collection rebuild.
    pub fn restore_rows(&mut self, journal: &[(String, usize, Row)]) {
        for (collection, idx, row) in journal.iter().rev() {
            let live = self
                .collections
                .get(collection)
                .is_some_and(|rows| *idx < rows.len());
            if !live {
                continue;
            }
            let current = self.collection(collection)[*idx].clone();
            self.index_remove_at(collection, *idx);
            self.fts_remove_at(collection, *idx);
            self.collection_mut(collection)[*idx] = row.clone();
            let _ = self.index_insert_at(collection, *idx);
            self.fts_insert_at(collection, *idx);
            self.row_maps_sync_updated(collection, *idx, &current, row);
        }
    }

    /// Inverse of [`Self::delete_rows_at`]: drop each removed row back into its old
    /// slot and push whatever the swap parked there onto the tail. Walking `positions`
    /// ascending reproduces the forward pass's intermediate lengths exactly, so
    /// `p == rows.len()` marks the slots that were the tail and need no push.
    pub fn restore_rows_at(&mut self, collection: &str, positions: &[usize], rows: &[Row]) {
        for (&p, doomed) in positions.iter().zip(rows) {
            let Some(current) = self.collections.get(collection) else {
                return;
            };
            let len = current.len();
            if p > len {
                continue;
            }
            if p < len {
                let moved = self.collections[collection][p].clone();
                self.index_remove_row(collection, p, &moved);
                self.fts_move_row(collection, p, len, &moved);
                self.row_maps_drop_keys(collection, &moved);
                if let Some(rows) = self.collections.get_mut(collection) {
                    rows.push(moved.clone());
                }
                let _ = self.index_insert_row(collection, len, &moved);
                self.swap_maps_register(collection, len, &moved);
            }
            if let Some(rows) = self.collections.get_mut(collection) {
                if p == rows.len() {
                    rows.push(doomed.clone());
                } else {
                    rows[p] = doomed.clone();
                }
            }
            let _ = self.index_insert_row(collection, p, doomed);
            self.fts_insert_row(collection, p, doomed);
            self.swap_maps_register(collection, p, doomed);
            // The slab's columns follow the rows array: the value at `p` moves to the
            // tail and the restored row takes `p`, which one swap-push expresses.
            if collection == "facts" {
                let get = |f: &str| {
                    doomed
                        .get(f)
                        .and_then(Cell::text_shared)
                        .unwrap_or_default()
                };
                swap_push_at(&mut self.facts_s, p, len, get("s"));
                swap_push_at(&mut self.facts_p, p, len, get("p"));
                swap_push_at(&mut self.facts_o, p, len, get("o"));
            }
        }
        let n = self
            .collections
            .get(collection)
            .map_or(0, std::vec::Vec::len);
        self.col_counts.insert(collection.to_string(), n);
        let soa_len = match collection {
            "docs" => self.docs_id.len(),
            "orders" => self.orders_id.len(),
            "users" => self.users_id.len(),
            "facts" => self.facts_s.len(),
            _ => n,
        };
        if soa_len != n {
            self.rebuild_row_maps_collection(collection);
        }
    }

    pub fn rebuild_row_maps_collection(&mut self, collection: &str) {
        if collection == "docs" {
            self.docs_title_packed.take();
        }
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
        let snaps: Vec<StoreRowSnap> = rows
            .iter()
            .map(|row| {
                (
                    row.get("id").and_then(Cell::text_shared),
                    row.get("uri").and_then(Cell::text_shared),
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
        if collection == "docs" {
            self.docs_title_packed.take();
        }
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
                .insert(Arc::clone(id), idx);
        }
        if collection == "docs" {
            let did = id.unwrap_or_default();
            let title = title.unwrap_or_default();
            let layer = layer.unwrap_or_default();
            let wing = wing.unwrap_or_default();
            if let Some(uri) = uri {
                self.docs_by_uri.insert(uri, idx);
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
        if collection == "docs" {
            self.docs_title_packed.take();
        }
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
        if collection == "docs" {
            self.docs_title_packed.take();
        }
        let id = row.get("id").and_then(Cell::text_shared);
        let uri = row.get("uri").and_then(Cell::text_shared);
        if let Some(ref id) = id {
            self.by_id
                .entry(collection.to_string())
                .or_default()
                .insert(Arc::clone(id), idx);
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
                self.docs_by_uri.insert(uri, idx);
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
        if collection == "docs" {
            self.docs_title_packed.take();
        }
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
            let by_id = self.by_id.entry(collection.to_owned()).or_default();
            for (i, row) in rows.iter().enumerate() {
                let idx = start + i;
                let mut texts: [Option<Arc<str>>; 5] = std::array::from_fn(|_| None);
                for (field, value) in row {
                    let slot = match field.as_str() {
                        "id" => 0,
                        "uri" => 1,
                        "title" => 2,
                        "layer" => 3,
                        "wing" => 4,
                        _ => continue,
                    };
                    texts[slot] = value.text_shared();
                }
                let [id, uri, title, layer, wing] = texts;
                if let Some(ref id) = id {
                    by_id.insert(Arc::clone(id), idx);
                }
                if let Some(uri) = uri {
                    self.docs_by_uri.insert(uri, idx);
                }
                self.docs_id.push(id.unwrap_or_default());
                self.docs_title.push(title.unwrap_or_default());
                self.docs_layer.push(layer.unwrap_or_default());
                self.docs_wing.push(wing.unwrap_or_default());
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

    /// Sync maps / SoA after an **in-place** row update. Identity fields are immutable,
    /// so the normal path only rewrites the slots and keys of this one row; a moved
    /// `id`/`uri` (owned collections aside) falls back to rebuilding the collection.
    pub fn row_maps_sync_updated(
        &mut self,
        collection: &str,
        idx: usize,
        before: &Row,
        after: &Row,
    ) {
 let _phase_probe=crate::phase_probe::Span::new(6);

        if collection == "docs" {
            self.docs_title_packed.take();
        }
        let id = row_text(after, "id");
        let identity_moved =
            row_text(before, "id") != id || row_text(before, "uri") != row_text(after, "uri");
        let stale_slot = id.is_some_and(|key| self.row_index(collection, key) != Some(idx));
        if identity_moved || stale_slot {
            self.rebuild_row_maps_collection(collection);
            return;
        }
        // Identity keys already address this slot. Re-inserting them would
        // allocate fresh id/uri strings for every non-identity field update.
        let text = |field: &str| {
            after
                .get(field)
                .and_then(Cell::text_shared)
                .unwrap_or_default()
        };
        match collection {
            "docs"
                if idx < self.docs_title.len()
                    && idx < self.docs_layer.len()
                    && idx < self.docs_wing.len() =>
            {
                self.docs_title[idx] = text("title");
                self.docs_layer[idx] = text("layer");
                self.docs_wing[idx] = text("wing");
            }
            "orders" if idx < self.orders_user_id.len() && idx < self.orders_total.len() => {
                self.orders_user_id[idx] = text("user_id");
                self.orders_total[idx] = after.get("total").and_then(Cell::as_f64).unwrap_or(0.0);
            }
            "users" if idx < self.users_email.len() => self.users_email[idx] = text("email"),
            "docs" | "orders" | "users" => self.rebuild_row_maps_collection(collection),
            _ => {}
        }
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
 let _phase_probe=crate::phase_probe::Span::new(4);

        for idx in self.indexes.values_mut() {
            if idx.def.collection == collection
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
        if rows.is_empty() {
            return Ok(());
        }
        let labels: Vec<String> = self
            .indexes
            .iter()
            .filter(|(_, idx)| idx.def.collection == collection)
            .map(|(k, _)| k.clone())
            .collect();
        if labels.is_empty() {
            return Ok(());
        }

        // Batch-only path for slab insertions to reduce per-row overhead
        for label in &labels {
            let Some(idx) = self.indexes.get_mut(label) else {
                continue;
            };

            // Use batch insert when multiple rows
            if rows.len() > 1 {
                if let Err(e) = idx.insert_slab_batch(start, rows) {
                    return Err(Error::runtime(e));
                }
            } else {
                // Fall back to single insert for edge case
                if let Err(e) = idx.insert_at_new(start, &rows[0]) {
                    return Err(Error::runtime(e));
                }
            }
        }
        Ok(())
    }

    /// Drop a position from every index of `collection`. The row at that position
    /// must still be the indexed version — keys are recomputed from it.
    pub fn index_remove_at(&mut self, collection: &str, row_idx: usize) {
        let Some(row) = self
            .collections
            .get(collection)
            .and_then(|c| c.get(row_idx))
            .cloned()
        else {
            return;
        };
        self.index_remove_row(collection, row_idx, &row);
    }

    /// Drop one row's index entries, given the row itself.
    pub fn index_remove_row(&mut self, collection: &str, row_idx: usize, _row: &Row) {
 let _phase_probe=crate::phase_probe::Span::new(9);

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
            let pred = snap
                .pred_src
                .as_deref()
                .and_then(|src| crate::parse::parse_pred_src(src).ok());
            let def = crate::catalog::IndexDef {
                collection: snap.collection.clone(),
                unique: snap.unique,
                fields: snap.fields.clone(),
                pred,
                bitmap_card_field: None,
            };
            let mut live = LiveIndex::new(def);
            for (i, row) in self.collection(&snap.collection).iter().enumerate() {
                let _ = live.insert_at(i, row);
            }

            // Build bloom filter for fast negative lookups
            #[cfg(feature = "parallel")]
            live.build_bloom_filter();

            self.indexes.insert(label, live);
        }
    }

    /// Load checkpointed posting lists when gen/nrows/fields match; ignore stale files.
    fn try_load_fts(&mut self, dir: &Path, catalog: &crate::catalog::Catalog) {
        for cname in catalog.collections.keys() {
            let fields = crate::fts::fts_fields(catalog, cname);
            if fields.is_empty() {
                continue;
            }
            let nrows = self.collection(cname).len() as u64;
            match crate::fts::read_fts(dir, cname) {
                Ok((generation, file_rows, idx))
                    if generation == self.r#gen && file_rows == nrows && idx.fields == fields =>
                {
                    self.fts.insert(cname.clone(), idx);
                }
                _ => {}
            }
        }
    }

    fn write_fts_files(&mut self, dir: &Path) -> Result<(), Error> {
        let mut keep = Vec::new();
        let rows_per_collection: Vec<(String, u64)> = self
            .fts
            .keys()
            .map(|name| (name.clone(), self.collection(name).len() as u64))
            .collect();
        let generation = self.r#gen;
        for (name, nrows) in rows_per_collection {
            if let Some(idx) = self.fts.get_mut(&name) {
                crate::fts::write_fts(dir, &name, idx, generation, nrows)?;
            }
            keep.push(name);
        }
        crate::fts::prune_fts(dir, &keep)
    }

    /// Rebuild FTS postings from catalog `fts` fields (memory / missing durable blob).
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

    /// Rebuild only collections that have no live posting list.
    pub fn ensure_fts(&mut self, catalog: &crate::catalog::Catalog) {
        for (cname, cdef) in &catalog.collections {
            let fields: Vec<String> = cdef
                .fields
                .iter()
                .filter(|(_, f)| f.fts)
                .map(|(n, _)| n.clone())
                .collect();
            if fields.is_empty() {
                self.fts.remove(cname);
                continue;
            }
            if self.fts.contains_key(cname) {
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

    /// Register one row's postings at `row_idx`, given the row itself.
    pub fn fts_insert_row(&mut self, collection: &str, row_idx: usize, row: &Row) {
        if let Some(fts) = self.fts.get_mut(collection) {
            fts.insert_row(row_idx, row);
        }
    }

    pub fn fts_insert_slab(&mut self, collection: &str, start: usize, rows: &[Row]) {
        let Some(fts) = self.fts.get_mut(collection) else {
            return;
        };
        fts.append_slab(start, rows);
    }

    pub fn hnsw_insert_slab(&mut self, collection: &str, start: usize, rows: &[Row]) {
        let Some(hnsw) = self.hnsw.get_mut(collection) else {
            return;
        };
        for (i, row) in rows.iter().enumerate() {
            if let Some(Cell::Vec(v)) = row.get("embedding") {
                hnsw.insert(start + i, v.clone());
            }
        }
    }

    pub fn build_hnsw(&mut self, collection: &str, m: usize, ef: usize) {
        let mut index = crate::embed::HnswIndex::new(m, ef);
        if let Some(rows) = self.collections.get(collection) {
            for (i, row) in rows.iter().enumerate() {
                if let Some(Cell::Vec(v)) = row.get("embedding") {
                    index.insert(i, v.clone());
                }
            }
        }
        self.hnsw.insert(collection.to_string(), index);
    }

    /// Drop the postings of the row currently at `row_idx`.
    pub fn fts_remove_at(&mut self, collection: &str, row_idx: usize) {
        let Some(row) = self
            .collections
            .get(collection)
            .and_then(|c| c.get(row_idx))
            .cloned()
        else {
            return;
        };
        self.fts_remove_row(collection, row_idx, &row);
    }

    /// Drop one row's postings, given the row itself: only that row's tokens are
    /// visited, so the cost is per-row rather than per-vocabulary.
    pub fn fts_remove_row(&mut self, collection: &str, row_idx: usize, row: &Row) {
        if let Some(fts) = self.fts.get_mut(collection) {
            fts.remove_row(row_idx, row);
        }
    }

    /// Re-point a row's postings between slots after a swap-move, tokenizing it
    /// once rather than once per edit.
    pub fn fts_move_row(&mut self, collection: &str, from: usize, to: usize, row: &Row) {
        if let Some(fts) = self.fts.get_mut(collection) {
            fts.move_row(from, to, row);
        }
        if let Some(hnsw) = self.hnsw.get_mut(collection) {
            hnsw.move_id(from, to);
        }
    }

    /// Re-point a single row's postings after an in-place field update, given
    /// the row as it was before the write.
    pub fn fts_sync_at(&mut self, collection: &str, row_idx: usize, before: &Row) {
 let _phase_probe=crate::phase_probe::Span::new(5);

        let Some(row) = self
            .collections
            .get(collection)
            .and_then(|c| c.get(row_idx))
            .cloned()
        else {
            return;
        };
        if let Some(fts) = self.fts.get_mut(collection) {
            fts.sync_row(row_idx, before, &row);
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
                pred: None,
                bitmap_card_field: None,
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
            idx.seek_each(use_, now, |i| {
                if seen.insert(i) {
                    out.push(i);
                }
            });
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

    /// Count with early termination - returns None if count exceeds threshold
    pub fn index_seek_count_with_threshold(
        &self,
        collection: &str,
        uses: &[crate::index::IndexUse],
        now: i64,
        threshold: usize,
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
            return idx.seek_count_with_early_termination(use_, now, threshold);
        }
        // For multiple uses, fall back to full seek
        Some(self.index_seek(collection, uses, now)?.len())
    }

    /// Count `docs.title ~ needle` (no group) using columnar titles + memchr.
    pub fn docs_title_contains_count(&self, needle: &str) -> i64 {
        if needle.is_empty() {
            return self.docs_title.len() as i64;
        }
        // A derived physical column, not a result cache: every needle scans the
        // bytes. Mutation invalidates it and the next scan rebuilds it once.
        let (bytes, offsets) = self.docs_title_packed.get_or_init(|| {
            let mut bytes = Vec::with_capacity(self.docs_title.iter().map(|s| s.len()).sum());
            let mut offsets = Vec::with_capacity(self.docs_title.len() + 1);
            for title in &self.docs_title {
                offsets.push(bytes.len());
                bytes.extend_from_slice(title.as_bytes());
            }
            offsets.push(bytes.len());
            (bytes, offsets)
        });
        let finder = memchr::memmem::Finder::new(needle.as_bytes());
        let mut cursor = 0;
        let mut count = 0;
        while let Some(relative) = finder.find(&bytes[cursor..]) {
            let pos = cursor + relative;
            let row = offsets.partition_point(|&offset| offset <= pos) - 1;
            if pos + needle.len() <= offsets[row + 1] {
                count += 1;
                cursor = offsets[row + 1];
            } else {
                // Cross-row candidates must not hide overlapping real matches.
                cursor = pos + 1;
            }
        }
        count
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
    pub fn docs_wing(&self) -> &[Arc<str>] {
        &self.docs_wing
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

        let n = self.docs_id.len();
        let mut out = Vec::with_capacity(idxs.len());
        let id_title = fields.len() == 2 && fields[0] == "id" && fields[1] == "title";

        for &i in idxs {
            if i >= n {
                continue;
            }
            if id_title {
                out.push(row_id_title(&self.docs_id[i], &self.docs_title[i]));
                continue;
            }
            let mut row = Row::new();
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
            let mut row = Row::new();
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
    let mut row = Row::new();
    row.insert(String::from("id"), Cell::Text(Arc::clone(id)));
    row.insert(String::from("title"), Cell::Text(Arc::clone(title)));
    row
}

fn spo_key(row: &Row) -> Option<(Arc<str>, Arc<str>, Arc<str>)> {
    Some((
        row.get("s").and_then(Cell::text_shared)?,
        row.get("p").and_then(Cell::text_shared)?,
        row.get("o").and_then(Cell::text_shared)?,
    ))
}

pub fn project_fields(row: &Row, fields: &[String]) -> Row {
    let mut out = acquire_row();
    for f in fields {
        if let Some((k, v)) = row.get_key_value(f) {
            out.insert(k.clone(), v.clone_for_borrow());
        } else {
            out.insert(f.clone(), Cell::Null);
        }
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
    extra_filters: BTreeMap<String, String>,
    extra_owned: BTreeMap<String, Vec<(String, String)>>,
    hnsw: BTreeMap<String, crate::embed::HnswIndex>,
}

/// Move `v[p]` on top of the tail and drop the tail, keeping the vec dense. A
/// no-op when `v` isn't parallel to the rows (the caller's length guard rebuilds).
fn swap_pop_at<T: Clone>(v: &mut Vec<T>, p: usize, len: usize) {
    if v.len() != len {
        return;
    }
    let last = len - 1;
    if p < last {
        let moved = v[last].clone();
        v[p] = moved;
    }
    v.truncate(last);
}

/// Inverse of [`swap_pop_at`]: `value` takes slot `p` and whatever was there moves
/// to the tail. A no-op when `v` isn't parallel to the rows.
fn swap_push_at<T: Clone>(v: &mut Vec<T>, p: usize, len: usize, value: T) {
    if v.len() != len {
        return;
    }
    if p == len {
        v.push(value);
    } else {
        let moved = v[p].clone();
        v.push(moved);
        v[p] = value;
    }
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
    buf.push('|');
    for (n, c) in &cat.collections {
        buf.push_str(n);
        buf.push(':');
        buf.push(if c.append_only { 'A' } else { 'M' });
        buf.push('{');
        for (f, info) in &c.fields {
            buf.push_str(f);
            buf.push(':');
            buf.push_str(info.ty.name());
            if info.fts {
                buf.push_str("+fts");
            }
            if info.unique {
                buf.push_str("+uniq");
            }
            buf.push(',');
        }
        buf.push('}');
        if let Some(src) = &c.filter_src {
            buf.push('?');
            buf.push_str(src);
        }
        buf.push(';');
    }
    buf.push('|');
    for (n, r) in &cat.rels {
        buf.push_str(n);
        if r.stub {
            buf.push_str(":stub");
        }
        if let Some(of) = &r.reverse_of {
            buf.push('=');
            buf.push_str(of);
        }
        buf.push(';');
    }
    buf.push('|');
    for (n, idx) in &cat.indexes {
        buf.push_str(n);
        buf.push(':');
        if idx.unique {
            buf.push('U');
        }
        buf.push('[');
        buf.push_str(&idx.fields.join(","));
        buf.push(']');
        if let Some(pred) = &idx.pred {
            buf.push('?');
            buf.push_str(&crate::plan::fmt_pred(pred));
        }
        buf.push(';');
    }
    buf.push('|');
    for (n, fields) in &cat.owned {
        buf.push_str(n);
        buf.push('{');
        for (f, info) in fields {
            buf.push_str(f);
            buf.push(':');
            buf.push_str(info.ty.name());
            buf.push(',');
        }
        buf.push_str("};");
    }
    buf.push('|');
    for fk in &cat.fks {
        buf.push_str(&fk.from_col);
        buf.push('.');
        buf.push_str(&fk.from_field);
        buf.push_str("->");
        buf.push_str(&fk.to_col);
        buf.push('.');
        buf.push_str(&fk.to_field);
        buf.push(';');
    }
    buf.push('|');
    for g in &cat.guards {
        buf.push_str(&g.collection);
        buf.push('.');
        buf.push_str(&g.field);
        buf.push('?');
        buf.push_str(&crate::plan::fmt_pred(&g.pred));
        buf.push(';');
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
    let mut r = Row::new();
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
    let mut r = Row::new();
    r.insert("id".into(), Cell::text_arc(id));
    r.insert("email".into(), Cell::text_arc(email));
    r
}

fn order_row(id: &str, user_id: &str, total: f64, ts: i64) -> Row {
    let mut r = Row::new();
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

#[cfg(test)]
mod borrowed_delete_tests {
    use super::*;

    #[test]
    fn doc_slab_registration_matches_single_rows_with_missing_and_nontext_fields() {
        let rows = (0..64)
            .map(|i| {
                let mut row = Row::new();
                for field in ["id", "uri", "title", "layer", "wing", "extra"] {
                    match (i + field.len()) % 4 {
                        0 => {}
                        1 => {
                            row.insert(field.into(), Cell::Null);
                        }
                        2 => {
                            row.insert(field.into(), Cell::Int(i as i64));
                        }
                        _ => {
                            row.insert(field.into(), Cell::text_arc(format!("{field}-{}", i % 9)));
                        }
                    }
                }
                row
            })
            .collect::<Vec<_>>();
        let mut slab = Store::empty("test/8");
        let mut scalar = Store::empty("test/8");
        slab.row_maps_register_slab("docs", 0, &rows);
        for (i, row) in rows.iter().enumerate() {
            scalar.row_maps_register_row("docs", i, row);
        }
        assert_eq!(slab.by_id, scalar.by_id);
        assert_eq!(slab.docs_by_uri, scalar.docs_by_uri);
        assert_eq!(slab.docs_id, scalar.docs_id);
        assert_eq!(slab.docs_title, scalar.docs_title);
        assert_eq!(slab.docs_layer, scalar.docs_layer);
        assert_eq!(slab.docs_wing, scalar.docs_wing);
    }
    use crate::catalog::IndexDef;

    fn seed(collection: &str, rows: Vec<Row>) -> Store {
        let mut store = Store::empty("test");
        store.collections.insert(collection.into(), rows);
        store.rebuild_row_maps();
        for field in ["id", "group"] {
            let mut index = LiveIndex::new(IndexDef {
                collection: collection.into(),
                fields: vec![field.into()],
                unique: field == "id",
                pred: None,
                bitmap_card_field: None,
            });
            for (pos, row) in store.collection(collection).iter().enumerate() {
                index.insert_at_new(pos, row).unwrap();
            }
            store.indexes.insert(field.into(), index);
        }
        let fields = vec!["body".into(), "title".into()];
        store.fts.insert(
            collection.into(),
            crate::fts::FtsIndex::build(store.collection(collection), &fields),
        );
        store
    }

    #[test]
    fn borrowed_moves_match_rebuilt_rows_maps_columns_indexes_and_fts() {
        for collection in ["docs", "orders", "users", "facts", "custom"] {
            let rows = (0..31)
                .map(|i| {
                    Row::from([
                        ("id".into(), Cell::text_arc(format!("id{i}"))),
                        ("uri".into(), Cell::text_arc(format!("uri{i}"))),
                        ("title".into(), Cell::text_arc(format!("shared title{i}"))),
                        ("body".into(), Cell::text_arc(format!("shared unique{i}"))),
                        ("layer".into(), Cell::text_arc(format!("layer{i}"))),
                        ("wing".into(), Cell::text_arc(format!("wing{i}"))),
                        ("user_id".into(), Cell::text_arc(format!("user{i}"))),
                        ("total".into(), Cell::Float(i as f64 + 0.25)),
                        ("email".into(), Cell::text_arc(format!("mail{i}"))),
                        ("s".into(), Cell::text_arc(format!("s{i}"))),
                        ("p".into(), Cell::text_arc("tagged")),
                        ("o".into(), Cell::text_arc(format!("o{i}"))),
                        ("group".into(), Cell::Int(i % 3)),
                    ])
                })
                .collect::<Vec<_>>();
            let mut model = rows.clone();
            let mut store = seed(collection, rows);
            while !model.is_empty() {
                let dead = [0, model.len() / 3, model.len() / 3, usize::MAX];
                let mut valid = dead
                    .into_iter()
                    .filter(|&p| p < model.len())
                    .collect::<Vec<_>>();
                valid.sort_unstable();
                valid.dedup();
                for &pos in valid.iter().rev() {
                    model.swap_remove(pos);
                }
                store.delete_rows_at(collection, &dead);
                let expected = seed(collection, model.clone());
                assert_eq!(
                    store.collection(collection),
                    expected.collection(collection)
                );
                assert_eq!(store.by_id, expected.by_id);
                assert_eq!(store.docs_by_uri, expected.docs_by_uri);
                assert_eq!(store.facts_by_spo, expected.facts_by_spo);
                assert_eq!(store.docs_id, expected.docs_id);
                assert_eq!(store.docs_title, expected.docs_title);
                assert_eq!(store.docs_layer, expected.docs_layer);
                assert_eq!(store.docs_wing, expected.docs_wing);
                assert_eq!(store.orders_id, expected.orders_id);
                assert_eq!(store.orders_user_id, expected.orders_user_id);
                assert_eq!(store.orders_total, expected.orders_total);
                assert_eq!(store.users_id, expected.users_id);
                assert_eq!(store.users_email, expected.users_email);
                assert_eq!(store.facts_s, expected.facts_s);
                assert_eq!(store.facts_p, expected.facts_p);
                assert_eq!(store.facts_o, expected.facts_o);
                for (label, index) in &store.indexes {
                    let mut actual = index.forward.clone();
                    for positions in actual.values_mut() {
                        positions.sort_unstable();
                    }
                    assert_eq!(actual, expected.indexes[label].forward);
                    assert_eq!(index.reverse, expected.indexes[label].reverse);
                }
                for query in std::iter::once("shared".to_string())
                    .chain((0..31).map(|i| format!("unique{i}")))
                {
                    assert_eq!(
                        store.fts[collection].candidate_idxs(&query),
                        expected.fts[collection].candidate_idxs(&query)
                    );
                }
            }
        }
    }

    #[test]
    fn nonparallel_columns_keep_the_rebuild_fallback() {
        let rows = (0..4)
            .map(|i| {
                Row::from([
                    ("id".into(), Cell::text_arc(format!("id{i}"))),
                    ("title".into(), Cell::text_arc(format!("title{i}"))),
                    ("group".into(), Cell::Int(i)),
                ])
            })
            .collect::<Vec<_>>();
        let mut store = seed("docs", rows);
        store.docs_id.clear();
        store.docs_title.clear();
        store.docs_layer.clear();
        store.docs_wing.clear();
        store.delete_rows_at("docs", &[1]);
        let expected = seed("docs", store.collection("docs").to_vec());
        assert_eq!(store.docs_id, expected.docs_id);
        assert_eq!(store.docs_title, expected.docs_title);
        assert_eq!(store.by_id, expected.by_id);
    }

    #[test]
    fn sso_inline_str_and_cell_small_text_semantics() {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        // Size check: Cell must remain 24 bytes
        assert_eq!(std::mem::size_of::<Cell>(), 24);
        assert_eq!(std::mem::size_of::<InlineStr>(), 23);

        // Strings <= 22 bytes become SmallText
        let short = "user_12345";
        let c1 = Cell::from_text(short);
        assert!(matches!(c1, Cell::SmallText(_)));
        assert_eq!(c1.text(), Some(short));

        // Strings > 22 bytes become Text(Arc<str>)
        let long = "this is a very long string that exceeds 22 bytes";
        let c2 = Cell::from_text(long);
        assert!(matches!(c2, Cell::Text(_)));
        assert_eq!(c2.text(), Some(long));

        // Equality between SmallText and Text
        let c_arc = Cell::Text(std::sync::Arc::from(short));
        assert_eq!(c1, c_arc);
        assert_eq!(c_arc, c1);

        // Hashing consistency
        let mut h1 = DefaultHasher::new();
        c1.hash(&mut h1);
        let mut h2 = DefaultHasher::new();
        c_arc.hash(&mut h2);
        assert_eq!(h1.finish(), h2.finish());

        // Zero-cost clone_for_borrow
        let borrowed = c1.clone_for_borrow();
        assert_eq!(borrowed, c1);
        assert!(matches!(borrowed, Cell::SmallText(_)));

        // Serde roundtrip
        let serialized = serde_json::to_string(&c1).unwrap();
        let deserialized: Cell = serde_json::from_str(&serialized).unwrap();
        assert_eq!(c1, deserialized);

        // Boundary tests for 22 bytes exactly
        let s22 = "1234567890123456789012";
        assert_eq!(s22.len(), 22);
        let c22 = Cell::from_text(s22);
        assert!(matches!(c22, Cell::SmallText(_)));

        let s23 = "12345678901234567890123";
        assert_eq!(s23.len(), 23);
        let c23 = Cell::from_text(s23);
        assert!(matches!(c23, Cell::Text(_)));
    }

    #[test]
    fn cell_equality_and_hash_contract_across_all_variants() {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        fn hash_cell(c: &Cell) -> u64 {
            let mut h = DefaultHasher::new();
            c.hash(&mut h);
            h.finish()
        }

        let nan_q = Cell::Float(f64::from_bits(0x7ff8_0000_0000_0000));
        let nan_payload = Cell::Float(f64::from_bits(0x7ff8_0000_0000_0001));
        assert_eq!(nan_q, nan_q.clone());
        assert_eq!(hash_cell(&nan_q), hash_cell(&nan_q.clone()));
        assert_ne!(nan_q, nan_payload);

        // Int(1) and Float(1.0) are distinct Cell variants in Eq/Hash
        let int_one = Cell::Int(1);
        let float_one = Cell::Float(1.0);
        assert_ne!(int_one, float_one);
        assert_ne!(hash_cell(&int_one), hash_cell(&float_one));

        // Signed zeros have distinct bit representations in Cell::Float
        let pos_zero = Cell::Float(0.0);
        let neg_zero = Cell::Float(-0.0);
        assert_ne!(pos_zero, neg_zero);

        // SmallText and Text with identical UTF-8 content must be equal and have identical hashes
        let s_inline = Cell::from_text("group_key");
        let s_arc = Cell::Text(Arc::from("group_key"));
        assert_eq!(s_inline, s_arc);
        assert_eq!(hash_cell(&s_inline), hash_cell(&s_arc));

        // Every equal pair of cells must produce the exact same hash
        let samples = [
            Cell::Null,
            s_inline,
            s_arc,
            Cell::from_text("this string is longer than twenty-two bytes"),
            int_one,
            float_one,
            pos_zero,
            neg_zero,
            nan_q,
            Cell::Bool(true),
            Cell::Bool(false),
            Cell::Time(1_700_000_000_000),
            Cell::Vec(Arc::from([0.5_f32, -0.5_f32].as_slice())),
        ];
        for a in &samples {
            for b in &samples {
                if a == b {
                    assert_eq!(
                        hash_cell(a),
                        hash_cell(b),
                        "Hash/Eq mismatch for {a:?} and {b:?}"
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod empty_index_slab_tests {
    #[test]
    fn empty_index_slab_preserves_empty_and_populated_indexes() {
        for populated in [false, true] {
            let mut db = crate::Db::empty();
            db.run("index docs [wing, ts]").unwrap();
            if populated {
                db.run(r#"insert docs [{id:"a",uri:"a",wing:"rag",body:"first"},{id:"b",uri:"b",wing:"sys",body:"second"}]"#).unwrap();
            }
            let before = db.store.indexes.clone();
            let rows_before = db.store.collection("docs").to_vec();
            for collection in ["docs", "users", "missing"] {
                for start in [0, rows_before.len(), usize::MAX] {
                    db.store.index_insert_slab(collection, start, &[]).unwrap();
                    assert_eq!(db.store.indexes.len(), before.len());
                    for (label, index) in &before {
                        let after = &db.store.indexes[label];
                        assert_eq!(after.forward, index.forward);
                        assert_eq!(after.reverse, index.reverse);
                        assert_eq!(after.def.fields, index.def.fields);
                    }
                    assert_eq!(db.store.collection("docs"), rows_before);
                }
            }
        }
    }
}
