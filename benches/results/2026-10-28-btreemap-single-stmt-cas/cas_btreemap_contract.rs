use std::collections::BTreeMap;
use lin::{Cell, Db, Row};

#[test]
fn row_keeps_exact_btreemap_type_identity_after_cas_update() {
    let mut db = Db::empty();
    db.run(r#"insert docs {id:"a",body:"before",title:"A"}"#).unwrap();
    let mut result = db.run(r#"update docs[id=="a"] cas each {title:"B"}"#).unwrap();
    let row: BTreeMap<String, Cell> = result.rows.pop().unwrap();
    let same_row: Row = row;
    assert_eq!(same_row["title"].text(), Some("B"));
}
