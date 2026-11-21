use std::sync::Arc;

use rustc_hash::FxHashMap;

use crate::ast::{CmpOp, Field, Pred, Value, WindowSort};
use crate::store::{Cell, Row, row_text};
use crate::window::{FrameBound, WindowFrame, WindowFunction};

const REGEX_CACHE_MAX_PATTERNS: usize = 256;

struct RegexCacheState {
    last: Option<(String, String, Option<Arc<regex::Regex>>)>,
    /// pattern -> (flags, regex); probed by `&str`, so a hit allocates nothing.
    map: FxHashMap<String, Vec<(String, Option<Arc<regex::Regex>>)>>,
}

// Thread-local regex compilation cache: (pattern, flags) -> Regex
// Includes a 1-slot MRU fast path so evaluating a regex predicate across rows does zero allocations.
thread_local! {
    static REGEX_CACHE: std::cell::RefCell<RegexCacheState> = std::cell::RefCell::new(RegexCacheState {
        last: None,
        map: FxHashMap::default(),
    });
}

#[inline(always)]
pub(crate) fn value_cell(v: &Value, now: i64) -> Cell {
    match v {
        Value::String(s) => Cell::Text(std::sync::Arc::clone(s)),
        Value::Int(n) => Cell::Int(*n),
        Value::Float(n) => Cell::Float(*n),
        Value::Bool(b) => Cell::Bool(*b),
        Value::Timestamp(millis) => Cell::Time(*millis),
        Value::Now => Cell::Time(now),
        Value::NowMinus(d) => Cell::Time(now - d.as_millis()),
        Value::Duration(d) => Cell::Int(d.as_millis()),
        Value::Name(n) => Cell::text_arc(n.as_str()),
        Value::Param(_) => Cell::Null,
    }
}

#[inline]
pub(crate) fn lookup_param<'a>(params: &'a [(&str, Cell)], name: &str) -> Option<&'a Cell> {
    params.iter().rfind(|(k, _)| *k == name).map(|(_, v)| v)
}

#[inline]
pub(crate) fn value_needs_now(v: &Value) -> bool {
    matches!(v, Value::Now | Value::NowMinus(_))
}

pub(crate) fn pred_needs_now(pred: &Pred) -> bool {
    match pred {
        Pred::And(a, b) | Pred::Or(a, b) => pred_needs_now(a) || pred_needs_now(b),
        Pred::Not(inner) => pred_needs_now(inner),
        Pred::Cmp { value, .. } => value_needs_now(value),
        Pred::In { values, .. } => values.iter().any(value_needs_now),
        _ => false,
    }
}

pub(crate) fn pred_has_param(pred: &Pred) -> bool {
    match pred {
        Pred::And(a, b) | Pred::Or(a, b) => pred_has_param(a) || pred_has_param(b),
        Pred::Not(inner) => pred_has_param(inner),
        Pred::Cmp { value, .. } => matches!(value, Value::Param(_)),
        Pred::In { values, .. } => values.iter().any(|v| matches!(v, Value::Param(_))),
        _ => false,
    }
}

pub(crate) fn record_has_param(record: &crate::ast::Record) -> bool {
    record
        .fields
        .iter()
        .any(|(_, v)| matches!(v, Value::Param(_)))
}

pub(crate) fn record_needs_now(record: &crate::ast::Record) -> bool {
    record.fields.iter().any(|(_, v)| value_needs_now(v))
}

fn cell_to_ast_value(c: &Cell) -> Value {
    match c {
        Cell::Null => Value::Param(Arc::from("")),
        Cell::Bool(b) => Value::Bool(*b),
        Cell::Int(n) => Value::Int(*n),
        Cell::Float(f) => Value::Float(*f),
        Cell::Time(t) => Value::Timestamp(*t),
        Cell::Text(s) => Value::String(Arc::clone(s)),
        Cell::SmallText(s) => Value::String(Arc::from(s.as_str())),
        Cell::Vec(_) => Value::Param(Arc::from("")),
    }
}

pub(crate) fn bind_pred_params(pred: &Pred, params: &[(&str, Cell)]) -> Pred {
    if !pred_has_param(pred) {
        return pred.clone();
    }
    match pred {
        Pred::And(a, b) => Pred::And(
            Box::new(bind_pred_params(a, params)),
            Box::new(bind_pred_params(b, params)),
        ),
        Pred::Or(a, b) => Pred::Or(
            Box::new(bind_pred_params(a, params)),
            Box::new(bind_pred_params(b, params)),
        ),
        Pred::Not(inner) => Pred::Not(Box::new(bind_pred_params(inner, params))),
        Pred::Cmp { field, op, value } => {
            if let Value::Param(name) = value {
                let bound = lookup_param(params, name)
                    .map(cell_to_ast_value)
                    .unwrap_or_else(|| Value::Param(Arc::from("")));
                Pred::Cmp {
                    field: field.clone(),
                    op: *op,
                    value: bound,
                }
            } else {
                pred.clone()
            }
        }
        Pred::In { field, values } => {
            let bound_vals = values
                .iter()
                .map(|v| match v {
                    Value::Param(name) => lookup_param(params, name)
                        .map(cell_to_ast_value)
                        .unwrap_or_else(|| Value::Param(Arc::from(""))),
                    other => other.clone(),
                })
                .collect();
            Pred::In {
                field: field.clone(),
                values: bound_vals,
            }
        }
        _ => pred.clone(),
    }
}

pub(crate) fn bind_record_params(
    record: &crate::ast::Record,
    params: &[(&str, Cell)],
) -> crate::ast::Record {
    if !record_has_param(record) {
        return record.clone();
    }
    crate::ast::Record {
        fields: record
            .fields
            .iter()
            .map(|(k, v)| {
                let nv = match v {
                    Value::Param(name) => lookup_param(params, name)
                        .map(cell_to_ast_value)
                        .unwrap_or_else(|| Value::Param(Arc::from(""))),
                    other => other.clone(),
                };
                (k.clone(), nv)
            })
            .collect(),
    }
}

pub(crate) fn bind_stmt_params(
    stmt: &crate::ast::Stmt,
    params: &[(&str, Cell)],
) -> crate::ast::Stmt {
    use crate::ast::{EdgeTarget, InsertEdge, Query, Step, Stmt};
    let bind_val = |v: &Value| -> Value {
        match v {
            Value::Param(name) => lookup_param(params, name)
                .map(cell_to_ast_value)
                .unwrap_or_else(|| Value::Param(Arc::from(""))),
            other => other.clone(),
        }
    };
    let bind_query = |q: &Query| -> Query {
        if !q
            .steps
            .iter()
            .any(|step| matches!(step, Step::Filter(p) if pred_has_param(p)))
        {
            return q.clone();
        }
        Query {
            explain: q.explain,
            ignore_filter: q.ignore_filter,
            source: q.source.clone(),
            steps: q
                .steps
                .iter()
                .map(|step| match step {
                    Step::Filter(p) => Step::Filter(bind_pred_params(p, params)),
                    other => other.clone(),
                })
                .collect(),
        }
    };
    match stmt {
        Stmt::Query(q) => Stmt::Query(bind_query(q)),
        Stmt::Let { name, query } => Stmt::Let {
            name: name.clone(),
            query: bind_query(query),
        },
        Stmt::Insert {
            collection,
            records,
            edges,
        } => Stmt::Insert {
            collection: collection.clone(),
            records: records
                .iter()
                .map(|r| bind_record_params(r, params))
                .collect(),
            edges: edges
                .iter()
                .map(|e| InsertEdge {
                    rel: e.rel.clone(),
                    target: match &e.target {
                        EdgeTarget::Page(p) => EdgeTarget::Page(p.clone()),
                        EdgeTarget::Value(v) => EdgeTarget::Value(bind_val(v)),
                    },
                })
                .collect(),
        },
        Stmt::AppendFacts { records } => Stmt::AppendFacts {
            records: records
                .iter()
                .map(|r| bind_record_params(r, params))
                .collect(),
        },
        Stmt::AppendEdges { edges } => Stmt::AppendEdges {
            edges: edges
                .iter()
                .map(|e| crate::ast::EdgeLit {
                    rel: e.rel.clone(),
                    from: bind_val(&e.from),
                    to: bind_val(&e.to),
                })
                .collect(),
        },
        Stmt::Update {
            collection,
            pred,
            cas,
            cas_each,
            record,
        } => Stmt::Update {
            collection: collection.clone(),
            pred: pred.as_ref().map(|p| bind_pred_params(p, params)),
            cas: cas.clone(),
            cas_each: *cas_each,
            record: bind_record_params(record, params),
        },
        Stmt::Delete {
            collection,
            pred,
            cas,
            cas_each,
        } => Stmt::Delete {
            collection: collection.clone(),
            pred: bind_pred_params(pred, params),
            cas: cas.clone(),
            cas_each: *cas_each,
        },
        Stmt::DeleteEdge { rel, from, to } => Stmt::DeleteEdge {
            rel: rel.clone(),
            from: bind_val(from),
            to: bind_val(to),
        },
        other => other.clone(),
    }
}

