//! Readers of the cold tier keep working while the writer replaces segment files: a
//! snapshot taken before a checkpoint and a compaction still answers with its own rows
//! (segment files it mapped stay valid after being replaced on disk), and new readers
//! see the new state.

use lin::Db;

fn count(db: &lin::ReadDb, q: &str) -> f64 {
    db.run(q).unwrap().rows[0].get("hits").and_then(|c| c.as_f64()).unwrap()
}

#[test]
fn snapshots_survive_segment_replacement() {
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!("lin-cold-reader-{}-{}", std::process::id(), SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let opts = || lin::OpenOpts { sync: lin::SyncMode::Normal, cold: true };
    {
        let mut db = Db::open_with(&dir, opts()).unwrap();
        db.run("col ev { id: text, k: i64 }").unwrap();
        let rows = (0..3000).map(|i| format!(r#"{{ id: "e{i}", k: {} }}"#, i % 10)).collect::<Vec<_>>().join(",");
        db.run(&format!("insert ev [{rows}]")).unwrap();
        db.checkpoint().unwrap();
        db.close().unwrap();
    }
    let mut db = Db::open_with(&dir, opts()).unwrap();
    assert!(db.store.is_disk_backed("ev"));
    let old = db.reader();
    let before = count(&old, "ev | k == 3 | count");
    assert_eq!(before, 300.0);
    // Change the data, write new segments, then compact (rewrites segment files).
    db.run(r#"delete ev[k == 3] cas each"#).unwrap();
    let more = (0..500).map(|i| format!(r#"{{ id: "n{i}", k: 3 }}"#)).collect::<Vec<_>>().join(",");
    db.run(&format!("insert ev [{more}]")).unwrap();
    db.checkpoint().unwrap();
    let _ = db.compact_collection("ev");
    db.checkpoint().unwrap();
    for _ in 0..3 {
        assert_eq!(count(&old, "ev | k == 3 | count"), before, "old snapshot changed after segment replacement");
        assert_eq!(count(&old, "ev | count"), 3000.0);
    }
    let new = db.reader();
    assert_eq!(count(&new, "ev | k == 3 | count"), 500.0);
    assert_eq!(count(&new, "ev | count"), 3200.0);
    drop(old);
    drop(db);
    let reopened = Db::open_with(&dir, opts()).unwrap().reader();
    assert_eq!(count(&reopened, "ev | k == 3 | count"), 500.0);
}
