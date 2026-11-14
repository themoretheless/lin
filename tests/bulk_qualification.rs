//! Bulk insert semantics: one statement with many rows must leave exactly what the same
//! rows inserted one by one leave (generated ids, defaults, missing and mixed fields),
//! report an exact count, return rows up to the documented limit, and survive log replay
//! unchanged.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use lin::Db;

static SEQ: AtomicU64 = AtomicU64::new(0);

fn tmp() -> PathBuf {
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("lin-bulk-qual-{}-{n}", std::process::id()));
    let _ = fs::remove_dir_all(&p);
    fs::create_dir_all(&p).unwrap();
    p
}

fn crash_image(from: &Path) -> PathBuf {
    let to = tmp();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        if entry.file_type().unwrap().is_file() && entry.file_name() != "LOCK" {
            fs::copy(entry.path(), to.join(entry.file_name())).unwrap();
        }
    }
    to
}

/// Rows of a collection with the generated `id` replaced by a placeholder, so two
/// databases that generated different ids for the same rows still compare equal.
fn rows(db: &mut Db, q: &str) -> Vec<String> {
    let mut out: Vec<String> = db
        .run(q)
        .unwrap_or_else(|e| panic!("{q}: {e}"))
        .rows
        .iter()
        .map(|r| {
            r.iter()
                .map(|(k, c)| match c.text() {
                    Some(t) => format!("{k}={t:?}"),
                    None => format!("{k}={c:?}"),
                })
                .collect::<Vec<_>>()
                .join(";")
        })
        .collect();
    out.sort();
    out
}