#[inline]
pub(crate) fn value_to_arc_str(v: &Value, now: i64) -> Arc<str> {
    match v {
        Value::String(s) => Arc::clone(s),
        Value::Name(s) => Arc::from(s.as_str()),
        Value::Int(n) => Arc::from(n.to_string()),
        Value::Float(f) => Arc::from(f.to_string()),
        Value::Bool(b) => {
            if *b {
                static TRUE_ARC: std::sync::LazyLock<Arc<str>> =
                    std::sync::LazyLock::new(|| Arc::from("true"));
                Arc::clone(&TRUE_ARC)
            } else {
                static FALSE_ARC: std::sync::LazyLock<Arc<str>> =
                    std::sync::LazyLock::new(|| Arc::from("false"));
                Arc::clone(&FALSE_ARC)
            }
        }
        Value::Timestamp(t) => Arc::from(t.to_string()),
        _ => Arc::from(value_text(v, now)),
    }
}

pub(crate) fn value_text(v: &Value, now: i64) -> String {
    let c = value_cell(v, now);
    if let Some(s) = c.text() {
        s.to_owned()
    } else {
        c.compact()
    }
}

pub(crate) fn point_key(pred: &Pred) -> Option<(&str, &str)> {
    match pred {
        Pred::Cmp {
            field,
            op: CmpOp::Eq,
            value: Value::String(s),
        } if field.parts.len() == 1 && matches!(field.parts[0].as_str(), "id" | "uri") => {
            Some((field.parts[0].as_str(), s.as_ref()))
        }
        Pred::And(a, b) => point_key(a).or_else(|| point_key(b)),
        _ => None,
    }
}

pub(crate) fn eval_pred(pred: &Pred, row: &Row, now: i64) -> bool {
    match pred {
        Pred::And(a, b) => eval_pred(a, row, now) && eval_pred(b, row, now),
        Pred::Or(a, b) => eval_pred(a, row, now) || eval_pred(b, row, now),
        Pred::Not(inner) => !eval_pred(inner, row, now),
        Pred::IsNull { field, .. }
        | Pred::In { field, .. }
        | Pred::Cmp { field, .. }
        | Pred::Has { field, .. }
        | Pred::Contains { field, .. }
        | Pred::Regex { field, .. } => eval_leaf(pred, field_cell(row, field), now),
    }
}

fn resolve_cached_regex(pattern: &str, flags: &str) -> Option<Arc<regex::Regex>> {
    REGEX_CACHE.with(|cache| {
        let mut state = cache.borrow_mut();
        if let Some((ref lp, ref lf, ref lre)) = state.last
            && lp == pattern
            && lf == flags
        {
            return lre.clone();
        }
        if let Some((_, re)) = state
            .map
            .get(pattern)
            .and_then(|v| v.iter().find(|(f, _)| f == flags))
        {
            let cloned = re.clone();
            state.last = Some((pattern.to_owned(), flags.to_owned(), cloned.clone()));
            return cloned;
        }
        if state.map.len() >= REGEX_CACHE_MAX_PATTERNS {
            state.map.clear();
        }
        let compiled = {
            let mut b = regex::RegexBuilder::new(pattern);
            b.size_limit(10 * 1024 * 1024);
            if flags.contains('i') {
                b.case_insensitive(true);
            }
            b.build().ok().map(Arc::new)
        };
        state
            .map
            .entry(pattern.to_owned())
            .or_default()
            .push((flags.to_owned(), compiled.clone()));
        state.last = Some((pattern.to_owned(), flags.to_owned(), compiled.clone()));
        compiled
    })
}

/// A leaf predicate decided on the value of its field (`Cell::Null` when absent).
/// `and` / `or` / `not` are not leaves and yield `false`.
pub(crate) fn eval_leaf(pred: &Pred, cell: &Cell, now: i64) -> bool {
    match pred {
        Pred::And(..) | Pred::Or(..) | Pred::Not(..) => false,
        Pred::IsNull { negated, .. } => {
            let is_null = matches!(cell, Cell::Null);
            if *negated { !is_null } else { is_null }
        }
        Pred::In { values, .. } => {
            let left = cell;
            values.iter().any(|v| cell_eq_val(left, v, now))
        }
        Pred::Cmp { op, value, .. } => cmp_cell(cell, value, *op, now),
        Pred::Has { ci, needle, .. } => cell
            .text()
            .is_some_and(|s| has_word(s, needle, *ci)),
        Pred::Contains { needle, .. } => cell
            .text()
            .is_some_and(|s| text_contains(s, needle)),
        Pred::Regex { pattern, flags, .. } => {
            let Some(text) = cell.text() else {
                return false;
            };

            REGEX_CACHE.with(|cache| {
                let mut state = cache.borrow_mut();
                if let Some((ref lp, ref lf, ref lre)) = state.last
                    && lp == pattern
                    && lf == flags
                {
                    return lre.as_ref().is_some_and(|re| re.is_match(text));
                }
                if let Some((_, re)) = state
                    .map
                    .get(pattern.as_str())
                    .and_then(|v| v.iter().find(|(f, _)| f == flags))
                {
                    let cloned = re.clone();
                    let matched = cloned.as_ref().is_some_and(|r| r.is_match(text));
                    state.last = Some((pattern.clone(), flags.clone(), cloned));
                    return matched;
                }
                // Patterns can come from parameters: keep the cache bounded.
                if state.map.len() >= REGEX_CACHE_MAX_PATTERNS {
                    state.map.clear();
                }
                let compiled = {
                    let mut b = regex::RegexBuilder::new(pattern);
                    b.size_limit(10 * 1024 * 1024);
                    if flags.contains('i') {
                        b.case_insensitive(true);
                    }
                    b.build().ok().map(Arc::new)
                };
                state
                    .map
                    .entry(pattern.clone())
                    .or_default()
                    .push((flags.clone(), compiled.clone()));
                let matched = compiled.as_ref().is_some_and(|re| re.is_match(text));
                state.last = Some((pattern.clone(), flags.clone(), compiled));
                matched
            })
        }
    }
}

pub(crate) fn pred_leaf_cost(pred: &Pred) -> u32 {
    match pred {
        Pred::IsNull { .. } => 1,
        Pred::Cmp { value, .. } => match value {
            Value::Bool(_)
            | Value::Int(_)
            | Value::Float(_)
            | Value::Timestamp(_)
            | Value::Duration(_)
            | Value::NowMinus(_)
            | Value::Now => 2,
            Value::String(_) | Value::Name(_) | Value::Param(_) => 4,
        },
        Pred::In { values, .. } => 6u32.saturating_add((values.len().min(16)) as u32),
        Pred::Contains { .. } => 25,
        Pred::Has { ci: false, .. } => 40,
        Pred::Has { ci: true, .. } => 55,
        Pred::Regex { .. } => 100,
        Pred::And(..) | Pred::Or(..) | Pred::Not(..) => 30,
    }
}

#[derive(Clone, Debug)]
enum SlotTree<L> {
    And(Vec<SlotTree<L>>),
    Or(Vec<SlotTree<L>>),
    Not(Box<SlotTree<L>>),
    Leaf(L),
}

fn build_slot_tree<'p, L>(
    pred: &'p Pred,
    make_leaf: &mut impl FnMut(&'p Pred, &'p Field) -> (L, u32),
) -> (SlotTree<L>, u32) {
    match pred {
        Pred::And(..) => {
            let mut terms = Vec::new();
            let mut stack = vec![pred];
            while let Some(p) = stack.pop() {
                if let Pred::And(a, b) = p {
                    // Push rhs first so lhs pops first, keeping tie order stable.
                    stack.push(b);
                    stack.push(a);
                } else {
                    terms.push(p);
                }
            }
            let mut built: Vec<(SlotTree<L>, u32)> = terms
                .into_iter()
                .map(|t| build_slot_tree(t, make_leaf))
                .collect();
            built.sort_by_key(|&(_, c)| c);
            let total_cost = built
                .iter()
                .fold(0u32, |acc, &(_, c)| acc.saturating_add(c))
                .max(1);
            let nodes = built.into_iter().map(|(n, _)| n).collect();
            (SlotTree::And(nodes), total_cost)
        }
        Pred::Or(..) => {
            let mut terms = Vec::new();
            let mut stack = vec![pred];
            while let Some(p) = stack.pop() {
                if let Pred::Or(a, b) = p {
                    stack.push(b);
                    stack.push(a);
                } else {
                    terms.push(p);
                }
            }
            let mut built: Vec<(SlotTree<L>, u32)> = terms
                .into_iter()
                .map(|t| build_slot_tree(t, make_leaf))
                .collect();
            built.sort_by_key(|&(_, c)| c);
            let total_cost = built
                .iter()
                .fold(0u32, |acc, &(_, c)| acc.saturating_add(c))
                .saturating_add(2);
            let nodes = built.into_iter().map(|(n, _)| n).collect();
            (SlotTree::Or(nodes), total_cost)
        }
        Pred::Not(inner) => {
            let (node, cost) = build_slot_tree(inner, make_leaf);
            (SlotTree::Not(Box::new(node)), cost)
        }
        Pred::IsNull { field, .. }
        | Pred::In { field, .. }
        | Pred::Cmp { field, .. }
        | Pred::Has { field, .. }
        | Pred::Contains { field, .. }
        | Pred::Regex { field, .. } => {
            let (leaf, cost) = make_leaf(pred, field);
            (SlotTree::Leaf(leaf), cost)
        }
    }
}

