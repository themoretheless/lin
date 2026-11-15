//! NaN and infinities (reachable through prepared-statement parameters): NaN sorts after
//! +inf ascending and first descending, so `max` agrees with the sort; comparisons never
//! select NaN; `sum` / `avg` over a NaN are NaN; `count by` keeps NaN as one group; and
//! the values survive a reopen unchanged.

use lin::{Cell, Db};

fn ids(db: &mut Db, q: &str) -> Vec<String> {
    db.run(q).unwrap().rows.iter().map(|r| r.get("id").and_then(|c| c.text()).unwrap().to_string()).collect()
}

fn one(db: &mut Db, q: &str) -> f64 {
    db.run(q).unwrap().rows[0].get("v").and_then(|c| c.as_f64()).unwrap()
}

fn check(db: &mut Db, step: &str) {
    assert_eq!(ids(db, "t | sort v | { id } | take all"), ["m", "b", "a", "i", "n"], "{step}");
    assert_eq!(ids(db, "t | sort v desc | { id } | take all"), ["n", "i", "a", "b", "m"], "{step}");
    let mut gt = ids(db, "t | v > 0 | { id } | take all");
    gt.sort();
    assert_eq!(gt, ["a", "i"], "{step}");
    let mut lt = ids(db, "t | v < 0 | { id } | take all");
    lt.sort();
    assert_eq!(lt, ["b", "m"], "{step}");
    assert!(one(db, "t | sum v").is_nan(), "{step}");
    assert!(one(db, "t | avg v").is_nan(), "{step}");
    assert!(one(db, "t | max v").is_nan(), "{step}");
    assert_eq!(one(db, "t | min v"), f64::NEG_INFINITY, "{step}");
    assert_eq!(db.run("t | count by v").unwrap().rows.len(), 5, "{step}");
}

#[test]
fn nan_and_infinity_policy() {
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!("lin-nan-{}-{}", std::process::id(), SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut db = Db::open(&dir).unwrap();
    db.run("col t { id: text, v: f64 }").unwrap();
    let p = db.prepare("insert t { id: $id, v: $v }").unwrap();
    for (id, v) in [("a", 1.0), ("n", f64::NAN), ("b", -1.0), ("i", f64::INFINITY), ("m", f64::NEG_INFINITY)] {
        p.execute_with(&mut db, &[("id", Cell::text_arc(id)), ("v", Cell::Float(v))]).unwrap();
    }
    check(&mut db, "live");
    drop(db);
    let mut db = Db::open(&dir).unwrap();
    check(&mut db, "after replay");
    db.checkpoint().unwrap();
    drop(db);
    let mut db = Db::open(&dir).unwrap();
    check(&mut db, "after checkpoint");
}

/// A unique index treats NaN like any other key: a second NaN is a duplicate (NaN keys
/// compare equal to each other in the index, as they do in `count by`).
#[test]
fn nan_in_a_unique_index_is_one_key() {
    let mut db = Db::empty();
    db.run("col u { id: text, v: f64 }").unwrap();
    db.run("index u unique [v]").unwrap();
    let p = db.prepare("insert u { id: $id, v: $v }").unwrap();
    p.execute_with(&mut db, &[("id", Cell::text_arc("x")), ("v", Cell::Float(f64::NAN))]).unwrap();
    let second = p.execute_with(&mut db, &[("id", Cell::text_arc("y")), ("v", Cell::Float(f64::NAN))]);
    assert!(second.is_err(), "a second NaN key was accepted by a unique index");
    assert_eq!(db.run("u | count").unwrap().rows[0].get("hits").and_then(|c| c.as_f64()), Some(1.0));
}
