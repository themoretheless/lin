//! Read-back regression for row deletion. Positions key the scalar indexes, FTS
//! postings and the id/uri maps, so a delete that quietly skips any of them still
//! reports success — the assertions below re-read through every structure to make
//! an unapplied or off-by-one deletion impossible to miss.

fn hash_of(db: &mut lin::Db, i: usize) -> String {
    let h = db
        .run(&format!(r#"docs | id == "d-{i}" | {{ hash }}"#))
        .unwrap();
    h.rows[0]
        .iter()
        .find_map(|(k, v)| k.contains("hash").then_some(v))
        .and_then(|c| c.text())
        .unwrap()
        .to_string()
}

fn del(db: &mut lin::Db, i: usize) {
    let hv = hash_of(db, i);
    db.run(&format!(r#"delete docs[id == "d-{i}"] cas "{hv}""#))
        .unwrap();
}

fn ids(db: &mut lin::Db, src: &str) -> Vec<String> {
    db.run(src)
        .unwrap()
        .rows
        .iter()
        .map(|r| r["id"].text().unwrap_or("?").to_string())
        .collect()
}

fn seeded(n: usize) -> lin::Db {
    let mut db = lin::Db::empty();
    db.run("index docs [wing]").unwrap();
    for i in 0..n {
        db.run(&format!(
            r#"insert docs {{ id: "d-{i}", uri: "b://{i}", wing: "rag", title: "wal doc{i}", body: "zeta alpha unique{i}" }}"#
        ))
        .unwrap();
    }
    db
}

fn count(db: &mut lin::Db, src: &str) -> i64 {
    db.run(src).unwrap().rows[0]
        .values()
        .next()
        .and_then(|c| c.as_int())
        .unwrap()
}

#[test]
fn single_delete_is_visible_through_every_lookup_path() {
    let mut db = seeded(10_000);
    del(&mut db, 500);

    assert!(
        ids(&mut db, r#"docs | id == "d-500" | { id }"#).is_empty(),
        "point get by id"
    );
    assert!(
        ids(&mut db, r#"docs | uri == "b://500" | { id }"#).is_empty(),
        "point get by uri"
    );
    assert!(
        !ids(&mut db, r#"docs | body ~ "unique500" | { id }"#).contains(&"d-500".to_string()),
        "fts"
    );

    // Neighbours must still resolve to themselves, not to a shifted row.
    for i in [0usize, 1, 499, 501, 502, 9_998, 9_999] {
        let want = format!("d-{i}");
        assert_eq!(
            ids(&mut db, &format!(r#"docs | id == "d-{i}" | {{ id }}"#)),
            vec![want.clone()],
            "id {i} drifted"
        );
        assert!(
            ids(&mut db, &format!(r#"docs | body ~ "unique{i}" | {{ id }}"#)).contains(&want),
            "fts {i} drifted"
        );
    }

    assert_eq!(count(&mut db, "docs | count"), 9_999);
    assert_eq!(
        count(&mut db, r#"docs | wing == "rag" | count"#),
        9_999,
        "indexed scan"
    );
}

#[test]
fn deleted_slot_is_refilled_by_the_last_live_row() {
    // Swap-remove contract: the vacated slot holds the collection's last row, which
    // must stay resolvable through every position-keyed structure from its new place.
    let mut db = seeded(10_000);
    del(&mut db, 3);
    // Unsorted reads follow the rows array, so slot 3 now holds the old tail row.
    assert_eq!(
        ids(&mut db, r#"docs | { id } | take 4"#),
        vec![
            "d-0".to_string(),
            "d-1".into(),
            "d-2".into(),
            "d-9999".into()
        ],
        "the vacated slot takes the last live row"
    );

    assert_eq!(
        ids(&mut db, r#"docs | id == "d-9999" | { id }"#),
        vec!["d-9999".to_string()],
        "moved row by id"
    );
    assert_eq!(
        ids(&mut db, r#"docs | uri == "b://9999" | { id }"#),
        vec!["d-9999".to_string()],
        "moved row by uri"
    );
    assert!(
        ids(&mut db, r#"docs | body ~ "unique9999" | { id }"#).contains(&"d-9999".to_string()),
        "moved row's postings follow it"
    );
    assert_eq!(count(&mut db, r#"docs | wing == "rag" | count"#), 9_999);
    assert!(
        ids(&mut db, r#"docs | body ~ "unique3" | { id }"#)
            .iter()
            .all(|id| id != "d-3" && id != "d-9999"),
        "deleted row left its postings, and its replacement is not posted under them"
    );
    assert_eq!(
        ids(&mut db, r#"docs | id == "d-9999" | { id }"#).len(),
        1,
        "moved row must not still be posted at its old slot"
    );
}

#[test]
fn repeated_deletes_do_not_drift() {
    let mut db = seeded(2_000);
    let gone: Vec<usize> = (0..50).map(|i| i * 37).collect();
    for &i in gone.iter().rev() {
        del(&mut db, i);
    }
    assert_eq!(count(&mut db, "docs | count"), 2_000 - gone.len() as i64);

    for &i in &gone {
        assert!(
            ids(&mut db, &format!(r#"docs | id == "d-{i}" | {{ id }}"#)).is_empty(),
            "resurrected {i}"
        );
    }
    // Survivors: every third row, none of them a multiple of 37 below 1850.
    for i in [1usize, 2, 998, 1_000, 1_500, 1_849, 1_999] {
        assert_eq!(i % 37, (i % 37), "guard");
        assert!(!gone.contains(&i), "probe row {i} was deleted by design");
        let want = format!("d-{i}");
        assert_eq!(
            ids(&mut db, &format!(r#"docs | id == "d-{i}" | {{ id }}"#)),
            vec![want.clone()],
            "id {i} drifted"
        );
        assert!(
            ids(&mut db, &format!(r#"docs | body ~ "unique{i}" | {{ id }}"#)).contains(&want),
            "fts {i} drifted"
        );
        assert_eq!(
            ids(&mut db, &format!(r#"docs | uri == "b://{i}" | {{ id }}"#)),
            vec![want],
            "uri {i} drifted"
        );
    }
}

#[test]
fn many_at_once_survive_reopen() {
    let dir = std::env::temp_dir().join(format!("lin-del-shift-{}", std::process::id()));
    let path = dir.join("store.lin");
    std::fs::create_dir_all(&dir).unwrap();
    let mut persisted = lin::Db::open(&path).unwrap();
    persisted.run("index docs [wing]").unwrap();
    for i in 0..1_000 {
        persisted.run(&format!(
            r#"insert docs {{ id: "d-{i}", uri: "b://{i}", wing: "rag", title: "wal doc{i}", body: "zeta alpha unique{i}" }}"#
        )).unwrap();
    }
    for i in (0..150).rev() {
        let hv = hash_of(&mut persisted, i);
        persisted
            .run(&format!(r#"delete docs[id == "d-{i}"] cas "{hv}""#))
            .unwrap();
    }
    assert_eq!(count(&mut persisted, "docs | count"), 850);
    persisted.checkpoint().ok();
    drop(persisted);

    let mut reopened = lin::Db::open(&path).unwrap();
    assert_eq!(
        count(&mut reopened, "docs | count"),
        850,
        "count after reopen"
    );
    assert!(
        ids(&mut reopened, r#"docs | id == "d-0" | { id }"#).is_empty(),
        "deleted row came back after reopen"
    );
    assert_eq!(
        ids(&mut reopened, r#"docs | id == "d-500" | { id }"#),
        vec!["d-500".to_string()]
    );
    assert!(
        ids(&mut reopened, r#"docs | body ~ "unique0" | { id }"#).is_empty(),
        "fts resurrected a deleted doc"
    );
    std::fs::remove_dir_all(&dir).ok();
}

// Pending posting deltas fold into the base list once a term has more of them
// than `FOLD_AFTER`, and checkpoint folds whatever is still pending. Both
// transitions must leave the term's live set exactly `(base ∪ adds) \ dels`:
// corpus-wide tokens cross the fold during this run while the per-row `uniqueN`
// tokens stay pending, so one test covers the folded and unfolded halves.
#[test]
fn posting_deltas_fold_without_losing_or_resurrecting_rows() {
    let mut db = seeded(10_000);
    let gone: Vec<usize> = (0..900).collect();
    for &i in gone.iter().rev() {
        del(&mut db, i);
    }
    assert_eq!(count(&mut db, "docs | count"), 10_000 - 900);

    for i in [0usize, 1, 255, 256, 899] {
        assert!(
            ids(&mut db, &format!(r#"docs | id == "d-{i}" | {{ id }}"#)).is_empty(),
            "point get resurrected {i}"
        );
        let hits = ids(
            &mut db,
            &format!(r#"docs | search lex "unique{i}" | {{ id }} | take all"#),
        );
        assert!(!hits.contains(&format!("d-{i}")), "fts resurrected {i}");
    }

    // Shared tokens must hold exactly the survivors.
    let mut want: Vec<String> = (900..10_000).map(|i| format!("d-{i}")).collect();
    want.sort();
    for tok in ["zeta", "alpha", "wal"] {
        let mut got = ids(
            &mut db,
            &format!(r#"docs | search lex "{tok}" | {{ id }} | take all"#),
        );
        got.sort();
        assert_eq!(got, want, "shared token {tok} drifted");
    }

    let dir = std::env::temp_dir().join(format!("lin-del-fold-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("store.lin");
    let mut persisted = lin::Db::open(&path).unwrap();
    persisted.run("index docs [wing]").ok();
    for i in 0..10_000 {
        persisted.run(&format!(
            r#"insert docs {{ id: "d-{i}", uri: "b://{i}", wing: "rag", title: "wal doc{i}", body: "zeta alpha unique{i}" }}"#
        )).unwrap();
    }
    for i in (0..900).rev() {
        let hv = hash_of(&mut persisted, i);
        persisted
            .run(&format!(r#"delete docs[id == "d-{i}"] cas "{hv}""#))
            .unwrap();
    }
    persisted.checkpoint().unwrap();
    drop(persisted);

    let mut reopened = lin::Db::open(&path).unwrap();
    assert_eq!(count(&mut reopened, "docs | count"), 10_000 - 900);
    let mut got = ids(
        &mut reopened,
        r#"docs | search lex "zeta" | { id } | take all"#,
    );
    got.sort();
    assert_eq!(got, want, "a folded term did not survive the checkpoint");
    let stray = ids(
        &mut reopened,
        r#"docs | search lex "unique0" | { id } | take all"#,
    );
    assert!(stray.is_empty(), "a pending delete was folded away");
    std::fs::remove_dir_all(&dir).ok();
}
