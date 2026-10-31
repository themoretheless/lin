use lin::Db;
use serde_json::json;
fn seed() -> Db {
    let mut db = Db::empty();
    db.run(r#"insert docs [{id:"a",title:"same",layer:"wiki"},{id:"b",title:"other",layer:"wiki"}]"#).unwrap();
    db
}
fn main() {
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
    if failed { std::process::exit(3); }
}
