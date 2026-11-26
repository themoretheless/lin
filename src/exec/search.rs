use std::cmp::Reverse;
use std::collections::{BTreeMap, BinaryHeap};
use std::sync::Arc;

use rustc_hash::FxHashMap;

use crate::ast::{Field, Query, SearchMode, Source, Step};
use crate::embed;
use crate::error::Error;
use crate::store::row::FieldSlot;
use crate::store::{Cell, Row, RowProjector, row_text};

use super::eval::{LexQuery, lex_score_prepared};
#[cfg(feature = "gpu")]
use super::gpu_ops::gpu_rrf_keys;
use super::{Db, field_names};

#[derive(Copy, Clone, PartialEq)]
struct MinScoredIdx(f64, usize);

impl Eq for MinScoredIdx {}

impl PartialOrd for MinScoredIdx {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for MinScoredIdx {
    /// Max-heap root is the *worst* retained candidate: lower score first, then larger row index.
    #[inline]
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        other
            .0
            .total_cmp(&self.0)
            .then_with(|| self.1.cmp(&other.1))
    }
}

impl Db {
    /// `col | search lex|hybrid …` — FTS postings → residual score (no full scan).
    pub(super) fn try_fts_search(
        &self,
        q: &Query,
        bindings: &BTreeMap<String, Vec<Row>>,
    ) -> Result<Option<Vec<Row>>, Error> {
        let Source::Collection(name) = &q.source else {
            return Ok(None);
        };
        if bindings.get(name).is_some() {
            return Ok(None);
        }

        let mut mode: Option<SearchMode> = None;
        let mut query: Option<&str> = None;
        let mut skip_n: usize = 0;
        let mut take_n: Option<Option<i64>> = None;
        let mut proj: Option<&[Field]> = None;
        let mut saw_take = false;

        for step in &q.steps {
            match step {
                Step::Search { mode: m, query: qq } => {
                    if mode.is_some() || proj.is_some() || skip_n > 0 || saw_take {
                        return Ok(None);
                    }
                    mode = Some(*m);
                    query = Some(qq.as_str());
                }
                Step::Skip { n } => {
                    if mode.is_none() || saw_take {
                        return Ok(None);
                    }
                    skip_n = skip_n.saturating_add((*n).max(0) as usize);
                }
                Step::Take { n } => {
                    if saw_take {
                        return Ok(None);
                    }
                    saw_take = true;
                    take_n = Some(*n);
                }
                Step::Project(f) => {
                    if proj.is_some() {
                        return Ok(None);
                    }
                    proj = Some(f.as_slice());
                }
                _ => return Ok(None),
            }
        }

        let (Some(mode), Some(query)) = (mode, query) else {
            return Ok(None);
        };

        if matches!(mode, SearchMode::Vec | SearchMode::Hybrid)
            && self.store.embed_id != self.catalog.embed_id
        {
            return Err(Error::runtime(format!(
                "embed_id mismatch: store={} catalog={}",
                self.store.embed_id, self.catalog.embed_id
            )));
        }

        if mode != SearchMode::Vec && !self.store.fts_indexed(name) {
            return Ok(None);
        }

        let limit = match take_n {
            Some(Some(n)) => Some(n.max(0) as usize),
            Some(None) => None,
            None => Some(50),
        };
        let rank_limit = limit.map(|n| n.saturating_add(skip_n));

        if self.store.is_disk_backed(name)
            && let Some(ids) = self.cold_search_ranked_ids(name, mode, query, rank_limit)?
            && let Some(eng) = self.store.disk_engine_read()
        {
            let start = skip_n.min(ids.len());
            let slice = &ids[start..];
            let end = limit.map_or(slice.len(), |n| n.min(slice.len()));
            let names = proj.map(field_names);
            let refs: Option<Vec<&str>> =
                names.as_ref().map(|n| n.iter().map(String::as_str).collect());
            let mut projector = names.as_ref().map(|n| RowProjector::new(n));
            let mut out = Vec::with_capacity(end);
            for id in &slice[..end] {
                let row = match &refs {
                    Some(fs) => eng.get_projected_by_id(name, id, fs)?,
                    None => eng.get_by_id(name, id)?,
                };
                if let Some(row) = row {
                    out.push(match &mut projector {
                        Some(p) => p.project(&row),
                        None => row,
                    });
                }
            }
            return Ok(Some(out));
        }

        let idxs = self.search_fts_indices_ranked(name, mode, query, rank_limit)?;
        let start = skip_n.min(idxs.len());
        let slice = &idxs[start..];
        let end = limit.map_or(slice.len(), |n| n.min(slice.len()));
        let target_idxs = &slice[..end];

        // Direct projected materialization without intermediate full-Row clones!
        if let Some(fields) = proj {
            let names = field_names(fields);
            if name == "docs"
                && let Some(rows) = self.store.project_docs_hot(target_idxs, &names)
            {
                return Ok(Some(rows));
            }
            let col = self.store.collection(name);
            let mut projector = RowProjector::new(&names);
            let mut out = Vec::with_capacity(target_idxs.len());
            for &i in target_idxs {
                if let Some(row) = col.get(i) {
                    out.push(projector.project(row));
                }
            }
            return Ok(Some(out));
        }

        let col = self.store.collection(name);
        let mut out = Vec::with_capacity(target_idxs.len());
        for &i in target_idxs {
            if let Some(row) = col.get(i) {
                out.push(row.clone());
            }
        }
        Ok(Some(out))
    }

