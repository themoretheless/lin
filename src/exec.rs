use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::time::Instant;

use crate::ast::*;
use crate::catalog::Catalog;
use crate::check;
use crate::error::Error;
use crate::explain::{self, ExplainCtx, RunStats};
use crate::graph::GraphFmt;
use crate::parse;
use crate::persist::Pack;
use crate::plan::{self, Plan};
use crate::store::{Cell, Edge, Row, Store, compact_row, content_hash, now_ms, row_text};

type StmtOut = (Vec<Row>, Option<String>, Option<Pack>);

struct PackCtx {
    snap: BTreeMap<(String, String), String>,
    written: BTreeSet<(String, String)>,
}

fn mark_written(ctx: &mut PackCtx, collection: &str, rows: &[Row]) {
    for r in rows {
        if let Some(id) = row_text(r, "id") {
            ctx.written.insert((collection.to_string(), id.to_string()));
        }
    }
}

#[derive(Debug, Clone)]
pub struct Done {
    pub r#gen: u64,
    pub n: usize,
}

#[derive(Debug, Clone)]
pub struct Handle {
    pub plan: Plan,
    pub rows: Vec<Row>,
    pub done: Done,
    pub message: Option<String>,
    pub ms: f64,
}

impl Handle {
    pub fn compact(&self) -> String {
        let mut out = String::new();
        for row in &self.rows {
            out.push_str(&compact_row(row));
            out.push('\n');
        }
        if let Some(msg) = &self.message {
            out.push_str(msg);
            out.push('\n');
        }
        out.push_str(&format!("gen={} n={}", self.done.r#gen, self.done.n));
        out
    }
}

pub struct Db {
    pub catalog: Catalog,
    pub store: Store,
}

impl Db {
    pub fn fixture() -> Self {
        let catalog = crate::catalog::fixture();
        let store = Store::fixture(&catalog);
        Self { catalog, store }
    }

    pub fn empty() -> Self {
        let catalog = crate::catalog::fixture();
        let store = Store::empty(catalog.embed_id.clone());
        Self { catalog, store }
    }

    pub fn open(path: impl AsRef<Path>) -> Result<Self, Error> {
        let catalog = crate::catalog::fixture();
        let store = Store::open_with(path, &catalog)?;
        let mut catalog = catalog;
        store.merge_extras_into(&mut catalog);
        Ok(Self { catalog, store })
    }

    pub fn close(&mut self) -> Result<(), Error> {
        self.store.close()
    }

    pub fn run(&mut self, src: &str) -> Result<Handle, Error> {
        let stmts = parse::parse_program(src)?;
        let mut check_cat = self.catalog.clone();
        check::check_program(&stmts, &mut check_cat)?;
        let plan = plan::plan_program(&stmts, &check_cat)?;
        let t0 = Instant::now();
        let store_backup = self.store.mem_backup();
        let cat_backup = self.catalog.clone();
        let (rows, message, pack) = match self.exec_program(&stmts) {
            Ok(v) => v,
            Err(e) => {
                self.store.mem_restore(store_backup);
                self.catalog = cat_backup;
                return Err(e);
            }
        };
        let ms = t0.elapsed().as_secs_f64() * 1000.0;
        if let Some(pack) = pack {
            // Durable first: fsync the log record, then bump in-memory gen.
            if self.store.is_durable() {
                self.store
                    .set_catalog_hash(crate::store::catalog_hash(&self.catalog));
                if let Err(e) = self.store.durable_commit(&pack) {
                    self.store.mem_restore(store_backup);
                    self.catalog = cat_backup;
                    return Err(e);
                }
            }
            self.store.r#gen += 1;
            if self.store.is_durable() {
                self.store.maybe_checkpoint()?;
            }
        }
        let n = rows.len();
        Ok(Handle {
            plan,
            rows,
            done: Done {
                r#gen: self.store.r#gen,
                n,
            },
            message,
            ms,
        })
    }

    pub fn explain_as(&mut self, src: &str, graph: Option<GraphFmt>) -> Result<String, Error> {
        let stmts = parse::parse_program(src)?;
        let mut cat = self.catalog.clone();
        check::check_program(&stmts, &mut cat)?;
        let mut plan = plan::plan_program(&stmts, &cat)?;
        if let Some(fmt) = graph {
            plan.explain = match fmt {
                GraphFmt::Mermaid => crate::ast::ExplainKind::Graph,
                GraphFmt::Dot => crate::ast::ExplainKind::Dot,
            };
        }
        let mut ctx = ExplainCtx {
            r#gen: Some(self.store.r#gen),
            ..ExplainCtx::default()
        };
        if plan.explain == crate::ast::ExplainKind::Run {
            let handle = self.run(src)?;
            ctx.stats = Some(RunStats {
                rows: handle.done.n,
                ms: handle.ms,
            });
            ctx.r#gen = Some(handle.done.r#gen);
            return Ok(explain::format_with(&handle.plan, &ctx));
        }
        Ok(explain::format_with(&plan, &ctx))
    }

    fn exec_program(&mut self, stmts: &[Stmt]) -> Result<StmtOut, Error> {
        let mut bindings: BTreeMap<String, Vec<Row>> = BTreeMap::new();
        let mut ctx = PackCtx {
            snap: self.hash_snapshot(),
            written: BTreeSet::new(),
        };
        let mut last_rows = Vec::new();
        let mut last_msg = None;
        let mut packs = Vec::new();
        for stmt in stmts {
            if let Stmt::Let { name, query } = stmt {
                let rows = self.exec_query(query, &bindings)?;
                last_rows = rows.clone();
                last_msg = None;
                bindings.insert(name.clone(), rows);
                continue;
            }
            let (rows, message, pack) = self.exec_stmt(stmt, &bindings, &mut ctx)?;
            last_rows = rows;
            last_msg = message;
            if let Some(p) = pack {
                packs.push(p);
            }
        }
        Ok((last_rows, last_msg, fold_packs(packs)))
    }

    fn hash_snapshot(&self) -> BTreeMap<(String, String), String> {
        let mut m = BTreeMap::new();
        for (col, rows) in &self.store.collections {
            for r in rows {
                if let Some(id) = row_text(r, "id") {
                    m.insert(
                        (col.clone(), id.to_string()),
                        row_text(r, "hash").unwrap_or("").to_string(),
                    );
                }
            }
        }
        m
    }

    fn exec_stmt(
        &mut self,
        stmt: &Stmt,
        bindings: &BTreeMap<String, Vec<Row>>,
        ctx: &mut PackCtx,
    ) -> Result<StmtOut, Error> {
        match stmt {
            Stmt::Query(q) => Ok((self.exec_query(q, bindings)?, None, None)),
            Stmt::AppendFacts { records } => {
                let mut rows = Vec::new();
                let mut packs = Vec::new();
                let mut all_idemp = true;
                for record in records {
                    let (row, changed) = self.append_fact(record)?;
                    if changed {
                        all_idemp = false;
                        packs.push(Pack::AppendFact { row: row.clone() });
                    }
                    rows.push(row);
                }
                let msg = if all_idemp {
                    Some("idempotent".into())
                } else {
                    None
                };
                Ok((rows, msg, fold_packs(packs)))
            }
            Stmt::AppendEdges { edges } => {
                let mut rows = Vec::new();
                let mut packs = Vec::new();
                let mut all_idemp = true;
                for e in edges {
                    let (row, changed) = self.append_edge(&e.rel, &e.from, &e.to)?;
                    if changed {
                        all_idemp = false;
                        packs.push(Pack::AppendEdge {
                            rel: e.rel.clone(),
                            from: row_text(&row, "from").unwrap_or("").to_string(),
                            to: row_text(&row, "to").unwrap_or("").to_string(),
                        });
                    }
                    rows.push(row);
                }
                let msg = if all_idemp {
                    Some("idempotent".into())
                } else {
                    None
                };
                Ok((rows, msg, fold_packs(packs)))
            }
            Stmt::Insert {
                collection,
                records,
                edges,
            } => {
                let mut rows = Vec::new();
                let mut packs = Vec::new();
                for (i, record) in records.iter().enumerate() {
                    let eds = if i == 0 { edges.as_slice() } else { &[] };
                    let (row, new_edges) = self.insert_pack(collection, record, eds)?;
                    packs.push(Pack::Insert {
                        collection: collection.clone(),
                        row: row.clone(),
                        edges: new_edges,
                    });
                    rows.push(row);
                }
                mark_written(ctx, collection, &rows);
                Ok((rows, None, fold_packs(packs)))
            }
            Stmt::Update {
                collection,
                pred,
                cas,
                cas_each,
                record,
            } => {
                let rows = self.update_cas(
                    collection,
                    pred.as_ref(),
                    cas.as_deref(),
                    *cas_each,
                    record,
                    ctx,
                )?;
                Ok((
                    rows.clone(),
                    None,
                    Some(Pack::Update {
                        collection: collection.clone(),
                        rows,
                    }),
                ))
            }
            Stmt::Delete {
                collection,
                pred,
                cas,
                cas_each,
            } => {
                let rows = self.delete_rows(collection, pred, cas.as_deref(), *cas_each, ctx)?;
                Ok((
                    rows.clone(),
                    None,
                    Some(Pack::Delete {
                        collection: collection.clone(),
                        rows,
                    }),
                ))
            }
            Stmt::DeleteEdge { rel, from, to } => {
                let row = self.delete_edge(rel, from, to)?;
                Ok((
                    vec![row],
                    None,
                    Some(Pack::DeleteEdge {
                        rel: rel.clone(),
                        from: value_text(from, now_ms()),
                        to: value_text(to, now_ms()),
                    }),
                ))
            }
            Stmt::Let { name, query } => {
                let rows = self.exec_query(query, bindings)?;
                Ok((rows, Some(format!("let {name}")), None))
            }
            Stmt::Reembed { collection, .. } => {
                let msg = format!(
                    "reembed {collection}: identity-preserving no-op (no embedder); embed_id={}",
                    self.store.embed_id
                );
                Ok((Vec::new(), Some(msg), Some(Pack::Reembed)))
            }
            Stmt::Decl(d) => self.exec_decl(d),
            Stmt::IdbSlice { query } => Ok((
                self.exec_query(query, bindings)?,
                Some("idb slice: native".into()),
                None,
            )),
            Stmt::IdbPull { .. } | Stmt::IdbPush => Ok((
                Vec::new(),
                Some("idb: not available in-process".into()),
                None,
            )),
            Stmt::Snapshot { name } | Stmt::Restore { name } => {
                Ok((Vec::new(), Some(format!("snapshot {name:?}: no-op")), None))
            }
        }
    }

    fn exec_query(
        &self,
        q: &Query,
        bindings: &BTreeMap<String, Vec<Row>>,
    ) -> Result<Vec<Row>, Error> {
        self.exec_query_lim(q, bindings, true)
    }

    fn exec_query_lim(
        &self,
        q: &Query,
        bindings: &BTreeMap<String, Vec<Row>>,
        allow_implicit_take: bool,
    ) -> Result<Vec<Row>, Error> {
        let now = now_ms();
        let mut primary = check::collection_of(&q.source).to_string();
        let mut implicit_take = true;
        let mut saw_agg = false;
        let first_filter = q.steps.iter().find_map(|s| match s {
            Step::Filter(p) => Some(p),
            _ => None,
        });
        let mut rows = match &q.source {
            Source::Page(uri) => self.get_docs("uri", uri),
            Source::Catalog => self.scan_catalog(),
            Source::Collection(name) => {
                if let Some(bound) = bindings.get(name) {
                    bound.clone()
                } else if let Some(pred) = first_filter
                    && let Some((field, key)) = point_key(pred)
                    && matches!(name.as_str(), "docs" | "users" | "orders")
                {
                    self.get_by(name, field, key)
                } else if let Some(pred) = first_filter
                    && let Some(ids) = self.seek_index(name, pred, now)
                {
                    self.rows_by_ids(name, &ids)
                } else {
                    self.scan(name)
                }
            }
        };

        for step in &q.steps {
            match step {
                Step::Filter(pred) => {
                    if let Some((field, key)) = point_key(pred)
                        && matches!(primary.as_str(), "docs" | "users" | "orders")
                        && rows.len() != 1
                    {
                        rows = self.get_by(&primary, field, key);
                    }
                    rows.retain(|r| eval_pred(pred, r, now));
                }
                Step::Project(fields) => {
                    rows = project(&rows, fields);
                }
                Step::Join {
                    left,
                    collection,
                    on,
                } => {
                    rows = self.join_rows(&primary, rows, collection, on, *left);
                }
                Step::Hop { rel, depth } => {
                    rows = self.hop(&primary, &rows, rel, depth.unwrap_or(1))?;
                }
                Step::Search { mode, query } => {
                    rows = self.search_rows(&rows, *mode, query)?;
                }
                Step::Count { by } => {
                    implicit_take = false;
                    saw_agg = true;
                    rows = agg_count(&rows, by);
                    primary = String::new();
                }
                Step::Sum { field, by } => {
                    implicit_take = false;
                    saw_agg = true;
                    rows = agg_sum(&rows, field, by);
                    primary = String::new();
                }
                Step::Sort { field, desc } => {
                    sort_rows(&mut rows, field, *desc);
                }
                Step::Take { n } => {
                    implicit_take = false;
                    if let Some(n) = n {
                        let n = (*n).max(0) as usize;
                        if rows.len() > n {
                            rows.truncate(n);
                        }
                    }
                }
                Step::Union(rhs) => {
                    let mut right = self.exec_query_lim(rhs, bindings, false)?;
                    rows.append(&mut right);
                }
            }
        }

        if allow_implicit_take && implicit_take && !saw_agg && rows.len() > 50 {
            rows.truncate(50);
        }
        Ok(rows)
    }

    fn scan(&self, name: &str) -> Vec<Row> {
        if name == "catalog" {
            return self.scan_catalog();
        }
        self.store.collection(name).to_vec()
    }

    fn scan_catalog(&self) -> Vec<Row> {
        let mut rows = Vec::new();
        for name in self.catalog.collections.keys() {
            rows.push(meta_row("col", name));
        }
        for name in self.catalog.rels.keys() {
            rows.push(meta_row("rel", name));
        }
        rows
    }

    fn get_docs(&self, field: &str, key: &str) -> Vec<Row> {
        self.get_by("docs", field, key)
    }

    fn get_by(&self, collection: &str, field: &str, key: &str) -> Vec<Row> {
        self.store
            .collection(collection)
            .iter()
            .filter(|r| row_text(r, field) == Some(key))
            .cloned()
            .collect()
    }

    fn join_rows(
        &self,
        left_col: &str,
        left: Vec<Row>,
        right_col: &str,
        on: &str,
        left_join: bool,
    ) -> Vec<Row> {
        let to_field = self
            .catalog
            .find_fk(left_col, on, right_col)
            .map(|fk| fk.to_field.as_str())
            .unwrap_or("id");
        let right = self.scan(right_col);
        let mut idx: BTreeMap<String, Vec<&Row>> = BTreeMap::new();
        for r in &right {
            if let Some(k) = cell_key(r.get(to_field).unwrap_or(&Cell::Null)) {
                idx.entry(k).or_default().push(r);
            }
        }
        let mut out = Vec::new();
        for l in left {
            let key = l.get(on).and_then(cell_key);
            let hits = key.as_ref().and_then(|k| idx.get(k));
            if let Some(rs) = hits {
                for r in rs {
                    let mut merged = l.clone();
                    for (k, v) in *r {
                        merged.insert(format!("{right_col}.{k}"), v.clone());
                    }
                    out.push(merged);
                }
            } else if left_join {
                out.push(l);
            }
        }
        out
    }

    fn hop(&self, primary: &str, rows: &[Row], rel: &str, depth: i64) -> Result<Vec<Row>, Error> {
        let (edge_rel, reverse) = match self.catalog.rel(rel) {
            Some(r) if let Some(of) = &r.reverse_of => (of.as_str(), true),
            _ => (rel, false),
        };
        let mut frontier: BTreeSet<String> = BTreeSet::new();
        for r in rows {
            if let Some(id) = row_text(r, "id") {
                frontier.insert(id.to_string());
            }
            if let Some(uri) = row_text(r, "uri") {
                frontier.insert(uri.to_string());
            }
        }
        let mut seen = frontier.clone();
        let mut reached: BTreeSet<String> = BTreeSet::new();
        let depth = depth.clamp(1, 3);
        for _ in 0..depth {
            let mut next = BTreeSet::new();
            for e in &self.store.edges {
                if e.rel != edge_rel {
                    continue;
                }
                let (src, dst) = if reverse {
                    (e.to.as_str(), e.from.as_str())
                } else {
                    (e.from.as_str(), e.to.as_str())
                };
                if frontier.contains(src) && seen.insert(dst.to_string()) {
                    next.insert(dst.to_string());
                    reached.insert(dst.to_string());
                }
            }
            if reached.len() >= 300 {
                break;
            }
            frontier = next;
            if frontier.is_empty() {
                break;
            }
        }
        let mut out = Vec::new();
        for key in reached {
            if let Some(row) = self.store.find_doc_key(&key) {
                out.push(row.clone());
            } else if primary != "docs"
                && let Some(row) = self
                    .store
                    .collection(primary)
                    .iter()
                    .find(|r| row_text(r, "id") == Some(key.as_str()))
            {
                out.push(row.clone());
            }
            if out.len() >= 300 {
                break;
            }
        }
        Ok(out)
    }

    fn search_rows(&self, rows: &[Row], mode: SearchMode, query: &str) -> Result<Vec<Row>, Error> {
        if matches!(mode, SearchMode::Vec | SearchMode::Hybrid)
            && self.store.embed_id != self.catalog.embed_id
        {
            return Err(Error::runtime(format!(
                "embed_id mismatch: store={} catalog={}",
                self.store.embed_id, self.catalog.embed_id
            )));
        }
        match mode {
            SearchMode::Vec => Ok(Vec::new()),
            SearchMode::Lex | SearchMode::Hybrid => {
                let mut scored: Vec<(i64, Row)> = rows
                    .iter()
                    .filter_map(|r| {
                        let s = lex_score(r, query);
                        if s > 0 { Some((s, r.clone())) } else { None }
                    })
                    .collect();
                scored.sort_by_key(|a| std::cmp::Reverse(a.0));
                Ok(scored.into_iter().map(|(_, r)| r).collect())
            }
        }
    }

    fn append_fact(&mut self, record: &Record) -> Result<(Row, bool), Error> {
        let now = now_ms();
        let row = record_row(record, now);
        let key = (
            row_text(&row, "s").unwrap_or("").to_string(),
            row_text(&row, "p").unwrap_or("").to_string(),
            row_text(&row, "o").unwrap_or("").to_string(),
        );
        let facts = self.store.collection("facts");
        if let Some(existing) = facts.iter().find(|r| {
            row_text(r, "s") == Some(key.0.as_str())
                && row_text(r, "p") == Some(key.1.as_str())
                && row_text(r, "o") == Some(key.2.as_str())
        }) {
            return Ok((existing.clone(), false));
        }
        self.store.collection_mut("facts").push(row.clone());
        Ok((row, true))
    }

    fn append_edge(&mut self, rel: &str, from: &Value, to: &Value) -> Result<(Row, bool), Error> {
        let now = now_ms();
        let from = value_text(from, now);
        let to = value_text(to, now);
        if self
            .store
            .edges
            .iter()
            .any(|e| e.rel == rel && e.from == from && e.to == to)
        {
            return Ok((edge_row(rel, &from, &to), false));
        }
        self.store.edges.push(Edge {
            rel: rel.to_string(),
            from: from.clone(),
            to: to.clone(),
        });
        Ok((edge_row(rel, &from, &to), true))
    }

    fn insert_pack(
        &mut self,
        collection: &str,
        record: &Record,
        edges: &[InsertEdge],
    ) -> Result<(Row, Vec<Edge>), Error> {
        let now = now_ms();
        let mut row = record_row(record, now);
        if row_text(&row, "id").is_none() {
            row.insert("id".into(), Cell::Text(self.store.alloc_id()));
        }
        if row_text(&row, "hash").is_none()
            && let Some(body) = row_text(&row, "body").map(str::to_string)
        {
            row.insert("hash".into(), Cell::Text(content_hash(&body)));
        }
        if let Some(id) = row_text(&row, "id").map(str::to_string)
            && self
                .store
                .collection(collection)
                .iter()
                .any(|r| row_text(r, "id") == Some(id.as_str()))
        {
            return Err(Error::runtime(format!("duplicate id: {id}")));
        }
        if let Some(uri) = row_text(&row, "uri").map(str::to_string)
            && collection == "docs"
            && self
                .store
                .collection("docs")
                .iter()
                .any(|r| row_text(r, "uri") == Some(uri.as_str()))
        {
            return Err(Error::runtime(format!("duplicate uri: {uri}")));
        }
        let from_id = row_text(&row, "id").unwrap_or("").to_string();
        let mut new_edges = Vec::new();
        for e in edges {
            let to = match &e.target {
                EdgeTarget::Page(uri) => self
                    .store
                    .find_doc_key(uri)
                    .and_then(|r| row_text(r, "id").map(|s| s.to_string()))
                    .unwrap_or_else(|| uri.clone()),
                EdgeTarget::Value(v) => value_text(v, now),
            };
            let edge = Edge {
                rel: e.rel.clone(),
                from: from_id.clone(),
                to,
            };
            if !self
                .store
                .edges
                .iter()
                .any(|x| x.rel == edge.rel && x.from == edge.from && x.to == edge.to)
            {
                self.store.edges.push(edge.clone());
            }
            new_edges.push(edge);
        }
        self.store.index_insert(collection, &row)?;
        self.store.collection_mut(collection).push(row.clone());
        Ok((row, new_edges))
    }

    fn seek_index(&self, collection: &str, pred: &Pred, now: i64) -> Option<Vec<String>> {
        let u = crate::index::pick_index(&self.catalog, collection, pred)?;
        self.store.index_seek(collection, &u, now)
    }

    fn rows_by_ids(&self, collection: &str, ids: &[String]) -> Vec<Row> {
        let col = self.store.collection(collection);
        ids.iter()
            .filter_map(|id| {
                col.iter()
                    .find(|r| row_text(r, "id") == Some(id.as_str()))
                    .cloned()
            })
            .collect()
    }

    fn update_cas(
        &mut self,
        collection: &str,
        pred: Option<&Pred>,
        cas: Option<&str>,
        cas_each: bool,
        record: &Record,
        ctx: &mut PackCtx,
    ) -> Result<Vec<Row>, Error> {
        let now = now_ms();
        if !cas_each && cas.is_none() {
            return Err(Error::runtime("update requires cas"));
        }
        let rows = self.store.collection(collection);
        let mut idxs = Vec::new();
        for (i, row) in rows.iter().enumerate() {
            if pred.is_none_or(|p| eval_pred(p, row, now)) {
                idxs.push(i);
            }
        }
        if idxs.is_empty() {
            return Err(Error::runtime("update: no matching row"));
        }
        for i in &idxs {
            let row = &rows[*i];
            let got = row_text(row, "hash").unwrap_or("");
            if cas_each {
                cas_each_ok(collection, row, got, ctx)?;
            } else {
                let cas = cas.ok_or_else(|| Error::runtime("update requires cas"))?;
                if got != cas {
                    return Err(Error::runtime(format!(
                        "cas mismatch: expected {cas:?}, found {got:?}"
                    )));
                }
            }
        }
        let patch = record_row(record, now);
        let mut out = Vec::new();
        for i in idxs {
            let old = self.store.collection(collection)[i].clone();
            self.store.index_remove(collection, &old);
            let rows = self.store.collection_mut(collection);
            let row = &mut rows[i];
            let layer_raw = row_text(row, "layer") == Some("raw")
                || matches!(patch.get("layer"), Some(Cell::Text(s)) if s == "raw");
            if layer_raw && patch.contains_key("body") {
                let _ = self.store.index_insert(collection, &old);
                return Err(Error::runtime("immutable field: docs.body"));
            }
            for (k, v) in &patch {
                row.insert(k.clone(), v.clone());
            }
            if patch.contains_key("body")
                && let Some(body) = row_text(row, "body").map(str::to_string)
            {
                row.insert("hash".into(), Cell::Text(content_hash(&body)));
            }
            let updated = row.clone();
            self.store.index_insert(collection, &updated)?;
            out.push(updated);
        }
        mark_written(ctx, collection, &out);
        Ok(out)
    }

    fn delete_rows(
        &mut self,
        collection: &str,
        pred: &Pred,
        cas: Option<&str>,
        cas_each: bool,
        ctx: &mut PackCtx,
    ) -> Result<Vec<Row>, Error> {
        let now = now_ms();
        let append_only = self
            .catalog
            .collection(collection)
            .is_some_and(|c| c.append_only);
        if !append_only && cas.is_none() && !cas_each {
            return Err(Error::runtime("delete requires cas"));
        }
        let rows = self.store.collection(collection);
        let mut idxs = Vec::new();
        for (i, row) in rows.iter().enumerate() {
            if eval_pred(pred, row, now) {
                idxs.push(i);
            }
        }
        if idxs.is_empty() {
            return Err(Error::runtime("delete: no matching row"));
        }
        if !append_only {
            for i in &idxs {
                let got = row_text(&rows[*i], "hash").unwrap_or("");
                if cas_each {
                    cas_each_ok(collection, &rows[*i], got, ctx)?;
                } else {
                    let cas = cas.ok_or_else(|| Error::runtime("delete requires cas"))?;
                    if got != cas {
                        return Err(Error::runtime(format!(
                            "cas mismatch: expected {cas:?}, found {got:?}"
                        )));
                    }
                }
            }
        }
        idxs.sort_unstable();
        let mut out = Vec::new();
        for i in idxs.into_iter().rev() {
            let row = self.store.collection_mut(collection).remove(i);
            self.store.index_remove(collection, &row);
            out.push(row);
        }
        out.reverse();
        mark_written(ctx, collection, &out);
        Ok(out)
    }

    fn delete_edge(&mut self, rel: &str, from: &Value, to: &Value) -> Result<Row, Error> {
        let now = now_ms();
        let from = value_text(from, now);
        let to = value_text(to, now);
        let before = self.store.edges.len();
        self.store
            .edges
            .retain(|e| !(e.rel == rel && e.from == from && e.to == to));
        if self.store.edges.len() == before {
            return Err(Error::runtime("delete: edge not found"));
        }
        Ok(edge_row(rel, &from, &to))
    }

    fn exec_decl(&mut self, d: &Decl) -> Result<StmtOut, Error> {
        self.catalog.apply_decl(d)?;
        match d {
            Decl::Col {
                name,
                append,
                fields,
            } => {
                self.store.collection_mut(name);
                let pack = Pack::SchemaCol {
                    name: name.clone(),
                    append: *append,
                    fields: fields
                        .iter()
                        .map(|(n, ty)| {
                            (
                                n.clone(),
                                match ty {
                                    TypeExpr::Named(t) => t.clone(),
                                    TypeExpr::Vec { model, .. } => format!("vec@{model}"),
                                },
                            )
                        })
                        .collect(),
                };
                self.store.apply_pack(&pack);
                Ok((Vec::new(), Some(format!("col {name}")), Some(pack)))
            }
            Decl::Rel { name, stub } => {
                let pack = Pack::SchemaRel {
                    name: name.clone(),
                    stub: *stub,
                    reverse_of: None,
                };
                self.store.apply_pack(&pack);
                Ok((Vec::new(), Some(format!("rel {name}")), Some(pack)))
            }
            Decl::RelReverse { name, of } => {
                let pack = Pack::SchemaRel {
                    name: name.clone(),
                    stub: false,
                    reverse_of: Some(of.clone()),
                };
                self.store.apply_pack(&pack);
                Ok((
                    Vec::new(),
                    Some(format!("rel {name} = reverse {of}")),
                    Some(pack),
                ))
            }
            Decl::Index {
                collection,
                unique,
                fields,
            } => {
                let pack = Pack::SchemaIndex {
                    collection: collection.clone(),
                    unique: *unique,
                    fields: fields.clone(),
                };
                self.store.apply_pack(&pack);
                Ok((
                    Vec::new(),
                    Some(format!("index {}", pack_index_label(collection, fields))),
                    Some(pack),
                ))
            }
            Decl::Fk { .. } | Decl::Guard { .. } => {
                Ok((Vec::new(), Some("schema: session only".into()), None))
            }
        }
    }
}

fn cas_each_ok(collection: &str, row: &Row, got: &str, ctx: &PackCtx) -> Result<(), Error> {
    let Some(id) = row_text(row, "id") else {
        return Ok(());
    };
    let key = (collection.to_string(), id.to_string());
    if ctx.written.contains(&key) {
        return Err(Error::runtime(format!(
            "cas mismatch: row {id} changed mid-pack"
        )));
    }
    if let Some(expect) = ctx.snap.get(&key)
        && expect.as_str() != got
    {
        return Err(Error::runtime(format!(
            "cas mismatch: expected {expect:?}, found {got:?}"
        )));
    }
    Ok(())
}

fn pack_index_label(collection: &str, fields: &[String]) -> String {
    format!("{collection}[{}]", fields.join(","))
}

fn fold_packs(packs: Vec<Pack>) -> Option<Pack> {
    match packs.len() {
        0 => None,
        1 => packs.into_iter().next(),
        2.. => Some(Pack::Batch { packs }),
    }
}

fn meta_row(kind: &str, name: &str) -> Row {
    let mut r = BTreeMap::new();
    r.insert("kind".into(), Cell::Text(kind.into()));
    r.insert("name".into(), Cell::Text(name.into()));
    r
}

fn edge_row(rel: &str, from: &str, to: &str) -> Row {
    let mut r = BTreeMap::new();
    r.insert("rel".into(), Cell::Text(rel.into()));
    r.insert("from".into(), Cell::Text(from.into()));
    r.insert("to".into(), Cell::Text(to.into()));
    r
}

fn record_row(record: &Record, now: i64) -> Row {
    let mut row = BTreeMap::new();
    for (k, v) in &record.fields {
        row.insert(k.clone(), value_cell(v, now));
    }
    row
}

fn value_cell(v: &Value, now: i64) -> Cell {
    match v {
        Value::String(s) => Cell::Text(s.clone()),
        Value::Int(n) => Cell::Int(*n),
        Value::Float(n) => Cell::Float(*n),
        Value::Bool(b) => Cell::Bool(*b),
        Value::Now => Cell::Time(now),
        Value::NowMinus(d) => Cell::Time(now - d.as_millis()),
        Value::Duration(d) => Cell::Int(d.as_millis()),
        Value::Name(n) => Cell::Text(n.clone()),
    }
}

fn value_text(v: &Value, now: i64) -> String {
    match value_cell(v, now) {
        Cell::Text(s) => s,
        other => other.compact(),
    }
}

fn point_key(pred: &Pred) -> Option<(&str, &str)> {
    match pred {
        Pred::Cmp {
            field,
            op: CmpOp::Eq,
            value: Value::String(s),
        } if field.parts.len() == 1 && matches!(field.parts[0].as_str(), "id" | "uri") => {
            Some((field.parts[0].as_str(), s.as_str()))
        }
        Pred::And(a, b) => point_key(a).or_else(|| point_key(b)),
        _ => None,
    }
}

fn eval_pred(pred: &Pred, row: &Row, now: i64) -> bool {
    match pred {
        Pred::And(a, b) => eval_pred(a, row, now) && eval_pred(b, row, now),
        Pred::Or(a, b) => eval_pred(a, row, now) || eval_pred(b, row, now),
        Pred::Cmp { field, op, value } => cmp_cell(field_cell(row, field), value, *op, now),
        Pred::Has { field, ci, needle } => field_cell(row, field)
            .text()
            .is_some_and(|s| has_word(s, needle, *ci)),
        Pred::Contains { field, needle } => field_cell(row, field)
            .text()
            .is_some_and(|s| s.contains(needle.as_str())),
        Pred::Regex {
            field,
            pattern,
            flags,
        } => {
            let Some(text) = field_cell(row, field).text() else {
                return false;
            };
            let mut b = regex::RegexBuilder::new(pattern);
            if flags.contains('i') {
                b.case_insensitive(true);
            }
            b.build().is_ok_and(|re| re.is_match(text))
        }
    }
}

fn field_cell<'a>(row: &'a Row, field: &Field) -> &'a Cell {
    static NULL: Cell = Cell::Null;
    row.get(&field.as_str()).unwrap_or(&NULL)
}

