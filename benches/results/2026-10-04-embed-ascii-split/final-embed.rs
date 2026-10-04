//! In-process embedders for `search vec` / `search hybrid`.
//!
//! Default is a deterministic feature-hash embedder sized from `embed_id`
//! (`…/768` → dim 768). It is **not** a neural model — swap via
//! [`crate::exec::Db::with_embedder`] when a real backend is available.

use std::sync::{Arc, OnceLock};

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

    /// Return owned vectors while preserving existing batch overrides.
    fn embed_batch_owned(&self, texts: &[&str]) -> Vec<Vec<f32>> {
        self.embed_batch(texts)
            .into_iter()
            .map(|v| v.as_ref().to_vec())
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
        if dim == 768 {
            bigram_slots();
        }
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
        let mut scratch = Scratch::new(self.dim);
        let mut lower = String::new();
        self.fill(&mut scratch, crate::fts::lowercased(text, &mut lower));
        scratch.finish()
    }

    fn embed_batch(&self, texts: &[&str]) -> Vec<Arc<[f32]>> {
        let mut scratch = Scratch::new(self.dim);
        let mut lower = String::new();
        let mut out = Vec::with_capacity(texts.len());
        for text in texts {
            self.fill(&mut scratch, crate::fts::lowercased(text, &mut lower));
            out.push(scratch.finish());
        }
        out
    }
}

impl HashingEmbedder {
    /// Add every feature of `lower` (unigrams, char trigrams, string bigrams)
    /// into the accumulator.
    fn fill(&self, s: &mut Scratch, lower: &str) {
        // The ASCII splitter excludes vertical tab; Unicode splitting includes it.
        if lower.is_ascii() && !lower.as_bytes().contains(&0x0b) {
            for token in lower.split_ascii_whitespace() {
                s.bump_token(token);
            }
        } else {
            for token in lower.split_whitespace() {
                s.bump_token(token);
            }
        }
        // Character bigrams over the whole string catch short queries.
        let bytes = lower.as_bytes();
        if self.dim == 768 {
            let slots = bigram_slots();
            for w in bytes.windows(2) {
                s.bump_slot(
                    slots[usize::from(w[0]) * 256 + usize::from(w[1])] as usize,
                    0.25,
                );
            }
        } else {
            for w in bytes.windows(2) {
                s.bump(w, 0.25);
            }
        }
    }
}

// Compute with the active hasher rather than baking platform-dependent hashes.
// Initialization happens once, including its cost in the first constructor.
fn bigram_slots() -> &'static [u16] {
    static SLOTS: OnceLock<Box<[u16]>> = OnceLock::new();
    SLOTS.get_or_init(|| {
        (0..65536usize)
            .map(|key| {
                let bytes = [(key / 256) as u8, key as u8];
                let mut h = FxHasher::default();
                bytes.as_slice().hash(&mut h);
                ((h.finish() as usize) % 768) as u16
            })
            .collect()
    })
}

/// Dense accumulator plus the list of slots the current row actually wrote, so
/// a batch reuses one allocation and normalizes only its own features.
struct Scratch {
    v: Vec<f32>,
    seen: Vec<bool>,
    touched: Vec<u32>,
}

impl Scratch {
    fn new(dim: usize) -> Self {
        Self {
            v: vec![0f32; dim],
            seen: vec![false; dim],
            touched: Vec::new(),
        }
    }

    #[inline]
    fn bump_token(&mut self, token: &str) {
        self.bump(token, 1.0);
        let bytes = token.as_bytes();
        if bytes.len() >= 3 {
            for window in bytes.windows(3) {
                self.bump(window, 0.5);
            }
        } else {
            self.bump(bytes, 0.5);
        }
    }

    #[inline]
    fn bump(&mut self, key: impl Hash, w: f32) {
        let mut h = FxHasher::default();
        key.hash(&mut h);
        let i = (h.finish() as usize) % self.v.len();
        self.bump_slot(i, w);
    }

