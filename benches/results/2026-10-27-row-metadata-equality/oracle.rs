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

#[cfg(test)] mod metadata_equality_oracle {
 use super::*;
 fn equal(actual:&Store,expected:&Store){
  assert_eq!(actual.by_id,expected.by_id);assert_eq!(actual.docs_by_uri,expected.docs_by_uri);
  assert_eq!(actual.docs_id,expected.docs_id);assert_eq!(actual.docs_title,expected.docs_title);assert_eq!(actual.docs_layer,expected.docs_layer);assert_eq!(actual.docs_wing,expected.docs_wing);
  assert_eq!(actual.orders_id,expected.orders_id);assert_eq!(actual.orders_user_id,expected.orders_user_id);assert_eq!(actual.users_id,expected.users_id);assert_eq!(actual.users_email,expected.users_email);
  assert_eq!(actual.orders_total.iter().map(|x|x.to_bits()).collect::<Vec<_>>(),expected.orders_total.iter().map(|x|x.to_bits()).collect::<Vec<_>>());
 }
 #[test] fn metadata_matches_full_rebuild_with_stale_maps_raw_changes_and_numeric_bits(){
  for col in ["docs","users","orders"] {
   let mut actual=Store::empty("test/8");let mut expected=Store::empty("test/8");
   let seed=Row::from([("id".into(),Cell::text_arc("a")),("uri".into(),Cell::text_arc("u")),("title".into(),Cell::text_arc("old")),("wing".into(),Cell::text_arc("rag")),("layer".into(),Cell::text_arc("wiki")),("email".into(),Cell::text_arc("a@x")),("user_id".into(),Cell::text_arc("a")),("total".into(),Cell::Float(0.0))]);
   actual.collection_mut(col).push(seed.clone());expected.collection_mut(col).push(seed);
   actual.rebuild_row_maps_collection(col);expected.rebuild_row_maps_collection(col);
   for step in 0..180 {
    if col=="docs"{actual.docs_title_contains_count("new");}
    // External raw row edit, deliberately without synchronizing SoA.
    if step%11==0 {actual.collection_mut(col)[0].insert("title".into(),Cell::text_arc(format!("raw-{step}")));}
    let before=actual.collection(col)[0].clone();let mut after=before.clone();
    let key=["title","layer","wing","email","user_id","total","id","uri"][step%8];
    let value=if key=="total"{Cell::Float([0.0,-0.0,f64::NAN,f64::INFINITY][step/8%4])}else if step%13==0{Cell::Null}else{Cell::text_arc(format!("new-{step}"))};after.insert(key.into(),value);
    actual.collection_mut(col)[0]=after.clone();expected.collection_mut(col)[0]=after.clone();
    if step%17==0 {actual.by_id.get_mut(col).unwrap().clear();}
    if step%19==0 && col=="docs" {actual.docs_wing.clear();}
    actual.row_maps_sync_updated(col,0,&before,&after);expected.rebuild_row_maps_collection(col);equal(&actual,&expected);
    if col=="docs"{assert_eq!(actual.docs_title_contains_count("new"),expected.docs_title_contains_count("new"));}
   }
  }
 }
 #[test] fn unchanged_titles_keep_packed_bytes_but_changed_title_invalidates(){
  let mut store=Store::empty("test/8");let row=Row::from([("id".into(),Cell::text_arc("a")),("title".into(),Cell::text_arc("old")),("wing".into(),Cell::text_arc("rag"))]);store.collection_mut("docs").push(row.clone());store.rebuild_row_maps_collection("docs");assert_eq!(store.docs_title_contains_count("old"),1);
  let mut after=row.clone();after.insert("wing".into(),Cell::text_arc("sys"));store.collection_mut("docs")[0]=after.clone();store.row_maps_sync_updated("docs",0,&row,&after);assert!(store.docs_title_packed.get().is_some());
  let before=after.clone();after.insert("title".into(),Cell::text_arc("new"));store.collection_mut("docs")[0]=after.clone();store.row_maps_sync_updated("docs",0,&before,&after);assert!(store.docs_title_packed.get().is_none());assert_eq!(store.docs_title_contains_count("new"),1);
 }
}
