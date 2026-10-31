use lin::Db;
use std::{fs,path::PathBuf};
fn main(){
 let root=PathBuf::from(std::env::args().nth(1).unwrap());
 for case in ["update","delete","batch","mixed_batch","deep_batch","empty_vector","negative_zero","ieee_bits"] {
  let expected=fs::read(root.join(format!("{case}.expected"))).unwrap();let expected_records=fs::read(root.join(format!("{case}.records"))).unwrap();
  for role in ["canonical","compact"] {
   let bytes=fs::read(root.join(format!("{case}-{role}.wal"))).unwrap();let mut records=Vec::new();lin::persist::for_each_wal_frame(&bytes,|r|{records.push(r);Ok(())}).unwrap();assert_eq!(rmp_serde::to_vec_named(&records).unwrap(),expected_records);
   let mut db=Db::empty();assert_eq!(db.apply_wal(&bytes).unwrap(),2);assert_eq!(db.store.r#gen,2);assert_eq!(db.store.next_id,77);assert_eq!(rmp_serde::to_vec_named(db.store.collection("docs")).unwrap(),expected);
   assert_eq!(db.apply_wal(&bytes).unwrap(),0);assert_eq!(rmp_serde::to_vec_named(db.store.collection("docs")).unwrap(),expected);
   for id in ["d1","d3"] {assert_eq!(db.store.get_by_id("docs",id).unwrap()["id"].text(),Some(id));}
   let query=db.run(r#"docs | id=="d1" | {id,title}"#).unwrap();assert_eq!(query.rows[0]["title"].text(),Some(if case=="delete"{"oldtoken d1"}else{"newtoken d1"}));
   let mut broken=bytes;*broken.last_mut().unwrap()^=1;assert!(lin::persist::for_each_wal_frame(&broken,|_|Ok(())).is_err());
  }
  println!("{}",serde_json::json!({"case":case,"legacy_full_record_bits_verified":true,"legacy_replay_verified":true,"generation_next_id_verified":true,"duplicate_apply_noop":true,"lookup_projection_verified":true,"crc_corruption_rejected":true,"performance_qualified":false}));
 }
}
