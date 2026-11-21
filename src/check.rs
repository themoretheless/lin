use std::collections::{BTreeMap, BTreeSet};

use crate::ast::*;
use crate::catalog::{self, Catalog, Type};
use crate::error::Error;

#[expect(dead_code)]
pub fn check(stmt: &Stmt, cat: &Catalog) -> Result<(), Error> {
    check_in(stmt, cat, &Bindings::new())
}

pub(crate) fn check_single_insert(
    collection: &str,
    rec: &Record,
    cat: &Catalog,
) -> Result<(), Error> {
    let col = cat
        .collection(collection)
        .ok_or_else(|| Error::new(format!("unknown collection: {collection}")))?;
    if col.append_only {
        return Err(Error::new(format!(
            "{collection} is append-only; use `append`"
        )));
    }
    check_record_col(rec, collection, col, cat, false)
}

pub(crate) fn check_single_insert_borrowed(
    collection: &str,
    fields: &[(&str, Value)],
    cat: &Catalog,
) -> Result<(), Error> {
    let col = cat
        .collection(collection)
        .ok_or_else(|| Error::new(format!("unknown collection: {collection}")))?;
    if col.append_only {
        return Err(Error::new(format!(
            "{collection} is append-only; use `append`"
        )));
    }
    for (name, value) in fields {
        let info = col
            .field(name)
            .ok_or_else(|| Error::new(format!("unknown field: {name}")))?;
        check_value_as(value, info.ty, cat)?;
    }
    Ok(())
}

pub(crate) fn check_query_only(q: &Query, cat: &Catalog) -> Result<(), Error> {
    check_query(q, cat, &Bindings::new()).map(|_| ())
}

/// Typecheck a program against the live catalog.
///
/// Declarations are staged into a private copy, returned here so the caller can
/// plan against it: a later statement must see the field or index an earlier one
/// just created, but the live catalog only changes when the program executes.
/// A program without a declaration — every read and every DML write — never
/// copies it, which is what keeps a cold `prepare` from paying for one.
pub fn check_program(stmts: &[Stmt], cat: &Catalog) -> Result<(Bindings, Option<Catalog>), Error> {
    let mut env = Bindings::new();
    let mut staged: Option<Catalog> = None;
    for stmt in stmts {
        let view = staged.as_ref().unwrap_or(cat);
        if let Stmt::Let { name, query } = stmt {
            if view.collection(name).is_some() || name == "catalog" {
                return Err(Error::new(format!("cannot bind over collection: {name}")));
            }
            let scope = check_query(query, view, &env)?;
            if env.contains(name) {
                return Err(Error::new(format!("binding exists: {name}")));
            }
            env.insert(name.clone(), scope);
            continue;
        }
        check_in(stmt, view, &env)?;
        if let Stmt::Decl(d) = stmt {
            staged.get_or_insert_with(|| cat.clone()).apply_decl(d)?;
        }
    }
    Ok((env, staged))
}

