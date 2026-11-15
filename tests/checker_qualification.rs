//! The compile phase and lexer: unknown fields and collections, mismatched operand types,
//! non-numeric aggregates and malformed nesting are compile errors with a position where
//! the parser knows one; all ASCII whitespace separates tokens; and unique indexes treat
//! `-0.0` and `0.0` as one key, refuse a conflicting redefinition and name the index in
//! the duplicate-key error.

use lin::Db;

fn seed() -> Db {
    let mut db = Db::empty();
    db.run("col t { id: text, k: i64, f: f64, s: text }").unwrap();
    db.run(r#"insert t [{ id: "a", k: 1, f: 0.0, s: "x" }]"#).unwrap();
    db
}

fn err(db: &mut Db, q: &str) -> lin::Error {
    db.run(q).err().unwrap_or_else(|| panic!("accepted: {q}"))
}

#[test]
fn compile_errors_name_the_problem() {
    let mut db = seed();
    for (q, needle) in [
        ("t | nosuch == 1 | count", "unknown field"),
        ("nosuch | count", "unknown collection"),
        ("t | s > 1 | count", "type mismatch"),
        ("t | k == \"x\" | count", "type mismatch"),
        ("t | { id, nosuch } | take all", "unknown field"),
        ("t | sum s", "numeric"),
        ("t | count by nosuch", "unknown field"),
    ] {
        let e = err(&mut db, q);
        assert!(e.to_string().contains(needle), "{q}: {e}");
        assert!(e.to_string().starts_with("compile error"), "{q}: not a compile error: {e}");
    }
    for q in ["t | (k == 1 | count", "t | k == 1) | count", "t | k in [] | count", "t | { id, { k } } | take all", "t | { x: id } | take all"] {
        let e = err(&mut db, q);
        let pos = e.pos.unwrap_or_else(|| panic!("{q}: syntax error without a position"));
        assert!(pos <= q.len(), "{q}: position {pos}");
    }
    // Tabs, newlines and CR LF separate tokens like spaces.
    assert_eq!(db.run("t\t|\nk\r\n== 1 | count").unwrap().rows[0].get("hits").and_then(|c| c.as_f64()), Some(1.0));
    // Nested boolean grouping.
    assert_eq!(db.run(r#"t | id == "a" and (k == 1 or s == "q") | count"#).unwrap().rows[0].get("hits").and_then(|c| c.as_f64()), Some(1.0));
}

#[test]
fn unique_index_policies() {
    let mut db = seed();
    db.run("index t unique [f]").unwrap();
    let e = err(&mut db, r#"insert t { id: "b", k: 2, f: -0.0, s: "y" }"#);
    assert!(e.to_string().contains("t[f]"), "duplicate-key error does not name the index: {e}");
    assert_eq!(db.run("t | count").unwrap().rows[0].get("hits").and_then(|c| c.as_f64()), Some(1.0));
    db.run("index t [k]").unwrap();
    assert!(db.run("index t unique [k]").is_err(), "conflicting redefinition accepted");
    // The plain index on k is still plain: equal k values are fine.
    db.run(r#"insert t { id: "c", k: 1, f: 3.0, s: "z" }"#).unwrap();
    assert_eq!(db.run("t | k == 1 | count").unwrap().rows[0].get("hits").and_then(|c| c.as_f64()), Some(2.0));
}
