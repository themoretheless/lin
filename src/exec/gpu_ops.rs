#[cfg(feature = "gpu")]
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::sync::Arc;

#[cfg(feature = "gpu")]
use rustc_hash::FxHashMap;

#[cfg(feature = "gpu")]
use crate::ast::{CmpOp, Field, MatchHop, Pred};
use crate::ast::{Query, Step};
use crate::batch::RecordBatch;
use crate::error::Error;
#[cfg(feature = "gpu")]
use crate::store::Cell;
use crate::store::Row;

#[cfg(feature = "gpu")]
use super::agg::{cell_key, hits_row};
#[cfg(feature = "gpu")]
use super::eval::{cmp_sort, eval_pred, field_cell, value_cell};
use super::eval::LexQuery;
#[cfg(feature = "gpu")]
use super::{edge_row, merge_join_row};
use super::{Db, field_names, push_project_batch_row, rows_to_rough_batch};
#[cfg(feature = "gpu")]
use crate::store::row_text;

impl Db {
    pub(super) fn uses_gpu_filter(&self, query: &Query) -> bool {
        #[cfg(feature = "gpu")]
        {
            self.gpu.is_some()
                && query
                    .steps
                    .iter()
                    .any(|step| matches!(step, Step::Filter(p) if gpu_supported_pred(p)))
        }
        #[cfg(not(feature = "gpu"))]
        {
            let _ = query;
            false
        }
    }
    #[cfg(feature = "gpu")]
    pub(crate) fn gpu_filter_mask(
        &self,
        pred: &Pred,
        rows: &[Row],
        now: i64,
    ) -> Result<Option<Vec<bool>>, Error> {
        let Some(gpu) = &self.gpu else {
            return Ok(None);
        };
        if !gpu_supported_pred(pred) {
            return Ok(None);
        }
        match pred {
            Pred::And(a, b) | Pred::Or(a, b) => {
                let a = self
                    .gpu_filter_mask(a, rows, now)?
                    .unwrap_or_else(|| rows.iter().map(|row| eval_pred(a, row, now)).collect());
                let b = self
                    .gpu_filter_mask(b, rows, now)?
                    .unwrap_or_else(|| rows.iter().map(|row| eval_pred(b, row, now)).collect());
                Ok(Some(gpu.combine_masks(
                    &a,
                    &b,
                    matches!(pred, Pred::Or(..)),
                )?))
            }
            Pred::Not(_) | Pred::IsNull { .. } | Pred::In { .. } => Ok(None),
            Pred::Cmp { field, op, value } => {
                let right = value_cell(value, now);
                if let Some(needle) = right.text() {
                    if !matches!(op, CmpOp::Eq | CmpOp::Ne) {
                        return Ok(Some(vec![false; rows.len()]));
                    }
                    let texts: Vec<_> = rows.iter().map(|r| field_cell(r, field).text()).collect();
                    return Ok(Some(gpu.text_mask(
                        &texts,
                        needle,
                        if *op == CmpOp::Ne { 2 } else { 1 },
                        false,
                    )?));
                }
                let records: Vec<_> = rows
                    .iter()
                    .map(|r| gpu_comparison_record(field_cell(r, field), &right, *op))
                    .collect();
                Ok(Some(gpu.comparison_mask(&records, *op)?))
            }
            Pred::Contains { field, needle } => {
                let texts: Vec<_> = rows.iter().map(|r| field_cell(r, field).text()).collect();
                Ok(Some(gpu.text_mask(&texts, needle, 0, false)?))
            }
            Pred::Has { field, needle, ci } => {
                let texts: Vec<_> = rows.iter().map(|r| field_cell(r, field).text()).collect();
                Ok(Some(gpu.text_mask(&texts, needle, 3, *ci)?))
            }
            Pred::Regex {
                field,
                pattern,
                flags,
            } => {
                let texts: Vec<_> = rows
                    .iter()
                    .map(|row| field_cell(row, field).text())
                    .collect();
                Ok(Some(gpu.regex_mask(&texts, pattern, flags)?))
            }
        }
    }
}
#[cfg(feature = "gpu")]
fn gpu_supported_pred(pred: &Pred) -> bool {
    match pred {
        Pred::And(a, b) | Pred::Or(a, b) => gpu_supported_pred(a) || gpu_supported_pred(b),
        Pred::Not(_) | Pred::IsNull { .. } | Pred::In { .. } => false,
        Pred::Cmp { .. } | Pred::Has { .. } | Pred::Contains { .. } => true,
        Pred::Regex { .. } => true,
    }
}

