use lin::Db;
use std::{fs,path::PathBuf};
fn main(){
 let root=PathBuf::from(std::env::args().nth(1).unwrap());
 for case in ["id_uri","uri_only","stale_id_uri_fallback","facts_tuple","facts_missing_part","custom_id","custom_no_identity","nested_batch"] {
  let collection=if case.starts_with("facts"){"facts"}else if case.starts_with("custom"){"custom"}else{"docs"};
  let expected=fs::read(root.join(format!("{case}.expected"))).unwrap();
  for role in ["full","keys"] {
   let bytes=fs::read(root.join(format!("{case}-{role}.wal"))).unwrap();let mut db=Db::empty();assert_eq!(db.apply_wal(&bytes).unwrap(),2);assert_eq!(db.store.r#gen,2);assert_eq!(db.store.next_id,77);
   assert_eq!(rmp_serde::to_vec_named(db.store.collection(collection)).unwrap(),expected);
   assert_eq!(db.apply_wal(&bytes).unwrap(),0);assert_eq!(rmp_serde::to_vec_named(db.store.collection(collection)).unwrap(),expected);
   let mut broken=bytes.clone();*broken.last_mut().unwrap()^=1;assert!(lin::persist::for_each_wal_frame(&broken,|_|Ok(())).is_err());
  }
  println!("{}",serde_json::json!({"case":case,"legacy_primary_reader_passed":true,"both_streams_match_expected_bits":true,"generation_next_id_verified":true,"duplicate_apply_noop":true,"corruption_rejected":true,"performance_qualified":false}));
 }
}
