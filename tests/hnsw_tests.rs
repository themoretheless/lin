use std::sync::Arc;
use lin::{Db, HashingEmbedder};

#[test]
fn test_hnsw_build_and_search() {
    let embedder = Arc::new(HashingEmbedder::new("test/64", 64));
    let mut db = Db::empty().with_embedder(embedder);

    // Insert 50 documents with distinct semantic topics
    for i in 0..50 {
        let text = format!("document category {} specific detail item {}", i / 5, i);
        db.run(&format!(
            r#"insert docs [{{ id: "doc_{i}", title: "{text}", body: "{text}" }}]"#
        ))
        .unwrap();
    }

    // Build HNSW index on docs
    db.build_hnsw("docs").unwrap();

    // Query with vector search for exact item 42
    let query_text = "document category 8 specific detail item 42";
    let res = db.run(&format!(r#"docs | search vec "{query_text}" | take 5"#)).unwrap();
    assert!(!res.rows.is_empty());
    assert_eq!(res.rows[0]["id"].text().unwrap(), "doc_42");
}

#[test]
fn test_hnsw_incremental_insert_and_deletion() {
    let embedder = Arc::new(HashingEmbedder::new("test/32", 32));
    let mut db = Db::empty().with_embedder(embedder);

    // Insert initial rows
    for i in 0..20 {
        let text = format!("chunk item {}", i);
        db.run(&format!(
            r#"insert docs [{{ id: "d_{i}", title: "{text}", body: "{text}" }}]"#
        ))
        .unwrap();
    }

    // Declare index which activates HNSW
    db.run("index docs [embedding]").unwrap();

    // Insert more rows (incremental update to HNSW)
    for i in 20..30 {
        let text = format!("chunk item {}", i);
        db.run(&format!(
            r#"insert docs [{{ id: "d_{i}", title: "{text}", body: "{text}" }}]"#
        ))
        .unwrap();
    }

    // Search for newly inserted chunk 25
    let res = db.run(r#"docs | search vec "chunk item 25" | take 1"#).unwrap();
    assert_eq!(res.rows[0]["id"].text().unwrap(), "d_25");

    // Delete a row
    let hash = db.run(r#"docs | id == "d_25" | { hash }"#).unwrap().rows[0]["hash"].text().unwrap().to_string();
    db.run(&format!(r#"delete docs[id == "d_25"] cas "{hash}""#)).unwrap();

    // Search should no longer return d_25 as top result
    let res2 = db.run(r#"docs | search vec "chunk item 25" | take 1"#).unwrap();
    assert_ne!(res2.rows[0]["id"].text().unwrap(), "d_25");
}

#[test]
fn test_hnsw_collection_isolation() {
    let embedder = Arc::new(HashingEmbedder::new("test/32", 32));
    let mut db = Db::empty().with_embedder(embedder);

    db.run(r#"insert docs [{ id: "art1", title: "quantum physics article", body: "quantum physics mechanics theory" }]"#).unwrap();
    db.run(r#"insert docs [{ id: "art2", title: "culinary recipes article", body: "culinary cooking baking recipe" }]"#).unwrap();

    db.build_hnsw("docs").unwrap();

    let res = db.run(r#"docs | search vec "quantum physics mechanics" | take 1"#).unwrap();
    assert_eq!(res.rows.len(), 1);
    assert_eq!(res.rows[0]["id"].text().unwrap(), "art1");
}
