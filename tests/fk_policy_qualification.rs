//! Foreign keys: declaring one over a missing collection or field is a compile error; an
//! insert or update whose reference has no target is rejected and changes nothing; and a
//! cycle of keys (a → b, b → a) can be declared and filled by inserting one side without
//! the reference first and setting it afterwards.

use lin::Db;

fn count(db: &mut Db, c: &str) -> f64 {
    db.run(&format!("{c} | count")).unwrap().rows[0].get("hits").and_then(|c| c.as_f64()).unwrap()
}

#[test]
fn foreign_key_declarations_and_references() {
    let mut db = Db::empty();
    db.run("col a { id: text, b: text }").unwrap();
    db.run("col b { id: text, a: text }").unwrap();
    for bad in ["fk a.b -> nosuch.id", "fk a.nofield -> b.id", "fk a.b -> b.nofield"] {
        assert!(db.run(bad).is_err(), "accepted: {bad}");
    }
    db.run("fk a.b -> b.id").unwrap();
    db.run("fk b.a -> a.id").unwrap();
    assert!(db.run(r#"insert a { id: "a1", b: "b1" }"#).is_err(), "dangling reference accepted on insert");
    assert_eq!(count(&mut db, "a"), 0.0);
    db.run(r#"insert a { id: "a1" }"#).unwrap();
    db.run(r#"insert b { id: "b1", a: "a1" }"#).unwrap();
    assert!(db.run(r#"update a[id == "a1"] cas each { b: "missing" }"#).is_err(), "dangling reference accepted on update");
    db.run(r#"update a[id == "a1"] cas each { b: "b1" }"#).unwrap();
    let joined = db.run("a | join b on b | take all").unwrap().rows.len();
    assert_eq!(joined, 1);
}