fn check_records_bulk(
    records: &[Record],
    col: &catalog::Collection,
    cat: &Catalog,
) -> Result<(), Error> {
    let mut shape_cache: smallvec::SmallVec<[(&str, Type); 12]> = smallvec::SmallVec::new();
    for record in records {
        if !shape_cache.is_empty()
            && record.fields.len() == shape_cache.len()
            && record
                .fields
                .iter()
                .zip(shape_cache.iter())
                .all(|((k, _), (ck, _))| k.as_str() == *ck)
        {
            for ((_, value), &(_, ty)) in record.fields.iter().zip(shape_cache.iter()) {
                check_value_as(value, ty, cat)?;
            }
            continue;
        }
        shape_cache.clear();
        for (name, value) in &record.fields {
            let info = col
                .field(name)
                .ok_or_else(|| Error::new(format!("unknown field: {name}")))?;
            check_value_as(value, info.ty, cat)?;
            shape_cache.push((name.as_str(), info.ty));
        }
    }
    Ok(())
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
            check_records_bulk(records, facts, cat)?;
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
            check_records_bulk(records, col, cat)?;
            for e in edges {
                if cat.rel(&e.rel).is_none() {
                    return Err(Error::new(format!("unknown rel: {}", e.rel)));
                }
            }
        }
        // The rows were built by `insert_batch`, which checks their field types against
        // the catalog with one probe record per kind of value before staging them.
        Stmt::InsertRows { collection, rows } => {
            let col = cat
                .collection(collection)
                .ok_or_else(|| Error::new(format!("unknown collection: {collection}")))?;
            if col.append_only {
                return Err(Error::new(format!(
                    "{collection} is append-only; use `append`"
                )));
            }
            if *rows == 0 {
                return Err(Error::new("empty list"));
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
            if let Some(p) = pred {
                let scope = Scope::from_col(collection, col);
                check_pred(p, &scope, cat)?;
            }
            check_record_col(record, collection, col, cat, true)?;
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
            let scope = Scope::from_col(collection, col);
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
        Stmt::IdbPull { .. }
        | Stmt::IdbPush
        | Stmt::Snapshot { .. }
        | Stmt::Restore { .. }
        | Stmt::Pin { .. }
        | Stmt::Unpin { .. } => {}
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
                        if Type::from_name(n).is_none() && !cat.owned.contains_key(n) {
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
            if scope.ty_of(field, cat).is_none() {
                return Err(Error::new(format!("unknown field: {collection}.{field}")));
            }
            check_pred(pred, &scope, cat)?;
        }
        Decl::Filter {
            collection,
            pred,
            src: _,
        } => {
            let scope = scope_of(collection, cat)?;
            if let Some(pred) = pred {
                check_pred(pred, &scope, cat)?;
            }
        }
        Decl::Owned { name, fields } => {
            if cat.owned.contains_key(name) || cat.collection(name).is_some() {
                return Err(Error::new(format!("owned exists: {name}")));
            }
            if fields.is_empty() {
                return Err(Error::new("empty owned"));
            }
            for (fname, ty) in fields {
                match ty {
                    TypeExpr::Named(n) => {
                        if Type::from_name(n).is_none() {
                            return Err(Error::new(format!("unknown type: {n}")));
                        }
                    }
                    TypeExpr::Vec { .. } => {}
                }
                if fname.is_empty() {
                    return Err(Error::new("empty owned field"));
                }
            }
        }
        Decl::Index {
            collection,
            fields,
            unique: _,
            pred,
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
            if let Some(p) = pred {
                let scope = Scope::from_col(collection, col);
                check_pred(p, &scope, cat)?;
            }
            if !cat.indexes.is_empty() {
                let label = format!("{}[{}]", collection, fields.join(","));
                if cat.indexes.contains_key(&label) {
                    return Err(Error::new(format!("index exists: {label}")));
                }
            }
        }
    }
    Ok(())
}

