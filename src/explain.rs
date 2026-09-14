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
}

impl CollectionSizes {
    pub fn fixture() -> Self {
        Self {
            docs: 3.0,
            users: 2.0,
            orders: 2.0,
            facts: 0.0,
            catalog: 9.0,
        }
    }

    fn get(&self, name: &str) -> f64 {
        match name {
            "docs" => self.docs,
            "users" => self.users,
            "orders" => self.orders,
            "facts" => self.facts,
            "catalog" => self.catalog,
            _ => 8.0,
        }
    }
}

pub fn format_with(plan: &Plan, ctx: &ExplainCtx) -> String {
    match plan.explain {
        ExplainKind::Graph => graph::render(plan, GraphFmt::Mermaid),
        ExplainKind::Dot => graph::render(plan, GraphFmt::Dot),
        other => {
            let mut out = String::new();
            write_node(&mut out, &plan.root, "", true, true, other, ctx);
            out
        }
    }
}

fn write_node(
    out: &mut String,
    node: &Node,
    prefix: &str,
    is_last: bool,
    is_root: bool,
    kind: ExplainKind,
    ctx: &ExplainCtx,
) {
    if !is_root {
        out.push_str(prefix);
        out.push_str(if is_last { "└─ " } else { "├─ " });
    }
    out.push_str(&head(node, kind, ctx));
    out.push('\n');

    let child_prefix = if is_root {
        String::new()
    } else {
        format!("{prefix}{}", if is_last { "   " } else { "│  " })
    };
    let n = node.children.len();
    for (i, child) in node.children.iter().enumerate() {
        write_node(out, child, &child_prefix, i + 1 == n, false, kind, ctx);
    }
}