    #[inline]
    fn bump_slot(&mut self, i: usize, w: f32) {
        let i = i as u32;
        if !self.seen[i as usize] {
            self.seen[i as usize] = true;
            self.touched.push(i);
        }
        self.v[i as usize] += w;
    }

    /// L2-normalize and hand out the vector. Untouched slots are exact `0.0`, so
    /// skipping them adds nothing to the sum. All feature weights are positive
    /// multiples of 1/4. Below 2^20, squares and their sum are exact multiples
    /// of 1/16 in f32, so visitation order cannot change the result. Larger
    /// sums retain the original ascending-slot order.
    fn finish(&mut self) -> Arc<[f32]> {
        if self.touched.len() * 5 < self.v.len() {
            let mut s = 0f32;
            for &i in &self.touched {
                let x = self.v[i as usize];
                s += x * x;
            }
            if s >= 1_048_576.0 || !s.is_finite() {
                self.touched.sort_unstable();
                s = 0.0;
                for &i in &self.touched {
                    let x = self.v[i as usize];
                    s += x * x;
                }
            }
            if s > 1e-12 {
                let inv = s.sqrt().recip();
                for &i in &self.touched {
                    self.v[i as usize] *= inv;
                }
            }
        } else {
            l2_normalize(&mut self.v);
        }
        let out = Arc::from(&self.v[..]);
        for &i in &self.touched {
            self.v[i as usize] = 0.0;
            self.seen[i as usize] = false;
        }
        self.touched.clear();
        out
    }
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
pub(crate) fn row_embed_text(row: &crate::store::Row) -> std::borrow::Cow<'_, str> {
    let texts = ["title", "body", "snippet"].map(|k| crate::store::row_text(row, k));
    let bytes = texts.iter().flatten().map(|t| t.len()).sum::<usize>();
    if bytes == 0 {
        return std::borrow::Cow::Borrowed("");
    }
    // Leading empty fields contribute no separator. Any present field after
    // the first nonempty one does, including an empty trailing field.
    if let Some(first) = texts.iter().position(|t| t.is_some_and(|s| !s.is_empty()))
        && texts[first + 1..].iter().all(Option::is_none)
    {
        return std::borrow::Cow::Borrowed(texts[first].unwrap());
    }
    let mut blob = String::with_capacity(bytes + 2);
    for text in texts.into_iter().flatten() {
        if !blob.is_empty() {
            blob.push(' ');
        }
        blob.push_str(text);
    }
    std::borrow::Cow::Owned(blob)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_embedding_field_borrows_the_row_text() {
        use crate::store::{Cell, Row};
        for field in ["title", "body", "snippet"] {
            let row = Row::from([(field.into(), Cell::text_arc("Ёж wal"))]);
            let text = row_embed_text(&row);
            assert!(matches!(text, std::borrow::Cow::Borrowed(_)));
            assert_eq!(text, "Ёж wal");
            let embedder = HashingEmbedder::new("test", 768);
            assert_eq!(embedder.embed(&text), embedder.embed("Ёж wal"));
        }
    }

    #[test]
    fn embedding_text_preserves_empty_fields_and_separators() {
        use crate::store::{Cell, Row};
        for (parts, expected) in [
            ([None, None, None], ""),
            ([Some(""), Some(""), Some("")], ""),
            ([Some(""), Some("body"), None], "body"),
            ([Some("title"), Some(""), Some("")], "title  "),
            ([Some("title"), None, Some("snippet")], "title snippet"),
            ([Some("Ёж 🦔"), Some("тело"), None], "Ёж 🦔 тело"),
        ] {
            let row = ["title", "body", "snippet"]
                .into_iter()
                .zip(parts)
                .map(|(k, v)| (k.to_owned(), v.map(Cell::text_arc).unwrap_or(Cell::Null)))
                .collect::<Row>();
            assert_eq!(row_embed_text(&row), expected);
        }
    }

