use crate::ast::{ExplainKind, SearchMode};
use crate::graph::{self, GraphFmt};
use crate::plan::{Effect, Node, NodeKind, Plan};

#[derive(Debug, Clone)]
pub struct RunStats {
    pub rows: usize,
    pub ms: f64,
}

#[derive(Debug, Clone)]
pub struct ExplainCtx {
    pub r#gen: Option<u64>,
    pub stats: Option<RunStats>,
    pub sizes: CollectionSizes,
}

impl Default for ExplainCtx {
    fn default() -> Self {
        Self {
            r#gen: Some(0),
            stats: None,
            sizes: CollectionSizes::fixture(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct CollectionSizes {
    pub docs: f64,
    pub users: f64,
    pub orders: f64,
    pub facts: f64,
    pub catalog: f64,
    pub custom: std::collections::BTreeMap<String, f64>,
}

impl CollectionSizes {
    pub fn fixture() -> Self {
        Self {
            docs: 3.0,
            users: 2.0,
            orders: 2.0,
            facts: 0.0,
            catalog: 9.0,
            custom: std::collections::BTreeMap::new(),
        }
    }

    pub fn from_store(store: &crate::store::Store, cat: &crate::catalog::Catalog) -> Self {
        let mut custom = std::collections::BTreeMap::new();
        for name in cat.collections.keys() {
            if !matches!(name.as_str(), "docs" | "users" | "orders" | "facts") {
                custom.insert(name.clone(), store.get_col_count(name) as f64);
            }
        }
        Self {
            docs: store.get_col_count("docs") as f64,
            users: store.get_col_count("users") as f64,
            orders: store.get_col_count("orders") as f64,
            facts: store.get_col_count("facts") as f64,
            catalog: (cat.collections.len()
                + cat.rels.len()
                + cat.fks.len()
                + cat.guards.len()
                + cat.indexes.len()) as f64,
            custom,
        }
    }

    fn get(&self, name: &str) -> f64 {
        match name {
            "docs" => self.docs,
            "users" => self.users,
            "orders" => self.orders,
            "facts" => self.facts,
            "catalog" => self.catalog,
            _ => self.custom.get(name).copied().unwrap_or(8.0),
        }
    }
}

pub fn format_with(plan: &Plan, ctx: &ExplainCtx) -> String {
    match plan.explain {
        ExplainKind::Graph => graph::render(plan, GraphFmt::Mermaid),
        ExplainKind::Dot => graph::render(plan, GraphFmt::Dot),
        other => {
            let node_count = count_nodes(&plan.root);
            let mut costs = Vec::new();
            if other == ExplainKind::Cost {
                costs.reserve(node_count);
                precompute_costs(&plan.root, &ctx.sizes, &mut costs);
            }
            let mut out = String::with_capacity(node_count * 64);
            let mut prefix_buf = String::with_capacity(64);
            let mut idx = 0usize;
            write_node(
                &mut out,
                &mut prefix_buf,
                &plan.root,
                true,
                true,
                other,
                ctx,
                &costs,
                &mut idx,
            );
            out
        }
    }
}

fn count_nodes(node: &Node) -> usize {
    1 + node.children.iter().map(count_nodes).sum::<usize>()
}

fn precompute_costs(
    node: &Node,
    sizes: &CollectionSizes,
    out: &mut Vec<(f64, usize)>,
) -> (f64, usize) {
    let my_idx = out.len();
    out.push((0.0, 3));
    let mut c0_est = 0.0;
    let mut c1_est = 0.0;
    let mut c0_cols = None;
    for (i, child) in node.children.iter().enumerate() {
        let (ce, cc) = precompute_costs(child, sizes, out);
        if i == 0 {
            c0_est = ce;
            c0_cols = Some(cc);
        } else if i == 1 {
            c1_est = ce;
        }
    }
    let cols = match &node.kind {
        NodeKind::Scan { cols, .. } => cols.len().max(1),
        NodeKind::Project { fields } => fields.len().max(1),
        _ => c0_cols.unwrap_or(3),
    };
    let est = match &node.kind {
        NodeKind::Reduce { .. } => c0_est,
        NodeKind::Scan { collection, .. } => sizes.get(collection),
        NodeKind::Get { .. } => 1.0,
        NodeKind::IndexSeek { collection, .. } => (sizes.get(collection) * 0.25).max(1.0),
        NodeKind::FtsSeek { collection, .. } => (sizes.get(collection) * 0.15).max(1.0),
        NodeKind::Filter { .. } => (c0_est * 0.35).max(0.0),
        NodeKind::Project { .. }
        | NodeKind::Without { .. }
        | NodeKind::Sort { .. }
        | NodeKind::Window { .. }
        | NodeKind::Skip { .. }
        | NodeKind::Cas { .. }
        | NodeKind::BatchCAS { .. } => c0_est,
        NodeKind::Take { n, .. } => match n {
            Some(n) => c0_est.min(*n as f64),
            None => c0_est,
        },
        NodeKind::Join { left, .. } => {
            let l = c0_est;
            let r = c1_est.max(1.0);
            if *left {
                l
            } else {
                (l * 0.8).min(l * r * 0.15).max(0.0)
            }
        }
        NodeKind::Union | NodeKind::Seq => c0_est + c1_est,
        NodeKind::Let { .. } | NodeKind::Delete { .. } | NodeKind::DeleteEdge { .. } => {
            c0_est.max(1.0)
        }
        NodeKind::Hop { cap, .. } => c0_est.min(*cap as f64).max(0.0) * 1.4,
        NodeKind::Graph { cap, .. } => c0_est.min(*cap as f64).max(0.0) * 1.4,
        NodeKind::Match { cap, .. } => c0_est.min(*cap as f64).max(0.0) * 1.6,
        NodeKind::Search { k, .. } | NodeKind::Rrf { k } => {
            let inn = if node.children.is_empty() {
                sizes.docs
            } else {
                c0_est
            };
            inn.min(*k as f64)
        }
        NodeKind::Agg { .. } => (c0_est / 3.0).max(1.0),
        NodeKind::Append { .. }
        | NodeKind::InsertPack { .. }
        | NodeKind::Reembed { .. }
        | NodeKind::Schema { .. } => 1.0,
        NodeKind::Split | NodeKind::Exchange => c0_est,
    };
    out[my_idx] = (est, cols);
    (est, cols)
}

#[allow(clippy::too_many_arguments)]
fn write_node(
    out: &mut String,
    prefix_buf: &mut String,
    node: &Node,
    is_last: bool,
    is_root: bool,
    kind: ExplainKind,
    ctx: &ExplainCtx,
    costs: &[(f64, usize)],
    idx: &mut usize,
) {
    let my_idx = *idx;
    *idx += 1;
    if !is_root {
        out.push_str(prefix_buf);
        out.push_str(if is_last { "└─ " } else { "├─ " });
    }
    write_head(out, node, kind, ctx, costs.get(my_idx).copied());
    out.push('\n');

    let prev_len = prefix_buf.len();
    if !is_root {
        prefix_buf.push_str(if is_last { "   " } else { "│  " });
    }
    let n = node.children.len();
    for (i, child) in node.children.iter().enumerate() {
        write_node(
            out,
            prefix_buf,
            child,
            i + 1 == n,
            false,
            kind,
            ctx,
            costs,
            idx,
        );
    }
    prefix_buf.truncate(prev_len);
}

fn push_joined(out: &mut String, items: &[String], sep: &str) {
    for (i, item) in items.iter().enumerate() {
        if i > 0 {
            out.push_str(sep);
        }
        out.push_str(item);
    }
}

fn write_head(
    out: &mut String,
    node: &Node,
    kind: ExplainKind,
    ctx: &ExplainCtx,
    cost: Option<(f64, usize)>,
) {
    use std::fmt::Write as _;
    if kind == ExplainKind::Backend && !matches!(node.kind, NodeKind::Reduce { .. }) {
        out.push_str(short_name(&node.kind));
        out.push_str(" backend=");
        out.push_str(node.backend.as_str());
        return;
    }
    let line_start = out.len();
    match &node.kind {
        NodeKind::Reduce {
            effects,
            snapshot,
            embed,
        } => {
            out.push_str("Reduce effect=");
            for (i, e) in effects.iter().enumerate() {
                if i > 0 {
                    out.push('+');
                }
                out.push_str(e.as_str());
            }
            out.push_str(" snapshot=");
            out.push_str(snapshot);
            out.push_str(" embed=");
            out.push_str(embed);
            if let Some(g) = ctx.r#gen {
                let _ = write!(out, " gen={g}");
            }
        }
        NodeKind::Scan { collection, cols } => {
            out.push_str("Scan ");
            out.push_str(collection);
            out.push_str(" cols=[");
            push_joined(out, cols, ", ");
            out.push_str("] backend=");
            out.push_str(node.backend.as_str());
        }
        NodeKind::Filter { pred } => {
            out.push_str("Filter ");
            out.push_str(pred);
        }
        NodeKind::Project { fields } => {
            out.push_str("Project [");
            push_joined(out, fields, ", ");
            out.push(']');
        }
        NodeKind::Without { fields } => {
            out.push_str("Without [");
            push_joined(out, fields, ", ");
            out.push(']');
        }
        NodeKind::Join { left, fk } => {
            let how = if *left { "left" } else { "inner" };
            out.push_str("Join ");
            out.push_str(how);
            out.push_str(" fk=");
            out.push_str(fk);
            out.push_str(" backend=");
            out.push_str(node.backend.as_str());
        }
        NodeKind::Hop { rel, depth, cap } => {
            let _ = write!(
                out,
                "Hop {rel} depth={depth} cap={cap} backend={}",
                node.backend.as_str()
            );
        }
        NodeKind::Graph { rel, depth, cap } => {
            let _ = write!(
                out,
                "Graph {rel} depth={depth} cap={cap} backend={}",
                node.backend.as_str()
            );
        }
        NodeKind::Match { start, hops, cap } => {
            out.push_str("Match ");
            if let Some(a) = start {
                out.push_str(a);
                out.push(' ');
            }
            push_joined(out, hops, " ");
            let _ = write!(out, " cap={cap} backend={}", node.backend.as_str());
        }
        NodeKind::Search { mode, query, k } => {
            let m = match mode {
                SearchMode::Hybrid => "hybrid",
                SearchMode::Lex => "lex",
                SearchMode::Vec => "vec",
            };
            let _ = write!(
                out,
                "Search {m} {query:?} k={k} backend={}",
                node.backend.as_str()
            );
        }
        NodeKind::Rrf { k } => {
            let _ = write!(out, "RRF k={k}");
        }
        NodeKind::Agg { op, by } => {
            out.push_str("Agg ");
            out.push_str(op);
            out.push_str(" by ");
            out.push_str(by);
        }
        NodeKind::Sort { field, desc } => {
            out.push_str("Sort ");
            out.push_str(field);
            out.push_str(if *desc { " desc" } else { " asc" });
        }
        NodeKind::Window {
            func,
            by,
            order,
            frame,
            as_field,
        } => {
            out.push_str("Window ");
            out.push_str(func);
            if !by.is_empty() {
                out.push_str(" by ");
                push_joined(out, by, ", ");
            }
            if !order.is_empty() {
                out.push_str(" sort ");
                for (i, (f, desc)) in order.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    out.push_str(f);
                    out.push_str(if *desc { " desc" } else { " asc" });
                }
            }
            if let Some(frame) = frame {
                out.push(' ');
                out.push_str(frame);
            }
            out.push_str(" as ");
            out.push_str(as_field);
        }
        NodeKind::Skip { n } => {
            let _ = write!(out, "Skip {n}");
        }
        NodeKind::Take { n, implicit } => match n {
            Some(n) if *implicit => {
                let _ = write!(out, "Take {n} implicit");
            }
            Some(n) => {
                let _ = write!(out, "Take {n}");
            }
            None => out.push_str("Take all"),
        },
        NodeKind::Get { collection, key } => {
            out.push_str("Get ");
            out.push_str(collection);
            out.push(' ');
            out.push_str(key);
            out.push_str(" backend=");
            out.push_str(node.backend.as_str());
        }
        NodeKind::Append { kind, detail, n } => {
            if *n > 1 {
                let _ = write!(out, "Append {kind} n={n}");
            } else {
                out.push_str("Append ");
                out.push_str(kind);
                out.push(' ');
                out.push_str(detail);
            }
        }
        NodeKind::InsertPack {
            collection,
            edges,
            n,
        } => {
            out.push_str("InsertPack ");
            out.push_str(collection);
            if *n > 1 {
                let _ = write!(out, " n={n}");
            }
            if !edges.is_empty() {
                out.push_str(" edges=[");
                push_joined(out, edges, ", ");
                out.push(']');
            }
        }
        NodeKind::Cas { hash } => {
            let _ = write!(out, "CAS hash={hash:?}");
        }
        NodeKind::BatchCAS { n } => match n {
            Some(n) => {
                let _ = write!(out, "BatchCAS n={n}");
            }
            None => out.push_str("BatchCAS n=?"),
        },
        NodeKind::IndexSeek { collection, fields } => {
            out.push_str("IndexSeek index=");
            out.push_str(collection);
            out.push('[');
            push_joined(out, fields, ",");
            out.push(']');
        }
        NodeKind::FtsSeek {
            collection,
            fields,
            query,
        } => {
            out.push_str("FtsSeek ");
            out.push_str(collection);
            out.push('[');
            push_joined(out, fields, ",");
            let _ = write!(out, "] query={query:?}");
        }
        NodeKind::Schema { detail } => {
            out.push_str("Schema ");
            out.push_str(detail);
        }
        NodeKind::Reembed { collection, to } => {
            out.push_str("Reembed ");
            out.push_str(collection);
            if let Some(t) = to {
                out.push_str(" to ");
                out.push_str(t);
            }
        }
        NodeKind::Delete { collection } => {
            out.push_str("Delete ");
            out.push_str(collection);
        }
        NodeKind::DeleteEdge { rel, from, to } => {
            out.push_str("DeleteEdge ");
            out.push_str(rel);
            out.push(' ');
            out.push_str(from);
            out.push_str(" -> ");
            out.push_str(to);
        }
        NodeKind::Union => out.push_str("Union"),
        NodeKind::Let { name } => {
            out.push_str("Let ");
            out.push_str(name);
        }
        NodeKind::Seq => out.push_str("Seq"),
        NodeKind::Split => out.push_str("Split"),
        NodeKind::Exchange => out.push_str("Exchange"),
    }

    if !matches!(node.kind, NodeKind::Reduce { .. })
        && !matches!(
            node.kind,
            NodeKind::Scan { .. }
                | NodeKind::Get { .. }
                | NodeKind::Join { .. }
                | NodeKind::Hop { .. }
                | NodeKind::Graph { .. }
                | NodeKind::Match { .. }
                | NodeKind::Search { .. }
                | NodeKind::FtsSeek { .. }
        )
        && matches!(kind, ExplainKind::Cost | ExplainKind::Tree)
        && !out[line_start..].contains("backend=")
    {
        out.push_str(" backend=");
        out.push_str(node.backend.as_str());
    }

    if node.effect != Effect::Read && !matches!(node.kind, NodeKind::Reduce { .. }) {
        out.push_str(" effect=");
        out.push_str(node.effect.as_str());
    }

    match &node.kind {
        NodeKind::Filter { .. } => out.push_str(" zone skip"),
        NodeKind::Hop { .. } => out.push_str(" serial"),
        NodeKind::Graph { .. } => out.push_str(" serial"),
        NodeKind::Match { .. } => out.push_str(" serial"),
        NodeKind::Rrf { .. } => out.push_str(" parallel"),
        _ => {}
    }

    for note in &node.notes {
        out.push(' ');
        out.push_str(note);
    }

    if kind == ExplainKind::Cost {
        let (est, cols) = cost.unwrap_or((0.0, 3));
        let bytes = (est * 24.0 * cols as f64).round() as i64;
        let _ = write!(out, " est.rows={est:.0} est.bytes={bytes}");
        match &node.kind {
            NodeKind::Take { n: Some(n), .. } => {
                let _ = write!(out, " budget.take={n}");
            }
            NodeKind::Scan { cols, .. } => {
                if !cols.iter().any(|c| c == "body") {
                    out.push_str(" -- no body");
                }
            }
            NodeKind::Hop { .. } | NodeKind::Graph { .. } | NodeKind::Match { .. } => {
                out.push_str(" after search");
            }
            NodeKind::Get { .. } => out.push_str(" point"),
            _ => {}
        }
    }

    if kind == ExplainKind::Run {
        if let Some(st) = &ctx.stats {
            if matches!(node.kind, NodeKind::Reduce { .. } | NodeKind::Take { .. }) {
                let _ = write!(out, " run=ok rows={} ms={:.2}", st.rows, st.ms);
            } else {
                out.push_str(" run=ok");
            }
        } else {
            out.push_str(" run=not-executed");
        }
    }
}

fn short_name(kind: &NodeKind) -> &'static str {
    kind.short_name()
}

// ============================================================================
// Structured EXPLAIN Output (`LIN-0507`, `LIN-0557`..`LIN-0560`, `LIN-0589`, `LIN-1463`)
// ============================================================================

use serde::{Deserialize, Serialize};

use crate::catalog::Catalog;
use crate::error::Error;
use crate::stats::{
    CostEstimator, SelectivityFeedbackReceipt, StatsCatalog, compare_estimate_actual,
};

/// Selected physical execution backend reported by [`StructuredExplain`] (`LIN-0557`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExplainBackend {
    InMemoryRow,
    ColumnarBatch,
    ColdDisk,
    GpuCandidate,
}

