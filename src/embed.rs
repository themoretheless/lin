//! In-process embedders for `search vec` / `search hybrid`.
//!
//! Default is a deterministic feature-hash embedder sized from `embed_id`
//! (`…/768` → dim 768). It is **not** a neural model — swap via
//! [`crate::exec::Db::with_embedder`] when a real backend is available.

use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, OnceLock};

use rustc_hash::{FxHashMap, FxHasher};

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

    /// Embed a sequence of rows. Default implementation extracts row text
    /// and delegates to `embed_batch`.
    fn embed_rows(&self, rows: &[&crate::store::Row]) -> Vec<Arc<[f32]>> {
        let texts = rows.iter().map(|r| row_embed_text(r)).collect::<Vec<_>>();
        let refs = texts.iter().map(|t| t.as_ref()).collect::<Vec<_>>();
        self.embed_batch(&refs)
    }

    /// Embed a contiguous mutable slice of rows in-place when all rows need embedding.
    fn embed_rows_in_place(&self, _rows: &mut [crate::store::Row]) -> bool {
        false
    }

    /// If this embedder is a local `HashingEmbedder`, returns `Some(dim)`.
    fn hashing_dim(&self) -> Option<usize> {
        None
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
            let (scratch, lower, _) = &mut *guard;
            scratch.ensure_dim(self.dim);
            self.fill(scratch, crate::fts::lowercased(text, lower));
            scratch.finish()
        })
    }

    fn embed_batch(&self, texts: &[&str]) -> Vec<Arc<[f32]>> {
        let n = texts.len();
        if n < 4096 {
            return self.embed_batch_serial(texts);
        }
        let threads = std::thread::available_parallelism()
            .map(|p| p.get())
            .unwrap_or(1)
            .min(8)
            .min((n + 511) / 512);
        if threads <= 1 {
            return self.embed_batch_serial(texts);
        }
        let chunk_size = (n + threads - 1) / threads;
        let chunks: Vec<&[&str]> = texts.chunks(chunk_size).collect();
        let (first, rest) = chunks.split_first().unwrap();
        std::thread::scope(|scope| {
            let mut handles = Vec::with_capacity(rest.len());
            for &chunk in rest {
                handles.push(scope.spawn(move || self.embed_batch_serial(chunk)));
            }
            let mut out = Vec::with_capacity(n);
            out.extend(self.embed_batch_serial(first));
            for handle in handles {
                out.extend(handle.join().expect("embedding worker panicked"));
            }
            out
        })
    }

    fn embed_rows(&self, rows: &[&crate::store::Row]) -> Vec<Arc<[f32]>> {
        let n = rows.len();
        if n < 4096 {
            return self.embed_rows_serial(rows);
        }
        let threads = std::thread::available_parallelism()
            .map(|p| p.get())
            .unwrap_or(1)
            .min(8)
            .min((n + 511) / 512);
        if threads <= 1 {
            return self.embed_rows_serial(rows);
        }
        let chunk_size = (n + threads - 1) / threads;
        let chunks: Vec<&[&crate::store::Row]> = rows.chunks(chunk_size).collect();
        let (first, rest) = chunks.split_first().unwrap();
        std::thread::scope(|scope| {
            let mut handles = Vec::with_capacity(rest.len());
            for &chunk in rest {
                handles.push(scope.spawn(move || self.embed_rows_serial(chunk)));
            }
            let mut out = Vec::with_capacity(n);
            out.extend(self.embed_rows_serial(first));
            for handle in handles {
                out.extend(handle.join().expect("embedding worker panicked"));
            }
            out
        })
    }

    fn embed_rows_in_place(&self, rows: &mut [crate::store::Row]) -> bool {
        let n = rows.len();
        if n < 256 {
            self.embed_rows_slice_in_place(rows);
            return true;
        }
        if !embed_rows_in_place_pooled(self.dim, rows) {
            self.embed_rows_slice_in_place(rows);
        }
        true
    }

    fn hashing_dim(&self) -> Option<usize> {
        Some(self.dim)
    }
}

#[inline]
pub(crate) fn embed_texts_hashing(dim: usize, texts: [Option<&str>; 3]) -> Arc<[f32]> {
    EMBED_SCRATCH.with(|cell| {
        let mut guard = cell.borrow_mut();
        let (scratch, lower, text_buf) = &mut *guard;
        scratch.ensure_dim(dim);
        let dummy = HashingEmbedder {
            id: String::new(),
            dim,
        };
        let bytes = texts.iter().flatten().map(|t| t.len()).sum::<usize>();
        if bytes == 0 {
            return scratch.finish();
        }
        if let Some(first) = texts.iter().position(|t| t.is_some_and(|s| !s.is_empty()))
            && texts[first + 1..].iter().all(Option::is_none)
        {
            dummy.fill(
                scratch,
                crate::fts::lowercased(texts[first].unwrap(), lower),
            );
            return scratch.finish();
        }
        text_buf.clear();
        text_buf.reserve(bytes + 2);
        for text in texts.into_iter().flatten() {
            if !text_buf.is_empty() {
                text_buf.push(' ');
            }
            text_buf.push_str(text);
        }
        dummy.fill(scratch, crate::fts::lowercased(text_buf, lower));
        scratch.finish()
    })
}

struct EmbedInPlaceJob {
    dim: usize,
    ptr: usize,
    len: usize,
}

struct EmbedPool {
    workers: Vec<std::sync::mpsc::SyncSender<EmbedInPlaceJob>>,
    done_rx: std::sync::mpsc::Receiver<()>,
}

fn embed_rows_in_place_pooled(dim: usize, rows: &mut [crate::store::Row]) -> bool {
    static POOL: OnceLock<std::sync::Mutex<Option<EmbedPool>>> = OnceLock::new();
    let mutex = POOL.get_or_init(|| {
        let threads = std::thread::available_parallelism()
            .map(|p| p.get())
            .unwrap_or(1)
            .min(4);
        if threads <= 1 {
            return std::sync::Mutex::new(None);
        }
        let num_workers = threads - 1;
        let (done_tx, done_rx) = std::sync::mpsc::sync_channel::<()>(num_workers);
        let mut workers = Vec::with_capacity(num_workers);
        for idx in 0..num_workers {
            let (tx, rx) = std::sync::mpsc::sync_channel::<EmbedInPlaceJob>(1);
            let done = done_tx.clone();
            if std::thread::Builder::new()
                .name(format!("lin-embed-{idx}"))
                .spawn(move || {
                    let mut dummy = HashingEmbedder {
                        id: String::new(),
                        dim: 768,
                    };
                    while let Ok(job) = rx.recv() {
                        dummy.dim = job.dim;
                        let slice = unsafe {
                            std::slice::from_raw_parts_mut(
                                job.ptr as *mut crate::store::Row,
                                job.len,
                            )
                        };
                        dummy.embed_rows_slice_in_place(slice);
                        let _ = done.send(());
                    }
                })
                .is_ok()
            {
                workers.push(tx);
            }
        }
        if workers.is_empty() {
            std::sync::Mutex::new(None)
        } else {
            std::sync::Mutex::new(Some(EmbedPool { workers, done_rx }))
        }
    });

    let Ok(guard) = mutex.try_lock() else {
        return false;
    };
    let Some(pool) = guard.as_ref() else {
        return false;
    };
    let total_ways = pool.workers.len() + 1;
    let n = rows.len();
    let chunk_size = (n + total_ways - 1) / total_ways;
    let mut chunks = rows.chunks_mut(chunk_size);
    let Some(first_chunk) = chunks.next() else {
        return true;
    };
    let mut dispatched = 0usize;
    for (worker, chunk) in pool.workers.iter().zip(chunks) {
        let job = EmbedInPlaceJob {
            dim,
            ptr: chunk.as_mut_ptr() as usize,
            len: chunk.len(),
        };
        if worker.send(job).is_ok() {
            dispatched += 1;
        }
    }
    let dummy = HashingEmbedder {
        id: String::new(),
        dim,
    };
    dummy.embed_rows_slice_in_place(first_chunk);
    for _ in 0..dispatched {
        let _ = pool.done_rx.recv();
    }
    true
}

