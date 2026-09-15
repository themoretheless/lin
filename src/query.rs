//! Fluent deferred query builder (IQueryable-style) over Lin AST.

use crate::ast::{
    CmpOp, Field, MatchHop, Pred, Query, SearchMode, Source, Stmt, Step, Value,
};
use crate::error::Error;
use crate::exec::{Db, Handle, ReadDb};
use crate::graph::GraphFmt;
use crate::row::{self, FromRow, LinRow};
use crate::store::Row;

/// Helpers for building [`Pred`] values.
pub mod pred {
    use super::*;

    pub fn field(name: impl AsRef<str>) -> Field {
        Field::path(name.as_ref())
    }

    pub fn eq(field: impl AsRef<str>, value: impl Into<Value>) -> Pred {
        Pred::Cmp {
            field: Field::path(field.as_ref()),
            op: CmpOp::Eq,
            value: value.into(),
        }
    }

    pub fn ne(field: impl AsRef<str>, value: impl Into<Value>) -> Pred {
        Pred::Cmp {
            field: Field::path(field.as_ref()),
            op: CmpOp::Ne,
            value: value.into(),
        }
    }

    pub fn gt(field: impl AsRef<str>, value: impl Into<Value>) -> Pred {
        Pred::Cmp {
            field: Field::path(field.as_ref()),
            op: CmpOp::Gt,
            value: value.into(),
        }
    }

    pub fn lt(field: impl AsRef<str>, value: impl Into<Value>) -> Pred {
        Pred::Cmp {
            field: Field::path(field.as_ref()),
            op: CmpOp::Lt,
            value: value.into(),
        }
    }

    pub fn ge(field: impl AsRef<str>, value: impl Into<Value>) -> Pred {
        Pred::Cmp {
            field: Field::path(field.as_ref()),
            op: CmpOp::Ge,
            value: value.into(),
        }
    }

    pub fn le(field: impl AsRef<str>, value: impl Into<Value>) -> Pred {
        Pred::Cmp {
            field: Field::path(field.as_ref()),
            op: CmpOp::Le,
            value: value.into(),
        }
    }

    pub fn has(field: impl AsRef<str>, needle: impl Into<String>) -> Pred {
        Pred::Has {
            field: Field::path(field.as_ref()),
            ci: false,
            needle: needle.into(),
        }
    }

    pub fn contains(field: impl AsRef<str>, needle: impl Into<String>) -> Pred {
        Pred::Contains {
            field: Field::path(field.as_ref()),
            needle: needle.into(),
        }
    }

    pub fn regex(
        field: impl AsRef<str>,
        pattern: impl Into<String>,
        flags: impl Into<String>,
    ) -> Pred {
        Pred::Regex {
            field: Field::path(field.as_ref()),
            pattern: pattern.into(),
            flags: flags.into(),
        }
    }

    pub fn and(a: Pred, b: Pred) -> Pred {
        a.and(b)
    }

    pub fn or(a: Pred, b: Pred) -> Pred {
        Pred::Or(Box::new(a), Box::new(b))
    }

    /// Keyset helper: `field > value` (prefer over deep `skip`).
    pub fn after(field: impl AsRef<str>, value: impl Into<Value>) -> Pred {
        gt(field, value)
    }
}

/// Deferred query: compose steps, then execute against a [`Db`] / [`ReadDb`].
#[derive(Debug, Clone, PartialEq)]
pub struct Queryable {
    pub(crate) query: Query,
}

impl Queryable {
    pub fn from(collection: impl Into<String>) -> Self {
        Self {
            query: Query {
                source: Source::Collection(collection.into()),
                steps: Vec::new(),
                explain: None,
            },
        }
    }

    pub fn from_page(page: impl Into<String>) -> Self {
        Self {
            query: Query {
                source: Source::Page(page.into()),
                steps: Vec::new(),
                explain: None,
            },
        }
    }

    pub fn catalog() -> Self {
        Self {
            query: Query {
                source: Source::Catalog,
                steps: Vec::new(),
                explain: None,
            },
        }
    }

    /// Start from a typed row's default collection (requires `LinRow::COLLECTION`).
    pub fn from_typed<T: LinRow>() -> Result<Self, Error> {
        let name = T::COLLECTION.ok_or_else(|| {
            Error::runtime("LinRow has no COLLECTION; pass collection to Queryable::from")
        })?;
        Ok(Self::from(name).select_row::<T>())
    }

    pub fn query(&self) -> &Query {
        &self.query
    }

