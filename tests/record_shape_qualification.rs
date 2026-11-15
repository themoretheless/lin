//! Record shapes at the edges: a record naming the same field twice is handled the same
//! way by single and bulk inserts (both refuse it, or both keep the same value), a field
//! that no row has comes back absent rather than as a phantom value, and queries over an
//! empty collection return empty results with every stage.

use lin::Db;

fn fresh() -> Db {
    let mut db = Db::empty();
    db.run("col t { id: text, v: i64, w: i64 }").unwrap();
    db
}

#[test]
fn repeated_field_names_behave_the_same_in_single_and_bulk() {
    let rec = r#"{ id: "a", v: 1, v: 2 }"#;
    let mut single = fresh();
    let mut bulk = fresh();
    let s = single.run(&format!("insert t {rec}")).map(|_| ()).map_err(|e| e.to_string());
    let b = bulk.run(&format!(r#"insert t [{rec}, {{ id: "b", v: 3 }}]"#)).map(|_| ()).map_err(|e| e.to_string());
    assert_eq!(s.is_ok(), b.is_ok(), "single {s:?} vs bulk {b:?}");
    if s.is_ok() {
        let get = |db: &mut Db| db.run(r#"t | id == "a" | { v }"#).unwrap().rows[0].get("v").and_then(|c| c.as_f64());
        assert_eq!(get(&mut single), get(&mut bulk));
    } else {
        assert_eq!(bulk.run("t | count").unwrap().rows[0].get("hits").and_then(|c| c.as_f64()), Some(0.0), "bulk partly applied");
    }
}

#[test]
fn never_set_fields_and_empty_collections() {
    let mut db = fresh();
    db.run(r#"insert t [{ id: "a", v: 1 }, { id: "b", v: 2 }]"#).unwrap();
    let rows = db.run("t | { id, w } | sort id | take all").unwrap().rows.clone();
    assert_eq!(rows.len(), 2);
    for r in &rows {
        assert!(r.get("w").is_none_or(|c| matches!(c, lin::Cell::Null)), "phantom value for w: {r:?}");
    }
    assert_eq!(db.run("t | w is null | count").unwrap().rows[0].get("hits").and_then(|c| c.as_f64()), Some(2.0));
    assert_eq!(db.run("t | w > 0 | count").unwrap().rows[0].get("hits").and_then(|c| c.as_f64()), Some(0.0));

    let mut empty = fresh();
    for q in ["t | take all", "t | sort v | take 5", "t | v > 1 | { id } | take all", "t | skip 3 | take all"] {
        assert!(empty.run(q).unwrap().rows.is_empty(), "{q}");
    }
    assert_eq!(empty.run("t | count").unwrap().rows[0].get("hits").and_then(|c| c.as_f64()), Some(0.0));
    assert!(empty.run("t | count by v").unwrap().rows.is_empty());
    let sum = empty.run("t | sum v").unwrap().rows.clone();
    assert!(sum.is_empty() || sum[0].get("v").and_then(|c| c.as_f64()) == Some(0.0), "{sum:?}");
}