impl HashingEmbedder {
    fn embed_batch_serial(&self, texts: &[&str]) -> Vec<Arc<[f32]>> {
        EMBED_SCRATCH.with(|cell| {
            let mut guard = cell.borrow_mut();
            let (scratch, lower, _) = &mut *guard;
            scratch.ensure_dim(self.dim);
            let mut out = Vec::with_capacity(texts.len());
            for text in texts {
                self.fill(scratch, crate::fts::lowercased(text, lower));
                out.push(scratch.finish());
            }
            out
        })
    }

    fn embed_rows_slice_in_place(&self, rows: &mut [crate::store::Row]) {
        EMBED_SCRATCH.with(|cell| {
            let mut guard = cell.borrow_mut();
            let (scratch, lower, text_buf) = &mut *guard;
            scratch.ensure_dim(self.dim);
            for row in rows {
                let vec = match prepare_row_embed_text(row, text_buf) {
                    RowEmbedText::Empty => scratch.finish(),
                    RowEmbedText::Borrowed(text) => {
                        self.fill(scratch, crate::fts::lowercased(text, lower));
                        scratch.finish()
                    }
                    RowEmbedText::Appended => {
                        self.fill(scratch, crate::fts::lowercased(text_buf, lower));
                        scratch.finish()
                    }
                };
                if let Some(cell) = row.get_mut("embedding") {
                    *cell = crate::store::Cell::Vec(vec);
                } else {
                    row.insert("embedding".into(), crate::store::Cell::Vec(vec));
                }
            }
        });
    }

