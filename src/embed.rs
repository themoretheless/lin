//! In-process embedders for `search vec` / `search hybrid`.
//!
//! Default is a deterministic feature-hash embedder sized from `embed_id`
//! (`…/768` → dim 768). It is **not** a neural model — swap via
//! [`crate::exec::Db::with_embedder`] when a real backend is available.

use std::sync::Arc;

use rustc_hash::FxHasher;
use std::hash::Hasher;

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

    /// Embed a slab into owned vectors. Implementations can use this to
    /// transfer vector allocations directly into storage without copying.
    fn embed_batch_owned(&self, texts: &[&str]) -> Vec<Vec<f32>> {
        self.embed_batch(texts)
            .into_iter()
            .map(|vector| vector.as_ref().to_vec())
            .collect()
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
        if text.is_ascii() && !has_ascii_uppercase(text) {
            return self.embed_lowered(text);
        }
        let mut lower = String::with_capacity(text.len());
        lowercase_into(text, &mut lower);
        self.embed_lowered(&lower)
    }

    fn embed_batch(&self, texts: &[&str]) -> Vec<Arc<[f32]>> {
        let mut lower = String::new();
        let mut scratch = vec![0f32; self.dim];
        let mut out = Vec::with_capacity(texts.len());
        for text in texts {
            if text.is_ascii() && !has_ascii_uppercase(text) {
                self.embed_lowered_into(text, &mut scratch);
                out.push(Arc::from(scratch.clone()));
                continue;
            }
            lower.clear();
            lower.reserve(text.len());
            lowercase_into(text, &mut lower);
            self.embed_lowered_into(&lower, &mut scratch);
            out.push(Arc::from(scratch.clone()));
        }
        out
    }

    fn embed_batch_owned(&self, texts: &[&str]) -> Vec<Vec<f32>> {
        let mut lower = String::new();
        let mut out = Vec::with_capacity(texts.len());
        for text in texts {
            let mut vector = vec![0f32; self.dim];
            if text.is_ascii() && !has_ascii_uppercase(text) {
                self.embed_lowered_into(text, &mut vector);
                out.push(vector);
                continue;
            }
            lower.clear();
            lower.reserve(text.len());
            lowercase_into(text, &mut lower);
            self.embed_lowered_into(&lower, &mut vector);
            out.push(vector);
        }
        out
    }
}

impl HashingEmbedder {
    fn embed_lowered(&self, lower: &str) -> Arc<[f32]> {
        let mut v = vec![0f32; self.dim];
        self.embed_lowered_into(lower, &mut v);
        Arc::from(v)
    }

    fn embed_lowered_into(&self, lower: &str, v: &mut [f32]) {
        let dim = v.len();
        if dim == 0 {
            return;
        }
        let dim_is_power_of_two = dim.is_power_of_two();
        v.fill(0.0);
        for token in lower.split_whitespace() {
            if token.is_empty() {
                continue;
            }
            let bytes = token.as_bytes();
            bump_hashed(v, bytes, dim, 1.0, dim_is_power_of_two);
            if bytes.len() >= 3 {
                for window in bytes.windows(3) {
                    bump_hashed(v, window, dim, 0.5, dim_is_power_of_two);
                }
            } else {
                bump_hashed(v, bytes, dim, 0.5, dim_is_power_of_two);
            }
        }
        // Character bigrams over the whole string catch short queries.
        let bytes = lower.as_bytes();
        for window in bytes.windows(2) {
            bump_hashed(v, window, dim, 0.25, dim_is_power_of_two);
        }
        l2_normalize(v);
    }
}

#[inline]
fn has_ascii_uppercase(text: &str) -> bool {
    text.bytes().any(|byte| byte.is_ascii_uppercase())
}

fn lowercase_into(text: &str, out: &mut String) {
    if text.is_ascii() {
        out.reserve(text.len());
        for byte in text.bytes() {
            out.push(byte.to_ascii_lowercase() as char);
        }
        return;
    }
    for ch in text.chars() {
        for lower in ch.to_lowercase() {
            out.push(lower);
        }
    }
}

#[inline]
fn hash_bytes(key: &[u8]) -> u64 {
    let mut h = FxHasher::default();
    h.write(key);
    h.finish()
}

#[inline]
fn bucket_index(hash_value: u64, dim: usize, dim_is_power_of_two: bool) -> usize {
    let hash = hash_value as usize;
    if dim_is_power_of_two {
        hash & (dim - 1)
    } else {
        hash % dim
    }
}

#[inline]
fn bump_hashed(v: &mut [f32], key: &[u8], dim: usize, w: f32, dim_is_power_of_two: bool) {
    let idx = bucket_index(hash_bytes(key), dim, dim_is_power_of_two);
    v[idx] += w;
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
    let fields = ["title", "body", "snippet"];
    let mut capacity = 0;
    let mut count = 0usize;
    for field in fields {
        if let Some(text) = crate::store::row_text(row, field) {
            capacity += text.len();
            count += 1;
        }
    }
    capacity += count.saturating_sub(1);
    let mut blob = String::with_capacity(capacity);
    let mut written = 0;
    for field in fields {
        if let Some(text) = crate::store::row_text(row, field) {
            if written != 0 {
                blob.push(' ');
            }
            blob.push_str(text);
            written += 1;
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
