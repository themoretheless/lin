use std::sync::Arc;

use rustc_hash::FxHashMap;

use crate::ast::{CmpOp, Field, Pred, Value, WindowSort};
use crate::store::{Cell, Row, row_text};
use crate::window::{FrameBound, WindowFrame, WindowFunction};

struct RegexCacheState {
    last: Option<(String, String, Option<Arc<regex::Regex>>)>,
    map: FxHashMap<(String, String), Option<Arc<regex::Regex>>>,
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
            if values.iter().any(|v| matches!(v, Value::Param(_))) {
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
            } else {
                pred.clone()
            }
        }
        _ => pred.clone(),
    }
}

pub(crate) fn bind_record_params(
    record: &crate::ast::Record,
    params: &[(&str, Cell)],
) -> crate::ast::Record {
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
        Pred::IsNull { field, negated } => {
            let is_null = matches!(field_cell(row, field), Cell::Null);
            if *negated { !is_null } else { is_null }
        }
        Pred::In { field, values } => {
            let left = field_cell(row, field);
            values.iter().any(|v| cell_eq_val(left, v, now))
        }
        Pred::Cmp { field, op, value } => cmp_cell(field_cell(row, field), value, *op, now),
        Pred::Has { field, ci, needle } => field_cell(row, field)
            .text()
            .is_some_and(|s| has_word(s, needle, *ci)),
        Pred::Contains { field, needle } => field_cell(row, field)
            .text()
            .is_some_and(|s| text_contains(s, needle)),
        Pred::Regex {
            field,
            pattern,
            flags,
        } => {
            let Some(text) = field_cell(row, field).text() else {
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
                let key = (pattern.clone(), flags.clone());
                let compiled = state
                    .map
                    .entry(key.clone())
                    .or_insert_with(|| {
                        let mut b = regex::RegexBuilder::new(pattern);
                        b.size_limit(10 * 1024 * 1024);
                        if flags.contains('i') {
                            b.case_insensitive(true);
                        }
                        b.build().ok().map(Arc::new)
                    })
                    .clone();
                let matched = compiled.as_ref().is_some_and(|re| re.is_match(text));
                state.last = Some((key.0, key.1, compiled));
                matched
            })
        }
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
    // Case-insensitive without allocating per-token Vec.
    let mut nbuf = String::new();
    for ch in needle.chars() {
        for c in ch.to_lowercase() {
            nbuf.push(c);
        }
    }
    let mut wbuf = String::new();
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
        if wbuf == nbuf {
            return true;
        }
    }
    false
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

pub(crate) fn project(rows: &[Row], fields: &[Field]) -> Vec<Row> {
    let mut result = Vec::with_capacity(rows.len());
    let keys: Vec<String> = fields.iter().map(|f| f.as_str()).collect();
    for r in rows {
        let mut out = crate::store::acquire_row();
        for k in &keys {
            let cell = r.get(k).map(Cell::clone_for_borrow).unwrap_or(Cell::Null);
            out.insert(k.clone(), cell);
        }
        result.push(out);
    }
    result
}