    fn embed_rows_serial(&self, rows: &[&crate::store::Row]) -> Vec<Arc<[f32]>> {
        EMBED_SCRATCH.with(|cell| {
            let mut guard = cell.borrow_mut();
            let (scratch, lower, text_buf) = &mut *guard;
            scratch.ensure_dim(self.dim);
            let mut out = Vec::with_capacity(rows.len());
            for row in rows {
                match prepare_row_embed_text(row, text_buf) {
                    RowEmbedText::Empty => {
                        out.push(scratch.finish());
                    }
                    RowEmbedText::Borrowed(text) => {
                        self.fill(scratch, crate::fts::lowercased(text, lower));
                        out.push(scratch.finish());
                    }
                    RowEmbedText::Appended => {
                        self.fill(scratch, crate::fts::lowercased(text_buf, lower));
                        out.push(scratch.finish());
                    }
                }
            }
            out
        })
    }
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
    static EMBED_SCRATCH: std::cell::RefCell<(Scratch, String, String)> = std::cell::RefCell::new((
        Scratch::new(768),
        String::with_capacity(256),
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
        let dim = self.v.len();
        let i = if dim == 768 {
            (h.finish() as usize) % 768
        } else if dim.is_power_of_two() {
            (h.finish() as usize) & (dim - 1)
        } else {
            (h.finish() as usize) % dim
        };
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
///
/// Returns 0.0 for mismatched dimensions, zero norms, or non-finite inputs (NaN, ±inf).
#[inline]
pub fn cosine(a: &[f32], b: &[f32]) -> f64 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    #[cfg(target_arch = "aarch64")]
    {
        return unsafe { cosine_neon(a, b) };
    }
    #[cfg(not(target_arch = "aarch64"))]
    {
        cosine_scalar(a, b)
    }
}

#[cfg(target_arch = "aarch64")]
#[inline]
unsafe fn cosine_neon(a: &[f32], b: &[f32]) -> f64 {
    use std::arch::aarch64::*;
    let n = a.len();
    let chunks = n / 16;
    let ap = a.as_ptr();
    let bp = b.as_ptr();

    let (mut dot, mut na, mut nb) = unsafe {
        let mut d0 = vdupq_n_f32(0.0);
        let mut d1 = vdupq_n_f32(0.0);
        let mut d2 = vdupq_n_f32(0.0);
        let mut d3 = vdupq_n_f32(0.0);

        let mut na0 = vdupq_n_f32(0.0);
        let mut na1 = vdupq_n_f32(0.0);
        let mut na2 = vdupq_n_f32(0.0);
        let mut na3 = vdupq_n_f32(0.0);

        let mut nb0 = vdupq_n_f32(0.0);
        let mut nb1 = vdupq_n_f32(0.0);
        let mut nb2 = vdupq_n_f32(0.0);
        let mut nb3 = vdupq_n_f32(0.0);

        for c in 0..chunks {
            let off = c * 16;
            let va0 = vld1q_f32(ap.add(off));
            let va1 = vld1q_f32(ap.add(off + 4));
            let va2 = vld1q_f32(ap.add(off + 8));
            let va3 = vld1q_f32(ap.add(off + 12));

            let vb0 = vld1q_f32(bp.add(off));
            let vb1 = vld1q_f32(bp.add(off + 4));
            let vb2 = vld1q_f32(bp.add(off + 8));
            let vb3 = vld1q_f32(bp.add(off + 12));

            d0 = vfmaq_f32(d0, va0, vb0);
            d1 = vfmaq_f32(d1, va1, vb1);
            d2 = vfmaq_f32(d2, va2, vb2);
            d3 = vfmaq_f32(d3, va3, vb3);

            na0 = vfmaq_f32(na0, va0, va0);
            na1 = vfmaq_f32(na1, va1, va1);
            na2 = vfmaq_f32(na2, va2, va2);
            na3 = vfmaq_f32(na3, va3, va3);

            nb0 = vfmaq_f32(nb0, vb0, vb0);
            nb1 = vfmaq_f32(nb1, vb1, vb1);
            nb2 = vfmaq_f32(nb2, vb2, vb2);
            nb3 = vfmaq_f32(nb3, vb3, vb3);
        }

        let d_sum = vaddq_f32(vaddq_f32(d0, d1), vaddq_f32(d2, d3));
        let na_sum = vaddq_f32(vaddq_f32(na0, na1), vaddq_f32(na2, na3));
        let nb_sum = vaddq_f32(vaddq_f32(nb0, nb1), vaddq_f32(nb2, nb3));
        (
            vaddvq_f32(d_sum) as f64,
            vaddvq_f32(na_sum) as f64,
            vaddvq_f32(nb_sum) as f64,
        )
    };

    for i in (chunks * 16)..n {
        let x = a[i] as f64;
        let y = b[i] as f64;
        dot += x * y;
        na += x * x;
        nb += y * y;
    }

    if !dot.is_finite() || !na.is_finite() || !nb.is_finite() || na <= 1e-18 || nb <= 1e-18 {
        return 0.0;
    }
    let denom = na.sqrt() * nb.sqrt();
    if denom <= 1e-18 || !denom.is_finite() {
        return 0.0;
    }
    let sim = dot / denom;
    if sim.is_nan() {
        0.0
    } else {
        sim.clamp(-1.0, 1.0)
    }
}

#[cfg(not(target_arch = "aarch64"))]
#[inline]
fn cosine_scalar(a: &[f32], b: &[f32]) -> f64 {
    let n = a.len();
    let chunks = n / 8;
    let mut dot0 = 0.0f32;
    let mut dot1 = 0.0f32;
    let mut dot2 = 0.0f32;
    let mut dot3 = 0.0f32;
    let mut dot4 = 0.0f32;
    let mut dot5 = 0.0f32;
    let mut dot6 = 0.0f32;
    let mut dot7 = 0.0f32;

    let mut na0 = 0.0f32;
    let mut na1 = 0.0f32;
    let mut na2 = 0.0f32;
    let mut na3 = 0.0f32;
    let mut na4 = 0.0f32;
    let mut na5 = 0.0f32;
    let mut na6 = 0.0f32;
    let mut na7 = 0.0f32;

    let mut nb0 = 0.0f32;
    let mut nb1 = 0.0f32;
    let mut nb2 = 0.0f32;
    let mut nb3 = 0.0f32;
    let mut nb4 = 0.0f32;
    let mut nb5 = 0.0f32;
    let mut nb6 = 0.0f32;
    let mut nb7 = 0.0f32;

    for c in 0..chunks {
        let base = c * 8;
        let a0 = a[base];
        let a1 = a[base + 1];
        let a2 = a[base + 2];
        let a3 = a[base + 3];
        let a4 = a[base + 4];
        let a5 = a[base + 5];
        let a6 = a[base + 6];
        let a7 = a[base + 7];

        let b0 = b[base];
        let b1 = b[base + 1];
        let b2 = b[base + 2];
        let b3 = b[base + 3];
        let b4 = b[base + 4];
        let b5 = b[base + 5];
        let b6 = b[base + 6];
        let b7 = b[base + 7];

        dot0 += a0 * b0;
        dot1 += a1 * b1;
        dot2 += a2 * b2;
        dot3 += a3 * b3;
        dot4 += a4 * b4;
        dot5 += a5 * b5;
        dot6 += a6 * b6;
        dot7 += a7 * b7;

        na0 += a0 * a0;
        na1 += a1 * a1;
        na2 += a2 * a2;
        na3 += a3 * a3;
        na4 += a4 * a4;
        na5 += a5 * a5;
        na6 += a6 * a6;
        na7 += a7 * a7;

        nb0 += b0 * b0;
        nb1 += b1 * b1;
        nb2 += b2 * b2;
        nb3 += b3 * b3;
        nb4 += b4 * b4;
        nb5 += b5 * b5;
        nb6 += b6 * b6;
        nb7 += b7 * b7;
    }

    let mut dot = ((dot0 + dot1) + (dot2 + dot3)) as f64 + ((dot4 + dot5) + (dot6 + dot7)) as f64;
    let mut na = ((na0 + na1) + (na2 + na3)) as f64 + ((na4 + na5) + (na6 + na7)) as f64;
    let mut nb = ((nb0 + nb1) + (nb2 + nb3)) as f64 + ((nb4 + nb5) + (nb6 + nb7)) as f64;

    for i in (chunks * 8)..n {
        let x = a[i] as f64;
        let y = b[i] as f64;
        dot += x * y;
        na += x * x;
        nb += y * y;
    }

    if !dot.is_finite() || !na.is_finite() || !nb.is_finite() || na <= 1e-18 || nb <= 1e-18 {
        return 0.0;
    }
    let denom = na.sqrt() * nb.sqrt();
    if denom <= 1e-18 || !denom.is_finite() {
        return 0.0;
    }
    let sim = dot / denom;
    if sim.is_nan() {
        0.0
    } else {
        sim.clamp(-1.0, 1.0)
    }
}

/// Fast cosine similarity when the query vector norm `q_norm = sqrt(sum(qv[i]^2))` is precomputed.
#[inline]
pub fn cosine_with_q_norm(qv: &[f32], q_norm: f64, v: &[f32]) -> f64 {
    if qv.len() != v.len() || qv.is_empty() || q_norm <= 1e-18 || !q_norm.is_finite() {
        return 0.0;
    }
    #[cfg(target_arch = "aarch64")]
    {
        return unsafe { cosine_with_q_norm_neon(qv, q_norm, v) };
    }
    #[cfg(not(target_arch = "aarch64"))]
    {
        cosine_with_q_norm_scalar(qv, q_norm, v)
    }
}

#[cfg(target_arch = "aarch64")]
#[inline]
unsafe fn cosine_with_q_norm_neon(qv: &[f32], q_norm: f64, v: &[f32]) -> f64 {
    use std::arch::aarch64::*;
    let n = qv.len();
    let chunks = n / 16;
    let ap = qv.as_ptr();
    let bp = v.as_ptr();

    let (mut dot, mut nb) = unsafe {
        let mut d0 = vdupq_n_f32(0.0);
        let mut d1 = vdupq_n_f32(0.0);
        let mut d2 = vdupq_n_f32(0.0);
        let mut d3 = vdupq_n_f32(0.0);

        let mut nb0 = vdupq_n_f32(0.0);
        let mut nb1 = vdupq_n_f32(0.0);
        let mut nb2 = vdupq_n_f32(0.0);
        let mut nb3 = vdupq_n_f32(0.0);

        for c in 0..chunks {
            let off = c * 16;
            let va0 = vld1q_f32(ap.add(off));
            let va1 = vld1q_f32(ap.add(off + 4));
            let va2 = vld1q_f32(ap.add(off + 8));
            let va3 = vld1q_f32(ap.add(off + 12));

            let vb0 = vld1q_f32(bp.add(off));
            let vb1 = vld1q_f32(bp.add(off + 4));
            let vb2 = vld1q_f32(bp.add(off + 8));
            let vb3 = vld1q_f32(bp.add(off + 12));

            d0 = vfmaq_f32(d0, va0, vb0);
            d1 = vfmaq_f32(d1, va1, vb1);
            d2 = vfmaq_f32(d2, va2, vb2);
            d3 = vfmaq_f32(d3, va3, vb3);

            nb0 = vfmaq_f32(nb0, vb0, vb0);
            nb1 = vfmaq_f32(nb1, vb1, vb1);
            nb2 = vfmaq_f32(nb2, vb2, vb2);
            nb3 = vfmaq_f32(nb3, vb3, vb3);
        }

        let d_sum = vaddq_f32(vaddq_f32(d0, d1), vaddq_f32(d2, d3));
        let nb_sum = vaddq_f32(vaddq_f32(nb0, nb1), vaddq_f32(nb2, nb3));
        (vaddvq_f32(d_sum) as f64, vaddvq_f32(nb_sum) as f64)
    };

    for i in (chunks * 16)..n {
        let x = qv[i] as f64;
        let y = v[i] as f64;
        dot += x * y;
        nb += y * y;
    }

    if !dot.is_finite() || !nb.is_finite() || nb <= 1e-18 {
        return 0.0;
    }
    let denom = q_norm * nb.sqrt();
    if denom <= 1e-18 || !denom.is_finite() {
        return 0.0;
    }
    let sim = dot / denom;
    if sim.is_nan() {
        0.0
    } else {
        sim.clamp(-1.0, 1.0)
    }
}

#[cfg(not(target_arch = "aarch64"))]
#[inline]
fn cosine_with_q_norm_scalar(qv: &[f32], q_norm: f64, v: &[f32]) -> f64 {
    let n = qv.len();
    let chunks = n / 8;
    let mut dot0 = 0.0f32;
    let mut dot1 = 0.0f32;
    let mut dot2 = 0.0f32;
    let mut dot3 = 0.0f32;
    let mut dot4 = 0.0f32;
    let mut dot5 = 0.0f32;
    let mut dot6 = 0.0f32;
    let mut dot7 = 0.0f32;

    let mut nb0 = 0.0f32;
    let mut nb1 = 0.0f32;
    let mut nb2 = 0.0f32;
    let mut nb3 = 0.0f32;
    let mut nb4 = 0.0f32;
    let mut nb5 = 0.0f32;
    let mut nb6 = 0.0f32;
    let mut nb7 = 0.0f32;

    for c in 0..chunks {
        let base = c * 8;
        let a0 = qv[base];
        let a1 = qv[base + 1];
        let a2 = qv[base + 2];
        let a3 = qv[base + 3];
        let a4 = qv[base + 4];
        let a5 = qv[base + 5];
        let a6 = qv[base + 6];
        let a7 = qv[base + 7];

        let b0 = v[base];
        let b1 = v[base + 1];
        let b2 = v[base + 2];
        let b3 = v[base + 3];
        let b4 = v[base + 4];
        let b5 = v[base + 5];
        let b6 = v[base + 6];
        let b7 = v[base + 7];

        dot0 += a0 * b0;
        dot1 += a1 * b1;
        dot2 += a2 * b2;
        dot3 += a3 * b3;
        dot4 += a4 * b4;
        dot5 += a5 * b5;
        dot6 += a6 * b6;
        dot7 += a7 * b7;

        nb0 += b0 * b0;
        nb1 += b1 * b1;
        nb2 += b2 * b2;
        nb3 += b3 * b3;
        nb4 += b4 * b4;
        nb5 += b5 * b5;
        nb6 += b6 * b6;
        nb7 += b7 * b7;
    }

    let mut dot = ((dot0 + dot1) + (dot2 + dot3)) as f64 + ((dot4 + dot5) + (dot6 + dot7)) as f64;
    let mut nb = ((nb0 + nb1) + (nb2 + nb3)) as f64 + ((nb4 + nb5) + (nb6 + nb7)) as f64;

    for i in (chunks * 8)..n {
        let x = qv[i] as f64;
        let y = v[i] as f64;
        dot += x * y;
        nb += y * y;
    }

    if !dot.is_finite() || !nb.is_finite() || nb <= 1e-18 {
        return 0.0;
    }
    let denom = q_norm * nb.sqrt();
    if denom <= 1e-18 || !denom.is_finite() {
        return 0.0;
    }
    let sim = dot / denom;
    if sim.is_nan() {
        0.0
    } else {
        sim.clamp(-1.0, 1.0)
    }
}

#[inline]
pub(crate) fn l2_norm_f64(v: &[f32]) -> f64 {
    let mut s = 0.0f64;
    for &x in v {
        let xd = x as f64;
        s += xd * xd;
    }
    if s.is_finite() && s > 1e-18 {
        s.sqrt()
    } else {
        0.0
    }
}

/// Candidate in max-heap: highest similarity at the top (popped first).
#[allow(dead_code)]
#[derive(Clone, Copy, Debug)]
struct MaxSimNode {
    sim: f64,
    idx: usize,
}

impl PartialEq for MaxSimNode {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.sim == other.sim && self.idx == other.idx
    }
}

impl Eq for MaxSimNode {}

impl PartialOrd for MaxSimNode {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for MaxSimNode {
    #[inline]
    fn cmp(&self, other: &Self) -> Ordering {
        self.sim
            .partial_cmp(&other.sim)
            .unwrap_or(Ordering::Equal)
            .then_with(|| other.idx.cmp(&self.idx))
    }
}

/// Dynamic result set in min-heap: lowest similarity at the top (evicted first).
#[allow(dead_code)]
#[derive(Clone, Copy, Debug)]
struct MinSimNode {
    sim: f64,
    idx: usize,
}

impl PartialEq for MinSimNode {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.sim == other.sim && self.idx == other.idx
    }
}

impl Eq for MinSimNode {}

impl PartialOrd for MinSimNode {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for MinSimNode {
    #[inline]
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .sim
            .partial_cmp(&self.sim)
            .unwrap_or(Ordering::Equal)
            .then_with(|| self.idx.cmp(&other.idx))
    }
}

/// Scalar element trait for HNSW vector storage (`f32` or quantized `i8`).
pub trait HnswScalar: Copy + Sized + 'static {
    type QueryCtx: Copy;
    fn prepare_query(q: &[Self]) -> Self::QueryCtx;
    fn sim_ctx(q: &[Self], ctx: Self::QueryCtx, b: &[Self]) -> f64;
    fn sim(a: &[Self], b: &[Self]) -> f64;
}

