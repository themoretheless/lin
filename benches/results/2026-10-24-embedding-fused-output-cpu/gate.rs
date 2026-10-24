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

mod baseline{use std::cmp::Ordering;
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
            self.fill(scratch, crate::lowercased(text, lower));
            scratch.finish()
        })
    }

    fn embed_batch(&self, texts: &[&str]) -> Vec<Arc<[f32]>> {
        EMBED_SCRATCH.with(|cell| {
            let mut guard = cell.borrow_mut();
            let (scratch, lower) = &mut *guard;
            scratch.ensure_dim(self.dim);
            let mut out = Vec::with_capacity(texts.len());
            for text in texts {
                self.fill(scratch, crate::lowercased(text, lower));
                out.push(scratch.finish());
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

}
mod candidate{use std::cmp::Ordering;
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
            self.fill(scratch, crate::lowercased(text, lower));
            scratch.finish()
        })
    }

    fn embed_batch(&self, texts: &[&str]) -> Vec<Arc<[f32]>> {
        EMBED_SCRATCH.with(|cell| {
            let mut guard = cell.borrow_mut();
            let (scratch, lower) = &mut *guard;
            scratch.ensure_dim(self.dim);
            let mut out = Vec::with_capacity(texts.len());
            for text in texts {
                self.fill(scratch, crate::lowercased(text, lower));
                out.push(scratch.finish());
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
        let sparse = self.touched.len() * 5 < self.v.len();
        let mut sum = 0.0f32;
        if sparse {
            for &i in &self.touched { let x=self.v[i as usize]; sum+=x*x; }
            if sum>=1_048_576.0 || !sum.is_finite() {
                self.touched.sort_unstable(); sum=0.0;
                for &i in &self.touched { let x=self.v[i as usize]; sum+=x*x; }
            }
        } else { for &x in &self.v {sum+=x*x;} }
        let inv = if (sparse && sum>1e-12) || (!sparse && !(sum<=1e-12)) {
            Some(sum.sqrt().recip())
        } else {None};
        let mut output=Arc::<[f32]>::new_uninit_slice(self.v.len());
        let slots=Arc::get_mut(&mut output).unwrap();
        for (slot,&value) in slots.iter_mut().zip(&self.v) {
            slot.write(match inv {Some(inv)=>value*inv,None=>value});
        }
        // Every slot has been initialized by the equal-length loop above.
        let output=unsafe{output.assume_init()};
        for &i in &self.touched {self.v[i as usize]=0.0;self.seen[i as usize]=false;}
        self.touched.clear();output
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

}
fn fixture(n:usize)->Vec<String>{(0..n).map(|i|if i%10==0 {format!("doc {i} wal note body {i}")}else{format!("doc {i} plain body {i}")}).collect()}

fn main(){
 use std::{hint::black_box,time::Instant};use baseline::Embedder as _;use candidate::Embedder as _;
 let process:usize=std::env::args().nth(1).unwrap().parse().unwrap();
 for dim in [8,9,768,1536] {let a=baseline::HashingEmbedder::new("test",dim);let b=candidate::HashingEmbedder::new("test",dim);
  for text in ["", "a", "doc 1 plain body 1", "ΟΣ İ ЁЖ 😀", "a\u{b}b", &"repeat ".repeat(8192)] {let x=a.embed(text);let y=b.embed(text);assert!(x.iter().zip(y.iter()).all(|(x,y)|x.to_bits()==y.to_bits()));}}
 for (name,texts,dim,loops) in [("default_1k",fixture(1000),768,16),("default_10k",fixture(10000),768,2),("unicode_1k",vec!["ΟΣ İ ЁЖ 😀 repeated ".repeat(8);1000],768,4),("small_dim",fixture(1000),8,16)] {
 let refs:Vec<&str>=texts.iter().map(String::as_str).collect();let a=baseline::HashingEmbedder::new("test",dim);let b=candidate::HashingEmbedder::new("test",dim);
 let x=a.embed_batch(&refs);let y=b.embed_batch(&refs);assert_eq!(x.len(),y.len());assert!(x.iter().zip(&y).all(|(x,y)|x.iter().zip(y.iter()).all(|(x,y)|x.to_bits()==y.to_bits())));drop(x);drop(y);
 let mut samples=vec![Vec::<f64>::new();2];
 for round in 0..24 {for pos in 0..2 {let v=(pos+round+process)%2;let t=Instant::now();for _ in 0..loops{if v==0{black_box(a.embed_batch(black_box(&refs)));}else{black_box(b.embed_batch(black_box(&refs)));}}samples[v].push(t.elapsed().as_nanos()as f64/loops as f64);}}
 println!("{}",serde_json::json!({"case":name,"ns":samples,"exact_bits":true}));
 }
}