fn cmp_cell(left: &Cell, value: &Value, op: CmpOp, now: i64) -> bool {
    let right = value_cell(value, now);
    match op {
        CmpOp::Eq => cells_eq(left, &right),
        CmpOp::Ne => !cells_eq(left, &right),
        CmpOp::Gt | CmpOp::Lt | CmpOp::Ge | CmpOp::Le => {
            let Some(l) = left.as_f64() else {
                return false;
            };
            let Some(r) = right.as_f64() else {
                return false;
            };
            match op {
                CmpOp::Gt => l > r,
                CmpOp::Lt => l < r,
                CmpOp::Ge => l >= r,
                CmpOp::Le => l <= r,
                _ => false,
            }
        }
    }
}

fn cells_eq(a: &Cell, b: &Cell) -> bool {
    match (a, b) {
        (Cell::Null, Cell::Null) => true,
        (Cell::Text(x), Cell::Text(y)) => x == y,
        (Cell::Bool(x), Cell::Bool(y)) => x == y,
        (Cell::Int(x), Cell::Int(y)) => x == y,
        (Cell::Float(x), Cell::Float(y)) => x == y,
        (Cell::Time(x), Cell::Time(y)) => x == y,
        (Cell::Int(x), Cell::Float(y)) => *x as f64 == *y,
        (Cell::Float(x), Cell::Int(y)) => *x == *y as f64,
        _ => false,
    }
}

