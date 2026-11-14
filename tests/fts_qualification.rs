//! Lexical search: Top-K is the prefix of the full ranking, ties order deterministically,
//! a multi-word query returns the union of its words' matches, a miss returns nothing,
//! deleted documents never come back, Unicode separators split words, short and long
//! titles tokenize alike, and the ranking is identical live, after a checkpoint and
//! reopen, after log replay, and on the cold tier.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use lin::{Cell, Db, RecordBatch};

static SEQ: AtomicU64 = AtomicU64::new(0);

fn tmp() -> PathBuf {
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("lin-fts-qual-{}-{n}", std::process::id()));
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

const WORDS: &[&str] = &["anchor", "beacon", "cipher", "drift", "ember", "fjord", "glyph", "harbor", "isle", "jetty"];

fn load(db: &mut Db) {
    let mut batch = RecordBatch::with_capacity(
        vec!["uri".to_string(), "title".into(), "layer".into(), "body".into()],
        0,
    );
    let mut x: u64 = 0xC0FF_EE12_3456_789B;
    let mut next = move || {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        x
    };
    for i in 0..300 {
        // "anchor" is frequent, "jetty" rare; titles alternate between short (inline)
        // and long (shared) storage.
        let words: Vec<&str> = (0..8)
            .map(|_| {
                let r = next() % 100;
                if r < 30 { WORDS[0] } else if r < 31 { WORDS[9] } else { WORDS[1 + (r as usize % 8)] }
            })
            .collect();
        let title = if i % 2 == 0 {
            words[..2].join(" ")
        } else {
            format!("{} and a long tail that keeps this title out of inline storage", words[..2].join(" "))
        };
        batch.push_row(&[
            Cell::from_text(&format!("raw://f/{i:03}")),
            Cell::from_text(&title),
            Cell::from_text("raw"),
            Cell::from_text(&words[2..].join(" ")),
        ]);
    }
    // Separators other than an ASCII space.
    for (i, body) in ["left\u{000B}vtabword", "left\u{00A0}nbspword", "left\u{3000}ideoword", "left\u{2003}emword"]
        .into_iter()
        .enumerate()
    {
        batch.push_row(&[
            Cell::from_text(&format!("raw://sep/{i}")),
            Cell::from_text("separator case"),
            Cell::from_text("raw"),
            Cell::from_text(body),
        ]);
    }
    db.insert_batch("docs", &batch).unwrap();
    // Rows with as many fields as the others but a different set: `wing` instead of
    // `body`. Their title must be indexed from the title, not from whatever field sits
    // at the title's position in the first row.
    let mut odd = RecordBatch::with_capacity(vec!["uri".to_string(), "title".into(), "layer".into(), "wing".into()], 0);
    for i in 0..3 {
        odd.push_row(&[
            Cell::from_text(&format!("raw://odd/{i}")),
            Cell::from_text("oddtitleword"),
            Cell::from_text("raw"),
            Cell::from_text("wingonlyword"),
        ]);
    }
    // Even rows carry `body` (and get a `hash`), odd rows `wing` and `room` instead: the
    // same number of fields, a different set.
    let mut mixed = RecordBatch::with_capacity(
        vec!["uri".to_string(), "title".into(), "layer".into(), "body".into(), "wing".into(), "room".into()],
        0,
    );
    for i in 0..4 {
        let even = i % 2 == 0;
        mixed.push_row(&[
            Cell::from_text(&format!("raw://mix/{i}")),
            Cell::from_text(if even { "mixtitle" } else { "mixother" }),
            Cell::from_text("raw"),
            if even { Cell::from_text("mixbodyword") } else { Cell::Null },
            if even { Cell::Null } else { Cell::from_text("wingonlyword") },
            if even { Cell::Null } else { Cell::from_text("roomonlyword") },
        ]);
    }
    db.insert_batch("docs", &odd).unwrap();
    db.insert_batch("docs", &mixed).unwrap();
    db.run(r#"delete docs[uri == "raw://f/007" or uri == "raw://f/100"] cas each"#).unwrap();
}

fn uris(db: &mut Db, q: &str) -> Vec<String> {
    db.run(q)
        .unwrap_or_else(|e| panic!("{q}: {e}"))
        .rows
        .iter()
        .map(|r| r.get("uri").and_then(|c| c.text()).unwrap().to_string())
        .collect()
}

fn rankings(db: &mut Db) -> Vec<Vec<String>> {
    let mut out = Vec::new();
    for w in ["anchor", "jetty", "beacon glyph", "anchor jetty", "harbor"] {
        out.push(uris(db, &format!(r#"docs | search lex "{w}" | {{ uri }} | take all"#)));
    }
    out
}

#[test]
fn lexical_search_is_exact_and_stable() {
    let mut db = Db::empty();
    load(&mut db);
    for w in ["anchor", "jetty", "beacon glyph", "anchor jetty"] {
        let full = uris(&mut db, &format!(r#"docs | search lex "{w}" | {{ uri }} | take all"#));
        assert!(!full.is_empty(), "{w}");
        assert!(!full.contains(&"raw://f/007".to_string()) && !full.contains(&"raw://f/100".to_string()), "{w}: deleted document returned");
        for k in [1usize, 3, 10, 50, 1000] {
            let top = uris(&mut db, &format!(r#"docs | search lex "{w}" | {{ uri }} | take {k}"#));
            assert_eq!(top, full[..k.min(full.len())], "{w} | take {k}");
        }
        let page = uris(&mut db, &format!(r#"docs | search lex "{w}" | {{ uri }} | skip 2 | take 5"#));
        assert_eq!(page, full[2.min(full.len())..7.min(full.len())], "{w} | skip 2 | take 5");
        // Same query, same order.
        assert_eq!(uris(&mut db, &format!(r#"docs | search lex "{w}" | {{ uri }} | take all"#)), full);
    }
    // A multi-word query matches documents with any of its words.
    let mut union: Vec<String> = ["beacon", "glyph"]
        .iter()
        .flat_map(|w| uris(&mut db, &format!(r#"docs | search lex "{w}" | {{ uri }} | take all"#)))
        .collect();
    union.sort();
    union.dedup();
    let mut both = uris(&mut db, r#"docs | search lex "beacon glyph" | { uri } | take all"#);
    both.sort();
    assert_eq!(both, union);
    // A miss is empty.
    assert!(uris(&mut db, r#"docs | search lex "zzzznotaword" | { uri } | take all"#).is_empty());
    // Words separated by vertical tab and Unicode spaces are found on their own.
    for (word, uri) in [("vtabword", "raw://sep/0"), ("nbspword", "raw://sep/1"), ("ideoword", "raw://sep/2"), ("emword", "raw://sep/3")] {
        let got = uris(&mut db, &format!(r#"docs | search lex "{word}" | {{ uri }} | take all"#));
        assert_eq!(got, vec![uri.to_string()], "{word}");
    }
    // Rows whose fields differ from the batch's first row are indexed by their own
    // fields: titles are found, a field that is not full-text indexed is not.
    assert_eq!(uris(&mut db, r#"docs | search lex "oddtitleword" | { uri } | take all"#).len(), 3);
    assert!(uris(&mut db, r#"docs | search lex "wingonlyword" | { uri } | take all"#).is_empty());
    assert_eq!(uris(&mut db, r#"docs | search lex "mixbodyword" | { uri } | take all"#).len(), 2);
    assert_eq!(uris(&mut db, r#"docs | search lex "mixother" | { uri } | take all"#).len(), 2);
    // Short and long titles: a title word finds both kinds.
    let hits = uris(&mut db, r#"docs | search lex "anchor" | { uri } | take all"#);
    let short = hits.iter().filter(|u| u.starts_with("raw://f/") && u[8..].parse::<usize>().unwrap() % 2 == 0).count();
    assert!(short > 0 && short < hits.len(), "short {short} of {}", hits.len());
}

#[test]
fn lexical_ranking_survives_checkpoint_replay_and_cold_tier() {
    let mut mem = Db::empty();
    load(&mut mem);
    let want = rankings(&mut mem);
    for cold in [false, true] {
        let dir = tmp();
        let opts = || lin::OpenOpts { sync: lin::SyncMode::Normal, cold };
        let mut db = Db::open_with(&dir, opts()).unwrap();
        load(&mut db);
        assert_eq!(rankings(&mut db), want, "cold={cold}: live");
        let mut replayed = Db::open_with(crash_image(&dir), opts()).unwrap();
        assert_eq!(rankings(&mut replayed), want, "cold={cold}: log replay");
        drop(replayed);
        db.checkpoint().unwrap();
        db.close().unwrap();
        drop(db);
        let mut reopened = Db::open_with(&dir, opts()).unwrap();
        if cold {
            assert!(reopened.store.is_disk_backed("docs"), "docs is not cold");
        }
        assert_eq!(rankings(&mut reopened), want, "cold={cold}: after checkpoint");
        // Changes after reopening are reflected like on the live database.
        for db in [&mut reopened, &mut mem] {
            db.run(r#"delete docs[uri == "raw://f/010"] cas each"#).unwrap();
            db.run(r#"insert docs { uri: "raw://f/new", title: "jetty jetty", layer: "raw", body: "anchor" }"#).unwrap();
        }
        assert_eq!(rankings(&mut reopened), rankings(&mut mem), "cold={cold}: after more writes");
        // Undo on the reference so the next round starts equal.
        mem.run(r#"delete docs[uri == "raw://f/new"] cas each"#).unwrap();
        let mut fresh = Db::empty();
        load(&mut fresh);
        mem = fresh;
    }
}
