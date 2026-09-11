use std::collections::BTreeMap;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::ast::{Decl, TypeExpr};
use crate::catalog::Catalog;
use crate::error::Error;
use crate::index::LiveIndex;
use crate::persist::{self, ColSnap, Head, IndexSnap, LogRecord, Pack, Persist, RelSnap, Snapshot};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", content = "v")]
pub enum Cell {
    Null,
    Text(String),
    Int(i64),
    Float(f64),
    Bool(bool),
    Time(i64),
}

impl Cell {
    pub fn text(&self) -> Option<&str> {
        match self {
            Cell::Text(s) => Some(s),
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

    pub fn compact(&self) -> String {
        match self {
            Cell::Null => "null".into(),
            Cell::Text(s) => format!("{s:?}"),
            Cell::Int(n) => n.to_string(),
            Cell::Float(n) => n.to_string(),
            Cell::Bool(b) => b.to_string(),
            Cell::Time(ms) => fmt_iso_millis(*ms),
        }
    }
}

pub type Row = BTreeMap<String, Cell>;

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
    persist: Option<Persist>,
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
            persist: None,
        }
    }

    /// Open a durable store at `path` (created if missing).
    /// Loads `snapshot` if present, replays `log`, then serves like the
    /// in-memory store. Writes fsync the log record **before** `gen` bumps.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, Error> {
        let cat = crate::catalog::fixture();
        Self::open_with(path, &cat)
    }

    pub fn open_with(path: impl AsRef<Path>, cat: &Catalog) -> Result<Self, Error> {
        let dir = path.as_ref();
        persist::ensure_dir(dir)?;
        let fixture_hash = catalog_hash(cat);

        let snap = persist::read_snapshot(dir)?;
        let mut store = match &snap {
            Some(s) => Self::from_snapshot(s),
            None => {
                let embed = persist::read_head(dir)?
                    .map(|h| h.embed_id)
                    .unwrap_or_else(|| cat.embed_id.clone());
                Self::empty(embed)
            }
        };

        let mut log = persist::open_log(dir)?;
        let min_gen = store.r#gen;
        let start = snap.as_ref().map(|s| s.log_offset).unwrap_or(0);
        let end = persist::replay_log(&mut log, min_gen, start, |rec| {
            store.apply_record(&rec);
            Ok(())
        })?;
        persist::truncate_log(&mut log, end)?;

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

        store.rebuild_indexes();
        store.persist = Some(Persist {
            dir: dir.to_path_buf(),
            log,
            catalog_hash: live_hash,
            writes_since_snapshot: 0,
        });
        Ok(store)
    }

    fn from_snapshot(s: &Snapshot) -> Self {
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
            persist: None,
        };
        for name in store.extra_collections.keys() {
            store.collections.entry(name.clone()).or_default();
        }
        store.rebuild_indexes();
        store
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

    pub fn set_catalog_hash(&mut self, hash: String) {
        if let Some(p) = self.persist.as_mut() {
            p.catalog_hash = hash;
        }
    }

    pub fn close(&mut self) -> Result<(), Error> {
        if self.persist.is_some() {
            self.write_snapshot()?;
            self.persist = None;
        }
        Ok(())
    }

    pub fn is_durable(&self) -> bool {
        self.persist.is_some()
    }

    /// Fsync one committed pack, then the caller bumps `gen`.
    pub fn durable_commit(&mut self, pack: &Pack) -> Result<(), Error> {
        let Some(p) = self.persist.as_mut() else {
            return Ok(());
        };
        let rec = LogRecord {
            r#gen: self.r#gen + 1,
            next_id: self.next_id,
            pack: pack.clone(),
        };
        persist::append_record(&mut p.log, &rec)?;
        let _ = persist::write_head(
            &p.dir,
            &Head {
                r#gen: rec.r#gen,
                catalog_hash: p.catalog_hash.clone(),
                embed_id: self.embed_id.clone(),
            },
        );
        p.writes_since_snapshot += 1;
        Ok(())
    }

    pub fn maybe_checkpoint(&mut self) -> Result<(), Error> {
        let n = self
            .persist
            .as_ref()
            .map(|p| p.writes_since_snapshot)
            .unwrap_or(0);
        if n >= persist::SNAPSHOT_EVERY {
            self.write_snapshot()?;
        }
        Ok(())
    }

    fn write_snapshot(&mut self) -> Result<(), Error> {
        let Some(p) = self.persist.as_mut() else {
            return Ok(());
        };
        let log_offset = p.log.metadata().map_err(persist::io_err)?.len();
        let snap = Snapshot {
            r#gen: self.r#gen,
            embed_id: self.embed_id.clone(),
            next_id: self.next_id,
            catalog_hash: p.catalog_hash.clone(),
            log_offset,
            collections: self.collections.clone(),
            edges: self.edges.clone(),
            extra_collections: self.extra_collections.clone(),
            extra_rels: self.extra_rels.clone(),
            extra_indexes: self.extra_indexes.clone(),
        };
        persist::write_snapshot(&p.dir, &snap)?;
        p.writes_since_snapshot = 0;
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
                let _ = self.index_insert(collection, row);
                for e in edges {
                    if !self
                        .edges
                        .iter()
                        .any(|x| x.rel == e.rel && x.from == e.from && x.to == e.to)
                    {
                        self.edges.push(e.clone());
                    }
                }
            }
            Pack::AppendFact { row } => {
                let key = (
                    row_text(row, "s").unwrap_or("").to_string(),
                    row_text(row, "p").unwrap_or("").to_string(),
                    row_text(row, "o").unwrap_or("").to_string(),
                );
                let exists = self.collection("facts").iter().any(|r| {
                    row_text(r, "s") == Some(key.0.as_str())
                        && row_text(r, "p") == Some(key.1.as_str())
                        && row_text(r, "o") == Some(key.2.as_str())
                });
                if !exists {
                    self.collection_mut("facts").push(row.clone());
                }
            }
            Pack::AppendEdge { rel, from, to } => {
                if !self
                    .edges
                    .iter()
                    .any(|e| e.rel == *rel && e.from == *from && e.to == *to)
                {
                    self.edges.push(Edge {
                        rel: rel.clone(),
                        from: from.clone(),
                        to: to.clone(),
                    });
                }
            }
            Pack::Update { collection, rows } => {
                for new in rows {
                    let id = row_text(new, "id").map(str::to_string);
                    let old = id.as_ref().and_then(|id| {
                        self.collection(collection)
                            .iter()
                            .find(|r| row_text(r, "id") == Some(id.as_str()))
                            .cloned()
                    });
                    if let Some(old) = old {
                        self.index_remove(collection, &old);
                        if let Some(slot) = self
                            .collection_mut(collection)
                            .iter_mut()
                            .find(|r| row_text(r, "id") == row_text(new, "id"))
                        {
                            *slot = new.clone();
                        }
                        let _ = self.index_insert(collection, new);
                        continue;
                    }
                    self.collection_mut(collection).push(new.clone());
                    let _ = self.index_insert(collection, new);
                }
            }
            Pack::Reembed => {}
            Pack::Delete { collection, rows } => {
                for d in rows {
                    self.index_remove(collection, d);
                }
                self.collection_mut(collection)
                    .retain(|r| !rows.iter().any(|d| same_row_key(collection, r, d)));
            }
            Pack::DeleteEdge { rel, from, to } => {
                self.edges
                    .retain(|e| !(e.rel == *rel && e.from == *from && e.to == *to));
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
                for row in self.collection(collection) {
                    if let Some(id) = crate::index::row_id(row) {
                        let _ = live.insert(&id, row);
                    }
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

    pub fn mem_backup(&self) -> MemBackup {
        MemBackup {
            r#gen: self.r#gen,
            embed_id: self.embed_id.clone(),
            next_id: self.next_id,
            collections: self.collections.clone(),
            edges: self.edges.clone(),
            extra_collections: self.extra_collections.clone(),
            extra_rels: self.extra_rels.clone(),
            extra_indexes: self.extra_indexes.clone(),
            indexes: self.indexes.clone(),
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
        self.indexes = b.indexes;
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
        s
    }

    pub fn alloc_id(&mut self) -> String {
        let id = format!("id-{}", self.next_id);
        self.next_id += 1;
        id
    }

    pub fn collection(&self, name: &str) -> &[Row] {
        self.collections
            .get(name)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    pub fn collection_mut(&mut self, name: &str) -> &mut Vec<Row> {
        self.collections.entry(name.to_string()).or_default()
    }

    pub fn find_doc_key(&self, key: &str) -> Option<&Row> {
        self.collection("docs")
            .iter()
            .find(|r| row_text(r, "id") == Some(key) || row_text(r, "uri") == Some(key))
    }

    pub fn index_insert(&mut self, collection: &str, row: &Row) -> Result<(), Error> {
        let Some(id) = crate::index::row_id(row) else {
            return Ok(());
        };
        let labels: Vec<String> = self
            .indexes
            .iter()
            .filter(|(_, idx)| idx.def.collection == collection)
            .map(|(k, _)| k.clone())
            .collect();
        for label in labels {
            if let Some(idx) = self.indexes.get_mut(&label)
                && let Err(e) = idx.insert(&id, row)
            {
                return Err(Error::runtime(e));
            }
        }
        Ok(())
    }

    pub fn index_remove(&mut self, collection: &str, row: &Row) {
        let Some(id) = crate::index::row_id(row) else {
            return;
        };
        for idx in self.indexes.values_mut() {
            if idx.def.collection == collection {
                idx.remove(&id);
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
            for row in self.collection(&snap.collection) {
                if let Some(id) = crate::index::row_id(row) {
                    let _ = live.insert(&id, row);
                }
            }
            self.indexes.insert(label, live);
        }
    }

    pub fn index_seek(
        &self,
        collection: &str,
        use_: &crate::index::IndexUse,
        now: i64,
    ) -> Option<Vec<String>> {
        let label = use_.def.label();
        let idx = self.indexes.get(&label)?;
        if idx.def.collection != collection {
            return None;
        }
        Some(idx.seek(use_, now))
    }
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
    indexes: BTreeMap<String, LiveIndex>,
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

pub fn content_hash(body: &str) -> String {
    format!("h:{:016x}", fnv1a64(body.as_bytes()))
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
    r.insert("id".into(), Cell::Text(d.id.into()));
    r.insert("uri".into(), Cell::Text(d.uri.into()));
    r.insert("title".into(), Cell::Text(d.title.into()));
    r.insert("wing".into(), Cell::Text(d.wing.into()));
    r.insert("room".into(), Cell::Text(d.room.into()));
    r.insert("layer".into(), Cell::Text(d.layer.into()));
    r.insert("body".into(), Cell::Text(d.body.into()));
    r.insert("hash".into(), Cell::Text(content_hash(d.body)));
    r.insert("snippet".into(), Cell::Text(d.snippet.into()));
    r.insert("ts".into(), Cell::Time(d.ts));
    r
}

fn user_row(id: &str, email: &str) -> Row {
    let mut r = BTreeMap::new();
    r.insert("id".into(), Cell::Text(id.into()));
    r.insert("email".into(), Cell::Text(email.into()));
    r
}

fn order_row(id: &str, user_id: &str, total: f64, ts: i64) -> Row {
    let mut r = BTreeMap::new();
    r.insert("id".into(), Cell::Text(id.into()));
    r.insert("user_id".into(), Cell::Text(user_id.into()));
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