    pub fn into_query(self) -> Query {
        self.query
    }

    pub fn stmt(&self) -> Stmt {
        Stmt::Query(self.query.clone())
    }

    pub fn filter(mut self, pred: Pred) -> Self {
        self.query.steps.push(Step::Filter(pred));
        self
    }

    pub fn select(mut self, fields: impl IntoFieldList) -> Self {
        self.query.steps.push(Step::Project(fields.into_fields()));
        self
    }

    /// Project using [`LinRow::COLUMNS`].
    pub fn select_row<T: LinRow>(self) -> Self {
        self.select(&T::COLUMNS[..])
    }

    pub fn take(mut self, n: i64) -> Self {
        self.query.steps.push(Step::Take { n: Some(n) });
        self
    }

    pub fn take_all(mut self) -> Self {
        self.query.steps.push(Step::Take { n: None });
        self
    }

    /// Drop the first `n` rows (`skip` / `offset` in the DSL).
    pub fn skip(mut self, n: i64) -> Self {
        self.query.steps.push(Step::Skip { n });
        self
    }

    /// Keyset paging: keep rows with `field > value` (prefer over deep [`Self::skip`]).
    pub fn after(self, field: impl AsRef<str>, value: impl Into<Value>) -> Self {
        self.filter(pred::after(field, value))
    }

    pub fn sort(mut self, field: impl AsRef<str>, desc: bool) -> Self {
        self.query.steps.push(Step::Sort {
            field: Field::path(field.as_ref()),
            desc,
        });
        self
    }

    pub fn join(mut self, collection: impl Into<String>, on: impl Into<String>) -> Self {
        self.query.steps.push(Step::Join {
            left: false,
            collection: collection.into(),
            on: on.into(),
        });
        self
    }

    pub fn left_join(mut self, collection: impl Into<String>, on: impl Into<String>) -> Self {
        self.query.steps.push(Step::Join {
            left: true,
            collection: collection.into(),
            on: on.into(),
        });
        self
    }

    pub fn hop(mut self, rel: impl Into<String>) -> Self {
        self.query.steps.push(Step::Hop {
            rel: rel.into(),
            depth: None,
        });
        self
    }

    pub fn hop_depth(mut self, rel: impl Into<String>, depth: i64) -> Self {
        self.query.steps.push(Step::Hop {
            rel: rel.into(),
            depth: Some(depth),
        });
        self
    }

    pub fn graph(mut self, rel: impl Into<String>, depth: Option<i64>) -> Self {
        self.query.steps.push(Step::Graph {
            rel: rel.into(),
            depth,
        });
        self
    }

    pub fn match_path(mut self, path: MatchPath) -> Self {
        self.query.steps.push(Step::Match {
            start: path.start,
            hops: path.hops,
        });
        self
    }

    pub fn search(mut self, q: impl Into<String>) -> Self {
        self.query.steps.push(Step::Search {
            mode: SearchMode::Hybrid,
            query: q.into(),
        });
        self
    }

    pub fn search_lex(mut self, q: impl Into<String>) -> Self {
        self.query.steps.push(Step::Search {
            mode: SearchMode::Lex,
            query: q.into(),
        });
        self
    }

    pub fn search_vec(mut self, q: impl Into<String>) -> Self {
        self.query.steps.push(Step::Search {
            mode: SearchMode::Vec,
            query: q.into(),
        });
        self
    }

    pub fn count(mut self) -> Self {
        self.query.steps.push(Step::Count { by: None });
        self
    }

    pub fn count_by(mut self, field: impl AsRef<str>) -> Self {
        self.query.steps.push(Step::Count {
            by: Some(Field::path(field.as_ref())),
        });
        self
    }

    pub fn union(mut self, other: Queryable) -> Self {
        self.query.steps.push(Step::Union(other.query));
        self
    }

    pub fn run(&self, db: &mut Db) -> Result<Handle, Error> {
        db.run_stmt(self.stmt())
    }

    pub fn run_read(&self, db: &ReadDb) -> Result<Handle, Error> {
        db.run_stmt(self.stmt())
    }

    pub fn to_vec(&self, db: &mut Db) -> Result<Vec<Row>, Error> {
        Ok(self.run(db)?.rows)
    }

    pub fn to_vec_read(&self, db: &ReadDb) -> Result<Vec<Row>, Error> {
        Ok(self.run_read(db)?.rows)
    }

