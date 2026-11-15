//! Group and sort keys: `count by` treats values as distinct exactly when they differ
//! (text compares case-sensitively, missing values form one null group); sorting
//! by a field the projection dropped is a compile error, and sorting by a kept field puts
//! missing values first ascending and last descending.

use std::collections::BTreeMap;

use lin::Db;

fn seed() -> Db {
    let mut db = Db::empty();
    db.run("col t { id: text, k: i64, f: f64, s: text }").unwrap();
    db.run(r#"insert t [{ id: "a", k: 1, f: 1.0, s: "x" }, { id: "b", k: 1, f: 1.0, s: "X" }, { id: "c", k: 2, s: "x" }, { id: "d", f: 1.0, s: "x" }]"#).unwrap();
    db
}

fn groups(db: &mut Db, q: &str, key: &str) -> BTreeMap<String, f64> {
    db.run(q).unwrap().rows.iter().map(|r| {
        let k = format!("{:?}", r.get(key).cloned().unwrap_or(lin::Cell::Null));
        let n = r.iter().find(|(f, _)| f.as_str() != key).and_then(|(_, v)| v.as_f64()).unwrap();
        (k, n)
    }).collect()
}

#[test]
fn group_and_sort_keys() {
    let mut db = seed();
    let s = groups(&mut db, "t | count by s", "s");
    assert_eq!(s.len(), 2, "{s:?}");
    assert_eq!(s.values().sum::<f64>(), 4.0);
    let k = groups(&mut db, "t | count by k", "k");
    assert_eq!(k.get("Null"), Some(&1.0), "{k:?}");
    assert_eq!(k.len(), 3, "{k:?}");
    let f = groups(&mut db, "t | count by f", "f");
    assert_eq!(f.get("Null"), Some(&1.0), "{f:?}");
    assert_eq!(f.get("Float(1.0)"), Some(&3.0), "{f:?}");

    assert!(db.run("t | { id } | sort k | take all").is_err(), "sorted by a dropped field");
    let ids = |db: &mut Db, q: &str| -> Vec<String> { db.run(q).unwrap().rows.iter().map(|r| r.get("id").and_then(|c| c.text()).unwrap().to_string()).collect() };
    assert_eq!(ids(&mut db, "t | { id, k } | sort k desc | take all"), ["c", "a", "b", "d"]);
    assert_eq!(ids(&mut db, "t | { id, k } | sort k | take all"), ["d", "a", "b", "c"]);
}
