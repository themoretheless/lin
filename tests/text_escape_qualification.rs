//! Text survives every path byte for byte: escapes in query literals, raw control and
//! non-ASCII characters, the printed (`{:?}`) form parsed back, prepared parameters, the
//! log replayed on reopen, and regex patterns that use backslash classes.

use lin::{Cell, Db};

const SAMPLES: &[&str] = &[
    "plain",
    "quote \" inside",
    "back\\slash",
    "line\nbreak\tand tab",
    "carriage\rreturn",
    "nul\0byte",
    "escape \u{1b}[0m",
    "кириллица и 漢字 и 🦀",
    "trailing backslash \\",
    "",
];

/// Literal form a user would write: the value printed with Rust `{:?}` escaping.
fn literal(s: &str) -> String {
    format!("{s:?}")
}

fn tmp() -> std::path::PathBuf {
    let n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let d = std::env::temp_dir().join(format!("lin-text-esc-{}-{n}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn read_back(db: &mut Db, id: &str) -> String {
    db.run(&format!(r#"t | id == "{id}" | {{ s }}"#)).unwrap().rows[0].get("s").and_then(|c| c.text()).unwrap().to_string()
}

#[test]
fn text_roundtrips_through_literals_params_and_replay() {
    let dir = tmp();
    let mut db = Db::open(&dir).unwrap();
    db.run("col t { id: text, s: text }").unwrap();
    for (i, s) in SAMPLES.iter().enumerate() {
        db.run(&format!(r#"insert t {{ id: "lit{i}", s: {} }}"#, literal(s))).unwrap();
        let p = db.prepare(&format!(r#"insert t {{ id: "par{i}", s: $v }}"#)).unwrap();
        p.execute_with(&mut db, &[("v", Cell::text_arc(*s))]).unwrap();
    }
    for (i, s) in SAMPLES.iter().enumerate() {
        assert_eq!(read_back(&mut db, &format!("lit{i}")), *s, "literal {i}");
        assert_eq!(read_back(&mut db, &format!("par{i}")), *s, "param {i}");
        // Equality against the literal finds exactly the two rows.
        let n = db.run(&format!("t | s == {} | count", literal(s))).unwrap().rows[0].get("hits").and_then(|c| c.as_f64()).unwrap();
        assert_eq!(n, 2.0, "lookup {i}");
    }
    drop(db);
    let mut db = Db::open(&dir).unwrap();
    for (i, s) in SAMPLES.iter().enumerate() {
        assert_eq!(read_back(&mut db, &format!("lit{i}")), *s, "after reopen {i}");
    }
}

#[test]
fn unknown_escapes_keep_their_backslash_for_regex() {
    let mut db = Db::empty();
    db.run("col t { id: text, s: text }").unwrap();
    db.run(r#"insert t [{ id: "a", s: "order 42" }, { id: "b", s: "no digits" }, { id: "c", s: "dd" }]"#).unwrap();
    let ids = |db: &mut Db, q: &str| -> Vec<String> {
        let mut v: Vec<String> = db.run(q).unwrap().rows.iter().map(|r| r.get("id").and_then(|c| c.text()).unwrap().to_string()).collect();
        v.sort();
        v
    };
    assert_eq!(ids(&mut db, r#"t | s regex "\d+" | { id }"#), vec!["a"]);
    assert_eq!(ids(&mut db, r#"t | s regex "^\w+ \w+$" | { id }"#), vec!["a", "b"]);
    // A bad \u escape is an error, not a silent substitution.
    assert!(db.run(r#"t | s == "\u{zz}" | count"#).is_err());
    assert!(db.run(r#"t | s == "\u{110000}" | count"#).is_err());
}