/// A predicate prepared for many rows: every single-name field keeps a [`FieldSlot`], so a
/// row with a shape seen before is read by position instead of a search by name. Conjuncts
/// and disjuncts are flattened and ordered by evaluation cost so cheap checks short-circuit
/// expensive text/regex matching while keeping results identical to [`eval_pred`].
pub(crate) struct PredSlots<'p> {
    tree: SlotTree<BorrowedLeaf<'p>>,
    slots: Vec<crate::store::row::FieldSlot<'p>>,
}

enum BorrowedLeaf<'p> {
    /// Leaf over a single-name field: index into `slots`.
    Slot(&'p Pred, usize),
    /// Pre-resolved regex over a single-name field.
    SlotRegex(usize, Option<Arc<regex::Regex>>),
    /// Leaf over a dotted path: resolved by name.
    ByName(&'p Pred),
    /// Pre-resolved regex over a dotted path.
    ByNameRegex(&'p Field, Option<Arc<regex::Regex>>),
}

impl<'p> PredSlots<'p> {
    pub(crate) fn new(pred: &'p Pred) -> Self {
        let mut slots: Vec<crate::store::row::FieldSlot<'p>> = Vec::new();
        let (tree, _) = build_slot_tree(pred, &mut |p, field| {
            let cost = pred_leaf_cost(p);
            match field.leaf() {
                Some(name) => {
                    let at = match slots.iter().position(|s| s.key() == name) {
                        Some(at) => at,
                        None => {
                            slots.push(crate::store::row::FieldSlot::new(name));
                            slots.len() - 1
                        }
                    };
                    if let Pred::Regex { pattern, flags, .. } = p {
                        (
                            BorrowedLeaf::SlotRegex(at, resolve_cached_regex(pattern, flags)),
                            cost,
                        )
                    } else {
                        (BorrowedLeaf::Slot(p, at), cost)
                    }
                }
                None => {
                    let c = cost.saturating_add(10);
                    if let Pred::Regex { pattern, flags, .. } = p {
                        (
                            BorrowedLeaf::ByNameRegex(field, resolve_cached_regex(pattern, flags)),
                            c,
                        )
                    } else {
                        (BorrowedLeaf::ByName(p), c)
                    }
                }
            }
        });
        PredSlots { tree, slots }
    }

    #[inline]
    pub(crate) fn eval(&mut self, row: &Row, now: i64) -> bool {
        fn go(
            node: &SlotTree<BorrowedLeaf<'_>>,
            slots: &mut [crate::store::row::FieldSlot<'_>],
            row: &Row,
            now: i64,
        ) -> bool {
            static NULL: Cell = Cell::Null;
            match node {
                SlotTree::And(parts) => parts.iter().all(|c| go(c, slots, row, now)),
                SlotTree::Or(parts) => parts.iter().any(|c| go(c, slots, row, now)),
                SlotTree::Not(inner) => !go(inner, slots, row, now),
                SlotTree::Leaf(BorrowedLeaf::Slot(pred, at)) => {
                    eval_leaf(pred, slots[*at].get(row).unwrap_or(&NULL), now)
                }
                SlotTree::Leaf(BorrowedLeaf::SlotRegex(at, re)) => slots[*at]
                    .get(row)
                    .and_then(Cell::text)
                    .is_some_and(|t| re.as_ref().is_some_and(|r| r.is_match(t))),
                SlotTree::Leaf(BorrowedLeaf::ByName(pred)) => eval_pred(pred, row, now),
                SlotTree::Leaf(BorrowedLeaf::ByNameRegex(field, re)) => field_cell(row, field)
                    .text()
                    .is_some_and(|t| re.as_ref().is_some_and(|r| r.is_match(t))),
            }
        }
        go(&self.tree, &mut self.slots, row, now)
    }
}

/// Owned, lifetime-free counterpart of [`PredSlots`] for [`crate::Prepared`] and [`crate::QueryCursor`].
#[derive(Clone, Debug)]
pub(crate) struct OwnedPredSlots {
    tree: SlotTree<OwnedLeaf>,
    slots: Vec<crate::store::row::OwnedFieldSlot>,
}

#[derive(Clone, Debug)]
enum OwnedLeaf {
    Slot(Pred, usize),
    SlotRegex(usize, Option<Arc<regex::Regex>>),
    ByName(Pred),
    ByNameRegex(Field, Option<Arc<regex::Regex>>),
}

impl OwnedPredSlots {
    pub(crate) fn new(pred: &Pred) -> Self {
        let mut slots: Vec<crate::store::row::OwnedFieldSlot> = Vec::new();
        let (tree, _) = build_slot_tree(pred, &mut |p, field| {
            let cost = pred_leaf_cost(p);
            match field.leaf() {
                Some(name) => {
                    let at = match slots.iter().position(|s| s.key() == name) {
                        Some(at) => at,
                        None => {
                            slots.push(crate::store::row::OwnedFieldSlot::new(name));
                            slots.len() - 1
                        }
                    };
                    if let Pred::Regex { pattern, flags, .. } = p {
                        (
                            OwnedLeaf::SlotRegex(at, resolve_cached_regex(pattern, flags)),
                            cost,
                        )
                    } else {
                        (OwnedLeaf::Slot(p.clone(), at), cost)
                    }
                }
                None => {
                    let c = cost.saturating_add(10);
                    if let Pred::Regex { pattern, flags, .. } = p {
                        (
                            OwnedLeaf::ByNameRegex(
                                field.clone(),
                                resolve_cached_regex(pattern, flags),
                            ),
                            c,
                        )
                    } else {
                        (OwnedLeaf::ByName(p.clone()), c)
                    }
                }
            }
        });
        OwnedPredSlots { tree, slots }
    }

    #[inline]
    pub(crate) fn eval(&mut self, row: &Row, now: i64) -> bool {
        fn go(
            node: &SlotTree<OwnedLeaf>,
            slots: &mut [crate::store::row::OwnedFieldSlot],
            row: &Row,
            now: i64,
        ) -> bool {
            static NULL: Cell = Cell::Null;
            match node {
                SlotTree::And(parts) => parts.iter().all(|c| go(c, slots, row, now)),
                SlotTree::Or(parts) => parts.iter().any(|c| go(c, slots, row, now)),
                SlotTree::Not(inner) => !go(inner, slots, row, now),
                SlotTree::Leaf(OwnedLeaf::Slot(pred, at)) => {
                    eval_leaf(pred, slots[*at].get(row).unwrap_or(&NULL), now)
                }
                SlotTree::Leaf(OwnedLeaf::SlotRegex(at, re)) => slots[*at]
                    .get(row)
                    .and_then(Cell::text)
                    .is_some_and(|t| re.as_ref().is_some_and(|r| r.is_match(t))),
                SlotTree::Leaf(OwnedLeaf::ByName(pred)) => eval_pred(pred, row, now),
                SlotTree::Leaf(OwnedLeaf::ByNameRegex(field, re)) => field_cell(row, field)
                    .text()
                    .is_some_and(|t| re.as_ref().is_some_and(|r| r.is_match(t))),
            }
        }
        go(&self.tree, &mut self.slots, row, now)
    }

    #[inline]
    pub(crate) fn eval_sync(&self, row: &Row, now: i64) -> bool {
        fn go(
            node: &SlotTree<OwnedLeaf>,
            slots: &[crate::store::row::OwnedFieldSlot],
            row: &Row,
            now: i64,
        ) -> bool {
            static NULL: Cell = Cell::Null;
            match node {
                SlotTree::And(parts) => parts.iter().all(|c| go(c, slots, row, now)),
                SlotTree::Or(parts) => parts.iter().any(|c| go(c, slots, row, now)),
                SlotTree::Not(inner) => !go(inner, slots, row, now),
                SlotTree::Leaf(OwnedLeaf::Slot(pred, at)) => {
                    eval_leaf(pred, slots[*at].get_sync(row).unwrap_or(&NULL), now)
                }
                SlotTree::Leaf(OwnedLeaf::SlotRegex(at, re)) => slots[*at]
                    .get_sync(row)
                    .and_then(Cell::text)
                    .is_some_and(|t| re.as_ref().is_some_and(|r| r.is_match(t))),
                SlotTree::Leaf(OwnedLeaf::ByName(pred)) => eval_pred(pred, row, now),
                SlotTree::Leaf(OwnedLeaf::ByNameRegex(field, re)) => field_cell(row, field)
                    .text()
                    .is_some_and(|t| re.as_ref().is_some_and(|r| r.is_match(t))),
            }
        }
        go(&self.tree, &self.slots, row, now)
    }
}

pub(crate) fn text_contains(hay: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return true;
    }
    memchr::memmem::find(hay.as_bytes(), needle.as_bytes()).is_some()
}

pub(crate) fn field_cell<'a>(row: &'a Row, field: &Field) -> &'a Cell {
    static NULL: Cell = Cell::Null;
    if let Some(k) = field.leaf() {
        return row.get(k).unwrap_or(&NULL);
    }
    let want = field.as_str();
    row.get(&want).unwrap_or(&NULL)
}