    pub fn to_vec_typed<T: FromRow>(&self, db: &mut Db) -> Result<Vec<T>, Error> {
        row::map_rows(&self.to_vec(db)?)
    }

    pub fn to_vec_typed_read<T: FromRow>(&self, db: &ReadDb) -> Result<Vec<T>, Error> {
        row::map_rows(&self.to_vec_read(db)?)
    }

    pub fn explain(&self, db: &mut Db) -> Result<String, Error> {
        db.explain_stmt(self.stmt(), None)
    }

    pub fn explain_graph(&self, db: &mut Db, fmt: GraphFmt) -> Result<String, Error> {
        db.explain_stmt(self.stmt(), Some(fmt))
    }

    pub fn explain_read(&self, db: &ReadDb) -> Result<String, Error> {
        db.explain_stmt(self.stmt(), None)
    }
}

/// Builder for [`Step::Match`] path patterns.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MatchPath {
    pub start: Option<String>,
    pub hops: Vec<MatchHop>,
}

impl MatchPath {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn start(mut self, bind: impl Into<String>) -> Self {
        self.start = Some(bind.into());
        self
    }

    pub fn fwd(rel: impl Into<String>, bind: impl Into<String>) -> Self {
        Self::new().then_fwd(rel, bind)
    }

    pub fn rev(rel: impl Into<String>, bind: impl Into<String>) -> Self {
        Self::new().then_rev(rel, bind)
    }

    pub fn then_fwd(mut self, rel: impl Into<String>, bind: impl Into<String>) -> Self {
        self.hops.push(MatchHop {
            rel: rel.into(),
            bind: bind.into(),
            edge: None,
            reverse: false,
            min_depth: 1,
            max_depth: 1,
        });
        self
    }

    pub fn then_rev(mut self, rel: impl Into<String>, bind: impl Into<String>) -> Self {
        self.hops.push(MatchHop {
            rel: rel.into(),
            bind: bind.into(),
            edge: None,
            reverse: true,
            min_depth: 1,
            max_depth: 1,
        });
        self
    }

    /// Set depth range on the last hop (`*min..max`, capped at 3 by check).
    pub fn star(mut self, min_depth: i64, max_depth: i64) -> Self {
        if let Some(h) = self.hops.last_mut() {
            h.min_depth = min_depth;
            h.max_depth = max_depth;
        }
        self
    }

    /// Bind the last hop's edge as `-[edge:rel]->`.
    pub fn edge(mut self, edge: impl Into<String>) -> Self {
        if let Some(h) = self.hops.last_mut() {
            h.edge = Some(edge.into());
        }
        self
    }
}

/// Convert various field list forms into `Vec<Field>`.
pub trait IntoFieldList {
    fn into_fields(self) -> Vec<Field>;
}

impl IntoFieldList for Vec<Field> {
    fn into_fields(self) -> Vec<Field> {
        self
    }
}

impl IntoFieldList for &[Field] {
    fn into_fields(self) -> Vec<Field> {
        self.to_vec()
    }
}

impl<const N: usize> IntoFieldList for [Field; N] {
    fn into_fields(self) -> Vec<Field> {
        self.into()
    }
}

impl<const N: usize> IntoFieldList for [&str; N] {
    fn into_fields(self) -> Vec<Field> {
        self.iter().map(|s| Field::path(*s)).collect()
    }
}

impl<const N: usize> IntoFieldList for &[&str; N] {
    fn into_fields(self) -> Vec<Field> {
        self.iter().map(|s| Field::path(*s)).collect()
    }
}

impl IntoFieldList for &[&str] {
    fn into_fields(self) -> Vec<Field> {
        self.iter().map(|s| Field::path(*s)).collect()
    }
}

impl Db {
    /// Start a fluent query against a collection.
    pub fn from(&mut self, collection: impl Into<String>) -> BoundQueryable<'_> {
        BoundQueryable {
            db: BoundDb::Mut(self),
            q: Queryable::from(collection),
        }
    }

    pub fn from_typed<T: LinRow>(&mut self) -> Result<BoundQueryable<'_>, Error> {
        Ok(BoundQueryable {
            db: BoundDb::Mut(self),
            q: Queryable::from_typed::<T>()?,
        })
    }
}

impl ReadDb {
    pub fn from(&self, collection: impl Into<String>) -> BoundQueryable<'_> {
        BoundQueryable {
            db: BoundDb::Read(self),
            q: Queryable::from(collection),
        }
    }

    pub fn from_typed<T: LinRow>(&self) -> Result<BoundQueryable<'_>, Error> {
        Ok(BoundQueryable {
            db: BoundDb::Read(self),
            q: Queryable::from_typed::<T>()?,
        })
    }
}