/// Exactness contract of a query plan (`LIN-0559`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlanAccuracy {
    Exact,
    Approximate,
}

/// Machine-readable operator node in [`StructuredExplain`] (`LIN-1463`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExplainNode {
    pub operator: String,
    pub detail: String,
    pub backend: ExplainBackend,
    pub effect: String,
    pub estimated_rows: Option<usize>,
    pub estimated_bytes: Option<usize>,
    pub confidence: Option<f64>,
    pub notes: Vec<String>,
    pub children: Vec<ExplainNode>,
}

/// Machine-readable `EXPLAIN` summary with fingerprints, output column types, backend/GPU
/// eligibility reasoning, accuracy labels, and calibrated cardinality/memory estimates
/// (`LIN-0507`, `LIN-0557`..`LIN-0560`, `LIN-1463`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StructuredExplain {
    pub plan_fingerprint: String,
    pub backend: ExplainBackend,
    pub gpu_eligibility: String,
    pub accuracy: PlanAccuracy,
    pub output_fields: Vec<(String, String)>,
    pub estimated_rows: Option<usize>,
    pub estimated_bytes: Option<usize>,
    pub steps: Vec<ExplainNode>,
}

/// Compare a [`StructuredExplain`] cardinality estimate against actual execution rows (`LIN-0589`, `LIN-0590`).
pub fn compare_explain_with_actual(
    explain: &StructuredExplain,
    actual_rows: usize,
) -> SelectivityFeedbackReceipt {
    compare_estimate_actual(explain.estimated_rows.unwrap_or(0), actual_rows)
}

