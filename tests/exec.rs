use lin::Db;

fn text<'a>(row: &'a lin::Row, k: &str) -> &'a str {
    row.get(k).and_then(|c| c.text()).unwrap_or("")
}

fn ids(h: &lin::Handle) -> Vec<String> {
    h.rows.iter().map(|r| text(r, "id").to_string()).collect()
}

#[test]
fn prepare_once_run_many() {
    let mut db = Db::fixture();
    let id = "e7c98d54-b4d6-4165-86e9-9b999e7ce9c3";
    let q = format!(r#"docs | id == "{id}" | {{ id, title }}"#);
    let prep = db.prepare(&q).unwrap();
    let a = prep.run(&mut db).unwrap();
    let b = db.run_prepared(&prep).unwrap();
    assert_eq!(a.done.n, 1);
    assert_eq!(b.done.n, 1);
    assert_eq!(text(&a.rows[0], "title"), text(&b.rows[0], "title"));
    // Cached path via run(&str)
    let c = db.run(&q).unwrap();
    assert_eq!(c.done.n, 1);
}

#[test]
fn insert_then_query() {
    let mut db = Db::fixture();
    let ins = db
        .run(r#"insert docs { uri: "raw://n/new", title: "fresh wal", layer: "wiki", body: "hello body" }"#)
        .unwrap();
    assert_eq!(ins.done.n, 1);
    assert!(ins.done.r#gen >= 1);
    let id = text(&ins.rows[0], "id");
    assert!(!id.is_empty());
    assert!(text(&ins.rows[0], "hash").starts_with("h:"));

    let q = db
        .run(&format!(r#"docs | id == "{id}" | {{ id, title }}"#))
        .unwrap();
    assert_eq!(q.done.n, 1);
    assert_eq!(text(&q.rows[0], "title"), "fresh wal");
}

#[test]
fn cas_reject() {
    let mut db = Db::fixture();
    let ins = db
        .run(r#"insert docs { uri: "raw://n/cas", title: "cas", layer: "wiki", body: "x" }"#)
        .unwrap();
    let id = text(&ins.rows[0], "id");
    let hash = text(&ins.rows[0], "hash");
    let err = db
        .run(&format!(
            r#"update docs[id == "{id}"] cas "nope" {{ room: "inbox" }}"#
        ))
        .unwrap_err();
    assert!(err.to_string().contains("cas mismatch"), "{err}");

    let ok = db
        .run(&format!(
            r#"update docs[id == "{id}"] cas "{hash}" {{ room: "inbox" }}"#
        ))
        .unwrap();
    assert_eq!(text(&ok.rows[0], "room"), "inbox");
}

#[test]
fn hop_after_insert_with_edge() {
    let mut db = Db::fixture();
    let ins = db
        .run(
            r#"insert docs { uri: "raw://n/hop", title: "hopper", layer: "raw" } with edge wikilink -> page "wiki://rag-overview""#,
        )
        .unwrap();
    let id = text(&ins.rows[0], "id");
    let hop = db
        .run(&format!(
            r#"docs | id == "{id}" | hop wikilink | {{ id, title }}"#
        ))
        .unwrap();
    assert!(
        hop.rows
            .iter()
            .any(|r| text(r, "id") == "e7c98d54-b4d6-4165-86e9-9b999e7ce9c3"
                || text(r, "title").contains("overview")),
        "{:?}",
        hop.rows
    );
}

#[test]
fn graph_after_insert_with_edge() {
    let mut db = Db::fixture();
    let ins = db
        .run(
            r#"insert docs { uri: "raw://n/graph", title: "grapher", layer: "raw" } with edge wikilink -> page "wiki://rag-overview""#,
        )
        .unwrap();
    let id = text(&ins.rows[0], "id");
    let g = db
        .run(&format!(
            r#"docs | id == "{id}" | graph wikilink | {{ rel, from, to }}"#
        ))
        .unwrap();
    assert_eq!(g.done.n, 1, "{:?}", g.rows);
    assert_eq!(text(&g.rows[0], "rel"), "wikilink");
    assert_eq!(text(&g.rows[0], "from"), id);
    assert!(
        text(&g.rows[0], "to") == "e7c98d54-b4d6-4165-86e9-9b999e7ce9c3"
            || text(&g.rows[0], "to") == "wiki://rag-overview",
        "{:?}",
        g.rows
    );
}

#[test]
fn graph_depth_chain() {
    let mut db = Db::fixture();
    db.run(
        r#"insert docs [
      { uri: "raw://g1", title: "G1", layer: "wiki" },
      { uri: "raw://g2", title: "G2", layer: "wiki" },
      { uri: "raw://g3", title: "G3", layer: "wiki" }
    ]"#,
    )
    .unwrap();
    let a = text(
        &db.run(r#"docs | uri == "raw://g1" | { id }"#).unwrap().rows[0],
        "id",
    )
    .to_string();
    let b = text(
        &db.run(r#"docs | uri == "raw://g2" | { id }"#).unwrap().rows[0],
        "id",
    )
    .to_string();
    let c = text(
        &db.run(r#"docs | uri == "raw://g3" | { id }"#).unwrap().rows[0],
        "id",
    )
    .to_string();
    db.run(&format!(
        r#"append edges [
      wikilink "{a}" -> "{b}",
      wikilink "{b}" -> "{c}"
    ]"#
    ))
    .unwrap();
    let d1 = db
        .run(&format!(
            r#"docs | id == "{a}" | graph wikilink | take all"#
        ))
        .unwrap();
    assert_eq!(d1.done.n, 1, "{:?}", d1.rows);
    assert_eq!(text(&d1.rows[0], "to"), b);
    let d2 = db
        .run(&format!(
            r#"docs | id == "{a}" | graph wikilink depth=2 | take all"#
        ))
        .unwrap();
    assert_eq!(d2.done.n, 2, "{:?}", d2.rows);
    assert!(
        d2.rows
            .iter()
            .any(|r| text(r, "from") == a && text(r, "to") == b)
    );
    assert!(
        d2.rows
            .iter()
            .any(|r| text(r, "from") == b && text(r, "to") == c)
    );
}

#[test]
fn match_one_hop() {
    let mut db = Db::fixture();
    let ins = db
        .run(
            r#"insert docs { uri: "raw://n/match", title: "matcher", layer: "raw" } with edge wikilink -> page "wiki://rag-overview""#,
        )
        .unwrap();
    let id = text(&ins.rows[0], "id");
    let m = db
        .run(&format!(
            r#"docs | id == "{id}" | match -wikilink-> b | {{ id, b.title }}"#
        ))
        .unwrap();
    assert_eq!(m.done.n, 1, "{:?}", m.rows);
    assert_eq!(text(&m.rows[0], "id"), id);
    assert!(
        text(&m.rows[0], "b.title").contains("overview") || !text(&m.rows[0], "b.title").is_empty(),
        "{:?}",
        m.rows
    );
}

#[test]
fn match_two_hop_chain() {
    let mut db = Db::fixture();
    db.run(
        r#"insert docs [
      { uri: "raw://m1", title: "M1", layer: "wiki" },
      { uri: "raw://m2", title: "M2", layer: "wiki" },
      { uri: "raw://m3", title: "M3", layer: "wiki" }
    ]"#,
    )
    .unwrap();
    let a = text(
        &db.run(r#"docs | uri == "raw://m1" | { id }"#).unwrap().rows[0],
        "id",
    )
    .to_string();
    let b = text(
        &db.run(r#"docs | uri == "raw://m2" | { id }"#).unwrap().rows[0],
        "id",
    )
    .to_string();
    let c = text(
        &db.run(r#"docs | uri == "raw://m3" | { id }"#).unwrap().rows[0],
        "id",
    )
    .to_string();
    db.run(&format!(
        r#"append edges [
      wikilink "{a}" -> "{b}",
      wikilink "{b}" -> "{c}"
    ]"#
    ))
    .unwrap();
    let m = db
        .run(&format!(
            r#"docs | id == "{a}" | match -wikilink-> mid -wikilink-> end | {{ mid.title, end.title }}"#
        ))
        .unwrap();
    assert_eq!(m.done.n, 1, "{:?}", m.rows);
    assert_eq!(text(&m.rows[0], "mid.title"), "M2");
    assert_eq!(text(&m.rows[0], "end.title"), "M3");

    let star = db
        .run(&format!(
            r#"docs | id == "{a}" | match -wikilink*2-> end | {{ end.title }}"#
        ))
        .unwrap();
    assert_eq!(star.done.n, 1, "{:?}", star.rows);
    assert_eq!(text(&star.rows[0], "end.title"), "M3");

    let rev = db
        .run(&format!(
            r#"docs | id == "{c}" | match <-wikilink- src | {{ src.title }}"#
        ))
        .unwrap();
    assert!(
        rev.rows.iter().any(|r| text(r, "src.title") == "M2"),
        "{:?}",
        rev.rows
    );

    let edge = db
        .run(&format!(
            r#"docs | id == "{a}" | match -[e:wikilink]-> b | {{ e.from, e.to, b.title }}"#
        ))
        .unwrap();
    assert_eq!(edge.done.n, 1, "{:?}", edge.rows);
    assert_eq!(text(&edge.rows[0], "e.from"), a);
    assert_eq!(text(&edge.rows[0], "e.to"), b);
    assert_eq!(text(&edge.rows[0], "b.title"), "M2");
}

#[test]
fn unknown_field_still_compile_fail() {
    let mut db = Db::fixture();
    let e = db.run(r#"docs | wign == "rag""#).unwrap_err();
    assert!(e.to_string().contains("unknown field: wign"), "{e}");
}

#[test]
fn take_50_implicit() {
    let mut db = Db::fixture();
    for i in 0..55 {
        db.run(&format!(
            r#"insert docs {{ uri: "raw://n/t{i}", title: "bulk {i}", layer: "wiki" }}"#
        ))
        .unwrap();
    }
    let h = db.run(r#"docs | layer == "wiki""#).unwrap();
    assert_eq!(h.rows.len(), 50, "implicit take 50, got {}", h.rows.len());
    let all = db.run(r#"docs | layer == "wiki" | take all"#).unwrap();
    assert!(all.rows.len() > 50, "take all should exceed 50");
}

#[test]
fn append_idempotent() {
    let mut db = Db::fixture();
    let a = db
        .run(r#"append facts { s: "lin", p: tagged, o: "db" }"#)
        .unwrap();
    let gen1 = a.done.r#gen;
    let b = db
        .run(r#"append facts { s: "lin", p: tagged, o: "db" }"#)
        .unwrap();
    assert_eq!(b.done.r#gen, gen1, "idempotent append must not bump gen");
    assert_eq!(b.message.as_deref(), Some("idempotent"));
    let n = db.run(r#"facts | s == "lin""#).unwrap();
    assert_eq!(n.rows.len(), 1);
}

#[test]
fn filter_ago_and_has() {
    let mut db = Db::fixture();
    let h = db
        .run(r#"docs | ts > ago 7d | title has "wal" | { id, title }"#)
        .unwrap();
    assert_eq!(h.done.n, 1);
    assert!(text(&h.rows[0], "title").contains("wal"));

    let old = db
        .run(r#"insert docs { uri: "raw://n/old", title: "wal ancient", layer: "wiki", ts: ago 30d }"#)
        .unwrap();
    assert_eq!(old.done.n, 1);
    let recent = db.run(r#"docs | ts > ago 7d | title has "wal""#).unwrap();
    assert!(
        recent.rows.iter().all(|r| text(r, "uri") != "raw://n/old"),
        "old row must fail ago 7d"
    );
}

#[test]
fn seed_join_and_search() {
    let mut db = Db::fixture();
    let j = db
        .run(r#"orders | total > 100 | join users on user_id | { id, users.email, total }"#)
        .unwrap();
    assert_eq!(j.done.n, 1);
    assert_eq!(text(&j.rows[0], "users.email"), "alice@lin.dev");

    let s = db.run(r#"docs | search "wal" | { id, title }"#).unwrap();
    assert!(s.done.n >= 1);
    assert!(s.rows.iter().any(|r| text(r, "title").contains("wal")));
}

#[test]
fn reembed_updates_embeddings() {
    let mut db = Db::fixture();
    let before = db.store.embed_id.clone();
    let h = db.run("reembed docs").unwrap();
    assert_eq!(db.store.embed_id, before);
    assert!(
        h.message.as_deref().unwrap_or("").contains("rows"),
        "{:?}",
        h.message
    );
    assert!(h.message.as_deref().unwrap_or("").contains(&before));
    let v = db
        .run(r#"docs | search vec "embedding identity" | take 5"#)
        .unwrap();
    assert!(v.done.n >= 1, "vec search should hit fixture docs");
}

#[test]
fn search_vec_and_hybrid_rank() {
    let mut db = Db::empty();
    db.run(r#"insert docs { uri: "raw://a", title: "wal shipping", layer: "wiki", body: "durable log" }"#)
        .unwrap();
    db.run(r#"insert docs { uri: "raw://b", title: "cats", layer: "wiki", body: "meow purr" }"#)
        .unwrap();
    let _ = db.reembed_collection("docs");

    let vec = db
        .run(r#"docs | search vec "wal shipping" | take 5"#)
        .unwrap();
    assert!(vec.done.n >= 1);
    assert_eq!(text(&vec.rows[0], "title"), "wal shipping");
    let hy = db.run(r#"docs | search "wal" | take 5"#).unwrap();
    assert!(hy.done.n >= 1);
    assert!(hy.rows.iter().any(|r| text(r, "title").contains("wal")));

    let lex = db.run(r#"docs | search lex "wal" | take 5"#).unwrap();
    assert!(lex.done.n >= 1);
    assert!(lex.rows.iter().any(|r| text(r, "title").contains("wal")));
    let plan = db
        .explain_as(r#"docs | search lex "wal" | take 5"#, None)
        .unwrap();
    assert!(plan.contains("FtsSeek"), "{plan}");
}

#[test]
fn search_lex_top_k_matches_full_ranking_prefix_and_skip() {
    let mut db = Db::fixture();
    for i in 0..80 {
        let title = if i % 3 == 0 {
            "wal wal tuning"
        } else {
            "wal tuning"
        };
        db.run(&format!(
            r#"insert docs {{ uri: "raw://top-k/{i:03}", title: "{title}", layer: "wiki", body: "wal storage" }}"#
        ))
        .unwrap();
    }

    let all = db
        .run(r#"docs | search lex "wal" | take all"#)
        .unwrap()
        .rows;
    let top = db.run(r#"docs | search lex "wal" | take 17"#).unwrap().rows;
    let skipped = db
        .run(r#"docs | search lex "wal" | skip 11 | take 13"#)
        .unwrap()
        .rows;

    let ids = |rows: &[lin::Row]| {
        rows.iter()
            .map(|row| text(row, "id").to_owned())
            .collect::<Vec<_>>()
    };
    assert_eq!(ids(&top), ids(&all[..17]));
    assert_eq!(ids(&skipped), ids(&all[11..24]));
}

#[test]
fn delete_docs_cas_and_edge_and_facts() {
    let mut db = Db::fixture();
    let ins = db
        .run(r#"insert docs { uri: "raw://n/del", title: "gone", layer: "wiki", body: "x" }"#)
        .unwrap();
    let id = text(&ins.rows[0], "id").to_string();
    let hash = text(&ins.rows[0], "hash").to_string();
    let gen1 = ins.done.r#gen;

    let err = db
        .run(&format!(r#"delete docs[id == "{id}"] cas "nope""#))
        .unwrap_err();
    assert!(err.to_string().contains("cas mismatch"), "{err}");
    assert_eq!(db.store.r#gen, gen1);

    let del = db
        .run(&format!(r#"delete docs[id == "{id}"] cas "{hash}""#))
        .unwrap();
    assert_eq!(del.done.n, 1);
    assert!(del.done.r#gen > gen1);
    let q = db.run(&format!(r#"docs | id == "{id}""#)).unwrap();
    assert_eq!(q.done.n, 0);

    db.run(r#"append edge wikilink "del-from" -> "del-to""#)
        .unwrap();
    let de = db
        .run(r#"delete edge wikilink "del-from" -> "del-to""#)
        .unwrap();
    assert_eq!(de.done.n, 1);
    let again = db
        .run(r#"delete edge wikilink "del-from" -> "del-to""#)
        .unwrap_err();
    assert!(again.to_string().contains("edge not found"), "{again}");

    db.run(r#"append facts { s: "lin", p: tagged, o: "zap" }"#)
        .unwrap();
    let df = db
        .run(r#"delete facts[s == "lin" and p == tagged and o == "zap"]"#)
        .unwrap();
    assert_eq!(df.done.n, 1);
    let left = db.run(r#"facts | s == "lin" and o == "zap""#).unwrap();
    assert_eq!(left.done.n, 0);
}

#[test]
fn union_concat_then_take() {
    let mut db = Db::fixture();
    let h = db.run(r#"docs | { id } | union orders | { id }"#).unwrap();
    assert_eq!(h.done.n, 5, "3 docs + 2 orders, got {}", h.done.n);
    let ids: Vec<&str> = h.rows.iter().map(|r| text(r, "id")).collect();
    assert!(ids.contains(&"o1"));
    assert!(ids.contains(&"e7c98d54-b4d6-4165-86e9-9b999e7ce9c3"));
}

#[test]
fn let_then_use() {
    let mut db = Db::fixture();
    let h = db
        .run(
            r#"let x = docs | wing == "rag" | { id }
x"#,
        )
        .unwrap();
    assert_eq!(h.done.n, 2);
    assert_eq!(h.done.r#gen, 0, "let is not a write");
}

#[test]
fn multi_stmt_one_pack_and_sees_writes() {
    let mut db = Db::fixture();
    let before = db.store.r#gen;
    let h = db
        .run(
            r#"insert docs { uri: "raw://a", title: "A", layer: "wiki" }
append edge wikilink "id-1" -> "id-2"
docs | uri == "raw://a" | { title }"#,
        )
        .unwrap();
    assert_eq!(h.done.n, 1);
    assert_eq!(text(&h.rows[0], "title"), "A");
    assert_eq!(
        h.done.r#gen,
        before + 1,
        "one gen bump for the whole program"
    );
}

#[test]
fn multi_stmt_atomic_rollback() {
    let mut db = Db::fixture();
    let before = db.store.r#gen;
    let docs_n = db.store.collection("docs").len();
    let e = db
        .run(
            r#"insert docs { uri: "raw://fail", title: "F", layer: "wiki" }
update docs[id == "missing"] cas "x" { room: "inbox" }"#,
        )
        .unwrap_err();
    assert!(e.to_string().contains("update: no matching row"), "{e}");
    assert_eq!(db.store.r#gen, before);
    assert_eq!(db.store.collection("docs").len(), docs_n);
    let q = db.run(r#"docs | uri == "raw://fail""#).unwrap();
    assert_eq!(q.done.n, 0);
}

#[test]
fn live_col_insert_query() {
    let mut db = Db::fixture();
    let h = db
        .run(
            r#"col notes { title: text }
rel cites
insert notes { title: "hi" }
notes | { title }"#,
        )
        .unwrap();
    assert_eq!(h.done.n, 1);
    assert_eq!(text(&h.rows[0], "title"), "hi");
    assert!(h.done.r#gen >= 1);
    let rels = db.run(r#"catalog | name == "cites""#).unwrap();
    assert!(rels.done.n >= 1);
}

#[test]
fn bulk_insert_then_count() {
    let mut db = Db::fixture();
    let before = db.store.r#gen;
    let h = db
        .run(
            r#"insert docs [
      { uri: "raw://a", title: "A", layer: "wiki" },
      { uri: "raw://b", title: "B", layer: "wiki" }
    ]"#,
        )
        .unwrap();
    assert_eq!(h.done.n, 2);
    assert_eq!(h.done.r#gen, before + 1);
    let n = db
        .run(r#"docs | uri == "raw://a" or uri == "raw://b" | take all"#)
        .unwrap();
    assert_eq!(n.done.n, 2);
}

#[test]
fn bulk_append_edges_hop() {
    let mut db = Db::fixture();
    db.run(
        r#"insert docs [
      { uri: "raw://h1", title: "H1", layer: "wiki" },
      { uri: "raw://h2", title: "H2", layer: "wiki" }
    ]"#,
    )
    .unwrap();
    let a = db.run(r#"docs | uri == "raw://h1" | { id }"#).unwrap();
    let b = db.run(r#"docs | uri == "raw://h2" | { id }"#).unwrap();
    let id_a = text(&a.rows[0], "id");
    let id_b = text(&b.rows[0], "id");
    db.run(&format!(
        r#"append edges [
      wikilink "{id_a}" -> "{id_b}"
    ]"#
    ))
    .unwrap();
    let hop = db
        .run(&format!(
            r#"docs | id == "{id_a}" | hop wikilink | {{ id, title }}"#
        ))
        .unwrap();
    assert!(
        hop.rows.iter().any(|r| text(r, "id") == id_b),
        "{:?}",
        hop.rows
    );
}

#[test]
fn cas_each_all_or_nothing() {
    let mut db = Db::fixture();
    let a = db
        .run(r#"insert docs { uri: "raw://ca", title: "CA", layer: "wiki", body: "x" }"#)
        .unwrap();
    let b = db
        .run(r#"insert docs { uri: "raw://cb", title: "CB", layer: "wiki", body: "y" }"#)
        .unwrap();
    let id_a = text(&a.rows[0], "id").to_string();
    let ha = text(&a.rows[0], "hash").to_string();
    let id_b = text(&b.rows[0], "id").to_string();
    let g0 = db.store.r#gen;
    let e = db
        .run(&format!(
            r#"update docs[id == "{id_a}"] cas "{ha}" {{ room: "changed" }}
update docs[id == "{id_a}" or id == "{id_b}"] cas each {{ room: "inbox" }}"#
        ))
        .unwrap_err();
    assert!(e.to_string().contains("cas mismatch"), "{e}");
    assert_eq!(db.store.r#gen, g0);
    let qa = db
        .run(&format!(r#"docs | id == "{id_a}" | {{ room }}"#))
        .unwrap();
    assert_ne!(text(&qa.rows[0], "room"), "inbox");
    assert_ne!(text(&qa.rows[0], "room"), "changed");
}

#[test]
fn cas_each_ok() {
    let mut db = Db::fixture();
    db.run(
        r#"insert docs [
      { uri: "raw://u1", title: "U1", layer: "wiki", body: "a" },
      { uri: "raw://u2", title: "U2", layer: "wiki", body: "b" }
    ]"#,
    )
    .unwrap();
    let ok = db
        .run(r#"update docs[uri == "raw://u1" or uri == "raw://u2"] cas each { room: "inbox" }"#)
        .unwrap();
    assert_eq!(ok.done.n, 2);
    let q = db
        .run(r#"docs | uri == "raw://u1" or uri == "raw://u2" | take all"#)
        .unwrap();
    assert!(q.rows.iter().all(|r| text(r, "room") == "inbox"));
}

#[test]
fn empty_list_exec() {
    let mut db = Db::fixture();
    let e = db.run("insert docs []").unwrap_err();
    assert!(e.to_string().contains("empty list"), "{e}");
}

#[test]
fn index_filter_explain_and_rows() {
    let mut db = Db::fixture();
    db.run("index docs [wing, ts]").unwrap();
    let plan = db
        .explain_as(
            r#"docs | wing == "rag" and ts > ago 7d | { id, title }"#,
            None,
        )
        .unwrap();
    assert!(plan.contains("index=docs[wing,ts]"), "{plan}");
    let h = db
        .run(r#"docs | wing == "rag" and ts > ago 7d | { id, title }"#)
        .unwrap();
    assert_eq!(h.done.n, 2);
}

#[test]
fn index_or_equality_rows() {
    let mut db = Db::fixture();
    db.run("index docs [wing, ts]").unwrap();
    let plan = db
        .explain_as(r#"docs | wing == "rag" or wing == "sys" | take all"#, None)
        .unwrap();
    assert!(plan.contains("index=docs[wing,ts]"), "{plan}");
    let h = db
        .run(r#"docs | wing == "rag" or wing == "sys" | take all"#)
        .unwrap();
    assert_eq!(h.done.n, 3);
    let only_rag = db.run(r#"docs | wing == "rag" | take all"#).unwrap();
    assert_eq!(only_rag.done.n, 2);
}

#[test]
fn index_maintain_delete_update() {
    let mut db = Db::fixture();
    db.run("index docs [wing, ts]").unwrap();
    let ins = db
        .run(r#"insert docs { uri: "raw://ix", title: "IX", layer: "wiki", wing: "rag", body: "z" }"#)
        .unwrap();
    let id = text(&ins.rows[0], "id").to_string();
    let hash = text(&ins.rows[0], "hash").to_string();
    db.run(&format!(
        r#"update docs[id == "{id}"] cas "{hash}" {{ wing: "sys" }}"#
    ))
    .unwrap();
    let rag = db.run(r#"docs | wing == "rag" | take all"#).unwrap();
    assert!(rag.rows.iter().all(|r| text(r, "id") != id));
    let hash2 = text(
        &db.run(&format!(r#"docs | id == "{id}""#)).unwrap().rows[0],
        "hash",
    )
    .to_string();
    db.run(&format!(r#"delete docs[id == "{id}"] cas "{hash2}""#))
        .unwrap();
    let again = db.run(r#"docs | uri == "raw://ix""#).unwrap();
    assert_eq!(again.done.n, 0);
}

#[test]
fn bulk_insert_uniqueness_checks_empty_batch_and_existing_rows_atomically() {
    for (seed, source, expected_error) in [
        (
            false,
            r#"insert docs [{id:"same",uri:"u://1"},{id:"same",uri:"u://2"}]"#,
            "duplicate id",
        ),
        (
            false,
            r#"insert docs [{id:"one",uri:"u://same"},{id:"two",uri:"u://same"}]"#,
            "duplicate uri",
        ),
        (
            true,
            r#"insert docs [{id:"new",uri:"u://new"},{id:"seed",uri:"u://other"}]"#,
            "duplicate id",
        ),
        (
            true,
            r#"insert docs [{id:"new",uri:"u://new"},{id:"other",uri:"u://seed"}]"#,
            "duplicate uri",
        ),
    ] {
        let mut db = Db::empty();
        if seed {
            db.run(r#"insert docs {id:"seed",uri:"u://seed"}"#).unwrap();
        }
        let before = db.store.collection("docs").to_vec();
        let generation = db.store.r#gen;
        let error = db.run(source).unwrap_err();
        assert!(error.to_string().contains(expected_error), "{error}");
        assert_eq!(db.store.collection("docs"), before);
        assert_eq!(db.store.r#gen, generation);
        db.run(r#"insert docs [{id:"new",uri:"u://new"},{id:"other",uri:"u://other"}]"#)
            .unwrap();
        assert_eq!(db.store.collection("docs").len(), before.len() + 2);
        assert!(db.store.get_by_id("docs", "new").is_some());
        assert!(db.store.get_by_uri("u://other").is_some());
    }
}

#[test]
fn unique_index_rejects_duplicate_and_rolls_back() {
    let mut db = Db::fixture();
    db.run("index docs unique [title]").unwrap();
    db.run(r#"insert docs { uri: "raw://u-a", title: "same", layer: "wiki" }"#)
        .unwrap();
    let before = db.store.r#gen;
    let n = db.store.collection("docs").len();
    let e = db
        .run(r#"insert docs { uri: "raw://u-b", title: "same", layer: "wiki" }"#)
        .unwrap_err();
    assert!(e.to_string().contains("unique index"), "{e}");
    assert_eq!(db.store.r#gen, before);
    assert_eq!(db.store.collection("docs").len(), n);
    assert_eq!(db.run(r#"docs | uri == "raw://u-b""#).unwrap().done.n, 0);
}

#[test]
fn grouped_commit_rolls_back_rows_and_fts_on_runtime_error() {
    let mut db = Db::fixture();
    db.run("index docs unique [title]").unwrap();
    let before = db.store.r#gen;
    let error = db
        .run_group([
            r#"insert docs { uri: "raw://group/a", title: "group-duplicate", layer: "wiki", body: "group rollback token" }"#,
            r#"insert docs { uri: "raw://group/b", title: "group-duplicate", layer: "wiki", body: "group rollback token" }"#,
        ])
        .unwrap_err();
    assert!(error.to_string().contains("unique index"), "{error}");
    assert_eq!(db.store.r#gen, before);
    assert_eq!(
        db.run(r#"docs | search lex "rollback token" | take all"#)
            .unwrap()
            .done
            .n,
        0
    );
}

#[test]
fn unique_index_refuses_existing_duplicates() {
    let mut db = Db::fixture();
    db.run(r#"insert docs { uri: "raw://d1", title: "dup", layer: "wiki" }"#)
        .unwrap();
    db.run(r#"insert docs { uri: "raw://d2", title: "dup", layer: "wiki" }"#)
        .unwrap();
    let e = db.run("index docs unique [title]").unwrap_err();
    assert!(e.to_string().contains("unique index"), "{e}");
}

#[test]
fn fk_insert_requires_parent_row() {
    let mut db = Db::empty();
    let e = db
        .run(r#"insert orders { user_id: "missing", total: 1 }"#)
        .unwrap_err();
    assert!(e.to_string().contains("fk:"), "{e}");
}

#[test]
fn fk_insert_ok_when_parent_exists() {
    let mut db = Db::empty();
    let u = db.run(r#"insert users { email: "a@b.c" }"#).unwrap();
    let uid = text(&u.rows[0], "id");
    let o = db
        .run(&format!(
            r#"insert orders {{ user_id: "{uid}", total: 9 }}"#
        ))
        .unwrap();
    assert_eq!(o.done.n, 1);
}

#[test]
fn bulk_insert_updates_index() {
    let mut db = Db::fixture();
    db.run("index docs [wing, ts]").unwrap();
    db.run(
        r#"insert docs [
      { uri: "raw://i1", title: "I1", layer: "wiki", wing: "rag" },
      { uri: "raw://i2", title: "I2", layer: "wiki", wing: "rag" }
    ]"#,
    )
    .unwrap();
    let plan = db
        .explain_as(r#"docs | wing == "rag" | take all"#, None)
        .unwrap();
    assert!(plan.contains("index=docs[wing,ts]"), "{plan}");
    let h = db.run(r#"docs | wing == "rag" | take all"#).unwrap();
    assert!(h.done.n >= 4);
}

#[test]
fn project_take_all_skips_body() {
    let mut db = Db::empty();
    db.run("index docs [wing, ts]").unwrap();
    db.run(
        r#"insert docs [
      { uri: "raw://p1", title: "has wal here", layer: "wiki", wing: "rag", body: "huge" },
      { uri: "raw://p2", title: "plain", layer: "wiki", wing: "sys", body: "huge" },
      { uri: "raw://p3", title: "wal note", layer: "wiki", wing: "rag", body: "huge" }
    ]"#,
    )
    .unwrap();
    let h = db
        .run(r#"docs | wing == "rag" | { id, title } | take all"#)
        .unwrap();
    assert_eq!(h.done.n, 2);
    assert!(h.rows.iter().all(|r| !r.contains_key("body")));
    assert!(h.rows.iter().all(|r| r.contains_key("title")));
    let c = db.run(r#"docs | title ~ "wal" | count by layer"#).unwrap();
    assert_eq!(c.done.n, 1);
    assert_eq!(c.rows[0].get("hits"), Some(&lin::Cell::Int(2)));
    let total = db.run(r#"docs | title ~ "wal" | count"#).unwrap();
    assert_eq!(total.done.n, 1);
    assert_eq!(total.rows[0].get("hits"), Some(&lin::Cell::Int(2)));
    assert!(!total.rows[0].contains_key("layer"));
    // Implicit take 50 still applies without `take all`.
    let mut db2 = Db::empty();
    db2.run("index docs [wing, ts]").unwrap();
    let mut batch = String::from("insert docs [\n");
    for i in 0..60 {
        if i > 0 {
            batch.push_str(",\n");
        }
        batch.push_str(&format!(
            r#"  {{ uri: "raw://m{i}", title: "t{i}", layer: "wiki", wing: "rag" }}"#
        ));
    }
    batch.push_str("\n]");
    db2.run(&batch).unwrap();
    let limited = db2.run(r#"docs | wing == "rag" | { id, title }"#).unwrap();
    assert_eq!(limited.done.n, 50);
    let all = db2
        .run(r#"docs | wing == "rag" | { id, title } | take all"#)
        .unwrap();
    assert_eq!(all.done.n, 60);
}

#[test]
fn catalog_filter_hides_reads_not_writes() {
    let mut db = Db::empty();
    db.run(
        r#"insert docs [
      { uri: "raw://wiki", title: "W", layer: "wiki" },
      { uri: "raw://raw", title: "R", layer: "raw" }
    ]"#,
    )
    .unwrap();
    db.run(r#"filter docs layer == "wiki""#).unwrap();
    let vis = db.run(r#"docs | take all"#).unwrap();
    assert_eq!(vis.done.n, 1);
    assert_eq!(text(&vis.rows[0], "title"), "W");
    let all = db.run(r#"docs all | take all"#).unwrap();
    assert_eq!(all.done.n, 2);
    let del = db
        .run(r#"delete docs[uri == "raw://raw"] cas each"#)
        .unwrap();
    assert_eq!(del.done.n, 1);
    db.run("unfilter docs").unwrap();
    assert_eq!(db.run(r#"docs | take all"#).unwrap().done.n, 1);
}

#[test]
fn owned_type_is_not_a_collection() {
    let mut db = Db::empty();
    db.run("owned stamp { hash: text, ts: time }").unwrap();
    db.run("col notes { title: text, stamp }").unwrap();
    db.run(r#"insert notes { title: "n", hash: "h", ts: ago 1s }"#)
        .unwrap();
    let h = db.run(r#"notes | { title, hash, ts } | take all"#).unwrap();
    assert_eq!(h.done.n, 1);
    assert_eq!(text(&h.rows[0], "hash"), "h");
    let err = db.run("insert stamp { hash: \"x\" }").unwrap_err();
    assert!(err.to_string().contains("unknown collection"), "{err}");
}

/// Snapshot every structure the write path maintains across a rollback: row
/// order, SoA columns, scalar index, FTS postings, id/uri point maps.
fn digest(db: &mut Db) -> String {
    let mut out = String::new();
    for src in [
        r#"docs | { id, uri, title, wing, layer, hash } | take all"#,
        r#"docs | wing == "rag" | { id, title } | take all"#,
        r#"docs | search lex "wal" | { id } | take all"#,
        r#"docs | uri == "raw://n/wal" | { id, title } | take all"#,
        r#"users | { id, email } | take all"#,
        r#"orders | { id, user_id, total } | take all"#,
    ] {
        let h = db.run(src).expect(src);
        out.push_str(&format!("{src} => {:#?}\n", h.rows));
    }
    out
}

#[test]
fn update_rollback_restores_rows_and_indexes() {
    let mut db = Db::fixture();
    db.run("index docs unique [title]").unwrap();
    let before = digest(&mut db);
    let e = db
        .run(r#"update docs[wing == "rag"] cas each { title: "same" }"#)
        .unwrap_err();
    assert!(e.to_string().contains("unique index"), "{e}");
    assert_eq!(before, digest(&mut db), "row journal undo must be exact");
}

#[test]
fn mixed_row_journal_rolls_back_multiple_swaps_and_updates() {
    let mut db = Db::fixture();
    db.run("index docs unique [title]").unwrap();
    let before = digest(&mut db);
    let generation = db.r#gen();
    let error = db
        .run_group([
        r#"update docs[uri == "wiki://rag-overview"] cas each { title: "journal token", wing: "sys" }"#,
            r#"delete docs[uri == "raw://n/wal"] cas each"#,
            r#"delete docs[wing == "rag"] cas each"#,
            r#"delete docs[uri == "missing"] cas each"#,
        ])
        .unwrap_err();
    assert!(error.to_string().contains("no matching row"), "{error}");
    assert_eq!(db.r#gen(), generation);
    assert_eq!(digest(&mut db), before);
    assert_eq!(
        db.run(r#"docs | search lex "journal" | take all"#)
            .unwrap()
            .done
            .n,
        0
    );
}

#[test]
fn bounded_dnf_falls_back_without_changing_results() {
    let mut db = Db::fixture();
    db.run("index docs [wing]").unwrap();
    let branches = (0..14)
        .map(|_| r#"(wing == "rag" or wing == "sys")"#)
        .collect::<Vec<_>>()
        .join(" and ");
    let query = format!("docs | {branches} | {{ id }} | sort id | take all");
    let expected = db
        .run(r#"docs | (wing == "rag" or wing == "sys") | { id } | sort id | take all"#)
        .unwrap()
        .rows;
    let prepared = db.prepare(&query).unwrap();
    assert!(!format!("{:?}", prepared.plan).contains("IndexSeek"));
    assert_eq!(prepared.run(&mut db).unwrap().rows, expected);
    // Small disjunctions still use the scalar index.
    let small = db
        .prepare(r#"docs | wing == "rag" or wing == "sys" | { id } | take all"#)
        .unwrap();
    assert!(format!("{:?}", small.plan).contains("IndexSeek"));
}

#[test]
fn point_mutation_keeps_residual_predicate() {
    let mut db = Db::fixture();
    let before = digest(&mut db);
    for source in [
        r#"update docs[uri == "raw://n/wal" and wing == "missing"] cas each { title: "wrong" }"#,
        r#"delete docs[uri == "raw://n/wal" and wing == "missing"] cas each"#,
    ] {
        assert!(
            db.run(source)
                .unwrap_err()
                .to_string()
                .contains("no matching row")
        );
        assert_eq!(digest(&mut db), before);
    }
}

#[test]
fn delete_rollback_restores_positions_and_structures() {
    let mut db = Db::fixture();
    db.run("index docs unique [title]").unwrap();
    let before = digest(&mut db);
    let e = db
        .run_group([
            r#"delete docs[uri == "raw://n/wal"] cas each"#,
            r#"delete docs[uri == "raw://gone"] cas each"#,
        ])
        .unwrap_err();
    assert!(e.to_string().contains("no matching row"), "{e}");
    assert_eq!(before, digest(&mut db));
    let again = db
        .run(r#"delete docs[uri == "raw://n/wal"] cas each"#)
        .unwrap();
    assert_eq!(again.done.n, 1);
    assert_eq!(db.run(r#"docs | take all"#).unwrap().done.n, 2);
    let dup = db
        .run(r#"insert docs { uri: "raw://dup", title: "Lin facts", layer: "wiki" }"#)
        .unwrap_err();
    assert!(dup.to_string().contains("unique index"), "{dup}");
}

#[test]
fn delete_rollback_across_collections_keeps_soa_and_maps() {
    let mut db = Db::fixture();
    let users = db.run(r#"users | { email } | take 1"#).unwrap();
    let email = text(&users.rows[0], "email").to_string();
    let before = digest(&mut db);
    let e = db
        .run_group([
            format!(r#"delete users[email == "{email}"] cas each"#),
            r#"delete docs[uri == "wiki://rag-overview"] cas each"#.to_string(),
            r#"delete orders[total > 100000] cas each"#.to_string(),
        ])
        .unwrap_err();
    assert!(e.to_string().contains("no matching row"), "{e}");
    assert_eq!(before, digest(&mut db));
    let joined = db
        .run(r#"orders | join users on user_id | { id, users.email, total } | take all"#)
        .unwrap();
    assert!(joined.done.n >= 1, "join should still resolve users");
    let by_email = db
        .run(&format!(r#"users | email == "{email}" | {{ id }}"#))
        .unwrap();
    assert_eq!(by_email.done.n, 1, "row map must still resolve by email");
}

fn three_docs() -> Db {
    let mut db = Db::empty();
    db.run("index docs [wing, ts]").unwrap();
    db.run(
        r#"insert docs [
          { uri: "raw://a", title: "alpha zeta", wing: "rag", layer: "wiki" },
          { uri: "raw://b", title: "beta zeta", wing: "rag", layer: "wiki" },
          { uri: "raw://c", title: "gamma zeta", wing: "rag", layer: "wiki" }
        ]"#,
    )
    .unwrap();
    db
}

fn hash_of(db: &mut Db, uri: &str) -> String {
    let q = format!(r#"docs | uri == "{uri}" | {{ hash }}"#);
    let h = db.run(&q).unwrap();
    text(&h.rows[0], "hash").to_string()
}

#[test]
fn update_repairs_fts_and_index_keys() {
    let mut db = three_docs();
    let hash = hash_of(&mut db, "raw://a");
    db.run(&format!(
        r#"update docs[uri == "raw://a"] cas "{hash}" {{ wing: "sys", title: "omega nova" }}"#
    ))
    .unwrap();

    let zeta = db
        .run(r#"docs | search lex "zeta" | { uri } | take all"#)
        .unwrap();
    assert_eq!(
        zeta.rows.iter().map(|r| text(r, "uri")).collect::<Vec<_>>(),
        vec!["raw://b", "raw://c"],
        "token left the updated row's postings"
    );
    let omega = db
        .run(r#"docs | search lex "omega" | { uri } | take all"#)
        .unwrap();
    assert_eq!(
        omega
            .rows
            .iter()
            .map(|r| text(r, "uri"))
            .collect::<Vec<_>>(),
        vec!["raw://a"],
        "token entered the updated row's postings"
    );
    let alpha = db
        .run(r#"docs | search lex "alpha" | { uri } | take all"#)
        .unwrap();
    assert_eq!(alpha.done.n, 0, "old token dropped from the old posting");

    let old_wing = db
        .run(r#"docs | wing == "rag" | { uri } | take all"#)
        .unwrap();
    assert_eq!(old_wing.done.n, 2, "index key must drop the old value");
    let new_wing = db.run(r#"docs | wing == "sys" | { uri }"#).unwrap();
    assert_eq!(new_wing.done.n, 1);
    assert_eq!(text(&new_wing.rows[0], "uri"), "raw://a");
    // The point map still resolves the row it moved in the vec.
    let by_uri = db
        .run(r#"docs | uri == "raw://a" | { title, wing }"#)
        .unwrap();
    assert_eq!(text(&by_uri.rows[0], "wing"), "sys");
    assert_eq!(text(&by_uri.rows[0], "title"), "omega nova");
}

#[test]
fn delete_reindexes_maps_and_postings() {
    let mut db = three_docs();
    // Front delete: the last live row swaps into the vacated slot, so survivors keep
    // their entries but the vec is no longer insertion-ordered (`sort` restores that).
    let hash = hash_of(&mut db, "raw://a");
    db.run(&format!(r#"delete docs[uri == "raw://a"] cas "{hash}""#))
        .unwrap();
    let shared = db
        .run(r#"docs | search lex "zeta" | { uri } | take all"#)
        .unwrap();
    let mut survivors: Vec<String> = shared
        .rows
        .iter()
        .map(|r| text(r, "uri").to_string())
        .collect();
    survivors.sort();
    assert_eq!(
        survivors,
        vec!["raw://b", "raw://c"],
        "survivors keep their postings after the swap"
    );
    let gone = db
        .run(r#"docs | search lex "alpha" | { uri } | take all"#)
        .unwrap();
    assert_eq!(gone.done.n, 0, "deleted row left every posting");
    let rag = db
        .run(r#"docs | wing == "rag" | { uri } | take all"#)
        .unwrap();
    assert_eq!(rag.done.n, 2, "index postings moved with their rows");
    let by_uri = db.run(r#"docs | uri == "raw://c" | { title }"#).unwrap();
    assert_eq!(
        text(&by_uri.rows[0], "title"),
        "gamma zeta",
        "uri map survived"
    );

    // Tail delete: positions below the hole must keep their postings.
    let hash_c = hash_of(&mut db, "raw://c");
    db.run(&format!(r#"delete docs[uri == "raw://c"] cas "{hash_c}""#))
        .unwrap();
    let b = db
        .run(r#"docs | uri == "raw://b" | { title, wing }"#)
        .unwrap();
    assert_eq!(text(&b.rows[0], "title"), "beta zeta");
    let shared = db
        .run(r#"docs | search lex "zeta" | { uri } | take all"#)
        .unwrap();
    assert_eq!(
        shared
            .rows
            .iter()
            .map(|r| text(r, "uri"))
            .collect::<Vec<_>>(),
        vec!["raw://b"],
        "tail delete dropped only its own posting entries"
    );
    let gamma = db
        .run(r#"docs | search lex "gamma" | { uri } | take all"#)
        .unwrap();
    assert_eq!(gamma.done.n, 0);
}

#[test]
fn fts_case_folds_both_paths() {
    let mut db = Db::empty();
    db.run(
        r#"insert docs [
          { uri: "raw://up", title: "WAL École Write-Ahead", body: "text" },
          { uri: "raw://low", title: "wal école write-ahead", body: "text" }
        ]"#,
    )
    .unwrap();
    // Mixed-case rows take the lowering path, lowercase rows borrow as-is.
    for q in ["wal", "école", "write-ahead"] {
        let hits = db
            .run(&format!(
                r#"docs | search lex "{q}" | {{ uri }} | take all"#
            ))
            .unwrap();
        assert_eq!(hits.done.n, 2, "{q} must match both rows");
    }
    let miss = db.run(r#"docs | search lex "writing" | take all"#).unwrap();
    assert_eq!(miss.done.n, 0);

    let hash = hash_of(&mut db, "raw://up");
    db.run(&format!(
        r#"update docs[uri == "raw://up"] cas "{hash}" {{ title: "École" }}"#
    ))
    .unwrap();
    let gone = db
        .run(r#"docs | search lex "wal" | { uri } | take all"#)
        .unwrap();
    assert_eq!(
        gone.rows.iter().map(|r| text(r, "uri")).collect::<Vec<_>>(),
        vec!["raw://low"],
        "removing an upper-case token must find its folded posting"
    );
}
#[test]
fn deep_dnf_plans_as_scan_but_returns_seek_rows() {
    fn groups(n: usize) -> String {
        let mut s = String::new();
        for _ in 0..n {
            if !s.is_empty() {
                s.push_str(" and ");
            }
            s.push_str(r#"(wing == "rag" or wing == "sys")"#);
        }
        format!(r#"docs | {s} | {{ id }} | take all"#)
    }

    let mut indexed = Db::fixture();
    indexed.run("index docs [wing, ts]").unwrap();
    let mut plain = Db::fixture();

    let deep = groups(14);
    let plan = indexed.explain_as(&deep, None).unwrap();
    assert!(
        !plan.contains("index=docs[wing,ts]"),
        "a 2^14-branch predicate must plan a scan: {plan}"
    );
    let shallow = groups(4);
    let plan = indexed.explain_as(&shallow, None).unwrap();
    assert!(
        plan.contains("index=docs[wing,ts]"),
        "a 2^4-branch predicate must still plan seeks: {plan}"
    );

    let with_index = indexed.run(&deep).unwrap();
    let scan_rows = plain.run(&deep).unwrap();
    assert_eq!(with_index.done.n, scan_rows.done.n);
    assert!(with_index.done.n > 0, "fixture must match the predicate");
    let ids = |rows: &[lin::Row]| {
        rows.iter()
            .map(|r| text(r, "id").to_string())
            .collect::<Vec<_>>()
    };
    assert_eq!(ids(&with_index.rows), ids(&scan_rows.rows));
    assert_eq!(
        ids(&indexed.run(&shallow).unwrap().rows),
        ids(&scan_rows.rows)
    );
}

/// A numeric field can hold `Int` and `Float` cells at once (the checker lets
/// either literal through), so index keys must rank both on the scale the scan
/// compares them on.
#[test]
fn index_seeks_mixed_int_and_float_numbers_like_the_scan() {
    let mut plain = Db::fixture();
    let uid = {
        let h = plain.run(r"users | take 1 | { id }").unwrap();
        text(&h.rows[0], "id").to_string()
    };
    let mut idx = Db::fixture();
    idx.run("index orders [total]").unwrap();
    for db in [&mut plain, &mut idx] {
        for (n, t) in [
            ("a", "9"),
            ("b", "10.5"),
            ("c", "3"),
            ("d", "2.25"),
            ("e", "100"),
            ("f", "0.5"),
            ("g", "-4"),
            ("h", "-4.0"),
            ("i", "-2.5"),
            ("j", "9007199254740992"),
            ("k", "9007199254740993"),
            ("l", "9007199254740992.0"),
            ("m", "0"),
            ("n", "-0.0"),
        ] {
            db.run(&format!(
                r#"insert orders {{ id: "{n}", user_id: "{uid}", total: {t}, ts: ago 1d }}"#
            ))
            .unwrap();
        }
    }

    for q in [
        r"total > 8.5",
        r"total < 9.5",
        r"total >= 2.25",
        r"total <= 2.25",
        r"total == 9",
        r"total == 10.5",
        r"total > 2 and total < 100",
        r"total < -1.5",
        r"total >= -4",
        r"total == -4",
        r"total > -4 and total < 0",
        r"total == 9007199254740992",
        r"total == 9007199254740993",
        r"total == 9007199254740992.0",
        r"total == 0.0",
        r"total > 2 and total > 100",
        r"total == 9 and total == 10.5",
    ] {
        let stmt = format!(r"orders | {q} | {{ id }} | sort id asc | take all");
        let plan = idx.explain_as(&stmt, None).unwrap();
        assert!(
            plan.contains("index=orders[total]"),
            "{q} must seek: {plan}"
        );
        let want: Vec<String> = ids(&plain.run(&stmt).unwrap());
        let got: Vec<String> = ids(&idx.run(&stmt).unwrap());
        assert_eq!(got, want, "{q} disagrees with the scan");
    }
}

#[test]
fn title_contains_count_matches_string_search_at_boundaries() {
    let mut db = lin::Db::empty();
    let titles = [
        "",
        "a",
        "aaaaab",
        "wal",
        "prefix wal",
        "walwal",
        "ёжик 🦔",
        "0123456789abcdefg",
        "0123456789abcdef",
    ];
    for (i, title) in titles.iter().enumerate() {
        db.run(&format!(
            r#"insert docs [{{id: "t-{i}", uri: "test://{i}", title: {}, body: ""}}]"#,
            serde_json::to_string(title).unwrap()
        ))
        .unwrap();
    }
    for needle in [
        "",
        "a",
        "aaab",
        "wal",
        "al",
        "missing",
        "ёж",
        "🦔",
        "0123456789abcdef",
        "0123456789abcdefg",
        "0123456789abcdefgh",
    ] {
        let got = db
            .run(&format!(
                "docs | title ~ {} | count",
                serde_json::to_string(needle).unwrap()
            ))
            .unwrap();
        let want = titles.iter().filter(|title| title.contains(needle)).count() as i64;
        assert_eq!(
            got.rows[0].values().next(),
            Some(&lin::Cell::Int(want)),
            "needle={needle:?}"
        );
    }
}

#[test]
fn packed_title_scan_tracks_mutations_and_rejects_cross_row_matches() {
    let mut db = lin::Db::empty();
    db.run(r#"insert docs [{id: "a", uri: "test://a", title: "aa", body: ""}, {id: "b", uri: "test://b", title: "aaa", body: ""}]"#).unwrap();
    let count = |db: &mut lin::Db, needle: &str| {
        let result = db
            .run(&format!(
                "docs | title ~ {} | count",
                serde_json::to_string(needle).unwrap()
            ))
            .unwrap();
        result.rows[0].values().next().unwrap().clone()
    };
    assert_eq!(count(&mut db, "aaaa"), lin::Cell::Int(0));
    assert_eq!(count(&mut db, "aaa"), lin::Cell::Int(1));
    db.run(r#"update docs[id == "a"] cas each {title: "aaaa"}"#)
        .unwrap();
    assert_eq!(count(&mut db, "aaaa"), lin::Cell::Int(1));
    db.run(r#"delete docs[id == "a"] cas each"#).unwrap();
    assert_eq!(count(&mut db, "aaaa"), lin::Cell::Int(0));
    assert_eq!(count(&mut db, "aaa"), lin::Cell::Int(1));
    db.run(r#"insert docs [{id: "c", uri: "test://c", title: "aaaa", body: ""}]"#)
        .unwrap();
    assert_eq!(count(&mut db, "aaaa"), lin::Cell::Int(1));
    // Rebuild the physical column between a mutation and its rollback.
    assert!(
        db.run(
            r#"update docs[id == "b"] cas each {title: "changed"}
        docs | title ~ "changed" | count
        update docs[id == "c"] cas "wrong" {title: "bad"}"#
        )
        .is_err()
    );
    assert_eq!(count(&mut db, "aaa"), lin::Cell::Int(2));
    assert_eq!(count(&mut db, "changed"), lin::Cell::Int(0));
}

#[test]
fn deleted_rows_keep_original_order_and_rollback_positions() {
    let mut db = Db::fixture();
    db.run(
        r#"col notes { title: text }
insert notes [
 {id: "n0", title: "live"}, {id: "n1", title: "dead"},
 {id: "n2", title: "live"}, {id: "n3", title: "dead"},
 {id: "n4", title: "live"}
]
index notes [title]"#,
    )
    .unwrap();
    let before = db.store.collection("notes").to_vec();
    assert!(
        db.run(
            r#"delete notes[title == "dead"] cas ""
update notes[id == "missing"] cas "" { title: "x" }"#
        )
        .is_err()
    );
    assert_eq!(db.store.collection("notes"), before);
    let removed = db.run(r#"delete notes[title == "dead"] cas """#).unwrap();
    assert_eq!(ids(&removed), vec!["n1", "n3"]);
    assert_eq!(
        db.run(r#"notes | title == "dead" | count"#)
            .unwrap()
            .scalar_as::<i64>()
            .unwrap(),
        0
    );
}

#[test]
fn prepared_insert_pack_keeps_statement_alignment_time_and_new_ids() {
    let mut db = Db::fixture();
    db.run("col notes { title: text, stamp: time }").unwrap();
    let prepared = db
        .prepare(
            r#"insert notes { title: "first", stamp: now }
notes | count
insert notes { title: "second", stamp: now }"#,
        )
        .unwrap();
    prepared.run(&mut db).unwrap();
    prepared.clone().run(&mut db).unwrap();
    let rows = db.store.collection("notes");
    assert_eq!(
        rows.iter().map(|r| text(r, "title")).collect::<Vec<_>>(),
        vec!["first", "second", "first", "second"]
    );
    let ids = rows
        .iter()
        .map(|r| text(r, "id"))
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(ids.len(), 4);
    for row in rows {
        assert!(matches!(row.get("stamp"), Some(lin::Cell::Time(t)) if *t > 0));
    }
}

#[test]
fn bulk_insert_count_survives_result_elision_boundary() {
    for n in [1, 128, 129, 256] {
        let mut db = Db::empty();
        let records = (0..n)
            .map(|i| format!(r#"{{ uri: "raw://count/{i}", title: "wal {i}", layer: "wiki" }}"#))
            .collect::<Vec<_>>()
            .join(",");
        let handle = db.run(&format!("insert docs [{records}]")).unwrap();
        assert_eq!(handle.done.n, n, "n={n}");
        assert_eq!(handle.rows.len(), if n > 128 { 0 } else { n });
        assert_eq!(
            db.run("docs | count").unwrap().scalar_as::<i64>().unwrap(),
            n as i64
        );
    }
}

#[test]
fn filters_after_aggregation_and_skip_apply_to_stage_output() {
    let mut db = Db::empty();
    db.run(
        r#"col stage_rows { group: text, n: i64 }
        insert stage_rows [ { group: "a", n: 1 }, { group: "a", n: 2 }, { group: "b", n: 3 } ]"#,
    )
    .unwrap();
    let first = db.run("stage_rows | take 1").unwrap().rows;
    let id = text(&first[0], "id");
    let skipped = db
        .run(&format!("stage_rows | skip 3 | id == \"{id}\" | take all"))
        .unwrap()
        .rows;
    assert!(
        skipped.is_empty(),
        "late point filter must not restore skipped rows"
    );
    let q = "stage_rows | count by group | hits > 1 | { group } | take all";
    let rows = db.run(q).unwrap().rows;
    assert_eq!(rows.len(), 1);
    assert_eq!(text(&rows[0], "group"), "a");
    assert_eq!(db.run_batch(q).unwrap().to_rows(), rows);
    assert_eq!(db.reader().run(q).unwrap().rows, rows);
    let rows = db
        .run("stage_rows | skip 1 | n < 2 | take all")
        .unwrap()
        .rows;
    assert!(rows.is_empty(), "skip must happen before filtering");
    let rows = db
        .run("stage_rows | sort n desc | take 1 | n < 3 | take all")
        .unwrap()
        .rows;
    assert!(rows.is_empty(), "take must happen before filtering");
}

#[test]
fn numeric_sort_orders_nan_after_numbers_and_keeps_zero_and_nan_ties_stable() {
    let mut db = Db::empty();
    db.run("col numeric_sort { value: f64, ordinal: i64 }")
        .unwrap();
    for i in 0..5 {
        db.run(&format!(
            "insert numeric_sort {{ value: 1.0, ordinal: {i} }}"
        ))
        .unwrap();
    }
    let values = [f64::NAN, -0.0, 0.0, 1.0, f64::NAN];
    for (row, value) in db
        .store
        .collections
        .get_mut("numeric_sort")
        .unwrap()
        .iter_mut()
        .zip(values)
    {
        row.insert("value".into(), lin::Cell::Float(value));
    }
    for (q, expected) in [
        (
            "numeric_sort | sort value | { ordinal } | take all",
            vec![1, 2, 3, 0, 4],
        ),
        (
            "numeric_sort | sort value desc | { ordinal } | take all",
            vec![0, 4, 3, 1, 2],
        ),
    ] {
        let actual: Vec<_> = db
            .run(q)
            .unwrap()
            .rows
            .iter()
            .map(|row| row.get("ordinal").unwrap().as_int().unwrap())
            .collect();
        assert_eq!(actual, expected);
    }
}

#[test]
fn filter_project_shortcuts_preserve_multiple_filters_and_windows() {
    let mut db = Db::empty();
    db.run("col shortcut_rows { n: i64, text: text }").unwrap();
    for n in 0..8 {
        db.run(&format!(
            "insert shortcut_rows {{ n: {n}, text: \"{}\" }}",
            if n % 2 == 0 { "hit" } else { "miss" }
        ))
        .unwrap();
    }
    for (q, expected) in [
        (
            "shortcut_rows | n >= 0 | take 4 | n > 1 | { n } | take all",
            vec![2, 3],
        ),
        (
            "shortcut_rows | n > 1 | text ~ \"hit\" | { n } | take all",
            vec![2, 4, 6],
        ),
        (
            "shortcut_rows | n >= 0 | { n } | take 4 | skip 2 | take all",
            vec![2, 3],
        ),
    ] {
        let rows = db.run(q).unwrap().rows;
        let values: Vec<_> = rows.iter().map(|r| r["n"].as_int().unwrap()).collect();
        assert_eq!(values, expected, "{q}");
        let rows = db.run_batch(q).unwrap().to_rows();
        let values: Vec<_> = rows.iter().map(|r| r["n"].as_int().unwrap()).collect();
        assert_eq!(values, expected, "batch {q}");
    }
}

#[test]
fn absolute_timestamps_compare_identically_in_scans_and_indexes() {
    let mut db = Db::empty();
    db.run("col stamps { id: text, stamp: time }").unwrap();
    let values = [i64::MIN, -1, 0, i64::MAX];
    for (i, millis) in values.into_iter().enumerate() {
        db.run(&format!(
            r#"insert stamps {{ id: "s{i}", stamp: timestamp({millis}) }}"#
        ))
        .unwrap();
    }
    let queries = [
        "stamp == timestamp(-1)",
        "stamp < timestamp(0)",
        "stamp <= timestamp(0)",
        "stamp >= timestamp(0)",
        "stamp > timestamp(0)",
        "stamp == timestamp(-9223372036854775808)",
        "stamp == timestamp(9223372036854775807)",
    ];
    let read = |db: &mut Db, pred: &str| {
        db.run(&format!("stamps | {pred} | {{ id }} | take all"))
            .unwrap()
            .rows
            .into_iter()
            .map(|row| row["id"].text().unwrap().to_string())
            .collect::<std::collections::BTreeSet<_>>()
    };
    let scan = queries.iter().map(|q| read(&mut db, q)).collect::<Vec<_>>();
    assert_eq!(
        scan[0],
        std::collections::BTreeSet::from(["s1".to_string()])
    );
    assert_eq!(
        scan[1],
        std::collections::BTreeSet::from(["s0".to_string(), "s1".to_string()])
    );
    db.run("index stamps [stamp]").unwrap();
    for (query, expected) in queries.iter().zip(scan) {
        assert_eq!(read(&mut db, query), expected);
    }
    let prepared = db
        .prepare("insert stamps { stamp: timestamp(123456789) }")
        .unwrap();
    prepared.run(&mut db).unwrap();
    prepared.run(&mut db).unwrap();
    assert_eq!(
        db.store
            .collection("stamps")
            .iter()
            .filter(|row| row.get("stamp") == Some(&lin::Cell::Time(123456789)))
            .count(),
        2
    );
    assert!(db.prepare("insert stamps { stamp: 123 }").is_err());
}

#[test]
fn hybrid_bounded_results_match_full_ranking() {
    let mut db = lin::Db::empty();
    let records = (0..211)
        .map(|i| {
            format!(
                r#"{{id:"h-{i}",uri:"hybrid://{i}",title:"wal {}",body:"body {}"}}"#,
                i % 7,
                i % 3
            )
        })
        .collect::<Vec<_>>();
    db.run(&format!("insert docs [{}]", records.join(",")))
        .unwrap();
    for query in ["wal", "missing", "body"] {
        let all = db
            .run(&format!(r#"docs | search "{query}" | {{id}} | take all"#))
            .unwrap()
            .rows;
        for skip in [0, 1, 17, 210, 211, 250] {
            for take in [0, 1, 7, 50, 210, 211, 300] {
                let got = db
                    .run(&format!(
                        r#"docs | search "{query}" | {{id}} | skip {skip} | take {take}"#
                    ))
                    .unwrap()
                    .rows;
                let want = all
                    .iter()
                    .skip(skip)
                    .take(take)
                    .cloned()
                    .collect::<Vec<_>>();
                assert_eq!(got, want, "{query}, skip={skip}, take={take}");
            }
        }
        let default = db
            .run(&format!(r#"docs | search "{query}" | {{id}}"#))
            .unwrap()
            .rows;
        assert_eq!(default, all.iter().take(50).cloned().collect::<Vec<_>>());
    }
}
