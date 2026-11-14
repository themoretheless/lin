//! Statement and transaction atomicity: a write that fails validation part way through a
//! batch leaves no trace — not in memory, not in the log (a crash image recovers the
//! same state), not on the cold tier — and the database stays usable. Covers duplicate
//! keys inside a batch and against existing rows, a foreign key and a unique index that
//! fail on a late row, a panic inside a transaction, and transactions over several
//! collections.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use lin::Db;

static SEQ: AtomicU64 = AtomicU64::new(0);

fn tmp() -> PathBuf {
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("lin-atomicity-{}-{n}", std::process::id()));
    let _ = fs::remove_dir_all(&p);
    fs::create_dir_all(&p).unwrap();
    p
}

fn crash_image(from: &Path) -> PathBuf {
    fn copy(from: &Path, to: &Path) {
        fs::create_dir_all(to).unwrap();
        for entry in fs::read_dir(from).unwrap() {
            let entry = entry.unwrap();
            let dst = to.join(entry.file_name());
            if entry.file_type().unwrap().is_dir() {
                copy(&entry.path(), &dst);
            } else if entry.file_name() != "LOCK" {
                fs::copy(entry.path(), dst).unwrap();
            }
        }
    }
    let to = tmp();
    copy(from, &to);
    to
}

fn opts(cold: bool) -> lin::OpenOpts {
    lin::OpenOpts {
        sync: lin::SyncMode::Normal,
        cold,
    }
}

const SCHEMA: &str = "col acct { id: text, org: text, name: text, n: i64 }\n\
                      index acct unique [org, name]\n\
                      col parent { id: text, label: text }\n\
                      col child { id: text, p: text, n: i64 }";
const SESSION: &str = "fk child.p -> parent.id";

