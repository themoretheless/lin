//! Graph edge storage, zero-allocation nested adjacency indexes, and SPO fact triple indexing.

use std::collections::BTreeMap;
use std::sync::Arc;

use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};

use super::Store;
use super::value::{Cell, Row, spo_key};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Edge {
    pub rel: String,
    pub from: String,
    pub to: String,
}

pub(super) type FactKey = (Arc<str>, Arc<str>, Arc<str>);
pub(crate) type AdjacencyMap = FxHashMap<Arc<str>, FxHashMap<Arc<str>, Vec<Arc<str>>>>;

impl Store {
    pub(super) fn rebuild_edge_keys(&mut self) {
        self.edge_keys.clear();
        self.edges_out.clear();
        self.edges_in.clear();
        self.edge_keys.reserve(self.edges.len());
        for e in &self.edges {
            let rel_arc = self
                .edges_out
                .get_key_value(e.rel.as_str())
                .map(|(k, _)| Arc::clone(k))
                .unwrap_or_else(|| Arc::<str>::from(e.rel.as_str()));
            let from_arc = Arc::<str>::from(e.from.as_str());
            let to_arc = Arc::<str>::from(e.to.as_str());
            self.edge_keys.insert((
                Arc::clone(&rel_arc),
                Arc::clone(&from_arc),
                Arc::clone(&to_arc),
            ));
            self.edges_out
                .entry(Arc::clone(&rel_arc))
                .or_default()
                .entry(Arc::clone(&from_arc))
                .or_default()
                .push(Arc::clone(&to_arc));
            self.edges_in
                .entry(rel_arc)
                .or_default()
                .entry(to_arc)
                .or_default()
                .push(from_arc);
        }
    }

