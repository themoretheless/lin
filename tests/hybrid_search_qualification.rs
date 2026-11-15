//! Hybrid search (lexical + vector) with a deterministic embedder: each document appears
//! at most once however many modalities match it, every lexical hit and every positive
//! vector hit is a candidate, scores and order are bit-stable across runs, a slice equals
//! the same slice of the full ranking, and documents with a zero vector are still found
//! by their words.

use std::collections::BTreeSet;
use std::sync::Arc;

use lin::{Db, Embedder};

struct Seeded;

impl Embedder for Seeded {
    fn id(&self) -> &str {
        "seeded-hybrid/8"
    }
    fn dim(&self) -> usize {
        8
    }
    fn embed(&self, text: &str) -> Arc<[f32]> {
        if text.contains("zero") {
            return Arc::from(vec![0.0f32; 8]);
        }
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for b in text.bytes() {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x100_0000_01b3);
        }
        Arc::from((0..8).map(|_| {
            h ^= h << 13;
            h ^= h >> 7;
            h ^= h << 17;
            ((h % 2001) as f32 - 1000.0) / 1000.0
        }).collect::<Vec<f32>>())
    }
}

fn seed() -> Db {
    let mut db = Db::empty().with_embedder(Arc::new(Seeded));
    let words = ["kestrel", "heron", "osprey", "plover"];
    for i in 0..120 {
        let title = if i % 17 == 0 { format!("zero {} vector {i}", words[i % 4]) } else { format!("{} field note {i}", words[i % 4]) };
        db.run(&format!(r#"insert docs {{ uri: "raw://h/{i}", title: "{title}", layer: "raw" }}"#)).unwrap();
    }
    db
}

fn ranked(db: &mut Db, q: &str) -> Vec<String> {
    db.run(q).unwrap_or_else(|e| panic!("{q}: {e}")).rows.iter().map(|r| format!("{:?}", r)).collect()
}

fn uris(db: &mut Db, q: &str) -> Vec<String> {
    db.run(q).unwrap().rows.iter().map(|r| r.get("uri").and_then(|c| c.text()).unwrap().to_string()).collect()
}

#[test]
fn hybrid_results_are_unique_complete_and_stable() {
    let mut db = seed();
    for query in ["kestrel", "heron field", "osprey note 7", "zero"] {
        let full_q = format!(r#"docs | search "{query}" | {{ uri }} | take all"#);
        let full = uris(&mut db, &full_q);
        let set: BTreeSet<&String> = full.iter().collect();
        assert_eq!(set.len(), full.len(), "{query}: a document appears twice");
        // Every lexical hit is in the hybrid result.
        let lex: BTreeSet<String> = uris(&mut db, &format!(r#"docs | search lex "{query}" | {{ uri }} | take all"#)).into_iter().collect();
        for u in &lex {
            assert!(set.contains(u), "{query}: lexical hit {u} missing from hybrid");
        }
        // Zero-vector documents are still found through their words.
        if query == "kestrel" {
            assert!(full.iter().any(|u| u == "raw://h/0"), "zero-vector doc with the word is missing");
        }
        // Bit-stable across runs, including the score columns.
        let detailed = ranked(&mut db, &format!(r#"docs | search "{query}" | take all"#));
        for _ in 0..3 {
            assert_eq!(ranked(&mut db, &format!(r#"docs | search "{query}" | take all"#)), detailed, "{query}: unstable");
        }
        // Slices are slices of the full ranking.
        for (skip, take) in [(0, 5), (3, 4), (10, 100)] {
            let part = uris(&mut db, &format!(r#"docs | search "{query}" | {{ uri }} | skip {skip} | take {take}"#));
            let want: Vec<String> = full.iter().skip(skip).take(take).cloned().collect();
            assert_eq!(part, want, "{query}: skip {skip} take {take}");
        }
    }
}

/// Vector Top-K under a filter is exact: filtering then searching with `take k` gives
/// the first k of the full filtered ranking, for every k.
#[test]
fn filtered_vector_top_k_is_a_prefix_of_the_filtered_ranking() {
    let mut db = Db::empty().with_embedder(Arc::new(Seeded));
    for i in 0..150 {
        let layer = if i % 3 == 0 { "wiki" } else { "raw" };
        db.run(&format!(r#"insert docs {{ uri: "raw://k/{i}", title: "entry {i} about birds", layer: "{layer}" }}"#)).unwrap();
    }
    for query in ["birds", "entry 42", "nothing alike"] {
        let full = uris(&mut db, &format!(r#"docs | layer == "wiki" | search vec "{query}" | {{ uri }} | take all"#));
        assert!(full.iter().all(|u| u[8..].parse::<usize>().unwrap() % 3 == 0), "{query}: filter leaked");
        for k in [1usize, 5, 17, 49, 50, 200] {
            let top = uris(&mut db, &format!(r#"docs | layer == "wiki" | search vec "{query}" | {{ uri }} | take {k}"#));
            let want: Vec<String> = full.iter().take(k).cloned().collect();
            assert_eq!(top, want, "{query}: take {k}");
        }
    }
}

/// Hybrid ranking is exactly reciprocal rank fusion (k = 60) of the lexical and vector
/// rankings, ties broken by the document key, recomputed here from those two rankings.
#[test]
fn hybrid_ranking_is_exact_rrf_of_lex_and_vec() {
    let mut db = seed();
    let key = |db: &mut Db, q: &str| -> Vec<(String, String)> {
        db.run(q).unwrap().rows.iter().map(|r| {
            let uri = r.get("uri").and_then(|c| c.text()).unwrap().to_string();
            let id = r.get("id").and_then(|c| c.text()).map(str::to_string).unwrap_or_else(|| uri.clone());
            (id, uri)
        }).collect()
    };
    for query in ["kestrel", "heron field", "osprey note 7", "zero"] {
        let lex = key(&mut db, &format!(r#"docs | search lex "{query}" | {{ id, uri }} | take all"#));
        let vec = key(&mut db, &format!(r#"docs | search vec "{query}" | {{ id, uri }} | take all"#));
        let mut scores: std::collections::BTreeMap<String, (f64, String)> = std::collections::BTreeMap::new();
        for list in [&lex, &vec] {
            for (rank, (id, uri)) in list.iter().enumerate() {
                let e = scores.entry(id.clone()).or_insert((0.0, uri.clone()));
                e.0 += 1.0 / (60.0 + rank as f64 + 1.0);
            }
        }
        let mut want: Vec<(f64, String)> = scores.into_values().collect();
        want.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
        let want: Vec<String> = want.into_iter().map(|(_, u)| u).collect();
        let got = uris(&mut db, &format!(r#"docs | search "{query}" | {{ uri }} | take all"#));
        assert_eq!(got, want, "{query}: hybrid is not RRF of lex and vec");
    }
}