enum BoundDb<'a> {
    Mut(&'a mut Db),
    Read(&'a ReadDb),
}

/// Fluent query bound to a live [`Db`] or [`ReadDb`] (sync execute).
pub struct BoundQueryable<'a> {
    db: BoundDb<'a>,
    q: Queryable,
}

impl<'a> BoundQueryable<'a> {
    pub fn into_queryable(self) -> Queryable {
        self.q
    }

    pub fn filter(mut self, pred: Pred) -> Self {
        self.q = self.q.filter(pred);
        self
    }

    pub fn select(mut self, fields: impl IntoFieldList) -> Self {
        self.q = self.q.select(fields);
        self
    }

    pub fn select_row<T: LinRow>(mut self) -> Self {
        self.q = self.q.select_row::<T>();
        self
    }

    pub fn take(mut self, n: i64) -> Self {
        self.q = self.q.take(n);
        self
    }

    pub fn take_all(mut self) -> Self {
        self.q = self.q.take_all();
        self
    }

    pub fn skip(mut self, n: i64) -> Self {
        self.q = self.q.skip(n);
        self
    }

    pub fn after(mut self, field: impl AsRef<str>, value: impl Into<Value>) -> Self {
        self.q = self.q.after(field, value);
        self
    }

    pub fn sort(mut self, field: impl AsRef<str>, desc: bool) -> Self {
        self.q = self.q.sort(field, desc);
        self
    }

    pub fn join(mut self, collection: impl Into<String>, on: impl Into<String>) -> Self {
        self.q = self.q.join(collection, on);
        self
    }

    pub fn left_join(mut self, collection: impl Into<String>, on: impl Into<String>) -> Self {
        self.q = self.q.left_join(collection, on);
        self
    }

    pub fn hop(mut self, rel: impl Into<String>) -> Self {
        self.q = self.q.hop(rel);
        self
    }

    pub fn hop_depth(mut self, rel: impl Into<String>, depth: i64) -> Self {
        self.q = self.q.hop_depth(rel, depth);
        self
    }

    pub fn graph(mut self, rel: impl Into<String>, depth: Option<i64>) -> Self {
        self.q = self.q.graph(rel, depth);
        self
    }

    pub fn match_path(mut self, path: MatchPath) -> Self {
        self.q = self.q.match_path(path);
        self
    }

    pub fn search(mut self, query: impl Into<String>) -> Self {
        self.q = self.q.search(query);
        self
    }

    pub fn search_lex(mut self, query: impl Into<String>) -> Self {
        self.q = self.q.search_lex(query);
        self
    }

    pub fn search_vec(mut self, query: impl Into<String>) -> Self {
        self.q = self.q.search_vec(query);
        self
    }

    pub fn count(mut self) -> Self {
        self.q = self.q.count();
        self
    }

    pub fn count_by(mut self, field: impl AsRef<str>) -> Self {
        self.q = self.q.count_by(field);
        self
    }

    pub fn union(mut self, other: Queryable) -> Self {
        self.q = self.q.union(other);
        self
    }

    pub fn explain(self) -> Result<String, Error> {
        match self.db {
            BoundDb::Mut(db) => db.explain_stmt(self.q.stmt(), None),
            BoundDb::Read(db) => db.explain_stmt(self.q.stmt(), None),
        }
    }

    pub fn run(self) -> Result<Handle, Error> {
        match self.db {
            BoundDb::Mut(db) => self.q.run(db),
            BoundDb::Read(db) => self.q.run_read(db),
        }
    }

    pub fn to_vec(self) -> Result<Vec<Row>, Error> {
        Ok(self.run()?.rows)
    }

    pub fn to_vec_typed<T: FromRow>(self) -> Result<Vec<T>, Error> {
        row::map_rows(&self.to_vec()?)
    }
}

impl From<&str> for Value {
    fn from(s: &str) -> Self {
        Value::String(s.to_string())
    }
}

impl From<String> for Value {
    fn from(s: String) -> Self {
        Value::String(s)
    }
}

impl From<i64> for Value {
    fn from(n: i64) -> Self {
        Value::Int(n)
    }
}

impl From<f64> for Value {
    fn from(n: f64) -> Self {
        Value::Float(n)
    }
}

impl From<bool> for Value {
    fn from(b: bool) -> Self {
        Value::Bool(b)
    }
}
