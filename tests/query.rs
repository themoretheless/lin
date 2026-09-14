use futures_util::StreamExt;
use lin::query::pred;
use lin::{
    AsyncDb, Cell, Db, Duration, DurUnit, LinRow, MatchPath, Queryable, RowExt, Value,
};

#[derive(Debug, LinRow)]
#[lin(collection = "docs")]
struct DocTitle {
    id: String,
    title: String,
}

#[test]
fn fluent_filter_select_take() {
    let mut db = Db::fixture();
    let rows = db
        .from("docs")
        .filter(pred::eq("wing", "rag"))
        .select(["id", "title"])
        .take(5)
        .to_vec()
        .unwrap();
    assert!(!rows.is_empty());
    assert!(rows.len() <= 5);
    assert!(rows[0].contains_key("id"));
    assert!(rows[0].contains_key("title"));
    assert!(!rows[0].contains_key("body"));
}

#[test]
fn run_stmt_matches_string() {
    let mut db = Db::fixture();
    let via_str = db
        .run(r#"docs | wing == "rag" | { id, title } | take 3"#)
        .unwrap();
    let q = Queryable::from("docs")
        .filter(pred::eq("wing", "rag"))
        .select(["id", "title"])
        .take(3);
    let via_stmt = db.run_stmt(q.stmt()).unwrap();
    assert_eq!(via_str.done.n, via_stmt.done.n);
    assert_eq!(via_str.rows.len(), via_stmt.rows.len());
}

#[test]
fn typed_from_row() {
    let mut db = Db::fixture();
    let docs: Vec<DocTitle> = db
        .from("docs")
        .filter(pred::eq("wing", "rag"))
        .select_row::<DocTitle>()
        .take(10)
        .to_vec_typed()
        .unwrap();
    assert!(!docs.is_empty());
    assert!(!docs[0].id.is_empty());
    assert!(!docs[0].title.is_empty());
}

#[test]
fn from_typed_uses_collection() {
    let mut db = Db::fixture();
    let docs: Vec<DocTitle> = db
        .from_typed::<DocTitle>()
        .unwrap()
        .take(3)
        .to_vec_typed()
        .unwrap();
    assert!(!docs.is_empty());
}

#[test]
fn pred_and_with_ago() {
    let mut db = Db::fixture();
    let rows = db
        .from("docs")
        .filter(
            pred::eq("wing", "rag").and(pred::gt(
                "ts",
                Value::NowMinus(Duration {
                    n: 3650,
                    unit: DurUnit::Day,
                }),
            )),
        )
        .select(["id", "title"])
        .take(5)
        .to_vec()
        .unwrap();
    assert!(!rows.is_empty());
}

#[test]
fn cell_match_via_row_ext() {
    let mut db = Db::fixture();
    let rows = db
        .from("docs")
        .filter(pred::eq("wing", "rag"))
        .select(["id", "title"])
        .take(1)
        .to_vec()
        .unwrap();
    let row = &rows[0];
    match row.cell("title") {
        Cell::Text(s) => assert!(!s.is_empty()),
        other => panic!("expected Text, got {other:?}"),
    }
    assert!(matches!(row.cell("missing_col"), Cell::Null));
    assert!(row.cell("missing_col").is_null());
}

#[test]
fn skip_offset_paging() {
    let mut db = Db::fixture();
    let all = db
        .from("docs")
        .filter(pred::eq("wing", "rag"))
        .select(["id", "title"])
        .take(10)
        .to_vec()
        .unwrap();
    assert!(all.len() >= 2);

    let page = db
        .from("docs")
        .filter(pred::eq("wing", "rag"))
        .select(["id", "title"])
        .skip(1)
        .take(1)
        .to_vec()
        .unwrap();
    assert_eq!(page.len(), 1);
    assert_eq!(page[0].get("id"), all[1].get("id"));

    let via_offset = db
        .run(r#"docs | wing == "rag" | { id, title } | offset 1 | take 1"#)
        .unwrap();
    assert_eq!(via_offset.rows.len(), 1);
    assert_eq!(via_offset.rows[0].get("id"), page[0].get("id"));
}

#[test]
fn lazy_cursor_matches_to_vec() {
    let mut db = Db::fixture();
    let q = Queryable::from("docs")
        .filter(pred::eq("wing", "rag"))
        .select(["id", "title"])
        .take(5);
    let via_vec = q.to_vec(&mut db).unwrap();
    let cur = q.cursor(&db).unwrap();
    assert!(cur.is_lazy());
    let via_cur: Vec<_> = cur.map(|r| r.unwrap()).collect();
    assert_eq!(via_cur, via_vec);
}

#[test]
fn explain_and_after_keyset() {
    let mut db = Db::fixture();
    let q = Queryable::from("docs")
        .filter(pred::eq("wing", "rag"))
        .select(["id", "title"])
        .skip(1)
        .take(2);
    let plan = q.explain(&mut db).unwrap();
    assert!(
        plan.contains("Take") || plan.contains("Skip") || plan.contains("Filter"),
        "{plan}"
    );

    // Keyset on numeric field (text/id columns disallow `>` in check).
    let page1 = db
        .from("orders")
        .select(["id", "total"])
        .sort("total", false)
        .take(1)
        .to_vec()
        .unwrap();
    assert_eq!(page1.len(), 1);
    let total = page1[0].get("total").and_then(|c| c.as_f64()).unwrap();
    let page2 = db
        .from("orders")
        .after("total", total)
        .select(["id", "total"])
        .sort("total", false)
        .take(1)
        .to_vec()
        .unwrap();
    assert_eq!(page2.len(), 1);
    let total2 = page2[0].get("total").and_then(|c| c.as_f64()).unwrap();
    assert!(total2 > total);
}

#[test]
fn lazy_join_cursor() {
    let mut db = Db::fixture();
    let q = Queryable::from("orders")
        .filter(pred::gt("total", 100))
        .join("users", "user_id")
        .select(["id", "users.email", "total"])
        .take(10);
    let via_vec = q.to_vec(&mut db).unwrap();
    assert_eq!(via_vec.len(), 1);
    let cur = q.cursor(&db).unwrap();
    assert!(cur.is_lazy(), "FK join should be lazy");
    let via_cur: Vec<_> = cur.map(|r| r.unwrap()).collect();
    assert_eq!(via_cur, via_vec);
}

#[test]
fn join_run_batch_matches_rows() {
    let mut db = Db::fixture();
    let src = r#"orders | total > 100 | join users on user_id | { id, users.email, total } | take all"#;
    let rows = db.run(src).unwrap();
    let batch = db.run_batch(src).unwrap();
    assert_eq!(batch.n(), rows.done.n);
    assert_eq!(batch.to_rows(), rows.rows);
}

#[test]
fn fluent_hop_and_match() {
    let mut db = Db::fixture();
    let ins = db
        .run(
            r#"insert docs { uri: "raw://n/fluent-hop", title: "fluent hop", layer: "raw" } with edge wikilink -> page "wiki://rag-overview""#,
        )
        .unwrap();
    let id = ins.rows[0].get("id").and_then(|c| c.text()).unwrap();

    let via_str = db
        .run(&format!(
            r#"docs | id == "{id}" | hop wikilink | {{ id, title }}"#
        ))
        .unwrap();
    let via_fluent = Queryable::from("docs")
        .filter(pred::eq("id", id))
        .hop("wikilink")
        .select(["id", "title"])
        .to_vec(&mut db)
        .unwrap();
    assert_eq!(via_fluent.len(), via_str.rows.len());

    let via_match_str = db
        .run(&format!(
            r#"docs | id == "{id}" | match -wikilink-> b | {{ id, b.title }}"#
        ))
        .unwrap();
    let via_match = Queryable::from("docs")
        .filter(pred::eq("id", id))
        .match_path(MatchPath::fwd("wikilink", "b"))
        .select(["id", "b.title"])
        .to_vec(&mut db)
        .unwrap();
    assert_eq!(via_match.len(), via_match_str.rows.len());

    let cur = Queryable::from("docs")
        .filter(pred::eq("id", id))
        .hop("wikilink")
        .select(["id", "title"])
        .cursor(&db)
        .unwrap();
    assert!(!cur.is_lazy(), "hop is buffered, not lazy");
}

#[tokio::test]
async fn async_offload_to_vec() {
    let db = AsyncDb::new(Db::fixture());
    let q = Queryable::from("docs")
        .filter(pred::eq("wing", "rag"))
        .select(["id", "title"])
        .take(5);
    let rows = q.clone().to_vec_async(&db).await.unwrap();
    assert!(!rows.is_empty());
    assert!(rows.len() <= 5);

    let typed: Vec<DocTitle> = Queryable::from("docs")
        .filter(pred::eq("wing", "rag"))
        .select_row::<DocTitle>()
        .take(5)
        .to_vec_typed_async(&db)
        .await
        .unwrap();
    assert_eq!(typed.len(), rows.len());
}

#[tokio::test]
async fn async_stream_matches_to_vec() {
    let db = AsyncDb::new(Db::fixture());
    let q = Queryable::from("docs")
        .filter(pred::eq("wing", "rag"))
        .select(["id", "title"])
        .take(5);
    let via_vec = q.clone().to_vec_async(&db).await.unwrap();
    let mut stream = q.to_stream_async(&db).await;
    let mut via_stream = Vec::new();
    while let Some(item) = stream.next().await {
        via_stream.push(item.unwrap());
    }
    assert_eq!(via_stream.len(), via_vec.len());
    assert_eq!(via_stream, via_vec);

    let mut typed_stream = Queryable::from("docs")
        .filter(pred::eq("wing", "rag"))
        .select_row::<DocTitle>()
        .take(5)
        .to_stream_typed_async::<DocTitle>(&db)
        .await;
    let mut typed = Vec::new();
    while let Some(item) = typed_stream.next().await {
        typed.push(item.unwrap());
    }
    assert_eq!(typed.len(), via_vec.len());
}
