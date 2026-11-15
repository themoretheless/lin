//! Prepared queries across catalog changes: a query prepared before an index exists, before
//! other collections are added, and before the data changes keeps returning the same
//! answers as the same text run fresh; a parameter bound with a different numeric type
//! compares by value; a prepared statement on a different database is rejected or exact.

use lin::{Cell, Db};

fn ids(h: &lin::Handle) -> Vec<String> {
    let mut v: Vec<String> = h.rows.iter().map(|r| r.get("id").and_then(|c| c.text()).unwrap().to_string()).collect();
    v.sort();
    v
}

#[test]
fn prepared_queries_follow_catalog_and_data_changes() {
    let mut db = Db::empty();
    db.run("col t { id: text, k: i64, s: text }").unwrap();
    let rows = (0..100).map(|i| format!(r#"{{ id: "r{i}", k: {}, s: "s{}" }}"#, i % 10, i % 3)).collect::<Vec<_>>().join(",");
    db.run(&format!("insert t [{rows}]")).unwrap();
    let texts = [
        "t | k == $k | { id } | take all",
        "t | k > $k and s == \"s1\" | { id } | take all",
        "t | k == $k | count",
    ];
    let prepared: Vec<_> = texts.iter().map(|q| db.prepare(q).unwrap()).collect();
    let check = |db: &mut Db, step: &str| {
        for (q, p) in texts.iter().zip(&prepared) {
            for k in [0i64, 3, 9, 42] {
                let params = [("k", Cell::Int(k))];
                let got = p.run_with(db, &params).unwrap_or_else(|e| panic!("{step}: {q}: {e}"));
                let fresh = db.prepare(q).unwrap().run_with(db, &params).unwrap();
                if q.ends_with("count") {
                    assert_eq!(format!("{:?}", got.rows), format!("{:?}", fresh.rows), "{step}: {q} k={k}");
                } else {
                    assert_eq!(ids(&got), ids(&fresh), "{step}: {q} k={k}");
                }
            }
        }
    };
    check(&mut db, "initial");
    db.run("index t [k]").unwrap();
    check(&mut db, "after index");
    db.run("col other { id: text, k: i64 }\nindex other [k]").unwrap();
    db.run(r#"insert other { id: "o1", k: 3 }"#).unwrap();
    check(&mut db, "after another collection");
    db.run(r#"insert t [{ id: "n1", k: 3, s: "s1" }, { id: "n2", k: 42, s: "s1" }]"#).unwrap();
    db.run(r#"update t[id == "r3"] cas each { k: 9 }"#).unwrap();
    db.run(r#"delete t[id == "r13"] cas each"#).unwrap();
    check(&mut db, "after data changes");

    // A float parameter compares by value against integer fields.
    let p = &prepared[0];
    let as_int = ids(&p.run_with(&mut db, &[("k", Cell::Int(3))]).unwrap());
    let as_float = ids(&p.run_with(&mut db, &[("k", Cell::Float(3.0))]).unwrap());
    assert_eq!(as_int, as_float);
    assert!(ids(&p.run_with(&mut db, &[("k", Cell::Float(3.5))]).unwrap()).is_empty());

    // The same prepared statement on another database: an error or that database's answer.
    let mut other = Db::empty();
    other.run("col t { id: text, k: i64, s: text }").unwrap();
    other.run(r#"insert t { id: "x", k: 3, s: "s0" }"#).unwrap();
    match p.run_with(&mut other, &[("k", Cell::Int(3))]) {
        Ok(h) => assert_eq!(ids(&h), ["x"]),
        Err(_) => {}
    }
}