    /// Resolve ranked primary-key `id`s for `search vec|lex|hybrid` on a disk-backed
    /// collection without materializing it into memory. Returns `Ok(None)` when the FTS
    /// index is absent or misaligned so the caller can fall back cleanly.
    pub(crate) fn cold_search_ranked_ids(
        &self,
        name: &str,
        mode: SearchMode,
        query: &str,
        rank_limit: Option<usize>,
    ) -> Result<Option<Vec<Arc<str>>>, Error> {
        if matches!(mode, SearchMode::Vec | SearchMode::Hybrid)
            && self.store.embed_id != self.catalog.embed_id
        {
            return Err(Error::runtime(format!(
                "embed_id mismatch: store={} catalog={}",
                self.store.embed_id, self.catalog.embed_id
            )));
        }
        if !self.store.is_disk_backed(name) {
            return Ok(None);
        }
        let Some(eng) = self.store.disk_engine_read() else {
            return Ok(None);
        };
        if mode == SearchMode::Vec {
            let Some(ref emb) = self.embedder else {
                return Ok(Some(Vec::new()));
            };
            let qv = emb.embed(query);
            self.check_embed_dim(emb.as_ref(), &qv)?;
            if qv.is_empty() || rank_limit == Some(0) {
                return Ok(Some(Vec::new()));
            }
            let top_k = rank_limit.unwrap_or_else(|| eng.count_table(name).max(1));
            let hits = eng.search_vectors_out_of_core(
                name,
                "embedding",
                "id",
                &qv,
                top_k,
                true,
                Some(&["id"]),
                None,
                |_| true,
            )?;
            let mut id_slot = FieldSlot::new("id");
            let ids = hits
                .into_iter()
                .filter_map(|(_, row)| id_slot.get(&row).and_then(Cell::text_shared))
                .collect();
            return Ok(Some(ids));
        }

        if !self.store.fts_indexed(name) {
            return Ok(None);
        }
        let Some(fts) = self.store.fts_get_with_engine(name, Some(&eng)) else {
            return Ok(None);
        };
        if rank_limit == Some(0) {
            return Ok(Some(Vec::new()));
        }
        let hybrid = mode == SearchMode::Hybrid;
        let leg_limit = if hybrid {
            rank_limit.map(|k| k.saturating_mul(8).max(128))
        } else {
            rank_limit
        };
        let Some((lex, ids)) = Self::cold_lex_scored(&eng, name, &fts, query, leg_limit, hybrid)?
        else {
            return Ok(None);
        };
        let ranked: Vec<usize> = if hybrid {
            let mut vec_leg: Vec<usize> = Vec::new();
            if let Some(emb) = self.embedder.as_ref() {
                let qv = emb.embed(query);
                self.check_embed_dim(emb.as_ref(), &qv)?;
                if !qv.is_empty() {
                    let top_k = leg_limit.unwrap_or_else(|| eng.count_table(name).max(1));
                    let mut pos_of: FxHashMap<&str, usize> =
                        FxHashMap::with_capacity_and_hasher(ids.len(), Default::default());
                    for (i, id) in ids.iter().enumerate() {
                        pos_of.insert(id.as_ref(), i);
                    }
                    let mut id_slot = FieldSlot::new("id");
                    for (score, row) in eng.search_vectors_out_of_core(
                        name,
                        "embedding",
                        "id",
                        &qv,
                        top_k,
                        true,
                        Some(&["id"]),
                        None,
                        |_| true,
                    )? {
                        if f64::from(score) > 0.01
                            && let Some(&i) = id_slot
                                .get(&row)
                                .and_then(Cell::text)
                                .and_then(|id| pos_of.get(id))
                        {
                            vec_leg.push(i);
                        }
                    }
                }
            }
            const RRF_K: f64 = 60.0;
            let mut scores: FxHashMap<usize, f64> =
                FxHashMap::with_capacity_and_hasher(lex.len() + vec_leg.len(), Default::default());
            for (rank, (_, i)) in lex.iter().enumerate() {
                *scores.entry(*i).or_default() += 1.0 / (RRF_K + rank as f64 + 1.0);
            }
            for (rank, i) in vec_leg.iter().enumerate() {
                *scores.entry(*i).or_default() += 1.0 / (RRF_K + rank as f64 + 1.0);
            }
            let mut merged: Vec<(f64, usize)> =
                scores.into_iter().map(|(i, score)| (score, i)).collect();
            let cmp_rrf = |a: &(f64, usize), b: &(f64, usize)| {
                b.0.partial_cmp(&a.0)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| a.1.cmp(&b.1))
            };
            if let Some(limit) = rank_limit {
                if limit == 0 {
                    return Ok(Some(Vec::new()));
                }
                if merged.len() > limit {
                    merged.select_nth_unstable_by(limit - 1, cmp_rrf);
                    merged.truncate(limit);
                }
            }
            merged.sort_unstable_by(cmp_rrf);
            merged.into_iter().map(|(_, i)| i).collect()
        } else {
            lex.into_iter().map(|(_, i)| i).collect()
        };
        Ok(Some(
            ranked.into_iter().map(|i| Arc::clone(&ids[i])).collect(),
        ))
    }

    /// Lexical leg over a cold collection.
    fn cold_lex_scored(
        eng: &crate::disk::DiskEngine,
        name: &str,
        fts: &crate::fts::FtsIndex,
        query: &str,
        rank_limit: Option<usize>,
        need_ids: bool,
    ) -> Result<Option<(Vec<(i64, usize)>, Vec<Arc<str>>)>, Error> {
        let candidates = fts.candidate_idxs(query);
        let mut ids: Vec<Arc<str>> = if need_ids || !candidates.is_empty() {
            Vec::with_capacity(eng.count_table(name))
        } else {
            Vec::new()
        };
        if need_ids || !candidates.is_empty() {
            let mut id_slot = FieldSlot::new("id");
            eng.scan_projected(name, Some(&["id"]), |row| {
                ids.push(
                    id_slot
                        .get(row)
                        .and_then(Cell::text_shared)
                        .unwrap_or_default(),
                );
                Ok(true)
            })?;
        }
        if candidates.iter().any(|&i| i >= ids.len()) {
            return Ok(None);
        }
        let lex = LexQuery::new(query);
        let mut scored: Vec<(i64, usize)> = Vec::with_capacity(candidates.len());
        for &i in &candidates {
            let Some(row) = eng.get_projected_by_id(name, &ids[i], &["title", "body", "snippet"])?
            else {
                continue;
            };
            let score = lex_score_prepared(&row, &lex);
            if score > 0 {
                scored.push((score, i));
            }
        }
        let cmp_lex = |(sa, ia): &(i64, usize), (sb, ib): &(i64, usize)| {
            sb.cmp(sa).then_with(|| ia.cmp(ib))
        };
        if let Some(k) = rank_limit {
            if k == 0 {
                scored.clear();
            } else if scored.len() > k {
                scored.select_nth_unstable_by(k - 1, cmp_lex);
                scored.truncate(k);
            }
        }
        scored.sort_unstable_by(cmp_lex);
        Ok(Some((scored, ids)))
    }

    pub(crate) fn search_fts_indices_ranked(
        &self,
        collection: &str,
        mode: SearchMode,
        query: &str,
        rank_limit: Option<usize>,
    ) -> Result<Vec<usize>, Error> {
        match mode {
            SearchMode::Lex => Ok(self
                .search_fts_lex_indices(collection, query, rank_limit)?
                .into_iter()
                .map(|(_, i)| i)
                .collect()),
            SearchMode::Vec => {
                let all = self.store.collection(collection);
                Ok(self
                    .vector_search_indices(all, query, rank_limit)?
                    .into_iter()
                    .map(|(_, i)| i)
                    .collect())
            }
            SearchMode::Hybrid => {
                const RRF_K: f64 = 60.0;
                let all = self.store.collection(collection);
                #[cfg(feature = "gpu")]
                let has_gpu = self.gpu.is_some();
                #[cfg(not(feature = "gpu"))]
                let has_gpu = false;
                let leg_limit = if has_gpu {
                    None
                } else {
                    rank_limit.map(|k| k.saturating_mul(8).max(128))
                };
                let lex = self.search_fts_lex_indices(collection, query, leg_limit)?;
                let vec = self.vector_search_indices(all, query, leg_limit)?;
                #[cfg(feature = "gpu")]
                if let Some(gpu) = &self.gpu {
                    let lex_keys: Vec<usize> = lex.iter().map(|&(_, i)| i).collect();
                    let vec_keys: Vec<usize> = vec.iter().map(|&(_, i)| i).collect();
                    let mut ranked = gpu_rrf_keys(gpu, &lex_keys, &vec_keys)?;
                    if let Some(limit) = rank_limit {
                        ranked.truncate(limit);
                    }
                    return Ok(ranked);
                }
                let mut scores: FxHashMap<usize, f64> = FxHashMap::with_capacity_and_hasher(
                    lex.len() + vec.len(),
                    Default::default(),
                );
                for (rank, (_, i)) in lex.iter().enumerate() {
                    *scores.entry(*i).or_default() += 1.0 / (RRF_K + rank as f64 + 1.0);
                }
                for (rank, (_, i)) in vec.iter().enumerate() {
                    *scores.entry(*i).or_default() += 1.0 / (RRF_K + rank as f64 + 1.0);
                }
                let mut ranked: Vec<(f64, usize)> =
                    scores.into_iter().map(|(i, score)| (score, i)).collect();
                let cmp_rrf = |a: &(f64, usize), b: &(f64, usize)| {
                    b.0.partial_cmp(&a.0)
                        .unwrap_or(std::cmp::Ordering::Equal)
                        .then_with(|| a.1.cmp(&b.1))
                };
                if let Some(limit) = rank_limit {
                    if limit == 0 {
                        return Ok(Vec::new());
                    }
                    if ranked.len() > limit {
                        ranked.select_nth_unstable_by(limit - 1, cmp_rrf);
                        ranked.truncate(limit);
                    }
                }
                ranked.sort_unstable_by(cmp_rrf);
                Ok(ranked.into_iter().map(|(_, i)| i).collect())
            }
        }
    }

    pub(super) fn search_fts_lex_indices(
        &self,
        collection: &str,
        query: &str,
        rank_limit: Option<usize>,
    ) -> Result<Vec<(i64, usize)>, Error> {
        if rank_limit == Some(0) {
            return Ok(Vec::new());
        }
        let fts = self
            .store
            .fts_get(collection)
            .expect("checked FTS collection");
        let mut idxs = fts.candidate_idxs(query);
        let col = self.store.collection(collection);
        if collection == "docs" && !self.store.fts_deferred_doc_swaps.is_empty() {
            for &(p, _, _) in &self.store.fts_deferred_doc_swaps {
                if p < col.len() && !idxs.contains(&p) {
                    idxs.push(p);
                }
            }
        }
        let query = LexQuery::new(query);
        idxs.retain(|&i| col.get(i).is_some());
        let gpu_scores = self.gpu_lex_scores(idxs.iter().map(|&i| &col[i]), &query)?;
        #[cfg(feature = "gpu")]
        if let Some(scores) = &gpu_scores {
            let positions = self.gpu.as_ref().expect("GPU scores").rank_i64(scores)?;
            return Ok(positions
                .into_iter()
                .filter(|&position| scores[position] > 0)
                .take(rank_limit.unwrap_or(usize::MAX))
                .map(|position| (scores[position], idxs[position]))
                .collect());
        }

        let mut scored = match rank_limit {
            Some(k) => {
                // The max-heap root is the worst retained result:
                // lower score first, then larger row index. This preserves the
                // old stable ordering (score desc, source row index asc).
                let mut heap: BinaryHeap<(Reverse<i64>, usize)> =
                    BinaryHeap::with_capacity(k.min(idxs.len()).saturating_add(1));
                for (position, i) in idxs.into_iter().enumerate() {
                    let Some(row) = col.get(i) else { continue };
                    let score = gpu_scores.as_ref().map_or_else(
                        || lex_score_prepared(row, &query),
                        |scores| scores[position],
                    );
                    if score == 0 {
                        continue;
                    }
                    if heap.len() == k {
                        if let Some(mut worst) = heap.peek_mut() {
                            if score > worst.0.0 || (score == worst.0.0 && i < worst.1) {
                                *worst = (Reverse(score), i);
                            }
                        }
                    } else {
                        heap.push((Reverse(score), i));
                    }
                }
                heap.into_iter()
                    .map(|(Reverse(score), i)| (score, i))
                    .collect::<Vec<_>>()
            }
            None => idxs
                .into_iter()
                .enumerate()
                .filter_map(|(position, i)| {
                    let row = col.get(i)?;
                    let score = gpu_scores.as_ref().map_or_else(
                        || lex_score_prepared(row, &query),
                        |scores| scores[position],
                    );
                    (score > 0).then_some((score, i))
                })
                .collect(),
        };
        scored.sort_unstable_by(|(sa, ia), (sb, ib)| sb.cmp(sa).then_with(|| ia.cmp(ib)));
        Ok(scored)
    }

    pub(crate) fn vector_search_indices(
        &self,
        rows: &[Row],
        query: &str,
        rank_limit: Option<usize>,
    ) -> Result<Vec<(f64, usize)>, Error> {
        let Some(emb) = self.embedder.as_ref() else {
            return Ok(Vec::new());
        };
        let qv = emb.embed(query);
        self.check_embed_dim(emb.as_ref(), &qv)?;

        // Fast-path: query maintained HNSW index if rows match collection
        for (col, hnsw) in &self.store.hnsw {
            if !hnsw.is_empty() {
                let col_rows = self.store.collection(col);
                if std::ptr::eq(rows.as_ptr(), col_rows.as_ptr())
                    && rows.len() == col_rows.len()
                    && !rows.is_empty()
                {
                    let k = rank_limit.unwrap_or(20).max(1);
                    let ef = (k * 2).max(64);
                    let mut results = hnsw.search(&qv, k, ef);
                    results.retain(|(s, _)| *s > 0.01);
                    return Ok(results);
                }
            }
        }

        #[cfg(feature = "gpu")]
        let has_gpu = self.gpu.is_some();
        #[cfg(not(feature = "gpu"))]
        let has_gpu = false;

        // Streaming top-k heap path on CPU for bounded `rank_limit` without allocating
        // full-table `vectors` and `scored` buffers.
        if !has_gpu
            && rows.len() < 16_384
            && let Some(k) = rank_limit
            && k <= 512
        {
            if k == 0 || rows.is_empty() {
                return Ok(Vec::new());
            }
            let q_norm = qv.iter().map(|&x| (x * x) as f64).sum::<f64>().sqrt();
            if q_norm <= 1e-18 {
                return Ok(Vec::new());
            }
            let mut emb_slot = FieldSlot::new("embedding");
            let mut heap: BinaryHeap<MinScoredIdx> = BinaryHeap::with_capacity(k.min(rows.len()));
            for (i, r) in rows.iter().enumerate() {
                let Some(v) = emb_slot.get(r).and_then(Cell::as_vec) else {
                    continue;
                };
                let score = crate::embed::cosine_with_q_norm(&qv, q_norm, v);
                if score <= 0.01 {
                    continue;
                }
                if heap.len() == k {
                    // Since `i` strictly increases, equal scores always lose tie-break to earlier rows.
                    if let Some(mut worst) = heap.peek_mut()
                        && score > worst.0
                    {
                        *worst = MinScoredIdx(score, i);
                    }
                } else {
                    heap.push(MinScoredIdx(score, i));
                }
            }
            let mut out: Vec<(f64, usize)> = heap.into_iter().map(|m| (m.0, m.1)).collect();
            out.sort_unstable_by(|a, b| b.0.total_cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
            return Ok(out);
        }

        let mut emb_slot = FieldSlot::new("embedding");
        let vectors: Vec<(usize, &[f32])> = rows
            .iter()
            .enumerate()
            .filter_map(|(i, r)| Some((i, emb_slot.get(r)?.as_vec()?)))
            .collect();
        #[cfg(feature = "gpu")]
        let gpu_scores = self
            .gpu
            .as_ref()
            .map(|gpu| {
                let mut g_slot = FieldSlot::new("embedding");
                let shared: Vec<Arc<[f32]>> = rows
                    .iter()
                    .filter_map(|r| match g_slot.get(r) {
                        Some(Cell::Vec(v)) => Some(Arc::clone(v)),
                        _ => None,
                    })
                    .collect();
                self.gpu_cache
                    .lock()
                    .map_err(|_| Error::runtime("GPU cache lock poisoned"))?
                    .score(gpu, &qv, &shared)
            })
            .transpose()?;
        let mut scored = {
            #[cfg(feature = "gpu")]
            if let Some(gpu_scores) = gpu_scores {
                vectors
                    .iter()
                    .enumerate()
                    .filter_map(|(slot, (i, _))| {
                        let score = gpu_scores[slot] as f64;
                        (score > 0.01).then_some((score, *i))
                    })
                    .collect::<Vec<_>>()
            } else {
                Self::score_vectors_cpu(&qv, &vectors)
            }
            #[cfg(not(feature = "gpu"))]
            Self::score_vectors_cpu(&qv, &vectors)
        };
        #[cfg(feature = "gpu")]
        if let Some(gpu) = &self.gpu {
            let keys: Vec<Cell> = scored
                .iter()
                .map(|(score, _)| Cell::Float(*score))
                .collect();
            let references: Vec<&Cell> = keys.iter().collect();
            let mut res: Vec<(f64, usize)> = gpu
                .sort_indices(&references, true)?
                .into_iter()
                .map(|index| scored[index])
                .collect();
            if let Some(limit) = rank_limit {
                res.truncate(limit);
            }
            return Ok(res);
        }
        let cmp_vec = |a: &(f64, usize), b: &(f64, usize)| {
            b.0.partial_cmp(&a.0)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.1.cmp(&b.1))
        };
        if let Some(limit) = rank_limit {
            if limit == 0 {
                return Ok(Vec::new());
            }
            if scored.len() > limit {
                scored.select_nth_unstable_by(limit - 1, cmp_vec);
                scored.truncate(limit);
            }
        }
        scored.sort_unstable_by(cmp_vec);
        Ok(scored)
    }

    fn score_vectors_cpu(qv: &[f32], vectors: &[(usize, &[f32])]) -> Vec<(f64, usize)> {
        let q_norm = qv.iter().map(|&x| (x * x) as f64).sum::<f64>().sqrt();
        if q_norm <= 1e-18 {
            return Vec::new();
        }
        let n = vectors.len();
        #[cfg(feature = "parallel")]
        if n >= 16_384 {
            use rayon::prelude::*;
            let parts: Vec<Vec<(f64, usize)>> = vectors
                .par_chunks(4096)
                .map(|chunk| {
                    let mut local = Vec::with_capacity(chunk.len());
                    for &(i, v) in chunk {
                        let score = crate::embed::cosine_with_q_norm(qv, q_norm, v);
                        if score > 0.01 {
                            local.push((score, i));
                        }
                    }
                    local
                })
                .collect();
            let total: usize = parts.iter().map(Vec::len).sum();
            let mut all = Vec::with_capacity(total);
            for p in parts {
                all.extend(p);
            }
            return all;
        }
        let mut all = Vec::with_capacity(n);
        for &(i, v) in vectors {
            let score = crate::embed::cosine_with_q_norm(qv, q_norm, v);
            if score > 0.01 {
                all.push((score, i));
            }
        }
        all
    }

    fn lex_search_indices(&self, rows: &[Row], query: &str) -> Result<Vec<usize>, Error> {
        let query = LexQuery::new(query);
        let gpu_scores = self.gpu_lex_scores(rows.iter(), &query)?;
        #[cfg(feature = "gpu")]
        if let Some(scores) = &gpu_scores {
            let positions = self.gpu.as_ref().expect("GPU scores").rank_i64(scores)?;
            return Ok(positions.into_iter().filter(|&i| scores[i] > 0).collect());
        }
        let mut scored: Vec<(i64, usize)> = rows
            .iter()
            .enumerate()
            .filter_map(|(i, r)| {
                let s = gpu_scores
                    .as_ref()
                    .map_or_else(|| lex_score_prepared(r, &query), |scores| scores[i]);
                if s > 0 { Some((s, i)) } else { None }
            })
            .collect();
        scored.sort_unstable_by(|(sa, ia), (sb, ib)| sb.cmp(sa).then_with(|| ia.cmp(ib)));
        Ok(scored.into_iter().map(|(_, i)| i).collect())
    }

    pub(super) fn search_filtered_vectors(
        &self,
        collection: &str,
        rows: &[Row],
        query: &str,
        limit: usize,
    ) -> Result<Vec<Row>, Error> {
        if self.store.embed_id != self.catalog.embed_id {
            return Err(Error::runtime(format!(
                "embed_id mismatch: store={} catalog={}",
                self.store.embed_id, self.catalog.embed_id
            )));
        }
        if limit == 0 {
            return Ok(Vec::new());
        }
        let collection_len = self.store.get_col_count(collection);
        // Exact scoring is cheaper for small or highly selective candidate sets.
        if self.store.filtered_hnsw_enabled
            && rows.len() >= 4096
            && rows.len().saturating_mul(8) >= collection_len
            && limit <= 512
            && let Some(index) = self.store.hnsw.get(collection)
            && !index.is_empty()
            && let Some(embedder) = self.embedder.as_ref()
        {
            let mut positions = FxHashMap::with_capacity_and_hasher(rows.len(), Default::default());
            for (input, row) in rows.iter().enumerate() {
                let Some(pos) = row
                    .get("id")
                    .and_then(Cell::text)
                    .and_then(|id| self.store.row_index(collection, id))
                else {
                    break;
                };
                positions.insert(pos, input);
            }
            if positions.len() == rows.len() {
                let vector = embedder.embed(query);
                self.check_embed_dim(embedder.as_ref(), &vector)?;
                let max_candidates = collection_len.min(4096);
                let mut candidates = limit.saturating_mul(2).max(64).min(max_candidates);
                loop {
                    // The traversal budget is separate from the returned candidate count.
                    // A small top-k still needs a broad enough neighborhood on high-dimensional data.
                    let ef = candidates.saturating_mul(4).max(512).min(max_candidates);
                    let hits: Vec<_> = index
                        .search(&vector, candidates, ef)
                        .into_iter()
                        .filter_map(|(score, pos)| {
                            (score > 0.01)
                                .then(|| positions.get(&pos).copied())
                                .flatten()
                        })
                        .take(limit)
                        .collect();
                    if hits.len() == limit {
                        return Ok(hits.into_iter().map(|i| rows[i].clone()).collect());
                    }
                    if candidates == max_candidates {
                        break;
                    }
                    candidates = candidates.saturating_mul(2).min(max_candidates);
                }
            }
        }
        Ok(self
            .vector_search_indices(rows, query, Some(limit))?
            .into_iter()
            .map(|(_, i)| rows[i].clone())
            .collect())
    }

    pub(super) fn search_rows(
        &self,
        rows: &[Row],
        mode: SearchMode,
        query: &str,
    ) -> Result<Vec<Row>, Error> {
        if matches!(mode, SearchMode::Vec | SearchMode::Hybrid)
            && self.store.embed_id != self.catalog.embed_id
        {
            return Err(Error::runtime(format!(
                "embed_id mismatch: store={} catalog={}",
                self.store.embed_id, self.catalog.embed_id
            )));
        }
        match mode {
            SearchMode::Lex => Ok(self
                .lex_search_indices(rows, query)?
                .into_iter()
                .map(|i| rows[i].clone())
                .collect()),

            SearchMode::Vec => Ok(self
                .vector_search_indices(rows, query, None)?
                .into_iter()
                .map(|(_, i)| rows[i].clone())
                .collect()),

            SearchMode::Hybrid => {
                // Zero-Row-clone Reciprocal Rank Fusion over lex + vec indices.
                const RRF_K: f64 = 60.0;
                let lex_idxs = self.lex_search_indices(rows, query)?;
                let vec_scored = self.vector_search_indices(rows, query, None)?;
                let mut id_slot = FieldSlot::new("id");
                let mut uri_slot = FieldSlot::new("uri");

                #[cfg(feature = "gpu")]
                if let Some(gpu) = &self.gpu {
                    let mut lex_keys: Vec<&str> = Vec::with_capacity(lex_idxs.len());
                    let mut vec_keys: Vec<&str> = Vec::with_capacity(vec_scored.len());
                    let mut first_idx: FxHashMap<&str, usize> = FxHashMap::with_capacity_and_hasher(
                        lex_idxs.len() + vec_scored.len(),
                        Default::default(),
                    );
                    for &i in &lex_idxs {
                        let k = id_slot
                            .get(&rows[i])
                            .and_then(Cell::text)
                            .or_else(|| uri_slot.get(&rows[i]).and_then(Cell::text))
                            .unwrap_or("");
                        lex_keys.push(k);
                        first_idx.entry(k).or_insert(i);
                    }
                    for &(_, i) in &vec_scored {
                        let k = id_slot
                            .get(&rows[i])
                            .and_then(Cell::text)
                            .or_else(|| uri_slot.get(&rows[i]).and_then(Cell::text))
                            .unwrap_or("");
                        vec_keys.push(k);
                        first_idx.entry(k).or_insert(i);
                    }
                    return Ok(gpu_rrf_keys(gpu, &lex_keys, &vec_keys)?
                        .into_iter()
                        .filter_map(|id| first_idx.remove(id).map(|i| rows[i].clone()))
                        .collect());
                }

                let mut by_key: FxHashMap<&str, (f64, usize)> = FxHashMap::with_capacity_and_hasher(
                    lex_idxs.len() + vec_scored.len(),
                    Default::default(),
                );
                for (rank, &i) in lex_idxs.iter().enumerate() {
                    let k = id_slot
                        .get(&rows[i])
                        .and_then(Cell::text)
                        .or_else(|| uri_slot.get(&rows[i]).and_then(Cell::text))
                        .unwrap_or("");
                    let entry = by_key.entry(k).or_insert((0.0, i));
                    entry.0 += 1.0 / (RRF_K + rank as f64 + 1.0);
                }
                for (rank, &(_, i)) in vec_scored.iter().enumerate() {
                    let k = id_slot
                        .get(&rows[i])
                        .and_then(Cell::text)
                        .or_else(|| uri_slot.get(&rows[i]).and_then(Cell::text))
                        .unwrap_or("");
                    let entry = by_key.entry(k).or_insert((0.0, i));
                    entry.0 += 1.0 / (RRF_K + rank as f64 + 1.0);
                }
                let mut ranked: Vec<(&str, f64, usize)> =
                    by_key.into_iter().map(|(k, (s, i))| (k, s, i)).collect();
                ranked.sort_unstable_by(|a, b| {
                    b.1.partial_cmp(&a.1)
                        .unwrap_or(std::cmp::Ordering::Equal)
                        .then_with(|| a.0.cmp(b.0))
                });
                Ok(ranked.into_iter().map(|(_, _, i)| rows[i].clone()).collect())
            }
        }
    }

    /// Recompute `embedding` for every row in a collection that has a vec field,
    /// durably logging the updated vectors and `embed_id` to the WAL when persistent.
    pub fn reembed_collection(&mut self, collection: &str) -> Result<usize, Error> {
        if self.follower {
            return Err(Error::runtime("follower is read-only"));
        }
        let Some(emb) = self.embedder.clone() else {
            return Err(Error::runtime(format!("reembed {collection}: no embedder")));
        };
        if self
            .catalog
            .collection(collection)
            .is_none_or(|c| !c.has_vec())
        {
            return Err(Error::runtime(format!(
                "reembed requires embedding: {collection}"
            )));
        }
        let new_id = emb.id().to_string();
        let strict = self.strict_embed_dim;
        let expected_dim = emb.dim();
        self.catalog.set_embed_id(new_id.clone());
        let rows = self.store.collection_mut(collection);
        for row in rows.iter_mut() {
            row.remove("embedding");
        }
        embed_rows_slice_with(emb.as_ref(), rows);
        if strict {
            let mut emb_slot = FieldSlot::new("embedding");
            for row in rows.iter() {
                if let Some(v) = emb_slot.get(row).and_then(Cell::as_vec)
                    && v.len() != expected_dim
                {
                    return Err(Error::runtime(format!(
                        "embedder '{}' dimension mismatch: expected {}, got {}",
                        emb.id(),
                        expected_dim,
                        v.len()
                    )));
                }
            }
        }
        let mut n = 0usize;
        let mut emb_slot = FieldSlot::new("embedding");
        for row in rows.iter_mut() {
            if emb_slot.get(row).and_then(Cell::as_vec).is_some() {
                n += 1;
            } else {
                row.insert("embedding", Cell::Null);
            }
        }
        self.store.embed_id = new_id;
        self.store.rebuild_row_maps();
        if self.store.hnsw.contains_key(collection) {
            self.store.build_hnsw(collection, 16, 64);
        }
        if let Some(p) = self.persist.as_mut() {
            p.catalog_hash = self.catalog.hash_str().to_string();
            let updated_rows: Vec<Row> = self
                .store
                .collection(collection)
                .iter()
                .filter(|r| row_text(r, "id").is_some())
                .cloned()
                .collect();
            if !updated_rows.is_empty() {
                let pack = crate::persist::Pack::Update {
                    collection: collection.to_string(),
                    rows: updated_rows,
                };
                self.store.durable_commit(p, pack)?;
                self.store.r#gen += 1;
            }
            crate::persist::write_head(
                &p.dir,
                &crate::persist::Head {
                    r#gen: self.store.r#gen,
                    catalog_hash: p.catalog_hash.clone(),
                    embed_id: self.store.embed_id.clone(),
                },
            )?;
        }
        self.clear_plan_cache();
        self.reader_cache = None;
        Ok(n)
    }

    #[inline]
    pub(crate) fn check_embed_dim(
        &self,
        emb: &dyn embed::Embedder,
        v: &[f32],
    ) -> Result<(), Error> {
        if self.strict_embed_dim && v.len() != emb.dim() {
            return Err(Error::runtime(format!(
                "embedder '{}' dimension mismatch: expected {}, got {}",
                emb.id(),
                emb.dim(),
                v.len()
            )));
        }
        Ok(())
    }

    pub(crate) fn maybe_embed_rows(
        &self,
        collection: &str,
        rows: &mut [Row],
    ) -> Result<(), Error> {
        let Some(emb) = self.embedder.as_ref() else {
            return Ok(());
        };
        if self
            .catalog
            .collection(collection)
            .is_none_or(|c| !c.has_vec())
        {
            return Ok(());
        }
        embed_rows_slice_with(emb.as_ref(), rows);
        if self.strict_embed_dim {
            let expected = emb.dim();
            let mut emb_slot = FieldSlot::new("embedding");
            for r in rows.iter() {
                if let Some(v) = emb_slot.get(r).and_then(Cell::as_vec)
                    && v.len() != expected
                {
                    return Err(Error::runtime(format!(
                        "embedder '{}' dimension mismatch: expected {expected}, got {}",
                        emb.id(),
                        v.len()
                    )));
                }
            }
        }
        Ok(())
    }
}

