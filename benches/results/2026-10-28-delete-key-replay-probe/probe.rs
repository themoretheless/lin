use lin::{Cell,Row,Store};
use lin::persist::{Pack,LogRecord,append_record,for_each_wal_frame};
use std::{fs,io::Seek,path::PathBuf};
fn keys(collection:&str,row:&Row)->Row {
 if collection!="facts"&&row.get("id").and_then(Cell::text).is_none()&&row.get("uri").and_then(Cell::text).is_none(){return row.clone();}
 let fields: &[&str]=if collection=="facts"{&["id","uri","s","p","o"]}else{&["id","uri"]};
 fields.iter().filter_map(|k|row.get(*k).map(|v|((*k).to_string(),v.clone()))).collect()
}
fn compact(pack:&Pack)->Pack {match pack {
 Pack::Delete{collection,rows}=>Pack::Delete{collection:collection.clone(),rows:rows.iter().map(|r|keys(collection,r)).collect()},
 Pack::Batch{packs}=>Pack::Batch{packs:packs.iter().map(compact).collect()},
 other=>other.clone(),
}}
fn main(){
 let dest=PathBuf::from(std::env::args().nth(1).unwrap());assert!(!dest.exists());fs::create_dir(&dest).unwrap();
 for case in ["id_uri","uri_only","stale_id_uri_fallback","facts_tuple","facts_missing_part","custom_id","custom_no_identity","nested_batch"] {
  let collection=if case.starts_with("facts"){"facts"}else if case.starts_with("custom"){"custom"}else{"docs"};
  let mut rows=Vec::new();
  for i in 0..8 {
   let mut row=Row::new();
   if case!="custom_no_identity"&&!case.starts_with("facts") {row.insert("id".into(),Cell::from_text(format!("id-{i}")));row.insert("uri".into(),Cell::from_text(format!("uri:{i}")));}
   if collection=="facts" {row.insert("s".into(),Cell::from_text(format!("s{i}")));row.insert("p".into(),Cell::from_text("predicate"));if case!="facts_missing_part" {row.insert("o".into(),Cell::from_text(format!("o{i}")));}}
   row.insert("title".into(),Cell::from_text(format!("title-{i}")));row.insert("layer".into(),Cell::from_text("layer"));row.insert("wing".into(),Cell::from_text("rag"));row.insert("body".into(),Cell::from_text("full logical body retained in prefix"));
   row.insert("embedding".into(),Cell::Vec((0..768).map(|n|if n%2==0{0.25}else{-0.0}).collect::<Vec<f32>>().into()));rows.push(row);
  }
  let prefix=Pack::InsertBulk{collection:collection.into(),rows:rows.clone(),edges:vec![]};
  let mut dead=vec![rows[1].clone(),rows[6].clone(),rows[1].clone()];
  if case=="uri_only" {for row in &mut dead{row.remove("id");}}
  if case=="stale_id_uri_fallback" {for row in &mut dead{row.insert("id".into(),Cell::from_text("missing-id"));}}
  let deletion=Pack::Delete{collection:collection.into(),rows:dead};let deletion=if case=="nested_batch"{Pack::Batch{packs:vec![Pack::Batch{packs:vec![deletion]}]}}else{deletion};
  let mut stores=Vec::new();let mut lengths=Vec::new();
  for (role,pack)in[("full",deletion.clone()),("keys",compact(&deletion))] {
   let path=dest.join(format!("{case}-{role}.wal"));let mut file=fs::OpenOptions::new().read(true).write(true).create_new(true).open(&path).unwrap();let mut written=0;let mut buf=Vec::new();
   append_record(&mut file,&LogRecord{r#gen:1,next_id:77,pack:prefix.clone()},&mut buf,lin::SyncMode::Normal,&mut written).unwrap();let prefix_len=written;
   append_record(&mut file,&LogRecord{r#gen:2,next_id:77,pack},&mut buf,lin::SyncMode::Normal,&mut written).unwrap();assert_eq!(file.stream_position().unwrap(),written);drop(file);
   let bytes=fs::read(path).unwrap();let mut store=Store::empty("key-replay-probe");let mut generation=0;
   assert_eq!(for_each_wal_frame(&bytes,|record|{generation+=1;assert_eq!(record.r#gen,generation);assert_eq!(record.next_id,77);store.apply_pack(&record.pack);Ok(())}).unwrap(),2);
   let mut broken=bytes.clone();*broken.last_mut().unwrap()^=1;assert!(for_each_wal_frame(&broken,|_|Ok(())).is_err());
   lengths.push(written-prefix_len);stores.push(store);
  }
  assert_eq!(rmp_serde::to_vec_named(stores[0].collection(collection)).unwrap(),rmp_serde::to_vec_named(stores[1].collection(collection)).unwrap());
  let expected=if case=="custom_no_identity"{8}else{6};assert_eq!(stores[1].collection(collection).len(),expected);
  if collection=="docs"{assert_eq!(stores[0].docs_wing(),stores[1].docs_wing());for i in 0..8{assert_eq!(stores[0].get_by_id(collection,&format!("id-{i}")),stores[1].get_by_id(collection,&format!("id-{i}")));assert_eq!(stores[0].get_by_uri(&format!("uri:{i}")),stores[1].get_by_uri(&format!("uri:{i}")));}assert_eq!(stores[0].docs_title_contains_count("title"),stores[1].docs_title_contains_count("title"));}
  if case=="custom_no_identity"{assert_eq!(lengths[0],lengths[1]);}else{assert!(lengths[1]<lengths[0]);}
  fs::write(dest.join(format!("{case}.expected")),rmp_serde::to_vec_named(stores[0].collection(collection)).unwrap()).unwrap();
  println!("{}",serde_json::json!({"case":case,"collection":collection,"full_delete_frame_bytes":lengths[0],"key_delete_frame_bytes":lengths[1],"survivors":expected,"canonical_state_bits_equal":true,"crc_corruption_rejected":true,"production_source_changed":false,"legacy_reader_verified":false,"performance_qualified":false}));
 }
}