#[inline]
pub(crate) fn int_eq_f64(i: i64, f: f64) -> bool {
    if !f.is_finite() || f.fract() != 0.0 {
        return false;
    }
    if f < -9223372036854775808.0 || f >= 9223372036854775808.0 {
        return false;
    }
    let fi = f as i64;
    (fi as f64 == f) && (i == fi)
}

#[inline]
pub(crate) fn cmp_i64_f64(i: i64, f: f64) -> Option<std::cmp::Ordering> {
    if f.is_nan() {
        return None;
    }
    if f.is_infinite() {
        return if f.is_sign_positive() {
            Some(std::cmp::Ordering::Less)
        } else {
            Some(std::cmp::Ordering::Greater)
        };
    }
    if f < -9223372036854775808.0 {
        return Some(std::cmp::Ordering::Greater);
    }
    if f >= 9223372036854775808.0 {
        return Some(std::cmp::Ordering::Less);
    }
    let fi = f as i64;
    let f_back = fi as f64;
    if f_back == f {
        Some(i.cmp(&fi))
    } else if f > f_back {
        if i <= fi {
            Some(std::cmp::Ordering::Less)
        } else {
            Some(std::cmp::Ordering::Greater)
        }
    } else if i >= fi {
        Some(std::cmp::Ordering::Greater)
    } else {
        Some(std::cmp::Ordering::Less)
    }
}

#[inline]
pub(crate) fn cell_eq_val(left: &Cell, value: &Value, now: i64) -> bool {
    match (left, value) {
        (Cell::Text(x), Value::String(y)) => x.as_ref() == y.as_ref(),
        (Cell::Text(x), Value::Name(y)) => x.as_ref() == y.as_str(),
        (Cell::SmallText(x), Value::String(y)) => x.as_str() == y.as_ref(),
        (Cell::SmallText(x), Value::Name(y)) => x.as_str() == y.as_str(),
        (Cell::Bool(x), Value::Bool(y)) => *x == *y,
        (Cell::Int(x), Value::Int(y)) => *x == *y,
        (Cell::Float(x), Value::Float(y)) => {
            if x.is_nan() || y.is_nan() {
                false
            } else {
                *x == *y
            }
        }
        (Cell::Int(x), Value::Float(y)) => int_eq_f64(*x, *y),
        (Cell::Float(x), Value::Int(y)) => int_eq_f64(*y, *x),
        (Cell::Time(x), Value::Timestamp(y)) => *x == *y,
        (Cell::Time(x), Value::Now) => *x == now,
        (Cell::Time(x), Value::NowMinus(d)) => *x == now - d.as_millis(),
        (Cell::Int(x), Value::Duration(d)) => *x == d.as_millis(),
        (Cell::Float(x), Value::Duration(d)) => int_eq_f64(d.as_millis(), *x),
        _ => false,
    }
}

pub(crate) fn cmp_cell(left: &Cell, value: &Value, op: CmpOp, now: i64) -> bool {
    match op {
        CmpOp::Eq => cell_eq_val(left, value, now),
        CmpOp::Ne => !cell_eq_val(left, value, now),
        CmpOp::Gt | CmpOp::Lt | CmpOp::Ge | CmpOp::Le => {
            let ord = match (left, value) {
                (Cell::Int(l), Value::Int(r)) => Some(l.cmp(r)),
                (Cell::Time(l), Value::Timestamp(r)) => Some(l.cmp(r)),
                (Cell::Time(l), Value::Now) => Some(l.cmp(&now)),
                (Cell::Time(l), Value::NowMinus(d)) => Some(l.cmp(&(now - d.as_millis()))),
                (Cell::Int(l), Value::Duration(d)) => Some(l.cmp(&d.as_millis())),
                (Cell::Float(l), Value::Float(r)) => l.partial_cmp(r),
                (Cell::Int(l), Value::Float(r)) => cmp_i64_f64(*l, *r),
                (Cell::Float(l), Value::Int(r)) => cmp_i64_f64(*r, *l).map(|o| o.reverse()),
                (Cell::Float(l), Value::Duration(d)) => {
                    cmp_i64_f64(d.as_millis(), *l).map(|o| o.reverse())
                }
                (Cell::Text(l), Value::String(r)) => Some(l.as_ref().cmp(r.as_ref())),
                (Cell::Text(l), Value::Name(r)) => Some(l.as_ref().cmp(r.as_str())),
                (Cell::SmallText(l), Value::String(r)) => Some(l.as_str().cmp(r.as_ref())),
                (Cell::SmallText(l), Value::Name(r)) => Some(l.as_str().cmp(r.as_str())),
                _ => None,
            };
            let Some(ord) = ord else {
                return false;
            };
            match op {
                CmpOp::Gt => ord == std::cmp::Ordering::Greater,
                CmpOp::Lt => ord == std::cmp::Ordering::Less,
                CmpOp::Ge => ord != std::cmp::Ordering::Less,
                CmpOp::Le => ord != std::cmp::Ordering::Greater,
                _ => false,
            }
        }
    }
}

thread_local! {
    static HAS_WORD_SCRATCH: std::cell::RefCell<(String, String)> =
        std::cell::RefCell::new((String::with_capacity(64), String::with_capacity(64)));
}

pub(crate) fn has_word(hay: &str, needle: &str, ci: bool) -> bool {
    if needle.is_empty() {
        return false;
    }
    if !ci {
        return hay
            .split(|c: char| !(c.is_alphanumeric() || c == '_'))
            .any(|w| w == needle);
    }
    if hay.is_ascii() && needle.is_ascii() {
        return hay
            .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .any(|w| w.eq_ignore_ascii_case(needle));
    }
    // Case-insensitive without allocating per-row or per-token Strings.
    HAS_WORD_SCRATCH.with(|cell| {
        let (nbuf, wbuf) = &mut *cell.borrow_mut();
        nbuf.clear();
        for ch in needle.chars() {
            for c in ch.to_lowercase() {
                nbuf.push(c);
            }
        }
        for tok in hay.split(|c: char| !(c.is_alphanumeric() || c == '_')) {
            if tok.is_empty() {
                continue;
            }
            wbuf.clear();
            for ch in tok.chars() {
                for c in ch.to_lowercase() {
                    wbuf.push(c);
                }
            }
            if *wbuf == *nbuf {
                return true;
            }
        }
        false
    })
}

pub(crate) struct LexQuery {
    pub(crate) lower: String,
    pub(crate) tokens: Vec<String>,
}

impl LexQuery {
    pub(crate) fn new(query: &str) -> Self {
        let lower = query.to_lowercase();
        let tokens = lower.split_whitespace().map(str::to_owned).collect();
        Self { lower, tokens }
    }
}

thread_local! {
    static LEX_SCORE_SCRATCH: std::cell::RefCell<(String, String)> =
        std::cell::RefCell::new((String::with_capacity(512), String::with_capacity(512)));
}

#[inline]
pub(crate) fn lex_score_prepared(row: &Row, query: &LexQuery) -> i64 {
    LEX_SCORE_SCRATCH.with(|cell| {
        let (blob, lower_buf) = &mut *cell.borrow_mut();
        blob.clear();
        for k in ["title", "body", "snippet"] {
            if let Some(t) = row_text(row, k) {
                blob.push_str(t);
                blob.push(' ');
            }
        }
        if blob.is_empty() {
            return 0;
        }
        let blob_l: &str = if blob.is_ascii() {
            if blob.bytes().any(|b| b.is_ascii_uppercase()) {
                blob.make_ascii_lowercase();
            }
            blob.as_str()
        } else {
            // Keep str lowercase semantics, including contextual Unicode casing.
            lower_buf.clear();
            lower_buf.push_str(&blob.to_lowercase());
            lower_buf.as_str()
        };
        let mut score = 0i64;
        for tok in &query.tokens {
            if let Some(prefix) = tok.strip_suffix('*') {
                if !prefix.is_empty() && blob_l.contains(prefix) {
                    score += 1;
                    if blob_l
                        .split(|c: char| !(c.is_alphanumeric() || c == '_'))
                        .any(|word| word.starts_with(prefix))
                    {
                        score += 2;
                    }
                }
            } else if blob_l.contains(tok) {
                score += 1;
                if blob_l
                    .split(|c: char| !(c.is_alphanumeric() || c == '_'))
                    .any(|word| word == tok)
                {
                    score += 2;
                }
            }
        }
        if !query.lower.is_empty() && blob_l.contains(&query.lower) {
            score += 3;
        }
        score
    })
}

