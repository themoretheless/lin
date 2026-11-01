use crate::{Db, fts::FtsIndex};
use std::borrow::Cow;

fn seeded(fields: &[String]) -> Db {
    let mut db = Db::empty();
    db.run(r#"insert docs {id:"a",uri:"bench://a",title:"alpha shared",body:"bodyold",wing:"rag"}"#).unwrap();
    let fts = FtsIndex::build(db.store.collection("docs"), fields);
    db.store.fts.insert("docs".into(), fts);
    db
}

#[test]
fn rollback_fts_unrelated_patch_keeps_base_postings() {
    let fields = vec!["title".to_owned()];
    for explicit in [false, true] {
        for field in ["wing", "layer"] {
            let mut db = seeded(&fields);
            let before = rmp_serde::to_vec_named(db.store.collection("docs")).unwrap();
            let generation = db.stats().r#gen;
            let hash = db.store.get_by_id("docs", "a").unwrap()["hash"].text().unwrap().to_owned();
            let pointer = match db.store.fts["docs"].matches_cow("alpha").unwrap() {
                Cow::Borrowed(ids) => ids.as_ptr(),
                Cow::Owned(_) => panic!("seed postings should be canonical"),
            };
            let mut tx = db.transaction().unwrap();
            let held = tx.run(&format!(r#"update docs[id=="a"] cas "{hash}" {{{field}:"changed"}}"#)).unwrap();
            let held_bytes = rmp_serde::to_vec_named(&held.rows).unwrap();
            if explicit { tx.rollback().unwrap(); } else { drop(tx); }
            assert_eq!(db.stats().r#gen, generation);
            assert_eq!(rmp_serde::to_vec_named(db.store.collection("docs")).unwrap(), before);
            assert_eq!(rmp_serde::to_vec_named(&held.rows).unwrap(), held_bytes);
            match db.store.fts["docs"].matches_cow("alpha").unwrap() {
                Cow::Borrowed(ids) => {
                    assert_eq!(ids, &[0]);
                    assert_eq!(ids.as_ptr(), pointer, "unrelated rollback rewrote postings");
                }
                Cow::Owned(_) => panic!("unrelated rollback introduced FTS delta edits"),
            }
        }
    }
}

#[test]
fn rollback_fts_journal_impact_matches_rebuild_after_folding() {
    for declared_snippet in [false, true] {
        let mut fields = vec!["title".to_owned()];
        if declared_snippet { fields.push("snippet".into()); }
        for field in ["title", "snippet"] {
            for mixed in [false, true] {
                for explicit in [false, true] {
                    let mut db = seeded(&fields);
                    let before = rmp_serde::to_vec_named(db.store.collection("docs")).unwrap();
                    let generation = db.stats().r#gen;
                    let hash = db.store.get_by_id("docs", "a").unwrap()["hash"].text().unwrap().to_owned();
                    let mut tx = db.transaction().unwrap();
                    if mixed {
                        tx.run(r#"insert docs {id:"transient",title:"transient shared"}"#).unwrap();
                    }
                    let mut held = None;
                    for i in 0..300 {
                        held = Some(tx.run(&format!(r#"update docs[id=="a"] cas "{hash}" {{{field}:"NEWTOKEN{i} shared ЁЖ"}}"#)).unwrap());
                    }
                    let held = held.unwrap();
                    let held_bytes = rmp_serde::to_vec_named(&held.rows).unwrap();
                    if explicit { tx.rollback().unwrap(); } else { drop(tx); }
                    assert_eq!(db.stats().r#gen, generation);
                    assert_eq!(rmp_serde::to_vec_named(db.store.collection("docs")).unwrap(), before);
                    assert_eq!(rmp_serde::to_vec_named(&held.rows).unwrap(), held_bytes);
                    let rebuilt = FtsIndex::build(db.store.collection("docs"), &fields);
                    for token in ["alpha", "shared", "bodyold", "transient", "newtoken0", "newtoken150", "newtoken299", "ёж"] {
                        let actual = db.store.fts["docs"].matches_cow(token).map(Cow::into_owned).unwrap_or_default();
                        let expected = rebuilt.matches_cow(token).map(Cow::into_owned).unwrap_or_default();
                        assert_eq!(actual, expected, "field={field}, mixed={mixed}, explicit={explicit}, token={token}");
                    }
                }
            }
        }
    }
}