    /// Invoke closure for each outgoing or incoming neighbor key with zero heap/Arc allocations!
    #[inline]
    pub fn for_each_neighbor(&self, rel: &str, node: &str, reverse: bool, mut f: impl FnMut(&str)) {
        let synced = self.edge_keys.len() == self.edges.len();
        if synced {
            let map = if reverse {
                &self.edges_in
            } else {
                &self.edges_out
            };
            if let Some(by_node) = map.get(rel)
                && let Some(list) = by_node.get(node)
            {
                for dst in list {
                    f(dst.as_ref());
                }
                return;
            }
            if self.edges.len() > 16 {
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

    /// Invoke closure for each outgoing or incoming neighbor `Arc<str>` key without string/tuple allocations.
    #[inline]
    pub fn for_each_neighbor_arc(
        &self,
        rel: &Arc<str>,
        node: &Arc<str>,
        reverse: bool,
        mut f: impl FnMut(&Arc<str>),
    ) {
        let synced = self.edge_keys.len() == self.edges.len();
        if synced {
            let map = if reverse {
                &self.edges_in
            } else {
                &self.edges_out
            };
            if let Some(by_node) = map.get(rel.as_ref())
                && let Some(list) = by_node.get(node.as_ref())
            {
                for dst in list {
                    f(dst);
                }
                return;
            }
            if self.edges.len() > 16 {
                return;
            }
        }
        for e in &self.edges {
            if e.rel == rel.as_ref() {
                if reverse {
                    if e.to == node.as_ref() {
                        let dst_arc = Arc::<str>::from(e.from.as_str());
                        f(&dst_arc);
                    }
                } else if e.from == node.as_ref() {
                    let dst_arc = Arc::<str>::from(e.to.as_str());
                    f(&dst_arc);
                }
            }
        }
    }

    /// Outgoing or incoming neighbor keys for `(rel, node)` in O(1) borrowed lookup.
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
        let map = if reverse {
            &self.edges_in
        } else {
            &self.edges_out
        };
        map.get(rel)
            .and_then(|by_node| by_node.get(node))
            .cloned()
            .unwrap_or_default()
    }

    /// Idempotent fact insert. Returns `(row, changed)`.
    pub fn append_fact_row(&mut self, row: &Row) -> (Row, bool) {
        let Some(key) = spo_key(row) else {
            self.ensure_facts_rows();
            self.collection_mut("facts").push(row.clone());
            self.update_col_count("facts", 1);
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
        self.facts_s.push(Arc::clone(&sa));
        self.facts_p.push(Arc::clone(&pa));
        self.facts_o.push(Arc::clone(&oa));
        let mut built = BTreeMap::new();
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
    pub fn append_facts_spo_bulk(
        &mut self,
        s: &[Arc<str>],
        p: &[Arc<str>],
        o: &[Arc<str>],
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
            let key = (Arc::clone(&s[i]), Arc::clone(&p[i]), Arc::clone(&o[i]));
            match self.facts_by_spo.entry(key) {
                std::collections::hash_map::Entry::Occupied(_) => {
                    if want_rows {
                        let mut row = BTreeMap::new();
                        row.insert("s".into(), Cell::Text(Arc::clone(&s[i])));
                        row.insert("p".into(), Cell::Text(Arc::clone(&p[i])));
                        row.insert("o".into(), Cell::Text(Arc::clone(&o[i])));
                        rows.push(row);
                    }
                }
                std::collections::hash_map::Entry::Vacant(e) => {
                    let idx = self.facts_s.len();
                    e.insert(idx);
                    self.facts_s.push(Arc::clone(&s[i]));
                    self.facts_p.push(Arc::clone(&p[i]));
                    self.facts_o.push(Arc::clone(&o[i]));
                    self.facts_rows_stale = true;
                    changed += 1;
                    if want_rows {
                        let mut row = BTreeMap::new();
                        row.insert("s".into(), Cell::Text(Arc::clone(&self.facts_s[idx])));
                        row.insert("p".into(), Cell::Text(Arc::clone(&self.facts_p[idx])));
                        row.insert("o".into(), Cell::Text(Arc::clone(&self.facts_o[idx])));
                        rows.push(row);
                    }
                }
            }
        }
        if changed > 0 {
            self.update_col_count("facts", changed as isize);
        }
        if want_rows && changed > 0 {
            self.ensure_facts_rows();
        }
        (rows, changed)
    }

    pub fn append_edge_parts(&mut self, rel: &str, from: &str, to: &str) -> bool {
        let rel_arc = self
            .edges_out
            .get_key_value(rel)
            .map(|(k, _)| Arc::clone(k))
            .unwrap_or_else(|| Arc::<str>::from(rel));
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
            .entry(Arc::clone(&rel_arc))
            .or_default()
            .entry(Arc::clone(&from_arc))
            .or_default()
            .push(Arc::clone(&to_arc));
        self.edges_in
            .entry(rel_arc)
            .or_default()
            .entry(to_arc)
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
        if self.edge_keys.len() == self.edges.len() {
            return self
                .edges_out
                .get(rel)
                .and_then(|by_node| by_node.get(from))
                .is_some_and(|list| list.iter().any(|x| x.as_ref() == to));
        }
        self.edges
            .iter()
            .any(|e| e.rel == rel && e.from == from && e.to == to)
    }

    pub fn remove_edge(&mut self, rel: &str, from: &str, to: &str) -> bool {
        let rel_arc = Arc::<str>::from(rel);
        let from_arc = Arc::<str>::from(from);
        let to_arc = Arc::<str>::from(to);
        let key = (rel_arc, from_arc, to_arc);
        if !self.edge_keys.remove(&key) {
            return false;
        }
        if let Some(by_node) = self.edges_out.get_mut(rel)
            && let Some(list) = by_node.get_mut(from)
        {
            list.retain(|x| x.as_ref() != to);
        }
        if let Some(by_node) = self.edges_in.get_mut(rel)
            && let Some(list) = by_node.get_mut(to)
        {
            list.retain(|x| x.as_ref() != from);
        }
        self.edges
            .retain(|e| !(e.rel == rel && e.from == from && e.to == to));
        true
    }
}