/// Build a [`StructuredExplain`] against the default fixture catalog (`LIN-1463`).
pub fn explain_structured(
    src: &str,
    stats: Option<&StatsCatalog>,
    gpu_attached: bool,
) -> Result<StructuredExplain, Error> {
    let cat = crate::catalog::fixture();
    explain_structured_with_catalog(src, &cat, stats, gpu_attached, &[])
}

/// Build a [`StructuredExplain`] against an explicit [`Catalog`], optional [`StatsCatalog`],
/// GPU attachment flag, and cold-disk collection list (`LIN-0507`, `LIN-0557`..`LIN-0560`, `LIN-1463`).
pub fn explain_structured_with_catalog(
    src: &str,
    cat: &Catalog,
    stats: Option<&StatsCatalog>,
    gpu_attached: bool,
    cold_collections: &[&str],
) -> Result<StructuredExplain, Error> {
    let stmts = crate::parse::parse_program(src)?;
    let (staged, output_fields) = crate::check::check_program_output_fields(&stmts, cat)?;
    let effective_cat = staged.as_ref().unwrap_or(cat);
    let plan = crate::plan::plan_program(&stmts, effective_cat)?;

    let fp = crate::plan::plan_fingerprint(&plan);
    let plan_fingerprint = format!("p:{fp:016x}");
    let accuracy = classify_plan_accuracy(&plan.root);

    let estimator = CostEstimator::default();
    let fallback_sizes = CollectionSizes::fixture();

    let (scan_collection, scan_rows, has_write, has_graph_or_point, has_gpu_incompatible_op) =
        inspect_plan_properties(&plan.root, stats, &fallback_sizes);

    let is_cold = scan_collection
        .as_deref()
        .is_some_and(|c| cold_collections.contains(&c));

    let (backend, gpu_eligibility) = classify_backend_and_gpu(
        &plan.root,
        is_cold,
        gpu_attached,
        scan_rows,
        has_write,
        has_graph_or_point,
        has_gpu_incompatible_op,
    );

    let root_explained = build_explain_tree(
        &plan.root,
        effective_cat,
        stats,
        &estimator,
        &fallback_sizes,
        backend,
        scan_collection.as_deref(),
    );

    let estimated_rows = root_explained.estimated_rows;
    let estimated_bytes = root_explained.estimated_bytes;
    let mut steps = Vec::new();
    flatten_pipeline_spine(root_explained, &mut steps);

    Ok(StructuredExplain {
        plan_fingerprint,
        backend,
        gpu_eligibility,
        accuracy,
        output_fields,
        estimated_rows,
        estimated_bytes,
        steps,
    })
}