fn check_query(q: &Query, cat: &Catalog, env: &Bindings) -> Result<Scope, Error> {
    let q = catalog::with_catalog_filter(q, cat);
    let q = &q;
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
                    let ty = require_field(&scope, f, cat)?;
                    next.insert(f.as_str(), ty);
                }
                scope.replace_fields(next);
            }
            Step::Without(fields) => {
                if fields.is_empty() {
                    return Err(Error::new("without requires at least one field"));
                }
                let mut next: BTreeMap<String, Type> = scope
                    .materialized_fields(cat)
                    .into_iter()
                    .map(|(k, ty)| (k.to_string(), ty))
                    .collect();
                for f in fields {
                    let Some(name) = f.leaf() else {
                        return Err(Error::new(format!(
                            "without only removes top-level fields: {}",
                            f.as_str()
                        )));
                    };
                    if require_field(&scope, f, cat).is_err() {
                        return Err(Error::new(format!("unknown field: {name}")));
                    }
                    next.remove(name);
                }
                if next.is_empty() {
                    return Err(Error::new("without cannot remove all fields"));
                }
                scope.replace_fields(next);
            }
            Step::Join { collection, on, .. } => {
                if cat.collection(collection).is_none() {
                    return Err(Error::new(format!("unknown collection: {collection}")));
                }
                if scope.ty_of(on, cat).is_none()
                    && (scope.fields.is_empty()
                        || scope.ty_of(&format!("{}.{on}", scope.primary), cat).is_none())
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
                let mut names: smallvec::SmallVec<[&str; 8]> = smallvec::SmallVec::new();
                if let Some(s) = start {
                    names.push(s.as_str());
                    let primary_col = cat.collection(&scope.primary).ok_or_else(|| {
                        Error::new(format!("unknown collection: {}", scope.primary))
                    })?;
                    scope.add_bind_col(s, primary_col);
                }
                let docs_col = cat.collection("docs");
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
                        if names.contains(&e.as_str()) {
                            return Err(Error::new(format!("duplicate match bind: {e}")));
                        }
                        names.push(e.as_str());
                        scope.add_edge_bind(e);
                    }
                    if names.contains(&h.bind.as_str()) {
                        return Err(Error::new(format!("duplicate match bind: {}", h.bind)));
                    }
                    names.push(h.bind.as_str());
                    let target_col = match docs_col {
                        Some(c) => c,
                        None => cat.collection(&scope.primary).ok_or_else(|| {
                            Error::new(format!("unknown collection: {}", scope.primary))
                        })?,
                    };
                    scope.add_bind_col(&h.bind, target_col);
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
                let by_ty = match by {
                    Some(by) => Some(require_field(&scope, by, cat)?),
                    None => None,
                };
                let primary = std::mem::take(&mut scope.primary);
                scope = match (by, by_ty) {
                    (Some(by), Some(by_ty)) => {
                        Scope::agg(primary, by.as_str(), by_ty, Type::I64, "hits".into())
                    }
                    _ => Scope::agg_total(primary, Type::I64, "hits".into()),
                };
            }
            Step::Sum { field, by } => {
                let ty = require_field(&scope, field, cat)?;
                if !ty.is_numeric() {
                    return Err(Error::new(format!(
                        "sum requires numeric field, found {}",
                        ty.name()
                    )));
                }
                let by_ty = match by {
                    Some(by) => Some(require_field(&scope, by, cat)?),
                    None => None,
                };
                let primary = std::mem::take(&mut scope.primary);
                scope = match (by, by_ty) {
                    (Some(by), Some(by_ty)) => {
                        Scope::agg(primary, by.as_str(), by_ty, Type::F64, field.as_str())
                    }
                    _ => Scope::agg_total(primary, Type::F64, field.as_str()),
                };
            }
            Step::Min { field, by } | Step::Max { field, by } => {
                let ty = require_field(&scope, field, cat)?;
                let by_ty = match by {
                    Some(by) => Some(require_field(&scope, by, cat)?),
                    None => None,
                };
                let primary = std::mem::take(&mut scope.primary);
                scope = match (by, by_ty) {
                    (Some(by), Some(by_ty)) => {
                        Scope::agg(primary, by.as_str(), by_ty, ty, field.as_str())
                    }
                    _ => Scope::agg_total(primary, ty, field.as_str()),
                };
            }
            Step::Avg { field, by } => {
                let ty = require_field(&scope, field, cat)?;
                if !ty.is_numeric() {
                    return Err(Error::new(format!(
                        "avg requires numeric field, found {}",
                        ty.name()
                    )));
                }
                let by_ty = match by {
                    Some(by) => Some(require_field(&scope, by, cat)?),
                    None => None,
                };
                let primary = std::mem::take(&mut scope.primary);
                scope = match (by, by_ty) {
                    (Some(by), Some(by_ty)) => {
                        Scope::agg(primary, by.as_str(), by_ty, Type::F64, field.as_str())
                    }
                    _ => Scope::agg_total(primary, Type::F64, field.as_str()),
                };
            }
            Step::Summarize { specs, by } => {
                if specs.is_empty() {
                    return Err(Error::new("summarize requires at least one metric"));
                }
                let mut out_fields = BTreeMap::new();
                let mut seen: BTreeSet<&str> = BTreeSet::new();
                for gk in by {
                    let ty = check_group_key(gk, &scope, cat)?;
                    if !seen.insert(gk.alias.as_str()) {
                        return Err(Error::new(format!(
                            "duplicate group key alias in summarize: {}",
                            gk.alias
                        )));
                    }
                    out_fields.insert(gk.alias.clone(), ty);
                }
                for spec in specs {
                    let out_ty = match &spec.func {
                        AggFunc::Count => Type::I64,
                        AggFunc::Sum(f) => {
                            let ty = require_field(&scope, f, cat)?;
                            if !ty.is_numeric() {
                                return Err(Error::new(format!(
                                    "sum requires numeric field, found {}",
                                    ty.name()
                                )));
                            }
                            if ty == Type::I64 { Type::I64 } else { Type::F64 }
                        }
                        AggFunc::Avg(f) => {
                            let ty = require_field(&scope, f, cat)?;
                            if !ty.is_numeric() {
                                return Err(Error::new(format!(
                                    "avg requires numeric field, found {}",
                                    ty.name()
                                )));
                            }
                            Type::F64
                        }
                        AggFunc::Min(f) | AggFunc::Max(f) => require_field(&scope, f, cat)?,
                        AggFunc::DCount(f) => {
                            require_field(&scope, f, cat)?;
                            Type::I64
                        }
                        AggFunc::Percentile { field, pct } => {
                            if !pct.is_finite() || !(0.0..=100.0).contains(pct) {
                                return Err(Error::new(
                                    "percentile target must be between 0 and 100",
                                ));
                            }
                            let ty = require_field(&scope, field, cat)?;
                            if !ty.is_numeric() {
                                return Err(Error::new(format!(
                                    "percentile requires numeric field, found {}",
                                    ty.name()
                                )));
                            }
                            Type::F64
                        }
                        AggFunc::ArgMax { order, value } | AggFunc::ArgMin { order, value } => {
                            require_field(&scope, order, cat)?;
                            require_field(&scope, value, cat)?
                        }
                    };
                    if !seen.insert(spec.alias.as_str()) {
                        return Err(Error::new(format!(
                            "duplicate summarize output field `{}`",
                            spec.alias
                        )));
                    }
                    out_fields.insert(spec.alias.clone(), out_ty);
                }
                scope.replace_fields(out_fields);
            }
            Step::ArgExtremum { order, by, .. } => {
                require_field(&scope, order, cat)?;
                let mut seen_by: BTreeSet<&str> = BTreeSet::new();
                for gk in by {
                    let gty = check_group_key(gk, &scope, cat)?;
                    if !seen_by.insert(gk.alias.as_str()) {
                        return Err(Error::new(format!(
                            "duplicate group key alias in arg_max/arg_min: {}",
                            gk.alias
                        )));
                    }
                    scope.insert_field(gk.alias.clone(), gty, cat);
                }
            }
            Step::Sort { field, .. } => {
                require_field(&scope, field, cat)?;
            }
            Step::Window {
                func,
                field,
                by,
                order,
                frame: _,
                as_field,
            } => {
                for b in by {
                    require_field(&scope, b, cat)?;
                }
                for s in order {
                    require_field(&scope, &s.field, cat)?;
                }
                let out_ty = match func {
                    crate::window::WindowFunction::RowNumber
                    | crate::window::WindowFunction::Rank
                    | crate::window::WindowFunction::DenseRank
                    | crate::window::WindowFunction::Count => Type::I64,
                    crate::window::WindowFunction::Sum
                    | crate::window::WindowFunction::Min
                    | crate::window::WindowFunction::Max => {
                        let f = field.as_ref().ok_or_else(|| {
                            Error::new(format!("{func} requires a field argument"))
                        })?;
                        let ty = require_field(&scope, f, cat)?;
                        if !ty.is_numeric() {
                            return Err(Error::new(format!(
                                "{func} requires numeric field, found {}",
                                ty.name()
                            )));
                        }
                        ty
                    }
                    crate::window::WindowFunction::Avg => {
                        let f = field.as_ref().ok_or_else(|| {
                            Error::new(format!("{func} requires a field argument"))
                        })?;
                        let ty = require_field(&scope, f, cat)?;
                        if !ty.is_numeric() {
                            return Err(Error::new(format!(
                                "avg requires numeric field, found {}",
                                ty.name()
                            )));
                        }
                        Type::F64
                    }
                    crate::window::WindowFunction::Lead { .. }
                    | crate::window::WindowFunction::Lag { .. }
                    | crate::window::WindowFunction::FirstValue
                    | crate::window::WindowFunction::LastValue => {
                        let f = field.as_ref().ok_or_else(|| {
                            Error::new(format!("{func} requires a field argument"))
                        })?;
                        require_field(&scope, f, cat)?
                    }
                };
                scope.insert_field(as_field.clone(), out_ty, cat);
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
                check_union_cols(&scope, &right, cat)?;
            }
        }
    }
    Ok(scope)
}

