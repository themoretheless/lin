use lin::Db;
use serde_json::json;
fn seed() -> Db {
    let mut db = Db::empty();
    db.run(r#"insert docs [{id:"a",title:"same",layer:"wiki"},{id:"b",title:"other",layer:"wiki"}]"#).unwrap();
    db
}
#[test]
fn schema_after_rows_rollback_restores_rows_metadata_and_constraints() {
    let mut failed = false;
    for explicit in [false, true] {
        let mut db = seed();
        let before = rmp_serde::to_vec_named(db.store.collection("docs")).unwrap();
        let generation = db.store.r#gen;
        let next_id = db.store.next_id;
        {
            let mut tx = db.transaction().unwrap();
            tx.run(r#"insert docs {id:"c",title:"appended"}"#).unwrap();
            tx.run("col ghost_schema { title: text }").unwrap();
            if explicit { tx.rollback().unwrap(); } else { drop(tx); }
        }
        let rows_restored = before == rmp_serde::to_vec_named(db.store.collection("docs")).unwrap();
        let ok = rows_restored && db.store.r#gen == generation && db.store.next_id == next_id && !db.store.extra_collections.contains_key("ghost_schema");
        failed |= !ok;
        println!("{}", json!({"case":"append_then_schema", "explicit":explicit,"passes":ok,"rows_restored":rows_restored,"rows_after":db.store.collection("docs").len(),"generation_restored":db.store.r#gen==generation,"next_id_restored":db.store.next_id==next_id}));

        let mut db = seed();
        let before = rmp_serde::to_vec_named(db.store.collection("docs")).unwrap();
        {
            let mut tx = db.transaction().unwrap();
            tx.run(r#"update docs[id=="a"] cas each {title:"changed"}"#).unwrap();
            tx.run("col ghost_schema { title: text }").unwrap();
            if explicit { tx.rollback().unwrap(); } else { drop(tx); }
        }
        let rows_restored = before == rmp_serde::to_vec_named(db.store.collection("docs")).unwrap();
        let catalog_restored = db.catalog.collection("ghost_schema").is_none();
        let store_restored = !db.store.extra_collections.contains_key("ghost_schema");
        let ok = rows_restored && catalog_restored && store_restored;
        failed |= !ok;
        println!("{}", json!({"case":"update_then_schema", "explicit":explicit,"passes":ok,"rows_restored":rows_restored,"catalog_restored":catalog_restored,"store_schema_restored":store_restored}));

        let mut db = seed();
        {
            let mut tx = db.transaction().unwrap();
            tx.run(r#"delete docs[id=="b"] cas each"#).unwrap();
            tx.run("index docs unique [title]").unwrap();
            if explicit { tx.rollback().unwrap(); } else { drop(tx); }
        }
        let no_ghost = !db.store.indexes.values().any(|i|i.def.collection=="docs"&&i.def.unique&&i.def.fields==["title"]);
        let duplicate = db.run(r#"insert docs {id:"c",title:"same"}"#);
        let error = duplicate.as_ref().err().map(ToString::to_string);
        let ok = no_ghost && duplicate.is_ok();
        failed |= !ok;
        println!("{}",json!({"case":"delete_then_unique_index","explicit":explicit,"passes":ok,"ghost_unique_index_absent":no_ghost,"duplicate_insert_succeeds":duplicate.is_ok(),"error":error}));
    }
    assert!(!failed, "schema-after-row rollback must restore every observed invariant");
}

#[test]
fn repeated_schema_changes_preserve_journal_order_and_abort_state() {
    for prefix in [
        &[][..],
        &[r#"insert docs {id:"c",title:"appended"}"#][..],
        &[r#"update docs[id=="a"] cas each {title:"updated"}"#][..],
        &[r#"update docs[id=="a"] cas each {body:"prefix body",layer:"wiki"}"#][..],
        &[r#"delete docs[id=="b"] cas each"#][..],
        &[r#"insert docs {id:"c",title:"appended"}"#, r#"delete docs[id=="b"] cas each"#][..],
        &[r#"delete docs[id=="b"] cas each"#, r#"insert docs {id:"c",title:"appended"}"#][..],
    ] {
        for finish in 0..3 {
            let mut db = seed();
            let before = rmp_serde::to_vec_named(db.store.collection("docs")).unwrap();
            let generation = db.store.r#gen;
            let next_id = db.store.next_id;
            let index_names = db.store.indexes.keys().cloned().collect::<Vec<_>>();
            let collection_names = db.store.extra_collections.keys().cloned().collect::<Vec<_>>();
            {
                let mut tx = db.transaction().unwrap();
                for source in prefix { tx.run(source).unwrap(); }
                tx.run("col schema_alpha { id: text, title: text }").unwrap();
                tx.run(r#"insert schema_alpha {id:"x",title:"alpha"}"#).unwrap();
                tx.run("index docs unique [title]").unwrap();
                tx.run(r#"delete docs[id=="a"] cas each"#).unwrap();
                tx.run(r#"col schema_beta { id: text, title: text }
                    insert schema_beta {id:"y",title:"beta"}"#).unwrap();
                match finish {
                    0 => tx.rollback().unwrap(),
                    1 => drop(tx),
                    _ => {
                        let error = tx.run(r#"delete docs[id=="missing"] cas each"#).unwrap_err();
                        assert!(error.to_string().contains("no matching row"), "{error}");
                        assert!(tx.commit().unwrap_err().to_string().contains("transaction aborted"));
                    }
                }
            }
            assert_eq!(rmp_serde::to_vec_named(db.store.collection("docs")).unwrap(), before, "{prefix:?}, {finish}");
            assert_eq!(db.store.r#gen, generation);
            assert_eq!(db.store.next_id, next_id);
            assert_eq!(db.store.indexes.keys().cloned().collect::<Vec<_>>(), index_names);
            assert_eq!(db.store.extra_collections.keys().cloned().collect::<Vec<_>>(), collection_names);
            for name in ["schema_alpha", "schema_beta"] {
                assert!(db.catalog.collection(name).is_none());
                assert!(db.store.collection(name).is_empty());
                assert!(db.store.get_by_id(name, "x").is_none());
                assert!(db.store.get_by_id(name, "y").is_none());
            }
            assert!(db.store.get_by_id("docs", "a").is_some());
            assert!(db.store.get_by_id("docs", "b").is_some());
            assert!(db.store.get_by_id("docs", "c").is_none());
            assert!(db.run(r#"docs | search lex "prefix body" | take all"#).unwrap().rows.is_empty());
            db.run(r#"insert docs {id:"duplicate",title:"same"}"#).unwrap();
        }
    }
}

#[test]
fn generated_ids_are_reusable_after_schema_snapshot_rollback() {
    for finish in 0..3 {
        let mut db = seed();
        let before = rmp_serde::to_vec_named(db.store.collection("docs")).unwrap();
        let next_id = db.store.next_id;
        let generated;
        {
            let mut tx = db.transaction().unwrap();
            generated = tx.run(r#"insert docs {title:"first transient"}"#).unwrap().rows[0]["id"].text().unwrap().to_owned();
            tx.run("col first_schema { title: text }").unwrap();
            tx.run(r#"insert docs {title:"second transient"}"#).unwrap();
            tx.run("col second_schema { title: text }").unwrap();
            match finish {
                0 => tx.rollback().unwrap(),
                1 => drop(tx),
                _ => {
                    tx.run(r#"delete docs[id=="missing"] cas each"#).unwrap_err();
                    tx.commit().unwrap_err();
                }
            }
        }
        assert_eq!(rmp_serde::to_vec_named(db.store.collection("docs")).unwrap(), before);
        assert_eq!(db.store.next_id, next_id);
        let next = db.run(r#"insert docs {title:"permanent"}"#).unwrap();
        assert_eq!(next.rows[0]["id"].text(), Some(generated.as_str()));
    }
}