fn seed(db: &mut Db) {
    db.run(SCHEMA).unwrap();
    db.run(SESSION).unwrap();
    let mut acct = String::from("insert acct [");
    for i in 0..200 {
        if i > 0 {
            acct.push(',');
        }
        acct.push_str(&format!(r#"{{ id: "a{i}", org: "o{}", name: "n{i}", n: {i} }}"#, i % 4));
    }
    acct.push(']');
    db.run(&acct).unwrap();
    db.run(r#"insert parent [{ id: "p1", label: "one" }, { id: "p2", label: "two" }]"#).unwrap();
    db.run(r#"insert child [{ id: "c1", p: "p1", n: 1 }, { id: "c2", p: "p2", n: 2 }]"#).unwrap();
    let mut docs = String::from("insert docs [");
    for i in 0..60 {
        if i > 0 {
            docs.push(',');
        }
        docs.push_str(&format!(r#"{{ uri: "raw://a/{i}", title: "Doc {i}", layer: "raw" }}"#));
    }
    docs.push(']');
    db.run(&docs).unwrap();
}

fn state(db: &mut Db) -> Vec<String> {
    let mut out = Vec::new();
    for q in [
        "acct | { id, org, name, n } | take all",
        "parent | { id, label } | take all",
        "child | { id, p, n } | take all",
        "docs | { uri, title } | take all",
    ] {
        let mut rows: Vec<String> = db
            .run(q)
            .unwrap_or_else(|e| panic!("{q}: {e}"))
            .rows
            .iter()
            .map(|r| {
                r.iter()
                    .map(|(k, c)| match (c.text(), c.as_f64()) {
                        (Some(t), _) => format!("{k}={t}"),
                        (None, Some(n)) => format!("{k}={n}"),
                        _ => format!("{k}={c:?}"),
                    })
                    .collect::<Vec<_>>()
                    .join(";")
            })
            .collect();
        rows.sort();
        out.push(format!("{q}: {}", rows.join(" | ")));
    }
    out
}

/// Statements that must fail as a whole, each with a valid prefix before the bad row.
fn failing_statements() -> Vec<(&'static str, String)> {
    vec![
        ("duplicate id inside a batch", r#"insert acct [{ id: "x1", org: "z", name: "z1", n: 1 }, { id: "x2", org: "z", name: "z2", n: 2 }, { id: "x1", org: "z", name: "z3", n: 3 }]"#.into()),
        ("duplicate uri inside a batch", r#"insert docs [{ uri: "raw://b/1", title: "B1", layer: "raw" }, { uri: "raw://b/2", title: "B2", layer: "raw" }, { uri: "raw://b/1", title: "B3", layer: "raw" }]"#.into()),
        ("collision with an existing id", r#"insert acct [{ id: "x9", org: "z", name: "z9", n: 9 }, { id: "a17", org: "z", name: "z10", n: 10 }]"#.into()),
        ("collision with an existing uri", r#"insert docs [{ uri: "raw://b/9", title: "B9", layer: "raw" }, { uri: "raw://a/33", title: "again", layer: "raw" }]"#.into()),
        ("unique index on a late row", r#"insert acct [{ id: "x5", org: "q", name: "q1", n: 5 }, { id: "x6", org: "o1", name: "n5", n: 6 }]"#.into()),
        ("unique index inside the batch", r#"insert acct [{ id: "x7", org: "q", name: "q7", n: 7 }, { id: "x8", org: "q", name: "q7", n: 8 }]"#.into()),
        ("foreign key on a late row", r#"insert child [{ id: "c8", p: "p1", n: 8 }, { id: "c9", p: "missing", n: 9 }]"#.into()),
        ("update onto an existing unique key", r#"update acct[id == "a8"] cas each { org: "o1", name: "n1" }"#.into()),
        ("program with a failing last statement", "insert acct { id: \"y1\", org: \"y\", name: \"y1\", n: 1 }\ninsert parent { id: \"p9\", label: \"nine\" }\ninsert acct { id: \"a3\", org: \"y\", name: \"y2\", n: 2 }".into()),
    ]
}

#[test]
fn failed_writes_leave_no_trace() {
    for cold in [false, true] {
        let dir = tmp();
        let mut db = Db::open_with(&dir, opts(cold)).unwrap();
        seed(&mut db);
        if cold {
            db.checkpoint().unwrap();
            db.close().unwrap();
            drop(db);
            db = Db::open_with(&dir, opts(true)).unwrap();
            db.run(SESSION).unwrap();
            assert!(db.store.is_disk_backed("acct"), "acct is not cold");
        }
        let before = state(&mut db);
        let gen_before = db.r#gen();
        let log_before = fs::metadata(dir.join(lin::persist::LOG_NAME)).unwrap().len();
        for (what, stmt) in failing_statements() {
            let err = db.run(&stmt).err().unwrap_or_else(|| panic!("cold={cold}, {what}: accepted"));
            assert_eq!(state(&mut db), before, "cold={cold}, {what}: state changed ({err})");
            assert_eq!(db.r#gen(), gen_before, "cold={cold}, {what}: generation moved");
            let log_now = fs::metadata(dir.join(lin::persist::LOG_NAME)).unwrap().len();
            assert_eq!(log_now, log_before, "cold={cold}, {what}: a record was logged");
        }
        // The same statements inside a transaction: the transaction is rolled back whole.
        for (what, stmt) in failing_statements() {
            let mut tx = db.transaction().unwrap();
            tx.run(r#"insert parent { id: "tx-before", label: "kept only on commit" }"#).unwrap();
            assert!(tx.run(&stmt).is_err(), "cold={cold}, tx {what}: accepted");
            drop(tx);
            assert_eq!(state(&mut db), before, "cold={cold}, tx {what}: state changed");
        }
        // Nothing of the above reached the log: a crash image recovers the same state.
        let mut recovered = Db::open_with(crash_image(&dir), opts(cold)).unwrap();
        assert_eq!(state(&mut recovered), before, "cold={cold}: crash image");
        drop(recovered);

        // And the database is still usable, with the keys the failures tried to take.
        db.run(r#"insert acct [{ id: "x1", org: "z", name: "z1", n: 1 }, { id: "x2", org: "z", name: "z2", n: 2 }]"#).unwrap();
        db.run(r#"insert docs { uri: "raw://b/1", title: "B1", layer: "raw" }"#).unwrap();
        db.run(r#"insert child { id: "c9", p: "p2", n: 9 }"#).unwrap();
        let after = state(&mut db);
        assert_ne!(after, before);
        let mut recovered = Db::open_with(crash_image(&dir), opts(cold)).unwrap();
        assert_eq!(state(&mut recovered), after, "cold={cold}: crash image after valid writes");
    }
}

#[test]
fn panic_inside_a_transaction_rolls_back_and_the_database_stays_usable() {
    for cold in [false, true] {
        let dir = tmp();
        let mut db = Db::open_with(&dir, opts(cold)).unwrap();
        seed(&mut db);
        if cold {
            db.checkpoint().unwrap();
        }
        let before = state(&mut db);
        let gen_before = db.r#gen();
        let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            db.transaction_fn(|tx| {
                tx.run(r#"insert acct { id: "pz", org: "pz", name: "pz", n: 1 }"#)?;
                tx.run(r#"insert docs { uri: "raw://panic", title: "panic", layer: "raw" }"#)?;
                tx.run(r#"update acct[id == "a1"] cas each { n: 1000 }"#)?;
                panic!("boom inside the transaction");
                #[allow(unreachable_code)]
                Ok(())
            })
        }));
        assert!(caught.is_err(), "cold={cold}: the panic was swallowed");
        assert_eq!(state(&mut db), before, "cold={cold}: writes before the panic survived");
        assert_eq!(db.r#gen(), gen_before, "cold={cold}");

        // The same database takes the next transaction and plain writes normally.
        db.transaction_fn(|tx| {
            tx.run(r#"insert acct { id: "pz", org: "pz", name: "pz", n: 1 }"#)?;
            tx.run(r#"insert docs { uri: "raw://panic", title: "after", layer: "raw" }"#)?;
            Ok(())
        })
        .unwrap();
        db.run(r#"update acct[id == "a1"] cas each { n: 1001 }"#).unwrap();
        let after = state(&mut db);
        assert!(after[0].contains("id=pz") && after[0].contains("n=1001"), "cold={cold}");
        let mut recovered = Db::open_with(crash_image(&dir), opts(cold)).unwrap();
        assert_eq!(state(&mut recovered), after, "cold={cold}: crash image");
    }
}

#[test]
fn transactions_over_several_collections_commit_and_roll_back_together() {
    for cold in [false, true] {
        let dir = tmp();
        let mut db = Db::open_with(&dir, opts(cold)).unwrap();
        seed(&mut db);
        if cold {
            db.checkpoint().unwrap();
        }
        let before = state(&mut db);
        let writes = [
            r#"insert parent { id: "p3", label: "three" }"#,
            r#"insert child { id: "c3", p: "p3", n: 3 }"#,
            r#"update acct[org == "o2"] cas each { n: -1 }"#,
            r#"delete docs[uri == "raw://a/5"] cas each"#,
            r#"insert docs { uri: "raw://t/1", title: "T1", layer: "wiki" }"#,
        ];
        // Explicit rollback.
        {
            let mut tx = db.transaction().unwrap();
            for w in writes {
                tx.run(w).unwrap();
            }
            tx.rollback().unwrap();
        }
        assert_eq!(state(&mut db), before, "cold={cold}: rollback");
        // Commit: every collection changes, and a crash image agrees.
        {
            let mut tx = db.transaction().unwrap();
            for w in writes {
                tx.run(w).unwrap();
            }
            tx.commit().unwrap();
        }
        let after = state(&mut db);
        assert!(after[1].contains("id=p3") && after[2].contains("id=c3"), "cold={cold}");
        assert!(after[0].contains("n=-1") && !after[3].contains("raw://a/5;"), "cold={cold}");
        let mut recovered = Db::open_with(crash_image(&dir), opts(cold)).unwrap();
        assert_eq!(state(&mut recovered), after, "cold={cold}: crash image after commit");
    }
}

/// Interactive transaction rollback restores the exact state from before the
/// transaction, whatever mix of statements it ran. Two journal bugs lost rows here: a
/// delete followed by an insert dropped the row the delete had moved into the gap, and
/// once the journal switched to a full backup part way through (a schema statement or an
/// insert after row events), the writes made before the switch survived the rollback.
#[test]
fn transaction_rollback_restores_every_statement_mix() {
    let setup = || {
        let mut db = Db::empty();
        db.run("col acct { id: text, org: text, n: i64 }").unwrap();
        let mut acct = String::from("insert acct [");
        for i in 0..20 {
            if i > 0 {
                acct.push(',');
            }
            acct.push_str(&format!(r#"{{ id: "a{i}", org: "o{}", n: {i} }}"#, i % 4));
        }
        acct.push(']');
        db.run(&acct).unwrap();
        let mut docs = String::from("insert docs [");
        for i in 0..10 {
            if i > 0 {
                docs.push(',');
            }
            docs.push_str(&format!(r#"{{ uri: "raw://m/{i}", title: "Doc {i}", layer: "raw" }}"#));
        }
        docs.push(']');
        db.run(&docs).unwrap();
        db
    };
    let snapshot = |db: &mut Db| -> Vec<String> {
        let mut out = Vec::new();
        for q in ["acct | take all", "docs | { uri, title } | take all", "facts | take all"] {
            let mut rows: Vec<String> = db
                .run(q)
                .unwrap()
                .rows
                .iter()
                .map(|r| {
                    r.iter()
                        .map(|(k, c)| match (c.text(), c.as_f64()) {
                            (Some(t), _) => format!("{k}={t}"),
                            (None, Some(n)) => format!("{k}={n}"),
                            _ => format!("{k}={c:?}"),
                        })
                        .collect::<Vec<_>>()
                        .join(";")
                })
                .collect();
            rows.sort();
            out.push(rows.join(" | "));
        }
        out
    };
    let update = r#"update acct[org == "o2"] cas each { n: -1 }"#;
    let delete_doc = r#"delete docs[uri == "raw://m/5"] cas each"#;
    let delete_acct = r#"delete acct[id == "a3" or id == "a11"] cas each"#;
    let insert_doc = r#"insert docs { uri: "raw://m/new", title: "New", layer: "wiki" }"#;
    let insert_acct = r#"insert acct [{ id: "x1", org: "o9", n: 1 }, { id: "x2", org: "o9", n: 2 }]"#;
    let fact = r#"append facts { s: "m", p: tagged, o: "t" }"#;
    let schema = "col extra { id: text }";
    let program = "insert acct { id: \"y1\", org: \"o8\", n: 1 }\ndelete docs[uri == \"raw://m/1\"] cas each";
    let mixes: [&[&str]; 14] = [
        &[delete_doc, insert_doc],
        &[delete_acct, insert_acct],
        &[insert_doc, delete_doc],
        &[delete_doc, insert_doc, delete_acct, insert_acct],
        &[insert_acct, update, insert_acct.replace("x1", "x3").replace("x2", "x4").leak()],
        &[update, schema],
        &[insert_acct, schema],
        &[delete_doc, schema],
        &[update, program],
        &[delete_doc, insert_doc, schema, update],
        &[fact, delete_doc, insert_doc],
        &[update, delete_acct, insert_acct, update],
        &[schema, insert_doc, delete_doc],
        &[program, delete_acct, insert_doc],
    ];
    for writes in mixes {
        for end in ["rollback", "drop", "error"] {
            let mut db = setup();
            let before = snapshot(&mut db);
            let gen_before = db.r#gen();
            let mut tx = db.transaction().unwrap();
            for w in writes {
                tx.run(w).unwrap_or_else(|e| panic!("{writes:?}: {w}: {e}"));
            }
            match end {
                "rollback" => tx.rollback().unwrap(),
                "drop" => drop(tx),
                _ => {
                    assert!(tx.run(r#"insert acct { id: "a0", org: "o0", n: 0 }"#).is_err());
                    drop(tx);
                }
            }
            assert_eq!(snapshot(&mut db), before, "{writes:?}, {end}");
            assert_eq!(db.r#gen(), gen_before, "{writes:?}, {end}");
            // The same writes then commit cleanly on the restored state.
            let mut tx = db.transaction().unwrap();
            for w in writes {
                tx.run(w).unwrap_or_else(|e| panic!("{writes:?} after {end}: {w}: {e}"));
            }
            tx.commit().unwrap();
            assert_ne!(snapshot(&mut db), before, "{writes:?}, {end}: commit changed nothing");
        }
    }
}

/// A batch that breaks several rules at once fails with the same error every time, and
/// the error names the first offending row in batch order, whatever the rule.
#[test]
fn several_violations_in_one_batch_fail_deterministically() {
    let cases = [
        // Duplicate id first, then a missing parent.
        r#"insert child [{ id: "c1", p: "p1", n: 1 }, { id: "c5", p: "missing", n: 5 }]"#,
        // Missing parent first, then a duplicate id.
        r#"insert child [{ id: "c6", p: "missing", n: 6 }, { id: "c2", p: "p2", n: 2 }]"#,
        // Unique index and duplicate id in one row, then a duplicate inside the batch.
        r#"insert acct [{ id: "a5", org: "o1", name: "n1", n: 1 }, { id: "z", org: "q", name: "q", n: 2 }, { id: "z", org: "q", name: "r", n: 3 }]"#,
    ];
    for stmt in cases {
        let mut errors = std::collections::BTreeSet::new();
        for _ in 0..5 {
            let mut db = Db::empty();
            seed(&mut db);
            let err = db.run(stmt).err().unwrap_or_else(|| panic!("{stmt}: accepted"));
            errors.insert(err.to_string());
        }
        assert_eq!(errors.len(), 1, "{stmt}: {errors:?}");
    }
    // The first offending row decides the message.
    let mut db = Db::empty();
    seed(&mut db);
    let first = db.run(cases[0]).unwrap_err().to_string();
    assert!(first.contains("c1"), "{first}");
    let second = db.run(cases[1]).unwrap_err().to_string();
    assert!(second.contains("missing") || second.contains("fk"), "{second}");
}

/// A schema statement rolled back with its transaction leaves no trace in the catalog:
/// the collection does not exist afterwards, can be declared again with another shape,
/// and the database reopens (the catalog hash on disk is the one from before).
#[test]
fn rolled_back_schema_leaves_the_catalog_unchanged() {
    let dir = tmp();
    let mut db = Db::open(&dir).unwrap();
    seed(&mut db);
    {
        let mut tx = db.transaction().unwrap();
        tx.run("col drift { id: text, a: i64 }").unwrap();
        tx.run(r#"insert drift { id: "d1", a: 1 }"#).unwrap();
        tx.run("index acct [name]").unwrap();
        tx.rollback().unwrap();
    }
    assert!(db.run("drift | take all").is_err(), "rolled-back collection still exists");
    db.run("col drift { id: text, b: text }").unwrap();
    db.run(r#"insert drift { id: "d2", b: "x" }"#).unwrap();
    let rows = db.run("drift | take all").unwrap().rows.clone();
    assert_eq!(rows.len(), 1);
    assert!(rows[0].get("a").is_none());
    db.close().unwrap();
    drop(db);
    let mut reopened = Db::open(&dir).unwrap();
    assert_eq!(reopened.run("drift | take all").unwrap().rows.len(), 1);
    let mut recovered = Db::open(crash_image(&dir)).unwrap();
    assert_eq!(recovered.run("drift | take all").unwrap().rows.len(), 1);
}
