#[cfg(test)]
mod rollback_fts_real_fold_supplement {
    use crate::{Db, fts::FtsIndex};
    use std::borrow::Cow;
    #[test]
    fn rollback_after_257_distinct_posting_removals() {
        for explicit in [false, true] {
            for mixed in [false, true] {
                let mut db = Db::empty();
                for i in 0..300 {
                    db.run(&format!(r#"insert docs {{id:"fold{i}",uri:"bench://fold{i}",body:"body",title:"alpha shared"}}"#)).unwrap();
                }
                let fields = vec!["title".to_owned()];
                db.store.fts.insert("docs".into(), FtsIndex::build(db.store.collection("docs"), &fields));
                let before = rmp_serde::to_vec_named(db.store.collection("docs")).unwrap();
                let generation = db.stats().r#gen;
                let hashes: Vec<_> = (0..300).map(|i| db.store.get_by_id("docs", &format!("fold{i}")).unwrap()["hash"].text().unwrap().to_owned()).collect();
                let mut tx = db.transaction().unwrap();
                if mixed { tx.run(r#"insert docs {id:"transient",title:"other"}"#).unwrap(); }
                for (i, hash) in hashes.iter().enumerate() {
                    tx.run(&format!(r#"update docs[id=="fold{i}"] cas "{hash}" {{title:"beta shared"}}"#)).unwrap();
                    if i == 256 {
                        match tx.db.store.fts["docs"].matches_cow("alpha").unwrap() {
                            Cow::Borrowed(ids) => assert_eq!(ids, &(257..300).collect::<Vec<_>>()),
                            Cow::Owned(_) => panic!("257 removals must fold alpha into canonical base"),
                        }
                    }
                }
                if explicit { tx.rollback().unwrap(); } else { drop(tx); }
                assert_eq!(db.stats().r#gen, generation);
                assert_eq!(rmp_serde::to_vec_named(db.store.collection("docs")).unwrap(), before);
                let rebuilt = FtsIndex::build(db.store.collection("docs"), &fields);
                for token in ["alpha", "beta", "shared", "other"] {
                    assert_eq!(db.store.fts["docs"].matches_cow(token).map(Cow::into_owned).unwrap_or_default(), rebuilt.matches_cow(token).map(Cow::into_owned).unwrap_or_default(), "{token}: explicit={explicit}, mixed={mixed}");
                }
            }
        }
    }
}