impl HnswScalar for f32 {
    type QueryCtx = f64;
    #[inline]
    fn prepare_query(q: &[Self]) -> f64 {
        l2_norm_f64(q)
    }
    #[inline]
    fn sim_ctx(q: &[Self], q_norm: f64, b: &[Self]) -> f64 {
        cosine_with_q_norm(q, q_norm, b)
    }
    #[inline]
    fn sim(a: &[Self], b: &[Self]) -> f64 {
        cosine(a, b)
    }
}

impl HnswScalar for i8 {
    type QueryCtx = ();
    #[inline]
    fn prepare_query(_q: &[Self]) {}
    #[inline]
    fn sim_ctx(q: &[Self], _ctx: (), b: &[Self]) -> f64 {
        dot_sq8(q, b)
    }
    #[inline]
    fn sim(a: &[Self], b: &[Self]) -> f64 {
        dot_sq8(a, b)
    }
}

struct HnswSearchScratch {
    visited_gen: Vec<u32>,
    cur_gen: u32,
    candidates: BinaryHeap<MaxSimNode>,
    w: BinaryHeap<MinSimNode>,
}

impl HnswSearchScratch {
    fn new() -> Self {
        Self {
            visited_gen: Vec::new(),
            cur_gen: 0,
            candidates: BinaryHeap::new(),
            w: BinaryHeap::new(),
        }
    }

    #[inline]
    fn prepare(&mut self, node_count: usize) {
        if self.visited_gen.len() < node_count {
            self.visited_gen
                .resize(node_count.next_power_of_two().max(64), 0);
        }
        self.cur_gen = self.cur_gen.wrapping_add(1);
        if self.cur_gen == 0 {
            self.visited_gen.fill(0);
            self.cur_gen = 1;
        }
        self.candidates.clear();
        self.w.clear();
    }

    #[inline]
    fn visit(&mut self, idx: usize) -> bool {
        if self.visited_gen[idx] == self.cur_gen {
            false
        } else {
            self.visited_gen[idx] = self.cur_gen;
            true
        }
    }
}

thread_local! {
    static HNSW_SCRATCH: std::cell::RefCell<HnswSearchScratch> =
        std::cell::RefCell::new(HnswSearchScratch::new());
}

#[derive(Clone, Debug)]
struct HnswNode<T> {
    id: usize,
    vector: Arc<[T]>,
    neighbors: Vec<Vec<usize>>,
}