fn check_group_key(gk: &GroupKey, scope: &Scope, cat: &Catalog) -> Result<Type, Error> {
    match &gk.expr {
        GroupExpr::Field(f) => require_field(scope, f, cat),
        GroupExpr::Bin { field, step } => {
            let fty = require_field(scope, field, cat)?;
            match (fty, step) {
                (Type::Time, BinStep::Duration(ms)) => {
                    if *ms <= 0 {
                        return Err(Error::new("bin duration step must be positive"));
                    }
                    Ok(Type::Time)
                }
                (Type::I64, BinStep::Int(n)) => {
                    if *n <= 0 {
                        return Err(Error::new("bin step must be positive"));
                    }
                    Ok(Type::I64)
                }
                (Type::F64, BinStep::Int(n)) => {
                    if *n <= 0 {
                        return Err(Error::new("bin step must be positive"));
                    }
                    Ok(Type::F64)
                }
                (Type::I64 | Type::F64, BinStep::Float(x)) => {
                    if !x.is_finite() || *x <= 0.0 {
                        return Err(Error::new("bin step must be positive and finite"));
                    }
                    Ok(Type::F64)
                }
                (Type::Time, _) => Err(Error::new(
                    "bin() on a time field requires a positive duration step (e.g. 5m, 1h, 1d)",
                )),
                (_, BinStep::Duration(_)) => Err(Error::new(format!(
                    "bin() with duration step requires a time field, found {}",
                    fty.name()
                ))),
                (other, _) => Err(Error::new(format!(
                    "bin() requires time or numeric field, found {}",
                    other.name()
                ))),
            }
        }
    }
}