fn embed_rows_slice_with(emb: &dyn embed::Embedder, rows: &mut [Row]) {
    if rows.is_empty() {
        return;
    }
    let mut emb_slot = FieldSlot::new("embedding");
    let mut title_slot = FieldSlot::new("title");
    let mut body_slot = FieldSlot::new("body");
    let mut snippet_slot = FieldSlot::new("snippet");

    let mut needs_embed = |r: &Row| -> bool {
        emb_slot.get(r).and_then(Cell::as_vec).is_none()
            && title_slot
                .get(r)
                .or_else(|| body_slot.get(r))
                .or_else(|| snippet_slot.get(r))
                .and_then(Cell::text)
                .is_some_and(|s| !s.is_empty())
    };

    let mut all_need = true;
    let mut row_idxs: Vec<usize> = Vec::new();
    for i in 0..rows.len() {
        if needs_embed(&rows[i]) {
            if !all_need {
                row_idxs.push(i);
            }
        } else if all_need {
            all_need = false;
            row_idxs.reserve(rows.len());
            row_idxs.extend(0..i);
        }
    }

    if all_need && emb.embed_rows_in_place(rows) {
        return;
    }

    let row_ptrs: Vec<&Row> = if all_need {
        row_idxs.extend(0..rows.len());
        rows.iter().collect()
    } else {
        if row_idxs.is_empty() {
            return;
        }
        row_idxs.iter().map(|&i| &rows[i]).collect()
    };

    let mut vectors = emb.embed_rows(&row_ptrs);
    if vectors.len() != row_idxs.len() {
        // Keep third-party embedders with a broken batch implementation
        // from silently leaving a partially embedded slab.
        vectors = row_ptrs
            .iter()
            .map(|r| emb.embed(&embed::row_embed_text(r)))
            .collect();
    }
    for (i, vector) in row_idxs.into_iter().zip(vectors) {
        if let Some(cell) = rows[i].get_mut("embedding") {
            *cell = Cell::Vec(vector);
        } else {
            rows[i].insert("embedding", Cell::Vec(vector));
        }
    }
}