/// Zero-allocation generic projector accepting `&[S]` (`S: AsRef<str>`), such as
/// `SmallVec<[Cow<'_, str>; 8]>`, without forcing `Vec<String>` allocations.
pub(crate) struct SliceRowProjector<'a> {
    shape: Option<Arc<crate::store::RowShape>>,
    slots: smallvec::SmallVec<[crate::store::row::FieldSlot<'a>; 8]>,
}

impl<'a> SliceRowProjector<'a> {
    #[inline]
    pub(crate) fn new<S: AsRef<str>>(fields: &'a [S]) -> Self {
        if fields.is_empty() {
            return Self {
                shape: None,
                slots: smallvec::SmallVec::new(),
            };
        }
        let shape = if fields.len() == 2
            && ((fields[0].as_ref() == "id" && fields[1].as_ref() == "title")
                || (fields[0].as_ref() == "title" && fields[1].as_ref() == "id"))
        {
            Arc::clone(crate::store::shape_id_title())
        } else {
            Row::shape_of(fields)
        };
        let slots = shape
            .names()
            .iter()
            .map(|target| {
                let src = fields
                    .iter()
                    .find(|f| f.as_ref() == target.as_str())
                    .map(AsRef::as_ref)
                    .unwrap_or("");
                crate::store::row::FieldSlot::new(src)
            })
            .collect();
        Self {
            shape: Some(shape),
            slots,
        }
    }

    #[inline]
    pub(crate) fn shape(&self) -> Option<&Arc<crate::store::RowShape>> {
        self.shape.as_ref()
    }

    #[inline]
    pub(crate) fn project(&mut self, row: &Row) -> Row {
        let Some(ref shape) = self.shape else {
            return Row::new();
        };
        Row::from_parts(
            Arc::clone(shape),
            self.slots
                .iter_mut()
                .map(|slot| slot.get(row).map(Cell::clone_for_borrow).unwrap_or(Cell::Null)),
        )
    }
}

pub(crate) fn project(rows: &[Row], fields: &[Field]) -> Vec<Row> {
    let keys: smallvec::SmallVec<[std::borrow::Cow<'_, str>; 8]> =
        fields.iter().map(Field::as_cow_str).collect();
    let mut proj = SliceRowProjector::new(&keys);
    let mut result = Vec::with_capacity(rows.len());
    for r in rows {
        result.push(proj.project(r));
    }
    result
}

/// [`project`] for rows the caller owns: rows already matching the projected shape stay in
/// place; all others are reshaped in O(1) per field via [`SliceRowProjector`].
pub(crate) fn project_owned(mut rows: Vec<Row>, fields: &[Field]) -> Vec<Row> {
    let keys: smallvec::SmallVec<[std::borrow::Cow<'_, str>; 8]> =
        fields.iter().map(Field::as_cow_str).collect();
    if keys.is_empty() {
        for row in &mut rows {
            row.clear();
        }
        return rows;
    }
    let mut proj = SliceRowProjector::new(&keys);
    for row in &mut rows {
        let same_shape = row
            .shape_arc()
            .zip(proj.shape())
            .is_some_and(|(a, b)| Arc::ptr_eq(a, b));
        if !same_shape {
            *row = proj.project(row);
        }
    }
    rows
}

/// Drop `fields` from owned rows in place without creating missing null entries or acquiring
/// per-row/per-field `Shape` transition `RwLock`s.
pub(crate) fn without_rows(mut rows: Vec<Row>, fields: &[Field]) -> Vec<Row> {
    if rows.is_empty() || fields.is_empty() {
        return rows;
    }
    let drop_keys: smallvec::SmallVec<[std::borrow::Cow<'_, str>; 8]> =
        fields.iter().map(Field::as_cow_str).collect();
    #[allow(clippy::type_complexity)]
    let mut shape_cache: smallvec::SmallVec<
        [(
            Arc<crate::store::RowShape>,
            Option<Arc<crate::store::RowShape>>,
            smallvec::SmallVec<[usize; 8]>,
        ); 4],
    > = smallvec::SmallVec::new();

    for row in &mut rows {
        let Some(src_shape_ref) = row.shape_arc() else {
            continue;
        };
        let hit_idx = shape_cache
            .iter()
            .position(|(s, _, _)| Arc::ptr_eq(s, src_shape_ref));
        let idx = match hit_idx {
            Some(i) => i,
            None => {
                let src_shape = Arc::clone(src_shape_ref);
                let mut keep_positions: smallvec::SmallVec<[usize; 8]> =
                    smallvec::SmallVec::with_capacity(src_shape.names().len());
                let dst_shape = {
                    let mut kept_names: smallvec::SmallVec<[&str; 16]> =
                        smallvec::SmallVec::with_capacity(src_shape.names().len());
                    for (pos, name) in src_shape.names().iter().enumerate() {
                        if !drop_keys.iter().any(|dk| dk.as_ref() == name.as_str()) {
                            keep_positions.push(pos);
                            kept_names.push(name.as_str());
                        }
                    }
                    if keep_positions.is_empty() {
                        None
                    } else if keep_positions.len() == src_shape.names().len() {
                        Some(Arc::clone(&src_shape))
                    } else {
                        Some(Row::shape_of(&kept_names))
                    }
                };
                if shape_cache.len() == 4 {
                    shape_cache.remove(0);
                }
                shape_cache.push((src_shape, dst_shape, keep_positions));
                shape_cache.len() - 1
            }
        };
        let (_, ref dst_shape_opt, ref keep_positions) = shape_cache[idx];
        match dst_shape_opt {
            None => row.clear(),
            Some(dst_shape) if keep_positions.len() == row.len() => {
                // None of `drop_keys` existed in `row`: keep untouched.
                let _ = dst_shape;
            }
            Some(dst_shape) => {
                let cells_mut = row.cells_mut();
                let mut kept_cells: smallvec::SmallVec<[Cell; 8]> =
                    smallvec::SmallVec::with_capacity(keep_positions.len());
                for &pos in keep_positions.as_slice() {
                    kept_cells.push(std::mem::replace(&mut cells_mut[pos], Cell::Null));
                }
                row.reset_from_parts(Arc::clone(dst_shape), kept_cells);
            }
        }
    }
    rows
}

pub(crate) fn sort_rows_top_k(
    rows: &mut Vec<Row>,
    field: &Field,
    desc: bool,
    top_k: Option<usize>,
) {
    let n = rows.len();
    if n <= 1 {
        return;
    }
    if let Some(0) = top_k {
        rows.clear();
        return;
    }
    let k = field.as_cow_str();
    static NULL_CELL: Cell = Cell::Null;

    if n <= 32 {
        let mut key_slot = crate::store::row::FieldSlot::new(k.as_ref());
        let mut order: smallvec::SmallVec<[u8; 32]> = (0..n as u8).collect();
        {
            let cells: smallvec::SmallVec<[&Cell; 32]> = rows
                .iter()
                .map(|r| key_slot.get(r).unwrap_or(&NULL_CELL))
                .collect();
            order.sort_by(|&a, &b| {
                let ord = cmp_sort(cells[a as usize], cells[b as usize]);
                if desc { ord.reverse() } else { ord }
            });
        }
        if let Some(limit) = top_k
            && limit < order.len()
        {
            order.truncate(limit);
        }
        let mut old_rows = std::mem::replace(rows, Vec::with_capacity(order.len()));
        for orig_idx in order {
            rows.push(std::mem::take(&mut old_rows[orig_idx as usize]));
        }
        return;
    }

    // Bounded max-heap fast path for small top-k over large inputs (`O(N + K log K)` with
    // `&Cell` comparison before cloning non-qualifying cells).
    if let Some(limit) = top_k
        && limit <= 256
        && n >= limit.saturating_mul(8)
    {
        struct HeapEntry {
            cell: Cell,
            idx: usize,
            desc: bool,
        }
        impl PartialEq for HeapEntry {
            fn eq(&self, other: &Self) -> bool {
                self.idx == other.idx
            }
        }
        impl Eq for HeapEntry {}
        impl PartialOrd for HeapEntry {
            fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
                Some(self.cmp(other))
            }
        }
        impl Ord for HeapEntry {
            fn cmp(&self, other: &Self) -> std::cmp::Ordering {
                let ord = cmp_sort(&self.cell, &other.cell);
                let ord = if self.desc { ord.reverse() } else { ord };
                ord.then_with(|| self.idx.cmp(&other.idx))
            }
        }

        let mut key_slot = crate::store::row::FieldSlot::new(k.as_ref());
        let mut heap: std::collections::BinaryHeap<HeapEntry> =
            std::collections::BinaryHeap::with_capacity(limit);
        for (i, r) in rows.iter().enumerate() {
            let cand = key_slot.get(r).unwrap_or(&NULL_CELL);
            if heap.len() < limit {
                heap.push(HeapEntry {
                    cell: cand.clone_for_borrow(),
                    idx: i,
                    desc,
                });
            } else if let Some(mut worst) = heap.peek_mut() {
                // Since `i > worst.idx`, `cand` strictly beats `worst` iff its sort key is
                // strictly before `worst.cell` (ties keep the earlier `worst.idx`).
                let ord = cmp_sort(cand, &worst.cell);
                let ord = if desc { ord.reverse() } else { ord };
                if ord == std::cmp::Ordering::Less {
                    *worst = HeapEntry {
                        cell: cand.clone_for_borrow(),
                        idx: i,
                        desc,
                    };
                }
            }
        }
        drop(key_slot);
        let winners = heap.into_sorted_vec();
        let mut old_rows = std::mem::replace(rows, Vec::with_capacity(winners.len()));
        for entry in winners {
            rows.push(std::mem::take(&mut old_rows[entry.idx]));
        }
        return;
    }

    // Decorate: extract sort keys once in O(N) instead of O(N log N) lookups by name.
    let mut key_slot = crate::store::row::FieldSlot::new(k.as_ref());
    let mut keys: Vec<(Cell, usize)> = rows
        .iter()
        .enumerate()
        .map(|(i, r)| {
            (
                key_slot.get(r).map(Cell::clone_for_borrow).unwrap_or(Cell::Null),
                i,
            )
        })
        .collect();
    drop(key_slot);

    let cmp_keys = |a: &(Cell, usize), b: &(Cell, usize)| {
        let ord = cmp_sort(&a.0, &b.0);
        let ord = if desc { ord.reverse() } else { ord };
        ord.then_with(|| a.1.cmp(&b.1))
    };

    let target_len = if let Some(limit) = top_k {
        if limit < keys.len() {
            keys.select_nth_unstable_by(limit - 1, cmp_keys);
            keys.truncate(limit);
        }
        keys.len()
    } else {
        keys.len()
    };

    keys.sort_by(cmp_keys);

    // Undecorate: permute rows into final order in O(N) without cloning
    let mut old_rows = std::mem::replace(rows, Vec::with_capacity(target_len));
    for (_, orig_idx) in keys {
        rows.push(std::mem::take(&mut old_rows[orig_idx]));
    }
}

/// Exact order of an integer and a non-NaN float, without rounding the integer to f64.
pub(crate) fn cmp_int_float(i: i64, f: f64) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    // 2^63 is exactly representable; every i64 is below it and at or above -2^63.
    const TWO_63: f64 = 9_223_372_036_854_775_808.0;
    if f >= TWO_63 {
        return Ordering::Less;
    }
    if f < -TWO_63 {
        return Ordering::Greater;
    }
    let t = f.trunc();
    match i.cmp(&(t as i64)) {
        Ordering::Equal => 0.0f64.partial_cmp(&(f - t)).unwrap_or(Ordering::Equal),
        o => o,
    }
}

pub(crate) fn cmp_sort(a: &Cell, b: &Cell) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    match (a, b) {
        (Cell::Null, Cell::Null) => Ordering::Equal,
        (Cell::Null, _) => Ordering::Less,
        (_, Cell::Null) => Ordering::Greater,
        (Cell::Text(x), Cell::Text(y)) => x.cmp(y),
        (Cell::SmallText(x), Cell::SmallText(y)) => x.cmp(y),
        (Cell::Text(x), Cell::SmallText(y)) => x.as_ref().cmp(y.as_str()),
        (Cell::SmallText(x), Cell::Text(y)) => x.as_str().cmp(y.as_ref()),
        (Cell::Bool(x), Cell::Bool(y)) => x.cmp(y),
        // Integers compare exactly: through f64, values above 2^53 that differ by one
        // tie or swap (`2^53 + 1` sorted before `2^53`).
        (Cell::Int(x) | Cell::Time(x), Cell::Int(y) | Cell::Time(y)) => x.cmp(y),
        (Cell::Int(x) | Cell::Time(x), Cell::Float(y)) if !y.is_nan() => cmp_int_float(*x, *y),
        (Cell::Float(x), Cell::Int(y) | Cell::Time(y)) if !x.is_nan() => {
            cmp_int_float(*y, *x).reverse()
        }
        _ => match (a.as_f64(), b.as_f64()) {
            (Some(x), Some(y)) => match (x.is_nan(), y.is_nan()) {
                (true, true) => Ordering::Equal,
                (true, false) => Ordering::Greater,
                (false, true) => Ordering::Less,
                (false, false) => x.partial_cmp(&y).expect("ordered numeric values"),
            },
            (Some(_), None) => Ordering::Greater,
            (None, Some(_)) => Ordering::Less,
            _ => a.compact().cmp(&b.compact()),
        },
    }
}

