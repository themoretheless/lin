use std::collections::{BTreeMap, BTreeSet};

use crate::ast::*;
use crate::catalog::{Catalog, Type};
use crate::error::Error;

#[expect(dead_code)]
pub fn check(stmt: &Stmt, cat: &Catalog) -> Result<(), Error> {
    check_in(stmt, cat, &Bindings::new())
}

pub fn check_program(stmts: &[Stmt], cat: &mut Catalog) -> Result<Bindings, Error> {
    let mut env = Bindings::new();
    for stmt in stmts {
        check_in(stmt, cat, &env)?;
        if let Stmt::Decl(d) = stmt {
            cat.apply_decl(d)?;
        }
        if let Stmt::Let { name, query } = stmt {
            if cat.collection(name).is_some() || name == "catalog" {
                return Err(Error::new(format!("cannot bind over collection: {name}")));
            }
            if env.contains(name) {
                return Err(Error::new(format!("binding exists: {name}")));
            }
            let scope = check_query(query, cat, &env)?;
            env.insert(name.clone(), scope);
        }
    }
    Ok(env)
}

fn check_in(stmt: &Stmt, cat: &Catalog, env: &Bindings) -> Result<(), Error> {
    match stmt {
        Stmt::Query(q) => {
            check_query(q, cat, env)?;
        }
        Stmt::AppendFacts { records } => {
            let facts = cat
                .collection("facts")
                .ok_or_else(|| Error::new("unknown collection: facts"))?;
            if !facts.append_only {
                return Err(Error::new("facts is not append-only"));
            }
            if records.is_empty() {
                return Err(Error::new("empty list"));
            }
            for record in records {
                check_record(record, "facts", cat, false)?;
            }
        }
        Stmt::AppendEdges { edges } => {
            if edges.is_empty() {
                return Err(Error::new("empty list"));
            }
            for e in edges {
                if cat.rel(&e.rel).is_none() {
                    return Err(Error::new(format!("unknown rel: {}", e.rel)));
                }
                check_value_as(&e.from, Type::Id, cat)?;
                check_value_as(&e.to, Type::Id, cat)?;
            }
        }
        Stmt::Insert {
            collection,
            records,
            edges,
        } => {
            let col = cat
                .collection(collection)
                .ok_or_else(|| Error::new(format!("unknown collection: {collection}")))?;
            if col.append_only {
                return Err(Error::new(format!(
                    "{collection} is append-only; use `append`"
                )));
            }
            if records.is_empty() {
                return Err(Error::new("empty list"));
            }
            if records.len() != 1 && !edges.is_empty() {
                return Err(Error::new("with not allowed on bulk insert"));
            }
            for record in records {
                check_record(record, collection, cat, false)?;
            }
            for e in edges {
                if cat.rel(&e.rel).is_none() {
                    return Err(Error::new(format!("unknown rel: {}", e.rel)));
                }
            }
        }
        Stmt::Update {
            collection,
            pred,
            cas,
            cas_each,
            record,
        } => {
            if cas.is_none() && !*cas_each {
                return Err(Error::new("update requires cas"));
            }
            let col = cat
                .collection(collection)
                .ok_or_else(|| Error::new(format!("unknown collection: {collection}")))?;
            if col.append_only {
                return Err(Error::new(format!(
                    "{collection} is append-only; cannot update"
                )));
            }
            let scope = scope_of(collection, cat)?;
            if let Some(p) = pred {
                check_pred(p, &scope, cat)?;
            }
            check_record(record, collection, cat, true)?;
        }
        Stmt::Reembed { collection, to } => {
            let col = cat
                .collection(collection)
                .ok_or_else(|| Error::new(format!("unknown collection: {collection}")))?;
            if !col.has_vec() {
                return Err(Error::new(format!(
                    "reembed requires embedding: {collection}"
                )));
            }
            if let Some(t) = to
                && t.dim <= 0
            {
                return Err(Error::new("embed dim must be positive"));
            }
        }
        Stmt::Delete {
            collection,
            pred,
            cas,
            cas_each,
        } => {
            let col = cat
                .collection(collection)
                .ok_or_else(|| Error::new(format!("unknown collection: {collection}")))?;
            if !col.append_only && cas.is_none() && !*cas_each {
                return Err(Error::new("delete requires cas"));
            }
            let scope = scope_of(collection, cat)?;
            check_pred(pred, &scope, cat)?;
        }
        Stmt::DeleteEdge { rel, from, to } => {
            if cat.rel(rel).is_none() {
                return Err(Error::new(format!("unknown rel: {rel}")));
            }
            check_value_as(from, Type::Id, cat)?;
            check_value_as(to, Type::Id, cat)?;
        }
        Stmt::Let { name, query } => {
            if cat.collection(name).is_some() || name == "catalog" {
                return Err(Error::new(format!("cannot bind over collection: {name}")));
            }
            check_query(query, cat, env)?;
        }
        Stmt::Decl(d) => check_decl(d, cat)?,
        Stmt::IdbSlice { query } => {
            check_query(query, cat, env)?;
        }
        Stmt::IdbPull { .. } | Stmt::IdbPush | Stmt::Snapshot { .. } | Stmt::Restore { .. } | Stmt::Pin { .. } | Stmt::Unpin { .. } => {}
    }
    Ok(())
}

