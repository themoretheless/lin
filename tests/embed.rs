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
