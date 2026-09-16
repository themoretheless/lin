//! In-process embedders for `search vec` / `search hybrid`.
//!
//! Default is a deterministic feature-hash embedder sized from `embed_id`
//! (`…/768` → dim 768). It is **not** a neural model — swap via
//! [`crate::exec::Db::with_embedder`] when a real backend is available.

use std::sync::Arc;

use rustc_hash::FxHasher;
use std::hash::{Hash, Hasher};

/// Text → dense vector. Must be `Send + Sync` for [`crate::exec::ReadDb`] sharing.
pub trait Embedder: Send + Sync {
    /// Stable identity (should match [`crate::catalog::Catalog::embed_id`] when active).
    fn id(&self) -> &str;
    fn dim(&self) -> usize;
    fn embed(&self, text: &str) -> Arc<[f32]>;

    /// Embed a slab. Implementations may reuse scratch buffers or issue a
    /// backend-native batch; output order and length must match `texts`.
    fn embed_batch(&self, texts: &[&str]) -> Vec<Arc<[f32]>> {
        texts.iter().map(|text| self.embed(text)).collect()
    }
}

/// Local hashing embedder (unigram + char trigrams → L2-normalized bag).
#[derive(Debug, Clone)]
pub struct HashingEmbedder {
    id: String,
    dim: usize,
}

impl HashingEmbedder {
    pub fn new(id: impl Into<String>, dim: usize) -> Self {
        let dim = dim.max(8);
        Self { id: id.into(), dim }
    }

    /// Parse `name/dim` (e.g. `nomic-embed-text/768`); default dim 768.
    pub fn from_embed_id(embed_id: &str) -> Self {
        let dim = embed_id
            .rsplit('/')
            .next()
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(768)
            .max(8);
        Self::new(embed_id, dim)
    }
}

impl Embedder for HashingEmbedder {
    fn id(&self) -> &str {
        &self.id
    }

    fn dim(&self) -> usize {
        self.dim
    }

    fn embed(&self, text: &str) -> Arc<[f32]> {
        let mut lower = String::with_capacity(text.len());
        lowercase_into(text, &mut lower);
        self.embed_lowered(&lower)
    }

    fn embed_batch(&self, texts: &[&str]) -> Vec<Arc<[f32]>> {
        let mut lower = String::new();
        let mut out = Vec::with_capacity(texts.len());
        for text in texts {
            lower.clear();
            lower.reserve(text.len());
            lowercase_into(text, &mut lower);
            out.push(self.embed_lowered(&lower));
        }
        out
    }
}

impl HashingEmbedder {
    fn embed_lowered(&self, lower: &str) -> Arc<[f32]> {
        let mut v = vec![0f32; self.dim];
        for token in lower.split_whitespace() {
            if token.is_empty() {
                continue;
            }
            bump(&mut v, &token, 1.0);
            let b = token.as_bytes();
            if b.len() >= 3 {
                for w in b.windows(3) {
                    bump(&mut v, w, 0.5);
                }
            } else {
                bump(&mut v, b, 0.5);
            }
        }
        // Character bigrams over the whole string catch short queries.
        let bytes = lower.as_bytes();
        for w in bytes.windows(2) {
            bump(&mut v, w, 0.25);
        }
        l2_normalize(&mut v);
        Arc::from(v)
    }
}

fn lowercase_into(text: &str, out: &mut String) {
    for ch in text.chars() {
        for lower in ch.to_lowercase() {
            out.push(lower);
        }
    }
}

#[inline]
fn bump(v: &mut [f32], key: impl Hash, w: f32) {
    let mut h = FxHasher::default();
    key.hash(&mut h);
    let i = (h.finish() as usize) % v.len();
    v[i] += w;
}

fn l2_normalize(v: &mut [f32]) {
    let mut s = 0f32;
    for x in v.iter() {
        s += x * x;
    }
    if s <= 1e-12 {
        return;
    }
    let inv = s.sqrt().recip();
    for x in v.iter_mut() {
        *x *= inv;
    }
}

/// Cosine similarity for L2-normalized (or arbitrary) vectors.
#[inline]
pub fn cosine(a: &[f32], b: &[f32]) -> f64 {
    let n = a.len().min(b.len());
    if n == 0 {
        return 0.0;
    }
    let mut dot = 0f64;
    let mut na = 0f64;
    let mut nb = 0f64;
    for i in 0..n {
        let x = a[i] as f64;
        let y = b[i] as f64;
        dot += x * y;
        na += x * x;
        nb += y * y;
    }
    if na <= 1e-18 || nb <= 1e-18 {
        return 0.0;
    }
    (dot / (na.sqrt() * nb.sqrt())).clamp(-1.0, 1.0)
}

/// Text blob used for embedding a row (title / body / snippet).
pub fn row_embed_text(row: &crate::store::Row) -> String {
    let mut blob = String::new();
    for k in ["title", "body", "snippet"] {
        if let Some(t) = crate::store::row_text(row, k) {
            if !blob.is_empty() {
                blob.push(' ');
            }
            blob.push_str(t);
        }
    }
    blob
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashing_batch_matches_single_embedding() {
        let embedder = HashingEmbedder::new("test/32", 32);
        let texts = ["WAL tuning", "short", "СМЕШАННЫЙ Регистр", ""];
        let batch = embedder.embed_batch(&texts);
        assert_eq!(batch.len(), texts.len());
        for (text, got) in texts.iter().zip(batch) {
            assert_eq!(got.as_ref(), embedder.embed(text).as_ref());
        }
    }
}
