use std::sync::{Arc, OnceLock};

use rustc_hash::{FxHashMap, FxHashSet};
use smallvec::SmallVec;

use crate::ast::MatchHop;
use crate::error::Error;
use crate::store::row::FieldSlot;
use crate::store::{Cell, Row, RowShape, row_text};

use super::Db;

#[inline]
fn edge_shape() -> &'static Arc<RowShape> {
    static SHAPE: OnceLock<Arc<RowShape>> = OnceLock::new();
    SHAPE.get_or_init(|| Row::shape_of(&["from", "rel", "to"]))
}

#[inline]
pub(crate) fn edge_row(rel: &str, from: &str, to: &str) -> Row {
    Row::from_parts(
        Arc::clone(edge_shape()),
        [
            Cell::from_text(from),
            Cell::from_text(rel),
            Cell::from_text(to),
        ],
    )
}

/// Zero-allocation visited set for small subgraphs (`<= 16` nodes), spilling to `FxHashSet` on larger walks.
struct VisitedNodes<'a> {
    inline: SmallVec<[&'a str; 16]>,
    spilled: Option<FxHashSet<&'a str>>,
}

impl<'a> VisitedNodes<'a> {
    #[inline]
    fn new() -> Self {
        Self {
            inline: SmallVec::new(),
            spilled: None,
        }
    }

    #[inline]
    fn len(&self) -> usize {
        match &self.spilled {
            Some(set) => set.len(),
            None => self.inline.len(),
        }
    }

    #[inline]
    fn contains(&self, s: &str) -> bool {
        match &self.spilled {
            Some(set) => set.contains(s),
            None => self.inline.contains(&s),
        }
    }

    #[inline]
    fn insert(&mut self, s: &'a str) -> bool {
        if let Some(set) = &mut self.spilled {
            return set.insert(s);
        }
        if self.inline.contains(&s) {
            return false;
        }
        if self.inline.len() < 16 {
            self.inline.push(s);
            return true;
        }
        let mut set: FxHashSet<&'a str> =
            FxHashSet::with_capacity_and_hasher(32, Default::default());
        for &prev in &self.inline {
            set.insert(prev);
        }
        self.inline.clear();
        set.insert(s);
        self.spilled = Some(set);
        true
    }
}

/// Zero-allocation visited edge set (`(from, to)`) for small subgraphs, spilling to `FxHashSet` past 16 edges.
struct VisitedEdges<'a> {
    inline: SmallVec<[(&'a str, &'a str); 16]>,
    spilled: Option<FxHashSet<(&'a str, &'a str)>>,
}

impl<'a> VisitedEdges<'a> {
    #[inline]
    fn new() -> Self {
        Self {
            inline: SmallVec::new(),
            spilled: None,
        }
    }

    #[inline]
    fn insert(&mut self, edge: (&'a str, &'a str)) -> bool {
        if let Some(set) = &mut self.spilled {
            return set.insert(edge);
        }
        if self.inline.contains(&edge) {
            return false;
        }
        if self.inline.len() < 16 {
            self.inline.push(edge);
            return true;
        }
        let mut set: FxHashSet<(&'a str, &'a str)> =
            FxHashSet::with_capacity_and_hasher(32, Default::default());
        for &prev in &self.inline {
            set.insert(prev);
        }
        self.inline.clear();
        set.insert(edge);
        self.spilled = Some(set);
        true
    }
}