#[cfg(feature = "gpu")]
fn gpu_comparison_record(left: &Cell, right: &Cell, _op: CmpOp) -> [u32; 5] {
    let integer_key = |v: i64| (v as u64) ^ (1u64 << 63);
    let float_key = |v: f64| {
        let bits = if v == 0.0 { 0 } else { v.to_bits() };
        if bits >> 63 != 0 {
            !bits
        } else {
            bits ^ (1u64 << 63)
        }
    };
    let keys = match (left, right) {
        (Cell::Bool(a), Cell::Bool(b)) => Some((*a as u64, *b as u64)),
        (Cell::Int(a), Cell::Int(b)) | (Cell::Time(a), Cell::Time(b)) => {
            Some((integer_key(*a), integer_key(*b)))
        }
        (Cell::Float(_), Cell::Float(_))
        | (Cell::Float(_), Cell::Int(_))
        | (Cell::Int(_), Cell::Float(_)) => left
            .as_f64()
            .zip(right.as_f64())
            .filter(|(a, b)| !a.is_nan() && !b.is_nan())
            .map(|(a, b)| (float_key(a), float_key(b))),
        _ => None,
    };
    match keys {
        Some((a, b)) => [a as u32, (a >> 32) as u32, b as u32, (b >> 32) as u32, 1],
        None => [0; 5],
    }
}

impl Db {
    pub(super) fn uses_gpu_count(&self, query: &Query) -> bool {
        #[cfg(feature = "gpu")]
        {
            self.gpu.is_some()
                && query
                    .steps
                    .iter()
                    .any(|step| matches!(step, Step::Count { .. } | Step::Sum { .. }))
        }
        #[cfg(not(feature = "gpu"))]
        {
            let _ = query;
            false
        }
    }
    #[cfg(feature = "gpu")]
    pub(super) fn gpu_count_rows(&self, rows: &[Row], by: Option<&Field>) -> Result<Option<Vec<Row>>, Error> {
        let Some(gpu) = &self.gpu else {
            return Ok(None);
        };
        let Some(by) = by else {
            let counts = gpu.group_counts(&vec![0u32; rows.len()], 1)?;
            return Ok(Some(vec![hits_row(counts[0])]));
        };
        let field = by.as_str();
        let mut group_ids: FxHashMap<Cell, u32> = FxHashMap::default();
        let mut group_cells: Vec<Cell> = Vec::new();
        let mut ids: Vec<u32> = Vec::with_capacity(rows.len());
        for row in rows {
            let cell = row
                .get(&field)
                .map(Cell::clone_for_borrow)
                .unwrap_or(Cell::Null);
            let id = if let Some(&id) = group_ids.get(&cell) {
                id
            } else {
                let id = u32::try_from(group_cells.len())
                    .map_err(|_| Error::runtime("GPU count has too many groups"))?;
                group_cells.push(cell.clone());
                group_ids.insert(cell, id);
                id
            };
            ids.push(id);
        }
        let counts = gpu.group_counts(&ids, group_cells.len())?;
        let mut entries: Vec<(Cell, i64)> = group_cells.into_iter().zip(counts).collect();
        entries.sort_unstable_by(|(a, _), (b, _)| cmp_sort(a, b));
        Ok(Some(
            entries
                .into_iter()
                .map(|(cell, count)| {
                    let mut row = crate::store::acquire_row();
                    row.insert(field.clone(), cell);
                    row.insert("hits".into(), Cell::Int(count));
                    row
                })
                .collect(),
        ))
    }
}

