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
    let path=std::env::var_os("LIN_EXTENT_DB_CHILD_PATH").expect("child-only path");
    if let Some(ready)=std::env::var_os("LIN_EXTENT_DB_READ_READY") {
        let reader=Db::open_extent_read(PathBuf::from(&path)).unwrap();let query=r#"docs | id == "one" | {title}"#;
        let before=reader.run(query).unwrap().rows[0]["title"].text().unwrap().to_owned();
        let ready=PathBuf::from(ready);let pending=ready.with_extension("pending");fs::write(&pending,b"ready").unwrap();fs::rename(pending,ready).unwrap();
        let release=PathBuf::from(std::env::var_os("LIN_EXTENT_DB_READ_RELEASE").expect("reader release"));
        let deadline=std::time::Instant::now()+std::time::Duration::from_secs(15);
        while !release.exists(){assert!(std::time::Instant::now()<deadline,"reader child timeout");std::thread::sleep(std::time::Duration::from_millis(5));}
        assert_eq!(reader.run(query).unwrap().rows[0]["title"].text(),Some(before.as_str()));std::process::exit(87);
    }
    let mut db=Db::open_extent(PathBuf::from(path),65536).unwrap().without_embedder();db.run(INSERT).unwrap();db.run(r#"update docs[id=="one"] cas each {title:"acknowledged"}"#).unwrap();assert_eq!(db.r#gen(),2);std::process::exit(86);
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

#[test]
fn extent_read_only_is_stable_beside_writer_and_does_not_modify_files(){
    let t=Temp::new();let dir=t.path("read");let mut writer=Db::open_extent(&dir,65536).unwrap().without_embedder();writer.run(INSERT).unwrap();
    let log=lin::persist::log_path(&dir);let head=lin::persist::head_path(&dir);let before_log=fs::read(&log).unwrap();let before_head=fs::read(&head).unwrap();
    let reader=Db::open_extent_read(&dir).unwrap();let query=r#"docs | id == "one" | {title}"#;assert_eq!(reader.run(query).unwrap().rows[0]["title"].text(),Some("before"));
    assert!(reader.run(r#"insert docs {id:"forbidden"}"#).is_err());assert_eq!(fs::read(&log).unwrap(),before_log);assert_eq!(fs::read(&head).unwrap(),before_head);
    writer.run(r#"update docs[id=="one"] cas each {title:"later"}"#).unwrap();assert_eq!(reader.run(query).unwrap().rows[0]["title"].text(),Some("before"));
    let fresh=Db::open_extent_read(&dir).unwrap();assert_eq!(fresh.run(query).unwrap().rows[0]["title"].text(),Some("later"));drop(fresh);drop(reader);writer.checkpoint().unwrap();
    let after=Db::open_extent_read(&dir).unwrap();assert_eq!(after.run(query).unwrap().rows[0]["title"].text(),Some("later"));drop(after);
}
#[test]
fn extent_read_only_rejects_corruption_without_rewriting_it(){
    let t=Temp::new();let dir=t.path("corrupt-read");let mut db=Db::open_extent(&dir,65536).unwrap().without_embedder();db.run(INSERT).unwrap();drop(db);
    let path=lin::persist::log_path(&dir);let mut bytes=fs::read(&path).unwrap();bytes[30]^=1;fs::write(&path,&bytes).unwrap();let head=fs::read(lin::persist::head_path(&dir)).unwrap();
    assert!(Db::open_extent_read(&dir).is_err());assert_eq!(fs::read(path).unwrap(),bytes);assert_eq!(fs::read(lin::persist::head_path(&dir)).unwrap(),head);
}
#[cfg(unix)]
#[test]
fn separate_reader_process_holds_fence_and_exit_releases_checkpoint(){
    struct Guard(std::process::Child);impl Drop for Guard{fn drop(&mut self){let _=self.0.kill();let _=self.0.wait();}}
    let t=Temp::new();let dir=t.path("reader-process");let mut writer=Db::open_extent(&dir,65536).unwrap().without_embedder();writer.run(INSERT).unwrap();let ready=t.path("ready");let release=t.path("release");
    let mut child=Guard(std::process::Command::new(std::env::current_exe().unwrap()).args(["--exact","extent_db_child","--ignored","--nocapture"]).env("LIN_EXTENT_DB_CHILD_PATH",&dir).env("LIN_EXTENT_DB_READ_READY",&ready).env("LIN_EXTENT_DB_READ_RELEASE",&release).stdout(std::process::Stdio::null()).spawn().unwrap());
    let deadline=std::time::Instant::now()+std::time::Duration::from_secs(15);
    while !ready.exists(){assert!(child.0.try_wait().unwrap().is_none(),"reader exited before ready");assert!(std::time::Instant::now()<deadline,"reader ready timeout");std::thread::sleep(std::time::Duration::from_millis(5));}
    use std::os::fd::AsRawFd;
    let fence=fs::OpenOptions::new().read(true).write(true).open(lin::persist::fence_path(&dir)).unwrap();
    let denied=unsafe{libc::flock(fence.as_raw_fd(),libc::LOCK_EX|libc::LOCK_NB)};let error=std::io::Error::last_os_error();assert_eq!(denied,-1,"checkpoint fence unexpectedly available");assert_eq!(error.kind(),std::io::ErrorKind::WouldBlock);
    writer.run(r#"update docs[id=="one"] cas each {title:"new process value"}"#).unwrap();fs::write(release,b"release").unwrap();assert_eq!(child.0.wait().unwrap().code(),Some(87));
    assert_eq!(unsafe{libc::flock(fence.as_raw_fd(),libc::LOCK_EX|libc::LOCK_NB)},0);drop(fence);writer.checkpoint().unwrap();assert_eq!(writer.stats().log_bytes,0);
}

#[test]
fn bootstrap_extent_follower_restores_schema_then_applies_new_wal(){
    let t=Temp::new();let mut primary=Db::open_extent(t.path("primary-bootstrap"),65536).unwrap().without_embedder();
    primary.run("col bootstrap_custom {id:text,value:i64}").unwrap();primary.run(r#"insert bootstrap_custom {id:"c",value:7}"#).unwrap();primary.run(INSERT).unwrap();let covered=primary.r#gen();
    let backup=t.path("bootstrap.backup");primary.export_backup(&backup).unwrap();primary.run(r#"update bootstrap_custom[id=="c"] cas each {value:9}"#).unwrap();primary.run(r#"update docs[id=="one"] cas each {title:"after backup"}"#).unwrap();let frames=primary.export_wal_since(covered).unwrap();
    let dir=t.path("bootstrapped");let mut follower=Db::bootstrap_extent_follower(&backup,&dir,65536).unwrap();assert!(follower.is_follower());assert_eq!(follower.r#gen(),covered);
    assert_eq!(follower.run("bootstrap_custom | {value}").unwrap().rows[0]["value"].as_int(),Some(7));assert_eq!(follower.apply_wal(&frames).unwrap(),2);assert_eq!(follower.r#gen(),primary.r#gen());
    for collection in ["docs","bootstrap_custom"]{assert_eq!(rmp_serde::to_vec_named(follower.store.collection(collection)).unwrap(),rmp_serde::to_vec_named(primary.store.collection(collection)).unwrap());}
    assert!(follower.run(r#"insert bootstrap_custom {id:"forbidden",value:0}"#).is_err());follower.checkpoint().unwrap();drop(follower);
    let reader=Db::open_extent_read(&dir).unwrap();assert_eq!(reader.run("bootstrap_custom | {value}").unwrap().rows[0]["value"].as_int(),Some(9));assert_eq!(reader.run(r#"docs | id=="one" | {title}"#).unwrap().rows[0]["title"].text(),Some("after backup"));drop(reader);
    let reopened=Db::open_extent_follower(&dir,65536).unwrap();assert_eq!(reopened.r#gen(),primary.r#gen());assert_eq!(std::fs::metadata(lin::persist::log_path(&dir)).unwrap().len(),65536);
}
#[test]
fn bootstrap_extent_refuses_existing_targets_and_invalid_input_before_creation(){
    let t=Temp::new();let mut source=Db::open_extent(t.path("source"),65536).unwrap().without_embedder();source.run(INSERT).unwrap();let backup=t.path("backup");source.export_backup(&backup).unwrap();
    let legacy=t.path("legacy-target");let mut db=Db::open(&legacy).unwrap().without_embedder();db.run(INSERT).unwrap();drop(db);
    let paths=[lin::persist::log_path(&legacy),lin::persist::head_path(&legacy),lin::persist::snapshot_path(&legacy)];let before=paths.iter().map(|p|fs::read(p).unwrap()).collect::<Vec<_>>();
    assert!(Db::bootstrap_extent_follower(&backup,&legacy,65536).is_err());assert_eq!(paths.iter().map(|p|fs::read(p).unwrap()).collect::<Vec<_>>(),before);
    let empty=t.path("already-empty");fs::create_dir(&empty).unwrap();assert!(Db::bootstrap_extent_follower(&backup,&empty,65536).is_err());assert_eq!(fs::read_dir(empty).unwrap().count(),0);
    let invalid=t.path("invalid-backup");fs::write(&invalid,b"not a backup").unwrap();let target=t.path("invalid-target");assert!(Db::bootstrap_extent_follower(&invalid,&target,65536).is_err());assert!(!target.exists());
    let target=t.path("invalid-capacity-target");assert!(Db::bootstrap_extent_follower(&backup,&target,8193).is_err());assert!(!target.exists());
}
