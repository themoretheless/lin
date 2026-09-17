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
            if records
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
                effects.clone(),
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
    plan_query_inner(q, cat, implicit_take)
}

fn plan_query(q: &Query, cat: &Catalog, explain: ExplainKind) -> Result<Plan, Error> {
    let q = crate::catalog::with_catalog_filter(q, cat);
    let cur = plan_query_inner(&q, cat, true)?;
    let root = reduce(cat, vec![Effect::Read], cur);
    Ok(Plan { root, explain })
}

fn plan_query_inner(q: &Query, cat: &Catalog, allow_implicit_take: bool) -> Result<Node, Error> {
    let collection = check::collection_of(&q.source).to_string();
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
                    push_field(&mut needed, &by.as_str());
                }
            }
            Step::Sum { field, by } => {
                implicit_take = false;
                saw_agg = true;
                push_field(&mut needed, &field.as_str());
                push_field(&mut needed, &by.as_str());
            }
            Step::Search { mode, .. } => {
                want_search = true;
                if matches!(mode, SearchMode::Vec | SearchMode::Hybrid) {
                    want_vec = true;
                }
            }
            Step::Project(fields) => {
                for f in fields {
                    push_field(&mut picked, &f.as_str());
                    push_field(&mut needed, &f.as_str());
                }
            }
            Step::Filter(pred) => collect_pred_fields(pred, &mut needed),
            Step::Join { on, .. } => push_field(&mut needed, on),
            Step::Sort { field, .. } => push_field(&mut needed, &field.as_str()),
            Step::Hop { .. } => {}
            Step::Graph { .. } => {}
            Step::Match { .. } => {}
            Step::Union(_) => {}
        }
    }

    let cols = scan_cols(cat, &collection, &needed, &picked, want_search, want_vec);

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
                cols: cols.clone(),
            },
            Backend::Native,
            Effect::Read,
            vec![],
        ),
        Source::Collection(name) => node(
            NodeKind::Scan {
                collection: name.clone(),
                cols: cols.clone(),
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
            Step::Join {
                left,
                collection: right,
                on,
            } => {
                let fk = cat
                    .find_fk(&collection, on, right)
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
                        by: by.as_str(),
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
    let (point, rest) = extract_point(pred);
    let mut out = cur;
    if let Some((field, value)) = point.as_ref()
        && let NodeKind::Scan { collection, .. } = &out.kind
    {
        out = node(
            NodeKind::Get {
                collection: collection.clone(),
                key: format!("{field}={}", fmt_value(value)),
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
    if let Some(rest) = rest.as_ref()
        && let NodeKind::Filter { pred: existing } = &out.kind
    {
        let merged = format!("{existing} and {}", fmt_pred(rest));
        out.kind = NodeKind::Filter { pred: merged };
        return out;
    } else if let Some(rest) = rest {
        out = filter_node(out, &rest);
    } else if point.is_none()
        && let NodeKind::Filter { pred: existing } = &out.kind
    {
        let merged = format!("{existing} and {}", fmt_pred(pred));
        out.kind = NodeKind::Filter { pred: merged };
        return out;
    } else if point.is_none() {
        out = filter_node(out, pred);
    }
    let _ = cat;
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

fn extract_point(pred: &Pred) -> (Option<(String, Value)>, Option<Pred>) {
    match pred {
        Pred::Cmp {
            field,
            op: CmpOp::Eq,
            value,
        } if field.parts.len() == 1
            && matches!(field.parts[0].as_str(), "id" | "uri")
            && matches!(value, Value::String(_)) =>
        {
            (Some((field.as_str(), value.clone())), None)
        }
        Pred::And(a, b) => {
            let (pa, ra) = extract_point(a);
            if pa.is_some() {
                let rest = Some(match ra {
                    Some(r) => r.and((**b).clone()),
                    None => (**b).clone(),
                });
                return (pa, rest);
            }
            let (pb, rb) = extract_point(b);
            if pb.is_some() {
                let rest = Some(match rb {
                    Some(r) => (**a).clone().and(r),
                    None => (**a).clone(),
                });
                return (pb, rest);
            }
            (None, Some(pred.clone()))
        }
        _ => (None, Some(pred.clone())),
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
    let mut fields = Vec::new();
    pred.walk_fields(&mut fields);
    for f in fields {
        push_field(needed, &f.as_str());
    }
}

fn push_field(out: &mut Vec<String>, name: &str) {
    if !out.iter().any(|x| x == name) {
        out.push(name.to_string());
    }
}

fn point_or_scan(collection: &str, pred: Option<&Pred>, cat: &Catalog) -> Node {
    if let Some(p) = pred
        && let (point, rest) = extract_point(p)
        && let Some((field, value)) = point
    {
        let mut n = node(
            NodeKind::Get {
                collection: collection.to_string(),
                key: format!("{field}={}", fmt_value(&value)),
            },
            Backend::Idb,
            Effect::Read,
            vec![],
        );
        if let Some(rest) = rest {
            n = filter_node(n, &rest);
        }
        return n;
    }
    if let Some(p) = pred
        && let Some(uses) = crate::index::pick_index(cat, collection, p)
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
        } => {
            if *unique {
                format!("index {collection} unique [{}]", fields.join(", "))
            } else {
                format!("index {collection} [{}]", fields.join(", "))
            }
        }
    }
}

pub fn fmt_pred(pred: &Pred) -> String {
    match pred {
        Pred::And(a, b) => format!("{} and {}", fmt_pred(a), fmt_pred(b)),
        Pred::Or(a, b) => format!("{} or {}", fmt_pred(a), fmt_pred(b)),
        Pred::Cmp { field, op, value } => {
            format!("{} {} {}", field.as_str(), op.as_str(), fmt_value(value))
        }
        Pred::Has { field, ci, needle } => {
            if *ci {
                format!("{} has ci {:?}", field.as_str(), needle)
            } else {
                format!("{} has {:?}", field.as_str(), needle)
            }
        }
        Pred::Contains { field, needle } => {
            format!("{} ~ {:?}", field.as_str(), needle)
        }
        Pred::Regex {
            field,
            pattern,
            flags,
        } => format!("{} ~ /{pattern}/{flags}", field.as_str()),
    }
}

pub fn fmt_value(v: &Value) -> String {
    match v {
        Value::String(s) => format!("{s:?}"),
        Value::Int(n) => n.to_string(),
        Value::Float(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Now => "now".into(),
        Value::NowMinus(d) => format!("now - {}", d.display()),
        Value::Duration(d) => d.display(),
        Value::Name(n) => n.clone(),
    }
}
