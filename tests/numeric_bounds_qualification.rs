//! Numbers at the edge of i64: the extreme literals store and compare exactly, a
//! literal past the range is a compile error rather than a wrapped value, sums that
//! leave i64 come back as floats instead of wrapping, and the numeric family stays
//! interchangeable (an f64 value in an i64 field is kept, not truncated).

use lin::{Cell, Db};

fn one(db: &mut Db, q: &str, field: &str) -> Cell {
    db.run(q).unwrap().rows[0].get(field).cloned().unwrap()
}

#[test]
fn i64_bounds_store_compare_and_aggregate_without_wrapping() {
    let mut db = Db::empty();
    db.run("col t { id: text, v: i64 }").unwrap();
    db.run(r#"insert t [{ id: "max", v: 9223372036854775807 }, { id: "min", v: -9223372036854775808 }]"#).unwrap();
    assert!(db.run(r#"insert t { id: "over", v: 9223372036854775808 }"#).is_err());
    assert!(db.run(r#"insert t { id: "under", v: -9223372036854775809 }"#).is_err());
    assert_eq!(one(&mut db, r#"t | id == "max" | { v }"#, "v"), Cell::Int(i64::MAX));
    assert_eq!(one(&mut db, r#"t | id == "min" | { v }"#, "v"), Cell::Int(i64::MIN));
    assert_eq!(one(&mut db, "t | v == 9223372036854775807 | count", "hits"), Cell::Int(1));
    assert_eq!(one(&mut db, "t | v > 9223372036854775806 | count", "hits"), Cell::Int(1));
    assert_eq!(one(&mut db, "t | v < -9223372036854775807 | count", "hits"), Cell::Int(1));
    // MAX + MIN fits: exact.
    assert_eq!(one(&mut db, "t | sum v", "v").as_f64(), Some(-1.0));
    // MAX + MAX does not fit: a float near 1.8e19, never a wrapped negative number.
    db.run(r#"insert t { id: "max2", v: 9223372036854775807 }"#).unwrap();
    let s = one(&mut db, r#"t | id != "min" | sum v"#, "v").as_f64().unwrap();
    assert!((s - 2.0 * i64::MAX as f64).abs() < 1e6, "sum wrapped: {s}");
    let a = one(&mut db, r#"t | id != "min" | avg v"#, "v").as_f64().unwrap();
    assert!((a - i64::MAX as f64).abs() < 1e6, "avg wrapped: {a}");
}

#[test]
fn numeric_family_is_interchangeable_without_truncation() {
    let mut db = Db::empty();
    db.run("col t { id: text, v: i64, f: f64 }").unwrap();
    db.run(r#"insert t { id: "a", v: 1.5, f: 2 }"#).unwrap();
    assert_eq!(one(&mut db, "t | { v }", "v").as_f64(), Some(1.5));
    assert_eq!(one(&mut db, "t | { f }", "f").as_f64(), Some(2.0));
    assert_eq!(one(&mut db, "t | v > 1 | count", "hits"), Cell::Int(1));
    assert!(db.run(r#"insert t { id: "b", v: "x", f: 1 }"#).is_err(), "text accepted as a number");
}

/// The cold tier sums its fixed-width columns without materialising rows; it must give
/// the same exact totals, across segments and the overlay of later writes.
#[test]
fn cold_tier_sums_are_exact_too() {
    let n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let dir = std::env::temp_dir().join(format!("lin-numeric-cold-{}-{n}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let opts = || lin::OpenOpts { sync: lin::SyncMode::Normal, cold: true };
    {
        let mut db = Db::open_with(&dir, opts()).unwrap();
        db.run("col big { id: text, v: i64, g: text }").unwrap();
        db.run(r#"insert big [{ id: "a", v: 9223372036854775807, g: "x" }, { id: "b", v: -9223372036854775808, g: "x" }]"#).unwrap();
        // Zero-valued filler so the table is large enough to go to the cold tier.
        let filler = (0..64).map(|i| format!(r#"{{ id: "z{i}", v: 0, g: "z" }}"#)).collect::<Vec<_>>().join(",");
        db.run(&format!("insert big [{filler}]")).unwrap();
        db.checkpoint().unwrap();
        db.close().unwrap();
    }
    let mut db = Db::open_with(&dir, opts()).unwrap();
    assert!(db.store.is_disk_backed("big"));
    assert_eq!(one(&mut db, "big | sum v", "v").as_f64(), Some(-1.0));
    // A later write lands in the overlay; the total combines both exactly.
    db.run(r#"insert big { id: "c", v: 9223372036854775807, g: "y" }"#).unwrap();
    assert_eq!(one(&mut db, "big | sum v", "v").as_f64(), Some(i64::MAX as f64 - 1.0));
    assert_eq!(one(&mut db, r#"big | g == "x" | sum v"#, "v").as_f64(), Some(-1.0));
    // Grouped, straight from the segment columns plus the overlay row.
    let rows = db.run("big | sum v by g").unwrap().rows.clone();
    let by = |g: &str| rows.iter().find(|r| r.get("g").and_then(|c| c.text()) == Some(g)).and_then(|r| r.get("v")).and_then(|c| c.as_f64());
    assert_eq!(by("x"), Some(-1.0));
    assert_eq!(by("y"), Some(i64::MAX as f64));
    assert_eq!(by("z"), Some(0.0));
    // The same groups after a checkpoint folds the overlay into a segment.
    db.checkpoint().unwrap();
    let rows = db.run("big | sum v by g").unwrap().rows.clone();
    let by = |g: &str| rows.iter().find(|r| r.get("g").and_then(|c| c.text()) == Some(g)).and_then(|r| r.get("v")).and_then(|c| c.as_f64());
    assert_eq!(by("x"), Some(-1.0));
}

/// The cold tier's columnar float sum keeps a small value between two large values that
/// cancel, like the in-memory sum.
#[test]
fn cold_float_sum_is_compensated() {
    let n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let dir = std::env::temp_dir().join(format!("lin-cold-fsum-{}-{n}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let opts = || lin::OpenOpts { sync: lin::SyncMode::Normal, cold: true };
    {
        let mut db = Db::open_with(&dir, opts()).unwrap();
        db.run("col cf { id: text, f: f64 }").unwrap();
        db.run(r#"insert cf [{ id: "a", f: 10000000000000000.0 }, { id: "b", f: 1.0 }, { id: "c", f: -10000000000000000.0 }]"#).unwrap();
        let filler = (0..64).map(|i| format!(r#"{{ id: "z{i}", f: 0.0 }}"#)).collect::<Vec<_>>().join(",");
        db.run(&format!("insert cf [{filler}]")).unwrap();
        db.checkpoint().unwrap();
        db.close().unwrap();
    }
    let mut db = Db::open_with(&dir, opts()).unwrap();
    assert!(db.store.is_disk_backed("cf"));
    assert_eq!(one(&mut db, "cf | sum f", "f").as_f64(), Some(1.0));
}
