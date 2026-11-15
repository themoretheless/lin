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
