//! Lexical query terms: repeating a word in the query never changes which documents
//! match, and adds the same weight to every document containing it, so the only reorder
//! it causes comes from the exact-phrase bonus (a document containing the repeated
//! phrase verbatim); word order changes nothing unless it changes the phrase; matching is
//! case-insensitive.

use std::collections::BTreeSet;

use lin::Db;

fn seed() -> Db {
    let mut db = Db::empty();
    let t = ["alpha beta", "alpha alpha gamma", "beta gamma", "alpha", "gamma gamma gamma beta"];
    for (i, x) in t.iter().enumerate() {
        db.run(&format!(r#"insert docs {{ uri: "raw://q/{i}", title: "{x}", layer: "raw" }}"#)).unwrap();
    }
    db
}

fn ranked(db: &mut Db, q: &str) -> Vec<String> {
    db.run(&format!(r#"docs | search lex "{q}" | {{ uri }} | take all"#)).unwrap().rows.iter().map(|r| r.get("uri").and_then(|c| c.text()).unwrap().to_string()).collect()
}

#[test]
fn repeated_and_reordered_terms() {
    let mut db = seed();
    let set = |v: &[String]| v.iter().cloned().collect::<BTreeSet<_>>();
    let one = ranked(&mut db, "alpha");
    let twice = ranked(&mut db, "alpha alpha");
    assert_eq!(set(&one), set(&twice), "repetition changed which documents match");
    // The only document moved up is the one containing the phrase "alpha alpha".
    assert_eq!(twice[0], "raw://q/1");
    let rest: Vec<&String> = twice.iter().skip(1).collect();
    let one_without: Vec<&String> = one.iter().filter(|u| *u != "raw://q/1").collect();
    assert_eq!(rest, one_without, "repetition reordered documents without the phrase");
    // Word order can only move documents through the phrase bonus; the matches are the same.
    assert_eq!(set(&ranked(&mut db, "alpha beta")), set(&ranked(&mut db, "beta alpha")), "word order changed which documents match");
    assert_eq!(ranked(&mut db, "ALPHA"), one, "case changed the result");
}

/// Empty and whitespace-only queries match nothing (and are not errors); runs of
/// whitespace between words are one separator, so they never form empty tokens that
/// would match every document.
#[test]
fn empty_tokens_match_nothing() {
    let mut db = seed();
    for q in ["", "   ", "\t \t"] {
        let rows = db.run(&format!("docs | search lex {q:?} | {{ uri }} | take all")).unwrap().rows.len();
        assert_eq!(rows, 0, "{q:?} matched {rows} documents");
    }
    let spaced = db.run("docs | search lex \"alpha   \\t  beta\" | { uri } | take all").unwrap().rows.len();
    let plain = ranked(&mut db, "alpha beta").len();
    assert_eq!(spaced, plain, "extra whitespace changed the result");
}