/// Generic Hierarchical Navigable Small World (HNSW) graph.
#[derive(Clone, Debug)]
pub struct HnswGraph<T> {
    nodes: Vec<HnswNode<T>>,
    id_to_idx: FxHashMap<usize, usize>,
    enter_point: Option<usize>,
    max_level: usize,
    m: usize,
    m0: usize,
    ef_construction: usize,
    ml: f64,
}

/// Hierarchical Navigable Small World (HNSW) index for approximate nearest neighbor search.
pub type HnswIndex = HnswGraph<f32>;

/// Hierarchical Navigable Small World (HNSW) index with 8-bit Scalar Quantization (SQ8).
///
/// Reduces vector memory consumption by 75% (1 byte per dimension vs 4 bytes for `f32`)
/// while retaining > 98% ANN recall with integer dot product acceleration.
pub type HnswIndexSq8 = HnswGraph<i8>;

impl<T: HnswScalar> Default for HnswGraph<T> {
    fn default() -> Self {
        Self::new(16, 64)
    }
}

impl<T: HnswScalar> HnswGraph<T> {
    pub fn new(m: usize, ef_construction: usize) -> Self {
        let m = m.max(4);
        let m0 = m * 2;
        let ef_construction = ef_construction.max(m);
        let ml = 1.0 / (m as f64).ln();
        Self {
            nodes: Vec::new(),
            id_to_idx: FxHashMap::default(),
            enter_point: None,
            max_level: 0,
            m,
            m0,
            ef_construction,
            ml,
        }
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.id_to_idx.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.id_to_idx.is_empty()
    }

    /// Estimated resident memory in bytes (nodes, vectors, adjacency graph, and lookup map).
    pub fn resident_memory(&self) -> usize {
        let mut total = std::mem::size_of::<Self>();
        total += self.nodes.capacity() * std::mem::size_of::<HnswNode<T>>();
        for node in &self.nodes {
            total += node.vector.len() * std::mem::size_of::<T>();
            for level in &node.neighbors {
                total += level.capacity() * std::mem::size_of::<usize>();
            }
        }
        total += self.id_to_idx.capacity() * (std::mem::size_of::<usize>() * 2 + 8);
        total
    }

    pub fn remove(&mut self, id: usize) {
        if let Some(node_idx) = self.id_to_idx.remove(&id) {
            self.nodes[node_idx].id = usize::MAX;
            if self.enter_point == Some(node_idx) {
                self.enter_point = self.id_to_idx.values().next().copied();
            }
        }
    }

    pub fn move_id(&mut self, from: usize, to: usize) {
        if from == to {
            return;
        }
        if let Some(node_idx) = self.id_to_idx.remove(&from) {
            self.nodes[node_idx].id = to;
            self.id_to_idx.insert(to, node_idx);
        }
    }

    pub fn clear(&mut self) {
        self.nodes.clear();
        self.id_to_idx.clear();
        self.enter_point = None;
        self.max_level = 0;
    }

    pub fn build(items: impl IntoIterator<Item = (usize, Arc<[T]>)>) -> Self {
        let mut index = Self::default();
        for (id, vec) in items {
            index.insert(id, vec);
        }
        index
    }

    fn random_level(&self, id: usize) -> usize {
        let mut h = FxHasher::default();
        id.hash(&mut h);
        let raw = h.finish() & 0x000f_ffff_ffff_ffff;
        let r = ((raw as f64) + 1.0) / ((0x0010_0000_0000_0000u64) as f64);
        let r = r.clamp(1e-9, 1.0);
        (-r.ln() * self.ml).floor() as usize
    }

    pub fn insert(&mut self, id: usize, vector: Arc<[T]>) {
        if let Some(&existing_idx) = self.id_to_idx.get(&id) {
            self.nodes[existing_idx].vector = vector;
            return;
        }

        let node_level = self.random_level(id);
        let new_idx = self.nodes.len();
        self.id_to_idx.insert(id, new_idx);

        let new_node = HnswNode {
            id,
            vector: Arc::clone(&vector),
            neighbors: vec![Vec::new(); node_level + 1],
        };
        self.nodes.push(new_node);

        let Some(mut curr) = self.enter_point else {
            self.enter_point = Some(new_idx);
            self.max_level = node_level;
            return;
        };

        let curr_max_level = self.max_level;
        let q_ctx = T::prepare_query(&vector);

        // 1. Greedy search from max_level down to node_level + 1
        for level in (node_level + 1..=curr_max_level).rev() {
            let mut curr_sim = T::sim_ctx(&vector, q_ctx, &self.nodes[curr].vector);
            loop {
                let mut changed = false;
                if level < self.nodes[curr].neighbors.len() {
                    for &nbr in &self.nodes[curr].neighbors[level] {
                        let sim = T::sim_ctx(&vector, q_ctx, &self.nodes[nbr].vector);
                        if sim > curr_sim {
                            curr_sim = sim;
                            curr = nbr;
                            changed = true;
                        }
                    }
                }
                if !changed {
                    break;
                }
            }
        }

        // 2. Search layer and connect from min(curr_max_level, node_level) down to 0
        let top_connect_level = curr_max_level.min(node_level);
        let mut entry_points = vec![curr];

        for level in (0..=top_connect_level).rev() {
            let candidates =
                self.search_layer(&vector, q_ctx, &entry_points, self.ef_construction, level);
            let max_conn = if level == 0 { self.m0 } else { self.m };
            let neighbors = self.select_neighbors(&candidates, max_conn);

            self.nodes[new_idx].neighbors[level] = neighbors.clone();

            for &nbr in &neighbors {
                if level < self.nodes[nbr].neighbors.len() {
                    self.nodes[nbr].neighbors[level].push(new_idx);
                    if self.nodes[nbr].neighbors[level].len() > max_conn {
                        self.prune_neighbors(nbr, level, max_conn);
                    }
                }
            }

            entry_points = candidates.into_iter().map(|n| n.idx).collect();
        }

        if node_level > self.max_level {
            self.max_level = node_level;
            self.enter_point = Some(new_idx);
        }
    }

    fn search_layer(
        &self,
        query: &[T],
        q_ctx: T::QueryCtx,
        entry_points: &[usize],
        ef: usize,
        level: usize,
    ) -> Vec<MaxSimNode> {
        HNSW_SCRATCH.with(|cell| {
            let mut scratch = cell.borrow_mut();
            scratch.prepare(self.nodes.len());

            for &ep in entry_points {
                if scratch.visit(ep) {
                    let sim = T::sim_ctx(query, q_ctx, &self.nodes[ep].vector);
                    scratch.candidates.push(MaxSimNode { sim, idx: ep });
                    scratch.w.push(MinSimNode { sim, idx: ep });
                }
            }

            while let Some(c) = scratch.candidates.pop() {
                let worst_in_w = scratch.w.peek().map(|d| d.sim).unwrap_or(f64::NEG_INFINITY);
                if c.sim < worst_in_w && scratch.w.len() >= ef {
                    break;
                }

                if level < self.nodes[c.idx].neighbors.len() {
                    for &nbr in &self.nodes[c.idx].neighbors[level] {
                        if scratch.visit(nbr) {
                            let sim = T::sim_ctx(query, q_ctx, &self.nodes[nbr].vector);
                            let worst_in_w =
                                scratch.w.peek().map(|x| x.sim).unwrap_or(f64::NEG_INFINITY);
                            if sim > worst_in_w || scratch.w.len() < ef {
                                scratch.candidates.push(MaxSimNode { sim, idx: nbr });
                                scratch.w.push(MinSimNode { sim, idx: nbr });
                                if scratch.w.len() > ef {
                                    scratch.w.pop();
                                }
                            }
                        }
                    }
                }
            }

            scratch
                .w
                .drain()
                .map(|n| MaxSimNode {
                    sim: n.sim,
                    idx: n.idx,
                })
                .collect()
        })
    }

