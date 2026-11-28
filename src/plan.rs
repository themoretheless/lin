use crate::ast::*;
use crate::catalog::Catalog;
use crate::check;
use crate::error::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    Idb,
    Native,
    Wasm,
}

impl Backend {
    pub fn as_str(self) -> &'static str {
        match self {
            Backend::Idb => "idb",
            Backend::Native => "native",
            Backend::Wasm => "wasm",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    Read,
    Append,
    Create,
    Meta,
    Body,
    Embed,
    Schema,
    HopBuild,
}

impl Effect {
    pub fn as_str(self) -> &'static str {
        match self {
            Effect::Read => "Read",
            Effect::Append => "Append",
            Effect::Create => "Create",
            Effect::Meta => "Meta",
            Effect::Body => "Body",
            Effect::Embed => "Embed",
            Effect::Schema => "Schema",
            Effect::HopBuild => "HopBuild",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    pub root: Node,
    pub explain: ExplainKind,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub kind: NodeKind,
    pub backend: Backend,
    pub effect: Effect,
    pub children: Vec<Node>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum NodeKind {
    Reduce {
        effects: Vec<Effect>,
        snapshot: String,
        embed: String,
    },
    Scan {
        collection: String,
        cols: Vec<String>,
    },
    Filter {
        pred: String,
    },
    Project {
        fields: Vec<String>,
    },
    Without {
        fields: Vec<String>,
    },
    Join {
        left: bool,
        fk: String,
    },
    Hop {
        rel: String,
        depth: i64,
        cap: i64,
    },
    Graph {
        rel: String,
        depth: i64,
        cap: i64,
    },
    Match {
        start: Option<String>,
        hops: Vec<String>,
        cap: i64,
    },
    Search {
        mode: SearchMode,
        query: String,
        k: i64,
    },
    Rrf {
        k: i64,
    },
    Agg {
        op: String,
        by: String,
    },
    Sort {
        field: String,
        desc: bool,
    },
    Window {
        func: String,
        by: Vec<String>,
        order: Vec<(String, bool)>,
        frame: Option<String>,
        as_field: String,
    },
    Skip {
        n: i64,
    },
    Take {
        n: Option<i64>,
        implicit: bool,
    },
    Get {
        collection: String,
        key: String,
    },
    Append {
        kind: String,
        detail: String,
        n: usize,
    },
    InsertPack {
        collection: String,
        edges: Vec<String>,
        n: usize,
    },
    Cas {
        hash: String,
    },
    BatchCAS {
        n: Option<usize>,
    },
    IndexSeek {
        collection: String,
        fields: Vec<String>,
    },
    /// FTS posting seek (lex / hybrid lex-half). Not an overload of IndexSeek.
    FtsSeek {
        collection: String,
        fields: Vec<String>,
        query: String,
    },
    Schema {
        detail: String,
    },
    Reembed {
        collection: String,
        to: Option<String>,
    },
    Delete {
        collection: String,
    },
    DeleteEdge {
        rel: String,
        from: String,
        to: String,
    },
    Union,
    Let {
        name: String,
    },
    Seq,
    Split,
    Exchange,
}

impl NodeKind {
    pub const fn short_name(&self) -> &'static str {
        match self {
            NodeKind::Reduce { .. } => "Reduce",
            NodeKind::Scan { .. } => "Scan",
            NodeKind::Filter { .. } => "Filter",
            NodeKind::Project { .. } => "Project",
            NodeKind::Without { .. } => "Without",
            NodeKind::Join { .. } => "Join",
            NodeKind::Hop { .. } => "Hop",
            NodeKind::Graph { .. } => "Graph",
            NodeKind::Match { .. } => "Match",
            NodeKind::Search { .. } => "Search",
            NodeKind::Rrf { .. } => "RRF",
            NodeKind::Agg { .. } => "Agg",
            NodeKind::Sort { .. } => "Sort",
            NodeKind::Window { .. } => "Window",
            NodeKind::Skip { .. } => "Skip",
            NodeKind::Take { .. } => "Take",
            NodeKind::Get { .. } => "Get",
            NodeKind::Append { .. } => "Append",
            NodeKind::InsertPack { .. } => "InsertPack",
            NodeKind::Cas { .. } => "CAS",
            NodeKind::BatchCAS { .. } => "BatchCAS",
            NodeKind::IndexSeek { .. } => "IndexSeek",
            NodeKind::FtsSeek { .. } => "FtsSeek",
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
}

pub fn plan_program(stmts: &[Stmt], cat: &Catalog) -> Result<Plan, Error> {
    if stmts.len() == 1 {
        return plan(&stmts[0], cat);
    }
    let mut parts = Vec::new();
    let mut effects = Vec::new();
    let mut explain = ExplainKind::Tree;
    for stmt in stmts {
        if let Stmt::Query(q) = stmt
            && let Some(k) = q.explain
        {
            explain = k;
        }
        let mut p = plan(stmt, cat)?;
        if let NodeKind::Reduce { effects: effs, .. } = &p.root.kind {
            for e in effs {
                if !effects.contains(e) {
                    effects.push(*e);
                }
            }
        }
        if p.root.children.len() == 1 {
            parts.push(p.root.children.remove(0));
        } else {
            parts.push(p.root);
        }
    }
    if effects.is_empty() {
        effects.push(Effect::Read);
    }
    let inner = if parts.len() == 1 {
        parts.remove(0)
    } else {
        node(NodeKind::Seq, Backend::Native, Effect::Read, parts)
    };
    Ok(write_plan(cat, effects, inner, explain))
}

pub fn plan(stmt: &Stmt, cat: &Catalog) -> Result<Plan, Error> {
    match stmt {
        Stmt::Query(q) => plan_query(q, cat, q.explain.unwrap_or(ExplainKind::Tree)),
        Stmt::Let { name, query } => {
            let mut p = plan_query(query, cat, ExplainKind::Tree)?;
            let inner = p.root.children.pop().unwrap_or(p.root);
            Ok(write_plan(
                cat,
                vec![Effect::Read],
                node(
                    NodeKind::Let { name: name.clone() },
                    Backend::Native,
                    Effect::Read,
                    vec![inner],
                ),
                ExplainKind::Tree,
            ))
        }
        Stmt::AppendFacts { records } => Ok(write_plan(
            cat,
            vec![Effect::Append],
            node(
                NodeKind::Append {
                    kind: "facts".into(),
                    detail: records
                        .first()
                        .map(record_preview)
                        .unwrap_or_else(|| "{}".into()),
                    n: records.len(),
                },
                Backend::Native,
                Effect::Append,
                vec![],
            ),
            ExplainKind::Tree,
        )),
        Stmt::AppendEdges { edges } => {
            let detail = edges
                .first()
                .map(|e| format!("{} {} -> {}", e.rel, fmt_value(&e.from), fmt_value(&e.to)))
                .unwrap_or_default();
            Ok(write_plan(
                cat,
                vec![Effect::Append],
                node(
                    NodeKind::Append {
                        kind: if edges.len() == 1 {
                            "edge".into()
                        } else {
                            "edges".into()
                        },
                        detail,
                        n: edges.len(),
                    },
                    Backend::Native,
                    Effect::Append,
                    vec![],
                ),
                ExplainKind::Tree,
            ))
        }
        Stmt::Insert {
            collection,
            records,
            edges,
        } => {
            let mut effects = vec![Effect::Create];
            let has_body_col = cat
                .collection(collection)
                .is_none_or(|c| c.field("body").is_some());
            if has_body_col
                && records
                    .iter()
                    .any(|r| r.fields.iter().any(|(n, _)| n == "body"))
            {
                effects.push(Effect::Body);
            }
            if !edges.is_empty() {
                effects.push(Effect::HopBuild);
            }
            let edge_s: Vec<String> = edges
                .iter()
                .map(|e| match &e.target {
                    EdgeTarget::Page(p) => format!("{} -> page {p:?}", e.rel),
                    EdgeTarget::Value(v) => format!("{} -> {}", e.rel, fmt_value(v)),
                })
                .collect();
            Ok(write_plan(
                cat,
                effects,
                node(
                    NodeKind::InsertPack {
                        collection: collection.clone(),
                        edges: edge_s,
                        n: records.len(),
                    },
                    Backend::Native,
                    Effect::Create,
                    vec![],
                ),
                ExplainKind::Tree,
            ))
        }
        Stmt::InsertRows { collection, rows } => Ok(write_plan(
            cat,
            vec![Effect::Create],
            node(
                NodeKind::InsertPack {
                    collection: collection.clone(),
                    edges: Vec::new(),
                    n: *rows,
                },
                Backend::Native,
                Effect::Create,
                vec![],
            ),
            ExplainKind::Tree,
        )),
        Stmt::Update {
            collection,
            pred,
            cas,
            cas_each,
            record,
        } => {
            let effect = if record.fields.iter().any(|(n, _)| n == "body") {
                Effect::Body
            } else {
                Effect::Meta
            };
            let mut inner = point_or_scan(collection, pred.as_ref(), cat);
            if *cas_each {
                inner = node(
                    NodeKind::BatchCAS { n: None },
                    Backend::Native,
                    effect,
                    vec![inner],
                );
            } else {
                let hash = cas.clone().expect("check requires cas before plan");
                inner = node(NodeKind::Cas { hash }, Backend::Native, effect, vec![inner]);
            }
            Ok(write_plan(cat, vec![effect], inner, ExplainKind::Tree))
        }
        Stmt::Delete {
            collection,
            pred,
            cas,
            cas_each,
        } => {
            let mut inner = point_or_scan(collection, Some(pred), cat);
            if *cas_each {
                inner = node(
                    NodeKind::BatchCAS { n: None },
                    Backend::Native,
                    Effect::Meta,
                    vec![inner],
                );
            } else if let Some(hash) = cas {
                inner = node(
                    NodeKind::Cas { hash: hash.clone() },
                    Backend::Native,
                    Effect::Meta,
                    vec![inner],
                );
            }
            inner = node(
                NodeKind::Delete {
                    collection: collection.clone(),
                },
                Backend::Native,
                Effect::Meta,
                vec![inner],
            );
            Ok(write_plan(
                cat,
                vec![Effect::Meta],
                inner,
                ExplainKind::Tree,
            ))
        }
        Stmt::DeleteEdge { rel, from, to } => Ok(write_plan(
            cat,
            vec![Effect::Meta],
            node(
                NodeKind::DeleteEdge {
                    rel: rel.clone(),
                    from: fmt_value(from),
                    to: fmt_value(to),
                },
                Backend::Native,
                Effect::Meta,
                vec![],
            ),
            ExplainKind::Tree,
        )),
        Stmt::Reembed { collection, to } => Ok(write_plan(
            cat,
            vec![Effect::Embed],
            node(
                NodeKind::Reembed {
                    collection: collection.clone(),
                    to: to.as_ref().map(|t| t.display()),
                },
                Backend::Native,
                Effect::Embed,
                vec![],
            ),
            ExplainKind::Tree,
        )),
        Stmt::Decl(d) => Ok(write_plan(
            cat,
            vec![Effect::Schema],
            node(
                NodeKind::Schema {
                    detail: decl_detail(d),
                },
                Backend::Native,
                Effect::Schema,
                vec![],
            ),
            ExplainKind::Tree,
        )),
        Stmt::IdbSlice { query } => {
            let mut p = plan_query(query, cat, ExplainKind::Tree)?;
            stamp_backend(&mut p.root, Backend::Idb);
            Ok(p)
        }
        Stmt::IdbPull { since, take } => Ok(write_plan(
            cat,
            vec![Effect::Read],
            node(
                NodeKind::Schema {
                    detail: format!(
                        "pull idb since {since}{}",
                        take.map(|n| format!(" take {n}")).unwrap_or_default()
                    ),
                },
                Backend::Idb,
                Effect::Read,
                vec![],
            ),
            ExplainKind::Tree,
        )),
        Stmt::IdbPush => Ok(write_plan(
            cat,
            vec![Effect::Meta],
            node(
                NodeKind::Schema {
                    detail: "push idb".into(),
                },
                Backend::Idb,
                Effect::Meta,
                vec![],
            ),
            ExplainKind::Tree,
        )),
        Stmt::Snapshot { name }
        | Stmt::Restore { name }
        | Stmt::Pin { name }
        | Stmt::Unpin { name } => Ok(write_plan(
            cat,
            vec![Effect::Meta],
            node(
                NodeKind::Schema {
                    detail: format!("memory pin {name:?}"),
                },
                Backend::Native,
                Effect::Meta,
                vec![],
            ),
            ExplainKind::Tree,
        )),
    }
}

fn plan_pipeline(q: &Query, cat: &Catalog, implicit_take: bool) -> Result<Node, Error> {
    if q.needs_simplify() {
        let mut q = q.clone();
        q.simplify_predicates();
        plan_query_inner(&q, cat, implicit_take)
    } else {
        plan_query_inner(q, cat, implicit_take)
    }
}

fn plan_query(q: &Query, cat: &Catalog, explain: ExplainKind) -> Result<Plan, Error> {
    let mut q = crate::catalog::with_catalog_filter(q, cat);
    if q.needs_simplify() {
        q.to_mut().simplify_predicates();
    }
    let cur = plan_query_inner(&q, cat, true)?;
    let root = reduce(cat, vec![Effect::Read], cur);
    Ok(Plan { root, explain })
}

fn plan_query_inner(q: &Query, cat: &Catalog, allow_implicit_take: bool) -> Result<Node, Error> {
    use std::borrow::Cow;
    let collection = check::collection_of(&q.source);
    let mut implicit_take = true;
    let mut take_n: Option<Option<i64>> = None;
    let mut search_k = 50i64;
    let mut saw_agg = false;
    let mut want_vec = false;
    let mut want_search = false;
    let mut picked: Vec<String> = Vec::new();
    let mut needed: Vec<String> = Vec::new();

    for step in &q.steps {
        match step {
            Step::Take { n } => {
                implicit_take = false;
                take_n = Some(*n);
                if !saw_agg && let Some(v) = n {
                    search_k = *v;
                }
            }
            Step::Skip { .. } => {
                // Does not cancel implicit take by itself; take/agg still apply.
            }
            Step::Count { by } => {
                implicit_take = false;
                saw_agg = true;
                if let Some(by) = by {
                    push_field(&mut needed, by.as_cow_str());
                }
            }
            Step::Sum { field, by }
            | Step::Min { field, by }
            | Step::Max { field, by }
            | Step::Avg { field, by } => {
                implicit_take = false;
                saw_agg = true;
                push_field(&mut needed, field.as_cow_str());
                if let Some(by) = by {
                    push_field(&mut needed, by.as_cow_str());
                }
            }
            Step::Summarize { specs, by } => {
                implicit_take = false;
                saw_agg = true;
                for gk in by {
                    push_field(&mut needed, gk.source_field().as_cow_str());
                }
                for spec in specs {
                    match &spec.func {
                        AggFunc::Count => {}
                        AggFunc::Sum(f)
                        | AggFunc::Avg(f)
                        | AggFunc::Min(f)
                        | AggFunc::Max(f)
                        | AggFunc::DCount(f)
                        | AggFunc::Percentile { field: f, .. } => {
                            push_field(&mut needed, f.as_cow_str());
                        }
                        AggFunc::ArgMax { order, value } | AggFunc::ArgMin { order, value } => {
                            push_field(&mut needed, order.as_cow_str());
                            push_field(&mut needed, value.as_cow_str());
                        }
                    }
                }
            }
            Step::ArgExtremum { order, by, .. } => {
                implicit_take = false;
                saw_agg = true;
                push_field(&mut needed, order.as_cow_str());
                for gk in by {
                    push_field(&mut needed, gk.source_field().as_cow_str());
                }
            }
            Step::Search { mode, .. } => {
                want_search = true;
                if matches!(mode, SearchMode::Vec | SearchMode::Hybrid) {
                    want_vec = true;
                }
            }
            Step::Project(fields) => {
                for f in fields {
                    let cow = f.as_cow_str();
                    push_field(&mut picked, Cow::Borrowed(cow.as_ref()));
                    push_field(&mut needed, cow);
                }
            }
            Step::Without(excluded) => {
                if let Some(col) = cat.collection(collection) {
                    for k in col.fields.keys() {
                        if !excluded.iter().any(|ex| ex.leaf() == Some(k.as_str())) {
                            push_field(&mut picked, Cow::Borrowed(k.as_str()));
                            push_field(&mut needed, Cow::Borrowed(k.as_str()));
                        }
                    }
                }
            }
            Step::Filter(pred) => collect_pred_fields(pred, &mut needed),
            Step::Join { on, .. } => push_field(&mut needed, Cow::Borrowed(on.as_str())),
            Step::Sort { field, .. } => push_field(&mut needed, field.as_cow_str()),
            Step::Window {
                field, by, order, ..
            } => {
                if let Some(f) = field {
                    push_field(&mut needed, f.as_cow_str());
                }
                for b in by {
                    push_field(&mut needed, b.as_cow_str());
                }
                for s in order {
                    push_field(&mut needed, s.field.as_cow_str());
                }
            }
            Step::Hop { .. } => {}
            Step::Graph { .. } => {}
            Step::Match { .. } => {}
            Step::Union(_) => {}
        }
    }

    let cols = scan_cols(cat, collection, &needed, &picked, want_search, want_vec);

    let mut cur = match &q.source {
        Source::Page(uri) => node(
            NodeKind::Get {
                collection: "docs".into(),
                key: format!("uri={uri:?}"),
            },
            Backend::Idb,
            Effect::Read,
            vec![],
        ),
        Source::Catalog => node(
            NodeKind::Scan {
                collection: "catalog".into(),
                cols,
            },
            Backend::Native,
            Effect::Read,
            vec![],
        ),
        Source::Collection(name) => node(
            NodeKind::Scan {
                collection: name.clone(),
                cols,
            },
            Backend::Native,
            Effect::Read,
            vec![],
        ),
    };

    for step in &q.steps {
        match step {
            Step::Filter(pred) => {
                cur = apply_filter(cur, pred, cat);
            }
            Step::Project(fields) => {
                cur = node(
                    NodeKind::Project {
                        fields: fields.iter().map(|f| f.as_str()).collect(),
                    },
                    Backend::Native,
                    Effect::Read,
                    vec![cur],
                );
            }
            Step::Without(fields) => {
                cur = node(
                    NodeKind::Without {
                        fields: fields.iter().map(|f| f.as_str()).collect(),
                    },
                    Backend::Native,
                    Effect::Read,
                    vec![cur],
                );
            }
            Step::Join {
                left,
                collection: right,
                on,
            } => {
                let fk = cat
                    .find_fk(collection, on, right)
                    .map(|f| {
                        format!(
                            "{}.{}→{}.{}",
                            f.from_col, f.from_field, f.to_col, f.to_field
                        )
                    })
                    .unwrap_or_else(|| format!("{on}→{right}"));
                let right_scan = node(
                    NodeKind::Scan {
                        collection: right.clone(),
                        cols: join_right_cols(cat, right),
                    },
                    Backend::Native,
                    Effect::Read,
                    vec![],
                );
                cur = node(
                    NodeKind::Join { left: *left, fk },
                    Backend::Native,
                    Effect::Read,
                    vec![cur, right_scan],
                );
            }
            Step::Hop { rel, depth } => {
                let depth = depth.unwrap_or(1);
                cur = node(
                    NodeKind::Hop {
                        rel: rel.clone(),
                        depth,
                        cap: 300,
                    },
                    Backend::Native,
                    Effect::Read,
                    vec![cur],
                );
            }
            Step::Graph { rel, depth } => {
                let depth = depth.unwrap_or(1);
                cur = node(
                    NodeKind::Graph {
                        rel: rel.clone(),
                        depth,
                        cap: 300,
                    },
                    Backend::Native,
                    Effect::Read,
                    vec![cur],
                );
            }
            Step::Match { start, hops } => {
                let hops: Vec<String> = hops.iter().map(|h| h.label()).collect();
                cur = node(
                    NodeKind::Match {
                        start: start.clone(),
                        hops,
                        cap: 300,
                    },
                    Backend::Native,
                    Effect::Read,
                    vec![cur],
                );
            }
            Step::Search { mode, query } => {
                cur = wrap_search(cur, *mode, query, search_k, cat);
            }
            Step::Count { by } => {
                cur = node(
                    NodeKind::Agg {
                        op: "count".into(),
                        by: by
                            .as_ref()
                            .map(|b| b.as_str())
                            .unwrap_or_else(|| "*".into()),
                    },
                    Backend::Native,
                    Effect::Read,
                    vec![cur],
                );
            }
            Step::Sum { field, by } => {
                cur = node(
                    NodeKind::Agg {
                        op: format!("sum({})", field.as_str()),
                        by: by
                            .as_ref()
                            .map(|b| b.as_str())
                            .unwrap_or_else(|| "*".into()),
                    },
                    Backend::Native,
                    Effect::Read,
                    vec![cur],
                );
            }
            Step::Min { field, by } => {
                cur = node(
                    NodeKind::Agg {
                        op: format!("min({})", field.as_str()),
                        by: by
                            .as_ref()
                            .map(|b| b.as_str())
                            .unwrap_or_else(|| "*".into()),
                    },
                    Backend::Native,
                    Effect::Read,
                    vec![cur],
                );
            }
            Step::Max { field, by } => {
                cur = node(
                    NodeKind::Agg {
                        op: format!("max({})", field.as_str()),
                        by: by
                            .as_ref()
                            .map(|b| b.as_str())
                            .unwrap_or_else(|| "*".into()),
                    },
                    Backend::Native,
                    Effect::Read,
                    vec![cur],
                );
            }
            Step::Avg { field, by } => {
                cur = node(
                    NodeKind::Agg {
                        op: format!("avg({})", field.as_str()),
                        by: by
                            .as_ref()
                            .map(|b| b.as_str())
                            .unwrap_or_else(|| "*".into()),
                    },
                    Backend::Native,
                    Effect::Read,
                    vec![cur],
                );
            }
            Step::Summarize { specs, by } => {
                cur = node(
                    NodeKind::Agg {
                        op: fmt_summarize_op(specs),
                        by: fmt_group_keys(by),
                    },
                    Backend::Native,
                    Effect::Read,
                    vec![cur],
                );
            }
            Step::ArgExtremum { is_max, order, by } => {
                let name = if *is_max { "arg_max" } else { "arg_min" };
                cur = node(
                    NodeKind::Agg {
                        op: format!("{name}({})", order.as_str()),
                        by: fmt_group_keys(by),
                    },
                    Backend::Native,
                    Effect::Read,
                    vec![cur],
                );
            }
            Step::Sort { field, desc } => {
                cur = node(
                    NodeKind::Sort {
                        field: field.as_str(),
                        desc: *desc,
                    },
                    Backend::Native,
                    Effect::Read,
                    vec![cur],
                );
            }
            Step::Window {
                func,
                field,
                by,
                order,
                frame,
                as_field,
            } => {
                let func_str = match field {
                    Some(f) => format!("{}({})", func, f.as_str()),
                    None => format!("{}", func),
                };
                cur = node(
                    NodeKind::Window {
                        func: func_str,
                        by: by.iter().map(Field::as_str).collect(),
                        order: order
                            .iter()
                            .map(|s| (s.field.as_str(), s.desc))
                            .collect(),
                        frame: frame.map(|fr| fr.to_string()),
                        as_field: as_field.clone(),
                    },
                    Backend::Native,
                    Effect::Read,
                    vec![cur],
                );
            }
            Step::Skip { n } => {
                cur = node(
                    NodeKind::Skip { n: *n },
                    Backend::Native,
                    Effect::Read,
                    vec![cur],
                );
            }
            Step::Take { n } => {
                cur = node(
                    NodeKind::Take {
                        n: *n,
                        implicit: false,
                    },
                    Backend::Native,
                    Effect::Read,
                    vec![cur],
                );
            }
            Step::Union(rhs) => {
                let right = plan_pipeline(rhs, cat, false)?;
                cur = node(
                    NodeKind::Union,
                    Backend::Native,
                    Effect::Read,
                    vec![cur, right],
                );
            }
        }
    }

    if allow_implicit_take && implicit_take {
        cur = node(
            NodeKind::Take {
                n: Some(50),
                implicit: true,
            },
            Backend::Native,
            Effect::Read,
            vec![cur],
        );
    } else if take_n.is_none() && !matches!(cur.kind, NodeKind::Take { .. }) {
        // count/sum without take: no implicit limit
    }

    Ok(cur)
}

fn apply_filter(cur: Node, pred: &Pred, cat: &Catalog) -> Node {
    let col_name = match &cur.kind {
        NodeKind::Scan { collection, .. }
        | NodeKind::Get { collection, .. }
        | NodeKind::IndexSeek { collection, .. } => collection.as_str(),
        _ => "",
    };
    let extracted = extract_point(pred, cat.collection(col_name));
    let mut out = cur;
    if let Some(((field, value), _)) = extracted.as_ref()
        && let NodeKind::Scan { collection, .. } = &out.kind
    {
        out = node(
            NodeKind::Get {
                collection: collection.clone(),
                key: fmt_point_key(field, value),
            },
            Backend::Idb,
            Effect::Read,
            vec![],
        );
    } else if let NodeKind::Scan { collection, .. } = &out.kind
        && let Some(uses) = crate::index::pick_index(cat, collection, pred)
    {
        out = node(
            NodeKind::IndexSeek {
                collection: collection.clone(),
                fields: uses[0].def.fields.clone(),
            },
            Backend::Native,
            Effect::Read,
            vec![],
        );
    }
    if let Some((_, residuals)) = extracted {
        if !residuals.is_empty() {
            if let NodeKind::Filter { pred: existing } = &mut out.kind {
                let extra: usize = residuals
                    .iter()
                    .map(|r| 5 + estimate_pred_fmt_len(r))
                    .sum();
                existing.reserve(extra);
                for r in residuals {
                    existing.push_str(" and ");
                    fmt_pred_into(existing, r);
                }
                return out;
            }
            out = filter_node_slice(out, &residuals);
        }
    } else if let NodeKind::Filter { pred: existing } = &mut out.kind {
        existing.reserve(5 + estimate_pred_fmt_len(pred));
        existing.push_str(" and ");
        fmt_pred_into(existing, pred);
        return out;
    } else {
        out = filter_node(out, pred);
    }
    out
}

fn filter_node(inner: Node, pred: &Pred) -> Node {
    node(
        NodeKind::Filter {
            pred: fmt_pred(pred),
        },
        Backend::Native,
        Effect::Read,
        vec![inner],
    )
}

fn filter_node_slice(inner: Node, preds: &[&Pred]) -> Node {
    let cap: usize = preds
        .iter()
        .map(|p| estimate_pred_fmt_len(p) + 5)
        .sum();
    let mut s = String::with_capacity(cap);
    for (i, p) in preds.iter().enumerate() {
        if i > 0 {
            s.push_str(" and ");
        }
        fmt_pred_into(&mut s, p);
    }
    node(
        NodeKind::Filter { pred: s },
        Backend::Native,
        Effect::Read,
        vec![inner],
    )
}

fn fmt_point_key(field: &str, value: &Value) -> String {
    let mut key = String::with_capacity(field.len() + 16);
    key.push_str(field);
    key.push('=');
    fmt_value_into(&mut key, value);
    key
}

fn extract_point<'a>(
    pred: &'a Pred,
    col: Option<&crate::catalog::Collection>,
) -> Option<((&'a str, &'a Value), smallvec::SmallVec<[&'a Pred; 4]>)> {
    match pred {
        Pred::Cmp {
            field,
            op: CmpOp::Eq,
            value,
        } if field.parts.len() == 1
            && (matches!(field.parts[0].as_str(), "id" | "uri")
                || col
                    .and_then(|c| c.field(&field.parts[0]))
                    .is_some_and(|f| f.unique))
            && matches!(value, Value::String(_) | Value::Param(_)) =>
        {
            Some(((field.parts[0].as_str(), value), smallvec::SmallVec::new()))
        }
        Pred::And(a, b) => {
            if let Some((pa, mut ra)) = extract_point(a, col) {
                ra.push(b);
                return Some((pa, ra));
            }
            if let Some((pb, rb)) = extract_point(b, col) {
                let mut out = smallvec::smallvec![a.as_ref()];
                out.extend(rb);
                return Some((pb, out));
            }
            None
        }
        _ => None,
    }
}

fn wrap_search(input: Node, mode: SearchMode, query: &str, k: i64, cat: &Catalog) -> Node {
    let input = maybe_fts_seek(input, mode, query, cat);
    match mode {
        SearchMode::Hybrid => node(
            NodeKind::Search {
                mode: SearchMode::Hybrid,
                query: query.to_string(),
                k,
            },
            Backend::Native,
            Effect::Read,
            vec![input],
        )
        .with_note("hybrid→lex(FtsSeek)+vec (hash embedder)"),
        SearchMode::Vec => node(
            NodeKind::Search {
                mode: SearchMode::Vec,
                query: query.to_string(),
                k,
            },
            Backend::Native,
            Effect::Read,
            vec![input],
        )
        .with_note("vec cosine (hash embedder)"),
        SearchMode::Lex => node(
            NodeKind::Search {
                mode: SearchMode::Lex,
                query: query.to_string(),
                k,
            },
            Backend::Native,
            Effect::Read,
            vec![input],
        )
        .with_note("lex via FtsSeek when available"),
    }
}

fn maybe_fts_seek(input: Node, mode: SearchMode, query: &str, cat: &Catalog) -> Node {
    if !matches!(mode, SearchMode::Lex | SearchMode::Hybrid) {
        return input;
    }
    let NodeKind::Scan { collection, .. } = &input.kind else {
        return input;
    };
    let Some(cdef) = cat.collections.get(collection) else {
        return input;
    };
    let fields: Vec<String> = cdef
        .fields
        .iter()
        .filter(|(_, f)| f.fts)
        .map(|(n, _)| n.clone())
        .collect();
    if fields.is_empty() {
        return input;
    }
    node(
        NodeKind::FtsSeek {
            collection: collection.clone(),
            fields,
            query: query.to_string(),
        },
        Backend::Native,
        Effect::Read,
        vec![],
    )
}

fn fmt_group_keys(by: &[GroupKey]) -> String {
    if by.is_empty() {
        return "*".into();
    }
    by.iter()
        .map(|gk| match &gk.expr {
            GroupExpr::Field(f) => f.as_str(),
            GroupExpr::Bin { field, step } => {
                let s = match step {
                    BinStep::Duration(ms) => format!("{ms}ms"),
                    BinStep::Int(n) => n.to_string(),
                    BinStep::Float(f) => f.to_string(),
                };
                format!("bin({}, {s})", field.as_str())
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn fmt_summarize_op(specs: &[AggSpec]) -> String {
    let parts: Vec<String> = specs
        .iter()
        .map(|s| match &s.func {
            AggFunc::Count => "count".into(),
            AggFunc::Sum(f) => format!("sum({})", f.as_str()),
            AggFunc::Avg(f) => format!("avg({})", f.as_str()),
            AggFunc::Min(f) => format!("min({})", f.as_str()),
            AggFunc::Max(f) => format!("max({})", f.as_str()),
            AggFunc::DCount(f) => format!("dcount({})", f.as_str()),
            AggFunc::Percentile { field, pct } => {
                format!("percentile({}, {pct})", field.as_str())
            }
            AggFunc::ArgMax { order, value } => {
                format!("arg_max({}, {})", order.as_str(), value.as_str())
            }
            AggFunc::ArgMin { order, value } => {
                format!("arg_min({}, {})", order.as_str(), value.as_str())
            }
        })
        .collect();
    format!("summarize({})", parts.join(", "))
}

fn scan_cols(
    cat: &Catalog,
    collection: &str,
    needed: &[String],
    picked: &[String],
    want_search: bool,
    want_vec: bool,
) -> Vec<String> {
    let Some(col) = cat.collection(collection) else {
        return needed.to_vec();
    };
    let mut cols = Vec::new();
    let push = |cols: &mut Vec<String>, n: &str| {
        if col.field(n).is_some() && !cols.iter().any(|c| c == n) {
            cols.push(n.to_string());
        }
    };
    push(&mut cols, "id");
    for n in needed {
        let base = n.split('.').next().unwrap_or(n);
        if base == "body" && !picked.iter().any(|p| p == "body" || p.ends_with(".body")) {
            continue;
        }
        if base == "embedding" && !want_vec {
            continue;
        }
        push(&mut cols, base);
    }
    if want_search {
        push(&mut cols, "title");
        push(&mut cols, "snippet");
        push(&mut cols, "wing");
    }
    if want_vec {
        push(&mut cols, "embedding");
    }
    if cols.is_empty() {
        push(&mut cols, "id");
    }
    cols
}

fn join_right_cols(cat: &Catalog, collection: &str) -> Vec<String> {
    let Some(col) = cat.collection(collection) else {
        return vec!["id".into()];
    };
    col.fields
        .keys()
        .filter(|k| k.as_str() != "body" && k.as_str() != "embedding")
        .cloned()
        .collect()
}

fn collect_pred_fields(pred: &Pred, needed: &mut Vec<String>) {
    match pred {
        Pred::And(a, b) | Pred::Or(a, b) => {
            collect_pred_fields(a, needed);
            collect_pred_fields(b, needed);
        }
        Pred::Not(inner) => collect_pred_fields(inner, needed),
        Pred::Cmp { field, .. }
        | Pred::IsNull { field, .. }
        | Pred::In { field, .. }
        | Pred::Has { field, .. }
        | Pred::Contains { field, .. }
        | Pred::Regex { field, .. } => push_field(needed, field.as_cow_str()),
    }
}

#[inline]
fn push_field(out: &mut Vec<String>, name: std::borrow::Cow<'_, str>) {
    if !out.iter().any(|x| x == name.as_ref()) {
        out.push(name.into_owned());
    }
}

fn point_or_scan(collection: &str, pred: Option<&Pred>, cat: &Catalog) -> Node {
    if let Some(p) = pred
        && let Some(((field, value), residuals)) = extract_point(p, cat.collection(collection))
    {
        let mut n = node(
            NodeKind::Get {
                collection: collection.to_string(),
                key: fmt_point_key(field, value),
            },
            Backend::Idb,
            Effect::Read,
            vec![],
        );
        if !residuals.is_empty() {
            n = filter_node_slice(n, &residuals);
        }
        return n;
    }
    if let Some(p) = pred
        && let Some(uses) = crate::index::pick_index(cat, collection, p)
        && !uses.is_empty()
    {
        let seek = node(
            NodeKind::IndexSeek {
                collection: collection.to_string(),
                fields: uses[0].def.fields.clone(),
            },
            Backend::Native,
            Effect::Read,
            vec![],
        );
        return filter_node(seek, p);
    }
    if let Some(p) = pred {
        let scan = node(
            NodeKind::Scan {
                collection: collection.to_string(),
                cols: scan_cols(cat, collection, &[], &[], false, false),
            },
            Backend::Native,
            Effect::Read,
            vec![],
        );
        return filter_node(scan, p);
    }
    node(
        NodeKind::Scan {
            collection: collection.to_string(),
            cols: scan_cols(cat, collection, &[], &[], false, false),
        },
        Backend::Native,
        Effect::Read,
        vec![],
    )
}

fn write_plan(cat: &Catalog, effects: Vec<Effect>, inner: Node, explain: ExplainKind) -> Plan {
    Plan {
        root: reduce(cat, effects, inner),
        explain,
    }
}

fn reduce(cat: &Catalog, effects: Vec<Effect>, inner: Node) -> Node {
    Node {
        kind: NodeKind::Reduce {
            effects,
            snapshot: "fixture".into(),
            embed: cat.embed_id.clone(),
        },
        backend: Backend::Native,
        effect: Effect::Read,
        children: vec![inner],
        notes: vec![],
    }
}

fn node(kind: NodeKind, backend: Backend, effect: Effect, children: Vec<Node>) -> Node {
    Node {
        kind,
        backend,
        effect,
        children,
        notes: vec![],
    }
}

impl Node {
    fn with_note(mut self, note: impl Into<String>) -> Self {
        self.notes.push(note.into());
        self
    }
}

fn stamp_backend(node: &mut Node, backend: Backend) {
    node.backend = backend;
    for c in &mut node.children {
        stamp_backend(c, backend);
    }
}

fn record_preview(record: &Record) -> String {
    let parts: Vec<String> = record
        .fields
        .iter()
        .map(|(k, v)| format!("{k}: {}", fmt_value(v)))
        .collect();
    format!("{{{}}}", parts.join(", "))
}

fn decl_detail(d: &Decl) -> String {
    match d {
        Decl::Col { name, append, .. } => {
            if *append {
                format!("col {name} append")
            } else {
                format!("col {name}")
            }
        }
        Decl::Rel { name, stub } => {
            if *stub {
                format!("rel {name} stub")
            } else {
                format!("rel {name}")
            }
        }
        Decl::RelReverse { name, of } => format!("rel {name} = reverse {of}"),
        Decl::Fk {
            from_col,
            from_field,
            to_col,
            to_field,
        } => format!("fk {from_col}.{from_field} → {to_col}.{to_field}"),
        Decl::Guard {
            collection, field, ..
        } => format!("guard {collection}.{field} immutable"),
        Decl::Filter {
            collection,
            pred,
            src,
        } => {
            if pred.is_none() {
                format!("unfilter {collection}")
            } else {
                format!("filter {collection} {src}")
            }
        }
        Decl::Owned { name, .. } => format!("owned {name}"),
        Decl::Index {
            collection,
            unique,
            fields,
            pred: _,
        } => {
            if *unique {
                format!("index {collection} unique [{}]", fields.join(", "))
            } else {
                format!("index {collection} [{}]", fields.join(", "))
            }
        }
    }
}

#[inline]
fn push_i64(out: &mut String, n: i64) {
    let mut u = if n < 0 {
        out.push('-');
        n.wrapping_neg() as u64
    } else {
        n as u64
    };
    let mut buf = [0u8; 20];
    let mut i = 20;
    loop {
        i -= 1;
        buf[i] = b'0' + (u % 10) as u8;
        u /= 10;
        if u == 0 {
            break;
        }
    }
    // SAFETY: `buf[i..]` contains only ASCII digits `'0'..='9'`.
    out.push_str(unsafe { std::str::from_utf8_unchecked(&buf[i..]) });
}

#[inline]
fn fmt_field_into(out: &mut String, field: &Field) {
    match field.parts.as_slice() {
        [p] => out.push_str(p),
        parts => {
            for (i, p) in parts.iter().enumerate() {
                if i > 0 {
                    out.push('.');
                }
                out.push_str(p);
            }
        }
    }
}

#[inline]
fn fmt_debug_str_into(out: &mut String, s: &str) {
    let trivial = s
        .as_bytes()
        .iter()
        .all(|&b| (0x20..=0x7E).contains(&b) && b != b'"' && b != b'\\');
    if trivial {
        out.push('"');
        out.push_str(s);
        out.push('"');
    } else {
        use std::fmt::Write as _;
        let _ = write!(out, "{s:?}");
    }
}

#[inline]
pub(crate) fn fmt_value_into(out: &mut String, v: &Value) {
    match v {
        Value::String(s) => fmt_debug_str_into(out, s),
        Value::Int(n) => push_i64(out, *n),
        Value::Float(n) => {
            use std::fmt::Write as _;
            let _ = write!(out, "{n}");
        }
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Timestamp(millis) => {
            out.push_str("timestamp(");
            push_i64(out, *millis);
            out.push(')');
        }
        Value::Now => out.push_str("now"),
        Value::NowMinus(d) => {
            out.push_str("now - ");
            push_i64(out, d.n);
            out.push(d.unit.as_char());
        }
        Value::Duration(d) => {
            push_i64(out, d.n);
            out.push(d.unit.as_char());
        }
        Value::Name(n) => out.push_str(n),
        Value::Param(p) => {
            out.push('$');
            out.push_str(p);
        }
    }
}

fn estimate_pred_fmt_len(pred: &Pred) -> usize {
    match pred {
        Pred::And(a, b) => estimate_pred_fmt_len(a) + 5 + estimate_pred_fmt_len(b),
        Pred::Or(a, b) => estimate_pred_fmt_len(a) + 4 + estimate_pred_fmt_len(b),
        Pred::Not(inner) => 6 + estimate_pred_fmt_len(inner),
        Pred::Cmp { field, value, .. } => {
            let vlen = match value {
                Value::String(s) => s.len() + 2,
                Value::Name(s) => s.len(),
                Value::Param(s) => s.len() + 1,
                _ => 12,
            };
            field.parts.iter().map(String::len).sum::<usize>() + 5 + vlen
        }
        Pred::IsNull { field, negated } => {
            field.parts.iter().map(String::len).sum::<usize>() + if *negated { 13 } else { 9 }
        }
        Pred::In { field, values } => {
            field.parts.iter().map(String::len).sum::<usize>() + 6 + values.len() * 10
        }
        Pred::Has { field, ci, needle } => {
            field.parts.iter().map(String::len).sum::<usize>()
                + if *ci { 10 } else { 7 }
                + needle.len()
        }
        Pred::Contains { field, needle } => {
            field.parts.iter().map(String::len).sum::<usize>() + 5 + needle.len()
        }
        Pred::Regex {
            field,
            pattern,
            flags,
        } => {
            field.parts.iter().map(String::len).sum::<usize>() + 5 + pattern.len() + flags.len()
        }
    }
}

pub(crate) fn fmt_pred_into(out: &mut String, pred: &Pred) {
    match pred {
        Pred::And(a, b) => {
            fmt_pred_into(out, a);
            out.push_str(" and ");
            fmt_pred_into(out, b);
        }
        Pred::Or(a, b) => {
            fmt_pred_into(out, a);
            out.push_str(" or ");
            fmt_pred_into(out, b);
        }
        Pred::Not(inner) => {
            out.push_str("not (");
            fmt_pred_into(out, inner);
            out.push(')');
        }
        Pred::Cmp { field, op, value } => {
            fmt_field_into(out, field);
            out.push(' ');
            out.push_str(op.as_str());
            out.push(' ');
            fmt_value_into(out, value);
        }
        Pred::IsNull { field, negated } => {
            fmt_field_into(out, field);
            out.push_str(if *negated { " is not null" } else { " is null" });
        }
        Pred::In { field, values } => {
            fmt_field_into(out, field);
            out.push_str(" in [");
            for (i, v) in values.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                fmt_value_into(out, v);
            }
            out.push(']');
        }
        Pred::Has { field, ci, needle } => {
            fmt_field_into(out, field);
            out.push_str(if *ci { " has ci " } else { " has " });
            fmt_debug_str_into(out, needle);
        }
        Pred::Contains { field, needle } => {
            fmt_field_into(out, field);
            out.push_str(" ~ ");
            fmt_debug_str_into(out, needle);
        }
        Pred::Regex {
            field,
            pattern,
            flags,
        } => {
            fmt_field_into(out, field);
            out.push_str(" ~ /");
            out.push_str(pattern);
            out.push('/');
            out.push_str(flags);
        }
    }
}

pub fn fmt_pred(pred: &Pred) -> String {
    let mut out = String::with_capacity(estimate_pred_fmt_len(pred));
    fmt_pred_into(&mut out, pred);
    out
}

pub fn fmt_value(v: &Value) -> String {
    let mut out = String::with_capacity(16);
    fmt_value_into(&mut out, v);
    out
}

// ============================================================================
// Logical Optimizer Rewrites, Volatile Barriers, Lazy DNF & Plan Fingerprints
// (`D14`: `LIN-0526`..`LIN-0560`, `LIN-0597`)
// ============================================================================

use serde::{Deserialize, Serialize};

/// Default upper bound on lazily streamed DNF clauses (`LIN-0526`, `LIN-0597`).
pub const MAX_DNF_CLAUSES: usize = 128;

/// Maximum AST nodes visited when building a bounded lazy DNF cursor (`LIN-0597`).
const MAX_DNF_AST_NODES: usize = 1024;

/// Configuration for [`plan_program_with_opts`] (`LIN-0533`..`LIN-0536`, `LIN-0550`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OptimizerOptions {
    pub enable_optimizations: bool,
    pub max_iterations: usize,
    pub record_trace: bool,
}

impl Default for OptimizerOptions {
    fn default() -> Self {
        Self {
            enable_optimizations: true,
            max_iterations: 8,
            record_trace: false,
        }
    }
}

impl OptimizerOptions {
    pub const fn unoptimized() -> Self {
        Self {
            enable_optimizations: false,
            max_iterations: 0,
            record_trace: false,
        }
    }

    pub const fn traced() -> Self {
        Self {
            enable_optimizations: true,
            max_iterations: 8,
            record_trace: true,
        }
    }
}

/// Trace record emitted per logical/physical optimizer pass (`LIN-0535`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RewriteTraceEntry {
    pub pass_name: String,
    pub changed: bool,
}

/// Diagnostic counters collected during optimization (`LIN-0552`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct OptimizerDebugCounters {
    pub iterations_run: usize,
    pub passes_evaluated: usize,
    pub passes_changed: usize,
    pub predicates_simplified: usize,
    pub conjuncts_deduplicated: usize,
    pub filters_merged: usize,
    pub point_gets_selected: usize,
    pub index_seeks_selected: usize,
    pub dnf_clauses_inspected: usize,
    pub dnf_caps_hit: usize,
    pub volatile_barriers_preserved: usize,
}

/// Returns `true` if `pred` references a runtime-relative time (`now`, `now - dur`),
/// relative [`Duration`], or unbound `$param` (`LIN-0539`).
pub fn has_volatile_barrier(pred: &Pred) -> bool {
    match pred {
        Pred::And(a, b) | Pred::Or(a, b) => has_volatile_barrier(a) || has_volatile_barrier(b),
        Pred::Not(inner) => has_volatile_barrier(inner),
        Pred::Cmp { value, .. } => value_is_volatile(value),
        Pred::In { values, .. } => values.iter().any(value_is_volatile),
        Pred::IsNull { .. }
        | Pred::Has { .. }
        | Pred::Contains { .. }
        | Pred::Regex { .. } => false,
    }
}

/// Returns `true` iff `pred` depends solely on row columns and immutable constants (`LIN-0538`).
#[inline]
pub fn is_deterministic_pred(pred: &Pred) -> bool {
    !has_volatile_barrier(pred)
}

#[inline]
fn value_is_volatile(v: &Value) -> bool {
    matches!(
        v,
        Value::Now | Value::NowMinus(_) | Value::Duration(_) | Value::Param(_)
    )
}

/// Deduplicate repeated conjuncts/disjuncts and factor common deterministic conjuncts
/// across `Or` branches (`(A and B) or (A and C)` -> `A and (B or C)`, `LIN-0537`).
///
/// Returns `(rewritten_predicate, shared_or_removed_count)`.
pub fn deduplicate_conjuncts(pred: &Pred) -> (Pred, usize) {
    match pred {
        Pred::And(_, _) => {
            let mut raw = Vec::new();
            collect_and_leaves(pred, &mut raw);
            let mut unique: Vec<Pred> = Vec::with_capacity(raw.len());
            let mut unique_fps: Vec<u64> = Vec::with_capacity(raw.len());
            let mut removed = 0usize;
            for item in raw {
                let (sub, sub_rem) = deduplicate_conjuncts(item);
                removed += sub_rem;
                if is_deterministic_pred(&sub) {
                    let fp = predicate_fingerprint(&sub);
                    if unique_fps.contains(&fp)
                        && unique.iter().any(|u| is_deterministic_pred(u) && predicate_fingerprint(u) == fp)
                    {
                        removed += 1;
                        continue;
                    }
                    unique_fps.push(fp);
                }
                unique.push(sub);
            }
            let mut it = unique.into_iter();
            let first = it.next().expect("conjunction has at least one term");
            let rebuilt = it.fold(first, |acc, r| Pred::And(Box::new(acc), Box::new(r)));
            (rebuilt, removed)
        }
        Pred::Or(a, b) => {
            let (da, ra) = deduplicate_conjuncts(a);
            let (db, rb) = deduplicate_conjuncts(b);
            let mut removed = ra + rb;
            if is_deterministic_pred(&da)
                && is_deterministic_pred(&db)
                && predicate_fingerprint(&da) == predicate_fingerprint(&db)
            {
                return (da, removed + 1);
            }
            // Common-conjunct factoring: `(C and X) or (C and Y)` -> `C and (X or Y)`
            let mut left_terms = Vec::new();
            let mut right_terms = Vec::new();
            collect_and_leaves(&da, &mut left_terms);
            collect_and_leaves(&db, &mut right_terms);

            let mut shared: Vec<Pred> = Vec::new();
            let mut shared_fps: Vec<u64> = Vec::new();
            for &lt in &left_terms {
                if !is_deterministic_pred(lt) {
                    continue;
                }
                let lfp = predicate_fingerprint(lt);
                if !shared_fps.contains(&lfp)
                    && right_terms
                        .iter()
                        .any(|rt| is_deterministic_pred(rt) && predicate_fingerprint(rt) == lfp)
                {
                    shared_fps.push(lfp);
                    shared.push(lt.clone());
                }
            }
            if !shared.is_empty() {
                removed += shared.len();
                let rem_left: Vec<Pred> = left_terms
                    .into_iter()
                    .filter(|t| {
                        !is_deterministic_pred(t) || !shared_fps.contains(&predicate_fingerprint(t))
                    })
                    .cloned()
                    .collect();
                let rem_right: Vec<Pred> = right_terms
                    .into_iter()
                    .filter(|t| {
                        !is_deterministic_pred(t) || !shared_fps.contains(&predicate_fingerprint(t))
                    })
                    .cloned()
                    .collect();

                let mut it = shared.into_iter();
                let mut prefix = it.next().unwrap();
                for s in it {
                    prefix = Pred::And(Box::new(prefix), Box::new(s));
                }
                // Absorption: `(C) or (C and Y)` == `C`
                if rem_left.is_empty() || rem_right.is_empty() {
                    return (prefix, removed);
                }
                let l_pred = rem_left
                    .into_iter()
                    .reduce(|acc, x| Pred::And(Box::new(acc), Box::new(x)))
                    .unwrap();
                let r_pred = rem_right
                    .into_iter()
                    .reduce(|acc, x| Pred::And(Box::new(acc), Box::new(x)))
                    .unwrap();
                let factored_or = Pred::Or(Box::new(l_pred), Box::new(r_pred));
                return (Pred::And(Box::new(prefix), Box::new(factored_or)), removed);
            }
            (Pred::Or(Box::new(da), Box::new(db)), removed)
        }
        Pred::Not(inner) => {
            let (di, r) = deduplicate_conjuncts(inner);
            (Pred::Not(Box::new(di)), r)
        }
        other => (other.clone(), 0),
    }
}

fn collect_and_leaves<'a>(pred: &'a Pred, out: &mut Vec<&'a Pred>) {
    match pred {
        Pred::And(a, b) => {
            collect_and_leaves(a, out);
            collect_and_leaves(b, out);
        }
        other => out.push(other),
    }
}

enum DnfNode<'a> {
    Atom(std::borrow::Cow<'a, Pred>),
    Or(Vec<DnfNode<'a>>),
    And(Vec<DnfNode<'a>>),
}

enum DnfCursor {
    Atom { done: bool },
    Or { branch: usize, sub: Box<DnfCursor> },
    And { subs: Vec<DnfCursor>, done: bool },
}

/// Bounded lazy DNF clause iterator (`LIN-0526`, `LIN-0597`).
///
/// Streams conjunction clauses one at a time using an explicit odometer cursor without
/// eagerly materializing exponential `And`-over-`Or` Cartesian products.
pub struct LazyDnfIter<'a> {
    root: Option<DnfNode<'a>>,
    cursor: Option<DnfCursor>,
    emitted: usize,
    max_clauses: usize,
    capped: bool,
}

impl<'a> LazyDnfIter<'a> {
    pub fn new(pred: &'a Pred, max_clauses: usize) -> Self {
        let cap = max_clauses.max(1);
        let mut visited = 0usize;
        match Self::build_tree(pred, cap, &mut visited) {
            Some(tree) => {
                let cursor = Self::init_cursor(&tree);
                Self {
                    root: Some(tree),
                    cursor: Some(cursor),
                    emitted: 0,
                    max_clauses: cap,
                    capped: false,
                }
            }
            None => Self {
                root: None,
                cursor: None,
                emitted: 0,
                max_clauses: cap,
                capped: true,
            },
        }
    }

    /// True if AST node budget or `max_clauses` cap truncated full DNF enumeration (`LIN-0597`).
    #[inline]
    pub const fn was_capped(&self) -> bool {
        self.capped
    }

    #[inline]
    pub const fn clauses_emitted(&self) -> usize {
        self.emitted
    }

    fn build_tree(
        pred: &'a Pred,
        max_clauses: usize,
        visited: &mut usize,
    ) -> Option<DnfNode<'a>> {
        *visited += 1;
        if *visited > MAX_DNF_AST_NODES {
            return None;
        }
        match pred {
            Pred::Or(a, b) => {
                let mut branches = Vec::new();
                Self::collect_or(a, max_clauses, visited, &mut branches)?;
                Self::collect_or(b, max_clauses, visited, &mut branches)?;
                Some(DnfNode::Or(branches))
            }
            Pred::And(a, b) => {
                let mut factors = Vec::new();
                Self::collect_and(a, max_clauses, visited, &mut factors)?;
                Self::collect_and(b, max_clauses, visited, &mut factors)?;
                Some(DnfNode::And(factors))
            }
            Pred::In { field, values } if !values.is_empty() && values.len() <= max_clauses => {
                let branches = values
                    .iter()
                    .map(|v| {
                        DnfNode::Atom(std::borrow::Cow::Owned(Pred::Cmp {
                            field: field.clone(),
                            op: CmpOp::Eq,
                            value: v.clone(),
                        }))
                    })
                    .collect();
                Some(DnfNode::Or(branches))
            }
            Pred::In { values, .. } if values.len() > max_clauses => None,
            other => Some(DnfNode::Atom(std::borrow::Cow::Borrowed(other))),
        }
    }

    fn collect_or(
        pred: &'a Pred,
        max_clauses: usize,
        visited: &mut usize,
        out: &mut Vec<DnfNode<'a>>,
    ) -> Option<()> {
        match Self::build_tree(pred, max_clauses, visited)? {
            DnfNode::Or(sub) => out.extend(sub),
            other => out.push(other),
        }
        if out.len() > max_clauses {
            return None;
        }
        Some(())
    }

    fn collect_and(
        pred: &'a Pred,
        max_clauses: usize,
        visited: &mut usize,
        out: &mut Vec<DnfNode<'a>>,
    ) -> Option<()> {
        match Self::build_tree(pred, max_clauses, visited)? {
            DnfNode::And(sub) => out.extend(sub),
            other => out.push(other),
        }
        Some(())
    }

    fn init_cursor(node: &DnfNode<'a>) -> DnfCursor {
        match node {
            DnfNode::Atom(_) => DnfCursor::Atom { done: false },
            DnfNode::Or(branches) => {
                let first = branches
                    .first()
                    .map(Self::init_cursor)
                    .unwrap_or(DnfCursor::Atom { done: true });
                DnfCursor::Or {
                    branch: 0,
                    sub: Box::new(first),
                }
            }
            DnfNode::And(factors) => DnfCursor::And {
                subs: factors.iter().map(Self::init_cursor).collect(),
                done: factors.is_empty(),
            },
        }
    }

    fn is_done(cur: &DnfCursor) -> bool {
        match cur {
            DnfCursor::Atom { done } => *done,
            DnfCursor::Or { sub, .. } => Self::is_done(sub),
            DnfCursor::And { done, .. } => *done,
        }
    }

    fn read_current(
        node: &DnfNode<'a>,
        cur: &DnfCursor,
        out: &mut Vec<std::borrow::Cow<'a, Pred>>,
    ) {
        match (node, cur) {
            (DnfNode::Atom(p), DnfCursor::Atom { done: false }) => out.push(p.clone()),
            (DnfNode::Or(branches), DnfCursor::Or { branch, sub }) => {
                if let Some(b) = branches.get(*branch) {
                    Self::read_current(b, sub, out);
                }
            }
            (DnfNode::And(factors), DnfCursor::And { subs, done: false }) => {
                for (f, s) in factors.iter().zip(subs.iter()) {
                    Self::read_current(f, s, out);
                }
            }
            _ => {}
        }
    }

    /// Advance cursor to next clause; returns `true` if another clause is available.
    fn advance(node: &DnfNode<'a>, cur: &mut DnfCursor) -> bool {
        match (node, cur) {
            (DnfNode::Atom(_), DnfCursor::Atom { done }) => {
                *done = true;
                false
            }
            (DnfNode::Or(branches), DnfCursor::Or { branch, sub }) => {
                if let Some(b) = branches.get(*branch)
                    && Self::advance(b, sub)
                {
                    return true;
                }
                while *branch + 1 < branches.len() {
                    *branch += 1;
                    **sub = Self::init_cursor(&branches[*branch]);
                    if !Self::is_done(sub) {
                        return true;
                    }
                }
                *sub = Box::new(DnfCursor::Atom { done: true });
                false
            }
            (DnfNode::And(factors), DnfCursor::And { subs, done }) => {
                if *done {
                    return false;
                }
                for i in (0..factors.len()).rev() {
                    if Self::advance(&factors[i], &mut subs[i]) {
                        for j in (i + 1)..factors.len() {
                            subs[j] = Self::init_cursor(&factors[j]);
                        }
                        return true;
                    }
                }
                *done = true;
                false
            }
            _ => false,
        }
    }
}

impl<'a> Iterator for LazyDnfIter<'a> {
    type Item = Vec<std::borrow::Cow<'a, Pred>>;

    fn next(&mut self) -> Option<Self::Item> {
        let (Some(root), Some(cur)) = (&self.root, &mut self.cursor) else {
            return None;
        };
        if Self::is_done(cur) {
            return None;
        }
        if self.emitted >= self.max_clauses {
            self.capped = true;
            self.cursor = None;
            return None;
        }
        let mut clause = Vec::new();
        Self::read_current(root, cur, &mut clause);
        self.emitted += 1;
        let has_more = Self::advance(root, cur);
        if !has_more {
            self.cursor = None;
        }
        Some(clause)
    }
}

/// Stream up to `max_clauses` DNF clauses and report whether the cap was reached (`LIN-0526`, `LIN-0597`).
pub fn stream_dnf_bounded<'a>(
    pred: &'a Pred,
    max_clauses: usize,
) -> (Vec<Vec<std::borrow::Cow<'a, Pred>>>, bool) {
    let mut iter = LazyDnfIter::new(pred, max_clauses);
    let mut out = Vec::new();
    while let Some(c) = iter.next() {
        out.push(c);
    }
    (out, iter.was_capped())
}

#[derive(Clone, Copy)]
struct Fnv64(u64);

impl Fnv64 {
    #[inline]
    const fn new() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }

