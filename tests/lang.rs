fn ok(src: &str) -> String {
    lin::explain(src).unwrap_or_else(|e| panic!("compile failed for `{src}`: {e}"))
}

fn err(src: &str) -> String {
    match lin::compile(src) {
        Ok(_) => panic!("expected error for `{src}`"),
        Err(e) => e.to_string(),
    }
}

#[test]
fn laconic_filter_project_ago() {
    let plan = ok(r#"docs | wing == "rag" and ts > ago 7d | { id, title, room }"#);
    assert!(plan.contains("Reduce effect=Read snapshot=fixture embed=nomic-embed-text/768"));
    assert!(plan.contains("Take 50 implicit"));
    assert!(plan.contains("Project [id, title, room]"));
    assert!(plan.contains("ts > now - 7d"));
    assert!(plan.contains("wing == \"rag\""));
    assert!(plan.contains("Scan docs"));
    assert!(!plan.contains("body"));
}

#[test]
fn ago_equals_now_minus() {
    let a = ok(r#"docs | ts > ago 7d | { id }"#);
    let b = ok(r#"docs | ts > now - 7d | { id }"#);
    let c = ok(r#"docs | ts > ago(7d) | { id }"#);
    assert_eq!(a, b);
    assert_eq!(a, c);
    assert!(a.contains("ts > now - 7d"));
    assert!(!a.contains("ago"));
}

#[test]
fn point_get_by_id() {
    let plan = ok(r#"docs | id == "e7c98d54-b4d6-4165-86e9-9b999e7ce9c3""#);
    assert!(plan.contains("Get docs id=\"e7c98d54-b4d6-4165-86e9-9b999e7ce9c3\" backend=idb"));
    assert!(plan.contains("Take 50 implicit"));
}

#[test]
fn join_fk() {
    let plan = ok(r#"orders | total > 100 | join users on user_id | { id, users.email, total }"#);
    assert!(plan.contains("Join inner fk=orders.user_id→users.id"));
    assert!(plan.contains("Project [id, users.email, total]"));
    assert!(plan.contains("Filter total > 100"));
    assert!(plan.contains("Scan users"));
}

#[test]
fn hop_wikilink() {
    let plan =
        ok(r#"docs | id == "e7c98d54-b4d6-4165-86e9-9b999e7ce9c3" | hop wikilink | { id, title }"#);
    assert!(plan.contains("Hop wikilink depth=1 cap=300"));
    assert!(plan.contains("Get docs id=\"e7c98d54-b4d6-4165-86e9-9b999e7ce9c3\""));
    assert!(plan.contains("Project [id, title]"));
}

#[test]
fn search_hybrid_then_take() {
    let plan = ok(r#"docs | wing == "rag" | search "embedding identity" | take 20"#);
    assert!(plan.contains("RRF k=20"));
    assert!(plan.contains("Search lex \"embedding identity\""));
    assert!(plan.contains("Search vec \"embedding identity\""));
    assert!(plan.contains("Take 20"));
    assert!(!plan.contains("Take 20 implicit"));
    assert!(plan.contains("embed=nomic-embed-text/768"));
}

#[test]
fn search_count_sort() {
    let plan = ok(r#"docs | search "wal" | count by room | sort hits desc | take 50"#);
    assert!(plan.contains("Agg count by room"));
    assert!(plan.contains("Sort hits desc"));
    assert!(plan.contains("Take 50"));
    assert!(plan.contains("RRF"));
}

#[test]
fn append_facts() {
    let plan = ok(r#"append facts { s: "lin", p: tagged, o: "db" }"#);
    assert!(plan.contains("Reduce effect=Append"));
    assert!(plan.contains("Append facts"));
    assert!(plan.contains("p: tagged"));
}

#[test]
fn insert_with_edge() {
    let plan = ok(
        r#"insert docs { uri: "raw://n/wal", title: "WAL", layer: "raw", body: "x" } with edge wikilink -> page "wiki://rag-overview""#,
    );
    assert!(plan.contains("effect=Create+Body+HopBuild"));
    assert!(plan.contains("InsertPack docs"));
    assert!(plan.contains("wikilink -> page"));
}

#[test]
fn update_cas() {
    let plan = ok(
        r#"update docs[id == "e7c98d54-b4d6-4165-86e9-9b999e7ce9c3"] cas "sha256:ab" { room: "inbox" }"#,
    );
    assert!(plan.contains("effect=Meta"));
    assert!(plan.contains("CAS hash=\"sha256:ab\""));
    assert!(plan.contains("Get docs id="));
}

#[test]
fn explain_cost_search() {
    let plan = ok(r#"docs | wing == "rag" | search "wal" | { id, title } | explain cost"#);
    assert!(plan.contains("budget.take=50"));
    assert!(plan.contains("zone skip"));
    assert!(plan.contains("RRF"));
    assert!(plan.contains("Project [id, title]"));
    assert!(plan.contains("-- no body"));
}

#[test]
fn title_has_and_regex_and_substr() {
    let has = ok(r#"docs | title has "WAL" | { id }"#);
    assert!(has.contains("title has \"WAL\""));

    let re = ok(r#"docs | title ~ /wal.*/i | { id }"#);
    assert!(re.contains("title ~ /wal.*/i"));

    let sub = ok(r#"docs | title ~ "wal" | { id }"#);
    assert!(sub.contains("title ~ \"wal\""));
    assert!(!sub.contains("title ~ /"));
}

#[test]
fn required_combo_explain() {
    let plan = ok(r#"docs | wing == "rag" and ts > ago 7d | title has "wal" | { id, title }"#);
    assert!(plan.contains("ts > now - 7d"));
    assert!(plan.contains("title has \"wal\""));
    assert!(plan.contains("Project [id, title]"));
    assert!(plan.contains("Take 50 implicit"));
}

#[test]
fn verbose_where_pick_alias() {
    let laconic = ok(r#"docs | wing == "rag" | { id, title }"#);
    let verbose = ok(r#"docs | where wing == "rag" | pick id, title"#);
    assert_eq!(laconic, verbose);
}

#[test]
fn page_and_catalog_and_reembed() {
    let page = ok(r#"page "wiki://rag-overview" | hop backlink depth=2"#);
    assert!(page.contains("Get docs uri=\"wiki://rag-overview\""));
    assert!(page.contains("Hop backlink depth=2 cap=300"));

    let cat = ok(r#"catalog | kind == "col""#);
    assert!(cat.contains("Scan catalog"));

    let re = ok("reembed docs to ollama/nomic-embed-text/768");
    assert!(re.contains("Reembed docs to ollama/nomic-embed-text/768"));
    assert!(re.contains("effect=Embed"));
}

#[test]
fn decls_and_idb_parse() {
    ok("rel extra stub");
    ok("fk orders.user_id -> users.id");
    ok(r#"guard docs.body immutable when layer == "raw""#);
    ok(r#"idb slice docs | layer == "wiki" | take 2000"#);
    ok("pull idb since 1842 take 2000");
    ok("push idb");
    ok(r#"snapshot "before-reembed""#);
}

#[test]
fn unknown_field() {
    let e = err(r#"docs | wign == "rag""#);
    assert!(e.contains("unknown field: wign"), "{e}");
}

#[test]
fn hop_unknown_rel() {
    let e = err("docs | hop missing");
    assert!(e.contains("unknown rel: missing"), "{e}");
}

#[test]
fn join_without_fk() {
    let e = err("docs | join users on id");
    assert!(e.contains("join requires fk"), "{e}");
}

#[test]
fn update_without_cas() {
    let e = err(r#"update docs[id == "e7c98d54-b4d6-4165-86e9-9b999e7ce9c3"] { room: "inbox" }"#);
    assert!(e.contains("update requires cas"), "{e}");
}

#[test]
fn update_body_immutable() {
    let e = err(
        r#"update docs[id == "e7c98d54-b4d6-4165-86e9-9b999e7ce9c3"] cas "sha256:ab" { body: "nope" }"#,
    );
    assert!(e.contains("immutable field: docs.body"), "{e}");
}

#[test]
fn mutation_on_pipe() {
    let e = err(
        r#"docs | search "x" | update docs[id == "e7c98d54-b4d6-4165-86e9-9b999e7ce9c3"] cas "sha256:ab" { room: "inbox" }"#,
    );
    assert!(e.contains("mutation on pipe"), "{e}");
}

#[test]
fn search_vec_on_users() {
    let e = err(r#"users | search vec "x""#);
    assert!(e.contains("search vec requires embedding: users"), "{e}");
}

#[test]
fn ago_rejects_unitless_and_string() {
    let e = err(r#"docs | ts > ago 7"#);
    assert!(e.contains("ago requires a duration with unit"), "{e}");
    let e = err(r#"docs | ts > ago("7d")"#);
    assert!(e.contains("ago requires a duration"), "{e}");
}

#[test]
fn regex_errors_and_has_type() {
    let e = err(r#"docs | title ~ /[unterminated"#);
    assert!(e.contains("unterminated regex literal"), "{e}");

    let e = err(r#"docs | title ~ /[a-z/"#);
    assert!(e.contains("invalid regex literal"), "{e}");

    let e = err(r#"orders | total has "x""#);
    assert!(e.contains("has requires text"), "{e}");
    assert!(e.contains("f64"), "{e}");
}

#[test]
fn bare_predicate_without_keyword_is_where() {
    let plan = ok(r#"docs | title has "wal""#);
    assert!(plan.contains("Filter title has \"wal\""));
}

#[test]
fn explain_graph_mermaid_search_filter() {
    let g = ok(r#"docs | wing == "rag" | search "wal" | { id, title } | explain graph"#);
    assert!(g.contains("flowchart TD"), "{g}");
    assert!(g.contains("Search lex"), "{g}");
    assert!(g.contains("Filter"), "{g}");
    assert!(g.contains("RRF") || g.contains("Search"), "{g}");
    assert!(g.contains("Take 50"), "{g}");
    assert!(g.contains("classDef read"), "{g}");
    assert!(g.contains("classDef reduce"), "{g}");
}

#[test]
fn explain_graph_dot_search_filter() {
    let g = ok(r#"docs | wing == "rag" | search "wal" | { id, title } | explain dot"#);
    assert!(g.contains("digraph"), "{g}");
    assert!(g.contains("Search lex"), "{g}");
    assert!(g.contains("Filter"), "{g}");
    assert!(g.contains("Take 50"), "{g}");
}

#[test]
fn explain_cost_still_has_budget() {
    let plan = ok(r#"docs | wing == "rag" | search "wal" | { id, title } | explain cost"#);
    assert!(plan.contains("budget.take=50"), "{plan}");
    assert!(plan.contains("est.rows="), "{plan}");
    assert!(plan.contains("zone skip"), "{plan}");
}

#[test]
fn delete_keyed_cas_plan() {
    let plan = ok(r#"delete docs[id == "e7c98d54-b4d6-4165-86e9-9b999e7ce9c3"] cas "sha256:ab""#);
    assert!(plan.contains("Delete docs"), "{plan}");
    assert!(plan.contains("CAS hash=\"sha256:ab\""), "{plan}");
    assert!(plan.contains("effect=Meta"), "{plan}");
}

#[test]
fn delete_edge_and_facts() {
    let edge = ok(r#"delete edge wikilink "a" -> "b""#);
    assert!(edge.contains("DeleteEdge wikilink"), "{edge}");

    let facts = ok(r#"delete facts[s == "lin" and p == tagged and o == "db"]"#);
    assert!(facts.contains("Delete facts"), "{facts}");
    assert!(!facts.contains("CAS"), "{facts}");
}

#[test]
fn delete_keyed_requires_cas() {
    let e = err(r#"delete docs[id == "e7c98d54-b4d6-4165-86e9-9b999e7ce9c3"]"#);
    assert!(e.contains("delete requires cas"), "{e}");
}

#[test]
fn delete_on_pipe_is_mutation() {
    let e =
        err(r#"docs | delete docs[id == "e7c98d54-b4d6-4165-86e9-9b999e7ce9c3"] cas "sha256:ab""#);
    assert!(e.contains("mutation on pipe"), "{e}");
}

#[test]
fn union_compatible_plan() {
    let plan = ok(r#"docs | { id } | union orders | { id }"#);
    assert!(plan.contains("Union"), "{plan}");
    assert!(plan.contains("Scan docs"), "{plan}");
    assert!(plan.contains("Scan orders"), "{plan}");
    assert!(plan.contains("Project [id]"), "{plan}");
    assert!(plan.contains("Take 50 implicit"), "{plan}");
}

#[test]
fn union_paren_and_mismatch() {
    let plan = ok(r#"docs | { id } | union (orders | { id })"#);
    assert!(plan.contains("Union"), "{plan}");

    let e = err(r#"docs | wing == "rag" | union (orders | total > 10 | { id })"#);
    assert!(e.contains("union columns mismatch"), "{e}");
}

#[test]
fn let_binding_plan() {
    let plan = ok(r#"let x = docs | wing == "rag" | { id }
x"#);
    assert!(plan.contains("Let x") || plan.contains("Scan x"), "{plan}");
    assert!(plan.contains("Seq") || plan.contains("Scan x"), "{plan}");
}

#[test]
fn live_col_then_query() {
    let plan = ok(r#"col notes { title: text }
notes | { title }"#);
    assert!(
        plan.contains("Schema col notes") || plan.contains("Scan notes"),
        "{plan}"
    );
}

#[test]
fn let_cannot_shadow_collection() {
    let e = err(r#"let docs = orders | { id }"#);
    assert!(e.contains("cannot bind over collection: docs"), "{e}");
}

#[test]
fn bulk_insert_plan() {
    let plan = ok(r#"insert docs [
      { uri: "raw://a", title: "A", layer: "wiki" },
      { uri: "raw://b", title: "B", layer: "wiki" }
    ]"#);
    assert!(plan.contains("InsertPack docs n=2"), "{plan}");
}

#[test]
fn bulk_append_plan() {
    let facts = ok(r#"append facts [
      { s: "a", p: tagged, o: "x" },
      { s: "b", p: tagged, o: "y" }
    ]"#);
    assert!(facts.contains("Append facts n=2"), "{facts}");

    let edges = ok(r#"append edges [
      wikilink "a" -> "b",
      wikilink "b" -> "c"
    ]"#);
    assert!(edges.contains("Append edges n=2"), "{edges}");
}

#[test]
fn cas_each_plan() {
    let u = ok(r#"update docs[wing == "rag"] cas each { room: "inbox" }"#);
    assert!(u.contains("BatchCAS n=?"), "{u}");
    let d = ok(r#"delete docs[wing == "old"] cas each"#);
    assert!(d.contains("BatchCAS n=?"), "{d}");
}

#[test]
fn empty_list_is_error() {
    let e = err("insert docs []");
    assert!(e.contains("empty list"), "{e}");
    let e = err("append facts []");
    assert!(e.contains("empty list"), "{e}");
    let e = err("append edges []");
    assert!(e.contains("empty list"), "{e}");
}

#[test]
fn bulk_insert_with_forbidden() {
    let e = err(r#"insert docs [
      { uri: "raw://a", title: "A", layer: "wiki" }
    ] with edge wikilink -> page "wiki://rag-overview""#);
    assert!(e.contains("with not allowed on bulk insert"), "{e}");
}

#[test]
fn index_wing_ts_uses_composite() {
    let plan = ok(r#"index docs [wing, ts]
docs | wing == "rag" and ts > ago 7d | { id, title, room }"#);
    assert!(plan.contains("index=docs[wing,ts]"), "{plan}");
    assert!(!plan.contains("Scan docs"), "{plan}");
}

#[test]
fn index_prefix_only_uses_index() {
    let plan = ok(r#"index docs [wing, ts]
docs | wing == "rag" | { id }"#);
    assert!(plan.contains("index=docs[wing,ts]"), "{plan}");
}

#[test]
fn index_ts_only_does_not_claim_composite() {
    let plan = ok(r#"index docs [wing, ts]
docs | ts > ago 7d | { id }"#);
    assert!(!plan.contains("index=docs[wing,ts]"), "{plan}");
    assert!(plan.contains("Scan docs"), "{plan}");
}

#[test]
fn index_point_get_still() {
    let plan = ok(r#"index docs [wing, ts]
docs | id == "e7c98d54-b4d6-4165-86e9-9b999e7ce9c3""#);
    assert!(plan.contains("Get docs id="), "{plan}");
    assert!(!plan.contains("index=docs[wing,ts]"), "{plan}");
}
