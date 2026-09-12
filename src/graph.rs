use crate::ast::SearchMode;
use crate::plan::{Effect, Node, NodeKind, Plan};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphFmt {
    Mermaid,
    Dot,
}

pub fn render(plan: &Plan, fmt: GraphFmt) -> String {
    let mut nodes = Vec::new();
    collect(&plan.root, &mut nodes);
    match fmt {
        GraphFmt::Mermaid => mermaid(&nodes),
        GraphFmt::Dot => dot(&nodes),
    }
}

struct GNode {
    id: usize,
    label: String,
    class: &'static str,
    children: Vec<usize>,
}

fn collect(node: &Node, out: &mut Vec<GNode>) -> usize {
    let id = out.len();
    out.push(GNode {
        id,
        label: short_label(node),
        class: class_of(node),
        children: Vec::new(),
    });
    let mut kids = Vec::new();
    for child in &node.children {
        kids.push(collect(child, out));
    }
    out[id].children = kids;
    id
}

fn class_of(node: &Node) -> &'static str {
    if matches!(node.kind, NodeKind::Reduce { .. }) {
        "reduce"
    } else if node.effect == Effect::Read {
        "read"
    } else {
        "write"
    }
}

fn short_label(node: &Node) -> String {
    match &node.kind {
        NodeKind::Reduce { effects, .. } => {
            let eff = effects
                .iter()
                .map(|e| e.as_str())
                .collect::<Vec<_>>()
                .join("+");
            format!("Reduce {eff}")
        }
        NodeKind::Scan { collection, .. } => format!("Scan {collection}"),
        NodeKind::Filter { pred } => format!("Filter {}", clip(pred, 28)),
        NodeKind::Project { .. } => "Project".into(),
        NodeKind::Join { left, .. } => {
            format!("Join {}", if *left { "left" } else { "inner" })
        }
        NodeKind::Hop { rel, .. } => format!("Hop {rel}"),
        NodeKind::Graph { rel, .. } => format!("Graph {rel}"),
        NodeKind::Match { start, hops, .. } => {
            let mut s = String::from("Match ");
            if let Some(a) = start {
                s.push_str(a);
                s.push(' ');
            }
            s.push_str(&hops.join(" "));
            s
        }
        NodeKind::Search { mode, query, .. } => {
            let m = match mode {
                SearchMode::Hybrid => "hybrid",
                SearchMode::Lex => "lex",
                SearchMode::Vec => "vec",
            };
            format!("Search {m} {}", clip(query, 16))
        }
        NodeKind::Rrf { .. } => "RRF".into(),
        NodeKind::Agg { op, .. } => format!("Agg {op}"),
        NodeKind::Sort { field, desc } => {
            format!("Sort {field} {}", if *desc { "desc" } else { "asc" })
        }
        NodeKind::Take { n, implicit } => match n {
            Some(n) if *implicit => format!("Take {n} implicit"),
            Some(n) => format!("Take {n}"),
            None => "Take all".into(),
        },
        NodeKind::Get { collection, .. } => format!("Get {collection}"),
        NodeKind::Append { kind, n, .. } => {
            if *n > 1 {
                format!("Append {kind} n={n}")
            } else {
                format!("Append {kind}")
            }
        }
        NodeKind::InsertPack { collection, n, .. } => {
            if *n > 1 {
                format!("InsertPack {collection} n={n}")
            } else {
                format!("InsertPack {collection}")
            }
        }
        NodeKind::Cas { .. } => "CAS".into(),
        NodeKind::BatchCAS { .. } => "BatchCAS".into(),
        NodeKind::IndexSeek { collection, fields } => {
            format!("index={collection}[{}]", fields.join(","))
        }
        NodeKind::Schema { .. } => "Schema".into(),
        NodeKind::Reembed { collection, .. } => format!("Reembed {collection}"),
        NodeKind::Delete { collection } => format!("Delete {collection}"),
        NodeKind::DeleteEdge { rel, .. } => format!("DeleteEdge {rel}"),
        NodeKind::Union => "Union".into(),
        NodeKind::Let { name } => format!("Let {name}"),
        NodeKind::Seq => "Seq".into(),
        NodeKind::Split => "Split".into(),
        NodeKind::Exchange => "Exchange".into(),
    }
}

fn clip(s: &str, max: usize) -> String {
    let t = s.replace('"', "'");
    if t.chars().count() <= max {
        t
    } else {
        let cut: String = t.chars().take(max.saturating_sub(1)).collect();
        format!("{cut}…")
    }
}

fn mermaid(nodes: &[GNode]) -> String {
    let mut out = String::from("flowchart TD\n");
    for n in nodes {
        let label = n.label.replace('"', "'");
        out.push_str(&format!("  n{}[\"{}\"]:::{}\n", n.id, label, n.class));
    }
    for n in nodes {
        for c in &n.children {
            out.push_str(&format!("  n{} --> n{}\n", n.id, c));
        }
    }
    out.push_str("  classDef read fill:#dbeafe,stroke:#2563eb,color:#1e3a8a\n");
    out.push_str("  classDef write fill:#ffedd5,stroke:#c2410c,color:#7c2d12\n");
    out.push_str("  classDef reduce fill:#ede9fe,stroke:#7c3aed,color:#4c1d95\n");
    out
}

fn dot(nodes: &[GNode]) -> String {
    let mut out = String::from("digraph lin {\n  rankdir=TB;\n");
    for n in nodes {
        let (shape, fill) = match n.class {
            "reduce" => ("hexagon", "#ede9fe"),
            "write" => ("box", "#ffedd5"),
            _ => ("box", "#dbeafe"),
        };
        let label = n.label.replace('\\', "\\\\").replace('"', "\\\"");
        out.push_str(&format!(
            "  n{} [label=\"{}\", shape={}, style=filled, fillcolor=\"{}\"];\n",
            n.id, label, shape, fill
        ));
    }
    for n in nodes {
        for c in &n.children {
            out.push_str(&format!("  n{} -> n{};\n", n.id, c));
        }
    }
    out.push_str("}\n");
    out
}
