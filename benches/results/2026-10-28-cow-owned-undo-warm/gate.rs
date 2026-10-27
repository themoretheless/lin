use lin::{Db,Cell};
fn seeded(n:usize)->Db {
    let mut db=Db::empty();db.run("index docs [wing, ts]").unwrap();
    for start in (0..n).step_by(500) {
        let input=format!("insert docs [{}]",(start..(start+500).min(n)).map(|i|format!(r#"{{ id:"d-{i}",uri:"bench://{i}",title:"{}",layer:"wiki",wing:"{}",body:"body {i}",ts:timestamp({}) }}"#,if i%10==0 {format!("doc {i} wal note")}else{format!("doc {i} plain")},if i%2==0 {"rag"}else{"sys"},1700000000000i64+(i%2)as i64)).collect::<Vec<_>>().join(","));
        db.run(&input).unwrap();
    }
    assert_eq!(db.store.collection("docs").len(),n);db
}
fn main(){
 let sizes=if std::env::var_os("LIN_ROLLBACK_SMOKE").is_some(){vec![1000usize]}else{vec![1000usize,10000,100000]};
 for n in sizes {
  let mut db=seeded(n);let hash=db.store.get_by_id("docs","d-500").unwrap()["hash"].text().unwrap().to_owned();
  let a=db.prepare(&format!(r#"update docs[id=="d-500"] cas "{hash}" {{wing:"sys"}}"#)).unwrap();
  let b=db.prepare(&format!(r#"update docs[id=="d-500"] cas "{hash}" {{wing:"rag"}}"#)).unwrap();
  let expected_row=rmp_serde::to_vec_named(db.store.get_by_id("docs","d-500").unwrap()).unwrap();
  for sample in 0..16 {
   let t=std::time::Instant::now();
   for _ in 0..256 {assert_eq!(a.run(&mut db).unwrap().done.n,1);assert_eq!(b.run(&mut db).unwrap().done.n,1);}
   let ns=t.elapsed().as_nanos() as f64/512.;
   assert_eq!(db.store.get_by_id("docs","d-500").unwrap()["wing"].text(),Some("rag"));assert_eq!(rmp_serde::to_vec_named(db.store.get_by_id("docs","d-500").unwrap()).unwrap(),expected_row);
   let h=db.run("docs | wing == \"rag\" | count").unwrap();assert_eq!(h.rows[0].values().next().and_then(Cell::as_int),Some((n/2)as i64));
   println!("{}",serde_json::json!({"rows":n,"sample":sample,"ns_per_update":ns,"operations":512,"exact_readback":true}));
   let before_gen=db.stats().r#gen;
   let t=std::time::Instant::now();
   {let mut tx=db.transaction().unwrap();assert_eq!(tx.run_prepared(&a).unwrap().done.n,1);tx.rollback().unwrap();}
   let rollback_ns=t.elapsed().as_nanos() as f64;
   assert_eq!(db.store.get_by_id("docs","d-500").unwrap()["wing"].text(),Some("rag"));assert_eq!(rmp_serde::to_vec_named(db.store.get_by_id("docs","d-500").unwrap()).unwrap(),expected_row);
   assert_eq!(db.stats().r#gen,before_gen);
   let h=db.run("docs | wing == \"rag\" | count").unwrap();assert_eq!(h.rows[0].values().next().and_then(Cell::as_int),Some((n/2)as i64));
   println!("{}",serde_json::json!({"rows":n,"sample":sample,"rollback_ns":rollback_ns,"operations":1,"exact_readback":true}));
  }
 }
}