/// Widest frame summed row by row; wider frames that do not start at the first row use
/// prefix-sum subtraction (O(1) per row) and accept its rounding.
const DIRECT_FRAME_SUM_MAX: usize = 1024;

/// Float sum of rows `s..=e`. Subtracting running prefix sums loses small values to a
/// large earlier prefix (after `1e16`, a frame of `1.0, 0.5, 1.0` came out as `0`), so a
/// frame that does not start at the first row is summed from its own rows when it is
/// narrow enough; a frame from the first row is the running sum itself.
fn float_frame_sum(tv: &[&Cell], prefix: &[f64], s: usize, e: usize) -> f64 {
    if s == 0 {
        return prefix[e + 1];
    }
    if e - s < DIRECT_FRAME_SUM_MAX {
        let mut acc = 0.0f64;
        for c in &tv[s..=e] {
            if let Some(v) = c.as_f64() {
                acc += v;
            }
        }
        return acc;
    }
    prefix[e + 1] - prefix[s]
}

fn get_frame_bounds(
    i: usize,
    n: usize,
    frame: Option<&WindowFrame>,
    has_order: bool,
) -> (usize, usize) {
    match frame {
        Some(fr) => {
            let s = match fr.start {
                FrameBound::UnboundedPreceding => 0,
                FrameBound::Preceding(k) => i.saturating_sub(k),
                FrameBound::CurrentRow => i,
                FrameBound::Following(k) => (i + k).min(n.saturating_sub(1)),
                FrameBound::UnboundedFollowing => n.saturating_sub(1),
            };
            let e = match fr.end {
                FrameBound::UnboundedPreceding => 0,
                FrameBound::Preceding(k) => i.saturating_sub(k),
                FrameBound::CurrentRow => i,
                FrameBound::Following(k) => (i + k).min(n.saturating_sub(1)),
                FrameBound::UnboundedFollowing => n.saturating_sub(1),
            };
            (s, e)
        }
        None => {
            if has_order {
                (0, i)
            } else {
                (0, n.saturating_sub(1))
            }
        }
    }
}

/// Values of one window function over a partition already in window order.
///
/// `tv[i]` is the target field of row `i` (`Cell::Null` when absent, or for functions
/// without a field); `ties[i]` says whether row `i` equals row `i - 1` on every sort key
/// (empty when the window has no ordering). Shared by the row and the columnar executor.
#[inline]
pub(crate) fn window_values(
    func: &WindowFunction,
    tv: &[&Cell],
    ties: &[bool],
    has_order: bool,
    has_target: bool,
    frame: Option<&WindowFrame>,
) -> Vec<Cell> {
    window_values_len(func, tv, tv.len(), ties, has_order, has_target, frame)
}

