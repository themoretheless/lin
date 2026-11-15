//! `Cell::clone_for_borrow` gives a value equal to the original for every kind of cell,
//! including inline small text at its size limit and shared long text, and the copy is
//! independent: changing a row built from it never changes the source. Rows taken out of
//! a record batch are owned the same way.

use std::sync::Arc;

use lin::{Cell, Db, Row};

#[test]
fn clone_for_borrow_is_equal_and_independent() {
    let long = "x".repeat(200);
    let cells = vec![
        Cell::Null,
        Cell::Int(i64::MIN),
        Cell::Int(i64::MAX),
        Cell::Float(-0.0),
        Cell::Float(f64::INFINITY),
        Cell::Bool(true),
        Cell::Time(-5),
        Cell::text_arc(""),
        Cell::text_arc("short"),
        Cell::text_arc("exactly-twenty-three-ch"),
        Cell::text_arc("twenty-four-bytes-long!!"),
        Cell::text_arc("ёжик 🦀"),
        Cell::text_arc(long.as_str()),
        Cell::Vec(Arc::from([1.0f32, -2.0, 0.5].as_slice())),
    ];
    for c in &cells {
        let b = c.clone_for_borrow();
        assert_eq!(&b, c, "clone_for_borrow changed {c:?}");
        assert_eq!(c.text(), b.text());
    }
    let nan = Cell::Float(f64::NAN).clone_for_borrow();
    assert!(nan.as_f64().unwrap().is_nan());

    // Independence: a row built from borrowed clones can change without touching the source.
    let mut src = Row::new();
    for (i, c) in cells.iter().enumerate() {
        src.insert(format!("f{i}"), c.clone());
    }
    let mut copy = Row::new();
    for (k, v) in src.iter() {
        copy.insert(k.to_string(), v.clone_for_borrow());
    }
    copy.insert("f12", Cell::text_arc("changed"));
    copy.insert("f0", Cell::Int(1));
    assert_eq!(src.get("f12").and_then(|c| c.text()), Some(long.as_str()));
    assert_eq!(src.get("f0"), Some(&Cell::Null));
}

#[test]
fn rows_from_a_batch_are_owned() {
    let mut db = Db::empty();
    db.run("col t { id: text, s: text }").unwrap();
    let long = "y".repeat(100);
    db.run(&format!(r#"insert t [{{ id: "a", s: "{long}" }}, {{ id: "b", s: "tiny" }}]"#)).unwrap();
    let batch = db.run_batch("t | sort id | take all").unwrap();
    let mut first = batch.to_rows();
    let second = batch.to_rows();
    first[0].insert("s", Cell::text_arc("mutated"));
    assert_eq!(second[0].get("s").and_then(|c| c.text()), Some(long.as_str()));
    assert_eq!(batch.to_rows()[0].get("s").and_then(|c| c.text()), Some(long.as_str()));
    drop(batch);
    assert_eq!(second[1].get("s").and_then(|c| c.text()), Some("tiny"));
}