fn head(node: &Node, kind: ExplainKind, ctx: &ExplainCtx) -> String {
    if kind == ExplainKind::Backend && !matches!(node.kind, NodeKind::Reduce { .. }) {
        return format!(
            "{} backend={}",
            short_name(&node.kind),
            node.backend.as_str()
        );
    }
    let mut s = match &node.kind {
        NodeKind::Reduce {
            effects,
            snapshot,
            embed,
        } => {
            let eff = effects
                .iter()
                .map(|e| e.as_str())
                .collect::<Vec<_>>()
                .join("+");
            let mut line = format!("Reduce effect={eff} snapshot={snapshot} embed={embed}");
            if let Some(g) = ctx.r#gen {
                line.push_str(&format!(" gen={g}"));
            }
            line
        }
        NodeKind::Scan { collection, cols } => {
            format!(
                "Scan {collection} cols=[{}] backend={}",
                cols.join(", "),
                node.backend.as_str()
            )
        }
        NodeKind::Filter { pred } => format!("Filter {pred}"),
        NodeKind::Project { fields } => format!("Project [{}]", fields.join(", ")),
        NodeKind::Join { left, fk } => {
            let how = if *left { "left" } else { "inner" };
            format!("Join {how} fk={fk} backend={}", node.backend.as_str())
        }
        NodeKind::Hop { rel, depth, cap } => {
            format!(
                "Hop {rel} depth={depth} cap={cap} backend={}",
                node.backend.as_str()
            )
        }
        NodeKind::Graph { rel, depth, cap } => {
            format!(
                "Graph {rel} depth={depth} cap={cap} backend={}",
                node.backend.as_str()
            )
        }
        NodeKind::Match { start, hops, cap } => {
            let mut s = String::from("Match ");
            if let Some(a) = start {
                s.push_str(a);
                s.push(' ');
            }
            s.push_str(&hops.join(" "));
            s.push_str(&format!(" cap={cap} backend={}", node.backend.as_str()));
            s
        }
        NodeKind::Search { mode, query, k } => {
            let m = match mode {
                SearchMode::Hybrid => "hybrid",
                SearchMode::Lex => "lex",
                SearchMode::Vec => "vec",
            };
            format!(
                "Search {m} {query:?} k={k} backend={}",
                node.backend.as_str()
            )
        }
        NodeKind::Rrf { k } => format!("RRF k={k}"),
        NodeKind::Agg { op, by } => format!("Agg {op} by {by}"),
        NodeKind::Sort { field, desc } => {
            format!("Sort {field} {}", if *desc { "desc" } else { "asc" })
        }
        NodeKind::Skip { n } => format!("Skip {n}"),
        NodeKind::Take { n, implicit } => match n {
            Some(n) if *implicit => format!("Take {n} implicit"),
            Some(n) => format!("Take {n}"),
            None => "Take all".into(),
        },
        NodeKind::Get { collection, key } => {
            format!("Get {collection} {key} backend={}", node.backend.as_str())
        }
        NodeKind::Append { kind, detail, n } => {
            if *n > 1 {
                format!("Append {kind} n={n}")
            } else {
                format!("Append {kind} {detail}")
            }
        }
        NodeKind::InsertPack {
            collection,
            edges,
            n,
        } => {
            let mut s = format!("InsertPack {collection}");
            if *n > 1 {
                s.push_str(&format!(" n={n}"));
            }
            if !edges.is_empty() {
                s.push_str(&format!(" edges=[{}]", edges.join(", ")));
            }
            s
        }
        NodeKind::Cas { hash } => format!("CAS hash={hash:?}"),
        NodeKind::BatchCAS { n } => match n {
            Some(n) => format!("BatchCAS n={n}"),
            None => "BatchCAS n=?".into(),
        },
        NodeKind::IndexSeek { collection, fields } => {
            format!("IndexSeek index={collection}[{}]", fields.join(","))
        }
        NodeKind::Schema { detail } => format!("Schema {detail}"),
        NodeKind::Reembed { collection, to } => match to {
            Some(t) => format!("Reembed {collection} to {t}"),
            None => format!("Reembed {collection}"),
        },
        NodeKind::Delete { collection } => format!("Delete {collection}"),
        NodeKind::DeleteEdge { rel, from, to } => {
            format!("DeleteEdge {rel} {from} -> {to}")
        }
        NodeKind::Union => "Union".into(),
        NodeKind::Let { name } => format!("Let {name}"),
        NodeKind::Seq => "Seq".into(),
        NodeKind::Split => "Split".into(),
        NodeKind::Exchange => "Exchange".into(),
    };

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
        )
        && matches!(kind, ExplainKind::Cost | ExplainKind::Tree)
        && !s.contains("backend=")
    {
        s.push_str(&format!(" backend={}", node.backend.as_str()));
    }

    if node.effect != Effect::Read && !matches!(node.kind, NodeKind::Reduce { .. }) {
        s.push_str(&format!(" effect={}", node.effect.as_str()));
    }

    match &node.kind {
        NodeKind::Filter { .. } => s.push_str(" zone skip"),
        NodeKind::Hop { .. } => s.push_str(" serial"),
        NodeKind::Graph { .. } => s.push_str(" serial"),
        NodeKind::Match { .. } => s.push_str(" serial"),
        NodeKind::Rrf { .. } => s.push_str(" parallel"),
        _ => {}
    }

    for note in &node.notes {
        s.push(' ');
        s.push_str(note);
    }

    if kind == ExplainKind::Cost {
        let est = est_rows(node, &ctx.sizes);
        let bytes = (est * 24.0 * cols_hint(node) as f64).round() as i64;
        s.push_str(&format!(" est.rows={est:.0} est.bytes={bytes}"));
        match &node.kind {
            NodeKind::Take { n: Some(n), .. } => {
                s.push_str(&format!(" budget.take={n}"));
            }
            NodeKind::Scan { cols, .. } => {
                if !cols.iter().any(|c| c == "body") {
                    s.push_str(" -- no body");
                }
            }
            NodeKind::Hop { .. } => s.push_str(" after search"),
            NodeKind::Graph { .. } => s.push_str(" after search"),
            NodeKind::Match { .. } => s.push_str(" after search"),
            NodeKind::Get { .. } => s.push_str(" point"),
            _ => {}
        }
    }

    if kind == ExplainKind::Run {
        if let Some(st) = &ctx.stats {
            if matches!(node.kind, NodeKind::Reduce { .. } | NodeKind::Take { .. }) {
                s.push_str(&format!(" run=ok rows={} ms={:.2}", st.rows, st.ms));
            } else {
                s.push_str(" run=ok");
            }
        } else {
            s.push_str(" run=not-executed");
        }
    }

    s
}