pub(crate) fn window_values_len(
    func: &WindowFunction,
    tv: &[&Cell],
    part_len: usize,
    ties: &[bool],
    has_order: bool,
    has_target: bool,
    frame: Option<&WindowFrame>,
) -> Vec<Cell> {
    let n = part_len;
    if n == 0 {
        return Vec::new();
    }
    match func {
        WindowFunction::RowNumber => (0..n).map(|i| Cell::Int((i + 1) as i64)).collect(),
        WindowFunction::Rank => {
            let mut ranks = Vec::with_capacity(n);
            let mut current_rank = 1i64;
            for i in 0..n {
                // `ties` is empty when there is nothing to order by: one shared rank.
                if i > 0 && ties.get(i).is_some_and(|tied| !tied) {
                    current_rank = (i + 1) as i64;
                }
                ranks.push(Cell::Int(current_rank));
            }
            ranks
        }
        WindowFunction::DenseRank => {
            let mut ranks = Vec::with_capacity(n);
            let mut current_rank = 1i64;
            for i in 0..n {
                if i > 0 && ties.get(i).is_some_and(|tied| !tied) {
                    current_rank += 1;
                }
                ranks.push(Cell::Int(current_rank));
            }
            ranks
        }
        WindowFunction::Lead { offset } => {
            if tv.is_empty() {
                return vec![Cell::Null; n];
            }
            let off = *offset;
            (0..n)
                .map(|i| {
                    if i + off < tv.len() {
                        tv[i + off].clone_for_borrow()
                    } else {
                        Cell::Null
                    }
                })
                .collect()
        }
        WindowFunction::Lag { offset } => {
            if tv.is_empty() {
                return vec![Cell::Null; n];
            }
            let off = *offset;
            (0..n)
                .map(|i| {
                    if i >= off && i - off < tv.len() {
                        tv[i - off].clone_for_borrow()
                    } else {
                        Cell::Null
                    }
                })
                .collect()
        }
        WindowFunction::FirstValue => {
            if tv.is_empty() {
                return vec![Cell::Null; n];
            }
            if let Some(fr) = frame {
                (0..n)
                    .map(|i| {
                        let (s, e) = get_frame_bounds(i, n, Some(fr), has_order);
                        if s <= e && s < tv.len() {
                            tv[s].clone_for_borrow()
                        } else {
                            Cell::Null
                        }
                    })
                    .collect()
            } else {
                let first_val = tv.first().map(|c| c.clone_for_borrow()).unwrap_or(Cell::Null);
                vec![first_val; n]
            }
        }
        WindowFunction::LastValue => {
            if tv.is_empty() {
                return vec![Cell::Null; n];
            }
            if let Some(fr) = frame {
                (0..n)
                    .map(|i| {
                        let (s, e) = get_frame_bounds(i, n, Some(fr), has_order);
                        if s <= e && s < n {
                            let end_idx = e.min(n - 1);
                            tv.get(end_idx)
                                .map(|c| c.clone_for_borrow())
                                .unwrap_or(Cell::Null)
                        } else {
                            Cell::Null
                        }
                    })
                    .collect()
            } else {
                let last_val = tv.last().map(|c| c.clone_for_borrow()).unwrap_or(Cell::Null);
                vec![last_val; n]
            }
        }
        WindowFunction::Sum => {
            if tv.is_empty() {
                return vec![Cell::Int(0); n];
            }
            let has_float = tv.iter().any(|c| matches!(c, Cell::Float(_)));
            if has_float {
                let mut prefix = Vec::with_capacity(n + 1);
                prefix.push(0.0f64);
                let mut acc = 0.0f64;
                for c in tv {
                    if let Some(v) = c.as_f64() {
                        acc += v;
                    }
                    prefix.push(acc);
                }
                let mut sums = Vec::with_capacity(n);
                for i in 0..n {
                    let (s, e) = get_frame_bounds(i, n, frame, has_order);
                    if s <= e && s < n {
                        let end_idx = e.min(n - 1);
                        let sum = float_frame_sum(tv, &prefix, s, end_idx);
                        sums.push(Cell::Float(sum));
                    } else {
                        sums.push(Cell::Float(0.0));
                    }
                }
                sums
            } else {
                // Prefix sums in i128 cannot overflow; a frame sum outside i64 becomes a
                // float, as the table-level `sum` does, instead of wrapping around.
                let mut prefix: Vec<i128> = Vec::with_capacity(n + 1);
                prefix.push(0);
                let mut acc = 0i128;
                for c in tv {
                    if let Some(v) = c.as_int() {
                        acc += i128::from(v);
                    }
                    prefix.push(acc);
                }
                let mut sums = Vec::with_capacity(n);
                for i in 0..n {
                    let (s, e) = get_frame_bounds(i, n, frame, has_order);
                    if s <= e && s < n {
                        let end_idx = e.min(n - 1);
                        let sum = prefix[end_idx + 1] - prefix[s];
                        sums.push(match i64::try_from(sum) {
                            Ok(v) => Cell::Int(v),
                            Err(_) => Cell::Float(sum as f64),
                        });
                    } else {
                        sums.push(Cell::Int(0));
                    }
                }
                sums
            }
        }
        WindowFunction::Avg => {
            if tv.is_empty() {
                return vec![Cell::Float(0.0); n];
            }
            let mut prefix_sum = Vec::with_capacity(n + 1);
            let mut prefix_cnt = Vec::with_capacity(n + 1);
            prefix_sum.push(0.0f64);
            prefix_cnt.push(0usize);
            let mut sum_acc = 0.0f64;
            let mut cnt_acc = 0usize;
            for c in tv {
                if let Some(v) = c.as_f64() {
                    sum_acc += v;
                    cnt_acc += 1;
                }
                prefix_sum.push(sum_acc);
                prefix_cnt.push(cnt_acc);
            }
            let mut avgs = Vec::with_capacity(n);
            for i in 0..n {
                let (s, e) = get_frame_bounds(i, n, frame, has_order);
                if s <= e && s < n {
                    let end_idx = e.min(n - 1);
                    let sum = float_frame_sum(tv, &prefix_sum, s, end_idx);
                    let count = prefix_cnt[end_idx + 1] - prefix_cnt[s];
                    let avg = if count == 0 { 0.0 } else { sum / count as f64 };
                    avgs.push(Cell::Float(avg));
                } else {
                    avgs.push(Cell::Float(0.0));
                }
            }
            avgs
        }
        WindowFunction::Min => {
            if tv.is_empty() {
                vec![Cell::Null; n]
            } else {
                eval_window_extremum(tv, frame, has_order, false)
            }
        }
        WindowFunction::Max => {
            if tv.is_empty() {
                vec![Cell::Null; n]
            } else {
                eval_window_extremum(tv, frame, has_order, true)
            }
        }
        WindowFunction::Count => {
            if has_target {
                if tv.is_empty() {
                    return vec![Cell::Int(0); n];
                }
                let mut prefix_cnt = Vec::with_capacity(n + 1);
                prefix_cnt.push(0usize);
                let mut cnt_acc = 0usize;
                for c in tv {
                    if !matches!(c, Cell::Null) {
                        cnt_acc += 1;
                    }
                    prefix_cnt.push(cnt_acc);
                }
                let mut counts = Vec::with_capacity(n);
                for i in 0..n {
                    let (s, e) = get_frame_bounds(i, n, frame, has_order);
                    if s <= e && s < n {
                        let end_idx = e.min(n - 1);
                        let count = prefix_cnt[end_idx + 1] - prefix_cnt[s];
                        counts.push(Cell::Int(count as i64));
                    } else {
                        counts.push(Cell::Int(0));
                    }
                }
                counts
            } else {
                let mut counts = Vec::with_capacity(n);
                for i in 0..n {
                    let (s, e) = get_frame_bounds(i, n, frame, has_order);
                    let count = if s <= e && s < n {
                        e.min(n - 1) - s + 1
                    } else {
                        0
                    };
                    counts.push(Cell::Int(count as i64));
                }
                counts
            }
        }
    }
}

