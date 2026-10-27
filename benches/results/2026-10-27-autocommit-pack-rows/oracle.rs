#[cfg(test)] mod cow_row_oracle {
 use super::*;
 #[test] fn cloned_rows_do_not_alias_mutations_and_serde_stays_map_shaped(){
  let original=Row::from([("name".into(),Cell::text_arc("ёж")),("float".into(),Cell::Float(-0.0)),("long_payload".into(),Cell::text_arc("shared long payload shared long payload"))]);
  let map:BTreeMap<String,Cell>=original.iter().map(|(k,v)|(k.clone(),v.clone())).collect();
  assert_eq!(serde_json::to_vec(&original).unwrap(),serde_json::to_vec(&map).unwrap());
  assert_eq!(rmp_serde::to_vec(&original).unwrap(),rmp_serde::to_vec(&map).unwrap());
  let mut changed=original.clone();changed.insert("name".into(),Cell::text_arc("new"));changed.remove("float");changed.entry("added".into()).or_insert(Cell::Int(4));
  assert_eq!(original.get("name").and_then(Cell::text),Some("ёж"));assert!(original.contains_key("float"));assert!(!original.contains_key("added"));
  let restored:Row=rmp_serde::from_slice(&rmp_serde::to_vec(&original).unwrap()).unwrap();assert_eq!(restored,original);
  let mut via_iter=original.clone();for(_,v)in&mut via_iter{*v=Cell::Null;}assert_eq!(original.get("name").and_then(Cell::text),Some("ёж"));
  let owned:Vec<_>=original.clone().into_iter().collect();assert_eq!(owned.len(),original.len());assert_eq!(original.get("name").and_then(Cell::text),Some("ёж"));
 }
}
#[cfg(test)] mod inplace_patch_engine_oracle {
 use super::*;
 #[test] fn held_row_isolated_and_body_hash_fts_maps_restore_on_rollback(){
  let mut db=crate::Db::empty();
  db.run(r#"insert docs {id:"d",uri:"fixture://d",title:"oldtoken",layer:"wiki",wing:"rag",body:"old body"}"#).unwrap();
  let held=db.run(r#"docs | id == "d""#).unwrap();
  let before=db.store.get_by_id("docs","d").unwrap().clone();let hash=before["hash"].text().unwrap().to_owned();
  let prepared=db.prepare(&format!(r#"update docs[id=="d"] cas "{hash}" {{layer:"wiki",body:"new body",title:"newtoken",wing:"sys"}}"#)).unwrap();
  {
   let mut tx=db.transaction().unwrap();let result=tx.run_prepared(&prepared).unwrap();assert_eq!(result.done.n,1);
   let now=tx.db().store.get_by_id("docs","d").unwrap();assert_eq!(now["title"].text(),Some("newtoken"));assert_ne!(now["hash"],before["hash"]);
   assert_eq!(held.rows[0],before);
   let fts=tx.db().store.fts.get("docs").unwrap();assert_eq!(fts.matches("newtoken"),Some(vec![0]));assert!(fts.matches("oldtoken").unwrap_or_default().is_empty());
   drop(result);tx.rollback().unwrap();
  }
  assert_eq!(db.store.get_by_id("docs","d").unwrap(),&before);assert_eq!(held.rows[0],before);
  let fts=db.store.fts.get("docs").unwrap();assert_eq!(fts.matches("oldtoken"),Some(vec![0]));assert!(fts.matches("newtoken").unwrap_or_default().is_empty());
 }
}

#[cfg(test)]mod map_row_guard_oracle{
 use super::*;
 #[test]fn derived_hash_skips_only_implicit_guard_and_explicit_hash_still_checks_before_row(){
  let mut db=crate::Db::empty();db.run(r#"insert docs {id:"g",uri:"guard://g",layer:"wiki",body:"before"}"#).unwrap();
  db.catalog.guards.push(crate::catalog::Guard{collection:"docs".into(),field:"hash".into(),pred:crate::ast::Pred::Cmp{field:crate::ast::Field::name("layer"),op:crate::ast::CmpOp::Eq,value:crate::ast::Value::String("wiki".into())}});
  let old=db.store.get_by_id("docs","g").unwrap().clone();
  db.run(r#"update docs[id=="g"] cas each {layer:"wiki",body:"after"}"#).unwrap();
  let updated=db.store.get_by_id("docs","g").unwrap().clone();assert_ne!(updated["hash"],old["hash"]);assert_eq!(updated["body"].text(),Some("after"));
  let prepared=db.prepare(r#"update docs[id=="g"] cas each {layer:"raw",hash:"manual"}"#).unwrap();
  let error=prepared.run(&mut db).unwrap_err();assert!(error.to_string().contains("immutable field: docs.hash"),"{error}");assert_eq!(db.store.get_by_id("docs","g").unwrap(),&updated);
 }
 #[test]fn public_row_is_exact_btree_map_type(){
  let map=std::collections::BTreeMap::<String,Cell>::new();let row:Row=map;let _:std::collections::BTreeMap<String,Cell>=row;
 }
}

#[cfg(test)] mod autocommit_payload_oracle {
 use super::*;
 #[test] fn full_outputs_and_replay_preserve_payloads() {
  for durable in [false,true] {
   let dir=std::env::temp_dir().join(format!("lin-payload-recover-{}",std::process::id()));
   let mut db=if durable {assert!(!dir.exists());crate::Db::open_with(&dir,crate::OpenOpts{sync:crate::SyncMode::Full,cold:false}).unwrap()}else{crate::Db::empty()};
   db.run(r#"insert docs [{id:"a",title:"first",body:"original",wing:"rag"},{id:"b",title:"second",body:"other",wing:"rag"}]"#).unwrap();
   let a=db.store.get_by_id("docs","a").unwrap().clone();
   let h=db.run(r#"update docs[id=="a"] cas each {title:"changed",wing:"sys"}"#).unwrap();
   assert_eq!(h.done.n,1);assert_eq!(h.rows,vec![db.store.get_by_id("docs","a").unwrap().clone()]);assert_eq!(h.rows[0]["body"],a["body"]);
   let changed=h.rows[0].clone();
   let del=db.run(r#"delete docs[id=="a"] cas each"#).unwrap();assert_eq!(del.done.n,1);assert_eq!(del.rows,vec![changed]);assert!(db.store.get_by_id("docs","a").is_none());
   let b=db.store.get_by_id("docs","b").unwrap().clone();
   let missing=db.run(r#"update docs[id=="missing"] cas each {title:"none"}"#).unwrap_err();assert!(missing.to_string().contains("no matching row"));
   drop(db);
   if durable {let mut reopened=crate::Db::open(&dir).unwrap();assert!(reopened.store.get_by_id("docs","a").is_none());assert_eq!(reopened.run(r#"docs | id == "b""#).unwrap().rows,vec![b]);drop(reopened);std::fs::remove_dir_all(dir).unwrap();}
  }
 }
}
