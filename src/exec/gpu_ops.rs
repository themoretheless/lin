#[cfg(feature = "gpu")]
use std::borrow::Cow;
#[cfg(feature = "gpu")]
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
use crate::store::row::FieldSlot;

#[cfg(feature = "gpu")]
use super::agg::{cell_key_ref, hits_row};
use super::eval::LexQuery;
#[cfg(feature = "gpu")]
use super::eval::{cmp_sort, eval_pred, value_cell};
#[cfg(feature = "gpu")]
use super::graph_ops::edge_row;
#[cfg(feature = "gpu")]
use super::join::merge_join_row;
use super::join::{push_project_batch_row, rows_to_rough_batch};
use super::{Db, field_names};

#[cfg(feature = "gpu")]
fn collect_field_texts<'a>(rows: &'a [Row], field: &Field) -> Vec<Option<&'a str>> {
    let key = field.as_cow_str();
    let mut slot = FieldSlot::new(&key);
    rows.iter()
        .map(|r| slot.get(r).and_then(Cell::text))
        .collect()
}

#[cfg(feature = "gpu")]
fn build_two_col_rows<V>(
    k0: &str,
    k1: &str,
    entries: Vec<(Cell, V)>,
    mut make_v: impl FnMut(V) -> Cell,
) -> Vec<Row> {
    if k0 == k1 {
        let shape = Row::shape_of(&[k1]);
        return entries
            .into_iter()
            .map(|(_, v)| Row::from_parts(Arc::clone(&shape), [make_v(v)]))
            .collect();
    }
    let shape = Row::shape_of(&[k0, k1]);
    let k0_first = k0 < k1;
    entries
        .into_iter()
        .map(|(c0, v)| {
            let c1 = make_v(v);
            let pair = if k0_first { [c0, c1] } else { [c1, c0] };
            Row::from_parts(Arc::clone(&shape), pair)
        })
        .collect()
}

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
                    let texts = collect_field_texts(rows, field);
                    return Ok(Some(gpu.text_mask(
                        &texts,
                        needle,
                        if *op == CmpOp::Ne { 2 } else { 1 },
                        false,
                    )?));
                }
                let key = field.as_cow_str();
                let mut slot = FieldSlot::new(&key);
                let records: Vec<_> = rows
                    .iter()
                    .map(|r| gpu_comparison_record(slot.get(r).unwrap_or(&Cell::Null), &right, *op))
                    .collect();
                Ok(Some(gpu.comparison_mask(&records, *op)?))
            }
            Pred::Contains { field, needle } => {
                let texts = collect_field_texts(rows, field);
                Ok(Some(gpu.text_mask(&texts, needle, 0, false)?))
            }
            Pred::Has { field, needle, ci } => {
                let texts = collect_field_texts(rows, field);
                Ok(Some(gpu.text_mask(&texts, needle, 3, *ci)?))
            }
            Pred::Regex {
                field,
                pattern,
                flags,
            } => {
                let texts = collect_field_texts(rows, field);
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
    let ord_pair = |ord: std::cmp::Ordering| match ord {
        std::cmp::Ordering::Less => (0u64, 1u64),
        std::cmp::Ordering::Equal => (1u64, 1u64),
        std::cmp::Ordering::Greater => (2u64, 1u64),
    };
    let keys = match (left, right) {
        (Cell::Bool(a), Cell::Bool(b)) => Some((*a as u64, *b as u64)),
        (Cell::Int(a), Cell::Int(b)) | (Cell::Time(a), Cell::Time(b)) => {
            Some((integer_key(*a), integer_key(*b)))
        }
        (Cell::Float(a), Cell::Float(b)) => {
            (!a.is_nan() && !b.is_nan()).then(|| (float_key(*a), float_key(*b)))
        }
        (Cell::Int(i) | Cell::Time(i), Cell::Float(f)) => {
            super::eval::cmp_i64_f64(*i, *f).map(ord_pair)
        }
        (Cell::Float(f), Cell::Int(i) | Cell::Time(i)) => {
            super::eval::cmp_i64_f64(*i, *f)
                .map(std::cmp::Ordering::reverse)
                .map(ord_pair)
        }
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
    pub(super) fn gpu_count_rows(
        &self,
        rows: &[Row],
        by: Option<&Field>,
    ) -> Result<Option<Vec<Row>>, Error> {
        let Some(gpu) = &self.gpu else {
            return Ok(None);
        };
        let Some(by) = by else {
            let counts = gpu.group_counts(&vec![0u32; rows.len()], 1)?;
            return Ok(Some(vec![hits_row(counts[0])]));
        };
        let field = by.as_cow_str();
        let mut by_slot = FieldSlot::new(&field);
        let mut group_ids: FxHashMap<Cell, u32> = FxHashMap::default();
        let mut group_cells: Vec<Cell> = Vec::new();
        let mut ids: Vec<u32> = Vec::with_capacity(rows.len());
        for row in rows {
            let cell_ref = by_slot.get(row).unwrap_or(&Cell::Null);
            let id = if let Some(&id) = group_ids.get(cell_ref) {
                id
            } else {
                let id = u32::try_from(group_cells.len())
                    .map_err(|_| Error::runtime("GPU count has too many groups"))?;
                let owned = cell_ref.clone_for_borrow();
                group_cells.push(owned.clone_for_borrow());
                group_ids.insert(owned, id);
                id
            };
            ids.push(id);
        }
        let counts = gpu.group_counts(&ids, group_cells.len())?;
        let mut entries: Vec<(Cell, i64)> = group_cells.into_iter().zip(counts).collect();
        entries.sort_unstable_by(|(a, _), (b, _)| cmp_sort(a, b));
        Ok(Some(build_two_col_rows(
            &field,
            "hits",
            entries,
            Cell::Int,
        )))
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
        let mut dictionary: FxHashMap<Box<str>, u32> = FxHashMap::with_capacity_and_hasher(
            right.len().min(left.len()),
            Default::default(),
        );
        let id_join = to_field == "id";
        let mut encode = |value: Option<Cow<'_, str>>| -> Result<u32, Error> {
            let Some(value) = value else {
                return Ok(0);
            };
            if let Some(&id) = dictionary.get(value.as_ref()) {
                return Ok(id);
            }
            let id = u32::try_from(dictionary.len() + 1)
                .map_err(|_| Error::runtime("GPU join has too many distinct keys"))?;
            dictionary.insert(Box::from(value.as_ref()), id);
            Ok(id)
        };
        let mut left_slot = FieldSlot::new(on);
        let left_keys: Vec<u32> = left
            .iter()
            .map(|row| {
                let k = if id_join {
                    left_slot.get(row).and_then(Cell::text).map(Cow::Borrowed)
                } else {
                    left_slot.get(row).and_then(cell_key_ref)
                };
                encode(k)
            })
            .collect::<Result<_, _>>()?;
        let mut right_slot = FieldSlot::new(to_field);
        let right_keys: Vec<u32> = right
            .iter()
            .map(|row| {
                let k = if id_join {
                    right_slot.get(row).and_then(Cell::text).map(Cow::Borrowed)
                } else {
                    right_slot.get(row).and_then(cell_key_ref)
                };
                encode(k)
            })
            .collect::<Result<_, _>>()?;
        if id_join {
            let hits = gpu.join_id_matches(&left_keys, &right_keys)?;
            let mut out = Vec::with_capacity(left.len());
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
        let mut out = Vec::with_capacity(left.len());
        for (row, hits) in left.into_iter().zip(hits) {
            match hits.split_last() {
                None => {
                    if left_join {
                        out.push(row);
                    }
                }
                Some((&last_hit, prefix)) => {
                    for &hit in prefix {
                        out.push(merge_join_row(row.clone(), &right[hit], right_col));
                    }
                    out.push(merge_join_row(row, &right[last_hit], right_col));
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
            Step::Without(_)
            | Step::Join { .. }
            | Step::Hop { .. }
            | Step::Graph { .. }
            | Step::Match { .. }
            | Step::Count { .. }
            | Step::Sum { .. }
            | Step::Min { .. }
            | Step::Max { .. }
            | Step::Avg { .. }
            | Step::Summarize { .. }
            | Step::ArgExtremum { .. }
            | Step::Union(_) => projection = None,
            _ => {}
        }
    }
    let Some(names) = projection else {
        return rows_to_rough_batch(rows);
    };
    let names_arc = Arc::<[String]>::from(names);
    let mut batch = RecordBatch::with_capacity(Arc::clone(&names_arc), rows.len());
    for row in rows {
        push_project_batch_row(&mut batch, row, &names_arc);
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
        let fk = field.as_cow_str();
        let mut val_slot = FieldSlot::new(&fk);
        // Split each value the way `ExactSum::add_cell` does: integers into an exact i128
        // part (summed here), other numbers into the float part (summed on the GPU with
        // the same compensation), and non-numeric cells skipped rather than added as 0.0.
        let mut ids: Vec<u32> = Vec::with_capacity(rows.len());
        let mut values: Vec<f64> = Vec::with_capacity(rows.len());
        let Some(by_field) = by else {
            let mut int_part: i128 = 0;
            for row in rows {
                match val_slot.get(row) {
                    Some(Cell::Int(i)) => int_part += *i as i128,
                    Some(c) => {
                        if let Some(v) = c.as_f64() {
                            ids.push(0);
                            values.push(v);
                        }
                    }
                    None => {}
                }
            }
            let pairs = gpu.group_sums_compensated(&ids, &values, 1)?;
            let [float, comp] = pairs[0];
            let sum = crate::exact_sum::ExactSum::from_parts(int_part, float, comp).value();
            let shape = Row::shape_of(&[fk.as_ref()]);
            return Ok(Some(vec![Row::from_parts(shape, [Cell::Float(sum)])]));
        };

        let bk = by_field.as_cow_str();
        let mut by_slot = FieldSlot::new(&bk);
        let mut group_ids: FxHashMap<Cell, u32> = FxHashMap::default();
        let mut group_cells: Vec<Cell> = Vec::new();
        let mut int_parts: Vec<i128> = Vec::new();
        for row in rows {
            let cell_ref = by_slot.get(row).unwrap_or(&Cell::Null);
            let id = if let Some(&id) = group_ids.get(cell_ref) {
                id
            } else {
                let id = u32::try_from(group_cells.len())
                    .map_err(|_| Error::runtime("GPU sum has too many groups"))?;
                let owned = cell_ref.clone_for_borrow();
                group_cells.push(owned.clone_for_borrow());
                group_ids.insert(owned, id);
                int_parts.push(0);
                id
            };
            match val_slot.get(row) {
                Some(Cell::Int(i)) => int_parts[id as usize] += *i as i128,
                Some(c) => {
                    if let Some(v) = c.as_f64() {
                        ids.push(id);
                        values.push(v);
                    }
                }
                None => {}
            }
        }
        if group_cells.is_empty() {
            return Ok(Some(Vec::new()));
        }
        let pairs = gpu.group_sums_compensated(&ids, &values, group_cells.len())?;
        let sums: Vec<f64> = pairs
            .iter()
            .zip(&int_parts)
            .map(|(&[float, comp], &int)| {
                crate::exact_sum::ExactSum::from_parts(int, float, comp).value()
            })
            .collect();
        let mut entries: Vec<(Cell, f64)> = group_cells.into_iter().zip(sums).collect();
        entries.sort_unstable_by(|(a, _), (b, _)| cmp_sort(a, b));
        Ok(Some(build_two_col_rows(&bk, &fk, entries, Cell::Float)))
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
            if query.tokens.is_empty() && query.lower.is_empty() {
                return Ok(Some(vec![0i64; rows.count()]));
            }
            let (low, _) = rows.size_hint();
            let mut all_bytes = String::with_capacity(low * 64);
            let mut ends = Vec::with_capacity(low);
            let mut title_slot = FieldSlot::new("title");
            let mut body_slot = FieldSlot::new("body");
            let mut snippet_slot = FieldSlot::new("snippet");
            for row in rows {
                for slot in [&mut title_slot, &mut body_slot, &mut snippet_slot] {
                    if let Some(text) = slot.get(row).and_then(Cell::text) {
                        if text.is_ascii() {
                            let start = all_bytes.len();
                            all_bytes.push_str(text);
                            all_bytes[start..].make_ascii_lowercase();
                        } else {
                            all_bytes.extend(text.chars().flat_map(char::to_lowercase));
                        }
                        all_bytes.push(' ');
                    }
                }
                ends.push(all_bytes.len());
            }
            let mut texts: Vec<Option<&str>> = Vec::with_capacity(ends.len());
            let mut prev = 0usize;
            for &end in &ends {
                texts.push(Some(&all_bytes[prev..end]));
                prev = end;
            }
            let mut scores = gpu.weight_accumulator(ends.len())?;
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
        let mut id_slot = FieldSlot::new("id");
        let mut uri_slot = FieldSlot::new("uri");
        let mut seed_strs: Vec<&str> = Vec::with_capacity(rows.len() * 2);
        for row in rows {
            if let Some(key) = id_slot.get(row).and_then(Cell::text) {
                seed_strs.push(key);
            }
            if let Some(key) = uri_slot.get(row).and_then(Cell::text) {
                seed_strs.push(key);
            }
        }
        let edges: Vec<_> = self
            .store
            .edges
            .iter()
            .filter(|edge| edge.rel == relation)
            .collect();
        let (keys, reached) =
            self.gpu_walk_state_borrowed(&edges, &seed_strs, reverse, depth, graph)?;
        if graph {
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
            .filter_map(|i| self.resolve_node_ref(primary, keys[i]).cloned())
            .take(300)
            .collect())
    }
    pub(crate) fn gpu_hop_keys(
        &self,
        seeds: &BTreeSet<String>,
        relation: &str,
        reverse: bool,
    ) -> Result<Vec<String>, Error> {
        let edges: Vec<_> = self
            .store
            .edges
            .iter()
            .filter(|edge| edge.rel == relation)
            .collect();
        let seed_strs: Vec<&str> = seeds.iter().map(String::as_str).collect();
        let (keys, reached) = self.gpu_walk_state_borrowed(&edges, &seed_strs, reverse, 1, false)?;
        Ok(reached.into_iter().map(|i| keys[i].to_owned()).collect())
    }
    fn gpu_walk_state_borrowed<'a>(
        &self,
        edges: &[&'a crate::store::Edge],
        seeds: &[&'a str],
        reverse: bool,
        depth: i64,
        graph: bool,
    ) -> Result<(Vec<&'a str>, Vec<usize>), Error> {
        let mut keys: Vec<&'a str> = Vec::with_capacity(seeds.len() + edges.len() * 2);
        keys.extend_from_slice(seeds);
        for edge in edges {
            keys.push(edge.from.as_str());
            keys.push(edge.to.as_str());
        }
        keys.sort_unstable();
        keys.dedup();
        let mut node_ids: FxHashMap<&'a str, u32> =
            FxHashMap::with_capacity_and_hasher(keys.len(), Default::default());
        for (i, &k) in keys.iter().enumerate() {
            node_ids.insert(k, i as u32);
        }
        let mut identities: FxHashMap<(&'a str, &'a str), u32> =
            FxHashMap::with_capacity_and_hasher(edges.len(), Default::default());
        let encoded: Vec<[u32; 4]> = edges
            .iter()
            .map(|edge| {
                let next = identities.len() as u32;
                let identity = *identities
                    .entry((edge.from.as_str(), edge.to.as_str()))
                    .or_insert(next);
                let (src, dst) = if reverse {
                    (edge.to.as_str(), edge.from.as_str())
                } else {
                    (edge.from.as_str(), edge.to.as_str())
                };
                [node_ids[src], node_ids[dst], identity, 0]
            })
            .collect();
        let mut seed_ids: Vec<u32> = seeds.iter().map(|&key| node_ids[key]).collect();
        seed_ids.sort_unstable();
        seed_ids.dedup();
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
        let cap = seeds.len() + edges.len() * 2;
        let mut node_ids: FxHashMap<&str, u32> =
            FxHashMap::with_capacity_and_hasher(cap, Default::default());
        let mut keys: Vec<&str> = Vec::with_capacity(cap);
        for s in seeds {
            let k = s.as_str();
            if !node_ids.contains_key(k) {
                let id = keys.len() as u32;
                keys.push(k);
                node_ids.insert(k, id);
            }
        }
        for edge in &edges {
            for k in [edge.from.as_str(), edge.to.as_str()] {
                if !node_ids.contains_key(k) {
                    let id = keys.len() as u32;
                    keys.push(k);
                    node_ids.insert(k, id);
                }
            }
        }
        let encoded: Vec<[u32; 4]> = edges
            .iter()
            .map(|e| {
                let (src, dst) = if reverse {
                    (e.to.as_str(), e.from.as_str())
                } else {
                    (e.from.as_str(), e.to.as_str())
                };
                [node_ids[src], node_ids[dst], 0, 0]
            })
            .collect();
        let resolved: Vec<Option<&Row>> = keys
            .iter()
            .map(|&key| self.resolve_node_ref(primary, key))
            .collect();
        let eligible: Vec<u32> = resolved.iter().map(|r| u32::from(r.is_some())).collect();
        let edge_bind_keys = hop.edge.as_ref().map(|bind| {
            (
                format!("{bind}.rel"),
                format!("{bind}.from"),
                format!("{bind}.to"),
            )
        });
        let mut field_key = String::with_capacity(hop.bind.len() + 16);
        for seed in seeds {
            let hits = self.gpu.as_ref().unwrap().match_dfs(
                &encoded,
                &eligible,
                node_ids[seed.as_str()],
                hop.min_depth as u32,
                hop.max_depth as u32,
                300usize.saturating_sub(out.len()),
            )?;
            for (node_id, edge_id) in hits {
                let node = resolved[node_id].expect("eligible node");
                let mut merged = row.clone();
                for (k, v) in node {
                    field_key.clear();
                    field_key.push_str(&hop.bind);
                    field_key.push('.');
                    field_key.push_str(k);
                    merged.insert(&field_key, v.clone());
                }
                if let Some((rel_k, from_k, to_k)) = &edge_bind_keys {
                    let edge = edges[edge_id];
                    merged.insert(rel_k.as_str(), Cell::text_arc(edge.rel.as_str()));
                    merged.insert(from_k.as_str(), Cell::text_arc(edge.from.as_str()));
                    merged.insert(to_k.as_str(), Cell::text_arc(edge.to.as_str()));
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
