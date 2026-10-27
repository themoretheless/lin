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
