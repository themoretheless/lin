//! Result types of window functions (row_number / rank → integers, sum over integers →
//! integer, avg → float, sum over floats → float), and filters around graph traversal:
//! a filter after `hop` keeps exactly the reached nodes that pass it, and a filter before
//! `hop` only changes which nodes the traversal starts from.

use std::collections::BTreeSet;

use lin::{Cell, Db};

#[test]
fn window_result_types() {
    let mut db = Db::empty();
    db.run("col w { id: text, g: i64, k: i64, v: i64, f: f64 }").unwrap();
    let rows = (0..12).map(|i| format!(r#"{{ id: "r{i}", g: {}, k: {i}, v: {i}, f: {i}.5 }}"#, i % 2)).collect::<Vec<_>>().join(",");
    db.run(&format!("insert w [{rows}]")).unwrap();
    let kinds = |db: &mut Db, f: &str| -> BTreeSet<&'static str> {
        db.run(&format!("w | window {f} by g sort k asc as x | pick {{ x }} | take all")).unwrap().rows.iter().map(|r| match r.get("x") {
            Some(Cell::Int(_)) => "int",
            Some(Cell::Float(_)) => "float",
            Some(Cell::Null) | None => "null",
            _ => "other",
        }).collect()
    };
    assert_eq!(kinds(&mut db, "row_number"), BTreeSet::from(["int"]));
    assert_eq!(kinds(&mut db, "rank"), BTreeSet::from(["int"]));
    assert_eq!(kinds(&mut db, "dense_rank"), BTreeSet::from(["int"]));
    assert_eq!(kinds(&mut db, "sum v"), BTreeSet::from(["int"]));
    assert_eq!(kinds(&mut db, "sum f"), BTreeSet::from(["float"]));
    assert_eq!(kinds(&mut db, "avg v"), BTreeSet::from(["float"]));
}

#[test]
fn filters_around_hop() {
    let mut db = Db::empty();
    let docs = (0..8).map(|i| format!(r#"{{ uri: "raw://hf/{i}", title: "T{i}", layer: "{}" }}"#, if i % 2 == 0 { "wiki" } else { "raw" })).collect::<Vec<_>>().join(",");
    db.run(&format!("insert docs [{docs}]")).unwrap();
    let ids: Vec<String> = (0..8).map(|i| db.run(&format!(r#"docs | uri == "raw://hf/{i}" | {{ id }}"#)).unwrap().rows[0].get("id").and_then(|c| c.text()).unwrap().to_string()).collect();
    for (a, b) in [(0, 1), (0, 2), (1, 3), (2, 4), (3, 5), (4, 6), (6, 7)] {
        db.run(&format!(r#"append edge wikilink "{}" -> "{}""#, ids[a], ids[b])).unwrap();
    }
    let titles = |db: &mut Db, q: &str| -> BTreeSet<String> { db.run(q).unwrap().rows.iter().map(|r| r.get("title").and_then(|c| c.text()).unwrap().to_string()).collect() };
    let reached = titles(&mut db, &format!(r#"docs | id == "{}" | hop wikilink depth=3 | {{ title, layer }} | take all"#, ids[0]));
    let reached_wiki = titles(&mut db, &format!(r#"docs | id == "{}" | hop wikilink depth=3 | layer == "wiki" | {{ title }} | take all"#, ids[0]));
    let wiki: BTreeSet<String> = (0..8).filter(|i| i % 2 == 0).map(|i| format!("T{i}")).collect();
    assert_eq!(reached_wiki, reached.intersection(&wiki).cloned().collect::<BTreeSet<_>>(), "filter after hop");
    // Filter before hop: start from every wiki doc whose title is T0 or T4.
    let from_two = titles(&mut db, r#"docs | layer == "wiki" and (title == "T0" or title == "T4") | hop wikilink | { title } | take all"#);
    assert_eq!(from_two, BTreeSet::from(["T1".to_string(), "T2".to_string(), "T6".to_string()]));
}