fn check_decl(d: &Decl, cat: &Catalog) -> Result<(), Error> {
    match d {
        Decl::Col { name, fields, .. } => {
            if cat.collection(name).is_some() {
                return Err(Error::new(format!("collection exists: {name}")));
            }
            for (_, ty) in fields {
                match ty {
                    TypeExpr::Named(n) => {
                        if !matches!(
                            n.as_str(),
                            "text"
                                | "i64"
                                | "f64"
                                | "bool"
                                | "time"
                                | "dur"
                                | "id"
                                | "uri"
                                | "rel"
                                | "var"
                        ) {
                            return Err(Error::new(format!("unknown type: {n}")));
                        }
                    }
                    TypeExpr::Vec { dim, .. } => {
                        if *dim <= 0 {
                            return Err(Error::new("vec dim must be positive"));
                        }
                    }
                }
            }
        }
        Decl::Rel { name, .. } => {
            if cat.rel(name).is_some() {
                return Err(Error::new(format!("rel exists: {name}")));
            }
        }
        Decl::RelReverse { name, of, .. } => {
            if cat.rel(name).is_some() {
                return Err(Error::new(format!("rel exists: {name}")));
            }
            if cat.rel(of).is_none() {
                return Err(Error::new(format!("unknown rel: {of}")));
            }
        }
        Decl::Fk {
            from_col,
            from_field,
            to_col,
            to_field,
        } => {
            let from = cat
                .collection(from_col)
                .ok_or_else(|| Error::new(format!("unknown collection: {from_col}")))?;
            let to = cat
                .collection(to_col)
                .ok_or_else(|| Error::new(format!("unknown collection: {to_col}")))?;
            if from.field(from_field).is_none() {
                return Err(Error::new(format!(
                    "unknown field: {from_col}.{from_field}"
                )));
            }
            if to.field(to_field).is_none() {
                return Err(Error::new(format!("unknown field: {to_col}.{to_field}")));
            }
        }
        Decl::Guard {
            collection,
            field,
            pred,
        } => {
            let scope = scope_of(collection, cat)?;
            if scope.ty_of(field).is_none() {
                return Err(Error::new(format!("unknown field: {collection}.{field}")));
            }
            check_pred(pred, &scope, cat)?;
        }
        Decl::Index {
            collection,
            fields,
            unique: _,
        } => {
            let col = cat
                .collection(collection)
                .ok_or_else(|| Error::new(format!("unknown collection: {collection}")))?;
            if fields.is_empty() {
                return Err(Error::new("empty index"));
            }
            for f in fields {
                if col.field(f).is_none() {
                    return Err(Error::new(format!("unknown field: {f}")));
                }
            }
            let label = format!("{}[{}]", collection, fields.join(","));
            if cat.indexes.contains_key(&label) {
                return Err(Error::new(format!("index exists: {label}")));
            }
        }
    }
    Ok(())
}

