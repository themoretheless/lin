use std::collections::BTreeMap;
use std::ops::Bound;
use std::sync::Arc;

use rustc_hash::FxHashMap;

use crate::ast::{CmpOp, Pred, Value};
use crate::catalog::{Catalog, IndexDef};
use crate::store::{Cell, Row};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum IndexPart {
    Min,
    Null,
    Bool(bool),
    Int(i64),
    Time(i64),
    Text(Arc<str>),
    Max,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct IndexKey(pub Vec<IndexPart>);

/// Live secondary index: BTree over composite keys → row indices in the collection vec.
#[derive(Debug, Clone)]
pub struct LiveIndex {
    pub def: IndexDef,
    pub forward: BTreeMap<IndexKey, Vec<usize>>,
    pub reverse: FxHashMap<usize, IndexKey>,
}

#[derive(Debug, Clone)]
pub struct IndexUse {
    pub def: IndexDef,
    pub eqs: Vec<(String, Value)>,
    pub range: Option<(String, CmpOp, Value)>,
}

impl LiveIndex {
    pub fn new(def: IndexDef) -> Self {
        Self {
            def,
            forward: BTreeMap::new(),
            reverse: FxHashMap::default(),
        }
    }

    pub fn key_of(&self, row: &Row) -> IndexKey {
        let parts = self
            .def
            .fields
            .iter()
            .map(|f| cell_part(row.get(f).unwrap_or(&Cell::Null)))
            .collect();
        IndexKey(parts)
    }

    pub fn insert_at(&mut self, idx: usize, row: &Row) -> Result<(), String> {
        let key = self.key_of(row);
        self.insert_key(idx, key)
    }

    /// Append-only bulk path: no reverse-key replace (fresh indices).
    pub fn insert_at_new(&mut self, idx: usize, row: &Row) -> Result<(), String> {
        let key = self.key_of(row);
        if self.def.unique
            && self.forward.contains_key(&key)
        {
            return Err(format!("unique index {}: duplicate key", self.def.label()));
        }
        self.reverse.insert(idx, key.clone());
        self.forward.entry(key).or_default().push(idx);
        Ok(())
    }

    fn insert_key(&mut self, idx: usize, key: IndexKey) -> Result<(), String> {
        if self.def.unique
            && let Some(ids) = self.forward.get(&key)
            && ids.iter().any(|&x| x != idx)
        {
            return Err(format!("unique index {}: duplicate key", self.def.label()));
        }
        if let Some(old) = self.reverse.insert(idx, key.clone())
            && let Some(vec) = self.forward.get_mut(&old)
        {
            if let Some(p) = vec.iter().position(|&x| x == idx) {
                vec.swap_remove(p);
            }
            if vec.is_empty() {
                self.forward.remove(&old);
            }
        }
        self.forward.entry(key).or_default().push(idx);
        Ok(())
    }

    pub fn remove_at(&mut self, idx: usize) {
        if let Some(key) = self.reverse.remove(&idx)
            && let Some(vec) = self.forward.get_mut(&key)
        {
            if let Some(p) = vec.iter().position(|&x| x == idx) {
                vec.swap_remove(p);
            }
            if vec.is_empty() {
                self.forward.remove(&key);
            }
        }
    }

    fn bounds(&self, use_: &IndexUse, now: i64) -> (Bound<IndexKey>, Bound<IndexKey>) {
        let arity = self.def.fields.len();
        let eqs: Vec<IndexPart> = use_.eqs.iter().map(|(_, v)| value_part(v, now)).collect();
        let mut lo = eqs.clone();
        let mut hi = eqs;
        while lo.len() < arity {
            lo.push(IndexPart::Min);
        }
        while hi.len() < arity {
            hi.push(IndexPart::Max);
        }
        if let Some((field, op, val)) = &use_.range
            && let Some(pos) = self.def.fields.iter().position(|f| f == field)
        {
            let p = value_part(val, now);
            match op {
                CmpOp::Gt => lo[pos] = next_part(&p),
                CmpOp::Ge => lo[pos] = p,
                CmpOp::Lt => hi[pos] = p,
                CmpOp::Le => hi[pos] = p,
                CmpOp::Eq | CmpOp::Ne => {}
            }
        }
        let start = Bound::Included(IndexKey(lo));
        let end = match &use_.range {
            Some((_, CmpOp::Lt, _)) => Bound::Excluded(IndexKey(hi)),
            _ => Bound::Included(IndexKey(hi)),
        };
        (start, end)
    }

    /// Count matching row indices without allocating an id/index list.
    pub fn seek_count(&self, use_: &IndexUse, now: i64) -> usize {
        let (start, end) = self.bounds(use_, now);
        self.forward
            .range((start, end))
            .map(|(_, idxs)| idxs.len())
            .sum()
    }

    pub fn seek_idxs(&self, use_: &IndexUse, now: i64) -> Vec<usize> {
        let (start, end) = self.bounds(use_, now);
        let mut out = Vec::new();
        for (_, idxs) in self.forward.range((start, end)) {
            out.extend_from_slice(idxs);
        }
        out
    }
}

fn next_part(p: &IndexPart) -> IndexPart {
    match p {
        IndexPart::Int(n) => IndexPart::Int(n.saturating_add(1)),
        IndexPart::Time(n) => IndexPart::Time(n.saturating_add(1)),
        IndexPart::Text(s) => {
            let mut t = s.as_ref().to_owned();
            t.push('\0');
            IndexPart::Text(Arc::from(t))
        }
        IndexPart::Bool(false) => IndexPart::Bool(true),
        other => other.clone(),
    }
}

fn cell_part(c: &Cell) -> IndexPart {
    match c {
        Cell::Null => IndexPart::Null,
        Cell::Bool(b) => IndexPart::Bool(*b),
        Cell::Int(n) => IndexPart::Int(*n),
        Cell::Time(n) => IndexPart::Time(*n),
        Cell::Text(s) => IndexPart::Text(Arc::clone(s)),
        Cell::Float(n) => IndexPart::Int(n.to_bits() as i64),
    }
}

fn value_part(v: &Value, now: i64) -> IndexPart {
    match v {
        Value::String(s) => IndexPart::Text(Arc::from(s.as_str())),
        Value::Name(s) => IndexPart::Text(Arc::from(s.as_str())),
        Value::Int(n) => IndexPart::Int(*n),
        Value::Float(n) => IndexPart::Int(n.to_bits() as i64),
        Value::Bool(b) => IndexPart::Bool(*b),
        Value::Now => IndexPart::Time(now),
        Value::NowMinus(d) => IndexPart::Time(now - d.as_millis()),
        Value::Duration(d) => IndexPart::Int(d.as_millis()),
    }
}

/// Pick one seek per DNF branch. `or` → union of seeks when every branch is indexable.
pub fn pick_index(cat: &Catalog, collection: &str, pred: &Pred) -> Option<Vec<IndexUse>> {
    let branches = dnf(pred);
    let mut uses = Vec::with_capacity(branches.len());
    for branch in &branches {
        uses.push(pick_conjunct(cat, collection, branch)?);
    }
    Some(uses)
}

fn pick_conjunct(cat: &Catalog, collection: &str, pred: &Pred) -> Option<IndexUse> {
    let atoms = flatten_and(pred);
    let mut best: Option<IndexUse> = None;
    let mut best_n = 0usize;
    for def in cat.indexes_on(collection) {
        let mut eqs = Vec::new();
        let mut range = None;
        let mut n = 0usize;
        for field in &def.fields {
            if let Some(v) = find_eq(&atoms, field) {
                eqs.push((field.clone(), v.clone()));
                n += 1;
                continue;
            }
            if let Some((op, v)) = find_range(&atoms, field) {
                range = Some((field.clone(), op, v.clone()));
                n += 1;
            }
            break;
        }
        if n == 0 {
            continue;
        }
        if eqs.is_empty() && def.fields.len() > 1 {
            continue;
        }
        if n > best_n {
            best_n = n;
            best = Some(IndexUse {
                def: def.clone(),
                eqs,
                range,
            });
        }
    }
    best
}

/// True when every atomic predicate is enforced by the index seek(s) (no residual filter).
pub fn index_covers_pred(pred: &Pred, uses: &[IndexUse]) -> bool {
    let branches = dnf(pred);
    if branches.len() != uses.len() {
        return false;
    }
    branches
        .iter()
        .zip(uses.iter())
        .all(|(branch, use_)| index_covers_conjunct(branch, use_))
}

fn index_covers_conjunct(pred: &Pred, use_: &IndexUse) -> bool {
    for atom in flatten_and(pred) {
        match atom {
            Pred::Cmp {
                field,
                op: CmpOp::Eq,
                ..
            } => {
                if !use_.eqs.iter().any(|(f, _)| f == &field.as_str()) {
                    return false;
                }
            }
            Pred::Cmp {
                field,
                op,
                ..
            } if matches!(op, CmpOp::Gt | CmpOp::Lt | CmpOp::Ge | CmpOp::Le) => {
                let Some((rf, rop, _)) = &use_.range else {
                    return false;
                };
                if rf != &field.as_str() || rop != op {
                    return false;
                }
            }
            _ => return false,
        }
    }
    true
}

/// Disjunctive normal form: list of AND-trees (no top-level `or` inside a branch).
fn dnf(pred: &Pred) -> Vec<Pred> {
    match pred {
        Pred::Or(a, b) => {
            let mut out = dnf(a);
            out.extend(dnf(b));
            out
        }
        Pred::And(a, b) => {
            let left = dnf(a);
            let right = dnf(b);
            let mut out = Vec::with_capacity(left.len() * right.len());
            for l in &left {
                for r in &right {
                    out.push(Pred::And(Box::new(l.clone()), Box::new(r.clone())));
                }
            }
            out
        }
        other => vec![other.clone()],
    }
}

fn flatten_and(pred: &Pred) -> Vec<&Pred> {
    match pred {
        Pred::And(a, b) => {
            let mut v = flatten_and(a);
            v.extend(flatten_and(b));
            v
        }
        other => vec![other],
    }
}

fn find_eq<'a>(atoms: &[&'a Pred], field: &str) -> Option<&'a Value> {
    for a in atoms {
        if let Pred::Cmp {
            field: f,
            op: CmpOp::Eq,
            value,
        } = a
            && f.as_str() == field
        {
            return Some(value);
        }
    }
    None
}

fn find_range<'a>(atoms: &[&'a Pred], field: &str) -> Option<(CmpOp, &'a Value)> {
    for a in atoms {
        if let Pred::Cmp {
            field: f,
            op,
            value,
        } = a
            && f.as_str() == field
            && matches!(op, CmpOp::Gt | CmpOp::Lt | CmpOp::Ge | CmpOp::Le)
        {
            return Some((*op, value));
        }
    }
    None
}
