//! A follower is read-only through every entry point (statements, schema statements,
//! transactions, prepared statements with parameters), a refused write leaves no trace,
//! and the leader's catalog changes (new collections, indexes, foreign keys) reach the
//! follower through shipped log frames, survive its reopen, and are usable by queries.

use lin::{Cell, Db};

fn tmp() -> std::path::PathBuf {
    let n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let d = std::env::temp_dir().join(format!("lin-follower-{}-{n}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn count(db: &mut Db, q: &str) -> f64 {
    db.run(q).unwrap().rows[0].get("hits").and_then(|c| c.as_f64()).unwrap()
}

#[test]
fn follower_refuses_every_write_and_receives_the_catalog() {
    let leader_dir = tmp();
    let follower_dir = tmp();
    let mut leader = Db::open(&leader_dir).unwrap();
    leader.run("col acct { id: text, org: text, n: i64 }").unwrap();
    leader.run("index acct [org]").unwrap();
    leader.run(r#"insert acct [{ id: "a1", org: "o1", n: 1 }, { id: "a2", org: "o2", n: 2 }]"#).unwrap();
    let mut follower = Db::open_follower(&follower_dir).unwrap();
    assert!(follower.is_follower());
    follower.apply_wal(&leader.export_wal_since(0).unwrap()).unwrap();
    assert_eq!(count(&mut follower, r#"acct | org == "o1" | count"#), 1.0);
    let gen_before = follower.r#gen();

    for w in [
        r#"insert acct { id: "x", org: "o1", n: 9 }"#,
        r#"update acct[id == "a1"] cas each { n: 100 }"#,
        r#"delete acct[id == "a2"] cas each"#,
        "col sneaky { id: text }",
        "index acct [n]",
        r#"insert docs { uri: "raw://f/1", title: "t", layer: "raw" }"#,
    ] {
        assert!(follower.run(w).is_err(), "follower accepted: {w}");
    }
    let p = follower.prepare(r#"insert acct { id: $id, org: "o9", n: 0 }"#).unwrap();
    assert!(p.execute_with(&mut follower, &[("id", Cell::text_arc("via-prepared"))]).is_err(), "prepared write accepted");
    if let Ok(mut tx) = follower.transaction() {
        assert!(tx.run(r#"insert acct { id: "via-tx", org: "o1", n: 0 }"#).is_err() || tx.commit().is_err(), "transaction write accepted");
    }
    assert_eq!(follower.r#gen(), gen_before, "a refused write moved the generation");
    assert_eq!(count(&mut follower, "acct | count"), 2.0);
    assert!(follower.run("sneaky | take all").is_err());

    // Catalog changes on the leader arrive with the frames.
    let since = follower.r#gen();
    leader.run("col team { id: text, acct: text, label: text }").unwrap();
    // `fk` may be a session statement; either outcome is fine for this test.
    let _ = leader.run("fk team.acct -> acct.id");
    leader.run("index team [label]").unwrap();
    leader.run(r#"insert team [{ id: "t1", acct: "a1", label: "red" }, { id: "t2", acct: "a2", label: "blue" }]"#).unwrap();
    follower.apply_wal(&leader.export_wal_since(since).unwrap()).unwrap();
    assert_eq!(count(&mut follower, r#"team | label == "red" | count"#), 1.0);
    drop(follower);
    let mut follower = Db::open_follower(&follower_dir).unwrap();
    assert_eq!(count(&mut follower, r#"team | label == "blue" | count"#), 1.0);
    assert_eq!(count(&mut follower, "acct | count"), 2.0);
    assert!(follower.run(r#"insert team { id: "t3", acct: "a1", label: "x" }"#).is_err());
}


/// The leader checkpoints (emptying its log) while a follower lags behind. Catching up
/// must either deliver every missed change or fail loudly: a follower must never end up
/// silently missing rows while believing it is current.
#[test]
fn follower_lagging_across_a_leader_checkpoint() {
    let leader_dir = tmp();
    let follower_dir = tmp();
    let mut leader = Db::open(&leader_dir).unwrap();
    leader.run("col ev { id: text, n: i64 }").unwrap();
    leader.run(r#"insert ev { id: "e0", n: 0 }"#).unwrap();
    let mut follower = Db::open_follower(&follower_dir).unwrap();
    follower.apply_wal(&leader.export_wal_since(0).unwrap()).unwrap();
    let lag_gen = follower.r#gen();
    for i in 1..=5 {
        leader.run(&format!(r#"insert ev {{ id: "e{i}", n: {i} }}"#)).unwrap();
    }
    leader.checkpoint().unwrap();
    leader.run(r#"insert ev { id: "e6", n: 6 }"#).unwrap();
    let catch_up = leader.export_wal_since(lag_gen).and_then(|frames| follower.apply_wal(&frames));
    match catch_up {
        Ok(_) => {
            assert_eq!(count(&mut follower, "ev | count"), 7.0, "follower caught up with rows missing");
            assert_eq!(follower.r#gen(), leader.r#gen());
        }
        Err(e) => {
            // Refused: the follower must still hold exactly what it had before.
            assert_eq!(count(&mut follower, "ev | count"), 1.0, "failed catch-up changed the follower: {e}");
        }
    }
}
