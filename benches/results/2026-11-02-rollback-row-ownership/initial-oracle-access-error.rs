#[cfg(test)]
mod tests {
use lin::{Db, Row, RowKey, Cell};
use std::collections::BTreeMap;
    fn map_address(row: &Row) -> usize {
        (std::ops::Deref::deref(row) as *const BTreeMap<RowKey, Cell>) as usize
    }

    #[test]
    fn rollback_reuses_unheld_map_and_preserves_held_versions() {
        for explicit in [false, true] {
            for retain_result in [false, true] {
                let mut db = Db::empty();
                db.run(r#"insert docs {id:"d",uri:"fixture://d",title:"oldtoken",wing:"rag",body:"body"}"#).unwrap();
                db.run("index docs [wing]").unwrap();
                let before = db.store.get_by_id("docs", "d").unwrap().clone();
                let generation = db.r#gen();
                let current_address;
                let held;
                {
                    let mut tx = db.transaction().unwrap();
                    tx.execute(r#"update docs[id == "d"] cas each {title:"middle",wing:"sys"}"#).unwrap();
                    let result = tx.run(r#"update docs[id == "d"] cas each {title:"newtoken",wing:"raw"}"#).unwrap();
                    held = retain_result.then(|| result.rows[0].clone());
                    drop(result);
                    current_address = map_address(tx.db().store.get_by_id("docs", "d").unwrap());
                    if explicit { tx.rollback().unwrap(); } else { drop(tx); }
                }
                let restored = db.store.get_by_id("docs", "d").unwrap();
                assert_eq!(restored, &before);
                assert_eq!(db.r#gen(), generation);
                if let Some(held) = held {
                    assert_eq!(held["title"].text(), Some("newtoken"));
                    assert_eq!(held["wing"].text(), Some("raw"));
                    assert_ne!(map_address(restored), current_address);
                } else {
                    assert_eq!(map_address(restored), current_address,
                        "unpublished Packs and undo must not force a map clone");
                }
                let fts = &db.store.fts["docs"];
                assert_eq!(fts.matches("oldtoken"), Some(vec![0]));
                for token in ["middle", "newtoken"] {
                    assert!(fts.matches(token).unwrap_or_default().is_empty());
                }
                assert_eq!(db.run(r#"docs | wing == "rag" | count"#).unwrap().scalar().unwrap(), Cell::Int(1));
                assert_eq!(db.run(r#"docs | wing == "raw" | count"#).unwrap().scalar().unwrap(), Cell::Int(0));
            }
        }
    }


}