fn check_union_cols(left: &Scope, right: &Scope, cat: &Catalog) -> Result<(), Error> {
    let lf = left.materialized_fields(cat);
    let rf = right.materialized_fields(cat);
    if lf.len() != rf.len()
        || lf
            .iter()
            .zip(rf.iter())
            .any(|((lk, _), (rk, _))| lk != rk)
    {
        let l: Vec<&str> = lf.iter().map(|(k, _)| *k).collect();
        let r: Vec<&str> = rf.iter().map(|(k, _)| *k).collect();
        return Err(Error::new(format!(
            "union columns mismatch: [{}] vs [{}]",
            l.join(", "),
            r.join(", ")
        )));
    }
    for (&(name, lty), &(_, rty)) in lf.iter().zip(rf.iter()) {
        if !types_ok(rty, lty) && !types_ok(lty, rty) {
            return Err(Error::new(format!(
                "union type mismatch: {name} {} vs {}",
                lty.name(),
                rty.name()
            )));
        }
    }
    Ok(())
}

fn pred_fields_present_in_record(pred: &Pred, record: &Record) -> bool {
    match pred {
        Pred::And(a, b) | Pred::Or(a, b) => {
            pred_fields_present_in_record(a, record) && pred_fields_present_in_record(b, record)
        }
        Pred::Not(inner) => pred_fields_present_in_record(inner, record),
        Pred::Cmp { field, .. }
        | Pred::IsNull { field, .. }
        | Pred::In { field, .. }
        | Pred::Has { field, .. }
        | Pred::Contains { field, .. }
        | Pred::Regex { field, .. } => record_literal_val(record, field).is_some(),
    }
}

fn check_record_col(
    record: &Record,
    collection: &str,
    col: &catalog::Collection,
    cat: &Catalog,
    is_update: bool,
) -> Result<(), Error> {
    let mut layer_raw = false;
    for (name, value) in &record.fields {
        let info = col
            .field(name)
            .ok_or_else(|| Error::new(format!("unknown field: {name}")))?;
        if is_update
            && name == "layer"
            && let Value::String(s) = value
        {
            layer_raw = s.as_ref() == "raw";
        }
        check_value_as(value, info.ty, cat)?;
    }
    if is_update {
        for (name, _) in &record.fields {
            for g in &cat.guards {
                if g.collection == collection && &g.field == name {
                    if !pred_fields_present_in_record(&g.pred, record)
                        || eval_record_pred(&g.pred, record)
                    {
                        return Err(Error::new(format!("immutable field: {collection}.{name}")));
                    }
                }
            }
        }
        if layer_raw && record.fields.iter().any(|(n, _)| n == "body") {
            return Err(Error::new(format!("immutable field: {collection}.body")));
        }
    }
    Ok(())
}

fn record_literal_val<'a>(record: &'a Record, field: &Field) -> Option<&'a Value> {
    let key = field.as_cow_str();
    record
        .fields
        .iter()
        .rfind(|(k, _)| k.as_str() == key.as_ref())
        .map(|(_, v)| v)
        .filter(|v| {
            matches!(
                v,
                Value::String(_) | Value::Int(_) | Value::Float(_) | Value::Bool(_)
            )
        })
}

