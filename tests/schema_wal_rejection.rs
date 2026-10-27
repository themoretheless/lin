use lin::{Db, Quotas};
#[test]
fn rejected_schema_wal_preserves_catalog_hash_after_close() {
    let path = std::env::temp_dir().join(format!("lin-schema-reject-{}", std::process::id()));
    std::fs::create_dir(&path).unwrap();
    let mut db = Db::open(&path).unwrap().with_quotas(Quotas {
        max_query_bytes: 64 * 1024 * 1024,
        ..Default::default()
    });
    db.run(r#"insert docs {id:"seed",body:"seed"}"#).unwrap();
    db.checkpoint().unwrap();
    let generation = db.store.r#gen;
    let name = "x".repeat(17 * 1024 * 1024);
    let query = format!("col {name} {{id: text}}");
    let error = db.run(&query).unwrap_err();
    assert!(
        error.to_string().contains("16MiB"),
        "unexpected error: {error}"
    );
    assert_eq!(db.store.r#gen, generation);
    assert_eq!(db.scalar("docs | count").unwrap(), lin::Cell::Int(1));
    let close = db.close();
    assert!(close.is_ok());
    drop(db);
    let reopened = Db::open(&path);
    let message = reopened.as_ref().err().map(ToString::to_string);
    println!(
        "{}",
        serde_json::json!({"wal_rejected":true,"generation_unchanged":true,"close_ok":close.is_ok(),"reopen_ok":reopened.is_ok(),"reopen_error":message})
    );
    assert!(
        message.is_none(),
        "reopen after rejected schema: {message:?}"
    );
    let mut reopened = reopened.unwrap();
    assert_eq!(reopened.scalar("docs | count").unwrap(), lin::Cell::Int(1));
    reopened
        .run(r#"insert docs {id:"next",body:"after"}"#)
        .unwrap();
    assert_eq!(reopened.store.r#gen, generation + 1);
    reopened.close().unwrap();
    drop(reopened);
    let mut final_db = Db::open(&path).unwrap();
    assert_eq!(final_db.scalar("docs | count").unwrap(), lin::Cell::Int(2));
    drop(final_db);
    std::fs::remove_dir_all(&path).unwrap();
}
