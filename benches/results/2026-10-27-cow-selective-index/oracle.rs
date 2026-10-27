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

#[cfg(test)] mod selective_index_oracle {
 use super::*;
 fn validate(db:&crate::Db){
  for live in db.store.indexes.values(){
   let mut expected=BTreeMap::<crate::index::IndexKey,Vec<usize>>::new();
   let mut reverse=BTreeMap::new();
   for(i,row)in db.store.collection(&live.def.collection).iter().enumerate(){
    if live.def.pred.as_ref().is_some_and(|p|!crate::exec::eval_pred(p,row,0)){continue;}
    let key=live.key_of(row);expected.entry(key.clone()).or_default().push(i);reverse.insert(i,key);
   }
   let actual:BTreeMap<_,_>=live.forward.iter().map(|(k,p)|(k.clone(),p.to_indices())).collect();
   assert_eq!(actual,expected,"forward {}",live.def.label());
   assert_eq!(live.reverse.iter().map(|(k,v)|(*k,v.clone())).collect::<BTreeMap<_,_>>(),reverse,"reverse {}",live.def.label());
  }
 }
 #[test]fn changed_unique_composite_partial_and_rollback_match_full_rebuild(){
  let mut db=crate::Db::empty();
  for q in [r#"index docs unique [title]"#,r#"index docs [wing, ts]"#,r#"index docs [title, uri] where wing == "rag""#,
   r#"insert docs [{id:"a",uri:"selective://a",title:"first",wing:"rag",ts:timestamp(1)},{id:"b",uri:"selective://b",title:"second",wing:"sys",ts:timestamp(2)}]"#]{db.run(q).unwrap();}validate(&db);
  db.run(r#"update docs[id=="a"] cas each {wing:"sys"}"#).unwrap();validate(&db);
  db.run(r#"update docs[id=="b"] cas each {title:"third"}"#).unwrap();validate(&db);
  let before=db.store.collection("docs").to_vec();
  let error=db.run(r#"update docs[id=="b"] cas each {title:"first"}"#).unwrap_err();assert!(error.to_string().contains("unique"));assert_eq!(db.store.collection("docs"),before.as_slice());validate(&db);
  {let mut tx=db.transaction().unwrap();tx.run(r#"update docs[id=="a"] cas each {wing:"rag"}"#).unwrap();validate(tx.db());tx.rollback().unwrap();}
  assert_eq!(db.store.collection("docs"),before.as_slice());validate(&db);
 }
}
