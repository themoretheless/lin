//! Graph edge storage, zero-allocation nested adjacency indexes, and SPO fact triple indexing.

use std::collections::BTreeMap;
use std::sync::{Arc, OnceLock};

use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};

use super::Store;
use super::value::{Cell, Row, spo_key};

#[inline]
pub(super) fn facts_ops_shape() -> &'static Arc<super::row::Shape> {
    static SHAPE: OnceLock<Arc<super::row::Shape>> = OnceLock::new();
    SHAPE.get_or_init(|| Row::shape_of(&["o", "p", "s"]))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Edge {
    pub rel: String,
    pub from: String,
    pub to: String,
}

pub(super) type FactKey = (Arc<str>, Arc<str>, Arc<str>);
pub(crate) type AdjacencyMap = FxHashMap<Arc<str>, FxHashMap<Arc<str>, Vec<Arc<str>>>>;

#[inline]
fn intern_node_arc(
    node: &str,
    rel_out: Option<&FxHashMap<Arc<str>, Vec<Arc<str>>>>,
    rel_in: Option<&FxHashMap<Arc<str>, Vec<Arc<str>>>>,
    by_id: &BTreeMap<String, FxHashMap<Arc<str>, usize>>,
) -> Arc<str> {
    if let Some(m) = rel_out
        && let Some((k, _)) = m.get_key_value(node)
    {
        return Arc::clone(k);
    }
    if let Some(m) = rel_in
        && let Some((k, _)) = m.get_key_value(node)
    {
        return Arc::clone(k);
    }
    for col_map in by_id.values() {
        if let Some((k, _)) = col_map.get_key_value(node) {
            return Arc::clone(k);
        }
    }
    Arc::<str>::from(node)
}

