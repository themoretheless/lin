#[cfg(test)] mod autocommit_boundary_oracle {
 use super::*;
 #[test] fn output_limit_preserves_committed_mutation_and_generation() {
  for delete in [false,true] {
   let mut db=Db::empty();db.run(r#"insert docs {id:"a",title:"old",body:"full payload",wing:"rag"}"#).unwrap();
   let generation=db.store.r#gen;db.quotas.max_output_bytes=1;
   let statement=if delete {r#"delete docs[id=="a"] cas each"#}else{r#"update docs[id=="a"] cas each {title:"new"}"#};
   let err=db.run(statement).unwrap_err();assert!(err.to_string().contains("max_output_bytes"),"{err}");assert_eq!(db.store.r#gen,generation+1);
   if delete {assert!(db.store.get_by_id("docs","a").is_none());}else{assert_eq!(db.store.get_by_id("docs","a").unwrap()["title"].text(),Some("new"));}
  }
 }
 #[test] fn failed_full_commit_restores_rows_generation_and_log() {
  for delete in [false,true] {
   let dir=std::env::temp_dir().join(format!("lin-pack-boundary-{}-{delete}",std::process::id()));assert!(!dir.exists());
   let mut db=Db::open_with(&dir,crate::OpenOpts{sync:crate::SyncMode::Full,cold:false}).unwrap();
   db.run(r#"insert docs {id:"a",title:"oldtoken",body:"full payload",wing:"rag"}"#).unwrap();
   let before=db.store.get_by_id("docs","a").unwrap().clone();let generation=db.store.r#gen;
   let path=crate::persist::log_path(&dir);let log_before=std::fs::read(&path).unwrap();
   let writable=std::mem::replace(&mut db.persist.as_mut().unwrap().log,std::fs::File::open(&path).unwrap());
   let statement=if delete {r#"delete docs[id=="a"] cas each"#}else{r#"update docs[id=="a"] cas each {title:"newtoken",wing:"sys"}"#};
   let result=db.run(statement);db.persist.as_mut().unwrap().log=writable;assert!(result.is_err());
   assert_eq!(db.store.r#gen,generation);assert_eq!(db.store.get_by_id("docs","a").unwrap(),&before);assert_eq!(std::fs::read(&path).unwrap(),log_before);
   assert_eq!(db.run(r#"docs | wing == "rag" | count"#).unwrap().rows[0].values().next().and_then(Cell::as_int),Some(1));
   assert_eq!(db.run(r#"docs | search lex "oldtoken" | take 10"#).unwrap().rows.len(),1);
   drop(db);let mut reopened=Db::open(&dir).unwrap();assert_eq!(reopened.run(r#"docs | id == "a""#).unwrap().rows,vec![before]);drop(reopened);std::fs::remove_dir_all(dir).unwrap();
  }
 }
}
