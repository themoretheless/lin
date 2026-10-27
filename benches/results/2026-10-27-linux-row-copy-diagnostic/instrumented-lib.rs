#![cfg_attr(feature = "gpu", recursion_limit = "256")]
//! # Lin 0.3 — local pipe database
//!
//! ## Stable public API (0.3 freeze)
//!
//! Prefer these entry points; treat other `pub` items as evolving:
//! - [`Db`]: [`Db::empty`], [`Db::fixture`], [`Db::open`], [`Db::open_read`],
//!   [`Db::open_follower`], [`Db::bootstrap_follower`], [`Db::close`], [`Db::checkpoint`],
//!   [`Db::run`], [`Db::execute`], [`Db::scalar`], [`Db::run_group`], [`Db::prepare`], [`Db::explain_as`],
//!   [`Db::reader`], [`Db::export_backup`], [`Db::import_backup`], [`Db::stats`],
//!   [`Db::with_quotas`], [`Db::with_sync_mode`], [`Db::open_with`],
//!   [`Db::export_wal_since`], [`Db::apply_wal`], [`Db::with_embedder`], [`Db::prepare_query`]
//! - [`ReadDb`]: shared read-only snapshot ([`ReadDb::run`], [`ReadDb::clone`])
//! - Fluent / typed: [`Queryable`], [`BoundQueryable`], [`query`], [`QueryCursor`],
//!   [`ProjectedRow`], [`FromRow`], [`LinRow`], [`FromCell`], [`ToCell`], [`Col`], [`map_rows`], [`cell_get`], [`cell_opt`]
//! - Feature `async` (default-on): [`AsyncDb`], [`AsyncReadDb`], `to_vec_async` /
//!   `stream_*` (offload via `spawn_blocking` — not async storage I/O)
//! - Free functions: [`parse`], [`compile`], [`run`], [`explain`], [`explain_as`]
//! - Types: [`Handle`], [`Done`], [`Prepared`], [`Error`], [`Row`], [`Cell`], [`Store`],
//!   [`Plan`], [`Stats`], [`ReopenPhases`], [`Quotas`], [`SyncMode`], [`OpenOpts`], [`Embedder`],
//!   [`HashingEmbedder`], [`VERSION`]
//!
//! Opt-in neural embed (not default): feature `embed-ollama` ([`OllamaEmbedder`]),
//! `embed-onnx` ([`OnnxEmbedder`]).
//!
//! Experimental (outside freeze): [`Db::run_stmt`], [`RowExt`], [`RecordBatch`]
//! (columnar OLAP), [`ship`] WAL pull/serve (TLS + token, one sink).
//!
//! Semver: breaking changes to the stable list require a major bump after 1.0;
//! until then minors may still adjust experimental surfaces (`Stmt`, plan IR,
//! `ship`, `RecordBatch`).
//!
//! ## Search honesty
//!
//! Default `search` / `search hybrid` use **lex + local hashing vec** (feature-hash
//! embedder bound to catalog `embed_id`, not a neural model). `search vec` ranks by
//! cosine over stored `embedding` cells. Swap via [`Db::with_embedder`].
//! Opt-in neural backends: feature `embed-ollama` ([`OllamaEmbedder`]),
//! `embed-onnx` ([`OnnxEmbedder`]).
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
pub mod cold;
pub mod crc;
mod cursor;
mod embed;
#[cfg(feature = "embed-ollama")]
mod embed_ollama;
#[cfg(feature = "embed-onnx")]
mod embed_onnx;
mod error;
mod exec;
mod explain;
mod fts;
#[cfg(feature = "gpu")]
pub mod gpu;
mod graph;
mod index;
mod parse;
pub mod persist;
mod plan;
mod store;
mod window;

pub mod cache;
pub mod query;
pub mod row;
pub mod ship;

#[cfg(feature = "async")]
pub mod async_db;

use std::cell::RefCell;

pub use ast::{
    CmpOp, DurUnit, Duration, ExplainKind, Field, MatchHop, Pred, Query, SearchMode, Source, Step,
    Stmt, Value,
};
pub use batch::RecordBatch;
pub use cursor::{ProjectedRow, QueryCursor};
pub use embed::{
    Embedder, HashingEmbedder, HnswIndex, HnswIndexSq8, cosine, dequantize_sq8, dot_sq8,
    quantize_sq8,
};
#[cfg(feature = "embed-ollama")]
pub use embed_ollama::OllamaEmbedder;
#[cfg(feature = "embed-onnx")]
pub use embed_onnx::{EmbeddingModel, OnnxEmbedder};
pub use error::Error;
pub use exec::{
    Db, Done, Handle, OpenOpts, Prepared, Quotas, ReadDb, ReopenPhases, Stats, SyncMode,
    Transaction,
};
pub use graph::GraphFmt;
pub use plan::Plan;
pub use query::{BoundFilter, BoundQueryable, Col, FilterArg, IntoFieldList, MatchPath, Queryable};
pub use row::{FromCell, FromRow, LinRow, ToCell, cell_get, cell_opt, map_rows};
pub use store::{Cell, InlineStr, Row, RowExt, Store};

#[cfg(feature = "async")]
pub use async_db::{AsyncDb, AsyncReadDb};

#[cfg(feature = "gpu")]
pub use gpu::{GpuAdapterReport, GpuCompute};

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

pub fn parse_pred(src: &str) -> Result<Pred, Error> {
    parse::parse_pred_src(src)
}

