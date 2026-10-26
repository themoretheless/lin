use lin::{Db, Error, OpenOpts};

#[test]
fn test_transaction_commit_persists_changes() {
    let mut db = Db::empty();

    let mut tx = db.transaction().unwrap();
    tx.run(r#"insert users [{ id: "u1", email: "alice@example.com" }]"#)
        .unwrap();
    tx.run(r#"insert users [{ id: "u2", email: "bob@example.com" }]"#)
        .unwrap();

    // Visibility inside transaction:
    let count = tx.run("users | count").unwrap().rows[0]["hits"]
        .as_i64()
        .unwrap();
    assert_eq!(count, 2);

    tx.commit().unwrap();

    // After commit, db has both rows:
    let count = db.run("users | count").unwrap().rows[0]["hits"]
        .as_i64()
        .unwrap();
    assert_eq!(count, 2);
}

#[test]
fn test_transaction_rollback_discards_changes() {
    let mut db = Db::empty();
    db.run(r#"insert users [{ id: "u0", email: "init@example.com" }]"#)
        .unwrap();

    let mut tx = db.transaction().unwrap();
    tx.run(r#"insert users [{ id: "u1", email: "alice@example.com" }]"#)
        .unwrap();
    tx.run(r#"insert users [{ id: "u2", email: "bob@example.com" }]"#)
        .unwrap();

    let count = tx.run("users | count").unwrap().rows[0]["hits"]
        .as_i64()
        .unwrap();
    assert_eq!(count, 3);

    tx.rollback().unwrap();

    // After rollback, only initial row remains:
    let count = db.run("users | count").unwrap().rows[0]["hits"]
        .as_i64()
        .unwrap();
    assert_eq!(count, 1);
    let email = db.run(r#"users | id == "u0""#).unwrap().rows[0]["email"]
        .text()
        .unwrap()
        .to_string();
    assert_eq!(email, "init@example.com");
}

#[test]
fn test_transaction_auto_rollback_on_drop() {
    let mut db = Db::empty();
    db.run(r#"insert users [{ id: "it1", email: "it1@example.com" }]"#)
        .unwrap();

    {
        let mut tx = db.transaction().unwrap();
        tx.run(r#"insert users [{ id: "it2", email: "it2@example.com" }]"#)
            .unwrap();
        // tx is dropped here without commit
    }

    let count = db.run("users | count").unwrap().rows[0]["hits"]
        .as_i64()
        .unwrap();
    assert_eq!(count, 1);
    assert_eq!(db.run(r#"users | id == "it2""#).unwrap().rows.len(), 0);
}

#[test]
fn test_transaction_fn_helper_success_and_error() {
    let mut db = Db::empty();

    // Success case:
    let res: Result<i64, Error> = db.transaction_fn(|tx| {
        tx.run(r#"insert users [{ id: "tx1", email: "tx1@example.com" }]"#)?;
        tx.run(r#"insert users [{ id: "tx2", email: "tx2@example.com" }]"#)?;
        Ok(300)
    });
    assert_eq!(res.unwrap(), 300);
    assert_eq!(
        db.run("users | count").unwrap().rows[0]["hits"]
            .as_i64()
            .unwrap(),
        2
    );

    // Error case:
    let err_res: Result<(), Error> = db.transaction_fn(|tx| {
        tx.run(r#"insert users [{ id: "tx3", email: "tx3@example.com" }]"#)?;
        Err(Error::runtime("simulated failure"))
    });
    assert!(err_res.is_err());
    // tx3 should be rolled back:
    assert_eq!(
        db.run("users | count").unwrap().rows[0]["hits"]
            .as_i64()
            .unwrap(),
        2
    );
}

#[test]
fn test_transaction_durable_commit_survives_reopen() {
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("lin-tx-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&path);

    {
        let mut db = Db::open_with(&path, OpenOpts::default()).unwrap();

        let mut tx = db.transaction().unwrap();
        tx.run(r#"insert users [{ id: "u1", email: "u1@example.com" }]"#)
            .unwrap();
        tx.run(r#"insert users [{ id: "u2", email: "u2@example.com" }]"#)
            .unwrap();
        tx.commit().unwrap();
    }

    // Reopen from disk and verify durability:
    {
        let mut db = Db::open_with(&path, OpenOpts::default()).unwrap();
        let count = db.run("users | count").unwrap().rows[0]["hits"]
            .as_i64()
            .unwrap();
        assert_eq!(count, 2);
    }

    // Now start a transaction, don't commit, and reopen:
    {
        let mut db = Db::open_with(&path, OpenOpts::default()).unwrap();
        let mut tx = db.transaction().unwrap();
        tx.run(r#"insert users [{ id: "uncommitted", email: "uncommitted@example.com" }]"#)
            .unwrap();
        tx.rollback().unwrap();
    }

    // Reopen and ensure uncommitted never made it to WAL:
    {
        let mut db = Db::open_with(&path, OpenOpts::default()).unwrap();
        let count = db.run("users | count").unwrap().rows[0]["hits"]
            .as_i64()
            .unwrap();
        assert_eq!(count, 2);
        assert_eq!(
            db.run(r#"users | id == "uncommitted""#).unwrap().rows.len(),
            0
        );
    }

    let _ = std::fs::remove_dir_all(&path);
}

#[test]
fn test_rollback_embeddings_lin_0205() {
    let mut db = Db::empty();
    db.run(r#"insert docs { id: "d1", title: "Original document about artificial intelligence" }"#)
        .unwrap();
    db.build_hnsw("docs").unwrap();

    let initial_results = db
        .run(r#"docs | search vec "machine learning" | take 1"#)
        .unwrap();
    assert_eq!(initial_results.rows.len(), 1);

    {
        let mut tx = db.transaction().unwrap();
        tx.run(r#"insert docs { id: "d2", title: "Completely unrelated text about gardening and plants" }"#).unwrap();
        // tx rolled back
    }

    // Embeddings & HNSW index must remain consistent with initial state
    let post_rollback = db
        .run(r#"docs | search vec "machine learning" | take 5"#)
        .unwrap();
    assert_eq!(post_rollback.rows.len(), 1);
    assert_eq!(post_rollback.rows[0]["id"].text().unwrap(), "d1");
}

#[test]
fn test_rollback_graph_edges_lin_0206() {
    let mut db = Db::empty();
    db.run(r#"insert docs { id: "n1", title: "Node 1" }"#)
        .unwrap();
    db.run(r#"insert docs { id: "n2", title: "Node 2" }"#)
        .unwrap();
    db.run(r#"insert docs { id: "n3", title: "Node 3" }"#)
        .unwrap();

    assert_eq!(db.store.edges.len(), 0);

    {
        let mut tx = db.transaction().unwrap();
        tx.run(r#"append edge wikilink "n1" -> "n2""#).unwrap();
        tx.run(r#"append edge wikilink "n2" -> "n3""#).unwrap();
        assert_eq!(tx.db().store.edges.len(), 2);
        // Abort
    }

    assert_eq!(db.store.edges.len(), 0);
}

#[test]
fn test_rollback_generated_ids_lin_0207() {
    let mut db = Db::empty();
    let initial_res = db.run(r#"insert docs { title: "Item A" }"#).unwrap();
    let id_a = initial_res.rows[0]["id"].text().unwrap().to_string();

    {
        let mut tx = db.transaction().unwrap();
        let r1 = tx.run(r#"insert docs { title: "Item B" }"#).unwrap();
        let r2 = tx.run(r#"insert docs { title: "Item C" }"#).unwrap();
        assert_ne!(r1.rows[0]["id"].text().unwrap(), id_a);
        assert_ne!(r2.rows[0]["id"].text().unwrap(), id_a);
        tx.rollback().unwrap();
    }

    // After rollback, the ID sequence is rolled back so no ID drift or ghost IDs remain
    let next_res = db.run(r#"insert docs { title: "Item D" }"#).unwrap();
    let id_d = next_res.rows[0]["id"].text().unwrap().to_string();
    assert_ne!(id_a, id_d);
    assert_eq!(
        db.run("docs | count").unwrap().rows[0]["hits"]
            .as_i64()
            .unwrap(),
        2
    );
}

#[test]
fn test_rollback_catalog_changes_lin_0208() {
    let mut db = Db::empty();
    assert!(db.catalog.collection("temp_catalog_test").is_none());

    {
        let mut tx = db.transaction().unwrap();
        tx.run("col temp_catalog_test { title: text, score: i64 }")
            .unwrap();
        assert!(tx.run("temp_catalog_test | count").is_ok());
        tx.rollback().unwrap();
    }

    // Catalog change must be cleanly discarded
    assert!(db.catalog.collection("temp_catalog_test").is_none());
    assert!(db.run("temp_catalog_test | count").is_err());
}

#[test]
fn test_mixed_insert_update_delete_rollback_lin_0209() {
    let mut db = Db::empty();
    db.run(r#"insert docs { id: "d1", title: "Title 1" }"#)
        .unwrap();
    db.run(r#"insert docs { id: "d2", title: "Title 2" }"#)
        .unwrap();

    let mut tx = db.transaction().unwrap();
    // 1. Insert new
    tx.run(r#"insert docs { id: "d3", title: "Title 3" }"#)
        .unwrap();
    // 2. Delete existing
    tx.run(r#"delete docs[id == "d1"] cas each"#).unwrap();
    // 3. Update existing
    tx.run(r#"update docs[id == "d2"] cas each { title: "Title 2 Updated" }"#)
        .unwrap();

    assert_eq!(
        tx.run("docs | count").unwrap().rows[0]["hits"]
            .as_i64()
            .unwrap(),
        2
    );
    tx.rollback().unwrap();

    // After rollback: exactly d1 and d2 exist with their original values
    let docs = db.run("docs | sort id").unwrap();
    assert_eq!(docs.rows.len(), 2);
    assert_eq!(docs.rows[0]["id"].text().unwrap(), "d1");
    assert_eq!(docs.rows[0]["title"].text().unwrap(), "Title 1");
    assert_eq!(docs.rows[1]["id"].text().unwrap(), "d2");
    assert_eq!(docs.rows[1]["title"].text().unwrap(), "Title 2");
}

#[test]
fn test_group_commit_failure_atomicity_lin_0210() {
    let mut db = Db::empty();
    db.run(r#"insert docs { id: "init", title: "Initial" }"#)
        .unwrap();

    let res: Result<(), Error> = db.transaction_fn(|tx| {
        tx.run(r#"insert docs { id: "gc1", title: "Group 1" }"#)?;
        tx.run(r#"insert docs { id: "gc2", title: "Group 2" }"#)?;
        // Validation failure
        Err(Error::runtime(
            "simulated validation failure in group commit",
        ))
    });

    assert!(res.is_err());
    // Both gc1 and gc2 must be absent; atomicity strictly preserved
    assert_eq!(
        db.run("docs | count").unwrap().rows[0]["hits"]
            .as_i64()
            .unwrap(),
        1
    );
    assert_eq!(db.run(r#"docs | id == "gc1""#).unwrap().rows.len(), 0);
    assert_eq!(db.run(r#"docs | id == "gc2""#).unwrap().rows.len(), 0);
}
