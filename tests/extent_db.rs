#![cfg(feature="experimental-extent-wal")]
use lin::{Db,SyncMode};
use std::{path::PathBuf,fs,sync::atomic::{AtomicU64,Ordering}};
struct Temp(PathBuf);
impl Temp {
    fn new()->Self {static N:AtomicU64=AtomicU64::new(0);let p=std::env::temp_dir().join(format!("lin-extent-db-{}-{}-{}",std::process::id(),N.fetch_add(1,Ordering::Relaxed),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));fs::create_dir(&p).unwrap();Self(p)}
    fn path(&self,name:&str)->PathBuf{self.0.join(name)}
}
impl Drop for Temp {fn drop(&mut self){let _=fs::remove_dir_all(&self.0);}}
const INSERT:&str=r#"insert docs {id:"one",uri:"extent://one",title:"before",body:"payload",layer:"wiki"}"#;
#[test]
fn extent_commit_rollback_checkpoint_backup_and_follower_use_logical_bytes(){
    let t=Temp::new();let dir=t.path("primary");let mut db=Db::open_extent(&dir,65536).unwrap().without_embedder();db.run(INSERT).unwrap();
    let original=rmp_serde::to_vec_named(db.store.get_by_id("docs","one").unwrap()).unwrap();let old_log=db.stats().log_bytes;
    let mut tx=db.transaction().unwrap();tx.run(r#"update docs[id=="one"] cas each {title:"rolled back"}"#).unwrap();tx.rollback().unwrap();
    assert_eq!(db.stats().log_bytes,old_log);assert_eq!(rmp_serde::to_vec_named(db.store.get_by_id("docs","one").unwrap()).unwrap(),original);
    let prepared=db.prepare(r#"update docs[id=="one"] cas each {title:"committed"}"#).unwrap();
    let mut tx=db.transaction().unwrap();tx.run_prepared(&prepared).unwrap();tx.commit().unwrap();
    let frames=db.export_wal_since(0).unwrap();assert_eq!(frames.len()as u64,db.stats().log_bytes);assert!(db.stats().log_bytes<65536);
    let expected=rmp_serde::to_vec_named(db.store.get_by_id("docs","one").unwrap()).unwrap();
    let mut follower=Db::open_extent_follower(t.path("follower"),65536).unwrap();assert_eq!(follower.apply_wal(&frames).unwrap(),2);assert_eq!(follower.export_wal_since(0).unwrap(),frames);assert_eq!(rmp_serde::to_vec_named(follower.store.get_by_id("docs","one").unwrap()).unwrap(),expected);
    let mut memory=Db::empty();assert_eq!(memory.apply_wal(&frames).unwrap(),2);assert_eq!(rmp_serde::to_vec_named(memory.store.get_by_id("docs","one").unwrap()).unwrap(),expected);
    db.checkpoint().unwrap();assert_eq!(db.stats().log_bytes,0);assert_eq!(fs::metadata(lin::persist::log_path(&dir)).unwrap().len(),65536);
    db.run(r#"update docs[id=="one"] cas each {title:"after checkpoint"}"#).unwrap();let delta=db.export_wal_since(2).unwrap();assert_eq!(follower.apply_wal(&delta).unwrap(),1);
    let backup=t.path("backup.json");db.export_backup(&backup).unwrap();assert_eq!(db.stats().log_bytes,0);let restored=Db::import_backup(&backup).unwrap();assert_eq!(restored.store.get_by_id("docs","one").unwrap()["title"].text(),Some("after checkpoint"));
    drop(db);let before=fs::read(lin::persist::log_path(&dir)).unwrap();assert!(Db::open(&dir).is_err());assert_eq!(fs::read(lin::persist::log_path(&dir)).unwrap(),before);let reopened=Db::open_extent(&dir,65536).unwrap();assert_eq!(reopened.store.get_by_id("docs","one").unwrap()["title"].text(),Some("after checkpoint"));assert_eq!(fs::metadata(lin::persist::log_path(&dir)).unwrap().len(),65536);
}
#[test]
fn capacity_failure_and_normal_mode_refusal_restore_memory_and_wal(){
    let t=Temp::new();let dir=t.path("db");let mut db=Db::open_extent(&dir,8192).unwrap().without_embedder();db.run(INSERT).unwrap();
    let row=rmp_serde::to_vec_named(db.store.get_by_id("docs","one").unwrap()).unwrap();let generation=db.r#gen();let before=fs::read(lin::persist::log_path(&dir)).unwrap();
    let query=format!(r#"update docs[id=="one"] cas each {{title:"{}"}}"#,"x".repeat(8192));let error=db.run(&query).unwrap_err();assert!(error.to_string().contains("extent full"),"{error}");
    assert_eq!(db.r#gen(),generation);assert_eq!(rmp_serde::to_vec_named(db.store.get_by_id("docs","one").unwrap()).unwrap(),row);assert_eq!(fs::read(lin::persist::log_path(&dir)).unwrap(),before);
    db=db.with_sync_mode(SyncMode::Normal);assert!(db.run(r#"update docs[id=="one"] cas each {title:"refused"}"#).unwrap_err().to_string().contains("requires Full"));assert_eq!(db.r#gen(),generation);assert_eq!(fs::read(lin::persist::log_path(&dir)).unwrap(),before);
    db=db.with_sync_mode(SyncMode::Full);db.run(r#"update docs[id=="one"] cas each {title:"valid"}"#).unwrap();assert_eq!(db.r#gen(),generation+1);
}
#[test]
fn existing_legacy_log_is_never_converted(){
    let t=Temp::new();let dir=t.path("legacy");let mut db=Db::open(&dir).unwrap().without_embedder();db.run(INSERT).unwrap();drop(db);let path=lin::persist::log_path(&dir);let bytes=fs::read(&path).unwrap();assert!(Db::open_extent(&dir,65536).is_err());assert_eq!(fs::read(path).unwrap(),bytes);let old=Db::open(&dir).unwrap();assert!(old.store.get_by_id("docs","one").is_some());
}
#[test]
#[ignore="subprocess helper used by acknowledged_writes_replay_without_drop_checkpoint"]
fn extent_db_child(){
    let path=std::env::var_os("LIN_EXTENT_DB_CHILD_PATH").expect("child-only path");let mut db=Db::open_extent(PathBuf::from(path),65536).unwrap().without_embedder();db.run(INSERT).unwrap();db.run(r#"update docs[id=="one"] cas each {title:"acknowledged"}"#).unwrap();assert_eq!(db.r#gen(),2);std::process::exit(86);
}
#[test]
fn acknowledged_writes_replay_without_drop_checkpoint(){
    let t=Temp::new();let dir=t.path("unclean");let status=std::process::Command::new(std::env::current_exe().unwrap()).args(["--exact","extent_db_child","--ignored","--nocapture"]).env("LIN_EXTENT_DB_CHILD_PATH",&dir).status().unwrap();assert_eq!(status.code(),Some(86));
    let mut db=Db::open_extent(&dir,65536).unwrap();assert_eq!(db.r#gen(),2);assert_eq!(db.store.get_by_id("docs","one").unwrap()["title"].text(),Some("acknowledged"));assert!(!db.export_wal_since(0).unwrap().is_empty());db.checkpoint().unwrap();assert_eq!(db.stats().log_bytes,0);
}

#[test]
fn extent_columnar_bulk_ships_every_row_and_survives_checkpoint(){
    let t=Temp::new();let dir=t.path("bulk");let mut db=Db::open_extent(&dir,2*1024*1024).unwrap().without_embedder();
    let records=(0..4096).map(|i|format!(r#"{{id:"bulk-{i}",uri:"extent://bulk/{i}",title:"row {i}",body:"payload"}}"#)).collect::<Vec<_>>().join(",");
    db.run(&format!("insert docs [{records}]")).unwrap();assert_eq!(db.store.collection("docs").len(),4096);
    let raw=db.export_wal_since(0).unwrap();let mut columnar=0;
    lin::persist::for_each_wal_frame(&raw,|record|{if let lin::persist::Pack::InsertCols{n,..}=record.pack{assert_eq!(n,4096);columnar+=1;}Ok(())}).unwrap();assert_eq!(columnar,1);
    let mut memory=Db::empty();assert_eq!(memory.apply_wal(&raw).unwrap(),1);
    for row in db.store.collection("docs") {let id=row["id"].text().unwrap();assert_eq!(rmp_serde::to_vec_named(row).unwrap(),rmp_serde::to_vec_named(memory.store.get_by_id("docs",id).unwrap()).unwrap());}
    db.checkpoint().unwrap();assert_eq!(db.stats().log_bytes,0);drop(db);
    let reopened=Db::open_extent(&dir,2*1024*1024).unwrap();assert_eq!(reopened.store.collection("docs").len(),4096);assert_eq!(reopened.store.get_by_id("docs","bulk-4095").unwrap()["title"].text(),Some("row 4095"));
}
