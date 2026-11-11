use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use rustc_hash::FxHashSet;

use crate::ast::MatchHop;
use crate::error::Error;
use crate::store::{Cell, Row, row_text};

use super::Db;

pub(crate) fn edge_row(rel: &str, from: &str, to: &str) -> Row {
    let mut r = BTreeMap::new();
    r.insert("rel".into(), Cell::text_arc(rel));
    r.insert("from".into(), Cell::text_arc(from));
    r.insert("to".into(), Cell::text_arc(to));
    r
}

fn match_frontier_keys(row: &Row, bind: Option<&str>) -> Vec<String> {
    let mut keys = Vec::new();
    match bind {
        Some(b) => {
            let id_k = format!("{b}.id");
            let uri_k = format!("{b}.uri");
            if let Some(id) = row_text(row, &id_k) {
                keys.push(id.to_string());
            }
            if let Some(uri) = row_text(row, &uri_k) {
                keys.push(uri.to_string());
            }
        }
        None => {
            if let Some(id) = row_text(row, "id") {
                keys.push(id.to_string());
            }
            if let Some(uri) = row_text(row, "uri") {
                keys.push(uri.to_string());
            }
        }
    }
    keys
}

impl Db {
    pub(super) fn hop(
        &self,
        primary: &str,
        rows: &[Row],
        rel: &str,
        depth: i64,
    ) -> Result<Vec<Row>, Error> {
        let (edge_rel, reverse) = match self.catalog.rel(rel) {
            Some(r) if let Some(of) = &r.reverse_of => (of.as_str(), true),
            _ => (rel, false),
        };
        #[cfg(feature = "gpu")]
        if self.gpu.is_some() {
            return self.gpu_walk_rows(primary, rows, edge_rel, reverse, depth, false);
        }
        let mut frontier: Vec<Arc<str>> = Vec::with_capacity(rows.len() * 2);
        let mut seen: FxHashSet<Arc<str>> = FxHashSet::default();
        for r in rows {
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
        let rel_arc: Arc<str> = Arc::from(edge_rel);
        let mut reached: BTreeSet<Arc<str>> = BTreeSet::new();
        let depth = depth.clamp(1, 3);
        for _ in 0..depth {
            let mut next: Vec<Arc<str>> = Vec::new();
            for src in &frontier {
                self.store
                    .for_each_neighbor_arc(&rel_arc, src, reverse, |dst| {
                        if seen.insert(Arc::clone(dst)) {
                            next.push(Arc::clone(dst));
                            reached.insert(Arc::clone(dst));
                        }
                    });
            }
            if seen.len() > self.quotas.max_graph_expansion {
                return Err(Error::runtime(format!(
                    "graph expansion count {} exceeds max_graph_expansion quota {}",
                    seen.len(),
                    self.quotas.max_graph_expansion
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
        let mut out = Vec::with_capacity(reached.len().min(300));
        for key in reached {
            if let Some(row) = self.store.find_doc_key(&key) {
                out.push(row.clone());
            } else if primary != "docs" {
                if let Some(row) = self.store.get_by_id(primary, &key) {
                    out.push(row.clone());
                } else if let Some(row) = self
                    .store
                    .collection(primary)
                    .iter()
                    .find(|r| row_text(r, "id") == Some(key.as_ref()))
                {
                    out.push(row.clone());
                }
            }
            if out.len() >= 300 {
                break;
            }
        }
        Ok(out)
    }

    /// Subgraph edges reachable from `rows` via `rel` (same walk as [`Self::hop`]).
    pub(super) fn graph_edges(
        &self,
        rows: &[Row],
        rel: &str,
        depth: i64,
    ) -> Result<Vec<Row>, Error> {
        let (edge_rel, reverse) = match self.catalog.rel(rel) {
            Some(r) if let Some(of) = &r.reverse_of => (of.as_str(), true),
            _ => (rel, false),
        };
        #[cfg(feature = "gpu")]
        if self.gpu.is_some() {
            return self.gpu_walk_rows("docs", rows, edge_rel, reverse, depth, true);
        }
        let mut frontier: Vec<Arc<str>> = Vec::with_capacity(rows.len() * 2);
        let mut frontier_set: FxHashSet<Arc<str>> = FxHashSet::default();
        for r in rows {
            if let Some(id) = r.get("id").and_then(Cell::text_shared)
                && frontier_set.insert(Arc::clone(&id))
            {
                frontier.push(id);
            }
            if let Some(uri) = r.get("uri").and_then(Cell::text_shared)
                && frontier_set.insert(Arc::clone(&uri))
            {
                frontier.push(uri);
            }
        }
        let mut seen_nodes = frontier_set.clone();
        let mut seen_edges: FxHashSet<(Arc<str>, Arc<str>)> = FxHashSet::default();
        let mut out = Vec::new();
        let depth = depth.clamp(1, 3);
        let rel_arc: Arc<str> = Arc::from(edge_rel);
        let synced = self.store.edge_keys.len() == self.store.edges.len();

        for _ in 0..depth {
            if frontier.len() <= 2 && synced {
                let mut next: Vec<Arc<str>> = Vec::new();
                for src in &frontier {
                    self.store
                        .for_each_neighbor_arc(&rel_arc, src, reverse, |dst| {
                            let (from, to) = if reverse {
                                (Arc::clone(dst), Arc::clone(src))
                            } else {
                                (Arc::clone(src), Arc::clone(dst))
                            };
                            if seen_edges.insert((Arc::clone(&from), Arc::clone(&to)))
                                && out.len() < 300
                            {
                                let mut r = BTreeMap::new();
                                r.insert("rel".into(), Cell::Text(Arc::clone(&rel_arc)));
                                r.insert("from".into(), Cell::Text(from));
                                r.insert("to".into(), Cell::Text(to));
                                out.push(r);
                            }
                            if seen_nodes.insert(Arc::clone(dst)) {
                                next.push(Arc::clone(dst));
                            }
                        });
                    if out.len() >= 300 {
                        return Ok(out);
                    }
                }
                if seen_nodes.len() > self.quotas.max_graph_expansion {
                    return Err(Error::runtime(format!(
                        "graph expansion count {} exceeds max_graph_expansion quota {}",
                        seen_nodes.len(),
                        self.quotas.max_graph_expansion
                    )));
                }
                frontier = next;
                if frontier.is_empty() {
                    break;
                }
                frontier_set.clear();
                for n in &frontier {
                    frontier_set.insert(Arc::clone(n));
                }
                continue;
            }

            let mut next: Vec<Arc<str>> = Vec::new();
            let mut next_set: FxHashSet<Arc<str>> = FxHashSet::default();
            for e in &self.store.edges {
                if e.rel != edge_rel {
                    continue;
                }
                let (src, dst) = if reverse {
                    (e.to.as_str(), e.from.as_str())
                } else {
                    (e.from.as_str(), e.to.as_str())
                };
                if !frontier_set.contains(src) {
                    continue;
                }
                let from_arc: Arc<str> = Arc::from(e.from.as_str());
                let to_arc: Arc<str> = Arc::from(e.to.as_str());
                if seen_edges.insert((Arc::clone(&from_arc), Arc::clone(&to_arc))) {
                    let mut r = BTreeMap::new();
                    r.insert("rel".into(), Cell::Text(Arc::clone(&rel_arc)));
                    r.insert("from".into(), Cell::Text(from_arc));
                    r.insert("to".into(), Cell::Text(to_arc));
                    out.push(r);
                    if out.len() >= 300 {
                        return Ok(out);
                    }
                }
                if !seen_nodes.contains(dst) {
                    let dst_arc: Arc<str> = Arc::from(dst);
                    seen_nodes.insert(Arc::clone(&dst_arc));
                    if next_set.insert(Arc::clone(&dst_arc)) {
                        next.push(dst_arc);
                    }
                }
            }
            if seen_nodes.len() > self.quotas.max_graph_expansion {
                return Err(Error::runtime(format!(
                    "graph expansion count {} exceeds max_graph_expansion quota {}",
                    seen_nodes.len(),
                    self.quotas.max_graph_expansion
                )));
            }
            frontier = next;
            frontier_set = next_set;
            if frontier.is_empty() {
                break;
            }
        }
        Ok(out)
    }

    /// Expand `match` hops; bound nodes as `bind.field`, optional edge as `e.rel|from|to`.
    pub(super) fn match_path(
        &self,
        primary: &str,
        rows: Vec<Row>,
        start: Option<&str>,
        hops: &[MatchHop],
    ) -> Result<Vec<Row>, Error> {
        let mut cur = Vec::with_capacity(rows.len());
        for row in rows {
            let mut r = row.clone();
            if let Some(alias) = start {
                for (k, v) in &row {
                    r.insert(format!("{alias}.{k}"), v.clone());
                }
            }
            cur.push(r);
        }
        let mut prev_bind = start;
        for hop in hops {
            let mut next_rows = Vec::new();
            for row in &cur {
                self.match_expand_hop(primary, row, prev_bind, hop, &mut next_rows)?;
                if next_rows.len() >= 300 {
                    return Ok(next_rows);
                }
            }
            cur = next_rows;
            prev_bind = Some(hop.bind.as_str());
            if cur.is_empty() {
                break;
            }
        }
        Ok(cur)
    }

    fn match_expand_hop(
        &self,
        primary: &str,
        row: &Row,
        prev_bind: Option<&str>,
        hop: &MatchHop,
        out: &mut Vec<Row>,
    ) -> Result<(), Error> {
        let (edge_rel, rev_catalog) = match self.catalog.rel(&hop.rel) {
            Some(r) if let Some(of) = &r.reverse_of => (of.as_str(), true),
            _ => (hop.rel.as_str(), false),
        };
        let reverse = hop.reverse ^ rev_catalog;
        let keys = match_frontier_keys(row, prev_bind);
        if keys.is_empty() {
            return Ok(());
        }
        #[cfg(feature = "gpu")]
        if self.gpu.is_some() {
            return self.gpu_match_expand(primary, row, hop, &keys, edge_rel, reverse, out);
        }
        let rel_arc: Arc<str> = Arc::from(edge_rel);
        for start_key in keys {
            type GraphSearchFrame = (
                Arc<str>,
                i64,
                BTreeSet<Arc<str>>,
                Option<(Arc<str>, Arc<str>, Arc<str>)>,
            );
            let mut stack: Vec<GraphSearchFrame> =
                vec![(Arc::from(start_key.as_str()), 0, BTreeSet::new(), None)];
            let mut expansions = 0;
            while let Some((at, depth, path, last_edge)) = stack.pop() {
                expansions += 1;
                if expansions > self.quotas.max_graph_expansion {
                    return Err(Error::runtime(format!(
                        "graph expansion count {} exceeds max_graph_expansion quota {}",
                        expansions, self.quotas.max_graph_expansion
                    )));
                }
                if depth > 0
                    && depth >= hop.min_depth
                    && depth <= hop.max_depth
                    && let Some(node) = self.resolve_node_ref(primary, &at)
                {
                    let mut merged = row.clone();
                    for (k, v) in node {
                        merged.insert(format!("{}.{k}", hop.bind), v.clone());
                    }
                    if let (Some(e_bind), Some((r, f, t))) = (&hop.edge, &last_edge) {
                        merged.insert(format!("{e_bind}.rel"), Cell::Text(Arc::clone(r)));
                        merged.insert(format!("{e_bind}.from"), Cell::Text(Arc::clone(f)));
                        merged.insert(format!("{e_bind}.to"), Cell::Text(Arc::clone(t)));
                    }
                    out.push(merged);
                    if out.len() >= 300 {
                        return Ok(());
                    }
                }
                if depth >= hop.max_depth {
                    continue;
                }
                let mut visited = path;
                visited.insert(Arc::clone(&at));
                self.store
                    .for_each_neighbor_arc(&rel_arc, &at, reverse, |dst| {
                        if visited.contains(dst) {
                            return;
                        }
                        let (from, to) = if reverse {
                            (Arc::clone(dst), Arc::clone(&at))
                        } else {
                            (Arc::clone(&at), Arc::clone(dst))
                        };
                        stack.push((
                            Arc::clone(dst),
                            depth + 1,
                            visited.clone(),
                            Some((Arc::clone(&rel_arc), from, to)),
                        ));
                    });
            }
        }
        Ok(())
    }

    pub(super) fn resolve_node_ref(&self, primary: &str, key: &str) -> Option<&Row> {
        if let Some(row) = self.store.find_doc_key(key) {
            return Some(row);
        }
        if primary != "docs" {
            if let Some(row) = self.store.get_by_id(primary, key) {
                return Some(row);
            }
            return self
                .store
                .collection(primary)
                .iter()
                .find(|r| row_text(r, "id") == Some(key));
        }
        None
    }
}