pub(crate) fn sort_rows_top_k(
    rows: &mut Vec<Row>,
    field: &Field,
    desc: bool,
    top_k: Option<usize>,
) {
    let k = field.as_str();
    let n = rows.len();
    if n <= 1 {
        return;
    }
    if let Some(0) = top_k {
        rows.clear();
        return;
    }

    if n <= 32 {
        let cmp = |a: &Row, b: &Row| {
            let av = a.get(&k).unwrap_or(&Cell::Null);
            let bv = b.get(&k).unwrap_or(&Cell::Null);
            let ord = cmp_sort(av, bv);
            if desc { ord.reverse() } else { ord }
        };
        if let Some(limit) = top_k {
            if rows.len() > limit {
                rows.select_nth_unstable_by(limit - 1, cmp);
                rows.truncate(limit);
            }
        }
        rows.sort_by(cmp);
        return;
    }

    // Decorate: extract sort keys once in O(N) instead of O(N log N) BTree lookups
    let mut keys: Vec<(Cell, usize)> = rows
        .iter()
        .enumerate()
        .map(|(i, r)| {
            (
                r.get(&k).map(Cell::clone_for_borrow).unwrap_or(Cell::Null),
                i,
            )
        })
        .collect();

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

    let total_len = rows.len();
    let by_keys: Vec<std::borrow::Cow<'_, str>> = by.iter().map(|b| b.as_cow_str()).collect();
    let order_keys: Vec<(std::borrow::Cow<'_, str>, bool)> = order
        .iter()
        .map(|s| (s.field.as_cow_str(), s.desc))
        .collect();
    let target_field: Option<std::borrow::Cow<'_, str>> = field.map(|f| f.as_cow_str());
    let tf: &str = target_field.as_deref().unwrap_or_default();

    let partition_vec: Vec<Vec<Row>> = if by_keys.is_empty() {
        vec![rows]
    } else {
        // Key -> position of its partition, partitions kept in first-seen order. Each key is
        // stored once, and rows move straight into their partition.
        let mut partition_of: FxHashMap<Vec<Cell>, usize> = FxHashMap::default();
        let mut partitions: Vec<Vec<Row>> = Vec::new();
        let mut key_buf: Vec<Cell> = Vec::with_capacity(by_keys.len());

        for row in rows {
            key_buf.clear();
            for bk in &by_keys {
                key_buf.push(
                    row.get(bk.as_ref())
                        .map(Cell::clone_for_borrow)
                        .unwrap_or(Cell::Null),
                );
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

    let mut result_rows = Vec::with_capacity(total_len);

    for mut part in partition_vec {
        // For `rank` / `dense_rank`: whether row `i` ties with row `i - 1` on every sort key,
        // read off the key buffer built for sorting instead of re-probing both rows' maps.
        let needs_ties = matches!(func, WindowFunction::Rank | WindowFunction::DenseRank);
        let mut ties: Vec<bool> = Vec::new();
        if !order_keys.is_empty() && part.len() > 1 {
            // Look every sort key up once, then sort row indices over the flat key buffer
            // instead of walking each row's map inside the comparator.
            let k = order_keys.len();
            let order: Vec<u32> = {
                static NULL: Cell = Cell::Null;
                let mut keys: Vec<&Cell> = Vec::with_capacity(part.len() * k);
                for row in &part {
                    for (ok, _) in &order_keys {
                        keys.push(row.get(ok.as_ref()).unwrap_or(&NULL));
                    }
                }
                let mut order: Vec<u32> = (0..part.len() as u32).collect();
                // Stable, like the row sort it replaces.
                order.sort_by(|&a, &b| {
                    let (a, b) = (a as usize * k, b as usize * k);
                    for (j, (_, desc)) in order_keys.iter().enumerate() {
                        let ord = cmp_sort(keys[a + j], keys[b + j]);
                        let ord = if *desc { ord.reverse() } else { ord };
                        if ord != std::cmp::Ordering::Equal {
                            return ord;
                        }
                    }
                    std::cmp::Ordering::Equal
                });
                if needs_ties {
                    ties = Vec::with_capacity(order.len());
                    ties.push(false);
                    for w in order.windows(2) {
                        let (a, b) = (w[0] as usize * k, w[1] as usize * k);
                        ties.push((0..k).all(|j| {
                            cmp_sort(keys[a + j], keys[b + j]) == std::cmp::Ordering::Equal
                        }));
                    }
                }
                order
            };
            let mut slots: Vec<Option<Row>> = part.into_iter().map(Some).collect();
            part = order
                .into_iter()
                .filter_map(|i| slots[i as usize].take())
                .collect();
        }

        let n = part.len();
        let vals: Vec<Cell> = match func {
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
                let off = *offset;
                (0..n)
                    .map(|i| {
                        if i + off < n {
                            part[i + off].get(tf).cloned().unwrap_or(Cell::Null)
                        } else {
                            Cell::Null
                        }
                    })
                    .collect()
            }
            WindowFunction::Lag { offset } => {
                let off = *offset;
                (0..n)
                    .map(|i| {
                        if i >= off {
                            part[i - off].get(tf).cloned().unwrap_or(Cell::Null)
                        } else {
                            Cell::Null
                        }
                    })
                    .collect()
            }
            WindowFunction::FirstValue => {
                if let Some(fr) = frame {
                    (0..n)
                        .map(|i| {
                            let (s, e) = get_frame_bounds(i, n, Some(fr), !order_keys.is_empty());
                            if s <= e && s < n {
                                part[s].get(tf).cloned().unwrap_or(Cell::Null)
                            } else {
                                Cell::Null
                            }
                        })
                        .collect()
                } else {
                    let first_val = part
                        .first()
                        .and_then(|r| r.get(tf))
                        .cloned()
                        .unwrap_or(Cell::Null);
                    vec![first_val; n]
                }
            }
            WindowFunction::LastValue => {
                if let Some(fr) = frame {
                    (0..n)
                        .map(|i| {
                            let (s, e) = get_frame_bounds(i, n, Some(fr), !order_keys.is_empty());
                            if s <= e && s < n {
                                let end_idx = e.min(n - 1);
                                part[end_idx].get(tf).cloned().unwrap_or(Cell::Null)
                            } else {
                                Cell::Null
                            }
                        })
                        .collect()
                } else {
                    let last_val = part
                        .last()
                        .and_then(|r| r.get(tf))
                        .cloned()
                        .unwrap_or(Cell::Null);
                    vec![last_val; n]
                }
            }
            WindowFunction::Sum => {
                let mut has_float = false;
                for r in &part {
                    if let Some(c) = r.get(tf) {
                        if matches!(c, Cell::Float(_)) {
                            has_float = true;
                            break;
                        }
                    }
                }
                if has_float {
                    let mut prefix = Vec::with_capacity(n + 1);
                    prefix.push(0.0f64);
                    let mut acc = 0.0f64;
                    for r in &part {
                        if let Some(c) = r.get(tf)
                            && let Some(v) = c.as_f64()
                        {
                            acc += v;
                        }
                        prefix.push(acc);
                    }
                    let mut sums = Vec::with_capacity(n);
                    for i in 0..n {
                        let (s, e) = get_frame_bounds(i, n, frame, !order_keys.is_empty());
                        if s <= e && s < n {
                            let end_idx = e.min(n - 1);
                            let sum = prefix[end_idx + 1] - prefix[s];
                            sums.push(Cell::Float(sum));
                        } else {
                            sums.push(Cell::Float(0.0));
                        }
                    }
                    sums
                } else {
                    let mut prefix = Vec::with_capacity(n + 1);
                    prefix.push(0i64);
                    let mut acc = 0i64;
                    for r in &part {
                        if let Some(c) = r.get(tf)
                            && let Some(v) = c.as_int()
                        {
                            acc += v;
                        }
                        prefix.push(acc);
                    }
                    let mut sums = Vec::with_capacity(n);
                    for i in 0..n {
                        let (s, e) = get_frame_bounds(i, n, frame, !order_keys.is_empty());
                        if s <= e && s < n {
                            let end_idx = e.min(n - 1);
                            let sum = prefix[end_idx + 1] - prefix[s];
                            sums.push(Cell::Int(sum));
                        } else {
                            sums.push(Cell::Int(0));
                        }
                    }
                    sums
                }
            }
            WindowFunction::Avg => {
                let mut prefix_sum = Vec::with_capacity(n + 1);
                let mut prefix_cnt = Vec::with_capacity(n + 1);
                prefix_sum.push(0.0f64);
                prefix_cnt.push(0usize);
                let mut sum_acc = 0.0f64;
                let mut cnt_acc = 0usize;
                for r in &part {
                    if let Some(c) = r.get(tf)
                        && let Some(v) = c.as_f64()
                    {
                        sum_acc += v;
                        cnt_acc += 1;
                    }
                    prefix_sum.push(sum_acc);
                    prefix_cnt.push(cnt_acc);
                }
                let mut avgs = Vec::with_capacity(n);
                for i in 0..n {
                    let (s, e) = get_frame_bounds(i, n, frame, !order_keys.is_empty());
                    if s <= e && s < n {
                        let end_idx = e.min(n - 1);
                        let sum = prefix_sum[end_idx + 1] - prefix_sum[s];
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
                eval_window_extremum(&part, tf, frame, !order_keys.is_empty(), false)
            }
            WindowFunction::Max => {
                eval_window_extremum(&part, tf, frame, !order_keys.is_empty(), true)
            }
            WindowFunction::Count => {
                if target_field.is_some() {
                    let mut prefix_cnt = Vec::with_capacity(n + 1);
                    prefix_cnt.push(0usize);
                    let mut cnt_acc = 0usize;
                    for r in &part {
                        if r.get(tf).is_some_and(|c| !matches!(c, Cell::Null)) {
                            cnt_acc += 1;
                        }
                        prefix_cnt.push(cnt_acc);
                    }
                    let mut counts = Vec::with_capacity(n);
                    for i in 0..n {
                        let (s, e) = get_frame_bounds(i, n, frame, !order_keys.is_empty());
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
                        let (s, e) = get_frame_bounds(i, n, frame, !order_keys.is_empty());
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
        };

        for (i, mut row) in part.into_iter().enumerate() {
            row.insert(as_field.to_string(), vals[i].clone());
            result_rows.push(row);
        }
    }

    result_rows
}

fn eval_window_extremum(
    part: &[Row],
    tf: &str,
    frame: Option<&WindowFrame>,
    has_order: bool,
    is_max: bool,
) -> Vec<Cell> {
    let n = part.len();
    let is_better = |cand: &Cell, prev: &Cell| -> bool {
        if is_max {
            cmp_sort(cand, prev).is_gt()
        } else {
            cmp_sort(cand, prev).is_lt()
        }
    };
    if frame.is_none() && !has_order {
        let mut best: Option<Cell> = None;
        for r in part {
            if let Some(c) = r.get(tf).filter(|c| !matches!(c, Cell::Null)) {
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
        for r in part {
            if let Some(c) = r.get(tf).filter(|c| !matches!(c, Cell::Null)) {
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
                if let Some(c) = part[pushed_up_to]
                    .get(tf)
                    .filter(|c| !matches!(c, Cell::Null))
                {
                    while let Some(&back_idx) = deque.back() {
                        if let Some(back_cell) = part[back_idx].get(tf)
                            && !is_better(back_cell, c)
                        {
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
                .and_then(|&idx| part[idx].get(tf).map(Cell::clone_for_borrow))
                .unwrap_or(Cell::Null);
            out.push(cell);
        } else {
            out.push(Cell::Null);
        }
    }
    out
}
