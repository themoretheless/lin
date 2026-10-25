use lin::Db;

#[test]
fn test_readdb_visibility_contract_lin_1321() {
    let mut db = Db::empty();
    db.run(r#"insert docs { id: "d1", title: "Doc 1" }"#)
        .unwrap();

    let reader = db.reader();

    // Mutate primary database
    db.run(r#"insert docs { id: "d2", title: "Doc 2" }"#)
        .unwrap();

    // Reader must NOT see d2
    let res = reader.run("docs | count").unwrap();
    assert_eq!(res.done.n, 1);

    let res_items = reader.run("docs | { id }").unwrap();
    assert_eq!(res_items.rows.len(), 1);
    assert_eq!(res_items.rows[0]["id"].text().unwrap(), "d1");
}

#[test]
fn test_snapshot_generation_isolation_lin_1322() {
    let mut db = Db::empty();
    db.run(r#"insert docs { id: "d1", title: "Doc 1" }"#)
        .unwrap();

    let reader = db.reader();
    let initial_gen = reader.r#gen();

    // Advance generation on primary
    db.run(r#"insert docs { id: "d2", title: "Doc 2" }"#)
        .unwrap();
    db.run(r#"insert docs { id: "d3", title: "Doc 3" }"#)
        .unwrap();
    assert!(db.r#gen() > initial_gen);

    // Reader generation must remain strictly unchanged
    assert_eq!(reader.r#gen(), initial_gen);
}

#[test]
fn test_snapshot_index_consistency_lin_1323() {
    let mut db = Db::empty();
    db.run("index docs [wing]").unwrap();
    let ins = db
        .run(r#"insert docs { id: "d1", uri: "raw://1", wing: "stable", title: "Original", layer: "wiki", body: "x" }"#)
        .unwrap();
    let hash = ins.rows[0]
        .get("hash")
        .and_then(|c| c.text())
        .unwrap()
        .to_string();

    let reader = db.reader();

    // Mutate primary after snapshot taken
    db.run(&format!(
        r#"update docs[id == "d1"] cas "{hash}" {{ wing: "changed" }}"#
    ))
    .unwrap();

    // Primary sees new value
    let p_res = db.run(r#"docs | wing == "changed""#).unwrap();
    assert_eq!(p_res.rows.len(), 1);

    // Reader snapshot index seek returns original value
    let r_res = reader.run(r#"docs | wing == "stable""#).unwrap();
    assert_eq!(r_res.rows.len(), 1);
    assert_eq!(r_res.rows[0]["id"].text().unwrap(), "d1");
}

#[test]
fn test_snapshot_fts_consistency_lin_1324() {
    let mut db = Db::empty();
    db.run(r#"insert docs { id: "d1", title: "Rust concurrency", body: "memory safety" }"#)
        .unwrap();

    let reader = db.reader();

    // Delete row on primary
    let hash = db.run(r#"docs | id == "d1" | { hash }"#).unwrap().rows[0]["hash"]
        .text()
        .unwrap()
        .to_string();
    db.run(&format!(r#"delete docs[id == "d1"] cas "{hash}""#))
        .unwrap();

    // Primary no longer finds it
    let p_res = db.run(r#"docs | search lex "concurrency""#).unwrap();
    assert!(p_res.rows.is_empty());

    // Reader snapshot FTS still reliably finds it
    let r_res = reader.run(r#"docs | search lex "concurrency""#).unwrap();
    assert_eq!(r_res.rows.len(), 1);
    assert_eq!(r_res.rows[0]["id"].text().unwrap(), "d1");
}
