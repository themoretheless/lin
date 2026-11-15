//! Index definitions: an empty or unknown field list is rejected without touching the
//! catalog, and an index built inside a transaction that rolls back leaves no index and
//! no stale entries behind, live or after reopen.

use lin::Db;

fn tmp() -> std::path::PathBuf {
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let d = std::env::temp_dir().join(format!("lin-idx-ddl-{}-{n}-{seq}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn hits(db: &mut Db, q: &str) -> i64 {
    db.run(q).unwrap().rows[0].get("hits").and_then(|c| c.as_f64()).unwrap() as i64
}

#[test]
fn empty_and_unknown_index_definitions_are_rejected() {
    let mut db = Db::empty();
    db.run("col t { id: text, k: i64 }").unwrap();
    db.run(r#"insert t [{ id: "a", k: 1 }, { id: "b", k: 2 }]"#).unwrap();
    for bad in ["index t []", "index t [nope]", "index missing [k]", "index t [k, nope]"] {
        assert!(db.run(bad).is_err(), "{bad} accepted");
    }
    db.run("index t [k]").unwrap();
    assert_eq!(hits(&mut db, "t | k == 2 | count"), 1);
}

#[test]
fn index_built_in_a_rolled_back_transaction_leaves_nothing() {
    let dir = tmp();
    let mut db = Db::open(&dir).unwrap();
    db.run("col t { id: text, k: i64 }").unwrap();
    let rows = (0..200).map(|i| format!(r#"{{ id: "r{i}", k: {} }}"#, i % 10)).collect::<Vec<_>>().join(",");
    db.run(&format!("insert t [{rows}]")).unwrap();
    {
        let mut tx = db.transaction().unwrap();
        tx.run("index t [k]").unwrap();
        tx.run(r#"insert t { id: "ghost", k: 3 }"#).unwrap();
        tx.run(r#"update t[id == "r3"] cas each { k: 99 }"#).unwrap();
        tx.rollback().unwrap();
    }
    assert_eq!(hits(&mut db, "t | k == 3 | count"), 20);
    assert_eq!(hits(&mut db, "t | k == 99 | count"), 0);
    // Building it for real afterwards indexes exactly the committed rows.
    db.run("index t [k]").unwrap();
    db.run(r#"update t[id == "r4"] cas each { k: 3 }"#).unwrap();
    assert_eq!(hits(&mut db, "t | k == 3 | count"), 21);
    assert_eq!(hits(&mut db, "t | k == 4 | count"), 19);
    drop(db);
    let mut db = Db::open(&dir).unwrap();
    assert_eq!(hits(&mut db, "t | k == 3 | count"), 21);
    assert_eq!(hits(&mut db, r#"t | id == "ghost" | count"#), 0);
}
