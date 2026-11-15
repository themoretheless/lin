//! Full-text search under mutation, against an oracle of which documents contain each
//! word: updates that change, keep or repeat the text, deletes, re-inserts of a deleted
//! uri, a checkpoint, a reopen that replays the log, and a reader snapshot taken before
//! the changes, which must keep answering for its own moment.

use std::collections::BTreeMap;

use lin::Db;

const WORDS: &[&str] = &["amber", "basil", "cedar", "dune", "ember"];

fn tmp() -> std::path::PathBuf {
    let n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let d = std::env::temp_dir().join(format!("lin-fts-mut-{}-{n}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn hits(db: &mut Db, w: &str) -> Vec<String> {
    let mut v: Vec<String> = db.run(&format!(r#"docs | search lex "{w}" | {{ uri }} | take all"#)).unwrap().rows.iter().map(|r| r.get("uri").and_then(|c| c.text()).unwrap().to_string()).collect();
    v.sort();
    v
}

fn read_hits(db: &lin::ReadDb, w: &str) -> Vec<String> {
    let mut v: Vec<String> = db.run(&format!(r#"docs | search lex "{w}" | {{ uri }} | take all"#)).unwrap().rows.iter().map(|r| r.get("uri").and_then(|c| c.text()).unwrap().to_string()).collect();
    v.sort();
    v
}

fn oracle(docs: &BTreeMap<String, String>, w: &str) -> Vec<String> {
    docs.iter().filter(|(_, t)| t.split_whitespace().any(|x| x == w)).map(|(u, _)| u.clone()).collect()
}

fn check(db: &mut Db, docs: &BTreeMap<String, String>, step: &str) {
    for w in WORDS {
        assert_eq!(hits(db, w), oracle(docs, w), "{step}: {w}");
    }
}

#[test]
fn search_follows_every_mutation_and_replay() {
    let dir = tmp();
    let mut db = Db::open(&dir).unwrap();
    let mut docs: BTreeMap<String, String> = BTreeMap::new();
    let ins = (0..40).map(|i| {
        let t = format!("{} {} note {i}", WORDS[i % 5], WORDS[(i / 5) % 5]);
        docs.insert(format!("raw://f/{i}"), t.clone());
        format!(r#"{{ uri: "raw://f/{i}", title: "{t}", layer: "raw" }}"#)
    }).collect::<Vec<_>>().join(",");
    db.run(&format!("insert docs [{ins}]")).unwrap();
    check(&mut db, &docs, "initial");
    let before = db.reader();
    let before_docs = docs.clone();

    let set = |db: &mut Db, docs: &mut BTreeMap<String, String>, i: usize, t: &str| {
        db.run(&format!(r#"update docs[uri == "raw://f/{i}"] cas each {{ title: "{t}" }}"#)).unwrap();
        docs.insert(format!("raw://f/{i}"), t.to_string());
    };
    set(&mut db, &mut docs, 0, "dune dune only");
    set(&mut db, &mut docs, 1, "basil cedar note 1");
    set(&mut db, &mut docs, 1, "basil cedar note 1");
    set(&mut db, &mut docs, 1, "basil cedar note 1");
    set(&mut db, &mut docs, 2, "nothing matches here");
    set(&mut db, &mut docs, 2, "ember again");
    check(&mut db, &docs, "after updates");
    for i in [3, 4, 5] {
        db.run(&format!(r#"delete docs[uri == "raw://f/{i}"] cas each"#)).unwrap();
        docs.remove(&format!("raw://f/{i}"));
    }
    check(&mut db, &docs, "after deletes");
    db.run(r#"insert docs { uri: "raw://f/4", title: "amber returns", layer: "raw" }"#).unwrap();
    docs.insert("raw://f/4".into(), "amber returns".into());
    check(&mut db, &docs, "after re-insert");
    db.checkpoint().unwrap();
    check(&mut db, &docs, "after checkpoint");
    set(&mut db, &mut docs, 6, "cedar after checkpoint");
    db.run(r#"delete docs[uri == "raw://f/7"] cas each"#).unwrap();
    docs.remove("raw://f/7");
    check(&mut db, &docs, "after post-checkpoint changes");
    for w in WORDS {
        assert_eq!(read_hits(&before, w), oracle(&before_docs, w), "old snapshot: {w}");
    }
    drop(before);
    drop(db);
    let mut db = Db::open(&dir).unwrap();
    check(&mut db, &docs, "after reopen");
    set(&mut db, &mut docs, 8, "basil after reopen");
    check(&mut db, &docs, "after change following reopen");
}
