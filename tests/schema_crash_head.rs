use lin::Db;
use std::sync::atomic::{AtomicU64, Ordering};
static SEQ: AtomicU64 = AtomicU64::new(0);
struct Scratch(std::path::PathBuf);
impl Scratch {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "lin-schema-head-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
#[test]
fn schema_crash_writer_child() {
    let Some(path) = std::env::var_os("LIN_SCHEMA_CRASH_ROOT") else {
        return;
    };
    let root = std::path::PathBuf::from(path);
    let mut db = Db::open(&root).unwrap();
    if std::env::var_os("LIN_SCHEMA_PRIOR_CHECKPOINT").is_some() {
        db.run(r#"insert docs {id:"seed",body:"seed"}"#).unwrap();
        db.checkpoint().unwrap();
    }
    if std::env::var_os("LIN_SCHEMA_MIDDLE_HEAD").is_some() {
        db.run(r#"insert docs {id:"middle",body:"middle"}"#)
            .unwrap();
        let mut head = lin::persist::read_head(&root).unwrap().unwrap();
        head.r#gen = db.store.r#gen;
        lin::persist::write_head(&root, &head).unwrap();
    }
    let prior_head = std::fs::read(lin::persist::head_path(&root)).unwrap();
    if std::env::var_os("LIN_SCHEMA_TX").is_some() {
        let mut tx = db.transaction().unwrap();
        tx.run("col recovered_schema {id: text, score: i64}")
            .unwrap();
        tx.run(r#"insert recovered_schema {id:"row",score:7}"#)
            .unwrap();
        tx.commit().unwrap();
    } else {
        db.run("col recovered_schema {id: text, score: i64}")
            .unwrap();
        db.run(r#"insert recovered_schema {id:"row",score:7}"#)
            .unwrap();
    }
    if std::env::var_os("LIN_SCHEMA_SNAPSHOT_AHEAD").is_some() {
        // Model the real publication window after a durable new snapshot,
        // before HEAD replacement, with the pre-checkpoint WAL still present.
        let wal = std::fs::read(lin::persist::log_path(&root)).unwrap();
        db.checkpoint().unwrap();
        for (path, bytes) in [
            (lin::persist::head_path(&root), prior_head),
            (lin::persist::log_path(&root), wal),
        ] {
            std::fs::write(&path, bytes).unwrap();
            std::fs::File::open(path).unwrap().sync_all().unwrap();
        }
    }
    std::process::exit(0);
}
fn recover(transaction: bool, checkpoint: bool) {
    let root = Scratch::new();
    let mut child = std::process::Command::new(std::env::current_exe().unwrap());
    child
        .args(["--exact", "schema_crash_writer_child", "--nocapture"])
        .env("LIN_SCHEMA_CRASH_ROOT", &root.0)
        .env_remove("LIN_SCHEMA_TX")
        .env_remove("LIN_SCHEMA_PRIOR_CHECKPOINT")
        .env_remove("LIN_SCHEMA_MIDDLE_HEAD")
        .env_remove("LIN_SCHEMA_SNAPSHOT_AHEAD");
    if transaction {
        child.env("LIN_SCHEMA_TX", "1");
    }
    if checkpoint {
        child.env("LIN_SCHEMA_PRIOR_CHECKPOINT", "1");
    }
    assert!(child.status().unwrap().success());
    let result = Db::open(&root.0);
    eprintln!(
        "SCHEMA_HEAD_PROBE {}",
        serde_json::json!({"transaction":transaction,"prior_checkpoint":checkpoint,"reopen_error":result.as_ref().err().map(|e|e.to_string())})
    );
    let db = result.expect("committed schema and rows must reopen after process exit");
    assert!(db.catalog.collection("recovered_schema").is_some());
    let row = db.store.get_by_id("recovered_schema", "row").unwrap();
    assert_eq!(row["score"], lin::Cell::Int(7));
    assert_eq!(db.store.collection("recovered_schema").len(), 1);
}
#[test]
fn autocommit_schema_crash_without_snapshot() {
    recover(false, false);
}
#[test]
fn transaction_schema_crash_without_snapshot() {
    recover(true, false);
}
#[test]
fn autocommit_schema_crash_after_checkpoint() {
    recover(false, true);
}
#[test]
fn transaction_schema_crash_after_checkpoint() {
    recover(true, true);
}

fn model_root(snapshot_ahead: bool, middle_head: bool) -> Scratch {
    let root = Scratch::new();
    let mut child = std::process::Command::new(std::env::current_exe().unwrap());
    child
        .args(["--exact", "schema_crash_writer_child", "--nocapture"])
        .env("LIN_SCHEMA_CRASH_ROOT", &root.0)
        .env_remove("LIN_SCHEMA_TX")
        .env_remove("LIN_SCHEMA_PRIOR_CHECKPOINT")
        .env_remove("LIN_SCHEMA_MIDDLE_HEAD")
        .env_remove("LIN_SCHEMA_SNAPSHOT_AHEAD");
    if snapshot_ahead {
        child.env("LIN_SCHEMA_SNAPSHOT_AHEAD", "1");
    }
    if middle_head {
        child.env("LIN_SCHEMA_MIDDLE_HEAD", "1");
    }
    assert!(child.status().unwrap().success());
    root
}
fn assert_schema_row(root: &Scratch) {
    let db = Db::open(&root.0).unwrap();
    assert!(db.catalog.collection("recovered_schema").is_some());
    assert_eq!(db.store.collection("recovered_schema").len(), 1);
    assert_eq!(
        db.store.get_by_id("recovered_schema", "row").unwrap()["score"],
        lin::Cell::Int(7)
    );
}
#[test]
fn modeled_new_snapshot_supersedes_stale_head() {
    let root = model_root(true, false);
    assert_schema_row(&root);
}
#[test]
fn modeled_head_catalog_is_checked_at_its_wal_generation() {
    let root = model_root(false, true);
    assert_schema_row(&root);
}
fn corrupt_head(root: &Scratch) {
    let mut head = lin::persist::read_head(&root.0).unwrap().unwrap();
    head.catalog_hash = "h:corrupt".to_owned();
    lin::persist::write_head(&root.0, &head).unwrap();
}
fn assert_catalog_rejected(root: &Scratch) {
    let error = Db::open(&root.0)
        .err()
        .expect("corrupt catalog metadata must fail");
    assert!(
        error.to_string().contains("catalog_hash mismatch"),
        "{error}"
    );
}
#[test]
fn corrupt_head_at_checkpoint_generation_still_rejected() {
    let root = Scratch::new();
    let mut db = Db::open(&root.0).unwrap();
    db.run(r#"insert docs {id:"seed",body:"seed"}"#).unwrap();
    db.checkpoint().unwrap();
    drop(db);
    corrupt_head(&root);
    assert_catalog_rejected(&root);
}
#[test]
fn corrupt_stale_head_before_schema_wal_still_rejected() {
    let root = model_root(false, false);
    corrupt_head(&root);
    assert_catalog_rejected(&root);
}
#[test]
fn unverified_snapshot_cannot_override_stale_head() {
    let root = model_root(true, false);
    let mut snapshot = lin::persist::read_snapshot(&root.0).unwrap().unwrap();
    snapshot.catalog_hash = "h:unverified".to_owned();
    lin::persist::write_snapshot(&root.0, &snapshot).unwrap();
    assert_catalog_rejected(&root);
}