impl Db {
    pub(super) fn uses_gpu_join(&self, query: &Query) -> bool {
        #[cfg(feature = "gpu")]
        {
            self.gpu.is_some()
                && query
                    .steps
                    .iter()
                    .any(|step| matches!(step, Step::Join { .. }))
        }
        #[cfg(not(feature = "gpu"))]
        {
            let _ = query;
            false
        }
    }
    #[cfg(feature = "gpu")]
    pub(crate) fn gpu_join_rows(
        &self,
        left_col: &str,
        left: Vec<Row>,
        right_col: &str,
        on: &str,
        left_join: bool,
    ) -> Result<Vec<Row>, Error> {
        let gpu = self.gpu.as_ref().expect("GPU join selected");
        let to_field = self
            .catalog
            .find_fk(left_col, on, right_col)
            .map(|fk| fk.to_field.as_str())
            .unwrap_or("id");
        let right = self.store.collection(right_col);
        let mut dictionary: FxHashMap<String, u32> = FxHashMap::default();
        let key = |row: &Row, field: &str| {
            if to_field == "id" {
                row.get(field).and_then(Cell::text).map(str::to_owned)
            } else {
                row.get(field).and_then(cell_key)
            }
        };
        let mut encode = |value: Option<String>| -> Result<u32, Error> {
            let Some(value) = value else {
                return Ok(0);
            };
            if let Some(&id) = dictionary.get(&value) {
                return Ok(id);
            }
            let id = u32::try_from(dictionary.len() + 1)
                .map_err(|_| Error::runtime("GPU join has too many distinct keys"))?;
            dictionary.insert(value, id);
            Ok(id)
        };
        let left_keys: Vec<u32> = left
            .iter()
            .map(|row| encode(key(row, on)))
            .collect::<Result<_, _>>()?;
        let right_keys: Vec<u32> = right
            .iter()
            .map(|row| encode(key(row, to_field)))
            .collect::<Result<_, _>>()?;
        if to_field == "id" {
            let hits = gpu.join_id_matches(&left_keys, &right_keys)?;
            let mut out = Vec::new();
            for (row, hit) in left.into_iter().zip(hits) {
                if let Some(hit) = hit {
                    out.push(merge_join_row(row, &right[hit], right_col));
                } else if left_join {
                    out.push(row);
                }
            }
            return Ok(out);
        }
        let hits = gpu.join_matches(&left_keys, &right_keys)?;
        let mut out = Vec::new();
        for (row, hits) in left.into_iter().zip(hits) {
            if hits.is_empty() {
                if left_join {
                    out.push(row);
                }
            } else {
                for &hit in &hits {
                    out.push(merge_join_row(row.clone(), &right[hit], right_col));
                }
            }
        }
        Ok(out)
    }
}

pub(super) fn rows_to_query_batch(rows: &[Row], query: &Query) -> RecordBatch {
    let mut projection = None;
    for step in &query.steps {
        match step {
            Step::Project(fields) => projection = Some(field_names(fields)),
            Step::Join { .. }
            | Step::Hop { .. }
            | Step::Graph { .. }
            | Step::Match { .. }
            | Step::Count { .. }
            | Step::Sum { .. }
            | Step::Min { .. }
            | Step::Max { .. }
            | Step::Avg { .. }
            | Step::Union(_) => projection = None,
            _ => {}
        }
    }
    let Some(names) = projection else {
        return rows_to_rough_batch(rows);
    };
    let mut batch = RecordBatch::with_capacity(Arc::<[String]>::from(names.clone()), rows.len());
    for row in rows {
        push_project_batch_row(&mut batch, row, &names);
    }
    batch
}

impl Db {
    pub(super) fn uses_gpu_sort(&self, query: &Query) -> bool {
        #[cfg(feature = "gpu")]
        {
            self.gpu.is_some()
                && query
                    .steps
                    .iter()
                    .any(|step| matches!(step, Step::Sort { .. }))
        }
        #[cfg(not(feature = "gpu"))]
        {
            let _ = query;
            false
        }
    }
}

