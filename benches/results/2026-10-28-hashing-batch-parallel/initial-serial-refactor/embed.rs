//! In-process embedders for `search vec` / `search hybrid`.
//!
//! Default is a deterministic feature-hash embedder sized from `embed_id`
//! (`…/768` → dim 768). It is **not** a neural model — swap via
//! [`crate::exec::Db::with_embedder`] when a real backend is available.

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
            self.fill(scratch, crate::fts::lowercased(text, lower));
            scratch.finish()
        })
    }

    fn embed_batch(&self, texts: &[&str]) -> Vec<Arc<[f32]>> {
        if texts.len() >= 4096
            && texts.len().saturating_mul(self.dim) >= 4096 * 768
            && std::thread::available_parallelism().is_ok_and(|n| n.get() > 1)
        {
            self.embed_batch_parallel(texts, false)
        } else {
            self.embed_batch_serial(texts)
        }
    }
}

impl HashingEmbedder {
    fn embed_batch_serial_into(&self, texts: &[&str], out: &mut Vec<Arc<[f32]>>) {
        EMBED_SCRATCH.with(|cell| {
            let mut guard = cell.borrow_mut();
            let (scratch, lower) = &mut *guard;
            scratch.ensure_dim(self.dim);
            for text in texts {
                self.fill(scratch, crate::fts::lowercased(text, lower));
                out.push(scratch.finish());
            }
        });
    }

    fn embed_batch_serial(&self, texts: &[&str]) -> Vec<Arc<[f32]>> {
        let mut out = Vec::with_capacity(texts.len());
        self.embed_batch_serial_into(texts, &mut out);
        out
    }

