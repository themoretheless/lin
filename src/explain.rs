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
