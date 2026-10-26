use lin::{Db, Quotas};

#[test]
fn native_update_and_delete_charge_transaction_journal() {
    for statement in [
        r#"update docs[id == "a"] cas each { wing: "sys" }"#,
        r#"delete docs[id == "a"] cas each"#,
    ] {
        let mut db = Db::empty().with_quotas(Quotas {
            max_journal_bytes: 1,
            ..Default::default()
        });
        db.run(r#"insert docs { id: "a", title: "original", wing: "rag" }"#)
            .unwrap();
        let before = db.store.collection("docs").to_vec();
        {
            let mut tx = db.transaction().unwrap();
            let error = tx.run(statement).unwrap_err();
            assert!(error.to_string().contains("max_journal_bytes"), "{error}");
        }
        assert_eq!(db.store.collection("docs"), before.as_slice());
        assert_eq!(
            db.run(r#"docs | id == "a" | { wing }"#).unwrap().rows[0]["wing"].text(),
            Some("rag")
        );
    }
}

#[test]
fn empty_final_query_preserves_native_affected_count() {
    let mut db = Db::empty();
    db.run(r#"insert docs [{ id: "a", title: "first", wing: "rag" }, { id: "b", title: "second", wing: "rag" }]"#)
        .unwrap();
    let before_gen = db.stats().r#gen;
    let result = db.run(r#"update docs[id == "a"] cas each { wing: "sys" }
delete docs[id == "b"] cas each
docs | id == "missing""#).unwrap();
    assert!(result.rows.is_empty());
    assert_eq!(result.done.n, 2);
    assert_eq!(result.done.r#gen, before_gen + 1);
    assert_eq!(db.store.collection("docs").len(), 1);
}
