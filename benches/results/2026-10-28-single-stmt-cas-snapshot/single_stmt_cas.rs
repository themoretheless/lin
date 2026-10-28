use lin::{Cell, Db};

fn seeded() -> Db {
    let mut db = Db::empty();
    db.run(r#"insert docs [{id:"a",body:"before a",wing:"rag",title:"A"},{id:"b",body:"before b",wing:"rag",title:"B"}]"#).unwrap();
    db.run("index docs [wing]").unwrap();
    db
}
fn snapshot(db: &Db) -> Vec<u8> {
    rmp_serde::to_vec_named(db.store.collection("docs")).unwrap()
}
#[test]
fn single_cas_each_checks_all_rows_and_retains_held_results() {
    let mut db = seeded();
    let held = db.run("docs | take all").unwrap();
    let before = rmp_serde::to_vec_named(&held.rows).unwrap();
    let update = db
        .prepare(r#"update docs[wing=="rag"] cas each {wing:"sys"}"#)
        .unwrap();
    assert_eq!(db.run_prepared(&update).unwrap().done.n, 2);
    assert_eq!(rmp_serde::to_vec_named(&held.rows).unwrap(), before);
    let count = db.run(r#"docs | wing=="sys" | count"#).unwrap();
    assert_eq!(
        count.rows[0].values().next().and_then(Cell::as_int),
        Some(2)
    );
    assert_eq!(
        db.run(r#"delete docs[wing=="sys"] cas each"#)
            .unwrap()
            .done
            .n,
        2
    );
    assert!(db.store.collection("docs").is_empty());
}
#[test]
fn multi_statement_cas_each_still_rejects_prior_writes_and_restores_everything() {
    for first in [
        r#"update docs[id=="a"] cas each {wing:"sys"}"#.to_owned(),
        format!(
            r#"update docs[id=="a"] cas "{}" {{wing:"sys"}}"#,
            seeded().store.get_by_id("docs", "a").unwrap()["hash"]
                .text()
                .unwrap()
        ),
    ] {
        let mut db = seeded();
        let before = snapshot(&db);
        let generation = db.stats().r#gen;
        let error = db
            .run(&format!("{first}\ndelete docs[id==\"a\"] cas each"))
            .unwrap_err();
        assert!(error.to_string().contains("changed mid-pack"), "{error}");
        assert_eq!(snapshot(&db), before);
        assert_eq!(db.stats().r#gen, generation);
        assert_eq!(
            db.run(r#"docs | wing=="rag" | count"#).unwrap().rows[0]
                .values()
                .next()
                .and_then(Cell::as_int),
            Some(2)
        );
    }
}
#[test]
fn single_cas_each_unique_failure_rolls_back_complete_rows_and_indexes() {
    let mut db = seeded();
    db.run("index docs unique [title]").unwrap();
    let before = snapshot(&db);
    let generation = db.stats().r#gen;
    let error = db
        .run(r#"update docs[wing=="rag"] cas each {title:"collision"}"#)
        .unwrap_err();
    assert!(error.to_string().contains("unique index"), "{error}");
    assert_eq!(snapshot(&db), before);
    assert_eq!(db.stats().r#gen, generation);
    for title in ["A", "B"] {
        assert_eq!(
            db.run(&format!(r#"docs | title=="{title}" | count"#))
                .unwrap()
                .rows[0]
                .values()
                .next()
                .and_then(Cell::as_int),
            Some(1)
        );
    }
}
#[test]
fn repeated_single_cas_each_in_transaction_preserves_commit_and_rollback_contracts() {
    for commit in [false, true] {
        let mut db = seeded();
        let before = snapshot(&db);
        let generation = db.stats().r#gen;
        let mut tx = db.transaction().unwrap();
        tx.run(r#"update docs[id=="a"] cas each {wing:"sys"}"#)
            .unwrap();
        tx.run(r#"update docs[id=="a"] cas each {title:"changed"}"#)
            .unwrap();
        if commit {
            tx.commit().unwrap();
        } else {
            tx.rollback().unwrap();
        }
        if commit {
            assert_eq!(db.stats().r#gen, generation + 1);
            assert_eq!(
                db.store.get_by_id("docs", "a").unwrap()["title"].text(),
                Some("changed")
            );
        } else {
            assert_eq!(db.stats().r#gen, generation);
            assert_eq!(snapshot(&db), before);
        }
    }
}