#[inline]
fn spo_contains(
    map: &FxHashMap<FactKey, usize>,
    s: &Arc<str>,
    p: &Arc<str>,
    o: &Arc<str>,
) -> Option<usize> {
    if map.is_empty() {
        return None;
    }
    // SAFETY: `probe` is wrapped in `ManuallyDrop` (never dropped) and only borrowed
    // as `&FactKey` for the duration of `map.get` while `s`, `p`, `o` remain live.
    let probe = std::mem::ManuallyDrop::new(unsafe {
        (std::ptr::read(s), std::ptr::read(p), std::ptr::read(o))
    });
    map.get(&*probe).copied()
}

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
            let rel_out = self.edges_out.get(e.rel.as_str());
            let rel_in = self.edges_in.get(e.rel.as_str());
            let from_arc = intern_node_arc(e.from.as_str(), rel_out, rel_in, &self.by_id);
            let to_arc = if e.from == e.to {
                Arc::clone(&from_arc)
            } else {
                intern_node_arc(e.to.as_str(), rel_out, rel_in, &self.by_id)
            };
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

    /// Idempotent fact insert without constructing/cloning a return `Row` (`Pack::AppendFact`).
    pub(crate) fn append_fact_row_no_ret(&mut self, row: &Row) -> bool {
        if let (Some(Cell::Text(s)), Some(Cell::Text(p)), Some(Cell::Text(o))) =
            (row.get_str("s"), row.get_str("p"), row.get_str("o"))
            && spo_contains(&self.facts_by_spo, s, p, o).is_some()
        {
            return false;
        }
        let Some(key) = spo_key(row) else {
            self.ensure_facts_rows();
            self.collection_mut("facts").push(row.clone());
            self.update_col_count("facts", 1);
            return true;
        };
        if self.facts_by_spo.contains_key(&key) {
            return false;
        }
        let sa = Arc::clone(&key.0);
        let pa = Arc::clone(&key.1);
        let oa = Arc::clone(&key.2);
        let idx = self.facts_s.len();
        self.facts_by_spo.insert(key, idx);
        self.facts_s.push(Arc::clone(&sa));
        self.facts_p.push(Arc::clone(&pa));
        self.facts_o.push(Arc::clone(&oa));
        if self.is_disk_backed("facts") {
            self.facts_rows_stale = true;
        } else if self.facts_rows_stale {
            self.ensure_facts_rows();
        } else {
            let facts = self.collections.entry("facts".into()).or_default();
            if facts.len() == idx {
                facts.push(Row::from_parts(
                    Arc::clone(facts_ops_shape()),
                    [Cell::Text(oa), Cell::Text(pa), Cell::Text(sa)],
                ));
            } else {
                self.facts_rows_stale = true;
                self.ensure_facts_rows();
            }
        }
        self.update_col_count("facts", 1);
        true
    }

    /// Idempotent fact insert. Returns `(row, changed)`.
    pub fn append_fact_row(&mut self, row: &Row) -> (Row, bool) {
        if let (Some(Cell::Text(s)), Some(Cell::Text(p)), Some(Cell::Text(o))) =
            (row.get_str("s"), row.get_str("p"), row.get_str("o"))
            && let Some(idx) = spo_contains(&self.facts_by_spo, s, p, o)
        {
            self.ensure_facts_rows();
            return (self.collection("facts")[idx].clone(), false);
        }
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
        let built = Row::from_parts(
            Arc::clone(facts_ops_shape()),
            [
                Cell::Text(oa),
                Cell::Text(pa),
                Cell::Text(sa),
            ],
        );
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
        let shape = facts_ops_shape();
        let mut changed = 0usize;
        let first_new = self.facts_s.len();
        for i in 0..n {
            if spo_contains(&self.facts_by_spo, &s[i], &p[i], &o[i]).is_some() {
                if want_rows {
                    rows.push(Row::from_parts(
                        Arc::clone(shape),
                        [
                            Cell::Text(Arc::clone(&o[i])),
                            Cell::Text(Arc::clone(&p[i])),
                            Cell::Text(Arc::clone(&s[i])),
                        ],
                    ));
                }
                continue;
            }
            let idx = self.facts_s.len();
            let sa = Arc::clone(&s[i]);
            let pa = Arc::clone(&p[i]);
            let oa = Arc::clone(&o[i]);
            self.facts_by_spo
                .insert((Arc::clone(&sa), Arc::clone(&pa), Arc::clone(&oa)), idx);
            self.facts_s.push(sa);
            self.facts_p.push(pa);
            self.facts_o.push(oa);
            changed += 1;
            if want_rows {
                rows.push(Row::from_parts(
                    Arc::clone(shape),
                    [
                        Cell::Text(Arc::clone(&self.facts_o[idx])),
                        Cell::Text(Arc::clone(&self.facts_p[idx])),
                        Cell::Text(Arc::clone(&self.facts_s[idx])),
                    ],
                ));
            }
        }
        if changed > 0 {
            self.update_col_count("facts", changed as isize);
            // Readers go through `collections["facts"]` and cannot materialize it
            // themselves, and a replayed delete looks its rows up there: keep the rows in
            // step with the columns instead of leaving them stale until some later write.
            if self.is_disk_backed("facts") {
                self.facts_rows_stale = true;
            } else {
                let in_step = !self.facts_rows_stale
                    && self.collections.get("facts").map_or(0, Vec::len) == first_new;
                if in_step {
                    // One shared shape: each row is an `Arc` clone plus three `Cell::Text(Arc::clone)` cells.
                    let facts = self.collections.entry("facts".into()).or_default();
                    facts.reserve(changed);
                    for idx in first_new..self.facts_s.len() {
                        facts.push(Row::from_parts(
                            Arc::clone(shape),
                            [
                                Cell::Text(Arc::clone(&self.facts_o[idx])),
                                Cell::Text(Arc::clone(&self.facts_p[idx])),
                                Cell::Text(Arc::clone(&self.facts_s[idx])),
                            ],
                        ));
                    }
                } else {
                    self.facts_rows_stale = true;
                    self.ensure_facts_rows();
                }
            }
        }
        (rows, changed)
    }

    pub(crate) fn append_facts_spo_bulk_owned(
        &mut self,
        mut s: Vec<Arc<str>>,
        mut p: Vec<Arc<str>>,
        mut o: Vec<Arc<str>>,
    ) {
        let n = s.len().min(p.len()).min(o.len());
        if n == 0 {
            return;
        }
        s.truncate(n);
        p.truncate(n);
        o.truncate(n);
        if self.facts_s.is_empty() && !self.facts_rows_stale {
            self.facts_by_spo.reserve(n);
            let shape = facts_ops_shape();
            let mut write = 0usize;
            for i in 0..n {
                if spo_contains(&self.facts_by_spo, &s[i], &p[i], &o[i]).is_some() {
                    continue;
                }
                self.facts_by_spo.insert(
                    (Arc::clone(&s[i]), Arc::clone(&p[i]), Arc::clone(&o[i])),
                    write,
                );
                if write != i {
                    s[write] = Arc::clone(&s[i]);
                    p[write] = Arc::clone(&p[i]);
                    o[write] = Arc::clone(&o[i]);
                }
                write += 1;
            }
            s.truncate(write);
            p.truncate(write);
            o.truncate(write);
            if write > 0 {
                self.update_col_count("facts", write as isize);
                if self.is_disk_backed("facts") {
                    self.facts_rows_stale = true;
                } else if self.collections.get("facts").map_or(0, Vec::len) == 0 {
                    let mut facts_vec = Vec::with_capacity(write);
                    for idx in 0..write {
                        facts_vec.push(Row::from_parts(
                            Arc::clone(shape),
                            [
                                Cell::Text(Arc::clone(&o[idx])),
                                Cell::Text(Arc::clone(&p[idx])),
                                Cell::Text(Arc::clone(&s[idx])),
                            ],
                        ));
                    }
                    self.collections.insert("facts".into(), facts_vec);
                } else {
                    self.facts_s = s;
                    self.facts_p = p;
                    self.facts_o = o;
                    self.facts_rows_stale = true;
                    self.ensure_facts_rows();
                    return;
                }
                self.facts_s = s;
                self.facts_p = p;
                self.facts_o = o;
            }
            return;
        }
        let _ = self.append_facts_spo_bulk(&s, &p, &o, false);
    }

    pub(crate) fn append_prepared_facts(
        &mut self,
        s: &[Arc<str>],
        p: &[Arc<str>],
        o: &[Arc<str>],
    ) -> usize {
        self.append_facts_spo_bulk(s, p, o, false).1
    }

    pub fn append_edge_parts(&mut self, rel: &str, from: &str, to: &str) -> bool {
        if let Some(by_node) = self.edges_out.get(rel)
            && let Some(list) = by_node.get(from)
            && list.len() <= 16
            && list.iter().any(|x| x.as_ref() == to)
        {
            return false;
        }
        let rel_arc = self
            .edges_out
            .get_key_value(rel)
            .map(|(k, _)| Arc::clone(k))
            .unwrap_or_else(|| Arc::<str>::from(rel));
        let rel_out = self.edges_out.get(rel);
        let rel_in = self.edges_in.get(rel);
        let from_arc = intern_node_arc(from, rel_out, rel_in, &self.by_id);
        let to_arc = if from == to {
            Arc::clone(&from_arc)
        } else {
            intern_node_arc(to, rel_out, rel_in, &self.by_id)
        };
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
        let removed = if let Some((rel_arc, out_by_node)) = self.edges_out.get_key_value(rel)
            && let Some((from_arc, _)) = out_by_node.get_key_value(from)
            && let Some((to_arc, _)) = self.edges_in.get(rel).and_then(|m| m.get_key_value(to))
        {
            let probe = std::mem::ManuallyDrop::new(unsafe {
                (
                    std::ptr::read(rel_arc),
                    std::ptr::read(from_arc),
                    std::ptr::read(to_arc),
                )
            });
            self.edge_keys.remove(&*probe)
        } else if self.edge_keys.len() != self.edges.len() {
            let before = self.edges.len();
            self.edges
                .retain(|e| !(e.rel == rel && e.from == from && e.to == to));
            if self.edges.len() != before {
                self.rebuild_edge_keys();
                return true;
            }
            return false;
        } else {
            false
        };
        if !removed {
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