fn has_word(hay: &str, needle: &str, ci: bool) -> bool {
    let tokens = |s: &str| {
        s.split(|c: char| !(c.is_alphanumeric() || c == '_'))
            .filter(|t| !t.is_empty())
            .map(|t| if ci { t.to_lowercase() } else { t.to_string() })
            .collect::<Vec<_>>()
    };
    let n = if ci {
        needle.to_lowercase()
    } else {
        needle.to_string()
    };
    tokens(hay).iter().any(|w| w == &n)
}

fn lex_score(row: &Row, query: &str) -> i64 {
    let mut blob = String::new();
    for k in ["title", "body", "snippet"] {
        if let Some(t) = row_text(row, k) {
            blob.push_str(t);
            blob.push(' ');
        }
    }
    if blob.is_empty() {
        return 0;
    }
    let blob_l = blob.to_lowercase();
    let q_l = query.to_lowercase();
    let mut score = 0i64;
    for tok in q_l.split_whitespace() {
        if blob_l.contains(tok) {
            score += 1;
            if has_word(&blob, tok, true) {
                score += 2;
            }
        }
    }
    if !q_l.is_empty() && blob_l.contains(&q_l) {
        score += 3;
    }
    score
}

fn project(rows: &[Row], fields: &[Field]) -> Vec<Row> {
    rows.iter()
        .map(|r| {
            let mut out = BTreeMap::new();
            for f in fields {
                let k = f.as_str();
                out.insert(k.clone(), r.get(&k).cloned().unwrap_or(Cell::Null));
            }
            out
        })
        .collect()
}