fn classify_plan_accuracy(node: &Node) -> PlanAccuracy {
    match &node.kind {
        NodeKind::Search {
            mode: SearchMode::Vec | SearchMode::Hybrid,
            ..
        }
        | NodeKind::Rrf { .. } => PlanAccuracy::Approximate,
        _ => {
            for c in &node.children {
                if classify_plan_accuracy(c) == PlanAccuracy::Approximate {
                    return PlanAccuracy::Approximate;
                }
            }
            PlanAccuracy::Exact
        }
    }
}

fn inspect_plan_properties(
    node: &Node,
    stats: Option<&StatsCatalog>,
    fallback: &CollectionSizes,
) -> (Option<String>, usize, bool, bool, bool) {
    let mut scan_col: Option<String> = None;
    let mut scan_rows = 0usize;
    let mut has_write = node.effect != Effect::Read;
    let mut has_graph_or_point = false;
    let mut has_gpu_incompat = false;

    fn walk(
        n: &Node,
        stats: Option<&StatsCatalog>,
        fallback: &CollectionSizes,
        scan_col: &mut Option<String>,
        scan_rows: &mut usize,
        has_write: &mut bool,
        has_graph_or_point: &mut bool,
        has_gpu_incompat: &mut bool,
    ) {
        if n.effect != Effect::Read {
            *has_write = true;
        }
        if n.backend == crate::plan::Backend::Idb && !matches!(n.kind, NodeKind::Get { .. }) {
            *has_gpu_incompat = true;
        }
        match &n.kind {
            NodeKind::Reduce { effects, .. } => {
                if effects.iter().any(|e| *e != Effect::Read) {
                    *has_write = true;
                }
            }
            NodeKind::Scan { collection, .. }
            | NodeKind::IndexSeek { collection, .. }
            | NodeKind::FtsSeek { collection, .. } => {
                if scan_col.is_none() {
                    *scan_col = Some(collection.clone());
                    *scan_rows = stats
                        .and_then(|s| s.inspect(collection))
                        .map(|ts| ts.row_count)
                        .unwrap_or_else(|| fallback.get(collection).round().max(0.0) as usize);
                }
                if matches!(n.kind, NodeKind::FtsSeek { .. }) {
                    *has_gpu_incompat = true;
                }
            }
            NodeKind::Get { collection, .. } => {
                if scan_col.is_none() {
                    *scan_col = Some(collection.clone());
                    *scan_rows = 1;
                }
                *has_graph_or_point = true;
            }
            NodeKind::Hop { .. } | NodeKind::Graph { .. } | NodeKind::Match { .. } => {
                *has_graph_or_point = true;
            }
            NodeKind::Window { .. }
            | NodeKind::Search {
                mode: SearchMode::Lex | SearchMode::Hybrid,
                ..
            } => {
                *has_gpu_incompat = true;
            }
            NodeKind::Append { .. }
            | NodeKind::InsertPack { .. }
            | NodeKind::Cas { .. }
            | NodeKind::BatchCAS { .. }
            | NodeKind::Delete { .. }
            | NodeKind::DeleteEdge { .. }
            | NodeKind::Reembed { .. }
            | NodeKind::Schema { .. } => {
                *has_write = true;
            }
            _ => {}
        }
        for c in &n.children {
            walk(
                c,
                stats,
                fallback,
                scan_col,
                scan_rows,
                has_write,
                has_graph_or_point,
                has_gpu_incompat,
            );
        }
    }

    walk(
        node,
        stats,
        fallback,
        &mut scan_col,
        &mut scan_rows,
        &mut has_write,
        &mut has_graph_or_point,
        &mut has_gpu_incompat,
    );
    (
        scan_col,
        scan_rows,
        has_write,
        has_graph_or_point,
        has_gpu_incompat,
    )
}