fn check_query(q: &Query, cat: &Catalog, env: &Bindings) -> Result<Scope, Error> {
    let mut scope = match &q.source {
        Source::Collection(name) => {
            if cat.collection(name).is_some() {
                scope_of(name, cat)?
            } else if let Some(s) = env.get(name) {
                s.clone()
            } else {
                return Err(Error::new(format!("unknown collection: {name}")));
            }
        }
        Source::Page(_) => scope_of("docs", cat)?,
        Source::Catalog => scope_of("catalog", cat)?,
    };
    for step in &q.steps {
        match step {
            Step::Filter(pred) => check_pred(pred, &scope, cat)?,
            Step::Project(fields) => {
                let mut next = BTreeMap::new();
                for f in fields {
                    let ty = require_field(&scope, f)?;
                    next.insert(f.as_str(), ty);
                }
                scope.fields = next;
            }
            Step::Join { collection, on, .. } => {
                if cat.collection(collection).is_none() {
                    return Err(Error::new(format!("unknown collection: {collection}")));
                }
                if scope.ty_of(on).is_none()
                    && scope.ty_of(&format!("{}.{on}", scope.primary)).is_none()
                {
                    return Err(Error::new(format!("unknown field: {on}")));
                }
                if cat.find_fk(&scope.primary, on, collection).is_none() {
                    return Err(Error::new(format!(
                        "join requires fk: {}.{} → {}.id",
                        scope.primary, on, collection
                    )));
                }
                scope.add_join(collection, cat)?;
            }
            Step::Hop { rel, depth } => {
                if cat.rel(rel).is_none() {
                    return Err(Error::new(format!("unknown rel: {rel}")));
                }
                match depth.unwrap_or(1) {
                    1..=3 => {}
                    ..=0 => return Err(Error::new("hop depth must be ≥ 1")),
                    4.. => return Err(Error::new("hop depth max 3")),
                }
            }
            Step::Graph { rel, depth } => {
                if cat.rel(rel).is_none() {
                    return Err(Error::new(format!("unknown rel: {rel}")));
                }
                match depth.unwrap_or(1) {
                    1..=3 => {}
                    ..=0 => return Err(Error::new("graph depth must be ≥ 1")),
                    4.. => return Err(Error::new("graph depth max 3")),
                }
                scope = Scope::edges();
            }
            Step::Match { start, hops } => {
                if hops.is_empty() {
                    return Err(Error::new("match requires at least one hop"));
                }
                if hops.len() > 3 {
                    return Err(Error::new("match hops max 3"));
                }
                let mut names = BTreeSet::new();
                if let Some(s) = start {
                    if !names.insert(s.clone()) {
                        return Err(Error::new(format!("duplicate match bind: {s}")));
                    }
                    let primary = scope.primary.clone();
                    scope.add_bind(s, &primary, cat)?;
                }
                for h in hops {
                    if cat.rel(&h.rel).is_none() {
                        return Err(Error::new(format!("unknown rel: {}", h.rel)));
                    }
                    if h.min_depth < 1 {
                        return Err(Error::new("match depth must be ≥ 1"));
                    }
                    if h.max_depth > 3 {
                        return Err(Error::new("match depth max 3"));
                    }
                    if h.min_depth > h.max_depth {
                        return Err(Error::new("match depth min > max"));
                    }
                    if h.edge.is_some() && (h.min_depth != 1 || h.max_depth != 1) {
                        return Err(Error::new("edge bind requires depth 1"));
                    }
                    if let Some(e) = &h.edge {
                        if !names.insert(e.clone()) {
                            return Err(Error::new(format!("duplicate match bind: {e}")));
                        }
                        scope.add_edge_bind(e);
                    }
                    if !names.insert(h.bind.clone()) {
                        return Err(Error::new(format!("duplicate match bind: {}", h.bind)));
                    }
                    let node_col = if cat.collection("docs").is_some() {
                        "docs".to_string()
                    } else {
                        scope.primary.clone()
                    };
                    scope.add_bind(&h.bind, &node_col, cat)?;
                }
            }
            Step::Search { mode, .. } => {
                let col = cat
                    .collection(&scope.primary)
                    .ok_or_else(|| Error::new(format!("unknown collection: {}", scope.primary)))?;
                match mode {
                    SearchMode::Lex => {
                        if !col.has_fts() {
                            return Err(Error::new(format!(
                                "search lex requires fts: {}",
                                scope.primary
                            )));
                        }
                    }
                    SearchMode::Vec => {
                        if !col.has_vec() {
                            return Err(Error::new(format!(
                                "search vec requires embedding: {}",
                                scope.primary
                            )));
                        }
                    }
                    SearchMode::Hybrid => {
                        if !col.has_fts() && !col.has_vec() {
                            return Err(Error::new(format!(
                                "search requires fts or embedding: {}",
                                scope.primary
                            )));
                        }
                        if !col.has_vec() {
                            return Err(Error::new(format!(
                                "search vec requires embedding: {}",
                                scope.primary
                            )));
                        }
                        if !col.has_fts() {
                            return Err(Error::new(format!(
                                "search lex requires fts: {}",
                                scope.primary
                            )));
                        }
                    }
                }
            }
            Step::Count { by } => {
                scope = match by {
                    Some(by) => {
                        require_field(&scope, by)?;
                        Scope::agg(&scope.primary, &by.as_str(), Type::I64, "hits")
                    }
                    None => Scope::agg_total(&scope.primary, Type::I64, "hits"),
                };
            }
            Step::Sum { field, by } => {
                let ty = require_field(&scope, field)?;
                if !ty.is_numeric() {
                    return Err(Error::new(format!(
                        "sum requires numeric field, found {}",
                        ty.name()
                    )));
                }
                require_field(&scope, by)?;
                scope = Scope::agg(&scope.primary, &by.as_str(), Type::F64, &field.as_str());
            }
            Step::Sort { field, .. } => {
                require_field(&scope, field)?;
            }
            Step::Skip { n } => {
                if *n < 0 {
                    return Err(Error::new("skip/offset must be ≥ 0"));
                }
            }
            Step::Take { n } => {
                if let Some(v) = n
                    && *v < 0
                {
                    return Err(Error::new("take must be ≥ 0"));
                }
            }
            Step::Union(rhs) => {
                let right = check_query(rhs, cat, env)?;
                check_union_cols(&scope, &right)?;
            }
        }
    }
    Ok(scope)
}

