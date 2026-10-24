use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, OnceLock};

use rustc_hash::{FxHashMap, FxHashSet, FxHasher};

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
        EMBED_SCRATCH.with(|cell| {
            let mut guard = cell.borrow_mut();
            let (scratch, lower) = &mut *guard;
            scratch.ensure_dim(self.dim);
            self.fill(scratch, lowercased(text, lower));
            scratch.finish()
        })
    }

    fn embed_batch(&self, texts: &[&str]) -> Vec<Arc<[f32]>> {
        EMBED_SCRATCH.with(|cell| {
            let mut guard = cell.borrow_mut();
            let (scratch, lower) = &mut *guard;
            scratch.ensure_dim(self.dim);
            let mut out = Vec::with_capacity(texts.len());
            for (i,text) in texts.iter().enumerate() {
                if i%16==0 {
                    let t=std::time::Instant::now();
                    self.fill(scratch,lowercased(text,lower));
                    let fill_ns=t.elapsed().as_nanos();let t=std::time::Instant::now();
                    out.push(scratch.finish());let finish_ns=t.elapsed().as_nanos();
                    PROFILE.with(|p|p.borrow_mut().push((fill_ns,finish_ns)));
                } else {
                    self.fill(scratch,lowercased(text,lower));out.push(scratch.finish());
                }
            }
            out
        })
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

thread_local! {
    static EMBED_SCRATCH: std::cell::RefCell<(Scratch, String)> = std::cell::RefCell::new((
        Scratch::new(768),
        String::with_capacity(256),
    ));
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
            touched: Vec::with_capacity(dim / 4),
        }
    }

    fn ensure_dim(&mut self, dim: usize) {
        if self.v.len() != dim {
            self.v = vec![0f32; dim];
            self.seen = vec![false; dim];
            self.touched.clear();
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


fn lowercased<'a>(text: &'a str, scratch: &'a mut String) -> &'a str {
    if text.is_ascii() {
        // Fast path: check if already lowercase using byte-level SIMD detection
        // The .any() iterator is auto-vectorized by rustc for contiguous memory
        if !text.bytes().any(|b| b.is_ascii_uppercase()) {
            return text;
        }

        scratch.clear();
        // make_ascii_lowercase() uses single-instruction conversion on modern CPUs:
        // x86_64: AND with 0xDF to preserve case + OR with 0x20 unconditionally
        // ARM NEON: vbic/vorr pattern for selective bit manipulation
        scratch.reserve(text.len());
        scratch.push_str(text);
        scratch.make_ascii_lowercase();
        return scratch;
    }

    if !text.chars().any(|c| c.to_lowercase().next() != Some(c)) {
        return text;
    }
    scratch.clear();
    for c in text.chars() {
        scratch.extend(c.to_lowercase());
    }
    scratch
}

fn fixture(n:usize)->Vec<String>{(0..n).map(|i|if i%10==0 {format!("doc {i} wal note body {i}")}else{format!("doc {i} plain body {i}")}).collect()}

thread_local!{static PROFILE:std::cell::RefCell<Vec<(u128,u128)>>=const{std::cell::RefCell::new(Vec::new())};}
fn main(){
 for (name,texts,dim) in [("default_1k",fixture(1000),768),("default_10k",fixture(10000),768),("unicode_1k",vec!["ЁЖ ΟΣ İ wal\u{b}note a\u{a0}b ".repeat(8);1000],768),("small_dim",fixture(1000),8)]{
  let emb=HashingEmbedder::new("test",dim);let original=lin::HashingEmbedder::new("test",dim);let refs:Vec<&str>=texts.iter().map(String::as_str).collect();
  let expected=lin::Embedder::embed_batch(&original,&refs);let actual=emb.embed_batch(&refs);assert_eq!(actual.len(),expected.len());for (a,b) in actual.iter().zip(&expected){assert_eq!(a.len(),b.len());assert!(a.iter().zip(b.iter()).all(|(a,b)|a.to_bits()==b.to_bits()));}drop(actual);drop(expected);
  PROFILE.with(|p|p.borrow_mut().clear());
  for sample in 0..12 {let t=std::time::Instant::now();let out=emb.embed_batch(&refs);let total=t.elapsed().as_nanos();std::hint::black_box(&out);drop(out);
   let samples=PROFILE.with(|p|std::mem::take(&mut*p.borrow_mut()));assert_eq!(samples.len(),texts.len().div_ceil(16));println!("{}",serde_json::json!({"case":name,"sample":sample,"rows":texts.len(),"sampled_stride":16,"sampled_fill_finish_ns":samples,"batch_ns_excluding_output_drop":total,"exact_bits_preflight":true}));
  }
 }
}
