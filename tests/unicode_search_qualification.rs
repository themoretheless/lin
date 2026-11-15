//! Lexical search folds case with the full Unicode rules on both sides: an upper-case
//! query finds lower-case text and the reverse, including Greek words whose capital sigma
//! becomes a final `ς`, and the same holds after a checkpoint rebuilds the index files.

use lin::Db;

fn hits(db: &mut Db, q: &str) -> Vec<String> {
    let mut v: Vec<String> = db.run(&format!("docs | search lex {q:?} | {{ uri }} | take all")).unwrap().rows.iter().map(|r| r.get("uri").and_then(|c| c.text()).unwrap().to_string()).collect();
    v.sort();
    v
}

fn check(db: &mut Db, step: &str) {
    for q in ["ΟΔΟΣ", "οδος", "Οδος"] {
        assert_eq!(hits(db, q), ["raw://g/0", "raw://g/1"], "{step}: {q}");
    }
    for q in ["ЁЖИК", "ёжик"] {
        assert_eq!(hits(db, q), ["raw://g/2", "raw://g/3"], "{step}: {q}");
    }
    assert_eq!(hits(db, "CAFÉ"), ["raw://g/4"], "{step}");
}

#[test]
fn case_folding_matches_across_scripts() {
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!("lin-unicode-search-{}-{}", std::process::id(), SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut db = Db::open(&dir).unwrap();
    for (i, t) in ["ΟΔΟΣ ΚΑΛΗ", "μια οδος", "ЁЖИК В ТУМАНЕ", "ёжик", "café"].iter().enumerate() {
        db.run(&format!("insert docs {{ uri: \"raw://g/{i}\", title: {t:?}, layer: \"raw\" }}")).unwrap();
    }
    check(&mut db, "live");
    db.checkpoint().unwrap();
    drop(db);
    let mut db = Db::open(&dir).unwrap();
    check(&mut db, "after reopen");
}
