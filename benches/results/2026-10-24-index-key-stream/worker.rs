use lin::{Db,Row,Cell};use std::{time::Instant,hint::black_box};
fn main(){
 for(case,n,card,enabled,arity)in[("bitmap_1k",1000,8,true,2),("bitmap_10k",10000,8,true,2),("standard_card100",1000,100,true,2),("disabled_1k",1000,8,false,2),("disabled_10k",10000,8,false,2),("spilled_3fields",1000,100,false,3)]{
  let mut db=Db::empty();let fields=if arity==2{vec!["wing".to_owned(),"ts".to_owned()]}else{vec!["wing".to_owned(),"title".to_owned(),"ts".to_owned()]};db.run(&format!("index docs [{}]",fields.join(","))).unwrap();let mut seed=db.store.indexes.values().find(|x|x.def.fields==fields).unwrap().clone();seed.def.bitmap_card_field=if enabled{Some("wing".into())}else{None};
  let rows:Vec<Row>=(0..n).map(|i|Row::from([("wing".into(),Cell::Int((i%card)as i64)),("ts".into(),Cell::Time((i%2)as i64)),("title".into(),Cell::Text(format!("title {}",i%card).into()))])).collect();let mut ns=Vec::new();let mut key_size=0;
  for _ in 0..12{let mut index=seed.clone();let t=Instant::now();index.insert_slab_batch(0,black_box(&rows)).unwrap();ns.push(t.elapsed().as_nanos());assert_eq!(index.reverse.len(),n);assert_eq!(index.forward.len(),card);key_size=std::mem::size_of_val(index.reverse.values().next().unwrap());
   for(key,list)in &index.forward{let expected:Vec<usize>=rows.iter().enumerate().filter(|(_,r)|index.key_of(r)==*key).map(|(i,_)|i).collect();assert_eq!(list.to_indices(),expected);}black_box(&index);
  }println!("{}",serde_json::json!({"case":case,"rows":n,"ns":ns,"key_size_bytes":key_size,"exact_postings":true}));
 }
}