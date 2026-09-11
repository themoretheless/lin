use std::collections::{BTreeMap, BTreeSet};
use std::ops::Bound;

use crate::ast::{CmpOp, Pred, Value};
use crate::catalog::{Catalog, IndexDef};
use crate::store::{Cell, Row, row_text};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum IndexPart {
    Min,
    Null,
    Bool(bool),
    Int(i64),
    Time(i64),
    Text(String),
    Max,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct IndexKey(pub Vec<IndexPart>);

#[derive(Debug, Clone)]
pub struct LiveIndex {
    pub def: IndexDef,
    pub forward: BTreeMap<IndexKey, BTreeSet<String>>,
    pub reverse: BTreeMap<String, IndexKey>,
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
            reverse: BTreeMap::new(),
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

    pub fn insert(&mut self, id: &str, row: &Row) -> Result<(), String> {
        let key = self.key_of(row);
        if self.def.unique
            && let Some(ids) = self.forward.get(&key)
            && ids.iter().any(|x| x != id)
        {
            return Err(format!("unique index {}: duplicate key", self.def.label()));
        }
        if let Some(old) = self.reverse.insert(id.to_string(), key.clone())
            && let Some(set) = self.forward.get_mut(&old)
        {
            set.remove(id);
            if set.is_empty() {
                self.forward.remove(&old);
            }
        }
        self.forward.entry(key).or_default().insert(id.to_string());
        Ok(())
    }

    pub fn remove(&mut self, id: &str) {
        if let Some(key) = self.reverse.remove(id)
            && let Some(set) = self.forward.get_mut(&key)
        {
            set.remove(id);
            if set.is_empty() {
                self.forward.remove(&key);
            }
        }
    }

    pub fn seek(&self, use_: &IndexUse, now: i64) -> Vec<String> {
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
        let mut out = Vec::new();
        for (_, ids) in self.forward.range((start, end)) {
            out.extend(ids.iter().cloned());
        }
        out
    }
}

fn next_part(p: &IndexPart) -> IndexPart {
    match p {
        IndexPart::Int(n) => IndexPart::Int(n.saturating_add(1)),
        IndexPart::Time(n) => IndexPart::Time(n.saturating_add(1)),
        IndexPart::Text(s) => {
            let mut t = s.clone();
            t.push('\0');
            IndexPart::Text(t)
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
        Cell::Text(s) => IndexPart::Text(s.clone()),
        Cell::Float(n) => IndexPart::Int(n.to_bits() as i64),
    }
}

fn value_part(v: &Value, now: i64) -> IndexPart {
    match v {
        Value::String(s) => IndexPart::Text(s.clone()),
        Value::Name(s) => IndexPart::Text(s.clone()),
        Value::Int(n) => IndexPart::Int(*n),
        Value::Float(n) => IndexPart::Int(n.to_bits() as i64),
        Value::Bool(b) => IndexPart::Bool(*b),
        Value::Now => IndexPart::Time(now),
        Value::NowMinus(d) => IndexPart::Time(now - d.as_millis()),
        Value::Duration(d) => IndexPart::Int(d.as_millis()),
    }
}

pub fn pick_index(cat: &Catalog, collection: &str, pred: &Pred) -> Option<IndexUse> {
    if pred_has_or(pred) {
        return None;
    }
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

pub fn pred_has_or(pred: &Pred) -> bool {
    match pred {
        Pred::Or(_, _) => true,
        Pred::And(a, b) => pred_has_or(a) || pred_has_or(b),
        _ => false,
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

pub fn row_id(row: &Row) -> Option<String> {
    row_text(row, "id")
        .or_else(|| row_text(row, "uri"))
        .map(str::to_string)
}