fn record(i: usize) -> String {
    // Fields vary per row: some rows miss fields, one row adds a field the others lack,
    // some give an explicit id.
    let mut fields = Vec::new();
    if i % 3 == 0 {
        fields.push(format!(r#"id: "e{i}""#));
    }
    fields.push(format!(r#"name: "n{i}""#));
    if i % 4 != 1 {
        fields.push(format!("n: {i}"));
    }
    if i % 5 == 2 {
        fields.push(format!("f: {:?}", i as f64 / 4.0));
    }
    if i % 7 == 0 {
        fields.push("ok: true".to_string());
    }
    format!("{{ {} }}", fields.join(", "))
}

const SCHEMA: &str = "col t { id: text, name: text, n: i64, f: f64, ok: bool }";

#[test]
fn ids_are_generated_wherever_an_id_less_row_sits_in_the_batch() {
    // The batch used to decide from its first record whether to generate ids, so rows
    // without an id after a row with one were left with no id at all.
    for src in [
        r#"insert t [{ id: "e0", name: "a" }, { name: "b" }, { name: "c" }, { id: "e3", name: "d" }, { name: "e" }]"#,
        r#"insert t [{ name: "a" }, { id: "e1", name: "b" }, { name: "c" }]"#,
        r#"insert t [{ id: "e0", name: "a" }, { name: "b" }]"#,
    ] {
        let mut db = Db::empty();
        db.run(SCHEMA).unwrap();
        db.run(src).unwrap();
        let all = db.run("t | take all").unwrap().rows.clone();
        let ids: Vec<String> = all
            .iter()
            .map(|r| r.get("id").and_then(|c| c.text()).unwrap_or_else(|| panic!("{src}: row without id: {r:?}")).to_string())
            .collect();
        let mut distinct = ids.clone();
        distinct.sort();
        distinct.dedup();
        assert_eq!(distinct.len(), ids.len(), "{src}: {ids:?}");
        // Every row is reachable by its id.
        for id in &ids {
            let n = db.run(&format!(r#"t | id == "{id}" | count"#)).unwrap().rows[0].get("hits").and_then(|c| c.as_f64());
            assert_eq!(n, Some(1.0), "{src}: {id}");
        }
    }
}

#[test]
fn bulk_insert_matches_single_inserts() {
    for size in [1usize, 2, 127, 128, 129, 300, 2500] {
        let mut bulk = Db::empty();
        let mut single = Db::empty();
        bulk.run(SCHEMA).unwrap();
        single.run(SCHEMA).unwrap();
        let src = format!(
            "insert t [{}]",
            (0..size).map(record).collect::<Vec<_>>().join(", ")
        );
        let handle = bulk.run(&src).unwrap();
        assert_eq!(handle.done.n, size, "done.n for {size} rows");
        // Rows come back up to the limit and not beyond it.
        if size <= 128 {
            assert_eq!(handle.rows.len(), size, "returned rows for {size}");
        } else {
            assert!(handle.rows.is_empty(), "returned rows for {size}: {}", handle.rows.len());
        }
        for i in 0..size {
            single.run(&format!("insert t {}", record(i))).unwrap();
        }
        let q = "t | { name, n, f, ok } | take all";
        assert_eq!(rows(&mut bulk, q), rows(&mut single, q), "{size} rows");
        // Every row has an id, explicit ones are kept, and all ids are distinct.
        let ids = rows(&mut bulk, "t | { id } | take all");
        assert_eq!(ids.len(), size);
        let mut distinct = ids.clone();
        distinct.dedup();
        assert_eq!(distinct.len(), size, "duplicate generated ids for {size}");
        for i in (0..size).step_by(3) {
            let got = bulk.run(&format!(r#"t | id == "e{i}" | count"#)).unwrap().rows.clone();
            assert_eq!(got[0].get("hits").and_then(|c| c.as_f64()), Some(1.0), "e{i}");
        }
    }
}

#[test]
fn empty_and_degenerate_bulk_statements() {
    let mut db = Db::empty();
    db.run(SCHEMA).unwrap();
    // An empty record is rejected before anything is written.
    assert!(db.run("insert t [{}]").is_err());
    assert!(db.run(r#"insert t [{ name: "x" }, {}]"#).is_err());
    assert_eq!(rows(&mut db, "t | count"), vec!["hits=Int(0)".to_string()]);
    let handle = db.run(r#"insert t [{ name: "only" }]"#).unwrap();
    assert_eq!(handle.done.n, 1);
    assert_eq!(rows(&mut db, "t | count"), vec!["hits=Int(1)".to_string()]);
    // Explicit ids that collide with a generated one are rejected, not overwritten.
    let generated = db.run("t | { id } | take all").unwrap().rows[0]
        .get("id")
        .and_then(|c| c.text())
        .unwrap()
        .to_string();
    assert!(db.run(&format!(r#"insert t [{{ id: "{generated}", name: "dup" }}]"#)).is_err());
    assert_eq!(rows(&mut db, "t | count"), vec!["hits=Int(1)".to_string()]);
}

#[test]
fn bulk_insert_survives_log_replay_unchanged() {
    let dir = tmp();
    let mut db = Db::open(&dir).unwrap();
    db.run(SCHEMA).unwrap();
    for (start, size) in [(0usize, 1usize), (1, 128), (129, 129), (258, 3000)] {
        let src = format!(
            "insert t [{}]",
            (start..start + size).map(record).collect::<Vec<_>>().join(", ")
        );
        db.run(&src).unwrap();
    }
    let q = "t | { id, name, n, f, ok } | take all";
    let want = rows(&mut db, q);
    assert_eq!(want.len(), 3258);
    let mut recovered = Db::open(crash_image(&dir)).unwrap();
    assert_eq!(rows(&mut recovered, q), want);
    // New rows after recovery get ids that do not collide with replayed ones.
    recovered.run("insert t [{ name: \"after-1\" }, { name: \"after-2\" }]").unwrap();
    let ids = rows(&mut recovered, "t | { id } | take all");
    let mut distinct = ids.clone();
    distinct.dedup();
    assert_eq!(distinct.len(), ids.len());
    assert_eq!(ids.len(), 3260);
}