fn classify_backend_and_gpu(
    root: &Node,
    is_cold: bool,
    gpu_attached: bool,
    scan_rows: usize,
    has_write: bool,
    has_graph_or_point: bool,
    has_gpu_incompat: bool,
) -> (ExplainBackend, String) {
    const GPU_CROSSOVER_ROWS: usize = 1024;
    const BATCH_CROSSOVER_ROWS: usize = 256;

    if has_write {
        return (
            ExplainBackend::InMemoryRow,
            "ineligible: write or schema mutation executes on CPU row engine".into(),
        );
    }
    if is_cold {
        let gpu_reason = if !gpu_attached {
            "ineligible: cold-disk collection scanned via mmap segments (and no GPU attached)"
                .into()
        } else {
            "ineligible: cold-disk collection scanned via mmap segments before resident upload"
                .into()
        };
        return (ExplainBackend::ColdDisk, gpu_reason);
    }
    if has_graph_or_point {
        return (
            ExplainBackend::InMemoryRow,
            "ineligible: point lookup or graph traversal executes on CPU".into(),
        );
    }
    if has_gpu_incompat {
        let backend = if scan_rows >= BATCH_CROSSOVER_ROWS && is_columnar_pipeline(root) {
            ExplainBackend::ColumnarBatch
        } else {
            ExplainBackend::InMemoryRow
        };
        return (
            backend,
            "ineligible: operator pipeline contains FTS/window/IDB step unsupported on GPU".into(),
        );
    }
    if scan_rows >= GPU_CROSSOVER_ROWS {
        if gpu_attached {
            return (
                ExplainBackend::GpuCandidate,
                format!(
                    "eligible: GPU attached and input cardinality ({scan_rows}) >= {GPU_CROSSOVER_ROWS} crossover"
                ),
            );
        }
        let cpu_backend = if is_columnar_pipeline(root) {
            ExplainBackend::ColumnarBatch
        } else {
            ExplainBackend::InMemoryRow
        };
        return (
            cpu_backend,
            format!(
                "ineligible: GPU not attached (pipeline is GPU-compatible at {scan_rows} rows)"
            ),
        );
    }

    let gpu_reason = if gpu_attached {
        format!(
            "ineligible: input cardinality ({scan_rows}) below GPU transfer crossover ({GPU_CROSSOVER_ROWS} rows)"
        )
    } else {
        format!(
            "ineligible: GPU not attached and input cardinality ({scan_rows}) below crossover ({GPU_CROSSOVER_ROWS} rows)"
        )
    };

    let backend = if scan_rows >= BATCH_CROSSOVER_ROWS && is_columnar_pipeline(root) {
        ExplainBackend::ColumnarBatch
    } else {
        ExplainBackend::InMemoryRow
    };
    (backend, gpu_reason)
}

