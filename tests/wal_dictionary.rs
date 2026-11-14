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
}
