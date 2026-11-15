//! Time values stay distinct from integers: mixing them in inserts, comparisons or
//! aggregates is a compile error, and a stored time comes back as a time (not an integer
//! of the same milliseconds) live, after log replay and after a checkpoint.

use lin::{Cell, Db};

fn tmp() -> std::path::PathBuf {
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let d = std::env::temp_dir().join(format!("lin-time-type-{}-{seq}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn stamp_cell(db: &mut Db) -> Cell {
    db.run(r#"s | id == "a" | { stamp }"#).unwrap().rows[0].get("stamp").cloned().unwrap()
}

#[test]
fn time_and_integers_do_not_mix() {
    let dir = tmp();
    let mut db = Db::open(&dir).unwrap();
    db.run("col s { id: text, stamp: time, v: i64 }").unwrap();
    db.run(r#"insert s { id: "a", stamp: now, v: 5 }"#).unwrap();
    for bad in [
        r#"insert s { id: "b", stamp: 5, v: 5 }"#,
        r#"insert s { id: "c", stamp: now, v: now }"#,
        "s | stamp > 5 | count",
        "s | v > now | count",
        "s | sum stamp",
        "s | avg stamp",
    ] {
        assert!(db.run(bad).is_err(), "accepted: {bad}");
    }
    let Cell::Time(t) = stamp_cell(&mut db) else { panic!("stored time is not a time") };
    drop(db);
    let mut db = Db::open(&dir).unwrap();
    assert_eq!(stamp_cell(&mut db), Cell::Time(t), "after replay");
    db.checkpoint().unwrap();
    drop(db);
    let mut db = Db::open(&dir).unwrap();
    assert_eq!(stamp_cell(&mut db), Cell::Time(t), "after checkpoint");
    assert_eq!(db.run("s | stamp <= now | count").unwrap().rows[0].get("hits").and_then(|c| c.as_f64()), Some(1.0));
}
