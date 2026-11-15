//! Backups that fail part-way leave everything as it was: an export to a path that
//! cannot be written is an error and the database keeps working and reopens; a damaged
//! or truncated backup is refused on import, and importing it into an existing data
//! directory does not touch that directory.

use lin::Db;

fn tmp(tag: &str) -> std::path::PathBuf {
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let d = std::env::temp_dir().join(format!("lin-backup-{tag}-{}-{n}-{seq}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn rows(db: &mut Db) -> Vec<String> {
    let mut v: Vec<String> = db.run("t | { id, v } | take all").unwrap().rows.iter()
        .map(|r| format!("{}={}", r.get("id").and_then(|c| c.text()).unwrap(), r.get("v").and_then(|c| c.as_f64()).unwrap()))
        .collect();
    v.sort();
    v
}

#[test]
fn failed_export_leaves_the_database_usable() {
    let dir = tmp("src");
    let mut db = Db::open(&dir).unwrap();
    db.run("col t { id: text, v: i64 }").unwrap();
    db.run(r#"insert t [{ id: "a", v: 1 }, { id: "b", v: 2 }]"#).unwrap();
    let before = rows(&mut db);
    // The parent of the target is a regular file: no directory can be created there.
    let blocker = dir.join("blocker");
    std::fs::write(&blocker, b"x").unwrap();
    let bad = blocker.join("backup.json");
    assert!(db.export_backup(&bad).is_err(), "export under a regular file succeeded");
    assert!(!bad.exists());
    assert_eq!(rows(&mut db), before);
    db.run(r#"insert t { id: "c", v: 3 }"#).unwrap();
    drop(db);
    let mut db = Db::open(&dir).unwrap();
    assert_eq!(rows(&mut db), vec!["a=1", "b=2", "c=3"]);
}

#[test]
fn damaged_backups_are_refused_and_do_not_touch_the_target() {
    let src = tmp("orig");
    let file = tmp("file").join("backup.json");
    {
        let mut db = Db::open(&src).unwrap();
        db.run("col t { id: text, v: i64 }").unwrap();
        let ins = (0..50).map(|i| format!(r#"{{ id: "r{i}", v: {i} }}"#)).collect::<Vec<_>>().join(",");
        db.run(&format!("insert t [{ins}]")).unwrap();
        db.export_backup(&file).unwrap();
    }
    let good = std::fs::read(&file).unwrap();
    let mut restored = Db::import_backup(&file).unwrap();
    assert_eq!(rows(&mut restored).len(), 50);

    // An existing data directory that a bad import must leave alone.
    let target = tmp("target");
    {
        let mut db = Db::open(&target).unwrap();
        db.run("col t { id: text, v: i64 }").unwrap();
        db.run(r#"insert t { id: "keep", v: 7 }"#).unwrap();
    }
    let damaged: Vec<(&str, Vec<u8>)> = vec![
        ("empty", Vec::new()),
        ("truncated half", good[..good.len() / 2].to_vec()),
        ("truncated tail", good[..good.len() - 3].to_vec()),
        ("garbage", b"{ not a backup".to_vec()),
    ];
    for (what, bytes) in damaged {
        std::fs::write(&file, &bytes).unwrap();
        assert!(Db::import_backup(&file).is_err(), "{what}: import accepted");
        assert!(Db::import_backup_into(&file, &target).is_err(), "{what}: import_into accepted");
        let mut db = Db::open(&target).unwrap();
        assert_eq!(rows(&mut db), vec!["keep=7"], "{what}: target changed");
    }
}