    #[inline]
    fn byte(&mut self, b: u8) {
        self.0 ^= u64::from(b);
        self.0 = self.0.wrapping_mul(0x0100_0000_01b3);
    }

    #[inline]
    fn bytes(&mut self, bs: &[u8]) {
        for &b in bs {
            self.byte(b);
        }
    }

    #[inline]
    fn str(&mut self, s: &str) {
        self.u64(s.len() as u64);
        self.bytes(s.as_bytes());
    }

    #[inline]
    fn u64(&mut self, v: u64) {
        self.bytes(&v.to_le_bytes());
    }

    #[inline]
    fn i64(&mut self, v: i64) {
        self.bytes(&v.to_le_bytes());
    }

    #[inline]
    fn finish(self) -> u64 {
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }
}

/// Normalized, order-independent 64-bit fingerprint for a [`Pred`] (`LIN-0540`).
///
/// Invariant under:
/// - Commutativity, associativity, and idempotence of `and` / `or`
/// - Double negation, negated comparisons (`not (a != 1)` == `a == 1`), `not (a is null)`
/// - De Morgan pushdown (`not (a or b)` == `(not a) and (not b)`)
/// - Singleton & unordered duplicate `in [...]` lists (`x in [2, 1, 2]` == `x in [1, 2]`, `x in [1]` == `x == 1`)
/// - Signed zero (`-0.0` == `0.0`) and canonical `NaN` bit patterns
pub fn predicate_fingerprint(pred: &Pred) -> u64 {
    pred_fp_inner(pred, false, 0)
}

fn pred_fp_inner(pred: &Pred, negated: bool, depth: usize) -> u64 {
    if depth > 64 {
        let mut h = Fnv64::new();
        h.byte(if negated { 0xE1 } else { 0xE0 });
        h.str(&fmt_pred(pred));
        return h.finish();
    }
    match pred {
        Pred::Not(inner) => pred_fp_inner(inner, !negated, depth + 1),
        Pred::And(a, b) if !negated => {
            let mut fps = Vec::with_capacity(4);
            collect_norm_conjunct_fps(a, false, depth + 1, &mut fps);
            collect_norm_conjunct_fps(b, false, depth + 1, &mut fps);
            fps.sort_unstable();
            fps.dedup();
            if fps.len() == 1 {
                return fps[0];
            }
            let mut h = Fnv64::new();
            h.byte(0x01);
            h.u64(fps.len() as u64);
            for fp in fps {
                h.u64(fp);
            }
            h.finish()
        }
        Pred::Or(a, b) => {
            if negated {
                // De Morgan: `not (a or b)` == `(not a) and (not b)`
                let mut fps = Vec::with_capacity(4);
                collect_norm_conjunct_fps(a, true, depth + 1, &mut fps);
                collect_norm_conjunct_fps(b, true, depth + 1, &mut fps);
                fps.sort_unstable();
                fps.dedup();
                if fps.len() == 1 {
                    return fps[0];
                }
                let mut h = Fnv64::new();
                h.byte(0x01);
                h.u64(fps.len() as u64);
                for fp in fps {
                    h.u64(fp);
                }
                h.finish()
            } else {
                let mut fps = Vec::with_capacity(4);
                collect_norm_disjunct_fps(a, false, depth + 1, &mut fps);
                collect_norm_disjunct_fps(b, false, depth + 1, &mut fps);
                fps.sort_unstable();
                fps.dedup();
                if fps.len() == 1 {
                    return fps[0];
                }
                let mut h = Fnv64::new();
                h.byte(0x02);
                h.u64(fps.len() as u64);
                for fp in fps {
                    h.u64(fp);
                }
                h.finish()
            }
        }
        Pred::And(a, b) => {
            // `negated == true`: De Morgan `not (a and b)` == `(not a) or (not b)`
            let mut fps = Vec::with_capacity(4);
            collect_norm_disjunct_fps(a, true, depth + 1, &mut fps);
            collect_norm_disjunct_fps(b, true, depth + 1, &mut fps);
            fps.sort_unstable();
            fps.dedup();
            if fps.len() == 1 {
                return fps[0];
            }
            let mut h = Fnv64::new();
            h.byte(0x02);
            h.u64(fps.len() as u64);
            for fp in fps {
                h.u64(fp);
            }
            h.finish()
        }
        Pred::Cmp { field, op, value } => {
            let eff_op = if negated { invert_cmp_op(*op) } else { *op };
            cmp_atom_fp(field, eff_op, value)
        }
        Pred::IsNull {
            field,
            negated: is_not_null,
        } => {
            let eff = *is_not_null ^ negated;
            let mut h = Fnv64::new();
            h.byte(0x04);
            hash_field(&mut h, field);
            h.byte(u8::from(eff));
            h.finish()
        }
        Pred::In { field, values } => {
            // Normalize `x in [v1, v2, ...]` as disjunction of `x == vi` (or conjunction of `x != vi` if negated)
            let mut fps: Vec<u64> = values
                .iter()
                .map(|v| {
                    cmp_atom_fp(
                        field,
                        if negated { CmpOp::Ne } else { CmpOp::Eq },
                        v,
                    )
                })
                .collect();
            fps.sort_unstable();
            fps.dedup();
            if fps.len() == 1 {
                return fps[0];
            }
            let mut h = Fnv64::new();
            h.byte(if negated { 0x01 } else { 0x02 });
            h.u64(fps.len() as u64);
            for fp in fps {
                h.u64(fp);
            }
            h.finish()
        }
        Pred::Has { field, ci, needle } => {
            let mut h = Fnv64::new();
            h.byte(0x05);
            hash_field(&mut h, field);
            h.byte(u8::from(*ci));
            h.str(needle);
            let atom = h.finish();
            if negated { wrap_not_fp(atom) } else { atom }
        }
        Pred::Contains { field, needle } => {
            let mut h = Fnv64::new();
            h.byte(0x06);
            hash_field(&mut h, field);
            h.str(needle);
            let atom = h.finish();
            if negated { wrap_not_fp(atom) } else { atom }
        }
        Pred::Regex {
            field,
            pattern,
            flags,
        } => {
            let mut h = Fnv64::new();
            h.byte(0x07);
            hash_field(&mut h, field);
            h.str(pattern);
            h.str(flags);
            let atom = h.finish();
            if negated { wrap_not_fp(atom) } else { atom }
        }
    }
}

fn collect_norm_conjunct_fps(pred: &Pred, negated: bool, depth: usize, out: &mut Vec<u64>) {
    if depth <= 64 {
        match (pred, negated) {
            (Pred::Not(inner), neg) => {
                collect_norm_conjunct_fps(inner, !neg, depth + 1, out);
                return;
            }
            (Pred::And(a, b), false) => {
                collect_norm_conjunct_fps(a, false, depth + 1, out);
                collect_norm_conjunct_fps(b, false, depth + 1, out);
                return;
            }
            (Pred::Or(a, b), true) => {
                collect_norm_conjunct_fps(a, true, depth + 1, out);
                collect_norm_conjunct_fps(b, true, depth + 1, out);
                return;
            }
            (Pred::In { field, values }, true) => {
                for v in values {
                    out.push(cmp_atom_fp(field, CmpOp::Ne, v));
                }
                return;
            }
            _ => {}
        }
    }
    out.push(pred_fp_inner(pred, negated, depth));
}

fn collect_norm_disjunct_fps(pred: &Pred, negated: bool, depth: usize, out: &mut Vec<u64>) {
    if depth <= 64 {
        match (pred, negated) {
            (Pred::Not(inner), neg) => {
                collect_norm_disjunct_fps(inner, !neg, depth + 1, out);
                return;
            }
            (Pred::Or(a, b), false) => {
                collect_norm_disjunct_fps(a, false, depth + 1, out);
                collect_norm_disjunct_fps(b, false, depth + 1, out);
                return;
            }
            (Pred::And(a, b), true) => {
                collect_norm_disjunct_fps(a, true, depth + 1, out);
                collect_norm_disjunct_fps(b, true, depth + 1, out);
                return;
            }
            (Pred::In { field, values }, false) => {
                for v in values {
                    out.push(cmp_atom_fp(field, CmpOp::Eq, v));
                }
                return;
            }
            _ => {}
        }
    }
    out.push(pred_fp_inner(pred, negated, depth));
}

#[inline]
const fn invert_cmp_op(op: CmpOp) -> CmpOp {
    match op {
        CmpOp::Eq => CmpOp::Ne,
        CmpOp::Ne => CmpOp::Eq,
        CmpOp::Gt => CmpOp::Le,
        CmpOp::Lt => CmpOp::Ge,
        CmpOp::Ge => CmpOp::Lt,
        CmpOp::Le => CmpOp::Gt,
    }
}

fn wrap_not_fp(inner_fp: u64) -> u64 {
    let mut h = Fnv64::new();
    h.byte(0x03);
    h.u64(inner_fp);
    h.finish()
}

fn cmp_atom_fp(field: &Field, op: CmpOp, value: &Value) -> u64 {
    let mut h = Fnv64::new();
    h.byte(0x10);
    hash_field(&mut h, field);
    h.byte(match op {
        CmpOp::Eq => 1,
        CmpOp::Ne => 2,
        CmpOp::Gt => 3,
        CmpOp::Lt => 4,
        CmpOp::Ge => 5,
        CmpOp::Le => 6,
    });
    hash_value(&mut h, value);
    h.finish()
}

fn hash_field(h: &mut Fnv64, field: &Field) {
    h.u64(field.parts.len() as u64);
    for p in &field.parts {
        h.str(p);
    }
}

fn hash_value(h: &mut Fnv64, val: &Value) {
    match val {
        Value::String(s) => {
            h.byte(1);
            h.str(s);
        }
        Value::Int(n) => {
            h.byte(2);
            h.i64(*n);
        }
        Value::Float(f) => {
            if f.is_finite()
                && f.fract() == 0.0
                && *f >= (i64::MIN as f64)
                && *f < (i64::MAX as f64)
            {
                h.byte(2);
                h.i64(*f as i64);
            } else {
                h.byte(3);
                let bits = if *f == 0.0 {
                    0u64
                } else if f.is_nan() {
                    0x7ff8_0000_0000_0000
                } else {
                    f.to_bits()
                };
                h.u64(bits);
            }
        }
        Value::Bool(b) => {
            h.byte(4);
            h.byte(u8::from(*b));
        }
        Value::Timestamp(ms) => {
            h.byte(5);
            h.i64(*ms);
        }
        Value::Now => h.byte(6),
        Value::NowMinus(d) => {
            h.byte(7);
            h.i64(d.as_millis());
        }
        Value::Duration(d) => {
            h.byte(8);
            h.i64(d.as_millis());
        }
        Value::Name(n) => {
            h.byte(9);
            h.str(n);
        }
        Value::Param(p) => {
            h.byte(10);
            h.str(p);
        }
    }
}

/// Reproducible, process-independent 64-bit structural fingerprint of a [`Plan`] (`LIN-0551`, `LIN-0560`).
pub fn plan_fingerprint(plan: &Plan) -> u64 {
    let mut h = Fnv64::new();
    hash_plan_node(&mut h, &plan.root);
    h.finish()
}

fn hash_plan_node(h: &mut Fnv64, n: &Node) {
    h.str(n.kind.short_name());
    h.str(n.backend.as_str());
    h.str(n.effect.as_str());
    match &n.kind {
        NodeKind::Reduce {
            effects,
            snapshot,
            embed,
        } => {
            h.u64(effects.len() as u64);
            for e in effects {
                h.str(e.as_str());
            }
            h.str(snapshot);
            h.str(embed);
        }
        NodeKind::Scan { collection, cols } => {
            h.str(collection);
            h.u64(cols.len() as u64);
            for c in cols {
                h.str(c);
            }
        }
        NodeKind::Filter { pred } => h.str(pred),
        NodeKind::Project { fields } | NodeKind::Without { fields } => {
            h.u64(fields.len() as u64);
            for f in fields {
                h.str(f);
            }
        }
        NodeKind::Join { left, fk } => {
            h.byte(u8::from(*left));
            h.str(fk);
        }
        NodeKind::Hop { rel, depth, cap } | NodeKind::Graph { rel, depth, cap } => {
            h.str(rel);
            h.i64(*depth);
            h.i64(*cap);
        }
        NodeKind::Match { start, hops, cap } => {
            h.byte(u8::from(start.is_some()));
            if let Some(s) = start {
                h.str(s);
            }
            h.u64(hops.len() as u64);
            for hop in hops {
                h.str(hop);
            }
            h.i64(*cap);
        }
        NodeKind::Search { mode, query, k } => {
            h.byte(match mode {
                SearchMode::Hybrid => 1,
                SearchMode::Lex => 2,
                SearchMode::Vec => 3,
            });
            h.str(query);
            h.i64(*k);
        }
        NodeKind::Rrf { k } => h.i64(*k),
        NodeKind::Agg { op, by } => {
            h.str(op);
            h.str(by);
        }
        NodeKind::Sort { field, desc } => {
            h.str(field);
            h.byte(u8::from(*desc));
        }
        NodeKind::Window {
            func,
            by,
            order,
            frame,
            as_field,
        } => {
            h.str(func);
            h.u64(by.len() as u64);
            for b in by {
                h.str(b);
            }
            h.u64(order.len() as u64);
            for (f, desc) in order {
                h.str(f);
                h.byte(u8::from(*desc));
            }
            if let Some(fr) = frame {
                h.byte(1);
                h.str(fr);
            } else {
                h.byte(0);
            }
            h.str(as_field);
        }
        NodeKind::Skip { n } => h.i64(*n),
        NodeKind::Take { n, implicit } => {
            h.byte(u8::from(n.is_some()));
            if let Some(v) = n {
                h.i64(*v);
            }
            h.byte(u8::from(*implicit));
        }
        NodeKind::Get { collection, key } => {
            h.str(collection);
            h.str(key);
        }
        NodeKind::Append { kind, detail, n } => {
            h.str(kind);
            h.str(detail);
            h.u64(*n as u64);
        }
        NodeKind::InsertPack {
            collection,
            edges,
            n,
        } => {
            h.str(collection);
            h.u64(edges.len() as u64);
            for e in edges {
                h.str(e);
            }
            h.u64(*n as u64);
        }
        NodeKind::Cas { hash } => h.str(hash),
        NodeKind::BatchCAS { n } => {
            h.byte(u8::from(n.is_some()));
            if let Some(v) = n {
                h.u64(*v as u64);
            }
        }
        NodeKind::IndexSeek { collection, fields } => {
            h.str(collection);
            h.u64(fields.len() as u64);
            for f in fields {
                h.str(f);
            }
        }
        NodeKind::FtsSeek {
            collection,
            fields,
            query,
        } => {
            h.str(collection);
            h.u64(fields.len() as u64);
            for f in fields {
                h.str(f);
            }
            h.str(query);
        }
        NodeKind::Schema { detail } => h.str(detail),
        NodeKind::Reembed { collection, to } => {
            h.str(collection);
            if let Some(t) = to {
                h.byte(1);
                h.str(t);
            } else {
                h.byte(0);
            }
        }
        NodeKind::Delete { collection } => h.str(collection),
        NodeKind::DeleteEdge { rel, from, to } => {
            h.str(rel);
            h.str(from);
            h.str(to);
        }
        NodeKind::Let { name } => h.str(name),
        NodeKind::Union | NodeKind::Seq | NodeKind::Split | NodeKind::Exchange => {}
    }
    h.u64(n.notes.len() as u64);
    for note in &n.notes {
        h.str(note);
    }
    h.u64(n.children.len() as u64);
    for child in &n.children {
        hash_plan_node(h, child);
    }
}

/// Plan a program under explicit [`OptimizerOptions`], returning the [`Plan`], optional
/// [`RewriteTraceEntry`] log, and [`OptimizerDebugCounters`] (`LIN-0533`..`LIN-0536`, `LIN-0550`, `LIN-0552`).
pub fn plan_program_with_opts(
    stmts: &[Stmt],
    cat: &Catalog,
    opts: &OptimizerOptions,
) -> Result<(Plan, Vec<RewriteTraceEntry>, OptimizerDebugCounters), Error> {
    let mut trace = Vec::new();
    let mut counters = OptimizerDebugCounters::default();

    if stmts.len() == 1 {
        let p = plan_stmt_with_opts(&stmts[0], cat, opts, &mut trace, &mut counters)?;
        return Ok((p, trace, counters));
    }

    let mut parts = Vec::new();
    let mut effects = Vec::new();
    let mut explain = ExplainKind::Tree;
    let mut staged: Option<Catalog> = None;

    for stmt in stmts {
        let view = staged.as_ref().unwrap_or(cat);
        if let Stmt::Query(q) = stmt
            && let Some(k) = q.explain
        {
            explain = k;
        }
        let mut p = plan_stmt_with_opts(stmt, view, opts, &mut trace, &mut counters)?;
        if let NodeKind::Reduce { effects: effs, .. } = &p.root.kind {
            for e in effs {
                if !effects.contains(e) {
                    effects.push(*e);
                }
            }
        }
        if p.root.children.len() == 1 {
            parts.push(p.root.children.remove(0));
        } else {
            parts.push(p.root);
        }
        if let Stmt::Decl(d) = stmt {
            let _ = staged.get_or_insert_with(|| cat.clone()).apply_decl(d);
        }
    }
    if effects.is_empty() {
        effects.push(Effect::Read);
    }
    let final_cat = staged.as_ref().unwrap_or(cat);
    let inner = if parts.len() == 1 {
        parts.remove(0)
    } else {
        node(NodeKind::Seq, Backend::Native, Effect::Read, parts)
    };
    Ok((write_plan(final_cat, effects, inner, explain), trace, counters))
}

fn plan_stmt_with_opts(
    stmt: &Stmt,
    cat: &Catalog,
    opts: &OptimizerOptions,
    trace: &mut Vec<RewriteTraceEntry>,
    counters: &mut OptimizerDebugCounters,
) -> Result<Plan, Error> {
    match stmt {
        Stmt::Query(q) => plan_query_with_opts(
            q,
            cat,
            q.explain.unwrap_or(ExplainKind::Tree),
            opts,
            trace,
            counters,
        ),
        Stmt::Let { name, query } => {
            let mut p =
                plan_query_with_opts(query, cat, ExplainKind::Tree, opts, trace, counters)?;
            let inner = p.root.children.pop().unwrap_or(p.root);
            Ok(write_plan(
                cat,
                vec![Effect::Read],
                node(
                    NodeKind::Let { name: name.clone() },
                    Backend::Native,
                    Effect::Read,
                    vec![inner],
                ),
                ExplainKind::Tree,
            ))
        }
        Stmt::IdbSlice { query } => {
            let mut p =
                plan_query_with_opts(query, cat, ExplainKind::Tree, opts, trace, counters)?;
            stamp_backend(&mut p.root, Backend::Idb);
            Ok(p)
        }
        other => plan(other, cat),
    }
}

fn plan_query_with_opts(
    q: &Query,
    cat: &Catalog,
    explain: ExplainKind,
    opts: &OptimizerOptions,
    trace: &mut Vec<RewriteTraceEntry>,
    counters: &mut OptimizerDebugCounters,
) -> Result<Plan, Error> {
    let mut working = crate::catalog::with_catalog_filter(q, cat).into_owned();
    count_volatile_barriers_in_query(&working, counters);

    if opts.enable_optimizations {
        optimize_query_logical(&mut working, opts, trace, counters);
        let cur = plan_query_physical_opts(&working, cat, true, true, opts.record_trace, trace, counters)?;
        let root = reduce(cat, vec![Effect::Read], cur);
        Ok(Plan { root, explain })
    } else {
        if opts.record_trace {
            trace.push(RewriteTraceEntry {
                pass_name: "no_optimization_reference".into(),
                changed: false,
            });
        }
        let cur = plan_query_physical_opts(&working, cat, true, false, opts.record_trace, trace, counters)?;
        let root = reduce(cat, vec![Effect::Read], cur);
        Ok(Plan { root, explain })
    }
}

fn count_volatile_barriers_in_query(q: &Query, counters: &mut OptimizerDebugCounters) {
    for step in &q.steps {
        match step {
            Step::Filter(p) => {
                if has_volatile_barrier(p) {
                    counters.volatile_barriers_preserved += 1;
                }
            }
            Step::Union(rhs) => count_volatile_barriers_in_query(rhs, counters),
            _ => {}
        }
    }
}

fn optimize_query_logical(
    q: &mut Query,
    opts: &OptimizerOptions,
    trace: &mut Vec<RewriteTraceEntry>,
    counters: &mut OptimizerDebugCounters,
) {
    let max_iters = opts.max_iterations;
    for _ in 0..max_iters {
        counters.iterations_run += 1;
        let mut iter_changed = false;

        // Pass 1: Algebraic & Not/In predicate simplification
        counters.passes_evaluated += 1;
        let simp_changed = pass_simplify_predicates(q, counters);
        if simp_changed {
            counters.passes_changed += 1;
            iter_changed = true;
        }
        if opts.record_trace {
            trace.push(RewriteTraceEntry {
                pass_name: "predicate_simplification".into(),
                changed: simp_changed,
            });
        }

        // Pass 2: Adjacent filter merging (respecting volatile barriers)
        counters.passes_evaluated += 1;
        let merge_changed = pass_merge_adjacent_filters(q, counters);
        if merge_changed {
            counters.passes_changed += 1;
            iter_changed = true;
        }
        if opts.record_trace {
            trace.push(RewriteTraceEntry {
                pass_name: "adjacent_filter_merge".into(),
                changed: merge_changed,
            });
        }

        // Pass 3: Common-conjunct deduplication & Or factoring
        counters.passes_evaluated += 1;
        let dedup_changed = pass_deduplicate_conjuncts(q, counters);
        if dedup_changed {
            counters.passes_changed += 1;
            iter_changed = true;
        }
        if opts.record_trace {
            trace.push(RewriteTraceEntry {
                pass_name: "common_conjunct_dedup".into(),
                changed: dedup_changed,
            });
        }

        if !iter_changed {
            break;
        }
    }
}

fn pass_simplify_predicates(q: &mut Query, counters: &mut OptimizerDebugCounters) -> bool {
    let mut changed = false;
    for step in &mut q.steps {
        match step {
            Step::Filter(pred) => {
                if pred.needs_simplify() {
                    let old = pred.clone();
                    let next = old.clone().simplify();
                    if next != old {
                        *pred = next;
                        counters.predicates_simplified += 1;
                        changed = true;
                    }
                }
            }
            Step::Union(rhs) => {
                if pass_simplify_predicates(rhs, counters) {
                    changed = true;
                }
            }
            _ => {}
        }
    }
    changed
}

fn pass_merge_adjacent_filters(q: &mut Query, counters: &mut OptimizerDebugCounters) -> bool {
    let mut changed = false;
    for step in &mut q.steps {
        if let Step::Union(rhs) = step
            && pass_merge_adjacent_filters(rhs, counters)
        {
            changed = true;
        }
    }
    let mut i = 0;
    while i + 1 < q.steps.len() {
        let can_merge = matches!(
            (&q.steps[i], &q.steps[i + 1]),
            (Step::Filter(p1), Step::Filter(p2))
                if !has_volatile_barrier(p1) && !has_volatile_barrier(p2)
        );
        if can_merge {
            let Step::Filter(p2) = q.steps.remove(i + 1) else {
                unreachable!()
            };
            if let Step::Filter(p1) = &mut q.steps[i] {
                let left = std::mem::replace(
                    p1,
                    Pred::IsNull {
                        field: Field { parts: Vec::new() },
                        negated: false,
                    },
                );
                *p1 = Pred::And(Box::new(left), Box::new(p2));
                counters.filters_merged += 1;
                changed = true;
                continue;
            }
        }
        i += 1;
    }
    changed
}

fn pass_deduplicate_conjuncts(q: &mut Query, counters: &mut OptimizerDebugCounters) -> bool {
    let mut changed = false;
    for step in &mut q.steps {
        match step {
            Step::Filter(pred) => {
                let (rewritten, rem) = deduplicate_conjuncts(pred);
                if rem > 0 && &rewritten != pred {
                    *pred = rewritten;
                    counters.conjuncts_deduplicated += rem;
                    changed = true;
                }
            }
            Step::Union(rhs) => {
                if pass_deduplicate_conjuncts(rhs, counters) {
                    changed = true;
                }
            }
            _ => {}
        }
    }
    changed
}

fn plan_query_physical_opts(
    q: &Query,
    cat: &Catalog,
    allow_implicit_take: bool,
    enable_access_optimizations: bool,
    record_trace: bool,
    trace: &mut Vec<RewriteTraceEntry>,
    counters: &mut OptimizerDebugCounters,
) -> Result<Node, Error> {
    if enable_access_optimizations {
        // Inspect DNF clauses via bounded lazy iterator for diagnostics & physical access selection
        for step in &q.steps {
            if let Step::Filter(pred) = step {
                let (clauses, capped) = stream_dnf_bounded(pred, MAX_DNF_CLAUSES);
                counters.dnf_clauses_inspected += clauses.len();
                if capped {
                    counters.dnf_caps_hit += 1;
                }
            }
        }
        let built = plan_query_inner(q, cat, allow_implicit_take)?;
        let (gets, idx_seeks) = count_access_paths(&built);
        counters.point_gets_selected += gets;
        counters.index_seeks_selected += idx_seeks;
        counters.passes_evaluated += 1;
        let access_changed = gets > 0 || idx_seeks > 0;
        if access_changed {
            counters.passes_changed += 1;
        }
        if record_trace {
            trace.push(RewriteTraceEntry {
                pass_name: "access_path_selection".into(),
                changed: access_changed,
            });
        }
        return Ok(built);
    }

    // Reference (`enable_optimizations = false`) physical lowering:
    // Full collection column scan + verbatim sequential operators without IndexSeek / PointGet / FtsSeek.
    let collection = check::collection_of(&q.source);
    let full_cols = match cat.collection(collection) {
        Some(col) => col.fields.keys().cloned().collect(),
        None => vec!["id".into()],
    };
    let mut implicit_take = true;
    let mut search_k = 50i64;
    let mut saw_agg = false;
    for step in &q.steps {
        match step {
            Step::Take { n } => {
                implicit_take = false;
                if !saw_agg && let Some(v) = n {
                    search_k = *v;
                }
            }
            Step::Count { .. }
            | Step::Sum { .. }
            | Step::Min { .. }
            | Step::Max { .. }
            | Step::Avg { .. }
            | Step::Summarize { .. }
            | Step::ArgExtremum { .. } => {
                implicit_take = false;
                saw_agg = true;
            }
            _ => {}
        }
    }

    let mut cur = match &q.source {
        Source::Page(uri) => {
            let scan = node(
                NodeKind::Scan {
                    collection: "docs".into(),
                    cols: full_cols,
                },
                Backend::Native,
                Effect::Read,
                vec![],
            );
            node(
                NodeKind::Filter {
                    pred: format!("uri == {uri:?}"),
                },
                Backend::Native,
                Effect::Read,
                vec![scan],
            )
        }
        Source::Catalog => node(
            NodeKind::Scan {
                collection: "catalog".into(),
                cols: full_cols,
            },
            Backend::Native,
            Effect::Read,
            vec![],
        ),
        Source::Collection(name) => node(
            NodeKind::Scan {
                collection: name.clone(),
                cols: full_cols,
            },
            Backend::Native,
            Effect::Read,
            vec![],
        ),
    };

    for step in &q.steps {
        match step {
            Step::Filter(pred) => {
                cur = filter_node(cur, pred);
            }
            Step::Project(fields) => {
                cur = node(
                    NodeKind::Project {
                        fields: fields.iter().map(Field::as_str).collect(),
                    },
                    Backend::Native,
                    Effect::Read,
                    vec![cur],
                );
            }
            Step::Without(fields) => {
                cur = node(
                    NodeKind::Without {
                        fields: fields.iter().map(Field::as_str).collect(),
                    },
                    Backend::Native,
                    Effect::Read,
                    vec![cur],
                );
            }
            Step::Join {
                left,
                collection: right,
                on,
            } => {
                let fk = cat
                    .find_fk(collection, on, right)
                    .map(|f| format!("{}.{}→{}.{}", f.from_col, f.from_field, f.to_col, f.to_field))
                    .unwrap_or_else(|| format!("{on}→{right}"));
                let right_cols = cat
                    .collection(right)
                    .map(|c| c.fields.keys().cloned().collect())
                    .unwrap_or_else(|| vec!["id".into()]);
                let right_scan = node(
                    NodeKind::Scan {
                        collection: right.clone(),
                        cols: right_cols,
                    },
                    Backend::Native,
                    Effect::Read,
                    vec![],
                );
                cur = node(
                    NodeKind::Join { left: *left, fk },
                    Backend::Native,
                    Effect::Read,
                    vec![cur, right_scan],
                );
            }
            Step::Hop { rel, depth } => {
                cur = node(
                    NodeKind::Hop {
                        rel: rel.clone(),
                        depth: depth.unwrap_or(1),
                        cap: 300,
                    },
                    Backend::Native,
                    Effect::Read,
                    vec![cur],
                );
            }
            Step::Graph { rel, depth } => {
                cur = node(
                    NodeKind::Graph {
                        rel: rel.clone(),
                        depth: depth.unwrap_or(1),
                        cap: 300,
                    },
                    Backend::Native,
                    Effect::Read,
                    vec![cur],
                );
            }
            Step::Match { start, hops } => {
                cur = node(
                    NodeKind::Match {
                        start: start.clone(),
                        hops: hops.iter().map(MatchHop::label).collect(),
                        cap: 300,
                    },
                    Backend::Native,
                    Effect::Read,
                    vec![cur],
                );
            }
            Step::Search { mode, query } => {
                cur = node(
                    NodeKind::Search {
                        mode: *mode,
                        query: query.clone(),
                        k: search_k,
                    },
                    Backend::Native,
                    Effect::Read,
                    vec![cur],
                );
            }
            Step::Count { by } => {
                cur = node(
                    NodeKind::Agg {
                        op: "count".into(),
                        by: by.as_ref().map(Field::as_str).unwrap_or_else(|| "*".into()),
                    },
                    Backend::Native,
                    Effect::Read,
                    vec![cur],
                );
            }
            Step::Sum { field, by } => {
                cur = node(
                    NodeKind::Agg {
                        op: format!("sum({})", field.as_str()),
                        by: by.as_ref().map(Field::as_str).unwrap_or_else(|| "*".into()),
                    },
                    Backend::Native,
                    Effect::Read,
                    vec![cur],
                );
            }
            Step::Min { field, by } => {
                cur = node(
                    NodeKind::Agg {
                        op: format!("min({})", field.as_str()),
                        by: by.as_ref().map(Field::as_str).unwrap_or_else(|| "*".into()),
                    },
                    Backend::Native,
                    Effect::Read,
                    vec![cur],
                );
            }
            Step::Max { field, by } => {
                cur = node(
                    NodeKind::Agg {
                        op: format!("max({})", field.as_str()),
                        by: by.as_ref().map(Field::as_str).unwrap_or_else(|| "*".into()),
                    },
                    Backend::Native,
                    Effect::Read,
                    vec![cur],
                );
            }
            Step::Avg { field, by } => {
                cur = node(
                    NodeKind::Agg {
                        op: format!("avg({})", field.as_str()),
                        by: by.as_ref().map(Field::as_str).unwrap_or_else(|| "*".into()),
                    },
                    Backend::Native,
                    Effect::Read,
                    vec![cur],
                );
            }
            Step::Summarize { specs, by } => {
                cur = node(
                    NodeKind::Agg {
                        op: fmt_summarize_op(specs),
                        by: fmt_group_keys(by),
                    },
                    Backend::Native,
                    Effect::Read,
                    vec![cur],
                );
            }
            Step::ArgExtremum { is_max, order, by } => {
                let name = if *is_max { "arg_max" } else { "arg_min" };
                cur = node(
                    NodeKind::Agg {
                        op: format!("{name}({})", order.as_str()),
                        by: fmt_group_keys(by),
                    },
                    Backend::Native,
                    Effect::Read,
                    vec![cur],
                );
            }
            Step::Sort { field, desc } => {
                cur = node(
                    NodeKind::Sort {
                        field: field.as_str(),
                        desc: *desc,
                    },
                    Backend::Native,
                    Effect::Read,
                    vec![cur],
                );
            }
            Step::Window {
                func,
                field,
                by,
                order,
                frame,
                as_field,
            } => {
                let func_str = match field {
                    Some(f) => format!("{func}({})", f.as_str()),
                    None => format!("{func}"),
                };
                cur = node(
                    NodeKind::Window {
                        func: func_str,
                        by: by.iter().map(Field::as_str).collect(),
                        order: order.iter().map(|s| (s.field.as_str(), s.desc)).collect(),
                        frame: frame.map(|fr| fr.to_string()),
                        as_field: as_field.clone(),
                    },
                    Backend::Native,
                    Effect::Read,
                    vec![cur],
                );
            }
            Step::Skip { n } => {
                cur = node(
                    NodeKind::Skip { n: *n },
                    Backend::Native,
                    Effect::Read,
                    vec![cur],
                );
            }
            Step::Take { n } => {
                cur = node(
                    NodeKind::Take {
                        n: *n,
                        implicit: false,
                    },
                    Backend::Native,
                    Effect::Read,
                    vec![cur],
                );
            }
            Step::Union(rhs) => {
                let right = plan_query_physical_opts(rhs, cat, false, false, record_trace, trace, counters)?;
                cur = node(
                    NodeKind::Union,
                    Backend::Native,
                    Effect::Read,
                    vec![cur, right],
                );
            }
        }
    }

    if allow_implicit_take && implicit_take {
        cur = node(
            NodeKind::Take {
                n: Some(50),
                implicit: true,
            },
            Backend::Native,
            Effect::Read,
            vec![cur],
        );
    }

    Ok(cur)
}

fn count_access_paths(node: &Node) -> (usize, usize) {
    let mut gets = usize::from(matches!(node.kind, NodeKind::Get { .. }));
    let mut seeks = usize::from(matches!(
        node.kind,
        NodeKind::IndexSeek { .. } | NodeKind::FtsSeek { .. }
    ));
    for c in &node.children {
        let (cg, cs) = count_access_paths(c);
        gets += cg;
        seeks += cs;
    }
    (gets, seeks)
}

/// Execute a read query under `OptimizerOptions` (`LIN-0549`, `LIN-0550`):
/// - When `opts.enable_optimizations == true`, executes against `db` with all active indexes and rewrites.
/// - When `opts.enable_optimizations == false`, executes against an index-free reference snapshot
///   without logical rewrites or secondary index seeks, returning `(rows, plan)` for differential verification.
pub fn execute_with_optimizer_opts(
    db: &mut crate::Db,
    src: &str,
    opts: &OptimizerOptions,
) -> Result<(Vec<crate::Row>, Plan), Error> {
    let mut stmts = crate::parse::parse_program(src)?;
    let (_, staged) = check::check_program(&stmts, &db.catalog)?;
    let view = staged.as_ref().unwrap_or(&db.catalog);
    let (plan, _, _) = plan_program_with_opts(&stmts, view, opts)?;

    if opts.enable_optimizations {
        // Run with rewritten AST against full indexed reader snapshot
        for stmt in &mut stmts {
            if let Stmt::Query(q) = stmt {
                let mut tr = Vec::new();
                let mut cnt = OptimizerDebugCounters::default();
                optimize_query_logical(q, opts, &mut tr, &mut cnt);
            }
        }
        let handle = db.reader().run(&crate::parse::format_program_canonical(&stmts))?;
        return Ok((handle.into_rows(), plan));
    }

    // Unoptimized reference execution: clone catalog & store without secondary scalar indexes
    let mut ref_db = crate::Db::empty();
    let mut ref_cat = db.catalog.clone();
    ref_cat.indexes.clear();
    ref_cat.refresh_hash();
    ref_db.catalog = ref_cat;
    ref_db.store = db.store.clone_mem();
    ref_db.store.indexes.clear();
    let handle = ref_db.run(src)?;
    Ok((handle.into_rows(), plan))
}