fn check_union_cols(left: &Scope, right: &Scope) -> Result<(), Error> {
    let l: Vec<&String> = left.fields.keys().collect();
    let r: Vec<&String> = right.fields.keys().collect();
    if l != r {
        return Err(Error::new(format!(
            "union columns mismatch: [{}] vs [{}]",
            l.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", "),
            r.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")
        )));
    }
    for (name, lty) in &left.fields {
        if let Some(rty) = right.fields.get(name)
            && !types_ok(*rty, *lty)
            && !types_ok(*lty, *rty)
        {
            return Err(Error::new(format!(
                "union type mismatch: {name} {} vs {}",
                lty.name(),
                rty.name()
            )));
        }
    }
    Ok(())
}

fn check_record(
    record: &Record,
    collection: &str,
    cat: &Catalog,
    is_update: bool,
) -> Result<(), Error> {
    let col = cat
        .collection(collection)
        .ok_or_else(|| Error::new(format!("unknown collection: {collection}")))?;
    let mut layer_raw = false;
    for (name, value) in &record.fields {
        let info = col
            .field(name)
            .ok_or_else(|| Error::new(format!("unknown field: {name}")))?;
        if name == "layer"
            && let Value::String(s) = value
        {
            layer_raw = s == "raw";
        }
        check_value_as(value, info.ty, cat)?;
        if is_update && cat.is_immutable_field(collection, name) {
            return Err(Error::new(format!("immutable field: {collection}.{name}")));
        }
    }
    if is_update && layer_raw && record.fields.iter().any(|(n, _)| n == "body") {
        return Err(Error::new(format!("immutable field: {collection}.body")));
    }
    Ok(())
}

