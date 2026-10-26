use lin::Db;

fn seed(db: &mut Db) {
    db.run(r#"index docs unique [title] where wing == "rag""#)
        .unwrap();
    db.run(
        r#"insert docs [
        { id: "a", uri: "partial://a", title: "shared", wing: "rag" },
        { id: "b", uri: "partial://b", title: "shared", wing: "sys" },
        { id: "c", uri: "partial://c", title: "shared", wing: "sys" }
    ]"#,
    )
    .unwrap();
}

fn seeded() -> Db {
    let mut db = Db::empty();
    seed(&mut db);
    db
}

fn members(db: &Db) -> Vec<usize> {
    let index = db
        .store
        .indexes
        .values()
        .find(|i| i.def.collection == "docs" && i.def.fields == ["title"])
        .unwrap();
    let mut reverse = index.reverse.keys().copied().collect::<Vec<_>>();
    reverse.sort_unstable();
    let mut forward = index
        .forward
        .values()
        .flat_map(|p| p.to_indices())
        .collect::<Vec<_>>();
    forward.sort_unstable();
    assert_eq!(forward, reverse, "forward and reverse membership agree");
    reverse
}

#[test]
fn leaving_partial_predicate_releases_unique_key() {
    let mut db = seeded();
    assert_eq!(members(&db), [0]);
    db.run(r#"update docs[id == "a"] cas each { wing: "sys" }"#)
        .unwrap();
    assert!(
        members(&db).is_empty(),
        "row no longer belongs to the partial index"
    );
    db.run(r#"update docs[id == "b"] cas each { wing: "rag" }"#)
        .unwrap();
    assert_eq!(members(&db), [1]);
    let result = db
        .run(r#"docs | title == "shared" and wing == "rag" | { id } | take all"#)
        .unwrap();
    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0]["id"].text(), Some("b"));
}

#[test]
fn leaving_partial_predicate_with_changed_key_removes_old_entry() {
    let mut db = seeded();
    db.run(r#"update docs[id == "a"] cas each { wing: "sys", title: "changed" }"#)
        .unwrap();
    assert!(members(&db).is_empty());
    db.run(r#"update docs[id == "b"] cas each { wing: "rag" }"#)
        .unwrap();
    assert_eq!(members(&db), [1]);
}

#[test]
fn failed_transfer_rolls_back_partial_membership_and_rows() {
    let mut db = seeded();
    let before = db.store.collection("docs").to_vec();
    let generation = db.stats().r#gen;
    let error = db
        .run_group([
            r#"update docs[id == "a"] cas each { wing: "sys" }"#,
            r#"update docs[id == "b"] cas each { wing: "rag" }"#,
            r#"update docs[id == "c"] cas each { wing: "rag" }"#,
        ])
        .unwrap_err();
    assert!(error.to_string().contains("unique"), "{error}");
    assert_eq!(db.store.collection("docs"), before.as_slice());
    assert_eq!(db.stats().r#gen, generation);
    assert_eq!(members(&db), [0]);
}

#[test]
fn partial_transfer_survives_wal_replay_and_checkpoint() {
    let path = std::env::temp_dir().join(format!(
        "lin-partial-membership-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let mut db = Db::open(&path).unwrap();
    seed(&mut db);
    db.run(r#"update docs[id == "a"] cas each { wing: "sys" }"#)
        .unwrap();
    db.run(r#"update docs[id == "b"] cas each { wing: "rag" }"#)
        .unwrap();
    assert_eq!(members(&db), [1]);
    let frames = db.export_wal_since(0).unwrap();
    let mut replica = Db::empty();
    assert_eq!(replica.apply_wal(&frames).unwrap(), 4);
    assert_eq!(
        replica.store.collection("docs"),
        db.store.collection("docs")
    );
    assert_eq!(members(&replica), [1]);
    db.checkpoint().unwrap();
    drop(db);
    let mut reopened = Db::open(&path).unwrap();
    assert_eq!(members(&reopened), [1]);
    // A conflicting entrant must still be rejected after recovery.
    assert!(
        reopened
            .run(r#"update docs[id == "c"] cas each { wing: "rag" }"#)
            .is_err()
    );
    assert_eq!(members(&reopened), [1]);
    drop(reopened);
    std::fs::remove_dir_all(path).unwrap();
}