fn record_val_eq(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::String(a), Value::String(b)) => a == b,
        (Value::String(a), Value::Name(b)) => a.as_ref() == b.as_str(),
        (Value::Bool(a), Value::Bool(b)) => a == b,
        (Value::Int(a), Value::Int(b)) => a == b,
        (Value::Float(a), Value::Float(b)) => !a.is_nan() && !b.is_nan() && a == b,
        (Value::Int(a), Value::Float(b)) | (Value::Float(b), Value::Int(a)) => {
            b.is_finite() && b.fract() == 0.0 && (*a as f64) == *b
        }
        (Value::Int(a), Value::Duration(d)) => *a == d.as_millis(),
        (Value::Float(a), Value::Duration(d)) => {
            a.is_finite() && a.fract() == 0.0 && (d.as_millis() as f64) == *a
        }
        _ => false,
    }
}

fn record_val_cmp(left: &Value, right: &Value) -> Option<std::cmp::Ordering> {
    match (left, right) {
        (Value::Int(a), Value::Int(b)) => Some(a.cmp(b)),
        (Value::Int(a), Value::Duration(d)) => Some(a.cmp(&d.as_millis())),
        (Value::Float(a), Value::Float(b)) => a.partial_cmp(b),
        (Value::Int(a), Value::Float(b)) => (*a as f64).partial_cmp(b),
        (Value::Float(a), Value::Int(b)) => a.partial_cmp(&(*b as f64)),
        (Value::Float(a), Value::Duration(d)) => a.partial_cmp(&(d.as_millis() as f64)),
        (Value::String(a), Value::String(b)) => Some(a.as_ref().cmp(b.as_ref())),
        (Value::String(a), Value::Name(b)) => Some(a.as_ref().cmp(b.as_str())),
        _ => None,
    }
}

