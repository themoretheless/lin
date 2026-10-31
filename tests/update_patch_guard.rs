use lin::Db;

#[test]
fn update_patch_preserves_explicit_and_derived_hash_guards() {
    let mut db = Db::empty();
    db.run(r#"insert docs {id:"guarded",uri:"u://guarded",body:"before",layer:"wiki"}"#).unwrap();
    db.run(r#"guard docs.hash immutable when layer == "wiki""#).unwrap();
    db.run("index docs [hash]").unwrap();
    let old_hash = db.store.get_by_id("docs", "guarded").unwrap()["hash"].text().unwrap().to_owned();
    db.run(r#"update docs[id == "guarded"] cas each {body:"after",layer:"wiki"}"#).unwrap();
    let row = db.store.get_by_id("docs", "guarded").unwrap();
    let new_hash = row["hash"].text().unwrap().to_owned();
    assert_ne!(old_hash, new_hash);
    assert_eq!(row["body"].text(), Some("after"));
    let hits = db.run(&format!(r#"docs | hash == "{new_hash}" | {{id}} | take all"#)).unwrap();
    assert_eq!(hits.rows.len(), 1);
    assert_eq!(hits.rows[0]["id"].text(), Some("guarded"));
    for patch in [r#"{hash:"manual",layer:"other"}"#, r#"{body:"tamper",hash:"manual",layer:"other"}"#] {
        let before = rmp_serde::to_vec_named(db.store.get_by_id("docs", "guarded").unwrap()).unwrap();
        let generation = db.stats().r#gen;
        let error = db.run(&format!(r#"update docs[id == "guarded"] cas each {patch}"#)).unwrap_err();
        assert!(error.to_string().contains("immutable field: docs.hash"), "{error}");
        assert_eq!(db.stats().r#gen, generation);
        assert_eq!(rmp_serde::to_vec_named(db.store.get_by_id("docs", "guarded").unwrap()).unwrap(), before);
    }
    db.run(r#"update docs[id == "guarded"] cas each {title:"unrelated"}"#).unwrap();
    assert_eq!(db.store.get_by_id("docs", "guarded").unwrap()["hash"].text(), Some(new_hash.as_str()));
}