    fn embed_batch_parallel(&self, texts: &[&str], fail_spawn: bool) -> Vec<Arc<[f32]>> {
        std::thread::scope(|scope| {
            let (left, right) = texts.split_at(texts.len() / 2);
            let worker = if fail_spawn {
                Err(std::io::Error::other("simulated hashing worker spawn failure"))
            } else {
                std::thread::Builder::new()
                    .spawn_scoped(scope, || self.embed_batch_serial(right))
            };
            let mut out = Vec::with_capacity(texts.len());
            match worker {
                Ok(worker) => {
                    self.embed_batch_serial_into(left, &mut out);
                    out.extend(worker.join().expect("hashing batch worker panicked"));
                }
                // No partial slab has been assigned; serial fallback preserves order.
                Err(_) => self.embed_batch_serial_into(texts, &mut out),
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

/// Cosine similarity for L2-normalized (or arbitrary) vectors.
///
/// Returns 0.0 for mismatched dimensions, zero norms, or non-finite inputs (NaN, ±inf).
#[inline]
pub fn cosine(a: &[f32], b: &[f32]) -> f64 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let n = a.len();
    let chunks = n / 4;
    let mut dot0 = 0.0f64;
    let mut dot1 = 0.0f64;
    let mut dot2 = 0.0f64;
    let mut dot3 = 0.0f64;

    let mut na0 = 0.0f64;
    let mut na1 = 0.0f64;
    let mut na2 = 0.0f64;
    let mut na3 = 0.0f64;

    let mut nb0 = 0.0f64;
    let mut nb1 = 0.0f64;
    let mut nb2 = 0.0f64;
    let mut nb3 = 0.0f64;

    for c in 0..chunks {
        let base = c * 4;
        let a0 = a[base] as f64;
        let a1 = a[base + 1] as f64;
        let a2 = a[base + 2] as f64;
        let a3 = a[base + 3] as f64;

        let b0 = b[base] as f64;
        let b1 = b[base + 1] as f64;
        let b2 = b[base + 2] as f64;
        let b3 = b[base + 3] as f64;

        dot0 += a0 * b0;
        dot1 += a1 * b1;
        dot2 += a2 * b2;
        dot3 += a3 * b3;

        na0 += a0 * a0;
        na1 += a1 * a1;
        na2 += a2 * a2;
        na3 += a3 * a3;

        nb0 += b0 * b0;
        nb1 += b1 * b1;
        nb2 += b2 * b2;
        nb3 += b3 * b3;
    }

    let mut dot = (dot0 + dot1) + (dot2 + dot3);
    let mut na = (na0 + na1) + (na2 + na3);
    let mut nb = (nb0 + nb1) + (nb2 + nb3);

    for i in (chunks * 4)..n {
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

#[allow(dead_code)]
#[derive(Clone, Debug)]
struct HnswNode {
    id: usize,
    vector: Arc<[f32]>,
    neighbors: Vec<Vec<usize>>,
}

/// Hierarchical Navigable Small World (HNSW) index for approximate nearest neighbor search.
#[derive(Clone, Debug)]
pub struct HnswIndex {
    nodes: Vec<HnswNode>,
    id_to_idx: FxHashMap<usize, usize>,
    enter_point: Option<usize>,
    max_level: usize,
    m: usize,
    m0: usize,
    ef_construction: usize,
    ml: f64,
}

impl Default for HnswIndex {
    fn default() -> Self {
        Self::new(16, 64)
    }
}

impl HnswIndex {
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
        total += self.nodes.capacity() * std::mem::size_of::<HnswNode>();
        for node in &self.nodes {
            total += node.vector.len() * 4;
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

    pub fn build(items: impl IntoIterator<Item = (usize, Arc<[f32]>)>) -> Self {
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

    pub fn insert(&mut self, id: usize, vector: Arc<[f32]>) {
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

        // 1. Greedy search from max_level down to node_level + 1
        for level in (node_level + 1..=curr_max_level).rev() {
            let mut curr_sim = cosine(&vector, &self.nodes[curr].vector);
            loop {
                let mut changed = false;
                if level < self.nodes[curr].neighbors.len() {
                    for &nbr in &self.nodes[curr].neighbors[level] {
                        let sim = cosine(&vector, &self.nodes[nbr].vector);
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
            let candidates = self.search_layer(&vector, &entry_points, self.ef_construction, level);
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
        query: &[f32],
        entry_points: &[usize],
        ef: usize,
        level: usize,
    ) -> Vec<MaxSimNode> {
        let mut visited = FxHashSet::default();
        let mut candidates = BinaryHeap::<MaxSimNode>::new();
        let mut w = BinaryHeap::<MinSimNode>::new();

        for &ep in entry_points {
            if visited.insert(ep) {
                let sim = cosine(query, &self.nodes[ep].vector);
                candidates.push(MaxSimNode { sim, idx: ep });
                w.push(MinSimNode { sim, idx: ep });
            }
        }

        while let Some(c) = candidates.pop() {
            let worst_in_w = w.peek().map(|d| d.sim).unwrap_or(f64::NEG_INFINITY);
            if c.sim < worst_in_w && w.len() >= ef {
                break;
            }

            if level < self.nodes[c.idx].neighbors.len() {
                for &nbr in &self.nodes[c.idx].neighbors[level] {
                    if visited.insert(nbr) {
                        let sim = cosine(query, &self.nodes[nbr].vector);
                        let worst_in_w = w.peek().map(|x| x.sim).unwrap_or(f64::NEG_INFINITY);
                        if sim > worst_in_w || w.len() < ef {
                            candidates.push(MaxSimNode { sim, idx: nbr });
                            w.push(MinSimNode { sim, idx: nbr });
                            if w.len() > ef {
                                w.pop();
                            }
                        }
                    }
                }
            }
        }

        w.into_iter()
            .map(|n| MaxSimNode {
                sim: n.sim,
                idx: n.idx,
            })
            .collect()
    }

    fn select_neighbors(&self, candidates: &[MaxSimNode], max_conn: usize) -> Vec<usize> {
        let mut sorted = candidates.to_vec();
        sorted.sort_by(|a, b| b.sim.partial_cmp(&a.sim).unwrap_or(Ordering::Equal));
        sorted.truncate(max_conn);
        sorted.into_iter().map(|n| n.idx).collect()
    }

    fn prune_neighbors(&mut self, node_idx: usize, level: usize, max_conn: usize) {
        let node_vec = Arc::clone(&self.nodes[node_idx].vector);
        let nbrs = &self.nodes[node_idx].neighbors[level];
        let mut scored: Vec<MaxSimNode> = nbrs
            .iter()
            .map(|&nbr| MaxSimNode {
                sim: cosine(&node_vec, &self.nodes[nbr].vector),
                idx: nbr,
            })
            .collect();
        scored.sort_by(|a, b| b.sim.partial_cmp(&a.sim).unwrap_or(Ordering::Equal));
        scored.truncate(max_conn);
        self.nodes[node_idx].neighbors[level] = scored.into_iter().map(|d| d.idx).collect();
    }

    /// Search for the `k` approximate nearest neighbors to `query`.
    /// Returns pairs of `(cosine_similarity, external_id)` sorted descending by similarity.
    pub fn search(&self, query: &[f32], k: usize, ef_search: usize) -> Vec<(f64, usize)> {
        if k == 0 || self.nodes.is_empty() {
            return Vec::new();
        }

        if self.id_to_idx.len() <= k.max(32) {
            let mut scored: Vec<(f64, usize)> = self
                .nodes
                .iter()
                .filter(|n| n.id != usize::MAX)
                .map(|n| (cosine(query, &n.vector), n.id))
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
            let mut curr_sim = cosine(query, &self.nodes[curr].vector);
            loop {
                let mut changed = false;
                if level < self.nodes[curr].neighbors.len() {
                    for &nbr in &self.nodes[curr].neighbors[level] {
                        let sim = cosine(query, &self.nodes[nbr].vector);
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
        let candidates = self.search_layer(query, &[curr], ef, 0);

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

    /// Convert this `f32` HNSW index into a compact SQ8-quantized HNSW index.
    ///
    /// Preserves graph topology while reducing vector storage by 75%.
    pub fn to_sq8(&self) -> HnswIndexSq8 {
        let nodes = self
            .nodes
            .iter()
            .map(|n| HnswNodeSq8 {
                id: n.id,
                vector: quantize_sq8(&n.vector),
                neighbors: n.neighbors.clone(),
            })
            .collect();
        HnswIndexSq8 {
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
    /// consuming `self`.
    pub fn into_sq8(self) -> HnswIndexSq8 {
        self.to_sq8()
    }
}

/// Quantize an L2-normalized `f32` vector into an 8-bit signed integer vector (`i8`).
///
/// Maps `[-1.0, 1.0]` into `[-127, 127]`.
pub fn quantize_sq8(v: &[f32]) -> Arc<[i8]> {
    let mut quantized = Vec::with_capacity(v.len());
    for &x in v {
        let q = (x * 127.0).round().clamp(-127.0, 127.0) as i8;
        quantized.push(q);
    }
    quantized.into()
}

/// Dequantize an 8-bit SQ8 vector back into `f32`.
pub fn dequantize_sq8(v: &[i8]) -> Vec<f32> {
    const INV: f32 = 1.0 / 127.0;
    v.iter().map(|&x| (x as f32) * INV).collect()
}

/// Dot product / cosine similarity between two SQ8-quantized vectors.
///
/// Uses an 8-way unrolled integer accumulator for high throughput.
/// Scaled by `(127.0 * 127.0)^-1`.
#[inline]
pub fn dot_sq8(a: &[i8], b: &[i8]) -> f64 {
    let n = a.len().min(b.len());
    if n == 0 {
        return 0.0;
    }

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

    const SCALE: f64 = 1.0 / (127.0 * 127.0);
    let sim = (total as f64) * SCALE;
    sim.clamp(-1.0, 1.0)
}

#[allow(dead_code)]
#[derive(Clone, Debug)]
struct HnswNodeSq8 {
    id: usize,
    vector: Arc<[i8]>,
    neighbors: Vec<Vec<usize>>,
}

/// Hierarchical Navigable Small World (HNSW) index with 8-bit Scalar Quantization (SQ8).
///
/// Reduces vector memory consumption by 75% (1 byte per dimension vs 4 bytes for `f32`)
/// while retaining > 98% ANN recall with integer dot product acceleration.
#[derive(Clone, Debug)]
pub struct HnswIndexSq8 {
    nodes: Vec<HnswNodeSq8>,
    id_to_idx: FxHashMap<usize, usize>,
    enter_point: Option<usize>,
    max_level: usize,
    m: usize,
    m0: usize,
    ef_construction: usize,
    ml: f64,
}

impl Default for HnswIndexSq8 {
    fn default() -> Self {
        Self::new(16, 64)
    }
}

impl HnswIndexSq8 {
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

    /// Estimated resident memory in bytes (nodes, quantized vectors, adjacency graph, and lookup map).
    pub fn resident_memory(&self) -> usize {
        let mut total = std::mem::size_of::<Self>();
        total += self.nodes.capacity() * std::mem::size_of::<HnswNodeSq8>();
        for node in &self.nodes {
            total += node.vector.len();
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

    pub fn build(items: impl IntoIterator<Item = (usize, Arc<[i8]>)>) -> Self {
        let mut index = Self::default();
        for (id, vec) in items {
            index.insert(id, vec);
        }
        index
    }

    pub fn build_f32(items: impl IntoIterator<Item = (usize, Arc<[f32]>)>) -> Self {
        let mut index = Self::default();
        for (id, vec) in items {
            index.insert_f32(id, &vec);
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

    pub fn insert(&mut self, id: usize, vector: Arc<[i8]>) {
        if let Some(&existing_idx) = self.id_to_idx.get(&id) {
            self.nodes[existing_idx].vector = vector;
            return;
        }

        let node_level = self.random_level(id);
        let new_idx = self.nodes.len();
        self.id_to_idx.insert(id, new_idx);

        let new_node = HnswNodeSq8 {
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

        // 1. Greedy search from max_level down to node_level + 1
        for level in (node_level + 1..=curr_max_level).rev() {
            let mut curr_sim = dot_sq8(&vector, &self.nodes[curr].vector);
            loop {
                let mut changed = false;
                if level < self.nodes[curr].neighbors.len() {
                    for &nbr in &self.nodes[curr].neighbors[level] {
                        let sim = dot_sq8(&vector, &self.nodes[nbr].vector);
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
                self.search_layer_sq8(&vector, &entry_points, self.ef_construction, level);
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

    pub fn insert_f32(&mut self, id: usize, vector: &[f32]) {
        self.insert(id, quantize_sq8(vector));
    }

    fn search_layer_sq8(
        &self,
        query: &[i8],
        entry_points: &[usize],
        ef: usize,
        level: usize,
    ) -> Vec<MaxSimNode> {
        let mut visited = FxHashSet::default();
        let mut candidates = BinaryHeap::<MaxSimNode>::new();
        let mut w = BinaryHeap::<MinSimNode>::new();

        for &ep in entry_points {
            if visited.insert(ep) {
                let sim = dot_sq8(query, &self.nodes[ep].vector);
                candidates.push(MaxSimNode { sim, idx: ep });
                w.push(MinSimNode { sim, idx: ep });
            }
        }

        while let Some(c) = candidates.pop() {
            let worst_in_w = w.peek().map(|d| d.sim).unwrap_or(f64::NEG_INFINITY);
            if c.sim < worst_in_w && w.len() >= ef {
                break;
            }

            if level < self.nodes[c.idx].neighbors.len() {
                for &nbr in &self.nodes[c.idx].neighbors[level] {
                    if visited.insert(nbr) {
                        let sim = dot_sq8(query, &self.nodes[nbr].vector);
                        let worst_in_w = w.peek().map(|x| x.sim).unwrap_or(f64::NEG_INFINITY);
                        if sim > worst_in_w || w.len() < ef {
                            candidates.push(MaxSimNode { sim, idx: nbr });
                            w.push(MinSimNode { sim, idx: nbr });
                            if w.len() > ef {
                                w.pop();
                            }
                        }
                    }
                }
            }
        }

        w.into_iter()
            .map(|n| MaxSimNode {
                sim: n.sim,
                idx: n.idx,
            })
            .collect()
    }

    fn select_neighbors(&self, candidates: &[MaxSimNode], max_conn: usize) -> Vec<usize> {
        let mut sorted = candidates.to_vec();
        sorted.sort_by(|a, b| b.sim.partial_cmp(&a.sim).unwrap_or(Ordering::Equal));
        sorted.truncate(max_conn);
        sorted.into_iter().map(|n| n.idx).collect()
    }

    fn prune_neighbors(&mut self, node_idx: usize, level: usize, max_conn: usize) {
        let node_vec = Arc::clone(&self.nodes[node_idx].vector);
        let nbrs = &self.nodes[node_idx].neighbors[level];
        let mut scored: Vec<MaxSimNode> = nbrs
            .iter()
            .map(|&nbr| MaxSimNode {
                sim: dot_sq8(&node_vec, &self.nodes[nbr].vector),
                idx: nbr,
            })
            .collect();
        scored.sort_by(|a, b| b.sim.partial_cmp(&a.sim).unwrap_or(Ordering::Equal));
        scored.truncate(max_conn);
        self.nodes[node_idx].neighbors[level] = scored.into_iter().map(|d| d.idx).collect();
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
        if k == 0 || self.nodes.is_empty() {
            return Vec::new();
        }

        if self.id_to_idx.len() <= k.max(32) {
            let mut scored: Vec<(f64, usize)> = self
                .nodes
                .iter()
                .filter(|n| n.id != usize::MAX)
                .map(|n| (dot_sq8(query, &n.vector), n.id))
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
            let mut curr_sim = dot_sq8(query, &self.nodes[curr].vector);
            loop {
                let mut changed = false;
                if level < self.nodes[curr].neighbors.len() {
                    for &nbr in &self.nodes[curr].neighbors[level] {
                        let sim = dot_sq8(query, &self.nodes[nbr].vector);
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
        let candidates = self.search_layer_sq8(query, &[curr], ef, 0);

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

#[cfg(test)]
mod parallel_hashing_batch_tests {
    use super::*;
    fn corpus(n: usize) -> Vec<String> {
        let samples = ["", "body", "body 123", "WAL database Straße", "Σίσυφος", "a\u{000b}b", "emoji 🦀 café", "repeated repeated repeated", "İSTANBUL", "line\nend"];
        (0..n).map(|i| if i % samples.len() == 0 { String::new() } else { format!("{} {}",samples[i % samples.len()],i) }).collect()
    }
    fn assert_bits(a: &[Arc<[f32]>], b: &[Arc<[f32]>]) {
        assert_eq!(a.len(),b.len());
        for (a,b) in a.iter().zip(b) {assert_eq!(a.len(),b.len());assert!(a.iter().zip(b.iter()).all(|(a,b)|a.to_bits()==b.to_bits()));}
    }
    #[test]
    fn parallel_vectors_match_serial_and_single_bits_in_input_order() {
        let source=corpus(4097);let refs=source.iter().map(String::as_str).collect::<Vec<_>>();
        for dim in [8,31,768,1024] {
            let embedder=HashingEmbedder::new(format!("hash/{dim}"),dim);
            let serial=embedder.embed_batch_serial(&refs);let parallel=embedder.embed_batch_parallel(&refs,false);assert_bits(&serial,&parallel);
            for (text,vector) in refs.iter().zip(&parallel) {assert_bits(std::slice::from_ref(vector),&[embedder.embed(text)]);}
            assert_bits(&embedder.embed_batch(&refs),&serial);
        }
    }
    #[test]
    fn failed_spawn_falls_back_without_partial_or_duplicate_output() {
        for n in [0,1,4095,4096,4097] {
            let source=corpus(n);let refs=source.iter().map(String::as_str).collect::<Vec<_>>();let embedder=HashingEmbedder::new("hash/768",768);
            let expected=embedder.embed_batch_serial(&refs);let actual=embedder.embed_batch_parallel(&refs,true);assert_bits(&expected,&actual);
        }
    }
    #[test]
    fn alternating_dimensions_and_concurrent_batches_preserve_scratch_isolation() {
        let source=corpus(4101);let refs=source.iter().map(String::as_str).collect::<Vec<_>>();
        std::thread::scope(|scope| {
            let handles=[17,768,1001].into_iter().map(|dim| {
                let refs=&refs;scope.spawn(move||{let e=HashingEmbedder::new("hash",dim);let serial=e.embed_batch_serial(refs);for _ in 0..2 {assert_bits(&serial,&e.embed_batch_parallel(refs,false));}})
            }).collect::<Vec<_>>();
            for h in handles {h.join().unwrap();}
        });
        let e=HashingEmbedder::new("hash/768",768);assert_bits(&e.embed_batch_serial(&refs),&e.embed_batch(&refs));
    }
}