fn is_columnar_pipeline(node: &Node) -> bool {
    match &node.kind {
        NodeKind::Reduce { .. }
        | NodeKind::Scan { .. }
        | NodeKind::IndexSeek { .. }
        | NodeKind::Filter { .. }
        | NodeKind::Project { .. }
        | NodeKind::Without { .. }
        | NodeKind::Sort { .. }
        | NodeKind::Skip { .. }
        | NodeKind::Take { .. }
        | NodeKind::Agg { .. }
        | NodeKind::Join { .. } => node.children.iter().all(is_columnar_pipeline),
        _ => false,
    }
}

fn build_explain_tree(
    node: &Node,
    cat: &Catalog,
    stats: Option<&StatsCatalog>,
    estimator: &CostEstimator,
    fallback: &CollectionSizes,
    plan_backend: ExplainBackend,
    primary_col: Option<&str>,
) -> ExplainNode {
    let children: Vec<ExplainNode> = node
        .children
        .iter()
        .map(|c| {
            build_explain_tree(
                c,
                cat,
                stats,
                estimator,
                fallback,
                plan_backend,
                primary_col,
            )
        })
        .collect();

    let c0_rows = children.first().and_then(|c| c.estimated_rows).unwrap_or(0);
    let c0_conf = children.first().and_then(|c| c.confidence).unwrap_or(0.5);
    let c1_rows = children.get(1).and_then(|c| c.estimated_rows).unwrap_or(0);

    let avg_row_bytes_for = |col_name: &str, num_cols: usize| -> usize {
        stats
            .and_then(|s| s.inspect(col_name))
            .map(|ts| ts.avg_row_bytes.max(24 * num_cols.max(1)))
            .unwrap_or_else(|| 24 * num_cols.max(1))
    };

    let default_row_bytes = primary_col
        .map(|c| avg_row_bytes_for(c, 3))
        .unwrap_or(72);

    let (est_rows, est_bytes, conf) = match &node.kind {
        NodeKind::Reduce { .. } => {
            let b = children
                .first()
                .and_then(|c| c.estimated_bytes)
                .unwrap_or(c0_rows * default_row_bytes);
            (c0_rows, b, c0_conf)
        }
        NodeKind::Scan { collection, cols } => match stats.and_then(|s| s.inspect(collection)) {
            Some(ts) => {
                let col_bytes = (cols.len().max(1) * 24).min(ts.avg_row_bytes.max(24));
                (ts.row_count, ts.row_count * col_bytes, 1.0)
            }
            None => {
                let r = fallback.get(collection).round().max(0.0) as usize;
                (r, r * cols.len().max(1) * 24, 0.35)
            }
        },
        NodeKind::Get { collection, .. } => {
            let rb = avg_row_bytes_for(collection, 4);
            (1, rb, 1.0)
        }
        NodeKind::IndexSeek { collection, fields } => {
            match stats.and_then(|s| s.inspect(collection)) {
                Some(ts) => {
                    let ndv = fields
                        .first()
                        .and_then(|f| ts.columns.get(f))
                        .map(|c| c.distinct_estimate.max(1))
                        .unwrap_or(4);
                    let r = (ts.row_count / ndv).max(usize::from(ts.row_count > 0));
                    let rb = avg_row_bytes_for(collection, fields.len() + 1);
                    (r, r * rb, 0.85)
                }
                None => {
                    let r = (fallback.get(collection) * 0.25).round().max(1.0) as usize;
                    (r, r * 48, 0.35)
                }
            }
        }
        NodeKind::FtsSeek { collection, .. } => match stats.and_then(|s| s.inspect(collection)) {
            Some(ts) => {
                let r = ((ts.row_count as f64) * 0.15).round() as usize;
                let r = r.min(ts.row_count);
                (r, r * avg_row_bytes_for(collection, 3), 0.6)
            }
            None => {
                let r = (fallback.get(collection) * 0.15).round().max(1.0) as usize;
                (r, r * 72, 0.35)
            }
        },
        NodeKind::Filter { pred } => {
            if let Some(col_name) = primary_col
                && let Some(ts) = stats.and_then(|s| s.inspect(col_name))
                && let Ok(parsed_pred) = crate::parse::parse_pred_src(pred)
            {
                let est = estimator.estimate_residual_filter(ts, &parsed_pred);
                // When child is already narrowed (e.g. IndexSeek), cap by child rows or use est directly
                // when child is IndexSeek on the same predicate!
                let child_is_seek = node
                    .children
                    .first()
                    .is_some_and(|c| matches!(c.kind, NodeKind::IndexSeek { .. }));
                let r = if child_is_seek {
                    est.rows.min(ts.row_count)
                } else if ts.row_count > 0 && c0_rows < ts.row_count {
                    let sel = est.rows as f64 / ts.row_count as f64;
                    ((c0_rows as f64) * sel).round() as usize
                } else {
                    est.rows
                };
                (r, r * default_row_bytes, est.confidence)
            } else {
                let r = ((c0_rows as f64) * 0.35).round() as usize;
                (r, r * default_row_bytes, (c0_conf * 0.6).clamp(0.1, 1.0))
            }
        }
        NodeKind::Project { fields } => {
            let rb = fields.len().max(1) * 24;
            (c0_rows, c0_rows * rb, c0_conf)
        }
        NodeKind::Without { fields } => {
            let rem = cat
                .collection(primary_col.unwrap_or(""))
                .map(|c| c.fields.len().saturating_sub(fields.len()).max(1))
                .unwrap_or(2);
            (c0_rows, c0_rows * rem * 24, c0_conf)
        }
        NodeKind::Sort { .. } => {
            let bytes = estimator.estimate_sort_bytes(c0_rows, default_row_bytes, None);
            (c0_rows, bytes, c0_conf)
        }
        NodeKind::Skip { n } => {
            let r = c0_rows.saturating_sub((*n).max(0) as usize);
            (r, r * default_row_bytes, c0_conf)
        }
        NodeKind::Take { n, .. } => {
            let r = match n {
                Some(k) => c0_rows.min((*k).max(0) as usize),
                None => c0_rows,
            };
            let b = children
                .first()
                .map(|c| {
                    let per_row = if c0_rows > 0 {
                        c.estimated_bytes.unwrap_or(default_row_bytes) / c0_rows
                    } else {
                        default_row_bytes
                    };
                    r * per_row.max(24)
                })
                .unwrap_or(r * default_row_bytes);
            (r, b, c0_conf)
        }
        NodeKind::Agg { by, .. } => {
            if by == "*" {
                (1, 32, c0_conf)
            } else if let Some(col_name) = primary_col
                && let Some(ts) = stats.and_then(|s| s.inspect(col_name))
            {
                let group_fields: Vec<&str> = by.split(',').map(str::trim).collect();
                let g = estimator.estimate_agg_groups(ts, &group_fields).min(c0_rows.max(1));
                let g = if c0_rows == 0 { 0 } else { g };
                (g, g * 48, (c0_conf * 0.85).clamp(0.1, 1.0))
            } else {
                let g = if c0_rows == 0 { 0 } else { (c0_rows / 3).max(1) };
                (g, g * 48, (c0_conf * 0.5).clamp(0.1, 1.0))
            }
        }
        NodeKind::Join { left, fk } => {
            let right_col = node.children.get(1).and_then(|c| match &c.kind {
                NodeKind::Scan { collection, .. } => Some(collection.as_str()),
                _ => None,
            });
            if let (Some(lcol), Some(rcol)) = (primary_col, right_col)
                && let (Some(lts), Some(rts)) = (
                    stats.and_then(|s| s.inspect(lcol)),
                    stats.and_then(|s| s.inspect(rcol)),
                )
            {
                let (lkey, rkey) = parse_fk_keys(fk);
                let mut scaled_left = lts.clone();
                scaled_left.row_count = c0_rows;
                let j = estimator.estimate_join(&scaled_left, rts, lkey, rkey);
                let out_r = if *left { j.output_rows.max(c0_rows) } else { j.output_rows };
                (
                    out_r,
                    j.build_bytes + out_r * (default_row_bytes + 48),
                    (c0_conf * 0.8).clamp(0.1, 1.0),
                )
            } else {
                let out_r = if *left {
                    c0_rows
                } else {
                    ((c0_rows as f64) * 0.8).round() as usize
                };
                (
                    out_r,
                    (c1_rows + out_r) * default_row_bytes,
                    (c0_conf * 0.5).clamp(0.1, 1.0),
                )
            }
        }
        NodeKind::Hop { depth, cap, .. } | NodeKind::Graph { depth, cap, .. } => {
            let est = estimator.estimate_graph_expansion(
                c0_rows,
                2.0,
                (*depth).max(1) as usize,
                (*cap).max(0) as usize,
            );
            (est.rows, est.rows * default_row_bytes, est.confidence)
        }
        NodeKind::Match { hops, cap, .. } => {
            let est = estimator.estimate_graph_expansion(
                c0_rows,
                2.0,
                hops.len().max(1),
                (*cap).max(0) as usize,
            );
            (est.rows, est.rows * default_row_bytes, est.confidence)
        }
        NodeKind::Search { k, .. } | NodeKind::Rrf { k } => {
            let r = c0_rows.min((*k).max(0) as usize);
            (r, r * default_row_bytes, 0.75)
        }
        NodeKind::Union | NodeKind::Seq => {
            let r = c0_rows + c1_rows;
            (r, r * default_row_bytes, c0_conf)
        }
        NodeKind::Window { .. } => (c0_rows, c0_rows * (default_row_bytes + 16), c0_conf),
        NodeKind::Append { n, .. } | NodeKind::InsertPack { n, .. } => (*n, *n * 64, 1.0),
        _ => (c0_rows.max(1), default_row_bytes, 1.0),
    };

    let node_backend = match &node.kind {
        NodeKind::Get { .. }
        | NodeKind::Hop { .. }
        | NodeKind::Graph { .. }
        | NodeKind::Match { .. } => ExplainBackend::InMemoryRow,
        _ => plan_backend,
    };

    ExplainNode {
        operator: node.kind.short_name().to_string(),
        detail: describe_node_detail(&node.kind),
        backend: node_backend,
        effect: node.effect.as_str().to_string(),
        estimated_rows: Some(est_rows),
        estimated_bytes: Some(est_bytes),
        confidence: Some(conf),
        notes: node.notes.clone(),
        children,
    }
}