/// Dialect comparison → [`Pred`]: `p!(coin > 300)`, `p!(wing == "rag")`.
#[macro_export]
macro_rules! p {
    ($field:ident > $val:expr) => {
        $crate::query::pred::gt(stringify!($field), $val)
    };
    ($field:ident < $val:expr) => {
        $crate::query::pred::lt(stringify!($field), $val)
    };
    ($field:ident >= $val:expr) => {
        $crate::query::pred::ge(stringify!($field), $val)
    };
    ($field:ident <= $val:expr) => {
        $crate::query::pred::le(stringify!($field), $val)
    };
    ($field:ident == $val:expr) => {
        $crate::query::pred::eq(stringify!($field), $val)
    };
    ($field:ident != $val:expr) => {
        $crate::query::pred::ne(stringify!($field), $val)
    };
}

pub fn compile(src: &str) -> Result<Plan, Error> {
    let stmts = parse::parse_program(src)?;
    let cat = catalog::fixture();
    let (_, staged) = check::check_program(&stmts, &cat)?;
    plan::plan_program(&stmts, staged.as_ref().unwrap_or(&cat))
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

pub mod phase_probe {
 use std::sync::{Mutex,atomic::{AtomicBool,AtomicU64,Ordering::Relaxed}};
 pub static ACTIVE:AtomicBool=AtomicBool::new(false);
 static NS:[AtomicU64;28]=[const{AtomicU64::new(0)};28];
 static RECORDS:Mutex<Vec<serde_json::Value>>=Mutex::new(Vec::new());
 pub struct Span(usize,Option<std::time::Instant>);
 impl Span{pub fn new(i:usize)->Self{Self(i,ACTIVE.load(Relaxed).then(std::time::Instant::now))}}
 impl Drop for Span{fn drop(&mut self){if let Some(t)=self.1{NS[self.0].fetch_add(t.elapsed().as_nanos()as u64,Relaxed);}}}
 pub fn begin(){for n in &NS{n.store(0,Relaxed);}ACTIVE.store(true,Relaxed);}
 pub fn end(kind:&str,rows:usize,prepared_ns:u64,handle_drop_ns:u64){ACTIVE.store(false,Relaxed);let ns=NS.iter().map(|v|v.load(Relaxed)).collect::<Vec<_>>();RECORDS.lock().unwrap().push(serde_json::json!({"kind":kind,"rows":rows,"prepared_ns":prepared_ns,"handle_drop_ns":handle_drop_ns,"stages_ns":ns,"diagnostic_only":true}));}
 pub struct Dump;
 impl Drop for Dump{fn drop(&mut self){for r in RECORDS.lock().unwrap().iter(){println!("PHASE {r}");}}}
}

pub mod row_copy_probe {
 use crate::store::{Row,Cell};use std::sync::atomic::{AtomicU8,Ordering::Relaxed};
 static MODE:AtomicU8=AtomicU8::new(0);
 pub fn set_mode(mode:u8){assert!(mode<=2);MODE.store(mode,Relaxed);}
 #[inline(never)] pub fn copy(row:&Row)->Row {match MODE.load(Relaxed){0=>row.clone(),1=>row.iter().map(|(k,v)|(k.clone(),v.clone())).collect(),2=>staged(row),_=>unreachable!()}}
 #[inline(never)] fn staged(row:&Row)->Row {
  let keys:smallvec::SmallVec<[String;16]>={let _span=crate::phase_probe::Span::new(25);row.keys().cloned().collect()};
  let values:smallvec::SmallVec<[Cell;16]>={let _span=crate::phase_probe::Span::new(26);row.values().cloned().collect()};
  let _span=crate::phase_probe::Span::new(27);keys.into_iter().zip(values).collect()
 }
 fn exact(a:&Row,b:&Row){
  assert_eq!(a.len(),b.len());for((ka,a),(kb,b))in a.iter().zip(b){assert_eq!(ka,kb);match(a,b){
   (Cell::Float(a),Cell::Float(b))=>assert_eq!(a.to_bits(),b.to_bits()),
   (Cell::Vec(a),Cell::Vec(b))=>{assert_eq!(a.len(),b.len());assert!(a.iter().zip(b.iter()).all(|(a,b)|a.to_bits()==b.to_bits()));},
   _=>assert_eq!(a,b),
  }}
 }
 pub fn preflight(){
  for fields in [0,1,8,11,12,16,17,32,129]{let mut row=Row::new();for i in 0..fields{row.insert(format!("field-{i:04}-ёж"),match i%8{0=>Cell::Null,1=>Cell::Text(std::sync::Arc::from("owned text")),2=>Cell::SmallText(crate::store::InlineStr::try_new("inline").unwrap()),3=>Cell::Int(i64::MIN),4=>Cell::Float(f64::from_bits(0x7ff8000000000042)),5=>Cell::Bool(true),6=>Cell::Time(i64::MAX),_=>Cell::Vec(std::sync::Arc::from([f32::from_bits(0x7fc00042),-0.0,f32::INFINITY]))});}
   for mode in 0..=2{set_mode(mode);let mut copied=copy(&row);exact(&row,&copied);assert_eq!(rmp_serde::to_vec(&row).unwrap(),rmp_serde::to_vec(&copied).unwrap());copied.insert("extra".into(),Cell::Null);assert!(!row.contains_key("extra"));}
  }
 }
}