fn cols_hint(node: &Node) -> usize {
    match &node.kind {
        NodeKind::Scan { cols, .. } => cols.len().max(1),
        NodeKind::Project { fields } => fields.len().max(1),
        _ => node.children.first().map(cols_hint).unwrap_or(3),
    }
}

fn est_rows(node: &Node, sizes: &CollectionSizes) -> f64 {
    let child = |i: usize| {
        node.children
            .get(i)
            .map(|c| est_rows(c, sizes))
            .unwrap_or(0.0)
    };
    match &node.kind {
        NodeKind::Reduce { .. } => child(0),
        NodeKind::Scan { collection, .. } => sizes.get(collection),
        NodeKind::Get { .. } => 1.0,
        NodeKind::IndexSeek { collection, .. } => (sizes.get(collection) * 0.25).max(1.0),
        NodeKind::Filter { .. } => (child(0) * 0.35).max(0.0),
        NodeKind::Project { .. }
        | NodeKind::Sort { .. }
        | NodeKind::Skip { .. }
        | NodeKind::Cas { .. }
        | NodeKind::BatchCAS { .. } => child(0),
        NodeKind::Take { n, .. } => match n {
            Some(n) => child(0).min(*n as f64),
            None => child(0),
        },
        NodeKind::Join { left, .. } => {
            let l = child(0);
            let r = child(1).max(1.0);
            if *left {
                l
            } else {
                (l * 0.8).min(l * r * 0.15).max(0.0)
            }
        }
        NodeKind::Union | NodeKind::Seq => child(0) + child(1),
        NodeKind::Let { .. } | NodeKind::Delete { .. } | NodeKind::DeleteEdge { .. } => {
            child(0).max(1.0)
        }
        NodeKind::Hop { cap, .. } => child(0).min(*cap as f64).max(0.0) * 1.4,
        NodeKind::Graph { cap, .. } => child(0).min(*cap as f64).max(0.0) * 1.4,
        NodeKind::Match { cap, .. } => child(0).min(*cap as f64).max(0.0) * 1.6,
        NodeKind::Search { k, .. } | NodeKind::Rrf { k } => {
            let inn = if node.children.is_empty() {
                sizes.docs
            } else {
                child(0)
            };
            inn.min(*k as f64)
        }
        NodeKind::Agg { .. } => (child(0) / 3.0).max(1.0),
        NodeKind::Append { .. }
        | NodeKind::InsertPack { .. }
        | NodeKind::Reembed { .. }
        | NodeKind::Schema { .. } => 1.0,
        NodeKind::Split | NodeKind::Exchange => child(0),
    }
}

fn short_name(kind: &NodeKind) -> &'static str {
    match kind {
        NodeKind::Reduce { .. } => "Reduce",
        NodeKind::Scan { .. } => "Scan",
        NodeKind::Filter { .. } => "Filter",
        NodeKind::Project { .. } => "Project",
        NodeKind::Join { .. } => "Join",
        NodeKind::Hop { .. } => "Hop",
        NodeKind::Graph { .. } => "Graph",
        NodeKind::Match { .. } => "Match",
        NodeKind::Search { .. } => "Search",
        NodeKind::Rrf { .. } => "RRF",
        NodeKind::Agg { .. } => "Agg",
        NodeKind::Sort { .. } => "Sort",
        NodeKind::Skip { .. } => "Skip",
        NodeKind::Take { .. } => "Take",
        NodeKind::Get { .. } => "Get",
        NodeKind::Append { .. } => "Append",
        NodeKind::InsertPack { .. } => "InsertPack",
        NodeKind::Cas { .. } => "CAS",
        NodeKind::BatchCAS { .. } => "BatchCAS",
        NodeKind::IndexSeek { .. } => "IndexSeek",
        NodeKind::Schema { .. } => "Schema",
        NodeKind::Reembed { .. } => "Reembed",
        NodeKind::Delete { .. } => "Delete",
        NodeKind::DeleteEdge { .. } => "DeleteEdge",
        NodeKind::Union => "Union",
        NodeKind::Let { .. } => "Let",
        NodeKind::Seq => "Seq",
        NodeKind::Split => "Split",
        NodeKind::Exchange => "Exchange",
    }
}