#[cfg(feature = "gpu")]
impl Db {
    pub(super) fn gpu_sum_rows(
        &self,
        rows: &[Row],
        field: &Field,
        by: Option<&Field>,
    ) -> Result<Option<Vec<Row>>, Error> {
        let Some(gpu) = &self.gpu else {
            return Ok(None);
        };
        let fk = field.as_str();
        let Some(by) = by else {
            let values: Vec<f64> = rows
                .iter()
                .map(|row| row.get(&fk).and_then(Cell::as_f64).unwrap_or(0.0))
                .collect();
            let sums = gpu.group_sums(&vec![0u32; rows.len()], &values, 1)?;
            let mut row = BTreeMap::new();
            row.insert(fk, Cell::Float(sums[0]));
            return Ok(Some(vec![row]));
        };
        let bk = by.as_str();
        let mut group_ids: FxHashMap<Cell, u32> = FxHashMap::default();
        let mut group_cells: Vec<Cell> = Vec::new();
        let mut ids: Vec<u32> = Vec::with_capacity(rows.len());
        for row in rows {
            let cell = row
                .get(&bk)
                .map(Cell::clone_for_borrow)
                .unwrap_or(Cell::Null);
            let id = if let Some(&id) = group_ids.get(&cell) {
                id
            } else {
                let id = u32::try_from(group_cells.len())
                    .map_err(|_| Error::runtime("GPU sum has too many groups"))?;
                group_cells.push(cell.clone());
                group_ids.insert(cell, id);
                id
            };
            ids.push(id);
        }
        let values: Vec<f64> = rows
            .iter()
            .map(|row| row.get(&fk).and_then(Cell::as_f64).unwrap_or(0.0))
            .collect();
        let sums = gpu.group_sums(&ids, &values, group_cells.len())?;
        let mut entries: Vec<(Cell, f64)> = group_cells.into_iter().zip(sums).collect();
        entries.sort_unstable_by(|(a, _), (b, _)| cmp_sort(a, b));
        Ok(Some(
            entries
                .into_iter()
                .map(|(cell, sum)| {
                    let mut row = crate::store::acquire_row();
                    row.insert(bk.clone(), cell);
                    row.insert(fk.clone(), Cell::Float(sum));
                    row
                })
                .collect(),
        ))
    }
}

impl Db {
    pub(super) fn uses_gpu_graph(&self, query: &Query) -> bool {
        #[cfg(feature = "gpu")]
        {
            self.gpu.is_some()
                && query.steps.iter().any(|step| {
                    matches!(
                        step,
                        Step::Hop { .. } | Step::Graph { .. } | Step::Match { .. }
                    )
                })
        }
        #[cfg(not(feature = "gpu"))]
        {
            let _ = query;
            false
        }
    }
    pub(crate) fn graph_frontier_mask(
        &self,
        relation: &str,
        reverse: bool,
        frontier: &BTreeSet<String>,
    ) -> Result<Option<Vec<bool>>, Error> {
        #[cfg(feature = "gpu")]
        {
            let Some(gpu) = &self.gpu else {
                return Ok(None);
            };
            let mut dictionary: FxHashMap<&str, u32> = FxHashMap::default();
            let mut keys: BTreeSet<&str> = frontier.iter().map(String::as_str).collect();
            for edge in &self.store.edges {
                if edge.rel == relation {
                    keys.insert(if reverse {
                        edge.to.as_str()
                    } else {
                        edge.from.as_str()
                    });
                }
            }
            for key in keys {
                let id = u32::try_from(dictionary.len() + 1)
                    .map_err(|_| Error::runtime("GPU graph has too many keys"))?;
                dictionary.insert(key, id);
            }
            let ids: Vec<u32> = frontier
                .iter()
                .map(|key| dictionary[key.as_str()])
                .collect();
            let sources: Vec<u32> = self
                .store
                .edges
                .iter()
                .map(|edge| {
                    if edge.rel != relation {
                        return 0;
                    }
                    let key = if reverse {
                        edge.to.as_str()
                    } else {
                        edge.from.as_str()
                    };
                    dictionary.get(key).copied().unwrap_or(0)
                })
                .collect();
            Ok(Some(gpu.graph_edge_mask(&sources, &ids)?))
        }
        #[cfg(not(feature = "gpu"))]
        {
            let _ = (relation, reverse, frontier);
            Ok(None)
        }
    }
}

