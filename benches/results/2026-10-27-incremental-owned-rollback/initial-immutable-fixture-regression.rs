use lin::{Db,Cell};
#[test]
fn owned_rollback_restores_changed_identity_and_row_maps(){
 let mut db=Db::empty();db.run(r#"insert docs {id:"seed",uri:"bench://seed",title:"seed",body:"original",wing:"rag"}"#).unwrap();let generation=db.store.r#gen;
 {let mut tx=db.transaction().unwrap();tx.run(r#"update docs[id=="seed"] cas each {id:"changed",uri:"bench://changed",wing:"sys"}"#).unwrap();tx.rollback().unwrap();}
 assert_eq!(db.store.r#gen,generation);assert!(db.store.get_by_id("docs","changed").is_none());let row=db.store.get_by_id("docs","seed").unwrap();assert_eq!(row["uri"].text(),Some("bench://seed"));assert_eq!(row["wing"].text(),Some("rag"));assert_eq!(db.scalar(r#"docs | wing == "rag" | count"#).unwrap(),Cell::Int(1));
}
#[test]
fn owned_rollback_reverses_repeated_updates(){
 let mut db=Db::empty();db.run(r#"insert docs {id:"seed",title:"seed",body:"original",wing:"rag"}"#).unwrap();db.run("index docs [wing]").unwrap();let before=rmp_serde::to_vec_named(db.store.collection("docs")).unwrap();let generation=db.store.r#gen;
 {let mut tx=db.transaction().unwrap();tx.run(r#"update docs[id=="seed"] cas each {wing:"sys"}"#).unwrap();tx.run(r#"update docs[id=="seed"] cas each {wing:"other",title:"changed"}"#).unwrap();tx.rollback().unwrap();}
 assert_eq!(rmp_serde::to_vec_named(db.store.collection("docs")).unwrap(),before);assert_eq!(db.store.r#gen,generation);assert_eq!(db.scalar(r#"docs | wing == "rag" | count"#).unwrap(),Cell::Int(1));assert_eq!(db.scalar(r#"docs | wing == "sys" | count"#).unwrap(),Cell::Int(0));
}

#[test]
fn incremental_rollback_preserves_fts_after_success_and_index_failure(){
 let mut db=Db::empty();db.run(r#"insert docs [{id:"seed",title:"free",body:"oldtoken",wing:"rag"},{id:"other",title:"taken",body:"unrelated",wing:"rag"}]"#).unwrap();db.run("index docs unique [title]").unwrap();let before=rmp_serde::to_vec_named(db.store.collection("docs")).unwrap();
 {let mut tx=db.transaction().unwrap();tx.run(r#"update docs[id=="seed"] cas each {body:"newtoken",title:"different"}"#).unwrap();tx.rollback().unwrap();}
 assert_eq!(rmp_serde::to_vec_named(db.store.collection("docs")).unwrap(),before);
 assert_eq!(db.run(r#"docs | search lex "oldtoken" | {id} | take all"#).unwrap().rows.len(),1);assert!(db.run(r#"docs | search lex "newtoken" | {id} | take all"#).unwrap().rows.is_empty());
 {let mut tx=db.transaction().unwrap();assert!(tx.run(r#"update docs[id=="seed"] cas each {body:"badtoken",title:"taken"}"#).is_err());tx.rollback().unwrap();}
 assert_eq!(rmp_serde::to_vec_named(db.store.collection("docs")).unwrap(),before);assert_eq!(db.run(r#"docs | search lex "oldtoken" | {id} | take all"#).unwrap().rows.len(),1);assert!(db.run(r#"docs | search lex "badtoken" | {id} | take all"#).unwrap().rows.is_empty());assert_eq!(db.scalar(r#"docs | title == "free" | count"#).unwrap(),Cell::Int(1));
}
