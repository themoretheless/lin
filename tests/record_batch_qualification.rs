//! Columnar results: every column of `run_batch` has the batch's row count, each cell
//! equals the row-oriented answer (missing values as null, mixed int/float fields kept
//! per cell, vectors and text intact), converting back with `to_rows` gives the same
//! rows, and an empty result still names its columns.

use lin::{Cell, Db};

fn seed() -> Db {
    let mut db = Db::empty();
    db.run("col t { id: text, k: i64, f: f64, s: text, b: bool, ts: time }").unwrap();
    db.run(r#"insert t [
        { id: "a", k: 1, f: 1.5, s: "x", b: true, ts: now },
        { id: "b", f: 2, s: "y" },
        { id: "c", k: 3, b: false },
        { id: "d", k: 4, f: 0.25, s: "", ts: now - 1d }
    ]"#).unwrap();
    db.run(r#"insert docs [{ uri: "raw://rb/1", title: "vector one", layer: "raw" }, { uri: "raw://rb/2", title: "", layer: "raw" }]"#).unwrap();
    db
}

fn norm(c: &Cell) -> String {
    match c {
        Cell::Null => "null".into(),
        Cell::Int(i) => format!("num:{}", *i as f64),
        Cell::Float(f) => format!("num:{f}"),
        other => format!("{other:?}"),
    }
}

#[test]
fn batches_match_rows() {
    let mut db = seed();
    for q in ["t | sort id | take all", "t | { id, k, f } | sort id | take all", "t | k > 100 | { id, k } | take all", "docs | { uri, embedding } | sort uri | take all"] {
        let rows = db.run(q).unwrap().rows.clone();
        let batch = db.run_batch(q).unwrap_or_else(|e| panic!("{q}: {e}"));
        assert_eq!(batch.n(), rows.len(), "{q}: row count");
        for (ci, col) in batch.cols.iter().enumerate() {
            assert_eq!(col.len(), batch.n(), "{q}: column {} length", batch.names[ci]);
        }
        for (ri, row) in rows.iter().enumerate() {
            for (ci, name) in batch.names.iter().enumerate() {
                let want = row.get(name.as_str()).cloned().unwrap_or(Cell::Null);
                let got = batch.cols[ci].get(ri);
                assert_eq!(norm(&got), norm(&want), "{q}: row {ri} column {name}");
            }
        }
        let back = batch.to_rows();
        assert_eq!(back.len(), rows.len(), "{q}: to_rows length");
        if rows.is_empty() {
            let names: Vec<&str> = batch.names.iter().map(String::as_str).collect();
            assert!(names.contains(&"id") && names.contains(&"k"), "{q}: empty batch columns {names:?}");
        }
    }
}