    fn select_neighbors(&self, candidates: &[MaxSimNode], max_conn: usize) -> Vec<usize> {
        let mut sorted = candidates.to_vec();
        sorted.sort_by(|a, b| b.sim.partial_cmp(&a.sim).unwrap_or(Ordering::Equal));
        sorted.truncate(max_conn);
        sorted.into_iter().map(|n| n.idx).collect()
    }

    fn prune_neighbors(&mut self, node_idx: usize, level: usize, max_conn: usize) {
        let node_vec = Arc::clone(&self.nodes[node_idx].vector);
        let q_ctx = T::prepare_query(&node_vec);
        let nbrs = &self.nodes[node_idx].neighbors[level];
        let mut scored: Vec<MaxSimNode> = nbrs
            .iter()
            .map(|&nbr| MaxSimNode {
                sim: T::sim_ctx(&node_vec, q_ctx, &self.nodes[nbr].vector),
                idx: nbr,
            })
            .collect();
        scored.sort_by(|a, b| b.sim.partial_cmp(&a.sim).unwrap_or(Ordering::Equal));
        scored.truncate(max_conn);
        self.nodes[node_idx].neighbors[level] = scored.into_iter().map(|d| d.idx).collect();
    }

    fn search_raw(&self, query: &[T], k: usize, ef_search: usize) -> Vec<(f64, usize)> {
        if k == 0 || self.nodes.is_empty() {
            return Vec::new();
        }
        let q_ctx = T::prepare_query(query);

        if self.id_to_idx.len() <= k.max(32) {
            let mut scored: Vec<(f64, usize)> = self
                .nodes
                .iter()
                .filter(|n| n.id != usize::MAX)
                .map(|n| (T::sim_ctx(query, q_ctx, &n.vector), n.id))
                .filter(|(s, _)| *s > 0.0)
                .collect();
            if scored.len() > k {
                scored.select_nth_unstable_by(k - 1, |a, b| {
                    b.0.partial_cmp(&a.0)
                        .unwrap_or(Ordering::Equal)
                        .then_with(|| a.1.cmp(&b.1))
                });
                scored.truncate(k);
            }
            scored.sort_by(|a, b| {
                b.0.partial_cmp(&a.0)
                    .unwrap_or(Ordering::Equal)
                    .then_with(|| a.1.cmp(&b.1))
            });
            return scored;
        }

        let mut curr = match self.enter_point {
            Some(ep) => ep,
            None => return Vec::new(),
        };

        for level in (1..=self.max_level).rev() {
            let mut curr_sim = T::sim_ctx(query, q_ctx, &self.nodes[curr].vector);
            loop {
                let mut changed = false;
                if level < self.nodes[curr].neighbors.len() {
                    for &nbr in &self.nodes[curr].neighbors[level] {
                        let sim = T::sim_ctx(query, q_ctx, &self.nodes[nbr].vector);
                        if sim > curr_sim {
                            curr_sim = sim;
                            curr = nbr;
                            changed = true;
                        }
                    }
                }
                if !changed {
                    break;
                }
            }
        }

        let ef = ef_search.max(k).max(16);
        let candidates = self.search_layer(query, q_ctx, &[curr], ef, 0);

        let mut results: Vec<(f64, usize)> = candidates
            .into_iter()
            .map(|n| (n.sim, self.nodes[n.idx].id))
            .filter(|(s, id)| *s > 0.0 && *id != usize::MAX)
            .collect();

        if results.len() > k {
            results.select_nth_unstable_by(k - 1, |a, b| {
                b.0.partial_cmp(&a.0)
                    .unwrap_or(Ordering::Equal)
                    .then_with(|| a.1.cmp(&b.1))
            });
            results.truncate(k);
        }

        results.sort_by(|a, b| {
            b.0.partial_cmp(&a.0)
                .unwrap_or(Ordering::Equal)
                .then_with(|| a.1.cmp(&b.1))
        });

        results
    }
}

impl HnswGraph<f32> {
    /// Search for the `k` approximate nearest neighbors to `query`.
    /// Returns pairs of `(cosine_similarity, external_id)` sorted descending by similarity.
    pub fn search(&self, query: &[f32], k: usize, ef_search: usize) -> Vec<(f64, usize)> {
        self.search_raw(query, k, ef_search)
    }

    /// Convert this `f32` HNSW index into a compact SQ8-quantized HNSW index.
    ///
    /// Preserves graph topology while reducing vector storage by 75%.
    pub fn to_sq8(&self) -> HnswIndexSq8 {
        let nodes = self
            .nodes
            .iter()
            .map(|n| HnswNode {
                id: n.id,
                vector: quantize_sq8(&n.vector),
                neighbors: n.neighbors.clone(),
            })
            .collect();
        HnswGraph {
            nodes,
            id_to_idx: self.id_to_idx.clone(),
            enter_point: self.enter_point,
            max_level: self.max_level,
            m: self.m,
            m0: self.m0,
            ef_construction: self.ef_construction,
            ml: self.ml,
        }
    }

    /// Convert this `f32` HNSW index into a compact SQ8-quantized HNSW index,
    /// consuming `self` without cloning the graph adjacency lists.
    pub fn into_sq8(self) -> HnswIndexSq8 {
        let nodes = self
            .nodes
            .into_iter()
            .map(|n| HnswNode {
                id: n.id,
                vector: quantize_sq8(&n.vector),
                neighbors: n.neighbors,
            })
            .collect();
        HnswGraph {
            nodes,
            id_to_idx: self.id_to_idx,
            enter_point: self.enter_point,
            max_level: self.max_level,
            m: self.m,
            m0: self.m0,
            ef_construction: self.ef_construction,
            ml: self.ml,
        }
    }
}

/// Quantize an L2-normalized `f32` vector into a caller-provided `i8` slice (`out.len() >= v.len()`).
///
/// Maps `[-1.0, 1.0]` into `[-127, 127]` without heap allocation.
#[inline]
pub fn quantize_sq8_into(v: &[f32], out: &mut [i8]) {
    let n = v.len().min(out.len());
    for i in 0..n {
        out[i] = (v[i] * 127.0).round().clamp(-127.0, 127.0) as i8;
    }
}

/// Quantize an L2-normalized `f32` vector into an 8-bit signed integer vector (`i8`).
///
/// Maps `[-1.0, 1.0]` into `[-127, 127]`.
pub fn quantize_sq8(v: &[f32]) -> Arc<[i8]> {
    let mut quantized = vec![0i8; v.len()];
    quantize_sq8_into(v, &mut quantized);
    quantized.into()
}

/// Dequantize an 8-bit SQ8 vector back into `f32`.
pub fn dequantize_sq8(v: &[i8]) -> Vec<f32> {
    const INV: f32 = 1.0 / 127.0;
    v.iter().map(|&x| (x as f32) * INV).collect()
}

