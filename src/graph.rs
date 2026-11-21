use std::fmt::Write as _;

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
    let mut kids = Vec::with_capacity(node.children.len());
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

fn push_joined<S: AsRef<str>>(out: &mut String, items: &[S], sep: char) {
    for (i, item) in items.iter().enumerate() {
        if i > 0 {
            out.push(sep);
        }
        out.push_str(item.as_ref());
    }
}

fn short_label(node: &Node) -> String {
    match &node.kind {
        NodeKind::Reduce { effects, .. } => {
            let extra: usize = effects.iter().map(|e| e.as_str().len() + 1).sum();
            let mut s = String::with_capacity(7 + extra);
            s.push_str("Reduce ");
            for (i, e) in effects.iter().enumerate() {
                if i > 0 {
                    s.push('+');
                }
                s.push_str(e.as_str());
            }
            s
        }
        NodeKind::Scan { collection, .. } => {
            let mut s = String::with_capacity(5 + collection.len());
            s.push_str("Scan ");
            s.push_str(collection);
            s
        }
        NodeKind::Filter { pred } => {
            let mut s = String::with_capacity(7 + 32);
            s.push_str("Filter ");
            clip_into(&mut s, pred, 28);
            s
        }
        NodeKind::Join { left, .. } => {
            if *left {
                String::from("Join left")
            } else {
                String::from("Join inner")
            }
        }
        NodeKind::Hop { rel, .. } => {
            let mut s = String::with_capacity(4 + rel.len());
            s.push_str("Hop ");
            s.push_str(rel);
            s
        }
        NodeKind::Graph { rel, .. } => {
            let mut s = String::with_capacity(6 + rel.len());
            s.push_str("Graph ");
            s.push_str(rel);
            s
        }
        NodeKind::Match { start, hops, .. } => {
            let hops_len: usize = hops.iter().map(|h| h.len() + 1).sum();
            let mut s =
                String::with_capacity(6 + start.as_ref().map_or(0, |a| a.len() + 1) + hops_len);
            s.push_str("Match ");
            if let Some(a) = start {
                s.push_str(a);
                s.push(' ');
            }
            push_joined(&mut s, hops, ' ');
            s
        }
        NodeKind::Search { mode, query, .. } => {
            let m = match mode {
                SearchMode::Hybrid => "hybrid",
                SearchMode::Lex => "lex",
                SearchMode::Vec => "vec",
            };
            let mut s = String::with_capacity(8 + m.len() + 20);
            s.push_str("Search ");
            s.push_str(m);
            s.push(' ');
            clip_into(&mut s, query, 16);
            s
        }
        NodeKind::Agg { op, .. } => {
            let mut s = String::with_capacity(4 + op.len());
            s.push_str("Agg ");
            s.push_str(op);
            s
        }
        NodeKind::Sort { field, desc } => {
            let dir = if *desc { " desc" } else { " asc" };
            let mut s = String::with_capacity(5 + field.len() + dir.len());
            s.push_str("Sort ");
            s.push_str(field);
            s.push_str(dir);
            s
        }
        NodeKind::Window { func, as_field, .. } => {
            let mut s = String::with_capacity(11 + func.len() + as_field.len());
            s.push_str("Window ");
            s.push_str(func);
            s.push_str(" as ");
            s.push_str(as_field);
            s
        }
        NodeKind::Skip { n } => {
            let mut s = String::with_capacity(16);
            let _ = write!(s, "Skip {n}");
            s
        }
        NodeKind::Take { n, implicit } => match n {
            Some(n) if *implicit => {
                let mut s = String::with_capacity(24);
                let _ = write!(s, "Take {n} implicit");
                s
            }
            Some(n) => {
                let mut s = String::with_capacity(16);
                let _ = write!(s, "Take {n}");
                s
            }
            None => String::from("Take all"),
        },
        NodeKind::Get { collection, .. } => {
            let mut s = String::with_capacity(4 + collection.len());
            s.push_str("Get ");
            s.push_str(collection);
            s
        }
        NodeKind::Append { kind, n, .. } => {
            if *n > 1 {
                let mut s = String::with_capacity(16 + kind.len());
                let _ = write!(s, "Append {kind} n={n}");
                s
            } else {
                let mut s = String::with_capacity(7 + kind.len());
                s.push_str("Append ");
                s.push_str(kind);
                s
            }
        }
        NodeKind::InsertPack { collection, n, .. } => {
            if *n > 1 {
                let mut s = String::with_capacity(20 + collection.len());
                let _ = write!(s, "InsertPack {collection} n={n}");
                s
            } else {
                let mut s = String::with_capacity(11 + collection.len());
                s.push_str("InsertPack ");
                s.push_str(collection);
                s
            }
        }
        NodeKind::Cas { .. }
        | NodeKind::BatchCAS { .. }
        | NodeKind::Schema { .. }
        | NodeKind::Project { .. }
        | NodeKind::Without { .. }
        | NodeKind::Rrf { .. }
        | NodeKind::Union
        | NodeKind::Seq
        | NodeKind::Split
        | NodeKind::Exchange => node.kind.short_name().into(),
        NodeKind::IndexSeek { collection, fields } => {
            let flen: usize = fields.iter().map(|f| f.len() + 1).sum();
            let mut s = String::with_capacity(8 + collection.len() + flen);
            s.push_str("index=");
            s.push_str(collection);
            s.push('[');
            push_joined(&mut s, fields, ',');
            s.push(']');
            s
        }
        NodeKind::FtsSeek {
            collection,
            fields,
            query,
        } => {
            let flen: usize = fields.iter().map(|f| f.len() + 1).sum();
            let mut s = String::with_capacity(12 + collection.len() + flen + 20);
            s.push_str("FtsSeek ");
            s.push_str(collection);
            s.push('[');
            push_joined(&mut s, fields, ',');
            s.push_str("] ");
            clip_into(&mut s, query, 16);
            s
        }
        NodeKind::Reembed { collection, .. } => {
            let mut s = String::with_capacity(8 + collection.len());
            s.push_str("Reembed ");
            s.push_str(collection);
            s
        }
        NodeKind::Delete { collection } => {
            let mut s = String::with_capacity(7 + collection.len());
            s.push_str("Delete ");
            s.push_str(collection);
            s
        }
        NodeKind::DeleteEdge { rel, .. } => {
            let mut s = String::with_capacity(11 + rel.len());
            s.push_str("DeleteEdge ");
            s.push_str(rel);
            s
        }
        NodeKind::Let { name } => {
            let mut s = String::with_capacity(4 + name.len());
            s.push_str("Let ");
            s.push_str(name);
            s
        }
    }
}

