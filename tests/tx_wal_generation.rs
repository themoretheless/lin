use lin::Db;
use std::sync::atomic::{AtomicU64, Ordering};
static SEQ: AtomicU64 = AtomicU64::new(0);
struct Scratch(std::path::PathBuf);
impl Scratch {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "lin-tx-gen-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&p).unwrap();
        Self(p)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn seeded() -> (Scratch, Db, u64) {
    let root = Scratch::new();
    let mut db = Db::open(&root.0).unwrap();
    let generation=db.run(r#"insert docs [{id:"a",uri:"u:a",body:"a",wing:"rag"},{id:"b",uri:"u:b",body:"b",wing:"rag"}]"#).unwrap().done.r#gen;
    (root, db, generation)
}
fn update(db: &Db, id: &str) -> String {
    let hash = db.store.get_by_id("docs", id).unwrap()["hash"]
        .text()
        .unwrap();
    format!(r#"update docs[id=="{id}"] cas "{hash}" {{wing:"sys"}}"#)
}
fn gens(frames: &[u8]) -> Vec<u64> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < frames.len() {
        let len = u32::from_le_bytes(frames[i + 4..i + 8].try_into().unwrap()) as usize;
        assert_eq!(frames[i + 8], 1);
        let value: serde_json::Value =
            rmp_serde::from_slice(&frames[i + 9..i + 8 + len - 4]).unwrap();
        out.push(value["gen"].as_u64().unwrap());
        i += 8 + len;
    }
    out
}
#[test]
fn rollback_keeps_next_wal_generation_contiguous() {
    let (_root, mut db, base) = seeded();
    let mut follower = Db::empty();
    follower
        .apply_wal(&db.export_wal_since(0).unwrap())
        .unwrap();
    let sql = update(&db, "a");
    {
        let mut tx = db.transaction().unwrap();
        tx.run(&sql).unwrap();
        tx.rollback().unwrap();
    }
    let after_rollback = db.store.r#gen;
    db.run(&sql).unwrap();
    let frames = db.export_wal_since(base).unwrap();
    let result = follower.apply_wal(&frames);
    eprintln!(
        "GEN_PROBE {}",
        serde_json::json!({"case":"rollback","base":base,"after_rollback":after_rollback,"current":db.store.r#gen,"wal_generations":gens(&frames),"follower_error":result.as_ref().err().map(|e|e.to_string())})
    );
    assert!(
        result.is_ok(),
        "follower must accept next committed generation after rollback: {result:?}"
    );
    assert_eq!(after_rollback, base);
    assert_eq!(
        follower.store.collection("docs"),
        db.store.collection("docs")
    );
}
#[test]
fn multi_statement_commit_is_one_contiguous_atomic_frame() {
    let (_root, mut db, base) = seeded();
    let mut follower = Db::empty();
    follower
        .apply_wal(&db.export_wal_since(0).unwrap())
        .unwrap();
    let a = update(&db, "a");
    let b = update(&db, "b");
    {
        let mut tx = db.transaction().unwrap();
        tx.run(&a).unwrap();
        tx.run(&b).unwrap();
        tx.commit().unwrap();
    }
    let frames = db.export_wal_since(base).unwrap();
    let generations = gens(&frames);
    let result = follower.apply_wal(&frames);
    eprintln!(
        "GEN_PROBE {}",
        serde_json::json!({"case":"multi_commit","base":base,"current":db.store.r#gen,"wal_generations":generations,"follower_error":result.as_ref().err().map(|e|e.to_string())})
    );
    assert!(
        result.is_ok(),
        "follower must accept transaction commit: {result:?}"
    );
    assert_eq!(generations, vec![base + 1]);
    assert_eq!(db.store.r#gen, base + 1);
    assert_eq!(
        follower.store.collection("docs"),
        db.store.collection("docs")
    );
}

#[test]
fn dropped_and_read_only_transactions_preserve_committed_generation() {
    let (_root, mut db, base) = seeded();
    let original = db.store.collection("docs").to_vec();
    let sql = update(&db, "a");
    {
        let mut tx = db.transaction().unwrap();
        assert_eq!(tx.run(&sql).unwrap().done.r#gen, base);
        assert_eq!(tx.db().store.r#gen, base);
    }
    assert_eq!(db.store.r#gen, base);
    assert_eq!(db.store.collection("docs"), original);
    db.transaction().unwrap().commit().unwrap();
    assert_eq!(db.store.r#gen, base);
    assert!(db.export_wal_since(base).unwrap().is_empty());
}

#[test]
fn every_truncated_commit_frame_leaves_follower_unchanged() {
    let (_root, mut db, base) = seeded();
    let initial = db.export_wal_since(0).unwrap();
    let a = update(&db, "a");
    let b = update(&db, "b");
    {
        let mut tx = db.transaction().unwrap();
        tx.run(&a).unwrap();
        tx.run(&b).unwrap();
        tx.commit().unwrap();
    }
    let frames = db.export_wal_since(base).unwrap();
    assert_eq!(gens(&frames), vec![base + 1]);
    for end in 0..frames.len() {
        let mut follower = Db::empty();
        follower.apply_wal(&initial).unwrap();
        let original = follower.store.collection("docs").to_vec();
        let _ = follower.apply_wal(&frames[..end]);
        assert_eq!(follower.store.r#gen, base, "truncation {end}");
        assert_eq!(
            follower.store.collection("docs"),
            original,
            "truncation {end}"
        );
    }
}

#[test]
fn in_memory_multi_write_commit_advances_generation_once() {
    let mut db = Db::empty();
    let base = db.store.r#gen;
    let mut tx = db.transaction().unwrap();
    tx.run(r#"insert docs {id:"a",body:"a"}"#).unwrap();
    tx.run(r#"insert docs {id:"b",body:"b"}"#).unwrap();
    tx.commit().unwrap();
    assert_eq!(db.store.r#gen, base + 1);
    assert_eq!(db.store.collection("docs").len(), 2);
}

#[test]
fn checkpoint_failure_after_wal_publication_keeps_transaction_committed() {
    let (root, mut db, _) = seeded();
    for _ in 1..lin::persist::SNAPSHOT_EVERY - 1 {
        let sql = update(&db, "a");
        db.run(&sql).unwrap();
    }
    let base = db.store.r#gen;
    let mut follower = Db::empty();
    follower
        .apply_wal(&db.export_wal_since(0).unwrap())
        .unwrap();
    // An owned directory at the fence path deterministically rejects opening a
    // checkpoint fence, without affecting the already open WAL descriptor.
    let fence = lin::persist::fence_path(&root.0);
    std::fs::remove_file(&fence).unwrap();
    std::fs::create_dir(&fence).unwrap();
    let sql = update(&db, "b");
    let mut tx = db.transaction().unwrap();
    tx.run(&sql).unwrap();
    assert!(tx.commit().is_err());
    assert_eq!(db.store.r#gen, base + 1);
    assert_eq!(
        db.store.get_by_id("docs", "b").unwrap()["wing"].text(),
        Some("sys")
    );
    let frames = db.export_wal_since(base).unwrap();
    assert_eq!(gens(&frames), vec![base + 1]);
    follower.apply_wal(&frames).unwrap();
    assert_eq!(
        follower.store.collection("docs"),
        db.store.collection("docs")
    );
    std::fs::remove_dir(&fence).unwrap();
    drop(db);
    let reopened = Db::open(&root.0).unwrap();
    assert_eq!(reopened.store.r#gen, base + 1);
    assert_eq!(
        reopened.store.collection("docs"),
        follower.store.collection("docs")
    );
}
