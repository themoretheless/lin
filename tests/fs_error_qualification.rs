//! Filesystem failures surface as errors, never panics or silent loss: a data directory
//! that turns read-only refuses writes without changing what readers see, and the data
//! is intact once the directory is writable again.

#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use lin::Db;

fn tmp() -> std::path::PathBuf {
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let d = std::env::temp_dir().join(format!("lin-fs-err-{}-{n}-{seq}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn set_mode(dir: &Path, file_mode: u32, dir_mode: u32) {
    for e in std::fs::read_dir(dir).unwrap() {
        let e = e.unwrap();
        if e.file_type().unwrap().is_dir() {
            set_mode(&e.path(), file_mode, dir_mode);
        } else {
            std::fs::set_permissions(e.path(), std::fs::Permissions::from_mode(file_mode)).unwrap();
        }
    }
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(dir_mode)).unwrap();
}

fn count(db: &mut Db) -> f64 {
    db.run("t | count").unwrap().rows[0].get("hits").and_then(|c| c.as_f64()).unwrap()
}

#[test]
fn read_only_directory_refuses_writes_and_keeps_data() {
    // Root ignores permission bits; the check is meaningless there.
    if unsafe { libc_geteuid() } == 0 {
        return;
    }
    let dir = tmp();
    {
        let mut db = Db::open(&dir).unwrap();
        db.run("col t { id: text, v: i64 }").unwrap();
        db.run(r#"insert t [{ id: "a", v: 1 }, { id: "b", v: 2 }]"#).unwrap();
        db.checkpoint().unwrap();
        db.run(r#"insert t { id: "c", v: 3 }"#).unwrap();
    }
    // An open database whose files become read-only underneath it.
    {
        let mut db = Db::open(&dir).unwrap();
        assert_eq!(count(&mut db), 3.0);
        set_mode(&dir, 0o444, 0o555);
        let w = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| db.run(r#"insert t { id: "d", v: 4 }"#)));
        assert!(w.is_ok(), "write on a read-only directory panicked");
        // Whether the already-open log handle still accepts the append is up to the OS;
        // what must hold is that a refused write leaves no trace.
        if w.unwrap().is_err() {
            assert_eq!(count(&mut db), 3.0, "refused write is visible");
        }
        let c = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| db.checkpoint()));
        assert!(c.is_ok(), "checkpoint on a read-only directory panicked");
        let c = c.unwrap();
        assert!(c.is_err(), "checkpoint into a read-only directory reported success");
        assert!(count(&mut db) >= 3.0);
        set_mode(&dir, 0o644, 0o755);
    }
    // Opening a read-only directory: an error or a working database, never a panic.
    set_mode(&dir, 0o444, 0o555);
    let opened = std::panic::catch_unwind(|| Db::open(&dir).map(|_| ()));
    set_mode(&dir, 0o644, 0o755);
    assert!(opened.is_ok(), "open on a read-only directory panicked");
    let mut db = Db::open(&dir).unwrap();
    let n = count(&mut db);
    assert!(n == 3.0 || n == 4.0, "rows lost or invented: {n}");
    db.run(r#"insert t { id: "e", v: 5 }"#).unwrap();
}

unsafe extern "C" {
    #[link_name = "geteuid"]
    fn libc_geteuid() -> u32;
}

/// Cold-tier segment files that cannot be read: opening or querying reports an error (or
/// answers correctly if the file was already mapped), never panics and never answers as
/// if the collection were empty.
#[test]
fn unreadable_cold_segments_are_errors_not_empty_results() {
    if unsafe { libc_geteuid() } == 0 {
        return;
    }
    let dir = tmp();
    let opts = || lin::OpenOpts { sync: lin::SyncMode::Normal, cold: true };
    {
        let mut db = Db::open_with(&dir, opts()).unwrap();
        db.run("col t { id: text, v: i64 }").unwrap();
        let rows = (0..200).map(|i| format!(r#"{{ id: "r{i}", v: {i} }}"#)).collect::<Vec<_>>().join(",");
        db.run(&format!("insert t [{rows}]")).unwrap();
        db.checkpoint().unwrap();
        db.close().unwrap();
    }
    let mut files = Vec::new();
    fn walk(d: &Path, out: &mut Vec<std::path::PathBuf>) {
        for e in std::fs::read_dir(d).unwrap() {
            let e = e.unwrap();
            if e.file_type().unwrap().is_dir() {
                walk(&e.path(), out);
            } else {
                out.push(e.path());
            }
        }
    }
    walk(&dir, &mut files);
    let segs: Vec<_> = files.iter().filter(|p| p.components().any(|c| c.as_os_str() == "disk" || c.as_os_str() == "cold" || c.as_os_str() == "segments")).cloned().collect();
    assert!(!segs.is_empty(), "no cold files found in {files:?}");
    for seg in &segs {
        std::fs::set_permissions(seg, std::fs::Permissions::from_mode(0o000)).unwrap();
    }
    let res = std::panic::catch_unwind(|| match Db::open_with(&dir, opts()) {
        Err(_) => None,
        Ok(mut db) => Some(db.run("t | count").map(|h| h.rows[0].get("hits").and_then(|c| c.as_f64()))),
    });
    for seg in &segs {
        std::fs::set_permissions(seg, std::fs::Permissions::from_mode(0o644)).unwrap();
    }
    match res {
        Err(_) => panic!("unreadable cold files caused a panic"),
        Ok(None) | Ok(Some(Err(_))) => {}
        Ok(Some(Ok(n))) => assert_eq!(n, Some(200.0), "unreadable cold files answered as {n:?}"),
    }
    let mut db = Db::open_with(&dir, opts()).unwrap();
    assert_eq!(count(&mut db), 200.0);
}
