mod ast;
mod catalog;
mod check;
mod error;
mod exec;
mod explain;
mod graph;
mod index;
mod parse;
mod persist;
mod plan;
mod store;

use std::cell::RefCell;

pub use ast::{ExplainKind, Stmt};
pub use error::Error;
pub use exec::{Db, Done, Handle, Prepared};
pub use graph::GraphFmt;
pub use plan::Plan;
pub use store::{Cell, Row, Store};

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
