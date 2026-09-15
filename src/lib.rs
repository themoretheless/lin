//! # Lin 0.2 — local pipe database
//!
//! ## Stable public API (0.2 freeze)
//!
//! Prefer these entry points; treat other `pub` items as evolving:
//! - [`Db`]: [`Db::empty`], [`Db::fixture`], [`Db::open`], [`Db::open_read`],
//!   [`Db::open_follower`], [`Db::bootstrap_follower`], [`Db::close`], [`Db::checkpoint`],
//!   [`Db::run`], [`Db::prepare`], [`Db::explain_as`],
//!   [`Db::reader`], [`Db::export_backup`], [`Db::import_backup`], [`Db::stats`],
//!   [`Db::with_quotas`], [`Db::with_sync_mode`], [`Db::open_with`],
//!   [`Db::export_wal_since`], [`Db::apply_wal`]
//! - [`ReadDb`]: shared read-only snapshot ([`ReadDb::run`], [`ReadDb::clone`])
//! - Free functions: [`parse`], [`compile`], [`run`], [`explain`], [`explain_as`]
//! - Types: [`Handle`], [`Done`], [`Prepared`], [`Error`], [`Row`], [`Cell`], [`Store`],
//!   [`Plan`], [`Stats`], [`Quotas`], [`SyncMode`], [`OpenOpts`]
//!
//! Experimental (outside freeze): [`query`] fluent builder, [`FromRow`] / [`LinRow`],
//! [`Db::run_stmt`], [`RowExt`], [`RecordBatch`] (columnar OLAP results), feature `async`
//! ([`AsyncDb`], `stream_*`), [`ship`] TCP WAL pull/serve.
//!
//! Semver: breaking changes to the list above require a major bump after 1.0;
//! until then minors may still adjust experimental surfaces (`Stmt`, plan IR).
//!
//! ## Search honesty
//!
//! Default `search` / `search hybrid` use **lex + local hashing vec** (feature-hash
//! embedder bound to catalog `embed_id`, not a neural model). `search vec` ranks by
//! cosine over stored `embedding` cells. Swap via [`Db::with_embedder`].
//!
//! ## Hot standby
//!
//! Primary: [`Db::open`] + [`Db::export_wal_since`] / [`ship::serve_blocking`].
//! Follower: [`Db::bootstrap_follower`] + [`ship::pull`] + [`Db::apply_wal`].
//! Concurrent readers: [`Db::open_read`] on the follower dir. Not multi-writer.

mod ast;
mod batch;
mod catalog;
mod check;
mod cold;
mod cursor;
mod embed;
mod error;
mod exec;
mod explain;
mod graph;
mod index;
mod parse;
mod persist;
mod plan;
mod store;

pub mod query;
pub mod row;
pub mod ship;

#[cfg(feature = "async")]
pub mod async_db;

use std::cell::RefCell;

pub use ast::{
    CmpOp, Duration, DurUnit, ExplainKind, Field, MatchHop, Pred, Query, SearchMode, Source, Stmt,
    Step, Value,
};
pub use batch::RecordBatch;
pub use cursor::QueryCursor;
pub use embed::{Embedder, HashingEmbedder};
pub use error::Error;
pub use exec::{Db, Done, Handle, OpenOpts, Prepared, Quotas, ReadDb, Stats, SyncMode};
pub use graph::GraphFmt;
pub use plan::Plan;
pub use query::{BoundQueryable, IntoFieldList, MatchPath, Queryable};
pub use row::{FromCell, FromRow, LinRow, cell_get, map_rows};
pub use store::{Cell, Row, RowExt, Store};

#[cfg(feature = "async")]
pub use async_db::{AsyncDb, AsyncReadDb};

#[cfg(feature = "derive")]
pub use lin_derive::LinRow;

/// Crate version string (same as `CARGO_PKG_VERSION`).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

thread_local! {
    static DEFAULT_DB: RefCell<Db> = RefCell::new(Db::fixture());
}

pub fn parse(src: &str) -> Result<Stmt, Error> {
    parse::parse(src)
}

pub fn compile(src: &str) -> Result<Plan, Error> {
    let stmts = parse::parse_program(src)?;
    let mut cat = catalog::fixture();
    check::check_program(&stmts, &mut cat)?;
    plan::plan_program(&stmts, &cat)
}

pub fn explain(src: &str) -> Result<String, Error> {
    explain_as(src, None)
}

pub fn explain_graph(src: &str, fmt: GraphFmt) -> Result<String, Error> {
    explain_as(src, Some(fmt))
}

pub fn explain_as(src: &str, graph: Option<GraphFmt>) -> Result<String, Error> {
    let mut plan = compile(src)?;
    if let Some(fmt) = graph {
        plan.explain = match fmt {
            GraphFmt::Mermaid => ExplainKind::Graph,
            GraphFmt::Dot => ExplainKind::Dot,
        };
    }
    let mut ctx = explain::ExplainCtx::default();
    DEFAULT_DB.with_borrow(|db| {
        ctx.r#gen = Some(db.store.r#gen);
    });
    if plan.explain == ExplainKind::Run {
        let handle = run(src)?;
        ctx.stats = Some(explain::RunStats {
            rows: handle.done.n,
            ms: handle.ms,
        });
        ctx.r#gen = Some(handle.done.r#gen);
        return Ok(explain::format_with(&handle.plan, &ctx));
    }
    Ok(explain::format_with(&plan, &ctx))
}

pub fn run(src: &str) -> Result<Handle, Error> {
    DEFAULT_DB.with_borrow_mut(|db| db.run(src))
}
