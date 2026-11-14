//! Text values around the inline-storage boundary: strings up to 22 bytes are stored
//! inline, longer ones shared. A value must compare, hash, index, persist and come back
//! the same whichever form it takes — including multi-byte characters straddling the
//! 22-byte boundary, the empty string, and an embedded NUL — in memory, after log
//! replay, after a checkpoint, and on the cold tier.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use lin::query::pred;
use lin::{Cell, Db, Queryable, RecordBatch};

static SEQ: AtomicU64 = AtomicU64::new(0);

fn tmp() -> PathBuf {
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("lin-text-key-{}-{n}", std::process::id()));
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

/// Strings of every length from 0 to 30 bytes built from 1-, 2-, 3- and 4-byte
/// characters, so that some end exactly at the 22-byte boundary and others would split
/// a character there; plus NUL, control characters and combining marks.
fn values() -> Vec<String> {
    let mut out = vec![String::new()];
    for ch in ["a", "Ж", "日", "🦀"] {
        let mut s = String::new();
        while s.len() <= 30 {
            s.push_str(ch);
            out.push(s.clone());
        }
    }
    for prefix in 18..=23 {
        for ch in ["Ж", "日", "🦀"] {
            out.push(format!("{}{ch}", "x".repeat(prefix)));
            out.push(format!("{ch}{}", "x".repeat(prefix)));
        }
    }
    out.extend(
        [
            "nul\0inside",
            "\0",
            "trailing nul\0",
            "tab\tand\nnewline",
            "e\u{301}",
            "\u{e9}",
            "quote\"back\\slash",
            "exactly-twenty-two-by!",
            "exactly-twenty-three-b!",
        ]
        .map(String::from),
    );
    out.sort();
    out.dedup();
    out
}

fn load(db: &mut Db) {
    db.run("col t { id: text, v: text, n: i64 }").unwrap();
    db.run("index t [v]").unwrap();
    let mut batch = RecordBatch::with_capacity(vec!["id".to_string(), "v".into(), "n".into()], 0);
    for (i, v) in values().iter().enumerate() {
        // Every value twice, under different ids: duplicates in the index.
        for copy in 0..2 {
            batch.push_row(&[
                Cell::from_text(&format!("r{i}-{copy}")),
                Cell::from_text(v),
                Cell::Int(i as i64),
            ]);
        }
    }
    db.insert_batch("t", &batch).unwrap();
}

/// For every value: how many rows equal it (through the index), and which value each
/// row holds, byte for byte.
fn check(db: &mut Db, what: &str) {
    for (i, v) in values().iter().enumerate() {
        let hits = Queryable::from("t")
            .filter(pred::eq("v", v.as_str()))
            .run(db)
            .unwrap_or_else(|e| panic!("{what}: {v:?}: {e}"))
            .rows
            .clone();
        assert_eq!(hits.len(), 2, "{what}: {v:?} ({} bytes)", v.len());
        for row in &hits {
            assert_eq!(row.get("v").and_then(|c| c.text()), Some(v.as_str()), "{what}");
            assert_eq!(row.get("n").and_then(|c| c.as_f64()), Some(i as f64), "{what}: {v:?}");
        }
    }
    // The empty string is a value, not a missing one.
    let null = Queryable::from("t").filter(pred::is_null("v")).run(db).unwrap().rows.len();
    assert_eq!(null, 0, "{what}: empty strings read back as null");
}

#[test]
fn text_values_round_trip_across_the_inline_boundary() {
    let mut mem = Db::empty();
    load(&mut mem);
    check(&mut mem, "in memory");

    for cold in [false, true] {
        let dir = tmp();
        let opts = || lin::OpenOpts {
            sync: lin::SyncMode::Normal,
            cold,
        };
        let mut db = Db::open_with(&dir, opts()).unwrap();
        load(&mut db);
        check(&mut db, &format!("cold={cold}, live"));
        let mut replayed = Db::open_with(crash_image(&dir), opts()).unwrap();
        check(&mut replayed, &format!("cold={cold}, log replay"));
        drop(replayed);
        db.checkpoint().unwrap();
        db.close().unwrap();
        drop(db);
        let mut reopened = Db::open_with(&dir, opts()).unwrap();
        if cold {
            assert!(reopened.store.is_disk_backed("t"), "t is not on the disk tier");
        }
        check(&mut reopened, &format!("cold={cold}, after checkpoint"));
    }
}
