use lin::{Cell,Row,Db};
use lin::persist::{LogRecord,Pack,SyncMode,append_record};
use std::{fs::{self,OpenOptions},path::PathBuf,sync::Arc};
fn row(id:&str,title:&str,vector:Vec<f32>)->Row {
 Row::from([("id".into(),Cell::text_arc(id)),("uri".into(),Cell::text_arc(format!("fixture://{id}"))),("title".into(),Cell::text_arc(title)),("body".into(),Cell::text_arc("ЁЖ 🦔 body \\\"")),("wing".into(),Cell::text_arc("rag")),("embedding".into(),Cell::Vec(Arc::from(vector))),("negative_zero".into(),Cell::Float(-0.0))])
}
fn main(){
 let root=PathBuf::from(std::env::args().nth(1).unwrap());assert!(!root.exists());fs::create_dir(&root).unwrap();
 for case in ["update","delete","batch","mixed_batch","deep_batch","empty_vector","negative_zero","ieee_bits"] {
  let original=row("d1","oldtoken d1",vec![0.0;768]);let d2=row("d2","oldtoken d2",vec![-0.0,0.0,1.0]);let d3=row("d3","oldtoken d3",vec![0.0;31]);let d4=row("d4","newtoken d4",vec![0.0,1.0]);
  let vector=match case {"empty_vector"=>vec![],"negative_zero"=>vec![-0.0,-1.0,1.0],"ieee_bits"=>[0u32,0x80000000,1,0x80000001,0x7f800000,0xff800000,0x7fc01234,0x7fa00001,0xffc05678].into_iter().map(f32::from_bits).collect(),_ =>(0..768).map(|i|if i%12==0{0.5}else{0.0}).collect()};
  let updated=row("d1","newtoken d1",vector);let update=Pack::Update {collection:"docs".into(),rows:vec![updated.clone()]};let delete=Pack::Delete {collection:"docs".into(),rows:vec![d2.clone()]};
  let mut expected=vec![updated.clone(),d2.clone(),d3.clone()];
  let pack=match case {
   "delete"=>{expected=vec![original.clone(),d3.clone()];delete},
   "batch"=>{expected=vec![updated,d3.clone()];Pack::Batch {packs:vec![update,delete]}},
   "mixed_batch"=>{expected=vec![updated,d4.clone(),d3.clone()];Pack::Batch {packs:vec![update,Pack::InsertBulk {collection:"docs".into(),rows:vec![d4],edges:vec![]},delete]}},
   "deep_batch"=>{let mut p=update;for _ in 0..17 {p=Pack::Batch {packs:vec![p]};}p},
   _=>update,
  };
  let records=vec![LogRecord {r#gen:1,next_id:20,pack:Pack::InsertBulk {collection:"docs".into(),rows:vec![original,d2,d3],edges:vec![]}},LogRecord {r#gen:2,next_id:77,pack}];
  fs::write(root.join(format!("{case}.records")),rmp_serde::to_vec_named(&records).unwrap()).unwrap();fs::write(root.join(format!("{case}.expected")),rmp_serde::to_vec_named(&expected).unwrap()).unwrap();
  let mut sizes=Vec::new();
  for (role,sync)in[("canonical",SyncMode::Normal),("compact",SyncMode::Full)] {
   let path=root.join(format!("{case}-{role}.wal"));let mut file=OpenOptions::new().write(true).create_new(true).open(&path).unwrap();let mut count=0;
   for record in &records{append_record(&mut file,record,&mut Vec::new(),sync,&mut count).unwrap();}
   let bytes=fs::read(path).unwrap();assert_eq!(bytes.len()as u64,count);let mut db=Db::empty();assert_eq!(db.apply_wal(&bytes).unwrap(),2);assert_eq!(rmp_serde::to_vec_named(db.store.collection("docs")).unwrap(),rmp_serde::to_vec_named(&expected).unwrap());sizes.push(count);
  }
  if ["deep_batch","empty_vector","negative_zero"].contains(&case) {assert_eq!(fs::read(root.join(format!("{case}-canonical.wal"))).unwrap(),fs::read(root.join(format!("{case}-compact.wal"))).unwrap());}else{assert!(sizes[1]<sizes[0]);}
  println!("{}",serde_json::json!({"case":case,"canonical_bytes":sizes[0],"compact_bytes":sizes[1],"current_replay_verified":true,"performance_qualified":false}));
 }
}