fn sort_rows(rows: &mut [Row], field: &Field, desc: bool) {
    let k = field.as_str();
    rows.sort_by(|a, b| {
        let av = a.get(&k).unwrap_or(&Cell::Null);
        let bv = b.get(&k).unwrap_or(&Cell::Null);
        let ord = cmp_sort(av, bv);
        if desc { ord.reverse() } else { ord }
    });
}

fn cmp_sort(a: &Cell, b: &Cell) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    match (a, b) {
        (Cell::Null, Cell::Null) => Ordering::Equal,
        (Cell::Null, _) => Ordering::Less,
        (_, Cell::Null) => Ordering::Greater,
        (Cell::Text(x), Cell::Text(y)) => x.cmp(y),
        (Cell::Bool(x), Cell::Bool(y)) => x.cmp(y),
        _ => match (a.as_f64(), b.as_f64()) {
            (Some(x), Some(y)) => x.partial_cmp(&y).unwrap_or(Ordering::Equal),
            (Some(_), None) => Ordering::Greater,
            (None, Some(_)) => Ordering::Less,
            _ => a.compact().cmp(&b.compact()),
        },
    }
}

fn agg_count(rows: &[Row], by: &Field) -> Vec<Row> {
    let k = by.as_str();
    let mut map: BTreeMap<String, i64> = BTreeMap::new();
    for r in rows {
        let key = r
            .get(&k)
            .map(Cell::compact)
            .unwrap_or_else(|| "null".into());
        *map.entry(key).or_insert(0) += 1;
    }
    map.into_iter()
        .map(|(g, n)| {
            let mut row = BTreeMap::new();
            row.insert(k.clone(), Cell::Text(unquote(&g)));
            row.insert("hits".into(), Cell::Int(n));
            row
        })
        .collect()
}

fn agg_sum(rows: &[Row], field: &Field, by: &Field) -> Vec<Row> {
    let fk = field.as_str();
    let bk = by.as_str();
    let mut map: BTreeMap<String, f64> = BTreeMap::new();
    for r in rows {
        let key = r
            .get(&bk)
            .map(Cell::compact)
            .unwrap_or_else(|| "null".into());
        let n = r.get(&fk).and_then(Cell::as_f64).unwrap_or(0.0);
        *map.entry(key).or_insert(0.0) += n;
    }
    map.into_iter()
        .map(|(g, n)| {
            let mut row = BTreeMap::new();
            row.insert(bk.clone(), Cell::Text(unquote(&g)));
            row.insert(fk.clone(), Cell::Float(n));
            row
        })
        .collect()
}

fn unquote(s: &str) -> String {
    if s.len() >= 2 && s.starts_with('"') && s.ends_with('"') {
        s[1..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
}

fn cell_key(c: &Cell) -> Option<String> {
    match c {
        Cell::Text(s) => Some(s.clone()),
        Cell::Int(n) => Some(n.to_string()),
        _ => None,
    }
}
