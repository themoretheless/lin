//! Log replay when the catalog itself lives in the log: a collection created, filled and
//! indexed after the last checkpoint is rebuilt on open from the log alone, statements
//! that ran before its index existed and after it are both found through the index, and
//! a crash image taken at any statement boundary reopens to exactly the state at that
//! point.

use lin::Db;

fn tmp() -> std::path::PathBuf {
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let d = std::env::temp_dir().join(format!("lin-replay-cat-{}-{n}-{seq}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn crash_image(from: &std::path::Path) -> std::path::PathBuf {
    let to = tmp();
    for e in std::fs::read_dir(from).unwrap() {
        let e = e.unwrap();
        let ft = e.file_type().unwrap();
        if ft.is_file() && e.file_name() != "LOCK" {
            std::fs::copy(e.path(), to.join(e.file_name())).unwrap();
        } else if ft.is_dir() {
            let sub = to.join(e.file_name());
            std::fs::create_dir_all(&sub).unwrap();
            for f in std::fs::read_dir(e.path()).unwrap() {
                let f = f.unwrap();
                std::fs::copy(f.path(), sub.join(f.file_name())).unwrap();
            }
        }
    }
    to
}

fn state(db: &mut Db) -> Vec<String> {
    let mut out = Vec::new();
    for q in ["late | { id, k } | take all", "late | k == 2 | { id } | take all", "late2 | { id } | take all"] {
        let mut rows: Vec<String> = match db.run(q) {
            Ok(h) => h.rows.iter().map(|r| format!("{r:?}")).collect(),
            Err(_) => vec!["<missing>".into()],
        };
        rows.sort();
        out.push(format!("{q}: {}", rows.join(" ")));
    }
    out
}

#[test]
fn catalog_defined_only_in_the_log_replays_at_every_boundary() {
    let dir = tmp();
    let mut db = Db::open(&dir).unwrap();
    db.run("col base { id: text }").unwrap();
    db.run(r#"insert base { id: "b" }"#).unwrap();
    db.checkpoint().unwrap();
    let steps = [
        "col late { id: text, k: i64 }",
        r#"insert late [{ id: "l1", k: 1 }, { id: "l2", k: 2 }]"#,
        "index late [k]",
        r#"insert late { id: "l3", k: 2 }"#,
        r#"update late[id == "l1"] cas each { k: 2 }"#,
        "col late2 { id: text }",
        r#"insert late2 { id: "m1" }"#,
        r#"delete late[id == "l2"] cas each"#,
    ];
    for (i, stmt) in steps.iter().enumerate() {
        db.run(stmt).unwrap();
        let want = state(&mut db);
        let image = crash_image(&dir);
        let mut re = Db::open(&image).unwrap_or_else(|e| panic!("after step {i} ({stmt}): open: {e}"));
        assert_eq!(state(&mut re), want, "after step {i} ({stmt})");
        // The recovered database takes further schema and data statements.
        re.run("col after_crash { id: text }").unwrap();
        re.run(r#"insert after_crash { id: "x" }"#).unwrap();
        drop(re);
        let _ = std::fs::remove_dir_all(&image);
    }
}
