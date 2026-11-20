//! End-to-end check of the opt-in dictionary vector WAL codec (codec 5): with the writer
//! on, document inserts are logged under it, and everything that reads the log — crash
//! recovery, a checkpoint, a follower, a later process with the writer off — sees the
//! same data as a database that logged the same statements under the default codecs.
//!
//! The switch is process-wide, so this file holds a single test.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use lin::Db;

static SEQ: AtomicU64 = AtomicU64::new(0);

fn tmp() -> PathBuf {
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("lin-wal-dict-{}-{n}", std::process::id()));
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

fn statements() -> Vec<String> {
    let mut out = Vec::new();
    // One large batch (the bulk encoders) and a few single inserts.
    let mut bulk = String::from("insert docs [");
    for i in 0..300 {
        if i > 0 {
            bulk.push(',');
        }
        let topic = ["wal shipping", "vector search", "graph walk", "cold tier"][i % 4];
        bulk.push_str(&format!(
            r#"{{ uri: "raw://d/{i}", title: "Doc {i} {topic}", layer: "raw", body: "body {i} about {topic} number {}" }}"#,
            i * 13
        ));
    }
    bulk.push(']');
    out.push(bulk);
    for i in 0..5 {
        out.push(format!(
            r#"insert docs {{ uri: "raw://s/{i}", title: "Single {i} dictionary codec", layer: "wiki" }}"#
        ));
    }
    out.push(r#"delete docs[uri == "raw://d/7"] cas each"#.to_string());
    out.push(r#"update docs[uri == "raw://d/8"] cas each { title: "Doc 8 retitled shipping" }"#.to_string());
    out
}

/// Rows as text, vectors by their exact bits.
fn answers(db: &mut Db) -> Vec<(String, Vec<String>)> {
    let queries = [
        r#"docs | sort uri | { id, uri, title, layer, embedding } | take all"#,
        r#"docs | search vec "wal shipping" | { uri } | take 8"#,
        r#"docs | search lex "shipping" | { uri } | take all"#,
        r#"docs | uri == "raw://d/7" | count"#,
    ];
    queries
        .iter()
        .map(|q| {
            let rows = db.run(q).unwrap_or_else(|e| panic!("{q}: {e}"));
            let keys = rows
                .rows
                .iter()
                .map(|r| {
                    r.iter()
                        .map(|(k, c)| match c {
                            lin::Cell::Vec(v) => format!(
                                "{k}=[{}]",
                                v.iter().map(|x| format!("{:08x}", x.to_bits())).collect::<String>()
                            ),
                            other => match other.text() {
                                Some(t) => format!("{k}={t}"),
                                None => format!("{k}={other:?}"),
                            },
                        })
                        .collect::<Vec<_>>()
                        .join(";")
                })
                .collect();
            (q.to_string(), keys)
        })
        .collect()
}

fn assert_same(got: &[(String, Vec<String>)], want: &[(String, Vec<String>)], what: &str) {
    for ((q, g), (_, w)) in got.iter().zip(want) {
        assert_eq!(g.len(), w.len(), "{what}: row count for {q}");
        assert!(g == w, "{what}: rows differ for {q}");
    }
}

/// Codec byte of every frame in a data directory's log.
fn log_codecs(dir: &Path) -> Vec<u8> {
    let bytes = fs::read(dir.join(lin::persist::LOG_NAME)).unwrap();
    let mut codecs = Vec::new();
    lin::persist::for_each_wal_frame_raw(&bytes, |raw, _| {
        codecs.push(raw[8]);
        Ok(())
    })
    .unwrap();
    codecs
}

#[test]
fn dictionary_codec_is_written_only_when_enabled_and_read_everywhere() {
    assert!(!lin::persist::wal_dictionary_writer(), "the writer must be off by default");

    // Reference: the default codecs.
    let reference_dir = tmp();
    let mut reference = Db::open(&reference_dir).unwrap();
    for stmt in statements() {
        reference.run(&stmt).unwrap();
    }
    let want = answers(&mut reference);
    assert_eq!(want[0].1.len(), 304);
    let default_codecs = log_codecs(&reference_dir);
    assert!(!default_codecs.contains(&5), "default writer produced codec 5: {default_codecs:?}");
    let default_log_len = fs::metadata(reference_dir.join(lin::persist::LOG_NAME)).unwrap().len();

    // Same statements with the dictionary writer on.
    assert!(!lin::persist::set_wal_dictionary_writer(true));
    let dir = tmp();
    let mut db = Db::open(&dir).unwrap();
    for stmt in statements() {
        db.run(&stmt).unwrap();
    }
    assert_same(&answers(&mut db), &want, "live, writer on");
    let codecs = log_codecs(&dir);
    assert!(codecs.contains(&5), "no frame used the dictionary codec: {codecs:?}");
    let dict_log_len = fs::metadata(dir.join(lin::persist::LOG_NAME)).unwrap().len();
    assert!(
        dict_log_len < default_log_len,
        "dictionary log is not smaller: {dict_log_len} vs {default_log_len}"
    );

    // A follower fed the exported frames.
    let frames = db.export_wal_since(0).unwrap();
    let mut follower = Db::empty();
    follower.apply_wal(&frames).unwrap();
    assert_same(&answers(&mut follower), &want, "follower");

    // Crash recovery straight from the log, with the writer on and then off.
    let image = crash_image(&dir);
    let mut recovered = Db::open(&image).unwrap();
    assert_same(&answers(&mut recovered), &want, "crash recovery, writer on");
    drop(recovered);
    assert!(lin::persist::set_wal_dictionary_writer(false));
    let image_off = crash_image(&dir);
    let mut recovered = Db::open(&image_off).unwrap();
    assert_same(&answers(&mut recovered), &want, "crash recovery, writer off");

    // New writes with the writer off go back to the default codecs, in the same log.
    let extra = r#"insert docs { uri: "raw://late", title: "Late default codec", layer: "raw" }"#;
    recovered.run(extra).unwrap();
    reference.run(extra).unwrap();
    let want_late = answers(&mut reference);
    assert_same(&answers(&mut recovered), &want_late, "after a write with the writer off");
    let mixed = log_codecs(&image_off);
    assert!(mixed.contains(&5) && mixed.last() != Some(&5), "{mixed:?}");
    let again = crash_image(&image_off);
    assert_same(&answers(&mut Db::open(&again).unwrap()), &want_late, "mixed-codec log replay");

    // Checkpoint and clean reopen.
    recovered.checkpoint().unwrap();
    recovered.close().unwrap();
    drop(recovered);
    assert_same(&answers(&mut Db::open(&image_off).unwrap()), &want_late, "after checkpoint");
    db.close().unwrap();
    drop(db);
    assert_same(&answers(&mut Db::open(&dir).unwrap()), &want, "original after clean close");

    // Per-instance OpenOpts::with_wal_dictionary(true) isolation + mid-frame crash truncation
    // fault injection + unknown future codec rejection + rollback compaction.
    assert!(
        !lin::persist::wal_dictionary_writer(),
        "process-wide dictionary flag must still be false"
    );
    let inst_on_dir = tmp();
    let inst_off_dir = tmp();
    let mut db_on = Db::open_with(
        &inst_on_dir,
        lin::OpenOpts::default().with_wal_dictionary(true),
    )
    .unwrap();
    let mut db_off = Db::open_with(&inst_off_dir, lin::OpenOpts::default()).unwrap();
    assert!(db_on.wal_dictionary_enabled());
    assert!(!db_off.wal_dictionary_enabled());
    assert!(
        !lin::persist::wal_dictionary_writer(),
        "per-instance OpenOpts must not flip global flag"
    );

    // Commit 1 (baseline doc that must survive a crash during Commit 2):
    let base_stmt =
        r#"insert docs { uri: "raw://base/0", title: "Committed before crash", layer: "raw" }"#;
    db_on.run(base_stmt).unwrap();
    db_off.run(base_stmt).unwrap();
    let base_log_len = fs::metadata(inst_on_dir.join(lin::persist::LOG_NAME))
        .unwrap()
        .len() as usize;

    // Commit 2 (300-row vector insert encoded with codec 5 on `db_on`, default codec on `db_off`):
    let batch_stmt = &statements()[0];
    db_on.run(batch_stmt).unwrap();
    db_off.run(batch_stmt).unwrap();
    assert!(
        log_codecs(&inst_on_dir).contains(&5),
        "OpenOpts::with_wal_dictionary(true) must write codec 5"
    );
    assert!(
        !log_codecs(&inst_off_dir).contains(&5),
        "sibling Db with default OpenOpts must NOT write codec 5"
    );

    // Fault-injection: simulate process crash mid-write at representative byte offsets across
    // the entire codec-5 frame (envelope header, codec byte, column headers, dictionary table,
    // ordinal payload, and trailing 4-byte CRC).
    let full_wal = fs::read(inst_on_dir.join(lin::persist::LOG_NAME)).unwrap();
    assert!(full_wal.len() > base_log_len + 64);
    let second_frame_len = full_wal.len() - base_log_len;
    let mut cut_points = Vec::new();
    for delta in 1..=24 {
        cut_points.push(base_log_len + delta);
    }
    for step in 0..16 {
        let pos = base_log_len + ((second_frame_len * (step + 1)) / 17).max(1);
        if pos < full_wal.len() {
            cut_points.push(pos);
        }
    }
    for tail in 1..=8 {
        cut_points.push(full_wal.len() - tail);
    }
    cut_points.sort_unstable();
    cut_points.dedup();

    for cut in cut_points {
        let crashed = crash_image(&inst_on_dir);
        fs::write(crashed.join(lin::persist::LOG_NAME), &full_wal[..cut]).unwrap();
        let mut rec = Db::open(&crashed).unwrap_or_else(|e| {
            panic!("mid-frame codec-5 crash at byte {cut}/{} failed to recover: {e}", full_wal.len())
        });
        assert_eq!(
            rec.run("docs | count").unwrap().scalar().unwrap(),
            lin::Cell::Int(1),
            "cut {cut}: only pre-crash commit may be visible"
        );
        // Appending after recovery must succeed cleanly over the truncated tail.
        rec.run(r#"insert docs { uri: "raw://after/0", title: "Post crash", layer: "wiki" }"#)
            .unwrap();
        assert_eq!(
            rec.run("docs | count").unwrap().scalar().unwrap(),
            lin::Cell::Int(2)
        );
        rec.close().unwrap();
        let _ = fs::remove_dir_all(&crashed);
    }

    // Unknown future codec byte (e.g. codec 9 with valid LIN\x06 envelope & CRC32C) must fail closed.
    {
        let future_dir = crash_image(&inst_off_dir);
        let mut bad_wal = fs::read(future_dir.join(lin::persist::LOG_NAME)).unwrap();
        let payload = [9u8, 2, 99, 0, 0, 0, 0, 0, 0, 0]; // codec = 9
        let crc = lin::crc::hardware_crc32c(&payload).to_le_bytes();
        let env_len = (payload.len() + 4) as u32;
        bad_wal.extend_from_slice(b"LIN\x06");
        bad_wal.extend_from_slice(&env_len.to_le_bytes());
        bad_wal.extend_from_slice(&payload);
        bad_wal.extend_from_slice(&crc);
        fs::write(future_dir.join(lin::persist::LOG_NAME), &bad_wal).unwrap();
        assert!(
            Db::open(&future_dir).is_err(),
            "unknown future WAL codec must fail closed instead of silently skipping"
        );
        let _ = fs::remove_dir_all(&future_dir);
    }

    // Rollback migration (`set_wal_dictionary(Some(false))` + checkpoint compacts codec-5 frames).
    db_on.set_wal_dictionary(Some(false));
    db_on.checkpoint().unwrap();
    db_on
        .run(r#"insert docs { uri: "raw://post-rb", title: "Rolled back writer", layer: "raw" }"#)
        .unwrap();
    assert!(
        !log_codecs(&inst_on_dir).contains(&5),
        "after disabling dictionary writer and checkpointing, log must contain zero codec-5 frames"
    );
    db_on.close().unwrap();
    db_off.close().unwrap();
}