    #[test]
    fn sparse_normalization_preserves_dense_bits_at_exact_sum_boundary() {
        let mut state = 0x1b203571u64;
        let mut cases = vec![
            vec![4095u32, 90, 9, 3], // 16 * norm2 = 2^24 - 1
            vec![4095, 90, 9, 3, 1], // exact threshold: ordered fallback
            vec![4095, 90, 9, 3, 1, 1],
            vec![4096, 1, 2, 3],
            vec![40000, 3, 50000, 11],
            vec![1, 2, 3, 4, 5],
        ];
        for case in 0..1000 {
            let mut values = Vec::new();
            for _ in 0..12 {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                let bound = if case % 2 == 0 { 100 } else { 80000 };
                values.push((state % bound) as u32);
            }
            cases.push(values);
        }
        for quarters in cases {
            let mut scratch = Scratch::new(768);
            for (k, q) in quarters.iter().enumerate() {
                let i = (k * 37 + 19) % 768;
                scratch.v[i] = *q as f32 / 4.0;
                scratch.seen[i] = true;
                scratch.touched.push(i as u32);
            }
            scratch.touched.reverse();
            let mut expected = scratch.v.clone();
            l2_normalize(&mut expected);
            let got = scratch.finish();
            assert_eq!(
                got.iter().map(|x| x.to_bits()).collect::<Vec<_>>(),
                expected.iter().map(|x| x.to_bits()).collect::<Vec<_>>(),
                "quarters={quarters:?}"
            );
            assert!(scratch.v.iter().all(|x| *x == 0.0));
            assert!(scratch.touched.is_empty());
        }
    }

    #[test]
    fn bigram_table_matches_slice_hash_for_every_byte_pair() {
        for (key, slot) in bigram_slots().iter().enumerate() {
            let bytes = [(key / 256) as u8, key as u8];
            let mut h = FxHasher::default();
            bytes.as_slice().hash(&mut h);
            assert_eq!(*slot as usize, (h.finish() as usize) % 768);
        }
    }

    #[test]
    fn hashing_embedding_matches_original_dense_modulo_bits() {
        fn bump(expected: &mut [f32], key: impl Hash, weight: f32) {
            let mut h = FxHasher::default();
            key.hash(&mut h);
            let i = (h.finish() as usize) % expected.len();
            expected[i] += weight;
        }
        for dim in [8, 9, 32, 384, 768, 1024, 1536] {
            let embedder = HashingEmbedder::new("test", dim);
            let mut texts = [
                "",
                "WAL tuning",
                "aa aa wal",
                "СМЕШАННЫЙ Регистр 🦔",
                "longer text with several tokens and repeated repeated words",
            ]
            .map(String::from)
            .to_vec();
            texts.extend((0u8..=127).map(|c| format!("ab{}cd", char::from(c))));
            texts.extend(
                [
                    '\u{85}', '\u{a0}', '\u{1680}', '\u{2000}', '\u{200a}', '\u{2028}', '\u{2029}',
                    '\u{202f}', '\u{205f}', '\u{3000}',
                ]
                .map(|c| format!("ab{c}cd")),
            );
            for text in texts {
                let lower = text.to_lowercase();
                let mut expected = vec![0f32; dim];
                for token in lower.split_whitespace() {
                    bump(&mut expected, token, 1.0);
                    if token.len() >= 3 {
                        for w in token.as_bytes().windows(3) {
                            bump(&mut expected, w, 0.5);
                        }
                    } else {
                        bump(&mut expected, token.as_bytes(), 0.5);
                    }
                }
                for w in lower.as_bytes().windows(2) {
                    bump(&mut expected, w, 0.25);
                }
                l2_normalize(&mut expected);
                let got = embedder.embed(&text);
                assert_eq!(
                    got.iter().map(|x| x.to_bits()).collect::<Vec<_>>(),
                    expected.iter().map(|x| x.to_bits()).collect::<Vec<_>>(),
                    "dim={dim}, text={text:?}"
                );
            }
        }
    }

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
