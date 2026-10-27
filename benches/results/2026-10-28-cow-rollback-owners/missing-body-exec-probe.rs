#[cfg(test)]
mod cow_rollback_owners_probe {
    use super::*;
    #[test]
    fn pending_pack_owns_updated_row_until_cleared() {
        for keep_handle in [false, true] {
            let mut db = Db::empty();
            db.run(r#"insert docs {id:"owners",uri:"bench://owners",title:"before",wing:"rag"}"#).unwrap();
            let before = rmp_serde::to_vec_named(db.store.get_by_id("docs", "owners").unwrap()).unwrap();
            let generation = db.stats().r#gen;
            let hash = db.store.get_by_id("docs", "owners").unwrap()["hash"].text().unwrap().to_owned();
            let prepared = db.prepare(&format!(r#"update docs[id=="owners"] cas "{hash}" {{wing:"sys"}}"#)).unwrap();
            let mut tx = db.transaction().unwrap();
            let handle = tx.run_prepared(&prepared).unwrap();
            let with_handle = tx.db.store.probe_row_owner_count("owners");
            assert_eq!(with_handle, 3);
            let held = if keep_handle { Some(handle) } else { drop(handle); None };
            let before_clear = tx.db.store.probe_row_owner_count("owners");
            assert_eq!(before_clear, if keep_handle { 3 } else { 2 });
            assert_eq!(tx.packs.len(), 1);
            tx.packs.clear();
            let after_clear = tx.db.store.probe_row_owner_count("owners");
            assert_eq!(after_clear, if keep_handle { 2 } else { 1 });
            tx.rollback().unwrap();
            assert_eq!(db.stats().r#gen, generation);
            assert_eq!(rmp_serde::to_vec_named(db.store.get_by_id("docs", "owners").unwrap()).unwrap(), before);
            if let Some(h) = held { assert_eq!(h.rows[0]["wing"].text(), Some("sys")); }
            eprintln!("COW_OWNERS {}",serde_json::json!({"keep_handle":keep_handle,"with_handle":with_handle,"before_clear":before_clear,"after_clear":after_clear}));
        }
    }
}
