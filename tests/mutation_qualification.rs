//! Mutations that touch identity: updates that change a row's id or a document's uri,
//! deletes that hit the same row several times or swap rows into removed slots, and bulk
//! inserts with explicit nulls. Every case is checked live, after reopen, and through
//! the id and uri lookups so indexes and rows cannot drift apart.

use std::path::PathBuf;

use lin::Db;

fn tmp() -> PathBuf {
    // Tests run in parallel and can read the same clock value: the counter keeps their
    // directories (and the writer lock in each) apart.
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let d = std::env::temp_dir().join(format!("lin-mut-{}-{}-{n}", std::process::id(), rand_suffix()));
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn rand_suffix() -> u128 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
}

fn ids(db: &mut Db, q: &str) -> Vec<String> {
    let mut v: Vec<String> = db.run(q).unwrap().rows.iter().map(|r| r.get("id").and_then(|c| c.text()).unwrap().to_string()).collect();
    v.sort();
    v
}

fn check_ids(db: &mut Db, expect: &[&str]) {
    let all = ids(db, "t | { id } | take all");
    assert_eq!(all, expect.iter().map(|s| s.to_string()).collect::<Vec<_>>());
    for id in expect {
        assert_eq!(ids(db, &format!(r#"t | id == "{id}" | {{ id }}"#)), vec![id.to_string()], "lookup of {id}");
    }
}

#[test]
fn update_changing_id_keeps_lookups_consistent() {
    let dir = tmp();
    let mut db = Db::open(&dir).unwrap();
    db.run("col t { id: text, v: i64 }").unwrap();
    db.run(r#"insert t [{ id: "a", v: 1 }, { id: "b", v: 2 }, { id: "c", v: 3 }]"#).unwrap();
    match db.run(r#"update t[id == "b"] { id: "z" }"#) {
        Ok(_) => {
            check_ids(&mut db, &["a", "c", "z"]);
            assert!(ids(&mut db, r#"t | id == "b" | { id }"#).is_empty(), "old id still found");
            // Renaming onto an existing id must fail and change nothing.
            assert!(db.run(r#"update t[id == "a"] { id: "c" }"#).is_err());
            check_ids(&mut db, &["a", "c", "z"]);
            drop(db);
            let mut db = Db::open(&dir).unwrap();
            check_ids(&mut db, &["a", "c", "z"]);
        }
        Err(_) => check_ids(&mut db, &["a", "b", "c"]),
    }
}

#[test]
fn update_changing_uri_moves_the_uri_lookup() {
    let dir = tmp();
    let mut db = Db::open(&dir).unwrap();
    db.run(r#"insert docs [{ uri: "raw://m/a", title: "A", layer: "raw" }, { uri: "raw://m/b", title: "B", layer: "raw" }]"#).unwrap();
    let n = |db: &mut Db, u: &str| db.run(&format!(r#"docs | uri == "{u}" | count"#)).unwrap().rows[0].get("hits").and_then(|c| c.as_f64()).unwrap() as i64;
    match db.run(r#"update docs[uri == "raw://m/a"] { uri: "raw://m/c" }"#) {
        Ok(_) => {
            assert_eq!((n(&mut db, "raw://m/a"), n(&mut db, "raw://m/c")), (0, 1));
            assert!(db.run(r#"update docs[uri == "raw://m/c"] { uri: "raw://m/b" }"#).is_err(), "duplicate uri accepted");
            drop(db);
            let mut db = Db::open(&dir).unwrap();
            assert_eq!((n(&mut db, "raw://m/a"), n(&mut db, "raw://m/b"), n(&mut db, "raw://m/c")), (0, 1, 1));
        }
        Err(_) => assert_eq!((n(&mut db, "raw://m/a"), n(&mut db, "raw://m/c")), (1, 0)),
    }
}

#[test]
fn deletes_with_overlapping_and_swapped_positions() {
    let dir = tmp();
    let mut db = Db::open(&dir).unwrap();
    db.run("col t { id: text, v: i64 }").unwrap();
    let rows = (0..20).map(|i| format!(r#"{{ id: "r{i:02}", v: {i} }}"#)).collect::<Vec<_>>().join(",");
    db.run(&format!("insert t [{rows}]")).unwrap();
    // The predicate matches some rows twice over; each is removed once.
    db.run(r#"delete t[v < 3 or id == "r01" or id == "r19" or v == 19] cas each"#).unwrap();
    // Delete from the middle and the end so the last rows move into freed slots.
    db.run(r#"delete t[v == 10 or v == 18 or v == 17] cas each"#).unwrap();
    let mut expect: Vec<String> = (3..20).filter(|i| ![10, 17, 18, 19].contains(i)).map(|i| format!("r{i:02}")).collect();
    expect.sort();
    let e: Vec<&str> = expect.iter().map(|s| s.as_str()).collect();
    check_ids(&mut db, &e);
    db.run(r#"insert t { id: "r10", v: 100 }"#).unwrap();
    let mut e2 = e.clone();
    e2.push("r10");
    e2.sort();
    check_ids(&mut db, &e2);
    drop(db);
    let mut db = Db::open(&dir).unwrap();
    check_ids(&mut db, &e2);
    assert_eq!(db.run(r#"t | id == "r10" | { v }"#).unwrap().rows[0].get("v").and_then(|c| c.as_f64()), Some(100.0));
}

#[test]
fn explicit_null_in_bulk_matches_single_inserts() {
    let mut bulk = Db::empty();
    let mut single = Db::empty();
    let recs = [r#"{ id: "n1", v: null, s: "x" }"#, r#"{ id: "n2", v: 2, s: null }"#, r#"{ id: "n3", s: "y" }"#];
    for db in [&mut bulk, &mut single] {
        db.run("col t { id: text, v: i64, s: text }").unwrap();
    }
    // Explicit null is either accepted or rejected the same way in both forms.
    let b = bulk.run(&format!("insert t [{}]", recs.join(","))).map(|_| ()).map_err(|e| e.to_string());
    let mut s: Result<(), String> = Ok(());
    {
        let mut tx = single.transaction().unwrap();
        for r in recs {
            if let Err(e) = tx.run(&format!("insert t {r}")) {
                s = Err(e.to_string());
                break;
            }
        }
        if s.is_ok() { tx.commit().unwrap() } else { tx.rollback().unwrap() }
    }
    assert_eq!(b.is_ok(), s.is_ok(), "bulk {b:?} vs single {s:?}");
    for q in ["t | v == null | count", "t | s == null | count", "t | v != null | count", "t | { id, v, s } | sort id | take all"] {
        let a = format!("{:?}", bulk.run(q).map(|h| h.rows.clone()));
        let b = format!("{:?}", single.run(q).map(|h| h.rows.clone()));
        assert_eq!(a, b, "{q}");
    }
}
