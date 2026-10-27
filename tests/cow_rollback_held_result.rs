use lin::{Cell, Db};
#[test]
fn cow_rollback_preserves_held_updated_result_and_restores_complete_row() {
    for explicit in [false, true] {
    let mut db = Db::empty();
    db.run("index docs [wing, ts]").unwrap();
    db.run(r#"insert docs {id:"held",uri:"bench://held",title:"before",body:"payload",layer:"wiki",wing:"rag",ts:timestamp(1)}"#).unwrap();
    let before = rmp_serde::to_vec_named(db.store.get_by_id("docs", "held").unwrap()).unwrap();
    let generation = db.stats().r#gen;
    let hash = db.store.get_by_id("docs", "held").unwrap()["hash"].text().unwrap().to_owned();
    let query = db.prepare(&format!(r#"update docs[id=="held"] cas "{hash}" {{title:"after",wing:"sys"}}"#)).unwrap();
    let mut tx = db.transaction().unwrap();
    let mut held = tx.run_prepared(&query).unwrap();
    assert_eq!(held.rows[0]["title"].text(), Some("after"));
    let updated = rmp_serde::to_vec_named(&held.rows[0]).unwrap();
    if explicit { tx.rollback().unwrap(); } else { drop(tx); }
    assert_eq!(db.stats().r#gen, generation);
    assert_eq!(rmp_serde::to_vec_named(db.store.get_by_id("docs", "held").unwrap()).unwrap(), before);
    assert_eq!(rmp_serde::to_vec_named(&held.rows[0]).unwrap(), updated);
    assert_eq!(db.run(r#"docs | wing == "rag" | count"#).unwrap().rows[0].values().next().and_then(Cell::as_int), Some(1));
    held.rows[0].insert("title".into(), Cell::from_text("client edit"));
    assert_eq!(rmp_serde::to_vec_named(db.store.get_by_id("docs", "held").unwrap()).unwrap(), before);
    }
}