fn parse_fk_keys(fk: &str) -> (&str, &str) {
    if let Some((lhs, rhs)) = fk.split_once('→') {
        let lk = lhs.rsplit('.').next().unwrap_or(lhs);
        let rk = rhs.rsplit('.').next().unwrap_or("id");
        (lk, rk)
    } else {
        ("id", "id")
    }
}

fn describe_node_detail(kind: &NodeKind) -> String {
    match kind {
        NodeKind::Reduce {
            effects,
            snapshot,
            embed,
        } => {
            let eff: Vec<&str> = effects.iter().copied().map(Effect::as_str).collect();
            format!("effect={} snapshot={snapshot} embed={embed}", eff.join("+"))
        }
        NodeKind::Scan { collection, cols } => {
            format!("{collection} cols=[{}]", cols.join(", "))
        }
        NodeKind::Filter { pred } => pred.clone(),
        NodeKind::Project { fields } | NodeKind::Without { fields } => {
            format!("[{}]", fields.join(", "))
        }
        NodeKind::Join { left, fk } => {
            format!("{} fk={fk}", if *left { "left" } else { "inner" })
        }
        NodeKind::Hop { rel, depth, cap } | NodeKind::Graph { rel, depth, cap } => {
            format!("{rel} depth={depth} cap={cap}")
        }
        NodeKind::Match { start, hops, cap } => {
            let prefix = start.as_deref().unwrap_or("");
            format!("{prefix} {} cap={cap}", hops.join(" "))
                .trim()
                .to_string()
        }
        NodeKind::Search { mode, query, k } => {
            let m = match mode {
                SearchMode::Hybrid => "hybrid",
                SearchMode::Lex => "lex",
                SearchMode::Vec => "vec",
            };
            format!("{m} {query:?} k={k}")
        }
        NodeKind::Rrf { k } => format!("k={k}"),
        NodeKind::Agg { op, by } => format!("{op} by {by}"),
        NodeKind::Sort { field, desc } => {
            format!("{field} {}", if *desc { "desc" } else { "asc" })
        }
        NodeKind::Window {
            func, as_field, ..
        } => format!("{func} as {as_field}"),
        NodeKind::Skip { n } => format!("{n}"),
        NodeKind::Take { n, implicit } => match n {
            Some(v) if *implicit => format!("{v} implicit"),
            Some(v) => format!("{v}"),
            None => "all".into(),
        },
        NodeKind::Get { collection, key } => format!("{collection} {key}"),
        NodeKind::IndexSeek { collection, fields } => {
            format!("index={collection}[{}]", fields.join(","))
        }
        NodeKind::FtsSeek {
            collection,
            fields,
            query,
        } => format!("{collection}[{}] query={query:?}", fields.join(",")),
        NodeKind::Schema { detail } => detail.clone(),
        NodeKind::Append { kind, detail, n } => format!("{kind} {detail} n={n}"),
        NodeKind::InsertPack { collection, n, .. } => format!("{collection} n={n}"),
        NodeKind::Cas { hash } => format!("hash={hash:?}"),
        NodeKind::BatchCAS { n } => format!("n={n:?}"),
        NodeKind::Reembed { collection, to } => format!("{collection} to={to:?}"),
        NodeKind::Delete { collection } => collection.clone(),
        NodeKind::DeleteEdge { rel, from, to } => format!("{rel} {from} -> {to}"),
        NodeKind::Let { name } => name.clone(),
        NodeKind::Union | NodeKind::Seq | NodeKind::Split | NodeKind::Exchange => String::new(),
    }
}

/// Flatten the primary pipe spine in execution order (`Scan`/`IndexSeek` -> ... -> `Reduce`),
/// preserving side-input `children` on multi-input operators (`Join`, `Union`, `Seq`).
fn flatten_pipeline_spine(mut node: ExplainNode, out: &mut Vec<ExplainNode>) {
    if !node.children.is_empty() {
        let primary_child = node.children.remove(0);
        flatten_pipeline_spine(primary_child, out);
    }
    out.push(node);
}
