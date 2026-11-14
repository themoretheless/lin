//! Cold-tier filtered scans against the in-memory tier on the same data: predicates that
//! mix leaves the columnar kernels decide (fixed-width comparisons, dictionary text,
//! null checks) with leaves that need decoded cells (block-compressed text, `has`, regex,
//! mixed-type comparisons), and zone-map pruning under `not` and `!=`.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use lin::Db;

static SEQ: AtomicU64 = AtomicU64::new(0);

fn tmp() -> PathBuf {
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("lin-cold-pred-{}-{n}", std::process::id()));
    let _ = fs::remove_dir_all(&p);
    fs::create_dir_all(&p).unwrap();
    p
}

/// Sort key that ignores how a tier represents a value (inline vs shared text).
fn row_key(row: &lin::Row) -> String {
    row.iter()
        .map(|(k, c)| match (c.text(), c.as_f64()) {
            (Some(t), _) => format!("{k}={t}"),
            (None, Some(n)) => format!("{k}={n}"),
            _ => format!("{k}={c:?}"),
        })
        .collect::<Vec<_>>()
        .join(";")
}

const N: usize = 20_000;

/// `band` is constant over runs of 5000 rows, so whole 4096-row zone blocks hold a single
/// value: that is what lets `!=` and `not` prune a block. `body` is unique per row, which
/// keeps it out of the dictionary encoding.
fn seed(db: &mut Db) {
    db.run("col ev { id: text, kind: text, tag: text, band: i64, n: i64, gap: i64, score: f64, body: text }")
        .unwrap();
    db.run("index ev [tag]").unwrap();
    for chunk in 0..(N / 1000) {
        let mut s = String::from("insert ev [\n");
        for i in (chunk * 1000)..((chunk + 1) * 1000) {
            if i > chunk * 1000 {
                s.push(',');
            }
            s.push_str(&format!(r#"{{ id: "e{i:05}", band: {}"#, i / 5000));
            // Present only in rows 4096..8191 and 12288..16383: whole zone blocks are null.
            if (i / 4096) % 2 == 1 {
                s.push_str(&format!(", gap: {}", i % 97));
            }
            if i % 3 != 0 {
                s.push_str(&format!(r#", tag: "t{}""#, i % 40));
            }
            if i % 7 != 0 {
                s.push_str(&format!(r#", kind: "{}""#, ["red", "green", "blue", "grey"][i % 4]));
            }
            if i % 5 != 0 {
                s.push_str(&format!(", n: {}", (i * 31) % 200));
            }
            if i % 11 != 0 {
                s.push_str(&format!(", score: {}.5", (i * 17) % 90));
            }
            if i % 13 != 0 {
                let word = if i % 97 == 0 { "needle" } else { "hay" };
                s.push_str(&format!(
                    r#", body: "row {i} holds {word} and filler text number {} to stay unique""#,
                    i * 7919
                ));
            }
            s.push_str(" }");
        }
        s.push_str("\n]");
        db.run(&s).unwrap();
    }
}

#[test]
fn cold_partially_typed_predicates_match_in_memory() {
    let cold_dir = tmp();
    let hot_dir = tmp();
    let cold_opts = || lin::OpenOpts {
        sync: lin::SyncMode::Normal,
        cold: true,
    };
    {
        let mut db = Db::open_with(&cold_dir, cold_opts()).unwrap();
        seed(&mut db);
        db.checkpoint().unwrap();
        db.close().unwrap();
    }
    let mut cold = Db::open_with(&cold_dir, cold_opts()).unwrap();
    let mut hot = Db::open(&hot_dir).unwrap();
    seed(&mut hot);
    assert!(cold.store.is_disk_backed("ev"));

    let preds = [
        // Typed and slow leaves under `and` / `or` / `not`.
        r#"n > 100 and body ~ "needle""#,
        r#"body ~ "needle" and n > 100"#,
        r#"n < 3 or body ~ "needle""#,
        r#"body ~ "needle" or kind == "grey""#,
        r#"not (body ~ "needle") and band == 2"#,
        r#"not (n > 100 or body ~ "needle")"#,
        r#"(kind == "red" and body ~ "needle") or (band == 3 and n == 31)"#,
        r#"kind in ["red", "blue"] and body has "needle""#,
        r#"body has "needle" and score > 40"#,
        r#"body regex "holds (needle|nothing) and" and n >= 50"#,
        r#"band == 1 and not body regex "hay""#,
        r#"body is null and n > 150"#,
        r#"body is not null and kind is null and n < 20"#,
        // A whole-number column against a fractional literal.
        r#"n > 10.5 and band == 0"#,
        r#"n < 10.5 or kind == "purple""#,
        r#"not n > 10.5"#,
        r#"n == 31.0 and body ~ "hay""#,
        // Zone-map pruning under `not` and `!=`.
        r#"band != 2"#,
        r#"band != 0 and band != 3"#,
        r#"not band > 1"#,
        r#"not (band >= 1 and band <= 2)"#,
        r#"not (band == 0 or band == 3)"#,
        r#"not (band != 1)"#,
        r#"not (not band < 2)"#,
        r#"band != 2 and body ~ "needle""#,
        r#"not band == 1 and n is null"#,
        r#"not (band < 3 or n > 100)"#,
        r#"n != 62 and not band <= 2"#,
        r#"score != 44.5 and band != 0 and band != 1 and band != 2"#,
        // An index lookup with a residual predicate: typed, slow, and both.
        r#"tag == "t7" and n > 100"#,
        r#"tag == "t7" and body ~ "needle""#,
        r#"tag == "t11" and (n < 20 or body has "needle") and score is not null"#,
        r#"tag in ["t1", "t2", "t38"] and band != 1 and not n > 150"#,
        r#"tag in ["t5", "t5", "t6"] and kind == "blue""#,
        r#"tag == "t4" and n > 10.5 and kind is null"#,
        r#"tag == "no-such-tag" and n > 1"#,
        r#"tag == "t9" and band > 100"#,
        // Zone-map pruning on `is null` / `is not null`: `gap` is null over whole blocks.
        r#"gap is null"#,
        r#"gap is not null"#,
        r#"gap is null and band == 1"#,
        r#"not (gap is null) and n > 150"#,
        r#"gap is not null or kind == "red""#,
        r#"not (gap is not null or band != 2)"#,
        // Nothing and everything.
        r#"band != 0 and band == 0"#,
        r#"not band > 100"#,
        r#"band > 100 or body ~ "no-such-word""#,
    ];
    let mut checked = 0usize;
    for phase in ["segments only", "with overlay"] {
        if phase == "with overlay" {
            // Uncheckpointed DML: new rows live in the memtable, changed and deleted rows
            // are tombstoned in their segments.
            for db in [&mut cold, &mut hot] {
                db.run(
                    r#"insert ev [
                        { id: "x1", kind: "red", tag: "t7", band: 2, n: 150, score: 3.5, body: "late needle row one" },
                        { id: "x2", kind: "blue", tag: "t5", band: 9, n: 7, body: "late hay row two" },
                        { id: "x3", tag: "t11", band: 0, n: 12, score: 80.5 }
                    ]"#,
                )
                .unwrap();
                db.run(r#"update ev[tag == "t7" and n > 170] cas each { n: 1, kind: "grey" }"#)
                    .unwrap();
                db.run(r#"update ev[id == "e00194"] cas each { body: "rewritten with a needle", band: 3 }"#)
                    .unwrap();
                db.run(r#"delete ev[tag == "t11" and n < 30] cas each"#).unwrap();
                db.run(r#"delete ev[band == 1 and n == 62] cas each"#).unwrap();
            }
            assert!(cold.store.is_disk_backed("ev"), "overlay DML promoted the collection");
        }
    for p in preds {
            for tail in ["pick { id } | take all", "pick { id, kind, tag, n, body } | take all", "take all", "count"] {
                let q = format!("ev | {p} | {tail}");
                let mut c: Vec<String> = cold.run(&q).unwrap().rows.iter().map(row_key).collect();
                let mut h: Vec<String> = hot.run(&q).unwrap().rows.iter().map(row_key).collect();
                c.sort();
                h.sort();
                assert_eq!(c.len(), h.len(), "row count for {q} ({phase})");
                assert_eq!(c, h, "rows for {q} ({phase})");
                checked += 1;
            }
            // A cursor over the cold collection goes through the resumable chunked scan.
            let q = format!("ev | {p} | pick {{ id, n }} | take all");
            let lin::Stmt::Query(parsed) = lin::parse(&q).unwrap() else {
                panic!("not a query: {q}");
            };
            let mut c: Vec<String> = cold
                .cursor(&parsed)
                .unwrap()
                .map(|r| row_key(&r.unwrap()))
                .collect();
            let mut h: Vec<String> = hot
                .cursor(&parsed)
                .unwrap()
                .map(|r| row_key(&r.unwrap()))
                .collect();
            c.sort();
            h.sort();
            assert_eq!(c.len(), h.len(), "cursor row count for {q} ({phase})");
            assert_eq!(c, h, "cursor rows for {q} ({phase})");
        }
    }
    assert_eq!(checked, preds.len() * 4 * 2);
    assert!(cold.store.is_disk_backed("ev"), "a filtered scan promoted the collection");
}
