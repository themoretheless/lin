
#[cfg(test)]
mod incremental_rollback_fold_oracle {
 #[test]
 fn repeated_inverse_updates_match_rebuilt_fts_across_pending_folds(){
  let mut db=crate::Db::empty();let rows=(0..600).map(|i|format!(r#"{{id:"d-{i}",uri:"bench://{i}",title:"old shared",body:"body stable",wing:"{}"}}"#,if i%2==0{"rag"}else{"sys"})).collect::<Vec<_>>().join(",");db.run(&format!("insert docs [{rows}]")).unwrap();db.run("index docs [wing]").unwrap();db.run("index docs unique [uri]").unwrap();let initial=rmp_serde::to_vec_named(db.store.collection("docs")).unwrap();let generation=db.store.r#gen;let fields=db.store.fts.get("docs").unwrap().fields.clone();let mut failures=0;let mut mixed=0;
  for step in 0..1200{let i=step%600;let id=format!("d-{i}");let before=rmp_serde::to_vec_named(db.store.get_by_id("docs",&id).unwrap()).unwrap();
   {let mut tx=db.transaction().unwrap();if step%11==0{tx.run(&format!(r#"insert docs {{id:"extra-{step}",uri:"extra://{step}",title:"temporary",body:"temporary",wing:"sys"}}"#)).unwrap();mixed+=1;}
    tx.run(&format!(r#"update docs[id=="d-{i}"] cas each {{title:"new shared",wing:"{}"}}"#,if i%2==0{"sys"}else{"rag"})).unwrap();
    if step%3==0{let error=tx.run(&format!(r#"update docs[id=="d-{i}"] cas each {{title:"bad notposted",uri:"bench://{}"}}"#,(i+1)%600)).unwrap_err();assert!(error.to_string().contains("unique"),"unexpected failure: {error}");failures+=1;}
    tx.rollback().unwrap();
   }
   assert_eq!(db.store.r#gen,generation);assert_eq!(db.store.row_count(),600);assert_eq!(rmp_serde::to_vec_named(db.store.get_by_id("docs",&id).unwrap()).unwrap(),before);assert_eq!(db.scalar(r#"docs | wing == "rag" | count"#).unwrap(),crate::Cell::Int(300));
   let reference=crate::fts::FtsIndex::build(db.store.collection("docs"),&fields);let actual=db.store.fts.get("docs").unwrap();for query in ["old","new","shared","body","stable","bad","notposted","temporary","o*","n*"]{assert_eq!(actual.candidate_idxs(query),reference.candidate_idxs(query),"step={step} query={query}");}
  }
  assert_eq!(rmp_serde::to_vec_named(db.store.collection("docs")).unwrap(),initial);assert_eq!(failures,400);assert_eq!(mixed,110);eprintln!("ROLLBACK_FTS_FOLD_PROBE {}",serde_json::json!({"rows":600,"rollbacks":1200,"unique_failures":failures,"mixed_append_update":mixed,"rebuilt_fts_comparisons":12000,"full_canonical_final_rows":true,"generation_unchanged":true}));
 }
}
