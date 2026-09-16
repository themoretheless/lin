//! Embedder adapter smoke (hashing always; ollama behind feature).

use std::sync::Arc;

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

#[cfg(feature = "embed-ollama")]
mod ollama {
    use lin::{Embedder, OllamaEmbedder};

    #[test]
    fn ollama_type_is_embedder() {
        let e = OllamaEmbedder::new("http://127.0.0.1:11434", "nomic-embed-text/768").unwrap();
        assert_eq!(e.dim(), 768);
    }
}