#[inline]
fn edges_index_synced(store: &crate::store::Store) -> bool {
    let n = store.edges.len();
    if store.edge_keys.len() != n {
        return false;
    }
    if n > 16 {
        return true;
    }
    for (i, e) in store.edges.iter().enumerate() {
        let in_adj = store
            .edges_out
            .get(e.rel.as_str())
            .and_then(|m| m.get(e.from.as_str()))
            .is_some_and(|v| v.iter().any(|dst| dst.as_ref() == e.to.as_str()));
        if !in_adj {
            return false;
        }
        for prev in &store.edges[..i] {
            if prev == e {
                return false;
            }
        }
    }
    true
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
        if rows.is_empty() || self.store.edges.is_empty() {
            return Ok(Vec::new());
        }

        let mut seeds: SmallVec<[&str; 8]> = SmallVec::new();
        let mut id_slot = FieldSlot::new("id");
        let mut uri_slot = FieldSlot::new("uri");
        for r in rows {
            if let Some(id) = id_slot.get(r).and_then(Cell::text) {
                seeds.push(id);
            }
            if let Some(uri) = uri_slot.get(r).and_then(Cell::text) {
                seeds.push(uri);
            }
        }
        self.hop_seeds(primary, &seeds, edge_rel, reverse, depth)
    }

    pub(crate) fn hop_seeds(
        &self,
        primary: &str,
        seeds: &[&str],
        edge_rel: &str,
        reverse: bool,
        depth: i64,
    ) -> Result<Vec<Row>, Error> {
        if seeds.is_empty() || self.store.edges.is_empty() {
            return Ok(Vec::new());
        }

        let synced = edges_index_synced(&self.store);
        let by_node: Option<&FxHashMap<Arc<str>, Vec<Arc<str>>>> = if synced {
            let map = if reverse {
                &self.store.edges_in
            } else {
                &self.store.edges_out
            };
            let found = map.get(edge_rel);
            if found.is_none() {
                return Ok(Vec::new());
            }
            found
        } else {
            None
        };

        let mut frontier: SmallVec<[&str; 8]> = SmallVec::new();
        let mut next: SmallVec<[&str; 8]> = SmallVec::new();
        let mut seen = VisitedNodes::new();
        for &s in seeds {
            if seen.insert(s) {
                frontier.push(s);
            }
        }
        if frontier.is_empty() {
            return Ok(Vec::new());
        }

        let mut reached: Vec<&str> = Vec::new();
        let depth = depth.clamp(1, 3);
        for _ in 0..depth {
            next.clear();
            if let Some(by_node) = by_node {
                for &src in &frontier {
                    if let Some(list) = by_node.get(src) {
                        for dst_arc in list {
                            let dst = dst_arc.as_ref();
                            if seen.insert(dst) {
                                next.push(dst);
                                reached.push(dst);
                            }
                        }
                    }
                }
            } else {
                for &src in &frontier {
                    for e in &self.store.edges {
                        if e.rel == edge_rel {
                            let dst = if reverse {
                                if e.to == src {
                                    Some(e.from.as_str())
                                } else {
                                    None
                                }
                            } else if e.from == src {
                                Some(e.to.as_str())
                            } else {
                                None
                            };
                            if let Some(dst) = dst
                                && seen.insert(dst)
                            {
                                next.push(dst);
                                reached.push(dst);
                            }
                        }
                    }
                }
            }
            if seen.len() > self.quotas.max_graph_expansion {
                return Err(Error::runtime(format!(
                    "graph expansion count {} exceeds max_graph_expansion quota {}",
                    seen.len(),
                    self.quotas.max_graph_expansion
                )));
            }
            if reached.len() >= 300 || next.is_empty() {
                break;
            }
            std::mem::swap(&mut frontier, &mut next);
        }

        if reached.is_empty() {
            return Ok(Vec::new());
        }
        reached.sort_unstable();

        let mut out = Vec::with_capacity(reached.len().min(300));
        // A cold collection resolves the reached keys with point reads on the engine
        // instead of the in-memory id maps, which would pull it into memory.
        let docs_cold = self.store.is_disk_backed("docs");
        let primary_cold = self.store.is_disk_backed(primary);
        if docs_cold || primary_cold {
            for key in reached {
                let row = if docs_cold {
                    self.store
                        .disk_point_row("docs", "id", key)
                        .or_else(|| self.store.disk_point_row("docs", "uri", key))
                } else {
                    self.store.find_doc_key(key).cloned()
                };
                let row = match row {
                    Some(row) => Some(row),
                    None if primary != "docs" => {
                        if primary_cold {
                            self.store.disk_point_row(primary, "id", key)
                        } else {
                            self.store.get_by_id(primary, key).cloned()
                        }
                    }
                    None => None,
                };
                if let Some(row) = row {
                    out.push(row);
                }
                if out.len() >= 300 {
                    break;
                }
            }
            return Ok(out);
        }

        let primary_rows = if primary != "docs" {
            self.store.collection(primary)
        } else {
            &[]
        };
        let mut prim_id_slot = FieldSlot::new("id");
        for key in reached {
            if let Some(row) = self.store.find_doc_key(key) {
                out.push(row.clone());
            } else if primary != "docs" {
                if let Some(row) = self.store.get_by_id(primary, key) {
                    out.push(row.clone());
                } else if let Some(row) = primary_rows
                    .iter()
                    .find(|r| prim_id_slot.get(r).and_then(Cell::text) == Some(key))
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
        if rows.is_empty() || self.store.edges.is_empty() {
            return Ok(Vec::new());
        }

        let mut seeds: SmallVec<[&str; 8]> = SmallVec::new();
        let mut id_slot = FieldSlot::new("id");
        let mut uri_slot = FieldSlot::new("uri");
        for r in rows {
            if let Some(id) = id_slot.get(r).and_then(Cell::text) {
                seeds.push(id);
            }
            if let Some(uri) = uri_slot.get(r).and_then(Cell::text) {
                seeds.push(uri);
            }
        }
        let rel_cell = Cell::from_text(edge_rel);
        self.graph_edges_seeds(&seeds, edge_rel, &rel_cell, reverse, depth)
    }

    pub(crate) fn graph_edges_seeds(
        &self,
        seeds: &[&str],
        edge_rel: &str,
        rel_cell: &Cell,
        reverse: bool,
        depth: i64,
    ) -> Result<Vec<Row>, Error> {
        if seeds.is_empty() || self.store.edges.is_empty() {
            return Ok(Vec::new());
        }

        let synced = edges_index_synced(&self.store);
        let by_node: Option<&FxHashMap<Arc<str>, Vec<Arc<str>>>> = if synced {
            let map = if reverse {
                &self.store.edges_in
            } else {
                &self.store.edges_out
            };
            let found = map.get(edge_rel);
            if found.is_none() {
                return Ok(Vec::new());
            }
            found
        } else {
            None
        };

        let mut frontier: SmallVec<[&str; 8]> = SmallVec::new();
        let mut next: SmallVec<[&str; 8]> = SmallVec::new();
        let mut seen_nodes = VisitedNodes::new();
        for &s in seeds {
            if seen_nodes.insert(s) {
                frontier.push(s);
            }
        }
        if frontier.is_empty() {
            return Ok(Vec::new());
        }

        let mut seen_edges = VisitedEdges::new();
        let mut out = Vec::new();
        let depth = depth.clamp(1, 3);
        let shape = edge_shape();

        for _ in 0..depth {
            next.clear();
            if let Some(by_node) = by_node {
                let mut active_src: Option<(&Arc<str>, &[Arc<str>])> = None;
                let mut multi_active = false;
                for &src in &frontier {
                    if let Some((src_arc, list)) = by_node.get_key_value(src)
                        && !list.is_empty()
                    {
                        if active_src.is_some() {
                            multi_active = true;
                            break;
                        }
                        active_src = Some((src_arc, list.as_slice()));
                    }
                }
                if !multi_active {
                    let Some((src_arc, list)) = active_src else {
                        break;
                    };
                    let src = src_arc.as_ref();
                    let src_cell = Cell::from_arc_str(src_arc);
                    for dst_arc in list {
                        let dst = dst_arc.as_ref();
                        let edge_pair = if reverse { (dst, src) } else { (src, dst) };
                        if seen_edges.insert(edge_pair) {
                            let (from_cell, to_cell) = if reverse {
                                (Cell::from_arc_str(dst_arc), src_cell.clone_for_borrow())
                            } else {
                                (src_cell.clone_for_borrow(), Cell::from_arc_str(dst_arc))
                            };
                            out.push(Row::from_parts(
                                Arc::clone(shape),
                                [from_cell, rel_cell.clone_for_borrow(), to_cell],
                            ));
                            if out.len() >= 300 {
                                return Ok(out);
                            }
                        }
                        if seen_nodes.insert(dst) {
                            next.push(dst);
                        }
                    }
                    if seen_nodes.len() > self.quotas.max_graph_expansion {
                        return Err(Error::runtime(format!(
                            "graph expansion count {} exceeds max_graph_expansion quota {}",
                            seen_nodes.len(),
                            self.quotas.max_graph_expansion
                        )));
                    }
                    if next.is_empty() {
                        break;
                    }
                    std::mem::swap(&mut frontier, &mut next);
                    continue;
                }
            }

            let mut frontier_set = VisitedNodes::new();
            for &n in &frontier {
                frontier_set.insert(n);
            }
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
                let from_s = e.from.as_str();
                let to_s = e.to.as_str();
                if seen_edges.insert((from_s, to_s)) {
                    out.push(Row::from_parts(
                        Arc::clone(shape),
                        [
                            Cell::from_text(from_s),
                            rel_cell.clone_for_borrow(),
                            Cell::from_text(to_s),
                        ],
                    ));
                    if out.len() >= 300 {
                        return Ok(out);
                    }
                }
                if seen_nodes.insert(dst) {
                    next.push(dst);
                }
            }
            if seen_nodes.len() > self.quotas.max_graph_expansion {
                return Err(Error::runtime(format!(
                    "graph expansion count {} exceeds max_graph_expansion quota {}",
                    seen_nodes.len(),
                    self.quotas.max_graph_expansion
                )));
            }
            if next.is_empty() {
                break;
            }
            std::mem::swap(&mut frontier, &mut next);
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
        let mut cur = rows;
        if let Some(alias) = start {
            for row in &mut cur {
                let snap = row.clone();
                for (k, v) in &snap {
                    super::join::insert_prefixed(row, alias, k, v.clone_for_borrow());
                }
            }
        }
        let mut prev_bind = start;
        for hop in hops {
            if cur.is_empty() {
                break;
            }
            let (edge_rel, rev_catalog) = match self.catalog.rel(&hop.rel) {
                Some(r) if let Some(of) = &r.reverse_of => (of.as_str(), true),
                _ => (hop.rel.as_str(), false),
            };
            let reverse = hop.reverse ^ rev_catalog;
            let (id_key_buf, uri_key_buf);
            let (id_key, uri_key): (&str, &str) = match prev_bind {
                Some(b) => {
                    id_key_buf = format!("{b}.id");
                    uri_key_buf = format!("{b}.uri");
                    (&id_key_buf, &uri_key_buf)
                }
                None => ("id", "uri"),
            };
            let edge_out_keys = hop.edge.as_ref().map(|eb| {
                (
                    format!("{eb}.rel"),
                    format!("{eb}.from"),
                    format!("{eb}.to"),
                )
            });
            let rel_cell = Cell::from_text(edge_rel);
            let synced = edges_index_synced(&self.store);
            let by_node = if synced {
                let map = if reverse {
                    &self.store.edges_in
                } else {
                    &self.store.edges_out
                };
                map.get(edge_rel)
            } else {
                None
            };

            let mut id_slot = FieldSlot::new(id_key);
            let mut uri_slot = FieldSlot::new(uri_key);
            let mut next_rows = Vec::new();
            for row in &cur {
                let mut keys: SmallVec<[&str; 2]> = SmallVec::new();
                if let Some(id) = id_slot.get(row).and_then(Cell::text) {
                    keys.push(id);
                }
                if let Some(uri) = uri_slot.get(row).and_then(Cell::text) {
                    keys.push(uri);
                }
                if keys.is_empty() {
                    continue;
                }
                #[cfg(feature = "gpu")]
                if self.gpu.is_some() {
                    let owned_keys: Vec<String> = keys.iter().map(|s| (*s).to_owned()).collect();
                    self.gpu_match_expand(
                        primary,
                        row,
                        hop,
                        &owned_keys,
                        edge_rel,
                        reverse,
                        &mut next_rows,
                    )?;
                    if next_rows.len() >= 300 {
                        return Ok(next_rows);
                    }
                    continue;
                }
                self.match_expand_hop_cpu(
                    primary,
                    row,
                    hop,
                    &keys,
                    edge_rel,
                    &rel_cell,
                    reverse,
                    synced,
                    by_node,
                    edge_out_keys.as_ref(),
                    &mut next_rows,
                )?;
                if next_rows.len() >= 300 {
                    return Ok(next_rows);
                }
            }
            cur = next_rows;
            prev_bind = Some(hop.bind.as_str());
        }
        Ok(cur)
    }

    #[allow(clippy::too_many_arguments)]
    fn match_expand_hop_cpu(
        &self,
        primary: &str,
        row: &Row,
        hop: &MatchHop,
        keys: &[&str],
        edge_rel: &str,
        rel_cell: &Cell,
        reverse: bool,
        synced: bool,
        by_node: Option<&FxHashMap<Arc<str>, Vec<Arc<str>>>>,
        edge_out_keys: Option<&(String, String, String)>,
        out: &mut Vec<Row>,
    ) -> Result<(), Error> {
        type VisitedPath<'a> = SmallVec<[&'a str; 8]>;
        type GraphSearchFrame<'a> = (
            &'a str,
            i64,
            VisitedPath<'a>,
            Option<(&'a str, &'a str)>,
        );
        for &start_key in keys {
            let mut stack: Vec<GraphSearchFrame<'_>> =
                vec![(start_key, 0, SmallVec::new(), None)];
            let mut expansions = 0;
            while let Some((at, depth, mut visited, last_edge)) = stack.pop() {
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
                    && let Some(node) = self.resolve_node_ref(primary, at)
                {
                    let mut merged = row.clone();
                    for (k, v) in node {
                        super::join::insert_prefixed(&mut merged, &hop.bind, k, v.clone_for_borrow());
                    }
                    if let (Some((k_rel, k_from, k_to)), Some((f, t))) = (edge_out_keys, last_edge)
                    {
                        merged.insert(k_rel.as_str(), rel_cell.clone_for_borrow());
                        merged.insert(k_from.as_str(), Cell::from_text(f));
                        merged.insert(k_to.as_str(), Cell::from_text(t));
                    }
                    out.push(merged);
                    if out.len() >= 300 {
                        return Ok(());
                    }
                }
                if depth >= hop.max_depth {
                    continue;
                }
                visited.push(at);
                if let Some(by_node) = by_node {
                    if let Some(list) = by_node.get(at) {
                        for dst_arc in list {
                            let dst = dst_arc.as_ref();
                            if visited.contains(&dst) {
                                continue;
                            }
                            let (from, to) = if reverse { (dst, at) } else { (at, dst) };
                            stack.push((dst, depth + 1, visited.clone(), Some((from, to))));
                        }
                    }
                } else if !synced || self.store.edges.len() <= 16 {
                    for e in &self.store.edges {
                        if e.rel == edge_rel {
                            let dst = if reverse {
                                (e.to == at).then_some(e.from.as_str())
                            } else {
                                (e.from == at).then_some(e.to.as_str())
                            };
                            if let Some(dst) = dst {
                                if visited.contains(&dst) {
                                    continue;
                                }
                                let (from, to) = if reverse { (dst, at) } else { (at, dst) };
                                stack.push((dst, depth + 1, visited.clone(), Some((from, to))));
                            }
                        }
                    }
                }
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
