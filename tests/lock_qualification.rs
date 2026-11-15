//! The data-directory lock: a second writer is refused with an error that says the
//! directory is locked; a lock file left behind by a crashed process (present on disk but
//! held by no one) does not block the next open; and a writer that panics releases the
//! lock as it unwinds, so the directory opens again in the same process.

use lin::Db;

fn tmp() -> std::path::PathBuf {
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let d = std::env::temp_dir().join(format!("lin-lock-{}-{}", std::process::id(), SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn lock_refusal_stale_files_and_panics() {
    let dir = tmp();
    let mut a = Db::open(&dir).unwrap();
    a.run(r#"insert docs { uri: "raw://lk/1", title: "one", layer: "raw" }"#).unwrap();
    let err = Db::open(&dir).err().expect("second writer accepted");
    let msg = err.to_string();
    assert!(msg.contains("lock"), "refusal does not mention the lock: {msg}");
    drop(a);

    // A lock file nobody holds (left by a crash) does not block opening.
    let lock = std::fs::read_dir(&dir).unwrap().filter_map(|e| e.ok()).map(|e| e.path()).find(|p| p.file_name().is_some_and(|n| n.to_string_lossy().to_uppercase().contains("LOCK")));
    if let Some(lock) = &lock {
        std::fs::write(lock, b"12345 stale").unwrap();
    } else {
        std::fs::write(dir.join("LOCK"), b"12345 stale").unwrap();
    }
    let mut b = Db::open(&dir).expect("stale lock file blocked the open");
    assert_eq!(b.run("docs | count").unwrap().rows[0].get("hits").and_then(|c| c.as_f64()), Some(1.0));
    drop(b);

    // A writer that panics releases the lock while unwinding.
    let d2 = dir.clone();
    let r = std::thread::spawn(move || {
        let mut db = Db::open(&d2).unwrap();
        db.run(r#"insert docs { uri: "raw://lk/2", title: "two", layer: "raw" }"#).unwrap();
        panic!("writer fails while holding the lock");
    }).join();
    assert!(r.is_err());
    let mut c = Db::open(&dir).expect("lock not released after a panic");
    assert_eq!(c.run("docs | count").unwrap().rows[0].get("hits").and_then(|c| c.as_f64()), Some(2.0));
}
