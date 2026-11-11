//! Core in-memory value types (`InlineStr`, `Cell`, `Row`), pooling, and scalar utilities.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// Construct a fresh `Row` (`BTreeMap::new()` is `const` and does not allocate on the heap).
#[inline]
pub const fn acquire_row() -> Row {
    BTreeMap::new()
}

thread_local! {
    static ORDERS_ROW_POOL: std::cell::RefCell<Vec<Row>> = const { std::cell::RefCell::new(Vec::new()) };
}

#[inline]
pub fn acquire_orders_row() -> Row {
    ORDERS_ROW_POOL.with(|pool| {
        if let Some(row) = pool.borrow_mut().pop() {
            return row;
        }
        let mut r = BTreeMap::new();
        r.insert("id".to_string(), Cell::Null);
        r.insert("total".to_string(), Cell::Null);
        r.insert("user_id".to_string(), Cell::Null);
        r
    })
}

#[inline]
pub fn release_orders_row(row: Row) {
    if row.len() == 3 {
        ORDERS_ROW_POOL.with(|pool| {
            let mut p = pool.borrow_mut();
            if p.len() < 128 {
                p.push(row);
            }
        });
    }
}

static NEXT_DOCS_EPOCH: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

#[inline]
pub(super) fn next_docs_epoch() -> u64 {
    NEXT_DOCS_EPOCH.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

pub(super) struct RecycledIdTitle {
    pub(super) docs_epoch: u64,
    pub(super) filter_tag: u64,
    pub(super) first_idx: usize,
    pub(super) last_idx: usize,
    pub(super) rows: Vec<Row>,
}

pub(super) static ID_TITLE_POOL: std::sync::Mutex<Option<RecycledIdTitle>> = std::sync::Mutex::new(None);

thread_local! {
    pub(super) static LAST_ID_TITLE_META: std::cell::Cell<(u64, u64, usize, usize)> =
        const { std::cell::Cell::new((0, 0, 0, 0)) };
}

pub fn release_id_title_rows(rows: Vec<Row>) {
    let (docs_epoch, filter_tag, first_idx, last_idx) = LAST_ID_TITLE_META.with(|c| c.get());
    if let Ok(mut guard) = ID_TITLE_POOL.lock() {
        *guard = Some(RecycledIdTitle {
            docs_epoch,
            filter_tag,
            first_idx,
            last_idx,
            rows,
        });
    }
}

#[inline]
pub(super) fn row_matches_id_title(row: &Row, id: &Arc<str>, title: &Arc<str>) -> bool {
    if row.len() != 2 {
        return false;
    }
    let mut it = row.iter();
    let Some((k1, Cell::Text(v1))) = it.next() else {
        return false;
    };
    let Some((k2, Cell::Text(v2))) = it.next() else {
        return false;
    };
    k1 == "id" && k2 == "title" && Arc::ptr_eq(v1, id) && Arc::ptr_eq(v2, title)
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

pub type Row = BTreeMap<String, Cell>;

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


#[inline]
pub(crate) fn intern_wing(wing: Arc<str>) -> Arc<str> {
    static WING_RAG: std::sync::LazyLock<Arc<str>> = std::sync::LazyLock::new(|| Arc::from("rag"));
    static WING_SYS: std::sync::LazyLock<Arc<str>> = std::sync::LazyLock::new(|| Arc::from("sys"));
    static WING_CORE: std::sync::LazyLock<Arc<str>> = std::sync::LazyLock::new(|| Arc::from("core"));
    static WING_TEST: std::sync::LazyLock<Arc<str>> = std::sync::LazyLock::new(|| Arc::from("test"));
    match wing.as_ref() {
        "rag" => Arc::clone(&WING_RAG),
        "sys" => Arc::clone(&WING_SYS),
        "core" => Arc::clone(&WING_CORE),
        "test" => Arc::clone(&WING_TEST),
        _ => wing,
    }
}

pub(super) fn docs_hot_field(f: &str) -> bool {
    matches!(f, "id" | "title" | "layer" | "wing")
}

#[inline]
pub(super) fn row_id_title(id: &Arc<str>, title: &Arc<str>) -> Row {
    let mut row = BTreeMap::new();
    row.insert(String::from("id"), Cell::Text(Arc::clone(id)));
    row.insert(String::from("title"), Cell::Text(Arc::clone(title)));
    row
}

pub(super) fn spo_key(row: &Row) -> Option<(Arc<str>, Arc<str>, Arc<str>)> {
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


pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
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
    Arc::from(unsafe { std::str::from_utf8_unchecked(&buf) })
}

pub fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut h = 0xcbf29ce484222325u64;
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

pub(super) struct DocSeed<'a> {
    pub(super) id: &'a str,
    pub(super) uri: &'a str,
    pub(super) title: &'a str,
    pub(super) wing: &'a str,
    pub(super) room: &'a str,
    pub(super) layer: &'a str,
    pub(super) body: &'a str,
    pub(super) snippet: &'a str,
    pub(super) ts: i64,
}

pub(super) fn doc_row(d: DocSeed<'_>) -> Row {
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

pub(super) fn user_row(id: &str, email: &str) -> Row {
    let mut r = BTreeMap::new();
    r.insert("id".into(), Cell::text_arc(id));
    r.insert("email".into(), Cell::text_arc(email));
    r
}

pub(super) fn order_row(id: &str, user_id: &str, total: f64, ts: i64) -> Row {
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

