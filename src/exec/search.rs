use std::cmp::Reverse;
use std::collections::{BTreeMap, BinaryHeap};
#[cfg(feature = "gpu")]
use std::sync::Arc;

use rustc_hash::FxHashMap;

use crate::ast::{Field, Query, SearchMode, Source, Step};
use crate::embed;
use crate::error::Error;
#[cfg(feature = "gpu")]
use crate::store::Cell;
use crate::store::{Row, project_fields, row_text};

use super::eval::{LexQuery, lex_score_prepared, project};
#[cfg(feature = "gpu")]
use super::gpu_ops::gpu_rrf_keys;
use super::{Db, field_names};

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

        if mode != SearchMode::Vec && !self.store.fts.contains_key(name) {
            return Ok(None);
        }

        let limit = match take_n {
            Some(Some(n)) => Some(n.max(0) as usize),
            Some(None) => None,
            None => Some(50),
        };
        let rank_limit = limit.map(|n| n.saturating_add(skip_n));

        if mode == SearchMode::Vec
            && self.store.is_disk_backed(name)
            && let Some(eng) = self.store.disk_engine_read()
        {
            let Some(ref emb) = self.embedder else {
                return Ok(Some(Vec::new()));
            };
            let qv = emb.embed(query);
            if qv.is_empty() {
                return Ok(Some(Vec::new()));
            }
            let top_k = match rank_limit {
                Some(0) => return Ok(Some(Vec::new())),
                Some(k) => k,
                None => eng.count_table(name).max(1),
            };
            let hits = eng.search_vectors_out_of_core(
                name,
                "embedding",
                "id",
                &qv,
                top_k,
                true,
                None,
                None,
                |_| true,
            )?;
            let start = skip_n.min(hits.len());
            let slice = &hits[start..];
            let end = limit.map_or(slice.len(), |n| n.min(slice.len()));
            let mut out = Vec::with_capacity(end);
            if let Some(fields) = proj {
                let names = field_names(fields);
                for (_, row) in &slice[..end] {
                    out.push(project_fields(row, &names));
                }
            } else {
                for (_, row) in &slice[..end] {
                    out.push(row.clone());
                }
            }
            return Ok(Some(out));
        }

        // Direct projected materialization without intermediate full-Row clones!
        if let Some(fields) = proj {
            let names = field_names(fields);
            let idxs = self.search_fts_indices_ranked(name, mode, query, rank_limit)?;
            let col = self.store.collection(name);
            let start = skip_n.min(idxs.len());
            let slice = &idxs[start..];
            let end = limit.map_or(slice.len(), |n| n.min(slice.len()));
            let mut out = Vec::with_capacity(end);
            for &i in &slice[..end] {
                if let Some(row) = col.get(i) {
                    out.push(project_fields(row, &names));
                }
            }
            return Ok(Some(out));
        }

        let mut rows = self.search_fts(name, mode, query, rank_limit)?;
        if skip_n > 0 {
            if skip_n >= rows.len() {
                rows.clear();
            } else {
                rows.drain(0..skip_n);
            }
        }
        if let Some(n) = limit
            && rows.len() > n
        {
            rows.truncate(n);
        }
        if let Some(fields) = proj {
            rows = project(&rows, fields);
        }
        Ok(Some(rows))
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
                let mut scores: FxHashMap<usize, f64> = FxHashMap::default();
                scores.reserve(lex.len() + vec.len());
                for (rank, (_, i)) in lex.iter().enumerate() {
                    *scores.entry(*i).or_default() += 1.0 / (RRF_K + rank as f64 + 1.0);
                }
                for (rank, (_, i)) in vec.iter().enumerate() {
                    *scores.entry(*i).or_default() += 1.0 / (RRF_K + rank as f64 + 1.0);
                }
                let mut ranked: Vec<(f64, usize)> =
                    scores.into_iter().map(|(i, score)| (score, i)).collect();
                if let Some(limit) = rank_limit {
                    if limit == 0 {
                        return Ok(Vec::new());
                    }
                    if ranked.len() > limit {
                        ranked.select_nth_unstable_by(limit - 1, |a, b| {
                            b.0.partial_cmp(&a.0)
                                .unwrap_or(std::cmp::Ordering::Equal)
                                .then_with(|| a.1.cmp(&b.1))
                        });
                        ranked.truncate(limit);
                    }
                }
                ranked.sort_by(|a, b| {
                    b.0.partial_cmp(&a.0)
                        .unwrap_or(std::cmp::Ordering::Equal)
                        .then_with(|| a.1.cmp(&b.1))
                });
                Ok(ranked.into_iter().map(|(_, i)| i).collect())
            }
        }
    }

    pub(crate) fn search_fts(
        &self,
        collection: &str,
        mode: SearchMode,
        query: &str,
        rank_limit: Option<usize>,
    ) -> Result<Vec<Row>, Error> {
        let col = self.store.collection(collection);
        let idxs = self.search_fts_indices_ranked(collection, mode, query, rank_limit)?;
        Ok(idxs
            .into_iter()
            .filter_map(|i| col.get(i).cloned())
            .collect())
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
            .fts
            .get(collection)
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
                    BinaryHeap::with_capacity(k.saturating_add(1));
                for (position, i) in idxs.into_iter().enumerate() {
                    let Some(row) = col.get(i) else { continue };
                    let score = gpu_scores.as_ref().map_or_else(
                        || lex_score_prepared(row, &query),
                        |scores| scores[position],
                    );
                    if score == 0 {
                        continue;
                    }
                    heap.push((Reverse(score), i));
                    if heap.len() > k {
                        heap.pop();
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

        let vectors: Vec<(usize, &[f32])> = rows
            .iter()
            .enumerate()
            .filter_map(|(i, r)| Some((i, r.get("embedding")?.as_vec()?)))
            .collect();
        #[cfg(feature = "gpu")]
        let gpu_scores = self
            .gpu
            .as_ref()
            .map(|gpu| {
                let shared: Vec<Arc<[f32]>> = rows
                    .iter()
                    .filter_map(|r| match r.get("embedding") {
                        Some(Cell::Vec(v)) => Some(v.clone()),
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
        if let Some(limit) = rank_limit {
            if limit == 0 {
                return Ok(Vec::new());
            }
            if scored.len() > limit {
                scored.select_nth_unstable_by(limit - 1, |a, b| {
                    b.0.partial_cmp(&a.0)
                        .unwrap_or(std::cmp::Ordering::Equal)
                        .then_with(|| a.1.cmp(&b.1))
                });
                scored.truncate(limit);
            }
        }
        scored.sort_by(|a, b| {
            b.0.partial_cmp(&a.0)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.1.cmp(&b.1))
        });
        Ok(scored)
    }

    fn score_vectors_cpu(qv: &[f32], vectors: &[(usize, &[f32])]) -> Vec<(f64, usize)> {
        let q_norm = qv.iter().map(|&x| (x * x) as f64).sum::<f64>().sqrt();
        if q_norm <= 1e-18 {
            return Vec::new();
        }
        let n = vectors.len();
        if n >= 8192 {
            let threads = std::thread::available_parallelism()
                .map(|p| p.get())
                .unwrap_or(1)
                .min(8);
            let chunk_size = n.div_ceil(threads);
            std::thread::scope(|scope| {
                let mut handles = Vec::with_capacity(threads);
                for chunk in vectors.chunks(chunk_size) {
                    handles.push(scope.spawn(move || {
                        let mut local = Vec::with_capacity(chunk.len());
                        for &(i, v) in chunk {
                            let score = crate::embed::cosine_with_q_norm(qv, q_norm, v);
                            if score > 0.01 {
                                local.push((score, i));
                            }
                        }
                        local
                    }));
                }
                let mut all = Vec::with_capacity(n);
                for handle in handles {
                    if let Ok(part) = handle.join() {
                        all.extend(part);
                    }
                }
                all
            })
        } else {
            let mut all = Vec::with_capacity(n);
            for &(i, v) in vectors {
                let score = crate::embed::cosine_with_q_norm(qv, q_norm, v);
                if score > 0.01 {
                    all.push((score, i));
                }
            }
            all
        }
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
            SearchMode::Lex => {
                let query = LexQuery::new(query);
                let gpu_scores = self.gpu_lex_scores(rows.iter(), &query)?;
                #[cfg(feature = "gpu")]
                if let Some(scores) = &gpu_scores {
                    let positions = self.gpu.as_ref().expect("GPU scores").rank_i64(scores)?;
                    return Ok(positions
                        .into_iter()
                        .filter(|&i| scores[i] > 0)
                        .map(|i| rows[i].clone())
                        .collect());
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
                scored.sort_by(|(sa, ia), (sb, ib)| sb.cmp(sa).then_with(|| ia.cmp(ib)));
                Ok(scored.into_iter().map(|(_, i)| rows[i].clone()).collect())
            }
            SearchMode::Vec => Ok(self
                .vector_search_indices(rows, query, None)?
                .into_iter()
                .map(|(_, i)| rows[i].clone())
                .collect()),

            SearchMode::Hybrid => {
                // Reciprocal rank fusion over lex + vec lists.
                const RRF_K: f64 = 60.0;
                let lex = self.search_rows(rows, SearchMode::Lex, query)?;
                let vec = self.search_rows(rows, SearchMode::Vec, query)?;
                #[cfg(feature = "gpu")]
                if let Some(gpu) = &self.gpu {
                    let key = |row: &Row| {
                        row_text(row, "id")
                            .or_else(|| row_text(row, "uri"))
                            .unwrap_or("")
                            .to_owned()
                    };
                    let lex_keys: Vec<String> = lex.iter().map(key).collect();
                    let vec_keys: Vec<String> = vec.iter().map(key).collect();
                    let mut by_id: BTreeMap<String, Row> = BTreeMap::new();
                    for (id, row) in lex_keys.iter().zip(&lex).chain(vec_keys.iter().zip(&vec)) {
                        by_id.entry(id.clone()).or_insert_with(|| row.clone());
                    }
                    return Ok(gpu_rrf_keys(gpu, &lex_keys, &vec_keys)?
                        .into_iter()
                        .filter_map(|id| by_id.remove(&id))
                        .collect());
                }
                let mut scores: BTreeMap<String, f64> = BTreeMap::new();
                let mut by_id: BTreeMap<String, Row> = BTreeMap::new();
                for (rank, r) in lex.into_iter().enumerate() {
                    let id = row_text(&r, "id")
                        .or_else(|| row_text(&r, "uri"))
                        .unwrap_or("")
                        .to_string();
                    *scores.entry(id.clone()).or_default() += 1.0 / (RRF_K + rank as f64 + 1.0);
                    by_id.entry(id).or_insert(r);
                }
                for (rank, r) in vec.into_iter().enumerate() {
                    let id = row_text(&r, "id")
                        .or_else(|| row_text(&r, "uri"))
                        .unwrap_or("")
                        .to_string();
                    *scores.entry(id.clone()).or_default() += 1.0 / (RRF_K + rank as f64 + 1.0);
                    by_id.entry(id).or_insert(r);
                }
                let mut ranked: Vec<(f64, Row)> = scores
                    .into_iter()
                    .filter_map(|(id, s)| by_id.remove(&id).map(|r| (s, r)))
                    .collect();
                ranked.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
                Ok(ranked.into_iter().map(|(_, r)| r).collect())
            }
        }
    }

    /// Recompute `embedding` for every row in a collection that has a vec field.
    pub fn reembed_collection(&mut self, collection: &str) -> Result<usize, Error> {
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
        self.store.embed_id = emb.id().to_string();
        self.catalog.set_embed_id(emb.id().to_string());
        let rows = self.store.collection_mut(collection);
        let mut n = 0usize;
        for row in rows.iter_mut() {
            let text = embed::row_embed_text(row);
            if text.is_empty() {
                row.insert("embedding".into(), crate::store::Cell::Null);
                continue;
            }
            row.insert(
                "embedding".into(),
                crate::store::Cell::Vec(emb.embed(&text)),
            );
            n += 1;
        }
        self.store.rebuild_row_maps();
        if self.store.hnsw.contains_key(collection) {
            self.store.build_hnsw(collection, 16, 64);
        }
        self.plan_cache.clear();
        self.query_cache.clear();
        Ok(n)
    }

    pub(crate) fn maybe_embed_rows(&self, collection: &str, rows: &mut [Row]) {
        let Some(emb) = self.embedder.as_ref() else {
            return;
        };
        if self
            .catalog
            .collection(collection)
            .is_none_or(|c| !c.has_vec())
        {
            return;
        }

        let first_needs = rows.first().is_some_and(|r| {
            r.get("embedding")
                .and_then(crate::store::Cell::as_vec)
                .is_none()
                && (r
                    .get("title")
                    .or_else(|| r.get("body"))
                    .or_else(|| r.get("snippet"))
                    .and_then(crate::store::Cell::text)
                    .is_some_and(|s| !s.is_empty()))
        });
        let all_need = rows.len() >= 64
            && first_needs
            && rows.iter().all(|r| {
                r.get("embedding")
                    .and_then(crate::store::Cell::as_vec)
                    .is_none()
                    && (r
                        .get("title")
                        .or_else(|| r.get("body"))
                        .or_else(|| r.get("snippet"))
                        .and_then(crate::store::Cell::text)
                        .is_some_and(|s| !s.is_empty()))
            });
        if all_need && emb.embed_rows_in_place(rows) {
            return;
        }
        let mut row_idxs = Vec::with_capacity(rows.len());
        let mut row_ptrs = Vec::with_capacity(rows.len());
        if all_need {
            row_idxs.extend(0..rows.len());
            row_ptrs.extend(rows.iter());
        } else {
            for (i, row) in rows.iter().enumerate() {
                if row
                    .get("embedding")
                    .and_then(crate::store::Cell::as_vec)
                    .is_some()
                {
                    continue;
                }
                let has_text = row
                    .get("title")
                    .or_else(|| row.get("body"))
                    .or_else(|| row.get("snippet"))
                    .and_then(crate::store::Cell::text)
                    .is_some_and(|s| !s.is_empty());
                if has_text {
                    row_idxs.push(i);
                    row_ptrs.push(row);
                }
            }
        }
        if row_ptrs.is_empty() {
            return;
        }
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
                *cell = crate::store::Cell::Vec(vector);
            } else {
                rows[i].insert("embedding".into(), crate::store::Cell::Vec(vector));
            }
        }
    }
}
