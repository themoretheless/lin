//! Unique indexes: the same statements must be accepted or rejected identically in
//! memory, on a durable database and on the cold tier, end in the same state, and that
//! state must survive a crash. On top of that agreement, the cases with one right answer
//! are pinned: an update keeping its own key is allowed, deleting or rolling back frees
//! a key, a duplicate inside one batch is rejected, a unique index cannot be declared
//! over existing duplicates, and distinct strings never collide (Unicode forms, case,
//! inline and shared storage, composite keys mixing types).

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use lin::Db;

static SEQ: AtomicU64 = AtomicU64::new(0);

fn tmp() -> PathBuf {
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("lin-unique-qual-{}-{n}", std::process::id()));
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

const LONG: &str = "a-shared-text-key-longer-than-twenty-two-bytes";

fn seed(db: &mut Db) {
    db.run(
        "col u { id: text, k: text, m: text, x: f64, i: i64 }\n\
         index u unique [k]\n\
         index u unique [m, i]",
    )
    .unwrap();
    let mut src = String::from("insert u [");
    for n in 0..60 {
        if n > 0 {
            src.push(',');
        }
        src.push_str(&format!(r#"{{ id: "s{n}", k: "key{n}", m: "m{}", i: {n} }}"#, n % 7));
    }
    src.push(']');
    db.run(&src).unwrap();
}

/// (statement, expected outcome when it is pinned: Some(true) = accepted)
fn statements() -> Vec<(String, Option<bool>)> {
    let e_acute = "\u{e9}";
    let e_combining = "e\u{301}";
    vec![
        (r#"insert u { id: "a1", k: "key1", m: "u1001", i: 1001 }"#.into(), Some(false)),
        (r#"insert u { id: "a2", k: "KEY1", m: "u1002", i: 1002 }"#.into(), Some(true)),
        (format!(r#"insert u {{ id: "a3", k: "{e_acute}", m: "v2001", i: 2001 }}"#), Some(true)),
        (format!(r#"insert u {{ id: "a4", k: "{e_combining}", m: "v2002", i: 2002 }}"#), Some(true)),
        (format!(r#"insert u {{ id: "a5", k: "{e_acute}", m: "v2003", i: 2003 }}"#), Some(false)),
        (r#"insert u { id: "a6", k: "", m: "u1006", i: 1006 }"#.into(), Some(true)),
        (r#"insert u { id: "a7", k: "", m: "u1007", i: 1007 }"#.into(), Some(false)),
        (format!(r#"insert u {{ id: "a8", k: "{LONG}", m: "v2004", i: 2004 }}"#), Some(true)),
        (format!(r#"insert u {{ id: "a9", k: "{LONG}", m: "v2005", i: 2005 }}"#), Some(false)),
        (format!(r#"insert u {{ id: "a10", k: "{}", m: "v2006", i: 2006 }}"#, &LONG[..22]), Some(true)),
        // Missing values in a unique key compare equal (unlike SQL NULLs): the first row
        // without `k` is accepted, a second one is a duplicate.
        (r#"insert u { id: "b1", m: "z", i: 1 }"#.into(), Some(true)),
        (r#"insert u { id: "b2", m: "z", i: 2 }"#.into(), Some(false)),
        // Composite keys: same text with a different integer is a different key.
        (r#"insert u { id: "c1", k: "c1", m: "m1", i: 1 }"#.into(), Some(false)),
        (r#"insert u { id: "c2", k: "c2", m: "m1", i: 1001 }"#.into(), Some(true)),
        (r#"insert u { id: "c3", k: "c3", m: "1", i: 1 }"#.into(), Some(true)),
        // Self-match: an update keeping or rewriting its own key.
        (r#"update u[id == "s5"] cas each { k: "key5" }"#.into(), Some(true)),
        (r#"update u[id == "s5"] cas each { k: "key5", n2: 1 }"#.into(), None),
        (r#"update u[id == "s6"] cas each { k: "key7" }"#.into(), Some(false)),
        // Two rows swapping keys in one statement each way is a conflict.
        (r#"update u[id == "s8" or id == "s9"] cas each { k: "same" }"#.into(), Some(false)),
        // Delete frees a key; the swap-removed row keeps its own.
        (r#"delete u[id == "s10"] cas each"#.into(), Some(true)),
        (r#"insert u { id: "d1", k: "key10", m: "u1011", i: 1011 }"#.into(), Some(true)),
        (r#"insert u { id: "d2", k: "key59", m: "u1012", i: 1012 }"#.into(), Some(false)),
        // Cross-conflicts inside one batch, and with existing rows.
        (r#"insert u [{ id: "e1", k: "batch", m: "u1013", i: 1013 }, { id: "e2", k: "batch", m: "u1014", i: 1014 }]"#.into(), Some(false)),
        (r#"insert u [{ id: "e3", k: "fresh1", m: "u1015", i: 1015 }, { id: "e4", k: "key20", m: "u1016", i: 1016 }]"#.into(), Some(false)),
        (r#"insert u [{ id: "e5", k: "fresh2", m: "u1017", i: 1017 }, { id: "e6", k: "fresh3", m: "u1018", i: 1018 }]"#.into(), Some(true)),
        // A unique index over existing duplicates cannot be declared.
        (r#"insert u [{ id: "f1", k: "f1", x: 1.5, m: "u1019", i: 1019 }, { id: "f2", k: "f2", x: 1.5, m: "u1020", i: 1020 }]"#.into(), Some(true)),
        ("index u unique [x]".into(), Some(false)),
        // Declaring the same index again is harmless or rejected, but changes nothing.
        ("index u unique [k]".into(), None),
    ]
}

fn state(db: &mut Db) -> Vec<String> {
    let mut rows: Vec<String> = db
        .run("u | { id, k, m, x, i } | take all")
        .unwrap()
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
    rows.sort();
    rows
}

#[test]
fn unique_indexes_agree_across_tiers_and_pin_the_definite_cases() {
    let mut mem = Db::empty();
    seed(&mut mem);
    let durable_dir = tmp();
    let mut durable = Db::open(&durable_dir).unwrap();
    seed(&mut durable);
    let cold_dir = tmp();
    let cold_opts = || lin::OpenOpts {
        sync: lin::SyncMode::Normal,
        cold: true,
    };
    {
        let mut c = Db::open_with(&cold_dir, cold_opts()).unwrap();
        seed(&mut c);
        c.checkpoint().unwrap();
        c.close().unwrap();
    }
    let mut cold = Db::open_with(&cold_dir, cold_opts()).unwrap();
    assert!(cold.store.is_disk_backed("u"), "u is not cold");

    for (stmt, expected) in statements() {
        let outcomes: Vec<Result<(), String>> = [&mut mem, &mut durable, &mut cold]
            .into_iter()
            .map(|db| db.run(&stmt).map(|_| ()).map_err(|e| e.to_string()))
            .collect();
        let accepted: Vec<bool> = outcomes.iter().map(Result::is_ok).collect();
        assert!(
            accepted.iter().all(|&a| a == accepted[0]),
            "{stmt}: memory / durable / cold disagree: {outcomes:?}"
        );
        if let Some(want) = expected {
            assert_eq!(accepted[0], want, "{stmt}: {outcomes:?}");
        }
        let s = state(&mut mem);
        assert_eq!(state(&mut durable), s, "{stmt}: durable state differs");
        assert_eq!(state(&mut cold), s, "{stmt}: cold state differs");
    }

    // Rolling back a transaction frees the keys it took.
    for db in [&mut mem, &mut durable, &mut cold] {
        let mut tx = db.transaction().unwrap();
        tx.run(r#"insert u { id: "t1", k: "tx-key", m: "tx", i: 1 }"#).unwrap();
        tx.run(r#"update u[id == "s30"] cas each { k: "tx-moved" }"#).unwrap();
        tx.rollback().unwrap();
        db.run(r#"insert u { id: "t2", k: "tx-key", m: "tx", i: 1 }"#).unwrap();
        db.run(r#"insert u { id: "t3", k: "key30-other", m: "u1021", i: 1021 }"#).unwrap();
        assert!(db.run(r#"insert u { id: "t4", k: "key30" }"#).is_err(), "key30 was freed by a rollback");
    }
    let s = state(&mut mem);
    assert_eq!(state(&mut durable), s);
    assert_eq!(state(&mut cold), s);

    // The final state, and the constraints, survive a crash.
    for (dir, cold_tier) in [(&durable_dir, false), (&cold_dir, true)] {
        let opts = lin::OpenOpts {
            sync: lin::SyncMode::Normal,
            cold: cold_tier,
        };
        let mut recovered = Db::open_with(crash_image(dir), opts).unwrap();
        assert_eq!(state(&mut recovered), s, "cold={cold_tier}: crash image");
        assert!(recovered.run(r#"insert u { id: "z1", k: "key1", m: "u1022", i: 1022 }"#).is_err(), "cold={cold_tier}");
        assert!(recovered.run(r#"insert u { id: "z2", k: "c2", m: "m1", i: 1001 }"#).is_err());
        recovered.run(r#"insert u { id: "z3", k: "after-crash", m: "u1023", i: 1023 }"#).unwrap();
    }
}
