//! Embedder adapter smoke (hashing always; ollama behind feature).

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use lin::{Db, Embedder, HashingEmbedder};

#[test]
fn hashing_remains_default_embedder() {
    let db = Db::fixture();
    // Fixture wires hashing from catalog embed_id.
    let emb = HashingEmbedder::from_embed_id("nomic-embed-text/768");
    assert_eq!(emb.dim(), 768);
    let v = emb.embed("wal shipping");
    assert_eq!(v.len(), 768);
    let _ = db;
}

#[test]
fn with_embedder_swap_roundtrip() {
    struct Fixed(Arc<[f32]>);
    impl Embedder for Fixed {
        fn id(&self) -> &str {
            "fixed/4"
        }
        fn dim(&self) -> usize {
            4
        }
        fn embed(&self, _text: &str) -> Arc<[f32]> {
            Arc::clone(&self.0)
        }
    }

    let mut db = Db::empty();
    db = db.with_embedder(Arc::new(Fixed(Arc::from([1f32, 0., 0., 0.]))));
    // Smoke: run still works after swap (empty catalog ok for empty db ops).
    let _ = db.stats();
}

#[test]
fn bulk_insert_uses_embedder_batch_api_once() {
    struct Counting {
        singles: Arc<AtomicUsize>,
        batches: Arc<AtomicUsize>,
    }
    impl Embedder for Counting {
        fn id(&self) -> &str {
            "nomic-embed-text/768"
        }
        fn dim(&self) -> usize {
            768
        }
        fn embed(&self, _text: &str) -> Arc<[f32]> {
            self.singles.fetch_add(1, Ordering::Relaxed);
            Arc::from(vec![1.0; 768])
        }
        fn embed_batch(&self, texts: &[&str]) -> Vec<Arc<[f32]>> {
            self.batches.fetch_add(1, Ordering::Relaxed);
            texts.iter().map(|_| Arc::from(vec![1.0; 768])).collect()
        }
    }

    let singles = Arc::new(AtomicUsize::new(0));
    let batches = Arc::new(AtomicUsize::new(0));
    let mut db = Db::fixture().with_embedder(Arc::new(Counting {
        singles: Arc::clone(&singles),
        batches: Arc::clone(&batches),
    }));
    db.run(
        r#"insert docs [
            { uri: "raw://batch/a", title: "alpha", layer: "wiki", body: "one" },
            { uri: "raw://batch/b", title: "beta", layer: "wiki", body: "two" }
        ]"#,
    )
    .unwrap();

    assert_eq!(batches.load(Ordering::Relaxed), 1);
    assert_eq!(singles.load(Ordering::Relaxed), 0);
}

#[cfg(feature = "embed-ollama")]
mod ollama {
    use lin::{Embedder, OllamaEmbedder};

    #[test]
    fn ollama_type_is_embedder() {
        let e = OllamaEmbedder::new("http://127.0.0.1:11434", "nomic-embed-text/768").unwrap();
        assert_eq!(e.dim(), 768);
    }
}
use rustc_hash::FxHasher;
use std::hash::{Hash, Hasher};

/// Reference copy of the feature-hash recipe (unigram 1.0, char trigram 0.5,
/// string bigram 0.25, then L2 over the dense vector). The sparse accumulator
/// in `HashingEmbedder` must keep producing exactly these vectors, or rows
/// stored earlier would score against embeddings nobody computes any more.
fn bump(v: &mut [f32], key: impl Hash, w: f32) {
    let mut h = FxHasher::default();
    key.hash(&mut h);
    let i = (h.finish() as usize) % v.len();
    v[i] += w;
}

fn reference_vector(text: &str, dim: usize) -> Vec<f32> {
    let mut lower = String::new();
    for ch in text.chars() {
        for l in ch.to_lowercase() {
            lower.push(l);
        }
    }
    let mut v = vec![0f32; dim];
    for token in lower.split_whitespace() {
        bump(&mut v, token, 1.0);
        let b = token.as_bytes();
        if b.len() >= 3 {
            for w in b.windows(3) {
                bump(&mut v, w, 0.5);
            }
        } else {
            bump(&mut v, b, 0.5);
        }
    }
    for w in lower.as_bytes().windows(2) {
        bump(&mut v, w, 0.25);
    }
    let s: f32 = v.iter().map(|x| x * x).sum();
    if s > 1e-12 {
        let inv = s.sqrt().recip();
        for x in v.iter_mut() {
            *x *= inv;
        }
    }
    v
}

#[test]
fn hashing_vectors_match_the_reference_recipe() {
    let long = "the quick brown fox jumps over the lazy dog ".repeat(120);
    let texts = ["wal shipping", "WAL École Write-Ahead", "a", &long];
    let e = HashingEmbedder::from_embed_id("nomic-embed-text/768");
    for text in &texts {
        assert_eq!(
            e.embed(text).as_ref(),
            reference_vector(text, 768).as_slice(),
            "single embed for {text:?}"
        );
    }
    for (n, got) in e.embed_batch(&texts).iter().enumerate() {
        assert_eq!(
            got.as_ref(),
            reference_vector(texts[n], 768).as_slice(),
            "batch embed for {:?}",
            texts[n]
        );
    }
}