/// Dot product / cosine similarity between two SQ8-quantized vectors.
///
/// Uses ARM NEON integer widening multiply-accumulate on aarch64 and an 8-way
/// unrolled integer accumulator elsewhere. Scaled by `(127.0 * 127.0)^-1`.
#[inline]
pub fn dot_sq8(a: &[i8], b: &[i8]) -> f64 {
    let n = a.len().min(b.len());
    if n == 0 {
        return 0.0;
    }

    #[cfg(target_arch = "aarch64")]
    let total = unsafe { dot_sq8_neon(a, b, n) };

    #[cfg(not(target_arch = "aarch64"))]
    let total = dot_sq8_scalar(a, b, n);

    const SCALE: f64 = 1.0 / (127.0 * 127.0);
    let sim = (total as f64) * SCALE;
    sim.clamp(-1.0, 1.0)
}

#[cfg(target_arch = "aarch64")]
#[inline]
unsafe fn dot_sq8_neon(a: &[i8], b: &[i8], n: usize) -> i32 {
    use std::arch::aarch64::*;
    let chunks = n / 16;
    let ap = a.as_ptr();
    let bp = b.as_ptr();
    let mut total = unsafe {
        let mut acc0 = vdupq_n_s32(0);
        let mut acc1 = vdupq_n_s32(0);
        for c in 0..chunks {
            let off = c * 16;
            let va = vld1q_s8(ap.add(off));
            let vb = vld1q_s8(bp.add(off));
            let prod_lo = vmull_s8(vget_low_s8(va), vget_low_s8(vb));
            let prod_hi = vmull_s8(vget_high_s8(va), vget_high_s8(vb));
            acc0 = vpadalq_s16(acc0, prod_lo);
            acc1 = vpadalq_s16(acc1, prod_hi);
        }
        vaddvq_s32(vaddq_s32(acc0, acc1))
    };
    for i in (chunks * 16)..n {
        total += (a[i] as i32) * (b[i] as i32);
    }
    total
}

#[cfg(not(target_arch = "aarch64"))]
#[inline]
fn dot_sq8_scalar(a: &[i8], b: &[i8], n: usize) -> i32 {
    let chunks = n / 8;
    let mut sum0 = 0i32;
    let mut sum1 = 0i32;
    let mut sum2 = 0i32;
    let mut sum3 = 0i32;

    for i in 0..chunks {
        let base = i * 8;
        sum0 += (a[base] as i32) * (b[base] as i32) + (a[base + 1] as i32) * (b[base + 1] as i32);
        sum1 += (a[base + 2] as i32) * (b[base + 2] as i32)
            + (a[base + 3] as i32) * (b[base + 3] as i32);
        sum2 += (a[base + 4] as i32) * (b[base + 4] as i32)
            + (a[base + 5] as i32) * (b[base + 5] as i32);
        sum3 += (a[base + 6] as i32) * (b[base + 6] as i32)
            + (a[base + 7] as i32) * (b[base + 7] as i32);
    }

    let mut total = (sum0 + sum1) + (sum2 + sum3);
    for i in (chunks * 8)..n {
        total += (a[i] as i32) * (b[i] as i32);
    }
    total
}

impl HnswGraph<i8> {
    pub fn build_f32(items: impl IntoIterator<Item = (usize, Arc<[f32]>)>) -> Self {
        let mut index = Self::default();
        for (id, vec) in items {
            index.insert_f32(id, &vec);
        }
        index
    }

    pub fn insert_f32(&mut self, id: usize, vector: &[f32]) {
        self.insert(id, quantize_sq8(vector));
    }

    /// Search for `k` approximate nearest neighbors with a raw `f32` query vector.
    ///
    /// The query vector is quantized once to SQ8 before traversing the index graph.
    pub fn search(&self, query: &[f32], k: usize, ef_search: usize) -> Vec<(f64, usize)> {
        let q_sq8 = quantize_sq8(query);
        self.search_sq8(&q_sq8, k, ef_search)
    }

    /// Search for `k` approximate nearest neighbors with an already quantized `i8` query vector.
    pub fn search_sq8(&self, query: &[i8], k: usize, ef_search: usize) -> Vec<(f64, usize)> {
        self.search_raw(query, k, ef_search)
    }
}

/// Unified vector index supporting standard uncompressed `f32` (HnswIndex)
/// or memory-optimized 8-bit scalar quantization (HnswIndexSq8).
#[derive(Clone, Debug)]
pub enum VectorIndex {
    F32(HnswIndex),
    Sq8(HnswIndexSq8),
}

impl Default for VectorIndex {
    fn default() -> Self {
        Self::F32(HnswIndex::default())
    }
}

impl VectorIndex {
    pub fn new_f32(m: usize, ef: usize) -> Self {
        Self::F32(HnswIndex::new(m, ef))
    }

    pub fn new_sq8(m: usize, ef: usize) -> Self {
        Self::Sq8(HnswIndexSq8::new(m, ef))
    }

    pub fn is_sq8(&self) -> bool {
        matches!(self, Self::Sq8(_))
    }

    pub fn to_sq8(&self) -> Self {
        match self {
            Self::F32(idx) => Self::Sq8(idx.to_sq8()),
            Self::Sq8(idx) => Self::Sq8(idx.clone()),
        }
    }

    pub fn into_sq8(self) -> Self {
        match self {
            Self::F32(idx) => Self::Sq8(idx.into_sq8()),
            Self::Sq8(idx) => Self::Sq8(idx),
        }
    }

    #[inline]
    pub fn len(&self) -> usize {
        match self {
            Self::F32(idx) => idx.len(),
            Self::Sq8(idx) => idx.len(),
        }
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        match self {
            Self::F32(idx) => idx.is_empty(),
            Self::Sq8(idx) => idx.is_empty(),
        }
    }

    pub fn resident_memory(&self) -> usize {
        match self {
            Self::F32(idx) => idx.resident_memory(),
            Self::Sq8(idx) => idx.resident_memory(),
        }
    }

    pub fn insert(&mut self, id: usize, vector: Arc<[f32]>) {
        match self {
            Self::F32(idx) => idx.insert(id, vector),
            Self::Sq8(idx) => idx.insert_f32(id, &vector),
        }
    }

    pub fn remove(&mut self, id: usize) {
        match self {
            Self::F32(idx) => idx.remove(id),
            Self::Sq8(idx) => idx.remove(id),
        }
    }

    pub fn move_id(&mut self, from: usize, to: usize) {
        match self {
            Self::F32(idx) => idx.move_id(from, to),
            Self::Sq8(idx) => idx.move_id(from, to),
        }
    }

    pub fn clear(&mut self) {
        match self {
            Self::F32(idx) => idx.clear(),
            Self::Sq8(idx) => idx.clear(),
        }
    }

    pub fn search(&self, query: &[f32], k: usize, ef_search: usize) -> Vec<(f64, usize)> {
        match self {
            Self::F32(idx) => idx.search(query, k, ef_search),
            Self::Sq8(idx) => idx.search(query, k, ef_search),
        }
    }
}

pub(crate) enum RowEmbedText<'a> {
    Empty,
    Borrowed(&'a str),
    Appended,
}

