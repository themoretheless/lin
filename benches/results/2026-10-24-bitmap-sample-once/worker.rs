use lin::{Db,Row,Cell};use std::{time::Instant,hint::black_box};
fn main(){
 for (case,n,card,enabled) in [("bitmap_1k",1000,8,true),("bitmap_10k",10000,8,true),("standard_card100",1000,100,true),("disabled_1k",1000,8,false)] {
  let mut db=Db::empty();db.run("index docs [wing]").unwrap();let mut seed=db.store.indexes.values().find(|x|x.def.fields==vec!["wing".to_owned()]).unwrap().clone();seed.def.bitmap_card_field=if enabled{Some("wing".into())}else{None};
  let rows:Vec<Row>=(0..n).map(|i|Row::from([("wing".into(),Cell::Int((i%card)as i64))])).collect();let mut ns=Vec::new();
  for _ in 0..12 {let mut index=seed.clone();let t=Instant::now();index.insert_slab_batch(0,black_box(&rows)).unwrap();ns.push(t.elapsed().as_nanos());assert_eq!(index.reverse.len(),n);assert_eq!(index.forward.len(),card);
   for (key,list) in &index.forward{let actual=list.to_indices();let expected:Vec<usize>=rows.iter().enumerate().filter(|(_,r)|index.key_of(r)==*key).map(|(i,_)|i).collect();assert_eq!(actual,expected);}
   black_box(&index);
  }
  println!("{}",serde_json::json!({"case":case,"rows":n,"ns":ns,"exact_postings":true}));
 }
}