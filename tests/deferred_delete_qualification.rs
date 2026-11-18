//! A prepared point delete on an in-memory `docs` defers index, FTS and id-map upkeep.
//! Reads through `&self` cannot flush it, and a row inserted afterwards reuses the vacated
//! tail position, so both must still see consistent data.

fn seeded(n: usize) -> lin::Db {
    let mut db = lin::Db::empty();
    db.run("index docs [wing]").unwrap();
    for i in 0..n {
        db.run(&format!(
            r#"insert docs {{ id: "d-{i}", uri: "b://{i}", wing: "rag", title: "wal doc{i}", body: "zeta alpha unique{i}" }}"#
        ))
        .unwrap();
    }
    db
}

fn ids(rows: &[lin::Row]) -> Vec<String> {
    rows.iter()
        .map(|r| r["id"].text().unwrap_or("?").to_string())
        .collect()
}

fn run_ids(db: &mut lin::Db, src: &str) -> Vec<String> {
    ids(&db.run(src).unwrap().rows)
}

#[test]
fn readonly_indexed_read_sees_a_pending_point_delete() {
    let mut db = seeded(200);
    let del = db.prepare(r#"delete docs[id == "d-7"] cas each"#).unwrap();
    db.run_prepared(&del).unwrap();

    let by_wing = ids(&db
        .run_readonly(r#"docs | wing == "rag" | take 1000 | { id }"#)
        .unwrap()
        .rows);
    assert_eq!(by_wing.len(), 199);
    assert!(
        !by_wing.iter().any(|id| id == "d-7"),
        "deleted row came back"
    );
    assert!(
        by_wing.iter().any(|id| id == "d-199"),
        "row moved into the vacated slot was lost"
    );
}

#[test]
fn row_inserted_after_a_point_delete_is_found_by_id_and_uri() {
    let mut db = seeded(200);
    let del = db.prepare(r#"delete docs[id == "d-7"] cas each"#).unwrap();
    db.run_prepared(&del).unwrap();

    db.run(
        r#"insert docs { id: "d-new", uri: "b://new", wing: "rag", title: "wal new", body: "zeta fresh" }"#,
    )
    .unwrap();

    assert_eq!(
        run_ids(&mut db, r#"docs | id == "d-new" | { id }"#),
        ["d-new"]
    );
    assert_eq!(
        run_ids(&mut db, r#"docs | uri == "b://new" | { id }"#),
        ["d-new"]
    );
    assert_eq!(
        run_ids(&mut db, r#"docs | id == "d-199" | { id }"#),
        ["d-199"]
    );
    assert!(run_ids(&mut db, r#"docs | id == "d-7" | { id }"#).is_empty());
    assert_eq!(
        run_ids(&mut db, r#"docs | wing == "rag" | take 1000 | { id }"#).len(),
        200
    );

    // A second round goes through the same machinery again.
    let del = db.prepare(r#"delete docs[id == "d-9"] cas each"#).unwrap();
    db.run_prepared(&del).unwrap();
    db.run(
        r#"insert docs { id: "d-new2", uri: "b://new2", wing: "rag", title: "wal new2", body: "zeta fresh2" }"#,
    )
    .unwrap();
    assert_eq!(
        run_ids(&mut db, r#"docs | id == "d-new2" | { id }"#),
        ["d-new2"]
    );
    assert_eq!(
        run_ids(&mut db, r#"docs | id == "d-new" | { id }"#),
        ["d-new"]
    );
    assert!(run_ids(&mut db, r#"docs | id == "d-9" | { id }"#).is_empty());
}
