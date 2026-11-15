//! Identifiers: fields named like keywords (`count`, `sort`, `take`, `hop`, `by`) can be
//! declared, inserted, projected, sorted and aggregated, and filtered through `where`
//! (a bare `count == 1` is the `count` stage, so `where` is the way to name such a field
//! in a filter). Non-ASCII and quoted identifiers are not part of the grammar and are
//! rejected at compile time rather than misread.

use lin::Db;

fn ids(db: &mut Db, q: &str) -> Vec<String> {
    let mut v: Vec<String> = db.run(q).unwrap_or_else(|e| panic!("{q}: {e}")).rows.iter().map(|r| r.get("id").and_then(|c| c.text()).unwrap().to_string()).collect();
    v.sort();
    v
}

#[test]
fn keyword_named_fields_and_identifier_policy() {
    let mut db = Db::empty();
    db.run("col t { id: text, count: i64, sort: text, take: i64, hop: i64, by: i64 }").unwrap();
    db.run(r#"insert t [{ id: "a", count: 1, sort: "x", take: 2, hop: 5, by: 1 }, { id: "b", count: 2, sort: "y", take: 3, hop: 6, by: 1 }]"#).unwrap();
    assert_eq!(ids(&mut db, "t | where count == 1 | { id } | take all"), ["a"]);
    assert_eq!(ids(&mut db, r#"t | where sort == "y" and take > 2 | { id } | take all"#), ["b"]);
    assert_eq!(ids(&mut db, "t | where hop == 5 | { id } | take all"), ["a"]);
    let proj = db.run("t | { id, count, sort } | take all").unwrap().rows.clone();
    assert!(proj.iter().all(|r| r.get("count").is_some() && r.get("sort").is_some()));
    let sorted: Vec<String> = db.run("t | sort sort desc | { id } | take all").unwrap().rows.iter().map(|r| r.get("id").and_then(|c| c.text()).unwrap().to_string()).collect();
    assert_eq!(sorted, ["b", "a"]);
    assert_eq!(db.run("t | sum take").unwrap().rows[0].get("take").and_then(|c| c.as_f64()), Some(5.0));
    assert_eq!(db.run("t | count by by").unwrap().rows.len(), 1);
    for bad in [r#"col "weird name" { id: text }"#, "col t2 { id: text, `x y`: i64 }", "col t3 { id: text, имя: text }", "t | count == 1 | take all"] {
        assert!(db.run(bad).is_err(), "accepted: {bad}");
    }
}
