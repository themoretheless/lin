//! `#[derive(LinRow)]` conversions: a renamed field reads its stored name, an `Option`
//! field is `None` when absent, a missing required field and a value out of range for a
//! narrower integer are errors, extra stored fields are ignored, and time, vector and
//! small-text cells convert to their Rust types.

#![cfg(feature = "derive")]

use lin::{Cell, Db, FromRow, InlineStr, LinRow};

#[derive(Debug, LinRow)]
struct Full {
    id: String,
    #[lin(rename = "n")]
    count: i64,
    small: u8,
    maybe: Option<i64>,
    stamp: i64,
    embedding: Vec<f32>,
    tag: InlineStr,
}

#[derive(Debug, LinRow)]
struct OnlyId {
    id: String,
}

fn rows(db: &mut Db, q: &str) -> Vec<lin::Row> {
    db.run(q).unwrap().rows.clone()
}

#[test]
fn derive_conversions() {
    let mut db = Db::empty();
    db.run("col t { id: text, n: i64, small: i64, maybe: i64, stamp: time, tag: text, extra: text }").unwrap();
    let mut row = lin::Row::new();
    row.insert("id", Cell::text_arc("a"));
    row.insert("n", Cell::Int(7));
    row.insert("small", Cell::Int(200));
    row.insert("stamp", Cell::Time(1_700_000_000_000));
    row.insert("embedding", Cell::Vec(std::sync::Arc::from([0.5f32, -0.25].as_slice())));
    row.insert("tag", Cell::text_arc("short"));
    row.insert("extra", Cell::text_arc("ignored"));
    let f = Full::from_row(&row).unwrap();
    assert_eq!((f.id.as_str(), f.count, f.small, f.maybe, f.stamp), ("a", 7, 200, None, 1_700_000_000_000));
    assert_eq!(f.embedding, vec![0.5, -0.25]);
    assert_eq!(f.tag.as_str(), "short");

    row.insert("maybe", Cell::Int(5));
    assert_eq!(Full::from_row(&row).unwrap().maybe, Some(5));
    row.insert("maybe", Cell::Null);
    assert_eq!(Full::from_row(&row).unwrap().maybe, None);

    let mut out_of_range = row.clone();
    out_of_range.insert("small", Cell::Int(300));
    assert!(Full::from_row(&out_of_range).is_err(), "300 accepted as u8");
    let mut negative = row.clone();
    negative.insert("small", Cell::Int(-1));
    assert!(Full::from_row(&negative).is_err(), "-1 accepted as u8");
    let mut missing = row.clone();
    missing.remove("n");
    assert!(Full::from_row(&missing).is_err(), "missing required renamed field accepted");
    let mut wrong_type = row.clone();
    wrong_type.insert("n", Cell::text_arc("seven"));
    assert!(Full::from_row(&wrong_type).is_err(), "text accepted as i64");

    // Through a real query: extra fields are ignored.
    db.run(r#"insert t { id: "q", n: 1, small: 2, stamp: now, tag: "x", extra: "y" }"#).unwrap();
    let got: Vec<OnlyId> = rows(&mut db, "t | take all").iter().map(|r| OnlyId::from_row(r).unwrap()).collect();
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].id, "q");
}
