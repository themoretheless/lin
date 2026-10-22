use lin::{Db, Error, OpenOpts};

#[test]
fn test_transaction_commit_persists_changes() {
    let mut db = Db::empty();

    let mut tx = db.transaction().unwrap();
    tx.run(r#"insert users [{ id: "u1", email: "alice@example.com" }]"#).unwrap();
    tx.run(r#"insert users [{ id: "u2", email: "bob@example.com" }]"#).unwrap();
    
    // Visibility inside transaction:
    let count = tx.run("users | count").unwrap().rows[0]["hits"].as_i64().unwrap();
    assert_eq!(count, 2);

    tx.commit().unwrap();

    // After commit, db has both rows:
    let count = db.run("users | count").unwrap().rows[0]["hits"].as_i64().unwrap();
    assert_eq!(count, 2);
}

#[test]
fn test_transaction_rollback_discards_changes() {
    let mut db = Db::empty();
    db.run(r#"insert users [{ id: "u0", email: "init@example.com" }]"#).unwrap();

    let mut tx = db.transaction().unwrap();
    tx.run(r#"insert users [{ id: "u1", email: "alice@example.com" }]"#).unwrap();
    tx.run(r#"insert users [{ id: "u2", email: "bob@example.com" }]"#).unwrap();

    let count = tx.run("users | count").unwrap().rows[0]["hits"].as_i64().unwrap();
    assert_eq!(count, 3);

    tx.rollback().unwrap();

    // After rollback, only initial row remains:
    let count = db.run("users | count").unwrap().rows[0]["hits"].as_i64().unwrap();
    assert_eq!(count, 1);
    let email = db.run(r#"users | id == "u0""#).unwrap().rows[0]["email"].text().unwrap().to_string();
    assert_eq!(email, "init@example.com");
}

#[test]
fn test_transaction_auto_rollback_on_drop() {
    let mut db = Db::empty();
    db.run(r#"insert users [{ id: "it1", email: "it1@example.com" }]"#).unwrap();

    {
        let mut tx = db.transaction().unwrap();
        tx.run(r#"insert users [{ id: "it2", email: "it2@example.com" }]"#).unwrap();
        // tx is dropped here without commit
    }

    let count = db.run("users | count").unwrap().rows[0]["hits"].as_i64().unwrap();
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
    assert_eq!(db.run("users | count").unwrap().rows[0]["hits"].as_i64().unwrap(), 2);

    // Error case:
    let err_res: Result<(), Error> = db.transaction_fn(|tx| {
        tx.run(r#"insert users [{ id: "tx3", email: "tx3@example.com" }]"#)?;
        Err(Error::runtime("simulated failure"))
    });
    assert!(err_res.is_err());
    // tx3 should be rolled back:
    assert_eq!(db.run("users | count").unwrap().rows[0]["hits"].as_i64().unwrap(), 2);
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
        tx.run(r#"insert users [{ id: "u1", email: "u1@example.com" }]"#).unwrap();
        tx.run(r#"insert users [{ id: "u2", email: "u2@example.com" }]"#).unwrap();
        tx.commit().unwrap();
    }

    // Reopen from disk and verify durability:
    {
        let mut db = Db::open_with(&path, OpenOpts::default()).unwrap();
        let count = db.run("users | count").unwrap().rows[0]["hits"].as_i64().unwrap();
        assert_eq!(count, 2);
    }

    // Now start a transaction, don't commit, and reopen:
    {
        let mut db = Db::open_with(&path, OpenOpts::default()).unwrap();
        let mut tx = db.transaction().unwrap();
        tx.run(r#"insert users [{ id: "uncommitted", email: "uncommitted@example.com" }]"#).unwrap();
        tx.rollback().unwrap();
    }

    // Reopen and ensure uncommitted never made it to WAL:
    {
        let mut db = Db::open_with(&path, OpenOpts::default()).unwrap();
        let count = db.run("users | count").unwrap().rows[0]["hits"].as_i64().unwrap();
        assert_eq!(count, 2);
        assert_eq!(db.run(r#"users | id == "uncommitted""#).unwrap().rows.len(), 0);
    }

    let _ = std::fs::remove_dir_all(&path);
}
