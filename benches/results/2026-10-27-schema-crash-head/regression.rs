use lin::Db;
use std::sync::atomic::{AtomicU64, Ordering};
static SEQ: AtomicU64 = AtomicU64::new(0);
struct Scratch(std::path::PathBuf);
impl Scratch { fn new() -> Self {
    let path = std::env::temp_dir().join(format!("lin-schema-head-{}-{}",std::process::id(),SEQ.fetch_add(1,Ordering::Relaxed)));
    std::fs::create_dir(&path).unwrap(); Self(path)
}}
impl Drop for Scratch { fn drop(&mut self) { let _=std::fs::remove_dir_all(&self.0); } }
#[test]
fn schema_crash_writer_child() {
    let Some(path)=std::env::var_os("LIN_SCHEMA_CRASH_ROOT") else { return };
    let mut db=Db::open(path).unwrap();
    if std::env::var_os("LIN_SCHEMA_PRIOR_CHECKPOINT").is_some() {
        db.run(r#"insert docs {id:"seed",body:"seed"}"#).unwrap();
        db.checkpoint().unwrap();
    }
    if std::env::var_os("LIN_SCHEMA_TX").is_some() {
        let mut tx=db.transaction().unwrap();
        tx.run("col recovered_schema {id: text, score: i64}").unwrap();
        tx.run(r#"insert recovered_schema {id:"row",score:7}"#).unwrap();
        tx.commit().unwrap();
    } else {
        db.run("col recovered_schema {id: text, score: i64}").unwrap();
        db.run(r#"insert recovered_schema {id:"row",score:7}"#).unwrap();
    }
    std::process::exit(0);
}
fn recover(transaction: bool, checkpoint: bool) {
    let root=Scratch::new();
    let mut child=std::process::Command::new(std::env::current_exe().unwrap());
    child.args(["--exact","schema_crash_writer_child","--nocapture"]).env("LIN_SCHEMA_CRASH_ROOT",&root.0).env_remove("LIN_SCHEMA_TX").env_remove("LIN_SCHEMA_PRIOR_CHECKPOINT");
    if transaction {child.env("LIN_SCHEMA_TX","1");}
    if checkpoint {child.env("LIN_SCHEMA_PRIOR_CHECKPOINT","1");}
    assert!(child.status().unwrap().success());
    let result=Db::open(&root.0);
    eprintln!("SCHEMA_HEAD_PROBE {}",serde_json::json!({"transaction":transaction,"prior_checkpoint":checkpoint,"reopen_error":result.as_ref().err().map(|e|e.to_string())}));
    let db=result.expect("committed schema and rows must reopen after process exit");
    assert!(db.catalog.collection("recovered_schema").is_some());
    let row=db.store.get_by_id("recovered_schema","row").unwrap();
    assert_eq!(row["score"],lin::Cell::Int(7));
    assert_eq!(db.store.collection("recovered_schema").len(),1);
}
#[test] fn autocommit_schema_crash_without_snapshot() { recover(false,false); }
#[test] fn transaction_schema_crash_without_snapshot() { recover(true,false); }
#[test] fn autocommit_schema_crash_after_checkpoint() { recover(false,true); }
#[test] fn transaction_schema_crash_after_checkpoint() { recover(true,true); }