/// Append or extract the text blob used for embedding a row (title / body / snippet).
/// Reuses `buf` if multiple fields are present.
pub(crate) fn prepare_row_embed_text<'a>(
    row: &'a crate::store::Row,
    buf: &mut String,
) -> RowEmbedText<'a> {
    let texts = ["title", "body", "snippet"].map(|k| crate::store::row_text(row, k));
    let bytes = texts.iter().flatten().map(|t| t.len()).sum::<usize>();
    if bytes == 0 {
        return RowEmbedText::Empty;
    }
    if let Some(first) = texts.iter().position(|t| t.is_some_and(|s| !s.is_empty()))
        && texts[first + 1..].iter().all(Option::is_none)
    {
        return RowEmbedText::Borrowed(texts[first].unwrap());
    }
    buf.clear();
    buf.reserve(bytes + 2);
    for text in texts.into_iter().flatten() {
        if !buf.is_empty() {
            buf.push(' ');
        }
        buf.push_str(text);
    }
    RowEmbedText::Appended
}

/// Text blob used for embedding a row (title / body / snippet).
pub(crate) fn row_embed_text(row: &crate::store::Row) -> std::borrow::Cow<'_, str> {
    let mut buf = String::new();
    match prepare_row_embed_text(row, &mut buf) {
        RowEmbedText::Empty => std::borrow::Cow::Borrowed(""),
        RowEmbedText::Borrowed(s) => std::borrow::Cow::Borrowed(s),
        RowEmbedText::Appended => std::borrow::Cow::Owned(buf),
    }
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

    #[test]
    fn test_hnsw_empty_and_single() {
        let index = HnswIndex::default();
        assert!(index.is_empty());
        assert_eq!(index.len(), 0);
        assert!(index.search(&[1.0, 0.0, 0.0], 5, 16).is_empty());

        let mut index = HnswIndex::default();
        let v: Arc<[f32]> = Arc::from([1.0, 0.0, 0.0]);
        index.insert(42, Arc::clone(&v));
        assert_eq!(index.len(), 1);
        assert!(!index.is_empty());
        let res = index.search(&[1.0, 0.0, 0.0], 5, 16);
        assert_eq!(res.len(), 1);
        assert_eq!(res[0].1, 42);
        assert!((res[0].0 - 1.0).abs() < 1e-5);
    }

    #[test]
    fn test_hnsw_recall_against_exhaustive() {
        let dim = 32;
        let embedder = HashingEmbedder::new("test/32", dim);
        let items: Vec<(usize, Arc<[f32]>)> = (0..120)
            .map(|i| {
                (
                    i,
                    embedder.embed(&format!("document item {i} with some words and features")),
                )
            })
            .collect();

        let index = HnswIndex::build(items.clone());
        assert_eq!(index.len(), 120);

        for query_id in [0, 15, 42, 99] {
            let q_vec = embedder.embed(&format!("document item {query_id} with query tokens"));
            let k = 5;
            let hnsw_res = index.search(&q_vec, k, 32);

            let mut exhaustive: Vec<(f64, usize)> = items
                .iter()
                .map(|(id, v)| (cosine(&q_vec, v), *id))
                .filter(|(s, _)| *s > 0.0)
                .collect();
            exhaustive.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap().then_with(|| a.1.cmp(&b.1)));
            exhaustive.truncate(k);

            assert!(!hnsw_res.is_empty());
            assert_eq!(hnsw_res[0].1, exhaustive[0].1);
        }
    }

    #[test]
    fn test_sq8_quantization_and_dot_product() {
        let v1 = vec![0.5, -0.5, 0.7071, 0.0, -0.05, 0.8];
        let q1 = quantize_sq8(&v1);
        assert_eq!(q1.len(), v1.len());

        let deq1 = dequantize_sq8(&q1);
        for (&orig, &deq) in v1.iter().zip(&deq1) {
            assert!(
                (orig - deq).abs() < 0.015,
                "Quantization error too large: {orig} vs {deq}"
            );
        }

        // Test dot product accuracy
        let v2 = vec![0.6, -0.4, 0.5, 0.1, -0.1, 0.7];
        let q2 = quantize_sq8(&v2);

        let cos_sim = cosine(&v1, &v2);
        let sq8_sim = dot_sq8(&q1, &q2);
        assert!(
            (cos_sim - sq8_sim).abs() < 0.05,
            "Dot product difference too large: {cos_sim} vs {sq8_sim}"
        );
    }

    #[test]
    fn test_hnsw_sq8_recall_and_conversion() {
        let dim = 32;
        let embedder = HashingEmbedder::new("test/32", dim);
        let items: Vec<(usize, Arc<[f32]>)> = (0..120)
            .map(|i| {
                (
                    i,
                    embedder.embed(&format!("document item {i} with some words and features")),
                )
            })
            .collect();

        // 1. Build HnswIndexSq8 directly
        let sq8_index = HnswIndexSq8::build_f32(items.clone());
        assert_eq!(sq8_index.len(), 120);

        // 2. Convert from f32 HnswIndex
        let f32_index = HnswIndex::build(items.clone());
        let converted_sq8 = f32_index.to_sq8();
        assert_eq!(converted_sq8.len(), 120);

        for query_id in [0, 15, 42, 99] {
            let q_vec = embedder.embed(&format!("document item {query_id} with query tokens"));
            let k = 5;

            let res_sq8 = sq8_index.search(&q_vec, k, 32);
            let res_conv = converted_sq8.search(&q_vec, k, 32);

            let mut exhaustive: Vec<(f64, usize)> = items
                .iter()
                .map(|(id, v)| (cosine(&q_vec, v), *id))
                .filter(|(s, _)| *s > 0.0)
                .collect();
            exhaustive.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap().then_with(|| a.1.cmp(&b.1)));
            exhaustive.truncate(k);

            assert!(!res_sq8.is_empty());
            assert!(!res_conv.is_empty());
            assert_eq!(res_sq8[0].1, exhaustive[0].1);
            assert_eq!(res_conv[0].1, exhaustive[0].1);
        }
    }

    #[test]
    fn test_hnsw_remove_and_move_id() {
        let dim = 16;
        let embedder = HashingEmbedder::new("test/16", dim);
        let items: Vec<(usize, Arc<[f32]>)> = (0..50)
            .map(|i| (i, embedder.embed(&format!("sample document number {i}"))))
            .collect();

        let mut index = HnswIndex::build(items.clone());
        assert_eq!(index.len(), 50);

        // Remove node 10
        index.remove(10);
        assert_eq!(index.len(), 49);

        // Search should not yield node 10
        let q_vec = items[10].1.clone();
        let res = index.search(&q_vec, 10, 32);
        for (_, id) in &res {
            assert_ne!(*id, 10, "removed node 10 should not be returned");
        }

        // Move node 49 to 10 (simulating swap_remove)
        index.move_id(49, 10);
        assert_eq!(index.len(), 49);

        // Search for node 49's vector should now return id 10
        let q_vec_49 = items[49].1.clone();
        let res_49 = index.search(&q_vec_49, 5, 32);
        assert!(!res_49.is_empty());
        assert_eq!(
            res_49[0].1, 10,
            "moved node should be found under its new id"
        );

        // SQ8 index removal and move
        let mut sq8_index = HnswIndexSq8::build_f32(items.clone());
        sq8_index.remove(20);
        assert_eq!(sq8_index.len(), 49);
        let res_sq8 = sq8_index.search(&items[20].1, 5, 32);
        for (_, id) in &res_sq8 {
            assert_ne!(*id, 20);
        }
        sq8_index.move_id(49, 20);
        let res_sq8_49 = sq8_index.search(&items[49].1, 5, 32);
        assert_eq!(res_sq8_49[0].1, 20);
    }
}