impl Db {
    pub(super) fn gpu_lex_scores<'a>(
        &self,
        rows: impl Iterator<Item = &'a Row>,
        query: &LexQuery,
    ) -> Result<Option<Vec<i64>>, Error> {
        #[cfg(feature = "gpu")]
        {
            let Some(gpu) = &self.gpu else {
                return Ok(None);
            };
            let blobs: Vec<String> = rows
                .map(|row| {
                    let mut blob = String::new();
                    for field in ["title", "body", "snippet"] {
                        if let Some(text) = row_text(row, field) {
                            blob.push_str(text);
                            blob.push(' ');
                        }
                    }
                    blob.to_lowercase()
                })
                .collect();
            let texts: Vec<Option<&str>> = blobs.iter().map(|blob| Some(blob.as_str())).collect();
            if query.tokens.is_empty() && query.lower.is_empty() {
                return Ok(Some(vec![0i64; blobs.len()]));
            }
            let mut scores = gpu.weight_accumulator(blobs.len())?;
            for token in &query.tokens {
                gpu.text_accumulate(&texts, token, 0, &mut scores, 1)?;
                // Punctuation-bearing tokens cannot equal a CPU-split word.
                if token.chars().all(|c| c.is_alphanumeric() || c == '_') {
                    gpu.text_accumulate(&texts, token, 3, &mut scores, 2)?;
                }
            }
            if !query.lower.is_empty() {
                gpu.text_accumulate(&texts, &query.lower, 0, &mut scores, 3)?;
            }
            Ok(Some(scores.finish()?))
        }
        #[cfg(not(feature = "gpu"))]
        {
            let _ = (rows, query);
            Ok(None)
        }
    }
}

#[cfg(feature = "gpu")]
pub(super) fn gpu_rrf_keys<K: Ord + Clone>(
    gpu: &crate::gpu::GpuCompute,
    lex: &[K],
    vectors: &[K],
) -> Result<Vec<K>, Error> {
    let keys: Vec<K> = lex
        .iter()
        .chain(vectors)
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    if keys.len() > u32::MAX as usize {
        return Err(Error::runtime("GPU RRF has too many identities"));
    }
    let mut ids = Vec::with_capacity(lex.len() + vectors.len());
    let mut denominators = Vec::with_capacity(ids.capacity());
    for list in [lex, vectors] {
        for (rank, key) in list.iter().enumerate() {
            let denominator = u32::try_from(rank)
                .ok()
                .and_then(|rank| rank.checked_add(61))
                .ok_or_else(|| Error::runtime("GPU RRF rank exceeds integer range"))?;
            ids.push(keys.binary_search(key).expect("known identity") as u32);
            denominators.push(denominator);
        }
    }
    let scores = gpu.group_rrf(&ids, &denominators, keys.len())?;
    let cells: Vec<Cell> = scores.into_iter().map(Cell::Float).collect();
    let references: Vec<&Cell> = cells.iter().collect();
    Ok(gpu
        .sort_indices(&references, true)?
        .into_iter()
        .map(|i| keys[i].clone())
        .collect())
}

