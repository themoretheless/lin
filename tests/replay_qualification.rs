//! Recovery qualification: a database recovered from disk must answer exactly like one
//! that ran the same statements and never stopped. Three crash images are recovered —
//! log only (no snapshot was ever written), snapshot only (empty log), and an older
//! snapshot with newer log records — and each is then written to again and reopened
//! repeatedly. The queries go through every derived structure a replay has to rebuild:
//! the id and uri maps, a secondary index, full-text postings, embeddings, graph edges
//! and facts, plus the id counter that the next insert draws from.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use lin::Db;

static SEQ: AtomicU64 = AtomicU64::new(0);

fn tmp() -> PathBuf {
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("lin-replay-qual-{}-{n}", std::process::id()));
    let _ = fs::remove_dir_all(&p);
    fs::create_dir_all(&p).unwrap();
    p
}

/// Copy of a live data directory as a crash would leave it: every file, minus the lock.
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

fn phase_a() -> Vec<String> {
    let mut out = vec![
        "col acct { id: text, org: text, n: i64 }".to_string(),
        "index acct [org]".to_string(),
    ];
    for i in 0..40 {
        let topic = ["wal shipping", "vector search", "graph walk", "cold tier"][i % 4];
        let body = format!("document {i} talks about {topic} and replay number {}", i * 7);
        let edge = if i == 0 {
            String::new()
        } else {
            format!(r#" with edge wikilink -> page "raw://q/{}""#, (i - 1) / 2)
        };
        out.push(format!(
            r#"insert docs {{ uri: "raw://q/{i}", title: "Doc {i} {topic}", layer: "{}", body: "{body}" }}{edge}"#,
            if i % 3 == 0 { "wiki" } else { "raw" }
        ));
    }
    let mut acct = String::from("insert acct [");
    for i in 0..60 {
        if i > 0 {
            acct.push(',');
        }
        acct.push_str(&format!(r#"{{ id: "a{i}", org: "o{}", n: {i} }}"#, i % 6));
    }
    acct.push(']');
    out.push(acct);
    for i in 0..8 {
        out.push(format!(r#"append facts {{ s: "lin", p: tagged, o: "t{i}" }}"#));
    }
    out
}

fn phase_b() -> Vec<String> {
    vec![
        r#"update docs[uri == "raw://q/3"] cas each { title: "Doc 3 rewritten shipping" }"#.to_string(),
        r#"update docs[uri == "raw://q/8"] cas each { title: "Doc 8 now about shipping only" }"#.to_string(),
        r#"delete docs[uri == "raw://q/11"] cas each"#.to_string(),
        r#"delete docs[uri == "raw://q/39"] cas each"#.to_string(),
        r#"update acct[org == "o2"] cas each { n: 1000 }"#.to_string(),
        r#"update acct[id == "a7"] cas each { org: "o9" }"#.to_string(),
        r#"delete acct[id == "a13"] cas each"#.to_string(),
        r#"delete facts[s == "lin" and p == tagged and o == "t3"]"#.to_string(),
        r#"insert docs { uri: "raw://q/late", title: "Late wal shipping note", layer: "raw", body: "arrives after the first batch" } with edge wikilink -> page "raw://q/2""#.to_string(),
        r#"insert acct [{ id: "b1", org: "o2", n: 5 }, { id: "b2", org: "o9", n: 6 }]"#.to_string(),
    ]
}

fn phase_c() -> Vec<String> {
    vec![
        r#"insert docs { uri: "raw://q/after", title: "After recovery shipping", layer: "wiki", body: "written to the recovered database" }"#.to_string(),
        r#"insert acct { id: "c1", org: "o1", n: 77 }"#.to_string(),
        r#"append facts { s: "lin", p: tagged, o: "after" }"#.to_string(),
        r#"delete docs[uri == "raw://q/20"] cas each"#.to_string(),
    ]
}

fn run_all(db: &mut Db, stmts: &[String]) {
    for stmt in stmts {
        db.run(stmt).unwrap_or_else(|e| panic!("{stmt}: {e}"));
    }
}

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

/// Every answer the comparison looks at, keyed by its query. Row order is kept where the
/// query defines it (search ranking, explicit sort) and normalized otherwise.
fn answers(db: &mut Db) -> Vec<(String, Vec<String>)> {
    let ordered = [
        r#"docs | search lex "shipping" | { uri, title } | take all"#,
        r#"docs | search lex "replay" | { uri } | take 7"#,
        r#"docs | search vec "wal shipping" | { uri } | take 5"#,
        r#"docs | sort uri | { id, uri } | take all"#,
    ];
    let unordered = [
        r#"docs | { id, uri, title, layer, body } | take all"#,
        r#"docs | uri == "raw://q/3" | { id, title }"#,
        r#"docs | uri == "raw://q/11" | count"#,
        r#"docs | uri == "raw://q/late" | { id, title, body }"#,
        r#"docs | layer == "wiki" | { uri } | take all"#,
        r#"docs | count by layer | take all"#,
        r#"docs | uri == "raw://q/1" | hop wikilink | { uri } | take all"#,
        r#"docs | uri == "raw://q/late" | hop wikilink | { uri } | take all"#,
        r#"acct | org == "o2" | { id, n } | take all"#,
        r#"acct | org == "o9" | { id, n } | take all"#,
        r#"acct | id == "a13" | count"#,
        r#"acct | count by org | take all"#,
        r#"acct | sum n by org | take all"#,
        r#"facts | s == "lin" | take all"#,
    ];
    let mut out = Vec::new();
    for q in ordered {
        let rows = db.run(q).unwrap_or_else(|e| panic!("{q}: {e}"));
        out.push((q.to_string(), rows.rows.iter().map(row_key).collect()));
    }
    for q in unordered {
        let rows = db.run(q).unwrap_or_else(|e| panic!("{q}: {e}"));
        let mut keys: Vec<String> = rows.rows.iter().map(row_key).collect();
        keys.sort();
        out.push((q.to_string(), keys));
    }
    // Lookups by the ids the uri lookups returned: the id map must agree with the uri map.
    let ids: Vec<String> = db
        .run(r#"docs | sort uri | { id } | take 6"#)
        .unwrap()
        .rows
        .iter()
        .filter_map(|r| r.get("id").and_then(|c| c.text()).map(str::to_string))
        .collect();
    for id in ids {
        let q = format!(r#"docs | id == "{id}" | {{ uri, title }}"#);
        let rows = db.run(&q).unwrap();
        out.push((q, rows.rows.iter().map(row_key).collect()));
    }
    out
}

fn assert_same(got: &[(String, Vec<String>)], want: &[(String, Vec<String>)], what: &str) {
    assert_eq!(got.len(), want.len(), "{what}: number of answers");
    for ((gq, g), (wq, w)) in got.iter().zip(want) {
        assert_eq!(gq, wq, "{what}: query order");
        assert_eq!(g, w, "{what}: {gq}");
    }
}

#[test]
fn recovered_database_matches_one_that_never_stopped() {
    // The reference never closes and never checkpoints.
    let reference_dir = tmp();
    let mut reference = Db::open(&reference_dir).unwrap();
    run_all(&mut reference, &phase_a());
    run_all(&mut reference, &phase_b());
    let want_ab = answers(&mut reference);
    assert!(
        want_ab.iter().all(|(q, rows)| !rows.is_empty() || q.contains("| count")),
        "a comparison query returns nothing: {:?}",
        want_ab.iter().filter(|(_, r)| r.is_empty()).map(|(q, _)| q).collect::<Vec<_>>()
    );

    // Crash images of a second database that ran the same statements.
    let source_dir = tmp();
    let mut images: Vec<(&str, PathBuf)> = Vec::new();
    {
        let mut source = Db::open(&source_dir).unwrap();
        run_all(&mut source, &phase_a());
        source.checkpoint().unwrap();
        run_all(&mut source, &phase_b());
        images.push(("older snapshot, newer log", crash_image(&source_dir)));
        source.checkpoint().unwrap();
        images.push(("snapshot only", crash_image(&source_dir)));
    }
    {
        let log_only_dir = tmp();
        let mut source = Db::open(&log_only_dir).unwrap();
        run_all(&mut source, &phase_a());
        run_all(&mut source, &phase_b());
        images.push(("log only", crash_image(&log_only_dir)));
    }

    run_all(&mut reference, &phase_c());
    let want_abc = answers(&mut reference);

    for (what, dir) in &images {
        let mut db = Db::open(dir).unwrap_or_else(|e| panic!("{what}: open: {e}"));
        assert_same(&answers(&mut db), &want_ab, what);

        // Writes after recovery draw ids from the recovered counter and land in the same
        // derived structures.
        run_all(&mut db, &phase_c());
        assert_same(&answers(&mut db), &want_abc, &format!("{what}, after new writes"));

        // Crash again (log replay over whatever the image had), then clean reopens.
        let again = crash_image(dir);
        let mut replayed = Db::open(&again).unwrap();
        assert_same(&answers(&mut replayed), &want_abc, &format!("{what}, second crash"));
        drop(replayed);
        db.close().unwrap();
        drop(db);
        for round in 0..3 {
            let mut db = Db::open(dir).unwrap();
            assert_same(&answers(&mut db), &want_abc, &format!("{what}, reopen {round}"));
            db.close().unwrap();
        }
        // A read-only handle sees the same data.
        let reader = Db::open_read(dir).unwrap();
        let rows = reader.run(r#"docs | sort uri | { id, uri } | take all"#).unwrap();
        let got: Vec<String> = rows.rows.iter().map(row_key).collect();
        let want = &want_abc.iter().find(|(q, _)| q.contains("sort uri | { id, uri }")).unwrap().1;
        assert_eq!(&got, want, "{what}: read-only handle");
    }
}

fn fact_objects(db: &mut Db, subject: &str) -> Vec<String> {
    let mut out: Vec<String> = db
        .run(&format!(r#"facts | s == "{subject}" | take all"#))
        .unwrap()
        .rows
        .iter()
        .map(|r| r.get("o").and_then(|c| c.text()).unwrap_or("?").to_string())
        .collect();
    out.sort();
    out
}

/// A statement that appends many facts at once returned `done = n` while the next query
/// still saw none of them: fact rows were materialized lazily and the read path never did
/// it. Every size must be visible to the very next statement.
#[test]
fn appended_facts_are_visible_to_the_next_query() {
    for durable in [false, true] {
        let dir = tmp();
        let mut db = if durable { Db::open(&dir).unwrap() } else { Db::empty() };
        let mut total = 0usize;
        for (batch, n) in [1usize, 2, 127, 128, 129, 200, 1000].into_iter().enumerate() {
            let mut src = String::from("append facts [");
            for i in 0..n {
                if i > 0 {
                    src.push(',');
                }
                src.push_str(&format!(r#"{{ s: "bulk", p: tagged, o: "b{batch}-{i}" }}"#));
            }
            src.push(']');
            assert_eq!(db.run(&src).unwrap().done.n, n, "durable={durable}, batch of {n}");
            total += n;
            assert_eq!(fact_objects(&mut db, "bulk").len(), total, "durable={durable}, after {n}");
            let count = db.run(r#"facts | s == "bulk" | count"#).unwrap().rows.clone();
            assert_eq!(
                count[0].get("hits").and_then(|c| c.as_f64()),
                Some(total as f64),
                "durable={durable}, count after {n}"
            );
            // Appending the same facts again changes nothing.
            db.run(&src).unwrap();
            assert_eq!(fact_objects(&mut db, "bulk").len(), total);
        }
        // A delete finds rows from a large append, and stays deleted.
        db.run(r#"delete facts[s == "bulk" and p == tagged and o == "b6-999"]"#).unwrap();
        assert_eq!(fact_objects(&mut db, "bulk").len(), total - 1);
    }
}

/// Facts appended and deleted in the log (never checkpointed) are replayed as they were:
/// the appended ones are visible right after open, and a deleted one does not come back
/// after the next write.
#[test]
fn replayed_fact_appends_and_deletes_hold_after_reopen() {
    for checkpoint_first in [false, true] {
        let dir = tmp();
        let mut db = Db::open(&dir).unwrap();
        for i in 0..4 {
            db.run(&format!(r#"append facts {{ s: "lin", p: tagged, o: "t{i}" }}"#)).unwrap();
        }
        if checkpoint_first {
            db.checkpoint().unwrap();
        }
        db.run(r#"delete facts[s == "lin" and p == tagged and o == "t3"]"#).unwrap();
        db.run(r#"append facts { s: "lin", p: tagged, o: "t4" }"#).unwrap();

        let mut recovered = Db::open(crash_image(&dir)).unwrap();
        let what = format!("checkpoint_first={checkpoint_first}");
        assert_eq!(fact_objects(&mut recovered, "lin"), ["t0", "t1", "t2", "t4"], "{what}: after open");
        recovered.run(r#"append facts { s: "lin", p: tagged, o: "t5" }"#).unwrap();
        assert_eq!(
            fact_objects(&mut recovered, "lin"),
            ["t0", "t1", "t2", "t4", "t5"],
            "{what}: after a write"
        );
        // The deleted fact can be appended again as a new one.
        recovered.run(r#"append facts { s: "lin", p: tagged, o: "t3" }"#).unwrap();
        assert_eq!(fact_objects(&mut recovered, "lin").len(), 6, "{what}");
    }
}

/// A schema statement followed by a crash before any checkpoint left a database that
/// refused to open (`catalog_hash mismatch`): the `head` file still described the catalog
/// from before the statement, while the log, which is authoritative, had moved on.
#[test]
fn schema_changes_in_the_log_do_not_block_reopen() {
    let cases: [&[&str]; 5] = [
        &["col acct { id: text, org: text, n: i64 }"],
        &["col acct { id: text, org: text, n: i64 }", r#"insert acct { id: "a", org: "o", n: 1 }"#],
        &["col acct { id: text, org: text, n: i64 }", "index acct [org]"],
        &[
            "col acct { id: text, org: text, n: i64 }",
            "index acct [org]",
            r#"insert acct { id: "a", org: "o", n: 1 }"#,
            "col other { id: text, v: f64 }",
            r#"insert other { id: "x", v: 1.5 }"#,
        ],
        &[r#"insert docs { uri: "raw://plain", title: "plain", layer: "raw" }"#],
    ];
    for (case, stmts) in cases.iter().enumerate() {
        // Crash right after the statements, and again after each later open: the head
        // file is rewritten on open, so the second crash starts from a different head.
        let dir = tmp();
        let mut db = Db::open(&dir).unwrap();
        for stmt in *stmts {
            db.run(stmt).unwrap();
        }
        let has_acct = stmts.iter().any(|s| s.starts_with("col acct"));
        let has_row = stmts.iter().any(|s| s.starts_with("insert acct"));
        let mut image = crash_image(&dir);
        for round in 0..3 {
            let mut recovered = Db::open(&image)
                .unwrap_or_else(|e| panic!("case {case}, crash {round}: open: {e}"));
            if has_acct {
                let rows = recovered.run("acct | take all").unwrap().rows.len();
                assert_eq!(rows, usize::from(has_row) + round, "case {case}, crash {round}");
                // More schema and data on top of the recovered state, then crash again.
                recovered.run(&format!("col extra{round} {{ id: text }}")).unwrap();
                recovered
                    .run(&format!(r#"insert acct {{ id: "r{round}", org: "o", n: {round} }}"#))
                    .unwrap();
            }
            image = crash_image(&image);
        }
        // The original still closes and reopens cleanly.
        db.close().unwrap();
        drop(db);
        Db::open(&dir).unwrap_or_else(|e| panic!("case {case}: clean reopen: {e}"));
    }
}

fn log_frames(dir: &Path) -> Vec<Vec<u8>> {
    let log = fs::read(dir.join(lin::persist::LOG_NAME)).unwrap();
    let mut out = Vec::new();
    lin::persist::for_each_wal_frame_raw(&log, |raw, _| {
        out.push(raw.to_vec());
        Ok(())
    })
    .unwrap();
    out
}

/// Replay on open used to apply whatever frames the log held, in file order: a frame
/// present twice inserted its rows twice (two documents with one `uri`), a frame out of
/// order moved the generation backwards, and a missing frame was skipped silently. A
/// repeated frame is now skipped, and a log with a hole or frames out of order is
/// refused without being modified.
#[test]
fn log_replay_skips_repeated_frames_and_refuses_holes() {
    let src = tmp();
    let mut db = Db::open(&src).unwrap();
    for i in 0..4 {
        db.run(&format!(r#"insert docs {{ uri: "raw://r/{i}", title: "T{i}", layer: "raw" }}"#))
            .unwrap();
    }
    let frames = log_frames(&src);
    assert_eq!(frames.len(), 4);
    let uris = |db: &mut Db| -> Vec<String> {
        let mut out: Vec<String> = db
            .run("docs | { uri } | take all")
            .unwrap()
            .rows
            .iter()
            .map(|r| r.get("uri").and_then(|c| c.text()).unwrap_or("?").to_string())
            .collect();
        out.sort();
        out
    };
    let all: Vec<String> = (0..4).map(|i| format!("raw://r/{i}")).collect();
    let with_log = |order: &[usize]| -> PathBuf {
        let dir = crash_image(&src);
        let bytes: Vec<u8> = order.iter().flat_map(|&i| frames[i].clone()).collect();
        fs::write(dir.join(lin::persist::LOG_NAME), bytes).unwrap();
        dir
    };

    // Repeated frames, adjacent or not, are applied once.
    for order in [&[0, 1, 2, 3, 0][..], &[0, 1, 1, 2, 3], &[0, 1, 2, 3, 2, 3], &[0, 0, 0, 1, 2, 3]] {
        let dir = with_log(order);
        let mut db = Db::open(&dir).unwrap_or_else(|e| panic!("{order:?}: {e}"));
        assert_eq!(uris(&mut db), all, "{order:?}");
        assert_eq!(db.r#gen(), 4, "{order:?}");
        // Writes continue from the right generation, and the result survives a crash.
        db.run(r#"insert docs { uri: "raw://r/next", title: "next", layer: "raw" }"#).unwrap();
        assert_eq!(db.r#gen(), 5, "{order:?}");
        let mut again = Db::open(crash_image(&dir)).unwrap();
        assert_eq!(uris(&mut again).len(), 5, "{order:?}");
    }

    // A hole or frames out of order: refused, and the log is left as it was.
    for order in [&[0, 2, 3][..], &[0, 2, 1, 3], &[1, 2, 3], &[0, 1, 3]] {
        let dir = with_log(order);
        let before = fs::read(dir.join(lin::persist::LOG_NAME)).unwrap();
        let err = Db::open(&dir).err().unwrap_or_else(|| panic!("{order:?}: opened"));
        assert!(err.to_string().contains("generation gap"), "{order:?}: {err}");
        assert_eq!(fs::read(dir.join(lin::persist::LOG_NAME)).unwrap(), before, "{order:?}");
        assert!(Db::open_read(&dir).is_err(), "{order:?}: read-only open");
    }
}
