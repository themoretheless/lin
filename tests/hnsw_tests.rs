use lin::{Db, HashingEmbedder, HnswIndex};
use std::sync::Arc;
use std::time::Instant;

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
    let res = db
        .run(&format!(r#"docs | search vec "{query_text}" | take 5"#))
        .unwrap();
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
    let res = db
        .run(r#"docs | search vec "chunk item 25" | take 1"#)
        .unwrap();
    assert_eq!(res.rows[0]["id"].text().unwrap(), "d_25");

    // Delete a row
    let hash = db.run(r#"docs | id == "d_25" | { hash }"#).unwrap().rows[0]["hash"]
        .text()
        .unwrap()
        .to_string();
    db.run(&format!(r#"delete docs[id == "d_25"] cas "{hash}""#))
        .unwrap();

    // Search should no longer return d_25 as top result
    let res2 = db
        .run(r#"docs | search vec "chunk item 25" | take 1"#)
        .unwrap();
    assert_ne!(res2.rows[0]["id"].text().unwrap(), "d_25");
}

#[test]
fn test_hnsw_collection_isolation() {
    let embedder = Arc::new(HashingEmbedder::new("test/32", 32));
    let mut db = Db::empty().with_embedder(embedder);

    db.run(r#"insert docs [{ id: "art1", title: "quantum physics article", body: "quantum physics mechanics theory" }]"#).unwrap();
    db.run(r#"insert docs [{ id: "art2", title: "culinary recipes article", body: "culinary cooking baking recipe" }]"#).unwrap();

    db.build_hnsw("docs").unwrap();

    let res = db
        .run(r#"docs | search vec "quantum physics mechanics" | take 1"#)
        .unwrap();
    assert_eq!(res.rows.len(), 1);
    assert_eq!(res.rows[0]["id"].text().unwrap(), "art1");
}

#[test]
fn test_exact_approximate_search_contract_lin_1041() {
    let embedder = Arc::new(HashingEmbedder::new("test/64", 64));
    let mut exact_db = Db::empty().with_embedder(embedder.clone());
    let mut hnsw_db = Db::empty().with_embedder(embedder.clone());

    for i in 0..40 {
        let text = format!(
            "topic {} article about science and engineering item {}",
            i % 8,
            i
        );
        let q = format!(r#"insert docs [{{ id: "d_{i}", title: "{text}", body: "{text}" }}]"#);
        exact_db.run(&q).unwrap();
        hnsw_db.run(&q).unwrap();
    }

    // hnsw_db activates HNSW index
    hnsw_db.build_hnsw("docs").unwrap();

    let query = "science and engineering topic 3 item 11";
    let exact_res = exact_db
        .run(&format!(r#"docs | search vec "{query}" | take 1"#))
        .unwrap();
    let hnsw_res = hnsw_db
        .run(&format!(r#"docs | search vec "{query}" | take 1"#))
        .unwrap();

    assert_eq!(
        exact_res.rows[0]["id"].text().unwrap(),
        hnsw_res.rows[0]["id"].text().unwrap()
    );
}

#[test]
fn test_hnsw_recall_against_exact_lin_1042() {
    let embedder = Arc::new(HashingEmbedder::new("test/64", 64));
    let mut exact_db = Db::empty().with_embedder(embedder.clone());
    let mut hnsw_db = Db::empty().with_embedder(embedder.clone());

    let n = 60;
    for i in 0..n {
        let text = format!(
            "knowledge domain {} key term {} paragraph text description",
            i % 10,
            i
        );
        let q = format!(r#"insert docs [{{ id: "doc_{i}", title: "{text}", body: "{text}" }}]"#);
        exact_db.run(&q).unwrap();
        hnsw_db.run(&q).unwrap();
    }

    hnsw_db.build_hnsw("docs").unwrap();

    let mut matches = 0;
    let queries = 10;
    for q_idx in 0..queries {
        let query = format!("knowledge domain {} key term {}", q_idx % 10, q_idx * 4);
        let exact = exact_db
            .run(&format!(r#"docs | search vec "{query}" | take 3"#))
            .unwrap();
        let approx = hnsw_db
            .run(&format!(r#"docs | search vec "{query}" | take 3"#))
            .unwrap();

        let exact_ids: std::collections::HashSet<_> = exact
            .rows
            .iter()
            .map(|r| r["id"].text().unwrap().to_string())
            .collect();
        let approx_ids: std::collections::HashSet<_> = approx
            .rows
            .iter()
            .map(|r| r["id"].text().unwrap().to_string())
            .collect();

        if !exact_ids.is_disjoint(&approx_ids) {
            matches += 1;
        }
    }

    let recall = matches as f64 / queries as f64;
    assert!(recall >= 0.90, "HNSW recall too low: {recall}");
}

#[test]
fn test_hnsw_build_latency_lin_1043() {
    let embedder = Arc::new(HashingEmbedder::new("test/32", 32));
    let mut db = Db::empty().with_embedder(embedder);

    for i in 0..100 {
        let text = format!("latency test row {i} with semantic vector keywords");
        db.run(&format!(
            r#"insert docs [{{ id: "d_{i}", title: "{text}", body: "{text}" }}]"#
        ))
        .unwrap();
    }

    let t0 = Instant::now();
    db.build_hnsw("docs").unwrap();
    let elapsed = t0.elapsed();

    // 100 items HNSW build latency must be well under 500ms
    assert!(
        elapsed.as_millis() < 500,
        "build latency exceeded: {elapsed:?}"
    );
}

#[test]
fn test_hnsw_resident_memory_lin_1044() {
    let mut index = HnswIndex::new(16, 64);
    assert_eq!(index.len(), 0);
    let initial_mem = index.resident_memory();
    assert!(initial_mem > 0);

    for i in 0..50 {
        let vec: Arc<[f32]> = (0..32).map(|x| (x + i) as f32 / 32.0).collect();
        index.insert(i, vec);
    }

    assert_eq!(index.len(), 50);
    let populated_mem = index.resident_memory();
    assert!(populated_mem > initial_mem);
}