fn eval_record_pred(pred: &Pred, record: &Record) -> bool {
    match pred {
        Pred::And(a, b) => eval_record_pred(a, record) && eval_record_pred(b, record),
        Pred::Or(a, b) => eval_record_pred(a, record) || eval_record_pred(b, record),
        Pred::Not(inner) => !eval_record_pred(inner, record),
        Pred::IsNull { field, negated } => {
            let is_null = record_literal_val(record, field).is_none();
            if *negated { !is_null } else { is_null }
        }
        Pred::In { field, values } => record_literal_val(record, field)
            .is_some_and(|l| values.iter().any(|v| record_val_eq(l, v))),
        Pred::Cmp { field, op, value } => {
            let Some(left) = record_literal_val(record, field) else {
                return matches!(op, CmpOp::Ne);
            };
            match op {
                CmpOp::Eq => record_val_eq(left, value),
                CmpOp::Ne => !record_val_eq(left, value),
                CmpOp::Gt | CmpOp::Lt | CmpOp::Ge | CmpOp::Le => {
                    let Some(ord) = record_val_cmp(left, value) else {
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
        Pred::Contains { field, needle } => matches!(
            record_literal_val(record, field),
            Some(Value::String(s)) if s.contains(needle.as_str())
        ),
        Pred::Has { field, ci, needle } => {
            let Some(Value::String(s)) = record_literal_val(record, field) else {
                return false;
            };
            if needle.is_empty() {
                return false;
            }
            if !*ci {
                return s
                    .split(|c: char| !(c.is_alphanumeric() || c == '_'))
                    .any(|w| w == needle);
            }
            if s.is_ascii() && needle.is_ascii() {
                return s
                    .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                    .any(|w| w.eq_ignore_ascii_case(needle));
            }
            let needle_lower = needle.to_lowercase();
            s.split(|c: char| !(c.is_alphanumeric() || c == '_'))
                .any(|w| w.eq_ignore_ascii_case(needle) || w.to_lowercase() == needle_lower)
        }
        Pred::Regex {
            field,
            pattern,
            flags,
        } => {
            let Some(Value::String(s)) = record_literal_val(record, field) else {
                return false;
            };
            let mut b = regex::RegexBuilder::new(pattern);
            b.size_limit(10 * 1024 * 1024);
            if flags.contains('i') {
                b.case_insensitive(true);
            }
            b.build().is_ok_and(|re| re.is_match(s))
        }
    }
}

fn check_value_as(value: &Value, expected: Type, cat: &Catalog) -> Result<(), Error> {
    if matches!(value, Value::Param(_)) {
        return Ok(());
    }
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
        Value::Timestamp(_) | Value::Now | Value::NowMinus(_) => Ok(Type::Time),
        Value::Duration(_) => Ok(Type::Dur),
        Value::Param(_) => Ok(Type::Var),
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
        || got == Type::Var
        || expected == Type::Var
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
    let base_col = if scope.base_from_catalog {
        cat.collection(&scope.primary)
    } else {
        None
    };
    let mut last_field: Option<(&str, Type)> = None;
    check_pred_inner(pred, scope, base_col, cat, &mut last_field)
}

#[inline]
fn require_pred_field<'p>(
    scope: &Scope,
    base_col: Option<&catalog::Collection>,
    field: &'p Field,
    cat: &Catalog,
    last_field: &mut Option<(&'p str, Type)>,
) -> Result<Type, Error> {
    if let Some(leaf) = field.leaf() {
        if let Some((prev_name, prev_ty)) = *last_field
            && prev_name == leaf
        {
            return Ok(prev_ty);
        }
        let found = if scope.fields.is_empty() {
            base_col.and_then(|c| c.field(leaf)).map(|i| i.ty)
        } else {
            scope
                .fields
                .get(leaf)
                .copied()
                .or_else(|| base_col.and_then(|c| c.field(leaf)).map(|i| i.ty))
        };
        let ty = found.ok_or_else(|| Error::new(format!("unknown field: {leaf}")))?;
        *last_field = Some((leaf, ty));
        return Ok(ty);
    }
    require_field(scope, field, cat)
}

fn check_pred_inner<'p>(
    pred: &'p Pred,
    scope: &Scope,
    base_col: Option<&catalog::Collection>,
    cat: &Catalog,
    last_field: &mut Option<(&'p str, Type)>,
) -> Result<(), Error> {
    match pred {
        Pred::And(a, b) | Pred::Or(a, b) => {
            check_pred_inner(a, scope, base_col, cat, last_field)?;
            check_pred_inner(b, scope, base_col, cat, last_field)?;
        }
        Pred::Not(inner) => {
            check_pred_inner(inner, scope, base_col, cat, last_field)?;
        }
        Pred::IsNull { field, .. } => {
            require_pred_field(scope, base_col, field, cat, last_field)?;
        }
        Pred::In { field, values } => {
            let lty = require_pred_field(scope, base_col, field, cat, last_field)?;
            if values.is_empty() {
                return Err(Error::new("empty `in` list"));
            }
            for v in values {
                let rty = value_type(v, cat)?;
                if !types_ok(rty, lty) && !types_ok(lty, rty) {
                    return Err(Error::new(format!(
                        "type mismatch in `in`: {} vs {}",
                        lty.name(),
                        rty.name()
                    )));
                }
            }
        }
        Pred::Cmp { field, op, value } => {
            let lty = require_pred_field(scope, base_col, field, cat, last_field)?;
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
                    let ok = rty == Type::Var
                        || lty == Type::Var
                        || matches!(
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
            let ty = require_pred_field(scope, base_col, field, cat, last_field)?;
            if !ty.is_textish() {
                return Err(Error::new(format!(
                    "has requires text, found {} ({})",
                    ty.name(),
                    field.as_str()
                )));
            }
        }
        Pred::Contains { field, .. } => {
            let ty = require_pred_field(scope, base_col, field, cat, last_field)?;
            if !ty.is_textish() {
                return Err(Error::new(format!(
                    "`~` requires text, found {} ({})",
                    ty.name(),
                    field.as_str()
                )));
            }
        }
        Pred::Regex {
            field,
            pattern,
            flags,
        } => {
            let ty = require_pred_field(scope, base_col, field, cat, last_field)?;
            if !ty.is_textish() {
                return Err(Error::new(format!(
                    "`~` requires text, found {} ({})",
                    ty.name(),
                    field.as_str()
                )));
            }
            validate_regex(pattern, flags)?;
        }
    }
    Ok(())
}

const MAX_VALID_REGEX_CACHE: usize = 256;

thread_local! {
    static VALID_REGEX_CACHE: std::cell::RefCell<[rustc_hash::FxHashSet<Box<str>>; 2]> =
        std::cell::RefCell::new([
            rustc_hash::FxHashSet::default(),
            rustc_hash::FxHashSet::default(),
        ]);
}

pub fn validate_regex(pattern: &str, flags: &str) -> Result<(), Error> {
    let mut ci = false;
    for f in flags.chars() {
        if f == 'i' {
            ci = true;
        } else {
            return Err(Error::new(format!("unknown regex flag `{f}`")));
        }
    }
    let slot = usize::from(ci);
    let cached = VALID_REGEX_CACHE
        .try_with(|cell| cell.borrow()[slot].contains(pattern))
        .unwrap_or(false);
    if cached {
        return Ok(());
    }
    let mut b = regex::RegexBuilder::new(pattern);
    b.size_limit(10 * 1024 * 1024);
    if ci {
        b.case_insensitive(true);
    }
    b.build()
        .map_err(|e| Error::new(format!("invalid regex literal: {e}")))?;
    let _ = VALID_REGEX_CACHE.try_with(|cell| {
        let mut sets = cell.borrow_mut();
        if sets[slot].len() >= MAX_VALID_REGEX_CACHE {
            sets[slot].clear();
        }
        sets[slot].insert(Box::from(pattern));
    });
    Ok(())
}

fn require_field(scope: &Scope, field: &Field, cat: &Catalog) -> Result<Type, Error> {
    let key = field.as_cow_str();
    scope
        .ty_of(&key, cat)
        .ok_or_else(|| Error::new(format!("unknown field: {key}")))
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
    /// When `true`, `primary` names a catalog collection whose unprojected fields are
    /// resolved directly from `cat.collection(&self.primary)` without allocating/cloning
    /// a `BTreeMap` of every field name.
    base_from_catalog: bool,
    fields: BTreeMap<String, Type>,
}

impl Scope {
    #[inline]
    fn from_col(collection: &str, _col: &catalog::Collection) -> Self {
        Self {
            primary: collection.to_string(),
            base_from_catalog: true,
            fields: BTreeMap::new(),
        }
    }

    #[inline]
    fn ty_of(&self, name: &str, cat: &Catalog) -> Option<Type> {
        if let Some(&ty) = self.fields.get(name) {
            return Some(ty);
        }
        if self.base_from_catalog
            && let Some(col) = cat.collection(&self.primary)
        {
            return col.field(name).map(|info| info.ty);
        }
        None
    }

    fn replace_fields(&mut self, next: BTreeMap<String, Type>) {
        self.base_from_catalog = false;
        self.fields = next;
    }

    fn insert_field(&mut self, name: String, ty: Type, _cat: &Catalog) {
        self.fields.insert(name, ty);
    }

    fn materialized_fields<'a>(&'a self, cat: &'a Catalog) -> Vec<(&'a str, Type)> {
        if !self.base_from_catalog {
            return self.fields.iter().map(|(k, &v)| (k.as_str(), v)).collect();
        }
        let mut merged: BTreeMap<&'a str, Type> = BTreeMap::new();
        if let Some(col) = cat.collection(&self.primary) {
            for (k, info) in &col.fields {
                merged.insert(k.as_str(), info.ty);
            }
        }
        for (k, &v) in &self.fields {
            merged.insert(k.as_str(), v);
        }
        merged.into_iter().collect()
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

    fn add_bind_col(&mut self, bind: &str, col: &catalog::Collection) {
        let mut key = String::with_capacity(bind.len() + 16);
        for (name, info) in &col.fields {
            key.clear();
            key.push_str(bind);
            key.push('.');
            key.push_str(name);
            self.fields.insert(key.clone(), info.ty);
        }
    }

    fn add_edge_bind(&mut self, bind: &str) {
        self.fields.insert(format!("{bind}.rel"), Type::Rel);
        self.fields.insert(format!("{bind}.from"), Type::Text);
        self.fields.insert(format!("{bind}.to"), Type::Text);
    }

    fn agg(primary: String, by: String, by_ty: Type, metric_ty: Type, metric: String) -> Self {
        let mut fields = BTreeMap::new();
        fields.insert(by, by_ty);
        fields.insert(metric, metric_ty);
        Self {
            primary,
            base_from_catalog: false,
            fields,
        }
    }

    fn agg_total(primary: String, metric_ty: Type, metric: String) -> Self {
        let mut fields = BTreeMap::new();
        fields.insert(metric, metric_ty);
        Self {
            primary,
            base_from_catalog: false,
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
            base_from_catalog: false,
            fields,
        }
    }
}

fn scope_of(collection: &str, cat: &Catalog) -> Result<Scope, Error> {
    let col = cat
        .collection(collection)
        .ok_or_else(|| Error::new(format!("unknown collection: {collection}")))?;
    Ok(Scope::from_col(collection, col))
}

pub fn collection_of(source: &Source) -> &str {
    match source {
        Source::Collection(n) => n,
        Source::Page(_) => "docs",
        Source::Catalog => "catalog",
    }
}
