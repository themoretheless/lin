use lin::{Db, Quotas};
use std::sync::atomic::{AtomicU64, Ordering};
static SEQ: AtomicU64 = AtomicU64::new(0);
struct Scratch(std::path::PathBuf);
impl Scratch {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "lin-tx-quota-{}-{}",
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
fn rejected_write_cannot_commit(journal: bool) {
    let root = Scratch::new();
    let mut db = Db::open(&root.0).unwrap();
    db.run(r#"insert docs {id:"a",uri:"u:a",body:"original",wing:"rag"}"#)
        .unwrap();
    let base = db.store.r#gen;
    let original = db.store.collection("docs").to_vec();
    let hash = db.store.get_by_id("docs", "a").unwrap()["hash"]
        .text()
        .unwrap()
        .to_owned();
    let mut quotas = Quotas::default();
    if journal {
        quotas.max_journal_bytes = 0;
    } else {
        quotas.max_tx_mutations = 0;
    }
    let mut db = db.with_quotas(quotas);
    let mut tx = db.transaction().unwrap();
    let error = tx
        .run(&format!(
            r#"update docs[id=="a"] cas "{hash}" {{wing:"sys"}}"#
        ))
        .unwrap_err();
    assert!(error.to_string().contains(if journal {
        "max_journal_bytes"
    } else {
        "max_tx_mutations"
    }));
    let result = tx.commit();
    eprintln!(
        "TX_QUOTA_PROBE {}",
        serde_json::json!({"journal":journal,"run_error":error.to_string(),"commit_error":result.as_ref().err().map(|e|e.to_string()),"generation":db.store.r#gen,"wing":db.store.get_by_id("docs","a").unwrap()["wing"].text(),"wal_after_rejected_write_bytes":db.export_wal_since(base).unwrap().len()})
    );
    assert!(result.is_err(), "a failed transaction must refuse commit");
    assert_eq!(db.store.r#gen, base);
    assert_eq!(db.store.collection("docs"), original);
    assert!(db.export_wal_since(base).unwrap().is_empty());
}
#[test]
fn mutation_quota_rejection_prevents_later_commit() {
    rejected_write_cannot_commit(false);
}
#[test]
fn journal_quota_rejection_prevents_later_commit() {
    rejected_write_cannot_commit(true);
}

fn seeded_pair() -> (Scratch, Db, Vec<lin::Row>, u64) {
    let root = Scratch::new();
    let mut db = Db::open(&root.0).unwrap();
    db.run(r#"insert docs [{id:"a",uri:"u:a",body:"a",wing:"rag"},{id:"b",uri:"u:b",body:"b",wing:"rag"}]"#).unwrap();
    let original = db.store.collection("docs").to_vec();
    let base = db.store.r#gen;
    (root, db, original, base)
}
fn update(id: &str) -> String {
    format!(r#"update docs[id=="{id}"] cas each {{wing:"sys"}}"#)
}

#[test]
fn quota_failure_rolls_back_already_accepted_packs() {
    let (_root, db, original, base) = seeded_pair();
    let mut quotas = Quotas::default();
    quotas.max_tx_mutations = 1;
    let mut db = db.with_quotas(quotas);
    let mut tx = db.transaction().unwrap();
    tx.run(&update("a")).unwrap();
    let error = tx.run(&update("b")).unwrap_err();
    assert!(error.to_string().contains("max_tx_mutations"));
    assert!(
        tx.run("docs | count")
            .unwrap_err()
            .to_string()
            .contains("aborted")
    );
    assert!(tx.prepare("docs | count").is_err());
    assert!(tx.commit().is_err());
    assert_eq!(db.store.collection("docs"), original);
    assert_eq!(db.store.r#gen, base);
    assert!(db.export_wal_since(base).unwrap().is_empty());
}

#[test]
fn execution_failure_prevents_prepared_run_and_commit() {
    let (_root, mut db, original, base) = seeded_pair();
    let prepared = db.prepare(&update("b")).unwrap();
    let mut tx = db.transaction().unwrap();
    tx.run(&update("a")).unwrap();
    let error = tx
        .run(r#"update docs[id=="b"] cas "stale" {wing:"sys"}"#)
        .unwrap_err();
    assert!(!error.to_string().contains("transaction aborted"));
    assert!(
        tx.run_prepared(&prepared)
            .unwrap_err()
            .to_string()
            .contains("aborted")
    );
    assert!(tx.commit().is_err());
    assert_eq!(db.store.collection("docs"), original);
    assert_eq!(db.store.r#gen, base);
    assert!(db.export_wal_since(base).unwrap().is_empty());
}

#[test]
fn explicit_rollback_and_drop_remain_available_after_failed_run() {
    for explicit in [false, true] {
        let (_root, mut db, original, base) = seeded_pair();
        let mut tx = db.transaction().unwrap();
        tx.run(&update("a")).unwrap();
        assert!(tx.run("this is not a valid query !!!").is_err());
        if explicit {
            tx.rollback().unwrap();
        } else {
            drop(tx);
        }
        assert_eq!(db.store.collection("docs"), original);
        assert_eq!(db.store.r#gen, base);
        assert!(db.export_wal_since(base).unwrap().is_empty());
        db.run(&update("a")).unwrap();
        assert_eq!(db.store.r#gen, base + 1);
    }
}