#[cfg(feature = "gpu")]
impl Db {
    pub(super) fn gpu_walk_rows(
        &self,
        primary: &str,
        rows: &[Row],
        relation: &str,
        reverse: bool,
        depth: i64,
        graph: bool,
    ) -> Result<Vec<Row>, Error> {
        let mut seeds = BTreeSet::new();
        for row in rows {
            for field in ["id", "uri"] {
                if let Some(key) = row_text(row, field) {
                    seeds.insert(key.to_owned());
                }
            }
        }
        let (keys, reached) = self.gpu_walk_state(&seeds, relation, reverse, depth, graph)?;
        if graph {
            let edges: Vec<_> = self
                .store
                .edges
                .iter()
                .filter(|edge| edge.rel == relation)
                .collect();
            return Ok(reached
                .into_iter()
                .map(|i| {
                    let edge = edges[i];
                    edge_row(&edge.rel, &edge.from, &edge.to)
                })
                .collect());
        }
        Ok(reached
            .into_iter()
            .filter_map(|i| self.resolve_node(primary, &keys[i]))
            .take(300)
            .collect())
    }
    pub(crate) fn gpu_hop_keys(
        &self,
        seeds: &BTreeSet<String>,
        relation: &str,
        reverse: bool,
    ) -> Result<Vec<String>, Error> {
        let (keys, reached) = self.gpu_walk_state(seeds, relation, reverse, 1, false)?;
        Ok(reached.into_iter().map(|i| keys[i].clone()).collect())
    }
    fn gpu_walk_state(
        &self,
        seeds: &BTreeSet<String>,
        relation: &str,
        reverse: bool,
        depth: i64,
        graph: bool,
    ) -> Result<(Vec<String>, Vec<usize>), Error> {
        let edges: Vec<_> = self
            .store
            .edges
            .iter()
            .filter(|edge| edge.rel == relation)
            .collect();
        let mut keys = seeds.clone();
        for edge in &edges {
            keys.insert(edge.from.clone());
            keys.insert(edge.to.clone());
        }
        let keys: Vec<String> = keys.into_iter().collect();
        let id = |key: &str| {
            keys.binary_search_by(|v| v.as_str().cmp(key))
                .expect("known node") as u32
        };
        let mut identities = BTreeMap::new();
        let encoded: Vec<[u32; 4]> = edges
            .iter()
            .map(|edge| {
                let next = identities.len() as u32;
                let identity = *identities
                    .entry((edge.from.as_str(), edge.to.as_str()))
                    .or_insert(next);
                let (src, dst) = if reverse {
                    (&edge.to, &edge.from)
                } else {
                    (&edge.from, &edge.to)
                };
                [id(src), id(dst), identity, 0]
            })
            .collect();
        let seed_ids: Vec<u32> = seeds.iter().map(|key| id(key)).collect();
        let reached = self.gpu.as_ref().unwrap().walk_graph(
            &encoded,
            &seed_ids,
            keys.len(),
            identities.len(),
            depth.clamp(1, 3) as u32,
            graph,
        )?;
        Ok((keys, reached))
    }
}

#[cfg(feature = "gpu")]
impl Db {
    pub(super) fn gpu_match_expand(
        &self,
        primary: &str,
        row: &Row,
        hop: &MatchHop,
        seeds: &[String],
        relation: &str,
        reverse: bool,
        out: &mut Vec<Row>,
    ) -> Result<(), Error> {
        let edges: Vec<_> = self
            .store
            .edges
            .iter()
            .filter(|e| e.rel == relation)
            .collect();
        let mut keys: BTreeSet<String> = seeds.iter().cloned().collect();
        for edge in &edges {
            keys.insert(edge.from.clone());
            keys.insert(edge.to.clone());
        }
        let keys: Vec<String> = keys.into_iter().collect();
        let id = |key: &str| {
            keys.binary_search_by(|s| s.as_str().cmp(key))
                .expect("known key") as u32
        };
        let encoded: Vec<[u32; 4]> = edges
            .iter()
            .map(|e| {
                let (src, dst) = if reverse {
                    (&e.to, &e.from)
                } else {
                    (&e.from, &e.to)
                };
                [id(src), id(dst), 0, 0]
            })
            .collect();
        let eligible: Vec<u32> = keys
            .iter()
            .map(|key| u32::from(self.resolve_node_ref(primary, key).is_some()))
            .collect();
        for seed in seeds {
            let hits = self.gpu.as_ref().unwrap().match_dfs(
                &encoded,
                &eligible,
                id(seed),
                hop.min_depth as u32,
                hop.max_depth as u32,
                300usize.saturating_sub(out.len()),
            )?;
            for (node_id, edge_id) in hits {
                let node = self
                    .resolve_node_ref(primary, &keys[node_id])
                    .expect("eligible node");
                let mut merged = row.clone();
                for (k, v) in node {
                    merged.insert(format!("{}.{k}", hop.bind), v.clone());
                }
                if let Some(bind) = &hop.edge {
                    let edge = edges[edge_id];
                    merged.insert(format!("{bind}.rel"), Cell::text_arc(edge.rel.as_str()));
                    merged.insert(format!("{bind}.from"), Cell::text_arc(edge.from.as_str()));
                    merged.insert(format!("{bind}.to"), Cell::text_arc(edge.to.as_str()));
                }
                out.push(merged);
            }
            if out.len() >= 300 {
                break;
            }
        }
        Ok(())
    }
}