#[inline]
fn recycle_ref_vec<'a, 'b, T>(mut v: Vec<&'a T>) -> Vec<&'b T> {
    v.clear();
    v.into_iter().map(|_| unreachable!()).collect()
}

pub(crate) fn eval_window(
    rows: Vec<Row>,
    func: &WindowFunction,
    field: Option<&Field>,
    by: &[Field],
    order: &[WindowSort],
    frame: Option<&WindowFrame>,
    as_field: &str,
) -> Vec<Row> {
    if rows.is_empty() {
        return rows;
    }

    static NULL_CELL: Cell = Cell::Null;
    let total_len = rows.len();
    let by_keys: smallvec::SmallVec<[std::borrow::Cow<'_, str>; 4]> =
        by.iter().map(|b| b.as_cow_str()).collect();
    let order_keys: smallvec::SmallVec<[(std::borrow::Cow<'_, str>, bool); 4]> =
        order.iter().map(|s| (s.field.as_cow_str(), s.desc)).collect();
    let target_field: Option<std::borrow::Cow<'_, str>> = field.map(|f| f.as_cow_str());

    let partition_vec: Vec<Vec<Row>> = if by_keys.is_empty() {
        vec![rows]
    } else if by_keys.len() == 1 {
        let mut by_slot = crate::store::row::FieldSlot::new(by_keys[0].as_ref());
        let mut partition_of: FxHashMap<Cell, usize> = FxHashMap::default();
        let mut partitions: Vec<Vec<Row>> = Vec::new();
        for row in rows {
            let cell_ref = by_slot.get(&row).unwrap_or(&NULL_CELL);
            let slot = match partition_of.get(cell_ref) {
                Some(&slot) => slot,
                None => {
                    let slot = partitions.len();
                    partition_of.insert(cell_ref.clone_for_borrow(), slot);
                    partitions.push(Vec::new());
                    slot
                }
            };
            partitions[slot].push(row);
        }
        partitions
    } else {
        // Key -> position of its partition, partitions kept in first-seen order. Each key is
        // stored once, and rows move straight into their partition.
        let mut by_slots: smallvec::SmallVec<[crate::store::row::FieldSlot<'_>; 4]> = by_keys
            .iter()
            .map(|bk| crate::store::row::FieldSlot::new(bk.as_ref()))
            .collect();
        let mut partition_of: FxHashMap<Vec<Cell>, usize> = FxHashMap::default();
        let mut partitions: Vec<Vec<Row>> = Vec::new();
        let mut key_buf: Vec<Cell> = Vec::with_capacity(by_keys.len());

        for row in rows {
            key_buf.clear();
            for bs in &mut by_slots {
                key_buf.push(bs.get(&row).map(Cell::clone_for_borrow).unwrap_or(Cell::Null));
            }
            let slot = match partition_of.get(&key_buf) {
                Some(&slot) => slot,
                None => {
                    let slot = partitions.len();
                    partition_of.insert(std::mem::take(&mut key_buf), slot);
                    key_buf = Vec::with_capacity(by_keys.len());
                    partitions.push(Vec::new());
                    slot
                }
            };
            partitions[slot].push(row);
        }
        partitions
    };

    let mut order_slots: smallvec::SmallVec<[(crate::store::row::FieldSlot<'_>, bool); 4]> =
        order_keys
            .iter()
            .map(|(ok, desc)| (crate::store::row::FieldSlot::new(ok.as_ref()), *desc))
            .collect();
    let mut target_slot = target_field
        .as_deref()
        .map(crate::store::row::FieldSlot::new);

    let needs_ties = matches!(func, WindowFunction::Rank | WindowFunction::DenseRank);
    let mut keys_buf: Vec<&'static Cell> = Vec::new();
    let mut order_buf: Vec<u32> = Vec::new();
    let mut ties_buf: Vec<bool> = Vec::new();
    let mut tv_buf: Vec<&'static Cell> = Vec::new();
    let mut scratch_part: Vec<Row> = Vec::new();

    // Cache `(src_shape, pos, overwrite_in_place, dst_shape)` for O(1) `as_field` insertion.
    #[allow(clippy::type_complexity)]
    let mut shape_cache: smallvec::SmallVec<
        [(
            Arc<crate::store::RowShape>,
            usize,
            bool,
            Arc<crate::store::RowShape>,
        ); 4],
    > = smallvec::SmallVec::new();

    let mut result_rows = Vec::with_capacity(total_len);

    for mut part in partition_vec {
        let part_len = part.len();
        ties_buf.clear();
        if !order_slots.is_empty() && part_len > 1 {
            let k = order_slots.len();
            {
                let mut keys: Vec<&Cell> = recycle_ref_vec(keys_buf);
                keys.reserve(part_len * k);
                for row in &part {
                    for (slot, _) in &mut order_slots {
                        keys.push(slot.get(row).unwrap_or(&NULL_CELL));
                    }
                }
                order_buf.clear();
                order_buf.extend(0..part_len as u32);
                order_buf.sort_by(|&a, &b| {
                    let (a, b) = (a as usize * k, b as usize * k);
                    for (j, (_, desc)) in order_slots.iter().enumerate() {
                        let ord = cmp_sort(keys[a + j], keys[b + j]);
                        let ord = if *desc { ord.reverse() } else { ord };
                        if ord != std::cmp::Ordering::Equal {
                            return ord;
                        }
                    }
                    std::cmp::Ordering::Equal
                });
                if needs_ties {
                    ties_buf.reserve(order_buf.len());
                    ties_buf.push(false);
                    for w in order_buf.windows(2) {
                        let (a, b) = (w[0] as usize * k, w[1] as usize * k);
                        ties_buf.push((0..k).all(|j| {
                            cmp_sort(keys[a + j], keys[b + j]) == std::cmp::Ordering::Equal
                        }));
                    }
                }
                keys_buf = recycle_ref_vec(keys);
            }
            scratch_part.clear();
            scratch_part.reserve(part_len);
            for &idx in &order_buf {
                scratch_part.push(std::mem::take(&mut part[idx as usize]));
            }
            std::mem::swap(&mut part, &mut scratch_part);
        }

        let vals = {
            let mut tv: Vec<&Cell> = recycle_ref_vec(tv_buf);
            if let Some(ref mut tslot) = target_slot {
                tv.reserve(part_len);
                for r in &part {
                    tv.push(tslot.get(r).unwrap_or(&NULL_CELL));
                }
            }
            let out = window_values_len(
                func,
                &tv,
                part_len,
                &ties_buf,
                !order_slots.is_empty(),
                target_slot.is_some(),
                frame,
            );
            tv_buf = recycle_ref_vec(tv);
            out
        };

        for (mut row, val) in part.into_iter().zip(vals) {
            let hit = row
                .shape_arc()
                .and_then(|s| shape_cache.iter().position(|(cs, _, _, _)| Arc::ptr_eq(cs, s)));
            if let Some(ci) = hit {
                let (_, pos, overwrite, ref dst_shape) = shape_cache[ci];
                if overwrite {
                    row.cells_mut()[pos] = val;
                    result_rows.push(row);
                } else {
                    let new_len = row.len() + 1;
                    let mut old_cells = row.into_values();
                    let mut new_cells: smallvec::SmallVec<[Cell; 8]> =
                        smallvec::SmallVec::with_capacity(new_len);
                    for _ in 0..pos {
                        if let Some(c) = old_cells.next() {
                            new_cells.push(c);
                        }
                    }
                    new_cells.push(val);
                    new_cells.extend(old_cells);
                    result_rows.push(Row::from_parts(Arc::clone(dst_shape), new_cells));
                }
            } else if let Some(s) = row.shape_arc() {
                let src_shape = Arc::clone(s);
                if let Some(pos) = src_shape.names().iter().position(|n| n == as_field) {
                    row.cells_mut()[pos] = val;
                    if shape_cache.len() == 4 {
                        shape_cache.remove(0);
                    }
                    shape_cache.push((Arc::clone(&src_shape), pos, true, src_shape));
                    result_rows.push(row);
                } else {
                    row.insert(as_field, val);
                    let dst_shape = Arc::clone(row.shape_arc().expect("inserted field"));
                    let insert_at = dst_shape
                        .names()
                        .iter()
                        .position(|n| n == as_field)
                        .expect("inserted field present");
                    if shape_cache.len() == 4 {
                        shape_cache.remove(0);
                    }
                    shape_cache.push((src_shape, insert_at, false, dst_shape));
                    result_rows.push(row);
                }
            } else {
                row.insert(as_field, val);
                result_rows.push(row);
            }
        }
    }

    result_rows
}

fn eval_window_extremum(
    // Target-field value of every row in the window (`Cell::Null` when absent).
    tv: &[&Cell],
    frame: Option<&WindowFrame>,
    has_order: bool,
    is_max: bool,
) -> Vec<Cell> {
    let n = tv.len();
    let is_better = |cand: &Cell, prev: &Cell| -> bool {
        if is_max {
            cmp_sort(cand, prev).is_gt()
        } else {
            cmp_sort(cand, prev).is_lt()
        }
    };
    if frame.is_none() && !has_order {
        let mut best: Option<Cell> = None;
        for &c in tv {
            if !matches!(c, Cell::Null) {
                best = Some(match best {
                    None => c.clone_for_borrow(),
                    Some(prev) => {
                        if is_better(c, &prev) {
                            c.clone_for_borrow()
                        } else {
                            prev
                        }
                    }
                });
            }
        }
        return vec![best.unwrap_or(Cell::Null); n];
    }
    if frame.is_none() && has_order {
        let mut out = Vec::with_capacity(n);
        let mut best: Option<Cell> = None;
        for &c in tv {
            if !matches!(c, Cell::Null) {
                best = Some(match best {
                    None => c.clone_for_borrow(),
                    Some(prev) => {
                        if is_better(c, &prev) {
                            c.clone_for_borrow()
                        } else {
                            prev
                        }
                    }
                });
            }
            out.push(best.clone().unwrap_or(Cell::Null));
        }
        return out;
    }

    // O(N) monotonic deque for non-decreasing sliding frames
    let mut out = Vec::with_capacity(n);
    let mut deque: std::collections::VecDeque<usize> = std::collections::VecDeque::new();
    let mut pushed_up_to = 0usize;
    let mut prev_s = 0usize;
    for i in 0..n {
        let (s, e) = get_frame_bounds(i, n, frame, has_order);
        if s <= e && s < n {
            let end_idx = e.min(n - 1);
            if s < prev_s || end_idx + 1 < pushed_up_to {
                // Non-monotonic frame fallback
                deque.clear();
                pushed_up_to = s;
            }
            prev_s = s;
            while pushed_up_to <= end_idx {
                let c = tv[pushed_up_to];
                if !matches!(c, Cell::Null) {
                    while let Some(&back_idx) = deque.back() {
                        if !is_better(tv[back_idx], c) {
                            deque.pop_back();
                        } else {
                            break;
                        }
                    }
                    deque.push_back(pushed_up_to);
                }
                pushed_up_to += 1;
            }
            while deque.front().is_some_and(|&front_idx| front_idx < s) {
                deque.pop_front();
            }
            let cell = deque
                .front()
                .map(|&idx| tv[idx].clone_for_borrow())
                .unwrap_or(Cell::Null);
            out.push(cell);
        } else {
            out.push(Cell::Null);
        }
    }
    out
}