fn check_value_as(value: &Value, expected: Type, cat: &Catalog) -> Result<(), Error> {
    let got = value_type(value, cat)?;
    if types_ok(got, expected) {
        Ok(())
    } else {
        Err(Error::new(format!(
            "type mismatch: expected {}, found {}",
            expected.name(),
            got.name()
        )))
    }
}

fn value_type(value: &Value, cat: &Catalog) -> Result<Type, Error> {
    match value {
        Value::String(_) => Ok(Type::Text),
        Value::Int(_) => Ok(Type::I64),
        Value::Float(_) => Ok(Type::F64),
        Value::Bool(_) => Ok(Type::Bool),
        Value::Now | Value::NowMinus(_) => Ok(Type::Time),
        Value::Duration(_) => Ok(Type::Dur),
        Value::Name(n) => {
            if cat.rel(n).is_some() {
                Ok(Type::Rel)
            } else {
                Err(Error::new(format!("unknown name: {n}")))
            }
        }
    }
}

fn types_ok(got: Type, expected: Type) -> bool {
    got == expected
        || matches!(
            (got, expected),
            (Type::Text, Type::Id | Type::Uri | Type::Rel | Type::Var)
                | (Type::Id, Type::Text | Type::Uri)
                | (Type::I64, Type::F64)
                | (Type::F64, Type::I64)
                | (Type::Rel, Type::Text)
        )
}

fn check_pred(pred: &Pred, scope: &Scope, cat: &Catalog) -> Result<(), Error> {
    match pred {
        Pred::And(a, b) | Pred::Or(a, b) => {
            check_pred(a, scope, cat)?;
            check_pred(b, scope, cat)?;
        }
        Pred::Cmp { field, op, value } => {
            let lty = require_field(scope, field)?;
            let rty = value_type(value, cat)?;
            match op {
                CmpOp::Eq | CmpOp::Ne => {
                    if !types_ok(rty, lty) && !types_ok(lty, rty) {
                        return Err(Error::new(format!(
                            "type mismatch: {} {} {}",
                            lty.name(),
                            op.as_str(),
                            rty.name()
                        )));
                    }
                }
                CmpOp::Gt | CmpOp::Lt | CmpOp::Ge | CmpOp::Le => {
                    let ok = matches!(
                        (lty, rty),
                        (Type::Time, Type::Time)
                            | (Type::Dur, Type::Dur)
                            | (Type::I64 | Type::F64, Type::I64 | Type::F64)
                    );
                    if !ok {
                        return Err(Error::new(format!(
                            "type mismatch: {} {} {}",
                            lty.name(),
                            op.as_str(),
                            rty.name()
                        )));
                    }
                }
            }
        }
        Pred::Has { field, .. } => {
            let ty = require_field(scope, field)?;
            if !ty.is_textish() {
                return Err(Error::new(format!(
                    "has requires text, found {} ({})",
                    ty.name(),
                    field.as_str()
                )));
            }
        }
        Pred::Contains { field, .. } | Pred::Regex { field, .. } => {
            let ty = require_field(scope, field)?;
            if !ty.is_textish() {
                return Err(Error::new(format!(
                    "`~` requires text, found {} ({})",
                    ty.name(),
                    field.as_str()
                )));
            }
        }
    }
    if let Pred::Regex { pattern, flags, .. } = pred {
        validate_regex(pattern, flags)?;
    }
    Ok(())
}