/// Single-pass `O(max)` clipper that maps `"` to `'` and appends `…` if `s` exceeds `max` chars.
fn clip_into(out: &mut String, s: &str, max: usize) {
    if s.is_empty() {
        return;
    }
    if max == 0 {
        out.push('…');
        return;
    }
    let mut chars = s.chars();
    for _ in 0..(max - 1) {
        let Some(c) = chars.next() else {
            return;
        };
        out.push(if c == '"' { '\'' } else { c });
    }
    let Some(last) = chars.next() else {
        return;
    };
    if chars.next().is_none() {
        out.push(if last == '"' { '\'' } else { last });
    } else {
        out.push('…');
    }
}

fn mermaid(nodes: &[GNode]) -> String {
    let edge_count: usize = nodes.iter().map(|n| n.children.len()).sum();
    let mut out = String::with_capacity(224 + nodes.len() * 40 + edge_count * 16);
    out.push_str("flowchart TD\n");
    for n in nodes {
        let _ = write!(out, "  n{}[\"", n.id);
        for ch in n.label.chars() {
            out.push(if ch == '"' { '\'' } else { ch });
        }
        let _ = writeln!(out, "\"]:::{}", n.class);
    }
    for n in nodes {
        for c in &n.children {
            let _ = writeln!(out, "  n{} --> n{}", n.id, c);
        }
    }
    out.push_str("  classDef read fill:#dbeafe,stroke:#2563eb,color:#1e3a8a\n");
    out.push_str("  classDef write fill:#ffedd5,stroke:#c2410c,color:#7c2d12\n");
    out.push_str("  classDef reduce fill:#ede9fe,stroke:#7c3aed,color:#4c1d95\n");
    out
}

fn dot(nodes: &[GNode]) -> String {
    let edge_count: usize = nodes.iter().map(|n| n.children.len()).sum();
    let mut out = String::with_capacity(64 + nodes.len() * 68 + edge_count * 16);
    out.push_str("digraph lin {\n  rankdir=TB;\n");
    for n in nodes {
        let (shape, fill) = match n.class {
            "reduce" => ("hexagon", "#ede9fe"),
            "write" => ("box", "#ffedd5"),
            _ => ("box", "#dbeafe"),
        };
        let _ = write!(out, "  n{} [label=\"", n.id);
        for ch in n.label.chars() {
            match ch {
                '\\' => out.push_str("\\\\"),
                '"' => out.push_str("\\\""),
                other => out.push(other),
            }
        }
        let _ = writeln!(
            out,
            "\", shape={}, style=filled, fillcolor=\"{}\"];",
            shape, fill
        );
    }
    for n in nodes {
        for c in &n.children {
            let _ = writeln!(out, "  n{} -> n{};", n.id, c);
        }
    }
    out.push_str("}\n");
    out
}
