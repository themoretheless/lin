use lin::Db;

fn text<'a>(row: &'a lin::Row, k: &str) -> &'a str {
    row.get(k).and_then(|c| c.text()).unwrap_or("")
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