pub fn validate_regex(pattern: &str, flags: &str) -> Result<(), Error> {
    for f in flags.chars() {
        if f != 'i' {
            return Err(Error::new(format!("unknown regex flag `{f}`")));
        }
    }
    let mut b = regex::RegexBuilder::new(pattern);
    if flags.contains('i') {
        b.case_insensitive(true);
    }
    b.build()
        .map_err(|e| Error::new(format!("invalid regex literal: {e}")))?;
    Ok(())
}

fn require_field(scope: &Scope, field: &Field) -> Result<Type, Error> {
    scope
        .ty_of(&field.as_str())
        .ok_or_else(|| Error::new(format!("unknown field: {}", field.as_str())))
}

#[derive(Debug, Clone, Default)]
pub struct Bindings {
    scopes: BTreeMap<String, Scope>,
}

impl Bindings {
    pub fn new() -> Self {
        Self {
            scopes: BTreeMap::new(),
        }
    }

    pub fn insert(&mut self, name: String, scope: Scope) {
        self.scopes.insert(name, scope);
    }

    pub fn get(&self, name: &str) -> Option<&Scope> {
        self.scopes.get(name)
    }

    pub fn contains(&self, name: &str) -> bool {
        self.scopes.contains_key(name)
    }
}

#[derive(Debug, Clone)]
pub struct Scope {
    pub primary: String,
    fields: BTreeMap<String, Type>,
}

impl Scope {
    fn ty_of(&self, name: &str) -> Option<Type> {
        self.fields.get(name).copied()
    }

    fn add_join(&mut self, collection: &str, cat: &Catalog) -> Result<(), Error> {
        let col = cat
            .collection(collection)
            .ok_or_else(|| Error::new(format!("unknown collection: {collection}")))?;
        for (name, info) in &col.fields {
            self.fields.insert(format!("{collection}.{name}"), info.ty);
        }
        Ok(())
    }

    fn add_bind(&mut self, bind: &str, collection: &str, cat: &Catalog) -> Result<(), Error> {
        let col = cat
            .collection(collection)
            .ok_or_else(|| Error::new(format!("unknown collection: {collection}")))?;
        for (name, info) in &col.fields {
            self.fields.insert(format!("{bind}.{name}"), info.ty);
        }
        Ok(())
    }

    fn add_edge_bind(&mut self, bind: &str) {
        self.fields.insert(format!("{bind}.rel"), Type::Rel);
        self.fields.insert(format!("{bind}.from"), Type::Text);
        self.fields.insert(format!("{bind}.to"), Type::Text);
    }

    fn agg(primary: &str, by: &str, metric_ty: Type, metric: &str) -> Self {
        let mut fields = BTreeMap::new();
        fields.insert(by.to_string(), Type::Text);
        fields.insert(metric.to_string(), metric_ty);
        if by.contains('.') {
            fields.insert(by.to_string(), Type::Text);
        }
        Self {
            primary: primary.to_string(),
            fields,
        }
    }

    fn agg_total(primary: &str, metric_ty: Type, metric: &str) -> Self {
        let mut fields = BTreeMap::new();
        fields.insert(metric.to_string(), metric_ty);
        Self {
            primary: primary.to_string(),
            fields,
        }
    }

    fn edges() -> Self {
        let mut fields = BTreeMap::new();
        fields.insert("rel".into(), Type::Rel);
        fields.insert("from".into(), Type::Text);
        fields.insert("to".into(), Type::Text);
        Self {
            primary: "edges".into(),
            fields,
        }
    }
}

fn scope_of(collection: &str, cat: &Catalog) -> Result<Scope, Error> {
    let col = cat
        .collection(collection)
        .ok_or_else(|| Error::new(format!("unknown collection: {collection}")))?;
    let mut fields = BTreeMap::new();
    for (name, info) in &col.fields {
        fields.insert(name.clone(), info.ty);
    }
    Ok(Scope {
        primary: collection.to_string(),
        fields,
    })
}

pub fn collection_of(source: &Source) -> &str {
    match source {
        Source::Collection(n) => n,
        Source::Page(_) => "docs",
        Source::Catalog => "catalog",
    }
}
