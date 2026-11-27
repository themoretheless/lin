use std::collections::BTreeMap;
use std::sync::Arc;

use rustc_hash::{FxHashMap, FxHashSet};
use smallvec::SmallVec;

use crate::ast::{
    AggFunc, AggSpec, BinStep, CmpOp, Field, GroupExpr, GroupKey, Pred, Query, Source, Step,
    Value,
};
use crate::catalog::Type;
use crate::error::Error;
use crate::store::{Cell, Row};
use crate::persist::spill::Spillable;

use super::Db;
use super::eval::{PredSlots, cmp_sort, eval_pred, pred_has_param, pred_needs_now};
use crate::store::row::FieldSlot;
use super::prepared::{AggKind, AggPlan, AggTail};

static SPILL_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
static NULL_CELL: Cell = Cell::Null;

#[inline]
pub(crate) fn single_cell_row(field: &str, val: Cell) -> Row {
    let shape = Row::shape_of(&[field]);
    Row::from_parts(shape, [val])
}

pub(crate) fn pair_entries_to_rows<V>(
    k0: &str,
    k1: &str,
    entries: Vec<(Cell, V)>,
    mut to_cell: impl FnMut(V) -> Cell,
) -> Vec<Row> {
    if entries.is_empty() {
        return Vec::new();
    }
    if k0 != k1 {
        let shape = Row::shape_of(&[k0, k1]);
        let swap = k0 > k1;
        return entries
            .into_iter()
            .map(|(c0, v)| {
                let c1 = to_cell(v);
                Row::from_parts(
                    Arc::clone(&shape),
                    if swap { [c1, c0] } else { [c0, c1] },
                )
            })
            .collect();
    }
    let shape = Row::shape_of(&[k1]);
    entries
        .into_iter()
        .map(|(_, v)| Row::from_parts(Arc::clone(&shape), [to_cell(v)]))
        .collect()
}

fn cell_hash(cell: &Cell) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = rustc_hash::FxHasher::default();
    cell.hash(&mut hasher);
    hasher.finish()
}

struct SpillPathsGuard(Vec<std::path::PathBuf>);

impl Drop for SpillPathsGuard {
    fn drop(&mut self) {
        for path in &self.0 {
            let _ = std::fs::remove_file(path);
        }
    }
}

fn spill_partition_aggregate<'a, I, P, S, Extract, Init, Fold>(
    tag: &str,
    rows: I,
    mut extract: Extract,
    init_state: Init,
    fold: Fold,
) -> Result<Vec<(Cell, S)>, Error>
where
    I: IntoIterator<Item = &'a Row>,
    P: Spillable + Send,
    S: Send,
    Extract: FnMut(&'a Row) -> (Cell, P),
    Init: Fn() -> S + Sync,
    Fold: Fn(&mut S, P) + Sync,
{
    use std::io::{BufRead, Write};
    use std::sync::atomic::{AtomicUsize, Ordering};

    const NUM_PARTITIONS: usize = 16;
    const SPILL_IO_BUF: usize = 64 * 1024;

    let temp_dir = std::env::temp_dir();
    let run_id = SPILL_SEQ.fetch_add(1, Ordering::Relaxed);
    let mut partition_paths = Vec::with_capacity(NUM_PARTITIONS);
    let mut partition_files = Vec::with_capacity(NUM_PARTITIONS);

    for p in 0..NUM_PARTITIONS {
        let path = temp_dir.join(format!("lin_spill_{tag}_{run_id}_{p}.bin"));
        let file = std::fs::File::create(&path).map_err(|e| Error::runtime(e.to_string()))?;
        partition_paths.push(path);
        partition_files.push(std::io::BufWriter::with_capacity(SPILL_IO_BUF, file));
    }
    let guard = SpillPathsGuard(partition_paths);

    for r in rows {
        let (cell, payload) = extract(r);
        let part_idx = (cell_hash(&cell) as usize) % NUM_PARTITIONS;
        (cell, payload).serialize(&mut partition_files[part_idx])?;
    }

    for mut writer in partition_files {
        writer.flush().map_err(|e| Error::runtime(e.to_string()))?;
    }

    let fold_one = |path: &std::path::Path| -> Result<Vec<(Cell, S)>, Error> {
        let Ok(file) = std::fs::File::open(path) else {
            return Ok(Vec::new());
        };
        let len = file.metadata().map(|m| m.len()).unwrap_or(0);
        if len == 0 {
            let _ = std::fs::remove_file(path);
            return Ok(Vec::new());
        }
        let mut part_map: FxHashMap<Cell, S> = FxHashMap::default();
        let mut reader = std::io::BufReader::with_capacity(SPILL_IO_BUF, file);
        loop {
            let buf = reader.fill_buf().map_err(|e| Error::runtime(e.to_string()))?;
            if buf.is_empty() {
                break;
            }
            let (cell, payload) = <(Cell, P)>::deserialize(&mut reader)?;
            let entry = part_map.entry(cell).or_insert_with(&init_state);
            fold(entry, payload);
        }
        drop(reader);
        let _ = std::fs::remove_file(path);
        Ok(part_map.into_iter().collect())
    };

    let workers = std::thread::available_parallelism()
        .map(|p| p.get())
        .unwrap_or(1)
        .min(4);

    let mut entries: Vec<(Cell, S)> = Vec::new();
    if workers <= 1 {
        for path in &guard.0 {
            entries.extend(fold_one(path)?);
        }
    } else {
        let next_part = AtomicUsize::new(0);
        let paths = guard.0.as_slice();
        let worker_results: Vec<Result<Vec<(Cell, S)>, Error>> = std::thread::scope(|scope| {
            let mut handles = Vec::with_capacity(workers);
            for _ in 0..workers {
                let next_ref = &next_part;
                let fold_ref = &fold_one;
                handles.push(scope.spawn(move || -> Result<Vec<(Cell, S)>, Error> {
                    let mut local_out = Vec::new();
                    loop {
                        let idx = next_ref.fetch_add(1, Ordering::Relaxed);
                        if idx >= paths.len() {
                            break;
                        }
                        local_out.extend(fold_ref(&paths[idx])?);
                    }
                    Ok(local_out)
                }));
            }
            handles
                .into_iter()
                .map(|h| {
                    h.join()
                        .unwrap_or_else(|_| Err(Error::runtime("spill partition worker panicked")))
                })
                .collect()
        });
        for res in worker_results {
            entries.extend(res?);
        }
    }

    entries.sort_unstable_by(|(a, _), (b, _)| cmp_sort(a, b));
    Ok(entries)
}

pub(crate) fn spill_agg_count<'a>(
    rows: impl IntoIterator<Item = &'a Row>,
    by: &Field,
) -> Result<Vec<Row>, Error> {
    let k = by.as_str();
    let mut k_slot = FieldSlot::new(&k);
    let entries = spill_partition_aggregate(
        "count",
        rows,
        |r| {
            let cell = k_slot
                .get(r)
                .map(Cell::clone_for_borrow)
                .unwrap_or(Cell::Null);
            (cell, ())
        },
        || 0i64,
        |acc, ()| *acc += 1,
    )?;
    Ok(pair_entries_to_rows(&k, "hits", entries, Cell::Int))
}

pub(crate) fn spill_agg_sum<'a>(
    rows: impl IntoIterator<Item = &'a Row>,
    field: &Field,
    by: Option<&Field>,
) -> Result<Vec<Row>, Error> {
    let fk = field.as_str();
    let bk = by.map(Field::as_str);
    let mut k_slot = bk.as_deref().map(FieldSlot::new);
    let mut v_slot = FieldSlot::new(&fk);
    let entries = spill_partition_aggregate(
        "sum",
        rows,
        |r| {
            let cell = k_slot
                .as_mut()
                .and_then(|s| s.get(r).map(Cell::clone_for_borrow))
                .unwrap_or(Cell::Null);
            let mut n = crate::exact_sum::ExactSum::default();
            n.add_cell(v_slot.get(r));
            (cell, n)
        },
        crate::exact_sum::ExactSum::default,
        |acc, n| acc.merge(n),
    )?;
    if let Some(ref group_col) = bk {
        return Ok(pair_entries_to_rows(group_col, &fk, entries, |n| {
            Cell::Float(n.value())
        }));
    }
    let shape = Row::shape_of(&[fk.as_str()]);
    Ok(entries
        .into_iter()
        .map(|(_, n)| Row::from_parts(Arc::clone(&shape), [Cell::Float(n.value())]))
        .collect())
}

pub(crate) fn spill_agg_min_max<'a>(
    rows: impl IntoIterator<Item = &'a Row>,
    field: &Field,
    by: Option<&Field>,
    is_max: bool,
) -> Result<Vec<Row>, Error> {
    let fk = field.as_str();
    let bk = by.map(Field::as_str);
    let mut k_slot = bk.as_deref().map(FieldSlot::new);
    let mut v_slot = FieldSlot::new(&fk);
    let entries = spill_partition_aggregate(
        "minmax",
        rows,
        |r| {
            let cell = k_slot
                .as_mut()
                .and_then(|s| s.get(r).map(Cell::clone_for_borrow))
                .unwrap_or(Cell::Null);
            let f_val = v_slot
                .get(r)
                .filter(|c| !matches!(c, Cell::Null))
                .map(Cell::clone_for_borrow);
            (cell, f_val)
        },
        || None::<Cell>,
        |entry, val| {
            if let Some(c) = val {
                let replace = match entry.as_ref() {
                    None => true,
                    Some(prev) => {
                        if is_max {
                            cmp_sort(&c, prev).is_gt()
                        } else {
                            cmp_sort(&c, prev).is_lt()
                        }
                    }
                };
                if replace {
                    *entry = Some(c);
                }
            }
        },
    )?;
    if let Some(ref group_col) = bk {
        return Ok(pair_entries_to_rows(group_col, &fk, entries, |val| {
            val.unwrap_or(Cell::Null)
        }));
    }
    let shape = Row::shape_of(&[fk.as_str()]);
    Ok(entries
        .into_iter()
        .map(|(_, val)| Row::from_parts(Arc::clone(&shape), [val.unwrap_or(Cell::Null)]))
        .collect())
}

pub(crate) fn spill_agg_avg<'a>(
    rows: impl IntoIterator<Item = &'a Row>,
    field: &Field,
    by: Option<&Field>,
) -> Result<Vec<Row>, Error> {
    let fk = field.as_str();
    let bk = by.map(Field::as_str);
    let mut k_slot = bk.as_deref().map(FieldSlot::new);
    let mut v_slot = FieldSlot::new(&fk);
    let entries = spill_partition_aggregate(
        "avg",
        rows,
        |r| {
            let cell = k_slot
                .as_mut()
                .and_then(|s| s.get(r).map(Cell::clone_for_borrow))
                .unwrap_or(Cell::Null);
            let v_opt = v_slot.get(r).and_then(Cell::as_f64);
            (cell, v_opt)
        },
        || (crate::exact_sum::ExactSum::default(), 0usize),
        |entry, v_opt| {
            if let Some(v) = v_opt {
                entry.0.add_f64(v);
                entry.1 += 1;
            }
        },
    )?;
    if let Some(ref group_col) = bk {
        return Ok(pair_entries_to_rows(
            group_col,
            &fk,
            entries,
            |(sum, count)| Cell::Float(if count > 0 { sum.value() / count as f64 } else { 0.0 }),
        ));
    }
    let shape = Row::shape_of(&[fk.as_str()]);
    Ok(entries
        .into_iter()
        .map(|(_, (sum, count))| {
            let avg = if count > 0 {
                sum.value() / count as f64
            } else {
                0.0
            };
            Row::from_parts(Arc::clone(&shape), [Cell::Float(avg)])
        })
        .collect())
}

pub(crate) fn agg_count(
    rows: &[Row],
    by: &Field,
    max_bytes: usize,
    spill: bool,
) -> Result<Vec<Row>, Error> {
    agg_count_filtered(rows, None, 0, by, max_bytes, spill)
}

/// Below this many rows the fork/join overhead of a parallel aggregate or join outweighs the scan.
pub(crate) const PAR_AGG_MIN_ROWS: usize = 65_536;
pub(crate) const PAR_AGG_CHUNK_ROWS: usize = 16_384;

/// Grouped aggregate over materialized rows in fixed `PAR_AGG_CHUNK_ROWS` chunks: each
/// chunk folds into its own map (in parallel on the rayon pool when `parallel` is enabled,
/// or sequentially otherwise), and the partials merge in chunk order so the result never
/// depends on thread scheduling or build features. Returns `None` when the merged state
/// exceeds `max_bytes`; the caller then runs the sequential path, which owns the quota
/// error and disk spill.
fn chunked_group_fold<V: Send + Default>(
    rows: &[Row],
    filter: Option<&Pred>,
    now: i64,
    key: &str,
    value: Option<&str>,
    max_bytes: usize,
    add: impl Fn(&mut V, Option<&Cell>) + Sync,
    merge: impl Fn(&mut V, V),
) -> Option<FxHashMap<Cell, V>> {
    let fold_chunk = |chunk: &[Row]| -> FxHashMap<Cell, V> {
        let mut map: FxHashMap<Cell, V> = FxHashMap::default();
        let mut flt = filter.map(PredSlots::new);
        let mut key_slot = FieldSlot::new(key);
        let mut value_slot = value.map(FieldSlot::new);
        for r in chunk {
            if !flt.as_mut().is_none_or(|f| f.eval(r, now)) {
                continue;
            }
            let key_ref = key_slot.get(r).unwrap_or(&NULL_CELL);
            let v = value_slot.as_mut().and_then(|s| s.get(r));
            if let Some(acc) = map.get_mut(key_ref) {
                add(acc, v);
            } else {
                let mut acc = V::default();
                add(&mut acc, v);
                map.insert(key_ref.clone_for_borrow(), acc);
            }
        }
        map
    };
    let mut merged: FxHashMap<Cell, V> = FxHashMap::default();
    let mut state_bytes = 0usize;
    let mut merge_part = |part: FxHashMap<Cell, V>| -> bool {
        for (cell, v) in part {
            match merged.entry(cell) {
                std::collections::hash_map::Entry::Occupied(mut e) => merge(e.get_mut(), v),
                std::collections::hash_map::Entry::Vacant(e) => {
                    let key_size = match e.key() {
                        Cell::Text(s) => s.len() + 32,
                        _ => 16,
                    };
                    state_bytes = state_bytes.saturating_add(key_size + 64);
                    if state_bytes > max_bytes {
                        return false;
                    }
                    e.insert(v);
                }
            }
        }
        true
    };
    #[cfg(feature = "parallel")]
    {
        use rayon::prelude::*;
        let parts: Vec<FxHashMap<Cell, V>> =
            rows.par_chunks(PAR_AGG_CHUNK_ROWS).map(fold_chunk).collect();
        for part in parts {
            if !merge_part(part) {
                return None;
            }
        }
    }
    #[cfg(not(feature = "parallel"))]
    {
        for chunk in rows.chunks(PAR_AGG_CHUNK_ROWS) {
            if !merge_part(fold_chunk(chunk)) {
                return None;
            }
        }
    }
    Some(merged)
}

pub(crate) fn scoped_par_filter_count(rows: &[Row], pred: &Pred, now: i64) -> i64 {
    if rows.len() >= PAR_AGG_MIN_ROWS {
        let threads = std::thread::available_parallelism()
            .map(|p| p.get())
            .unwrap_or(1)
            .min(8)
            .min((rows.len() + PAR_AGG_CHUNK_ROWS - 1) / PAR_AGG_CHUNK_ROWS);
        if threads > 1 {
            let chunk_size = (rows.len() + threads - 1) / threads;
            return std::thread::scope(|scope| {
                let mut handles = Vec::with_capacity(threads);
                for chunk in rows.chunks(chunk_size) {
                    handles.push(scope.spawn(move || {
                        let mut flt = PredSlots::new(pred);
                        let mut count = 0i64;
                        for r in chunk {
                            if flt.eval(r, now) {
                                count += 1;
                            }
                        }
                        count
                    }));
                }
                handles.into_iter().map(|h| h.join().unwrap()).sum()
            });
        }
    }
    let mut flt = PredSlots::new(pred);
    let mut count = 0i64;
    for r in rows {
        if flt.eval(r, now) {
            count += 1;
        }
    }
    count
}

fn scoped_par_count_by(
    rows: &[Row],
    filter: Option<&Pred>,
    now: i64,
    key: &str,
    max_bytes: usize,
) -> Option<FxHashMap<Cell, i64>> {
    if rows.len() < PAR_AGG_MIN_ROWS {
        return None;
    }
    let threads = std::thread::available_parallelism()
        .map(|p| p.get())
        .unwrap_or(1)
        .min(8)
        .min((rows.len() + PAR_AGG_CHUNK_ROWS - 1) / PAR_AGG_CHUNK_ROWS);
    if threads <= 1 {
        return None;
    }
    let chunk_size = (rows.len() + threads - 1) / threads;
    let parts: Vec<Option<FxHashMap<Cell, i64>>> = std::thread::scope(|scope| {
        let mut handles = Vec::with_capacity(threads);
        for chunk in rows.chunks(chunk_size) {
            handles.push(scope.spawn(move || {
                let mut map: FxHashMap<Cell, i64> = FxHashMap::default();
                let mut local_bytes = 0usize;
                let mut flt = filter.map(PredSlots::new);
                let mut key_slot = FieldSlot::new(key);
                for r in chunk {
                    if !flt.as_mut().is_none_or(|f| f.eval(r, now)) {
                        continue;
                    }
                    let key_ref = key_slot.get(r).unwrap_or(&NULL_CELL);
                    if let Some(cnt) = map.get_mut(key_ref) {
                        *cnt += 1;
                    } else {
                        let key_size = match key_ref {
                            Cell::Text(s) => s.len() + 32,
                            _ => 16,
                        };
                        local_bytes = local_bytes.saturating_add(key_size + 64);
                        if local_bytes > max_bytes {
                            return None;
                        }
                        map.insert(key_ref.clone_for_borrow(), 1);
                    }
                }
                Some(map)
            }));
        }
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let mut merged: FxHashMap<Cell, i64> = FxHashMap::default();
    let mut state_bytes = 0usize;
    for part_opt in parts {
        let part = part_opt?;
        for (cell, v) in part {
            match merged.entry(cell) {
                std::collections::hash_map::Entry::Occupied(mut e) => *e.get_mut() += v,
                std::collections::hash_map::Entry::Vacant(e) => {
                    let key_size = match e.key() {
                        Cell::Text(s) => s.len() + 32,
                        _ => 16,
                    };
                    state_bytes = state_bytes.saturating_add(key_size + 64);
                    if state_bytes > max_bytes {
                        return None;
                    }
                    e.insert(v);
                }
            }
        }
    }
    Some(merged)
}

pub(crate) fn agg_count_filtered(
    rows: &[Row],
    filter: Option<&Pred>,
    now: i64,
    by: &Field,
    max_bytes: usize,
    spill: bool,
) -> Result<Vec<Row>, Error> {
    let k = by.as_str();
    if let Some(map) = scoped_par_count_by(rows, filter, now, &k, max_bytes) {
        return Ok(count_cell_map_to_rows(&k, map));
    }
    let mut map: FxHashMap<Cell, i64> = FxHashMap::default();
    let mut state_bytes = 0usize;
    let mut flt = filter.map(PredSlots::new);
    let mut key_slot = FieldSlot::new(&k);
    for r in rows {
        if !flt.as_mut().is_none_or(|f| f.eval(r, now)) {
            continue;
        }
        let key_ref = key_slot.get(r).unwrap_or(&NULL_CELL);
        if let Some(cnt) = map.get_mut(key_ref) {
            *cnt += 1;
        } else {
            let key_size = match key_ref {
                Cell::Text(s) => s.len() + 32,
                _ => 16,
            };
            state_bytes = state_bytes.saturating_add(key_size + 64);
            if state_bytes > max_bytes {
                if spill {
                    return spill_agg_count(
                        rows.iter()
                            .filter(|r| filter.is_none_or(|p| eval_pred(p, r, now))),
                        by,
                    );
                }
                return Err(Error::runtime(format!(
                    "aggregate state bytes {state_bytes} exceeds max_aggregate_bytes quota {max_bytes}"
                )));
            }
            map.insert(key_ref.clone_for_borrow(), 1);
        }
    }
    Ok(count_cell_map_to_rows(&k, map))
}

pub(crate) fn count_cell_map_to_rows(by: &str, map: FxHashMap<Cell, i64>) -> Vec<Row> {
    if map.is_empty() {
        return Vec::new();
    }
    let mut entries: Vec<(Cell, i64)> = map.into_iter().collect();
    entries.sort_unstable_by(|(a, _), (b, _)| cmp_sort(a, b));
    if by != "hits" {
        let shape = Row::shape_of(&[by, "hits"]);
        let swap = by > "hits";
        return entries
            .into_iter()
            .map(|(cell, n)| {
                let hits = Cell::Int(n);
                Row::from_parts(
                    std::sync::Arc::clone(&shape),
                    if swap { [hits, cell] } else { [cell, hits] },
                )
            })
            .collect();
    }
    entries
        .into_iter()
        .map(|(cell, n)| {
            let mut row = crate::store::acquire_row();
            row.insert(by, cell);
            row.insert("hits", Cell::Int(n));
            row
        })
        .collect()
}

pub(crate) fn count_arc_map_to_rows(by: &str, map: BTreeMap<std::sync::Arc<str>, i64>) -> Vec<Row> {
    if map.is_empty() {
        return Vec::new();
    }
    if by != "hits" {
        let shape = Row::shape_of(&[by, "hits"]);
        let swap = by > "hits";
        return map
            .into_iter()
            .map(|(g, n)| {
                let cell = Cell::from_arc_str(&g);
                let hits = Cell::Int(n);
                Row::from_parts(
                    std::sync::Arc::clone(&shape),
                    if swap { [hits, cell] } else { [cell, hits] },
                )
            })
            .collect();
    }
    let shape = Arc::clone(hits_shape());
    map.into_iter()
        .map(|(_, n)| Row::from_parts(Arc::clone(&shape), [Cell::Int(n)]))
        .collect()
}

/// `field <range-op> value`, optionally AND-ed with `eq_field == "text"` (either order).
#[allow(clippy::type_complexity)]
fn disk_range_count_shape(
    pred: &Pred,
) -> Option<(&Field, CmpOp, &Value, Option<(&Field, &str)>)> {
    fn range(p: &Pred) -> Option<(&Field, CmpOp, &Value)> {
        match p {
            Pred::Cmp { field, op, value }
                if matches!(op, CmpOp::Gt | CmpOp::Ge | CmpOp::Lt | CmpOp::Le) =>
            {
                Some((field, *op, value))
            }
            _ => None,
        }
    }
    fn text_eq(p: &Pred) -> Option<(&Field, &str)> {
        match p {
            Pred::Cmp {
                field,
                op: CmpOp::Eq,
                value: Value::String(s),
            } => Some((field, s.as_ref())),
            _ => None,
        }
    }
    if let Some((f, op, v)) = range(pred) {
        return Some((f, op, v, None));
    }
    let Pred::And(a, b) = pred else {
        return None;
    };
    if let (Some((f, op, v)), Some(eq)) = (range(a), text_eq(b)) {
        return Some((f, op, v, Some(eq)));
    }
    if let (Some(eq), Some((f, op, v))) = (text_eq(a), range(b)) {
        return Some((f, op, v, Some(eq)));
    }
    None
}

fn disk_col_kind(
    eng: &crate::disk::DiskEngine,
    table: &str,
    field: &str,
) -> Option<crate::disk::DiskColKind> {
    eng.manifest
        .tables
        .get(table)
        .and_then(|t| t.columns.iter().find(|c| c.name == field))
        .map(|c| c.kind)
}

/// Count rows matching `pred` without materializing rows, when `pred` is a range comparison
/// (optionally AND-ed with a text equality) or a substring match on a leaf column.
/// Returns the count and the leaf name / value of the equality conjunct, if any.
fn disk_zero_row_count<'p>(
    eng: &crate::disk::DiskEngine,
    table: &str,
    pred: &'p Pred,
    now: i64,
) -> Result<Option<(usize, Option<(&'p str, &'p str)>)>, Error> {
    if let Pred::Contains { field, needle } = pred
        && let Some(fname) = field.leaf()
    {
        return Ok(eng
            .count_text_contains(table, fname, needle)?
            .map(|n| (n, None)));
    }
    let Some((cmp_field, op, value, eq)) = disk_range_count_shape(pred) else {
        return Ok(None);
    };
    let Some(cmp_name) = cmp_field.leaf() else {
        return Ok(None);
    };
    let eq_leaf = match eq {
        None => None,
        Some((f, s)) => match f.leaf() {
            Some(l) => Some((l, s)),
            None => return Ok(None),
        },
    };
    let rhs = super::eval::value_cell(value, now);
    let Some(kind) = disk_col_kind(eng, table, cmp_name) else {
        return Ok(None);
    };
    if !disk_cmp_rhs_compatible(kind, &rhs) {
        return Ok(None);
    }
    Ok(eng
        .count_cmp_and_optional_eq(table, cmp_name, op, &rhs, eq_leaf, pred, now)?
        .map(|n| (n, eq_leaf)))
}

/// The columnar comparison kernel must agree with `cmp_cell`, which is type-strict:
/// `Int` cells compare with int/duration values, `Time` cells only with timestamps,
/// `Float` cells with floats and exactly representable ints.
fn disk_cmp_rhs_compatible(kind: crate::disk::DiskColKind, rhs: &Cell) -> bool {
    use crate::disk::DiskColKind;
    match (kind, rhs) {
        (DiskColKind::Int64, Cell::Int(_)) => true,
        (DiskColKind::Time64, Cell::Time(_)) => true,
        (DiskColKind::Float64, Cell::Float(f)) => !f.is_nan(),
        (DiskColKind::Float64, Cell::Int(i)) => i.unsigned_abs() <= (1u64 << 53),
        _ => false,
    }
}

#[inline]
fn hits_shape() -> &'static Arc<crate::store::RowShape> {
    static SHAPE: std::sync::OnceLock<Arc<crate::store::RowShape>> = std::sync::OnceLock::new();
    SHAPE.get_or_init(|| Row::shape_of(&["hits"]))
}

#[inline]
pub(crate) fn hits_row(n: i64) -> Row {
    Row::from_parts(Arc::clone(hits_shape()), [Cell::Int(n)])
}

pub(crate) fn count_row(by: &str, value: &str, n: i64) -> Row {
    static HITS_WING: std::sync::OnceLock<Arc<crate::store::RowShape>> = std::sync::OnceLock::new();
    static HITS_LAYER: std::sync::OnceLock<Arc<crate::store::RowShape>> =
        std::sync::OnceLock::new();
    static HITS_USER_ID: std::sync::OnceLock<Arc<crate::store::RowShape>> =
        std::sync::OnceLock::new();

    if by == "hits" {
        return hits_row(n);
    }
    let val_cell = Cell::from_text(value);
    let hits_cell = Cell::Int(n);
    let shape = match by {
        "wing" => Arc::clone(HITS_WING.get_or_init(|| Row::shape_of(&["hits", "wing"]))),
        "layer" => Arc::clone(HITS_LAYER.get_or_init(|| Row::shape_of(&["hits", "layer"]))),
        "user_id" => Arc::clone(HITS_USER_ID.get_or_init(|| Row::shape_of(&["hits", "user_id"]))),
        _ => Row::shape_of(&[by, "hits"]),
    };
    Row::from_parts(
        shape,
        if by > "hits" {
            [hits_cell, val_cell]
        } else {
            [val_cell, hits_cell]
        },
    )
}

pub(crate) fn eq_value_for_field<'a>(pred: &'a Pred, field: &str) -> Option<&'a str> {
    match pred {
        Pred::Cmp {
            field: f,
            op: CmpOp::Eq,
            value: Value::String(s),
        } if f.as_str() == field => Some(s.as_ref()),
        Pred::Cmp {
            field: f,
            op: CmpOp::Eq,
            value: Value::Name(s),
        } if f.as_str() == field => Some(s.as_str()),
        Pred::And(a, b) => eq_value_for_field(a, field).or_else(|| eq_value_for_field(b, field)),
        _ => None,
    }
}

pub(crate) fn agg_sum(
    rows: &[Row],
    field: &Field,
    by: Option<&Field>,
    max_bytes: usize,
    spill: bool,
) -> Result<Vec<Row>, Error> {
    agg_sum_filtered(rows, None, 0, field, by, max_bytes, spill)
}

pub(crate) fn agg_sum_filtered(
    rows: &[Row],
    filter: Option<&Pred>,
    now: i64,
    field: &Field,
    by: Option<&Field>,
    max_bytes: usize,
    spill: bool,
) -> Result<Vec<Row>, Error> {
    let fk = field.as_str();
    let Some(by) = by else {
        let sum_rows = |rows: &[Row]| -> crate::exact_sum::ExactSum {
            let mut total = crate::exact_sum::ExactSum::default();
            let mut flt = filter.map(PredSlots::new);
            let mut value_slot = FieldSlot::new(&fk);
            for r in rows {
                if flt.as_mut().is_none_or(|f| f.eval(r, now)) {
                    total.add_cell(value_slot.get(r));
                }
            }
            total
        };
        #[cfg(feature = "parallel")]
        let total = if rows.len() >= PAR_AGG_MIN_ROWS {
            use rayon::prelude::*;
            // Chunk sums are added in chunk order: the result is deterministic.
            let partials: Vec<crate::exact_sum::ExactSum> =
                rows.par_chunks(PAR_AGG_CHUNK_ROWS).map(sum_rows).collect();
            let mut total = crate::exact_sum::ExactSum::default();
            for p in partials {
                total.merge(p);
            }
            total
        } else {
            sum_rows(rows)
        };
        #[cfg(not(feature = "parallel"))]
        let total = if rows.len() >= PAR_AGG_MIN_ROWS {
            let mut total = crate::exact_sum::ExactSum::default();
            for chunk in rows.chunks(PAR_AGG_CHUNK_ROWS) {
                total.merge(sum_rows(chunk));
            }
            total
        } else {
            sum_rows(rows)
        };
        return Ok(vec![single_cell_row(&fk, Cell::Float(total.value()))]);
    };
    let bk = by.as_str();
    let sum_rows_out = |map: FxHashMap<Cell, crate::exact_sum::ExactSum>| -> Vec<Row> {
        let mut entries: Vec<(Cell, crate::exact_sum::ExactSum)> = map.into_iter().collect();
        entries.sort_unstable_by(|(a, _), (b, _)| cmp_sort(a, b));
        pair_entries_to_rows(&bk, &fk, entries, |n| Cell::Float(n.value()))
    };
    if rows.len() >= PAR_AGG_MIN_ROWS
        && let Some(map) = chunked_group_fold::<crate::exact_sum::ExactSum>(
            rows,
            filter,
            now,
            &bk,
            Some(&fk),
            max_bytes,
            |n, v| n.add_cell(v),
            |n, m| n.merge(m),
        )
    {
        return Ok(sum_rows_out(map));
    }
    let mut map: FxHashMap<Cell, crate::exact_sum::ExactSum> = FxHashMap::default();
    let mut state_bytes = 0usize;
    let mut flt = filter.map(PredSlots::new);
    let mut key_slot = FieldSlot::new(&bk);
    let mut value_slot = FieldSlot::new(&fk);
    for r in rows {
        if !flt.as_mut().is_none_or(|f| f.eval(r, now)) {
            continue;
        }
        let key_ref = key_slot.get(r).unwrap_or(&NULL_CELL);
        let val = value_slot.get(r);
        if let Some(acc) = map.get_mut(key_ref) {
            acc.add_cell(val);
        } else {
            let key_size = match key_ref {
                Cell::Text(s) => s.len() + 32,
                _ => 16,
            };
            state_bytes = state_bytes.saturating_add(key_size + 64);
            if state_bytes > max_bytes {
                if spill {
                    return spill_agg_sum(
                        rows.iter()
                            .filter(|r| filter.is_none_or(|p| eval_pred(p, r, now))),
                        field,
                        Some(by),
                    );
                }
                return Err(Error::runtime(format!(
                    "aggregate state bytes {state_bytes} exceeds max_aggregate_bytes quota {max_bytes}"
                )));
            }
            let mut acc = crate::exact_sum::ExactSum::default();
            acc.add_cell(val);
            map.insert(key_ref.clone_for_borrow(), acc);
        }
    }
    Ok(sum_rows_out(map))
}

pub(crate) fn agg_min(
    rows: &[Row],
    field: &Field,
    by: Option<&Field>,
    max_bytes: usize,
    spill: bool,
) -> Result<Vec<Row>, Error> {
    agg_extremum_filtered(rows, None, 0, field, by, max_bytes, spill, false)
}

pub(crate) fn agg_max(
    rows: &[Row],
    field: &Field,
    by: Option<&Field>,
    max_bytes: usize,
    spill: bool,
) -> Result<Vec<Row>, Error> {
    agg_extremum_filtered(rows, None, 0, field, by, max_bytes, spill, true)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn agg_extremum_filtered(
    rows: &[Row],
    filter: Option<&Pred>,
    now: i64,
    field: &Field,
    by: Option<&Field>,
    max_bytes: usize,
    spill: bool,
    is_max: bool,
) -> Result<Vec<Row>, Error> {
    let fk = field.as_str();
    let is_better = |cand: &Cell, prev: &Cell| -> bool {
        if is_max {
            cmp_sort(cand, prev).is_gt()
        } else {
            cmp_sort(cand, prev).is_lt()
        }
    };
    let Some(by) = by else {
        // Borrowed scan: the winning cell is cloned once at the end, not on every improvement.
        let best_of = |rows: &'_ [Row]| -> Option<Cell> {
            let mut best: Option<&Cell> = None;
            let mut flt = filter.map(PredSlots::new);
            let mut value_slot = FieldSlot::new(&fk);
            for r in rows {
                if flt.as_mut().is_none_or(|f| f.eval(r, now))
                    && let Some(c) = value_slot.get(r).filter(|c| !matches!(c, Cell::Null))
                    && best.is_none_or(|prev| is_better(c, prev))
                {
                    best = Some(c);
                }
            }
            best.map(Cell::clone_for_borrow)
        };
        #[cfg(feature = "parallel")]
        let best: Option<Cell> = if rows.len() >= PAR_AGG_MIN_ROWS {
            use rayon::prelude::*;
            // Chunk winners merge in chunk order with the same strict comparison, so the
            // first of equal values wins exactly as in the sequential loop.
            let partials: Vec<Option<Cell>> =
                rows.par_chunks(PAR_AGG_CHUNK_ROWS).map(best_of).collect();
            let mut best: Option<Cell> = None;
            for c in partials.into_iter().flatten() {
                if best.as_ref().is_none_or(|prev| is_better(&c, prev)) {
                    best = Some(c);
                }
            }
            best
        } else {
            best_of(rows)
        };
        #[cfg(not(feature = "parallel"))]
        let best = best_of(rows);
        return Ok(vec![single_cell_row(&fk, best.unwrap_or(Cell::Null))]);
    };
    let bk = by.as_str();
    let mut map: FxHashMap<Cell, Option<Cell>> = FxHashMap::default();
    let mut state_bytes = 0usize;
    #[allow(unused_mut)]
    let mut seq_rows: &[Row] = rows;
    // Chunk partials merge in order with the same strict comparison, so the first of
    // equal values wins exactly as in the sequential loop.
    #[cfg(feature = "parallel")]
    if rows.len() >= PAR_AGG_MIN_ROWS
        && let Some(folded) = chunked_group_fold::<Option<Cell>>(
            rows,
            filter,
            now,
            &bk,
            Some(&fk),
            max_bytes,
            |best, v| {
                if let Some(c) = v.filter(|c| !matches!(c, Cell::Null))
                    && best.as_ref().is_none_or(|prev| is_better(c, prev))
                {
                    *best = Some(c.clone_for_borrow());
                }
            },
            |best, other| {
                if let Some(c) = other
                    && best.as_ref().is_none_or(|prev| is_better(&c, prev))
                {
                    *best = Some(c);
                }
            },
        )
    {
        map = folded;
        seq_rows = &[];
    }
    let mut flt = filter.map(PredSlots::new);
    let mut key_slot = FieldSlot::new(&bk);
    let mut value_slot = FieldSlot::new(&fk);
    for r in seq_rows {
        if !flt.as_mut().is_none_or(|f| f.eval(r, now)) {
            continue;
        }
        let key_ref = key_slot.get(r).unwrap_or(&NULL_CELL);
        let f_val = value_slot.get(r).filter(|c| !matches!(c, Cell::Null));
        if let Some(entry) = map.get_mut(key_ref) {
            if let Some(c) = f_val
                && entry.as_ref().is_none_or(|prev| is_better(c, prev))
            {
                *entry = Some(c.clone_for_borrow());
            }
        } else {
            let key_size = match key_ref {
                Cell::Text(s) => s.len() + 32,
                _ => 16,
            };
            state_bytes = state_bytes.saturating_add(key_size + 64);
            if state_bytes > max_bytes {
                if spill {
                    return spill_agg_min_max(
                        rows.iter()
                            .filter(|r| filter.is_none_or(|p| eval_pred(p, r, now))),
                        field,
                        Some(by),
                        is_max,
                    );
                }
                return Err(Error::runtime(format!(
                    "aggregate state bytes {state_bytes} exceeds max_aggregate_bytes quota {max_bytes}"
                )));
            }
            map.insert(
                key_ref.clone_for_borrow(),
                f_val.map(Cell::clone_for_borrow),
            );
        }
    }
    let mut entries: Vec<(Cell, Option<Cell>)> = map.into_iter().collect();
    entries.sort_unstable_by(|(a, _), (b, _)| cmp_sort(a, b));
    Ok(pair_entries_to_rows(&bk, &fk, entries, |val| {
        val.unwrap_or(Cell::Null)
    }))
}

pub(crate) fn agg_avg(
    rows: &[Row],
    field: &Field,
    by: Option<&Field>,
    max_bytes: usize,
    spill: bool,
) -> Result<Vec<Row>, Error> {
    agg_avg_filtered(rows, None, 0, field, by, max_bytes, spill)
}

pub(crate) fn agg_avg_filtered(
    rows: &[Row],
    filter: Option<&Pred>,
    now: i64,
    field: &Field,
    by: Option<&Field>,
    max_bytes: usize,
    spill: bool,
) -> Result<Vec<Row>, Error> {
    let fk = field.as_str();
    let Some(by) = by else {
        let fold = |rows: &[Row]| -> (crate::exact_sum::ExactSum, usize) {
            let mut sum = crate::exact_sum::ExactSum::default();
            let mut count = 0usize;
            let mut flt = filter.map(PredSlots::new);
            let mut value_slot = FieldSlot::new(&fk);
            for r in rows {
                if flt.as_mut().is_none_or(|f| f.eval(r, now))
                    && sum.add_numeric(value_slot.get(r))
                {
                    count += 1;
                }
            }
            (sum, count)
        };
        #[cfg(feature = "parallel")]
        let (sum, count) = if rows.len() >= PAR_AGG_MIN_ROWS {
            use rayon::prelude::*;
            // Chunk partials are added in chunk order: the result is deterministic.
            let partials: Vec<(crate::exact_sum::ExactSum, usize)> =
                rows.par_chunks(PAR_AGG_CHUNK_ROWS).map(fold).collect();
            partials.into_iter().fold((crate::exact_sum::ExactSum::default(), 0usize), |mut a, b| {
                a.0.merge(b.0);
                (a.0, a.1 + b.1)
            })
        } else {
            fold(rows)
        };
        #[cfg(not(feature = "parallel"))]
        let (sum, count) = if rows.len() >= PAR_AGG_MIN_ROWS {
            rows.chunks(PAR_AGG_CHUNK_ROWS)
                .map(fold)
                .fold((crate::exact_sum::ExactSum::default(), 0usize), |mut a, b| {
                    a.0.merge(b.0);
                    (a.0, a.1 + b.1)
                })
        } else {
            fold(rows)
        };
        let avg = if count == 0 {
            0.0
        } else {
            sum.value() / (count as f64)
        };
        return Ok(vec![single_cell_row(&fk, Cell::Float(avg))]);
    };
    let bk = by.as_str();
    let mut map: FxHashMap<Cell, (crate::exact_sum::ExactSum, usize)> = FxHashMap::default();
    let mut state_bytes = 0usize;
    let mut seq_rows: &[Row] = rows;
    if rows.len() >= PAR_AGG_MIN_ROWS
        && let Some(folded) = chunked_group_fold::<(crate::exact_sum::ExactSum, usize)>(
            rows,
            filter,
            now,
            &bk,
            Some(&fk),
            max_bytes,
            |acc, v| {
                if acc.0.add_numeric(v) {
                    acc.1 += 1;
                }
            },
            |acc, other| {
                acc.0.merge(other.0);
                acc.1 += other.1;
            },
        )
    {
        map = folded;
        seq_rows = &[];
    }
    let mut flt = filter.map(PredSlots::new);
    let mut key_slot = FieldSlot::new(&bk);
    let mut value_slot = FieldSlot::new(&fk);
    for r in seq_rows {
        if !flt.as_mut().is_none_or(|f| f.eval(r, now)) {
            continue;
        }
        let key_ref = key_slot.get(r).unwrap_or(&NULL_CELL);
        let value = value_slot.get(r);
        if let Some(entry) = map.get_mut(key_ref) {
            if entry.0.add_numeric(value) {
                entry.1 += 1;
            }
        } else {
            let key_size = match key_ref {
                Cell::Text(s) => s.len() + 32,
                _ => 16,
            };
            state_bytes = state_bytes.saturating_add(key_size + 64);
            if state_bytes > max_bytes {
                if spill {
                    return spill_agg_avg(
                        rows.iter()
                            .filter(|r| filter.is_none_or(|p| eval_pred(p, r, now))),
                        field,
                        Some(by),
                    );
                }
                return Err(Error::runtime(format!(
                    "aggregate state bytes {state_bytes} exceeds max_aggregate_bytes quota {max_bytes}"
                )));
            }
            let mut acc = (crate::exact_sum::ExactSum::default(), 0usize);
            if acc.0.add_numeric(value) {
                acc.1 = 1;
            }
            map.insert(key_ref.clone_for_borrow(), acc);
        }
    }
    let mut entries: Vec<(Cell, (crate::exact_sum::ExactSum, usize))> = map.into_iter().collect();
    entries.sort_unstable_by(|(a, _), (b, _)| cmp_sort(a, b));
    Ok(pair_entries_to_rows(&bk, &fk, entries, |(sum, count)| {
        Cell::Float(if count == 0 {
            0.0
        } else {
            sum.value() / (count as f64)
        })
    }))
}

/// Join key of a cell without allocating for text (ints keep joining to equal text).
#[inline]
pub(crate) fn cell_key_ref(c: &Cell) -> Option<std::borrow::Cow<'_, str>> {
    match c {
        Cell::Text(s) => Some(std::borrow::Cow::Borrowed(s.as_ref())),
        Cell::SmallText(s) => Some(std::borrow::Cow::Borrowed(s.as_str())),
        Cell::Int(n) => Some(std::borrow::Cow::Owned(n.to_string())),
        _ => None,
    }
}

pub(crate) fn cell_key(c: &Cell) -> Option<String> {
    match c {
        Cell::Text(s) => Some(s.as_ref().to_owned()),
        Cell::SmallText(s) => Some(s.as_str().to_owned()),
        Cell::Int(n) => Some(n.to_string()),
        _ => None,
    }
}

impl Db {
    pub(crate) fn try_extract_agg_plan(
        q: &Query,
        cat: &crate::catalog::Catalog,
    ) -> Option<AggPlan> {
        if !matches!(q.source, Source::Collection(_)) {
            return None;
        }
        let mut q = crate::catalog::with_catalog_filter(q, cat);
        if q.needs_simplify() {
            q.to_mut().simplify_predicates();
        }
        let q = &*q;
        let name = match &q.source {
            Source::Collection(n) => n.clone(),
            _ => return None,
        };
        let mut filter: Option<Pred> = None;
        let mut kind: Option<AggKind> = None;
        let mut tail: Vec<AggTail> = Vec::new();
        for step in &q.steps {
            match step {
                Step::Filter(p) => {
                    if filter.is_some() || kind.is_some() {
                        return None;
                    }
                    filter = Some(p.clone());
                }
                Step::Count { by } => {
                    if kind.is_some() {
                        return None;
                    }
                    kind = Some(AggKind::Count { by: by.clone() });
                }
                Step::Sum { field, by } => {
                    if kind.is_some() {
                        return None;
                    }
                    kind = Some(AggKind::Sum {
                        field: field.clone(),
                        by: by.clone(),
                    });
                }
                Step::Avg { field, by } => {
                    if kind.is_some() {
                        return None;
                    }
                    kind = Some(AggKind::Avg {
                        field: field.clone(),
                        by: by.clone(),
                    });
                }
                Step::Min { field, by } => {
                    if kind.is_some() {
                        return None;
                    }
                    kind = Some(AggKind::Min {
                        field: field.clone(),
                        by: by.clone(),
                    });
                }
                Step::Max { field, by } => {
                    if kind.is_some() {
                        return None;
                    }
                    kind = Some(AggKind::Max {
                        field: field.clone(),
                        by: by.clone(),
                    });
                }
                // Trailing `skip` / `take` only slice the groups the plan returns; without
                // this the query fell back to materializing the collection. `take all`
                // keeps every group and needs no tail entry.
                Step::Take { n: None } if kind.is_some() => {}
                Step::Take { n: Some(n) } if kind.is_some() => {
                    tail.push(AggTail::Take((*n).max(0) as usize));
                }
                Step::Skip { n } if kind.is_some() => {
                    tail.push(AggTail::Skip((*n).max(0) as usize));
                }
                Step::Sort { field, desc } if kind.is_some() => {
                    tail.push(AggTail::Sort {
                        field: field.clone(),
                        desc: *desc,
                    });
                }
                _ => return None,
            }
            // Nothing but `sort` / `skip` / `take` may follow the first tail step.
            if !tail.is_empty()
                && !matches!(
                    step,
                    Step::Take { .. } | Step::Skip { .. } | Step::Sort { .. }
                )
            {
                return None;
            }
        }
        let kind = kind?;
        if filter.as_ref().is_some_and(pred_has_param) {
            return None;
        }
        let needs_now = filter.as_ref().is_some_and(pred_needs_now);
        let index_plan = filter
            .as_ref()
            .and_then(|pred| crate::index::pick_index_with_coverage(cat, &name, pred));
        let pred_slots = filter.as_ref().map(crate::eval::OwnedPredSlots::new);
        let value_slot = match &kind {
            AggKind::Count { .. } => None,
            AggKind::Sum { field, .. }
            | AggKind::Avg { field, .. }
            | AggKind::Min { field, .. }
            | AggKind::Max { field, .. } => field.leaf().map(crate::store::row::OwnedFieldSlot::new),
        };
        Some(AggPlan {
            collection: name,
            filter,
            pred_slots,
            value_slot,
            kind,
            needs_now,
            index_plan,
            tail,
        })
    }

    fn count_arc_slice_low_cardinality<'a>(
        slice: &'a [Arc<str>],
    ) -> (
        Option<(Arc<str>, i64)>,
        Option<(Arc<str>, i64)>,
        Option<FxHashMap<Arc<str>, i64>>,
    ) {
        let mut slot0_ptr: *const u8 = std::ptr::null();
        let mut slot0_arc: Option<&'a Arc<str>> = None;
        let mut slot0_cnt: i64 = 0;

        let mut slot1_ptr: *const u8 = std::ptr::null();
        let mut slot1_arc: Option<&'a Arc<str>> = None;
        let mut slot1_cnt: i64 = 0;

        let mut overflow: Option<FxHashMap<Arc<str>, i64>> = None;

        for w_arc in slice {
            let w_ptr = Arc::as_ptr(w_arc) as *const u8;
            if w_ptr == slot0_ptr {
                slot0_cnt += 1;
            } else if w_ptr == slot1_ptr {
                slot1_cnt += 1;
            } else if slot0_ptr.is_null() {
                slot0_ptr = w_ptr;
                slot0_arc = Some(w_arc);
                slot0_cnt = 1;
            } else if slot1_ptr.is_null() {
                slot1_ptr = w_ptr;
                slot1_arc = Some(w_arc);
                slot1_cnt = 1;
            } else {
                let w = w_arc.as_ref();
                if slot0_arc.is_some_and(|a| a.as_ref() == w) {
                    slot0_cnt += 1;
                } else if slot1_arc.is_some_and(|a| a.as_ref() == w) {
                    slot1_cnt += 1;
                } else {
                    let map = overflow.get_or_insert_with(FxHashMap::default);
                    if let Some(cnt) = map.get_mut(w) {
                        *cnt += 1;
                    } else {
                        map.insert(Arc::clone(w_arc), 1);
                    }
                }
            }
        }

        let s0 = slot0_arc.map(|arc| (Arc::clone(arc), slot0_cnt));
        let s1 = slot1_arc.map(|arc| (Arc::clone(arc), slot1_cnt));
        (s0, s1, overflow)
    }

    fn execute_disk_agg_plan(&self, plan: &AggPlan, now: i64) -> Result<Option<Vec<Row>>, Error> {
        let col = &plan.collection;
        if !self.store.is_disk_backed(col) {
            return Ok(None);
        }
        let Some(eng) = self.store.disk_engine_read() else {
            return Ok(None);
        };
        let max_bytes = self.quotas.max_aggregate_bytes;
        let spill = self.quotas.spill_enabled;

        // Fast path 1: Ungrouped & unfiltered total count -> O(1) segment + memtable count
        if let AggKind::Count { by: None } = &plan.kind
            && plan.filter.is_none()
        {
            return Ok(Some(vec![hits_row(eng.count_table(col) as i64)]));
        }

        // Fast path 2: Equality filter count -> O(1) MmapHashIndex lookup
        if let AggKind::Count { by } = &plan.kind
            && let Some(Pred::Cmp {
                field,
                op: CmpOp::Eq,
                value,
            }) = &plan.filter
            && let Some(fname) = field.leaf()
        {
            let cell = super::eval::value_cell(value, now);
            if !matches!(cell, Cell::Null) {
                if by.is_none() {
                    let n = eng.count_by_eq(col, fname, &cell)?;
                    return Ok(Some(vec![hits_row(n as i64)]));
                } else if let Some(by_f) = by
                    && by_f.leaf() == Some(fname)
                {
                    let n = eng.count_by_eq(col, fname, &cell)?;
                    if n == 0 {
                        return Ok(Some(Vec::new()));
                    }
                    let mut row = crate::store::acquire_row();
                    row.insert(by_f.as_str(), cell);
                    row.insert("hits", Cell::Int(n as i64));
                    return Ok(Some(vec![row]));
                }
            }
        }

        let col_kind = |fname: &str| disk_col_kind(&eng, col, fname);

        // Fast path 2a/2b: range comparison (optionally AND-ed with a text equality) or
        // substring count -> zone-map block skipping / block-level SIMD `memmem`,
        // no `Row` materialization.
        if let AggKind::Count { by } = &plan.kind
            && let Some(pred) = &plan.filter
            && let Some((n, eq)) = disk_zero_row_count(&eng, col, pred, now)?
        {
            match (by, eq) {
                (None, _) => return Ok(Some(vec![hits_row(n as i64)])),
                // Grouping on the equality column yields at most one group.
                (Some(b), Some((eq_leaf, s))) if b.leaf() == Some(eq_leaf) => {
                    if n == 0 {
                        return Ok(Some(Vec::new()));
                    }
                    let mut row = crate::store::acquire_row();
                    row.insert(b.as_str(), Cell::from_text(s));
                    row.insert("hits", Cell::Int(n as i64));
                    return Ok(Some(vec![row]));
                }
                _ => {}
            }
        }

        // Fast path 2c: unfiltered SUM over a fixed-width column (optionally grouped).
        if let AggKind::Sum { field, by } = &plan.kind
            && plan.filter.is_none()
            && let Some(fname) = field.leaf()
            && matches!(
                col_kind(fname),
                Some(crate::disk::DiskColKind::Int64 | crate::disk::DiskColKind::Float64)
            )
        {
            match by {
                None => {
                    if let Some(total) = eng.sum_f64_col(col, fname)? {
                        let mut row = crate::store::acquire_row();
                        row.insert(field.as_str(), Cell::Float(total));
                        return Ok(Some(vec![row]));
                    }
                }
                Some(by_field) => {
                    if let Some(by_name) = by_field.leaf()
                        && let Some(mut entries) = eng.sum_f64_by_col_entries(col, fname, by_name)?
                    {
                        let state_bytes: usize = entries
                            .iter()
                            .map(|(cell, _)| match cell {
                                Cell::Text(s) => s.len() + 32 + 64,
                                _ => 16 + 64,
                            })
                            .sum();
                        // Over quota: let the general path decide between spill and error.
                        if state_bytes <= max_bytes {
                            let bk = by_field.as_str();
                            let fk = field.as_str();
                            entries.sort_unstable_by(|(a, _), (b, _)| cmp_sort(a, b));
                            let rows = match bk.cmp(&fk) {
                                std::cmp::Ordering::Less => {
                                    let shape = Row::shape_of(&[bk.as_str(), fk.as_str()]);
                                    entries
                                        .into_iter()
                                        .map(|(cell, n)| {
                                            Row::from_parts(
                                                Arc::clone(&shape),
                                                [cell, Cell::Float(n)],
                                            )
                                        })
                                        .collect()
                                }
                                std::cmp::Ordering::Greater => {
                                    let shape = Row::shape_of(&[fk.as_str(), bk.as_str()]);
                                    entries
                                        .into_iter()
                                        .map(|(cell, n)| {
                                            Row::from_parts(
                                                Arc::clone(&shape),
                                                [Cell::Float(n), cell],
                                            )
                                        })
                                        .collect()
                                }
                                std::cmp::Ordering::Equal => {
                                    let shape = Row::shape_of(&[fk.as_str()]);
                                    entries
                                        .into_iter()
                                        .map(|(_, n)| {
                                            Row::from_parts(Arc::clone(&shape), [Cell::Float(n)])
                                        })
                                        .collect()
                                }
                            };
                            return Ok(Some(rows));
                        }
                    }
                }
            }
        }

        // Fast path 3: Unfiltered single-column text/dict group count -> zero-Row columnar scan
        if let AggKind::Count { by: Some(by_field) } = &plan.kind
            && plan.filter.is_none()
            && let Some(fname) = by_field.leaf()
            && eng
                .manifest
                .tables
                .get(col)
                .and_then(|t| t.columns.iter().find(|c| c.name == fname))
                .is_some_and(|c| {
                    matches!(
                        c.kind,
                        crate::disk::DiskColKind::TextDict | crate::disk::DiskColKind::TextZstd
                    )
                })
        {
            let map = eng.group_count_cells1(col, fname)?;
            if !spill {
                let mut state_bytes = 0usize;
                for cell in map.keys() {
                    let key_size = match cell {
                        Cell::Text(s) => s.len() + 32,
                        _ => 16,
                    };
                    state_bytes = state_bytes.saturating_add(key_size + 64);
                    if state_bytes > max_bytes {
                        return Err(Error::runtime(format!(
                            "aggregate state bytes {state_bytes} exceeds max_aggregate_bytes quota {max_bytes}"
                        )));
                    }
                }
            }
            return Ok(Some(count_cell_map_to_rows(&by_field.as_str(), map)));
        }

        // General out-of-core filtered/projected aggregation without promoting table into Store::collections
        let mut needed: Vec<String> = Vec::new();
        match &plan.kind {
            AggKind::Count { by } => {
                if let Some(b) = by
                    && let Some(lf) = b.leaf()
                {
                    needed.push(lf.to_string());
                }
            }
            AggKind::Sum { field, by }
            | AggKind::Avg { field, by }
            | AggKind::Min { field, by }
            | AggKind::Max { field, by } => {
                if let Some(lf) = field.leaf() {
                    needed.push(lf.to_string());
                }
                if let Some(b) = by
                    && let Some(lf) = b.leaf()
                    && !needed.iter().any(|x| x == lf)
                {
                    needed.push(lf.to_string());
                }
            }
        }
        // `query_filter` adds the predicate's own columns to the scan set, so projection is safe
        // with a filter too (it bails to a full-row scan only for non-leaf predicate fields).
        let can_project_only = {
            match &plan.kind {
                AggKind::Count { by: Some(b) } => b.leaf().is_some(),
                AggKind::Count { by: None } => true,
                AggKind::Sum { field, by }
                | AggKind::Avg { field, by }
                | AggKind::Min { field, by }
                | AggKind::Max { field, by } => {
                    field.leaf().is_some() && by.as_ref().is_none_or(|b| b.leaf().is_some())
                }
            }
        };
        let proj_refs: Vec<&str> = needed.iter().map(String::as_str).collect();
        let proj_arg = if can_project_only {
            Some(proj_refs.as_slice())
        } else {
            None
        };
        let rows = eng.query_filter(col, plan.filter.as_ref(), None, proj_arg, now)?;
        let res = match &plan.kind {
            AggKind::Count { by } => match by {
                None => Ok(vec![hits_row(rows.len() as i64)]),
                Some(b) => agg_count_filtered(&rows, None, now, b, max_bytes, spill),
            },
            AggKind::Sum { field, by } => {
                agg_sum_filtered(&rows, None, now, field, by.as_ref(), max_bytes, spill)
            }
            AggKind::Avg { field, by } => {
                agg_avg_filtered(&rows, None, now, field, by.as_ref(), max_bytes, spill)
            }
            AggKind::Min { field, by } => agg_extremum_filtered(
                &rows,
                None,
                now,
                field,
                by.as_ref(),
                max_bytes,
                spill,
                false,
            ),
            AggKind::Max { field, by } => {
                agg_extremum_filtered(&rows, None, now, field, by.as_ref(), max_bytes, spill, true)
            }
        }?;
        Ok(Some(res))
    }

    pub(crate) fn execute_agg_plan(&self, plan: &AggPlan, now: i64) -> Result<Vec<Row>, Error> {
        let mut rows = self.execute_agg_plan_groups(plan, now)?;
        for step in &plan.tail {
            match step {
                AggTail::Skip(n) => {
                    rows.drain(..(*n).min(rows.len()));
                }
                AggTail::Take(n) => rows.truncate(*n),
                AggTail::Sort { field, desc } => {
                    super::eval::sort_rows_top_k(&mut rows, field, *desc, None);
                }
            }
        }
        Ok(rows)
    }

    fn execute_agg_plan_groups(&self, plan: &AggPlan, now: i64) -> Result<Vec<Row>, Error> {
        if let Some(rows) = self.execute_disk_agg_plan(plan, now)? {
            return Ok(rows);
        }
        if let AggKind::Count { by: None } = &plan.kind
            && let Some(pred) = &plan.filter
            && let Some(rows) = self.execute_filter_count_cached(
                &plan.collection,
                pred,
                None,
                now,
                plan.index_plan.as_ref(),
            )
        {
            return Ok(rows);
        }
        if let AggKind::Count { by: Some(by) } = &plan.kind
            && let Some(pred) = &plan.filter
            && let Some(value) = eq_value_for_field(pred, &by.as_str())
        {
            let fresh_idx = if plan.index_plan.is_none() {
                crate::index::pick_index_with_coverage(&self.catalog, &plan.collection, pred)
            } else {
                None
            };
            if let Some((uses, true)) = plan.index_plan.as_ref().or(fresh_idx.as_ref())
                && let Some(n) = self.store.index_seek_count(&plan.collection, uses, now)
            {
                if n == 0 {
                    return Ok(Vec::new());
                }
                let state_bytes = value.len().saturating_add(32).saturating_add(64);
                if state_bytes > self.quotas.max_aggregate_bytes {
                    return Err(Error::runtime(format!(
                        "aggregate state bytes {state_bytes} exceeds max_aggregate_bytes quota {}",
                        self.quotas.max_aggregate_bytes
                    )));
                }
                return Ok(vec![count_row(&by.as_str(), value, n as i64)]);
            }
        }
        // 1. SoA paths for orders
        if plan.collection == "orders" && self.store.orders_soa_ready() {
            let max_bytes = self.quotas.max_aggregate_bytes;
            match &plan.kind {
                AggKind::Count { by: Some(by_field) }
                    if by_field.leaf() == Some("user_id") && plan.filter.is_none() =>
                {
                    let out_key = by_field.as_str();
                    if let Some(live_idx) = self.store.indexes.values().find(|idx| {
                        idx.def.collection == "orders"
                            && idx.def.pred.is_none()
                            && idx.def.fields.len() == 1
                            && idx.def.fields[0] == "user_id"
                    }) {
                        let mut entries: Vec<(Cell, i64)> =
                            Vec::with_capacity(live_idx.forward.len());
                        let mut state_bytes = 0usize;
                        let mut ok_quota = true;
                        for (key, list) in live_idx.forward.iter() {
                            let c = list.len() as i64;
                            if c == 0 {
                                continue;
                            }
                            let uid_cell = match key.0.first() {
                                Some(crate::index::IndexPart::Text(s)) => {
                                    state_bytes = state_bytes.saturating_add(s.len() + 32 + 64);
                                    Cell::Text(Arc::clone(s))
                                }
                                Some(crate::index::IndexPart::SmallText(s)) => {
                                    state_bytes = state_bytes.saturating_add(16 + 64);
                                    Cell::SmallText(*s)
                                }
                                _ => continue,
                            };
                            if state_bytes > max_bytes {
                                ok_quota = false;
                                break;
                            }
                            entries.push((uid_cell, c));
                        }
                        if ok_quota {
                            return Ok(pair_entries_to_rows(&out_key, "hits", entries, Cell::Int));
                        }
                    } else {
                        let uids = self.store.orders_user_id();
                        let mut map: FxHashMap<Arc<str>, i64> = FxHashMap::default();
                        map.reserve((uids.len() / 8).clamp(64, 4096));
                        let mut state_bytes = 0usize;
                        let mut ok_quota = true;
                        for uid in uids {
                            if let Some(cnt) = map.get_mut(uid.as_ref()) {
                                *cnt += 1;
                            } else {
                                let ksz = if uid.len() <= 22 { 16 } else { uid.len() + 32 };
                                state_bytes = state_bytes.saturating_add(ksz + 64);
                                if state_bytes > max_bytes {
                                    ok_quota = false;
                                    break;
                                }
                                map.insert(Arc::clone(uid), 1);
                            }
                        }
                        if ok_quota {
                            let mut entries: Vec<(Arc<str>, i64)> = map.into_iter().collect();
                            entries.sort_unstable_by(|(a, _), (b, _)| a.cmp(b));
                            let cell_entries: Vec<(Cell, i64)> = entries
                                .into_iter()
                                .map(|(uid, c)| (Cell::Text(uid), c))
                                .collect();
                            return Ok(pair_entries_to_rows(
                                &out_key,
                                "hits",
                                cell_entries,
                                Cell::Int,
                            ));
                        }
                    }
                }
                AggKind::Sum { field, by } if field.leaf() == Some("total") => {
                    if by.is_none() && plan.filter.is_none() {
                        let total: f64 = self.store.orders_total().iter().sum();
                        return Ok(vec![single_cell_row("total", Cell::Float(total))]);
                    }
                    if let Some(by_field) = by
                        && by_field.leaf() == Some("user_id")
                    {
                        if plan.filter.is_none() {
                            if let Some(live_idx) = self.store.indexes.values().find(|idx| {
                                idx.def.collection == "orders"
                                    && idx.def.pred.is_none()
                                    && !idx.def.fields.is_empty()
                                    && idx.def.fields[0] == "user_id"
                            }) {
                                let totals = self.store.orders_total();
                                let single_col = live_idx.def.fields.len() == 1;
                                let mut entries: Vec<(Cell, f64)> =
                                    Vec::with_capacity(live_idx.forward.len());
                                for (key, list) in live_idx.forward.iter() {
                                    let uid_cell = match key.0.first() {
                                        Some(crate::index::IndexPart::Text(s)) => {
                                            Cell::Text(Arc::clone(s))
                                        }
                                        Some(crate::index::IndexPart::SmallText(s)) => {
                                            Cell::SmallText(*s)
                                        }
                                        _ => continue,
                                    };
                                    let mut sum = 0.0f64;
                                    match list {
                                        crate::index::PostingList::Standard(vec) => {
                                            for &pos in vec {
                                                sum += totals.get(pos).copied().unwrap_or(0.0);
                                            }
                                        }
                                        crate::index::PostingList::Bitmap(set) => {
                                            set.for_each(|pos| {
                                                sum += totals.get(pos).copied().unwrap_or(0.0);
                                            });
                                        }
                                    }
                                    if !single_col
                                        && let Some((last_k, last_sum)) = entries.last_mut()
                                        && *last_k == uid_cell
                                    {
                                        *last_sum += sum;
                                    } else {
                                        entries.push((uid_cell, sum));
                                    }
                                }
                                return Ok(pair_entries_to_rows(
                                    "user_id",
                                    "total",
                                    entries,
                                    Cell::Float,
                                ));
                            }
                            let uids = self.store.orders_user_id();
                            let totals = self.store.orders_total();
                            let total_len = totals.len();
                            let mut map: FxHashMap<Arc<str>, f64> = FxHashMap::default();
                            map.reserve((total_len / 8).clamp(64, 4096));
                            for (uid, &tot) in uids.iter().zip(totals) {
                                if let Some(acc) = map.get_mut(uid.as_ref()) {
                                    *acc += tot;
                                } else {
                                    map.insert(Arc::clone(uid), tot);
                                }
                            }
                            let mut entries: Vec<(Arc<str>, f64)> = map.into_iter().collect();
                            entries.sort_unstable_by(|(a, _), (b, _)| a.cmp(b));
                            let cell_entries: Vec<(Cell, f64)> = entries
                                .into_iter()
                                .map(|(uid, tot)| (Cell::Text(uid), tot))
                                .collect();
                            return Ok(pair_entries_to_rows(
                                "user_id",
                                "total",
                                cell_entries,
                                Cell::Float,
                            ));
                        } else if let Some(Pred::Cmp {
                            field: filt_field,
                            op: CmpOp::Gt,
                            value,
                        }) = &plan.filter
                            && filt_field.leaf() == Some("total")
                        {
                            let thresh = match value {
                                Value::Float(f) => Some(*f),
                                Value::Int(n) => Some(*n as f64),
                                _ => None,
                            };
                            if let Some(threshold) = thresh {
                                let uids = self.store.orders_user_id();
                                let totals = self.store.orders_total();
                                let mut map: FxHashMap<Arc<str>, f64> = FxHashMap::default();
                                map.reserve((totals.len() / 8).clamp(64, 4096));
                                for (uid, &tot) in uids.iter().zip(totals) {
                                    if tot > threshold {
                                        if let Some(acc) = map.get_mut(uid.as_ref()) {
                                            *acc += tot;
                                        } else {
                                            map.insert(Arc::clone(uid), tot);
                                        }
                                    }
                                }
                                let mut entries: Vec<(Arc<str>, f64)> = map.into_iter().collect();
                                entries.sort_unstable_by(|(a, _), (b, _)| a.cmp(b));
                                let cell_entries: Vec<(Cell, f64)> = entries
                                    .into_iter()
                                    .map(|(uid, tot)| (Cell::Text(uid), tot))
                                    .collect();
                                return Ok(pair_entries_to_rows(
                                    "user_id",
                                    "total",
                                    cell_entries,
                                    Cell::Float,
                                ));
                            }
                        }
                    }
                }
                AggKind::Avg { field, by }
                    if field.leaf() == Some("total") && by.is_none() && plan.filter.is_none() =>
                {
                    let totals = self.store.orders_total();
                    let avg = if totals.is_empty() {
                        0.0
                    } else {
                        totals.iter().sum::<f64>() / totals.len() as f64
                    };
                    return Ok(vec![single_cell_row("total", Cell::Float(avg))]);
                }
                _ => {}
            }
        }

        // 2. SoA paths for docs (`wing` and `layer`)
        if plan.collection == "docs"
            && !self.store.is_disk_backed("docs")
            && plan.filter.is_none()
            && let AggKind::Count { by: Some(by_field) } = &plan.kind
            && let Some(col_leaf @ ("wing" | "layer")) = by_field.leaf()
        {
            let docs_len = self.store.collection("docs").len();
            let dict_col = if col_leaf == "wing" {
                self.store.docs_wing()
            } else {
                self.store.docs_layer()
            };
            if dict_col.len() == docs_len {
                let entries: Vec<(Arc<str>, i64)> =
                    if let Some((indices, dict)) = dict_col.as_indices() {
                        let mut counts0 = [0i64; 256];
                        let mut counts1 = [0i64; 256];
                        let mut counts2 = [0i64; 256];
                        let mut counts3 = [0i64; 256];
                        let chunks = indices.chunks_exact(4);
                        let rem = chunks.remainder();
                        for c in chunks {
                            counts0[c[0] as usize] += 1;
                            counts1[c[1] as usize] += 1;
                            counts2[c[2] as usize] += 1;
                            counts3[c[3] as usize] += 1;
                        }
                        for &b in rem {
                            counts0[b as usize] += 1;
                        }
                        let mut v: Vec<(Arc<str>, i64)> = Vec::with_capacity(dict.len());
                        for (idx, arc) in dict.iter().enumerate() {
                            let c = counts0[idx] + counts1[idx] + counts2[idx] + counts3[idx];
                            if c > 0 {
                                v.push((Arc::clone(arc), c));
                            }
                        }
                        v.sort_unstable_by(|(a, _), (b, _)| a.cmp(b));
                        v
                    } else {
                        let borrowed;
                        let col_slice: &[Arc<str>] = match dict_col.as_spilled() {
                            Some(s) => s,
                            None => {
                                borrowed = dict_col.to_vec();
                                &borrowed
                            }
                        };
                        let (s0, s1, overflow) = Self::count_arc_slice_low_cardinality(col_slice);
                        let mut combined: FxHashMap<Arc<str>, i64> = overflow.unwrap_or_default();
                        if let Some((k, v)) = s0 {
                            *combined.entry(k).or_insert(0) += v;
                        }
                        if let Some((k, v)) = s1 {
                            *combined.entry(k).or_insert(0) += v;
                        }
                        let mut v: Vec<(Arc<str>, i64)> = combined.into_iter().collect();
                        v.sort_unstable_by(|(a, _), (b, _)| a.cmp(b));
                        v
                    };
                let state_bytes: usize = entries
                    .iter()
                    .map(|(arc, _)| (if arc.len() <= 22 { 16 } else { arc.len() + 32 }) + 64)
                    .sum();
                if state_bytes <= self.quotas.max_aggregate_bytes {
                    let cell_entries: Vec<(Cell, i64)> = entries
                        .into_iter()
                        .map(|(arc, count)| (Cell::Text(arc), count))
                        .collect();
                    return Ok(pair_entries_to_rows(
                        col_leaf,
                        "hits",
                        cell_entries,
                        Cell::Int,
                    ));
                }
            }
        }

        // 2b. ColMirror (declared or schema-driven SoA) fast paths for general collections
        if let Some(mirror) = self.get_or_build_mirror(&plan.collection) {
            match &plan.kind {
                AggKind::Count { by: None } => {
                    if let Some(ref p) = plan.filter {
                        if let Some(n) = mirror.count(p, now) {
                            return Ok(vec![hits_row(n as i64)]);
                        }
                    } else {
                        return Ok(vec![hits_row(mirror.len as i64)]);
                    }
                }
                AggKind::Count { by: Some(by_field) } => {
                    if let Some(by_leaf) = by_field.leaf()
                        && let Some(map) = mirror.group_count(by_leaf, plan.filter.as_ref(), now)
                    {
                        let state_bytes: usize = map
                            .keys()
                            .map(|c| match c {
                                Cell::Text(s) => s.len() + 32 + 64,
                                _ => 16 + 64,
                            })
                            .sum();
                        if state_bytes <= self.quotas.max_aggregate_bytes {
                            return Ok(count_cell_map_to_rows(&by_field.as_str(), map));
                        }
                    }
                }
                AggKind::Sum { field, by: None } | AggKind::Avg { field, by: None } => {
                    if let Some(val_leaf) = field.leaf()
                        && let Some((sum, cnt)) =
                            mirror.sum_and_count(val_leaf, plan.filter.as_ref(), now)
                    {
                        let fk = field.as_str();
                        let out = if matches!(plan.kind, AggKind::Sum { .. }) {
                            sum.value()
                        } else if cnt == 0 {
                            0.0
                        } else {
                            sum.value() / (cnt as f64)
                        };
                        return Ok(vec![single_cell_row(&fk, Cell::Float(out))]);
                    }
                }
                AggKind::Sum {
                    field,
                    by: Some(by_field),
                } => {
                    if let (Some(by_leaf), Some(val_leaf)) = (by_field.leaf(), field.leaf())
                        && by_leaf != val_leaf
                        && let Some(mut entries) =
                            mirror.group_sum(by_leaf, val_leaf, plan.filter.as_ref(), now)
                    {
                        let state_bytes: usize = entries
                            .iter()
                            .map(|(kcell, _)| match kcell {
                                Cell::Text(s) => s.len() + 32 + 64,
                                _ => 16 + 64,
                            })
                            .sum();
                        if state_bytes <= self.quotas.max_aggregate_bytes {
                            entries.sort_unstable_by(|(a, _), (b, _)| cmp_sort(a, b));
                            return Ok(pair_entries_to_rows(
                                &by_field.as_str(),
                                &field.as_str(),
                                entries,
                                Cell::Float,
                            ));
                        }
                    }
                }
                _ => {}
            }
        }

        // 3. Index-sought ungrouped Sum / Avg / Min / Max fast path (no full table scan, no Row clone!)
        if let Some((ref uses, covered)) = plan.index_plan
            && let Some(idxs) = self.store.index_seek(&plan.collection, uses, now)
        {
            let col = self.store.collection(&plan.collection);
            let residual_slots = if covered {
                None
            } else {
                plan.pred_slots.as_ref()
            };
            match &plan.kind {
                AggKind::Sum { field, by: None } => {
                    let fk = field.as_str();
                    let mut total = crate::exact_sum::ExactSum::default();
                    for i in idxs {
                        if let Some(r) = col.get(i)
                            && residual_slots.is_none_or(|f| f.eval_sync(r, now))
                        {
                            let cell = match &plan.value_slot {
                                Some(vs) => vs.get_sync(r),
                                None => r.get(&fk),
                            };
                            total.add_cell(cell);
                        }
                    }
                    return Ok(vec![single_cell_row(&fk, Cell::Float(total.value()))]);
                }
                AggKind::Avg { field, by: None } => {
                    let fk = field.as_str();
                    let mut sum = crate::exact_sum::ExactSum::default();
                    let mut count = 0usize;
                    for i in idxs {
                        if let Some(r) = col.get(i)
                            && residual_slots.is_none_or(|f| f.eval_sync(r, now))
                            && sum.add_numeric(match &plan.value_slot {
                                Some(vs) => vs.get_sync(r),
                                None => r.get(&fk),
                            })
                        {
                            count += 1;
                        }
                    }
                    let avg = if count == 0 {
                        0.0
                    } else {
                        sum.value() / (count as f64)
                    };
                    return Ok(vec![single_cell_row(&fk, Cell::Float(avg))]);
                }
                AggKind::Min { field, by: None } | AggKind::Max { field, by: None } => {
                    let is_max = matches!(plan.kind, AggKind::Max { .. });
                    let fk = field.as_str();
                    let mut best: Option<Cell> = None;
                    for i in idxs {
                        if let Some(r) = col.get(i)
                            && residual_slots.is_none_or(|f| f.eval_sync(r, now))
                            && let Some(c) = match &plan.value_slot {
                                Some(vs) => vs.get_sync(r),
                                None => r.get(&fk),
                            }
                            .filter(|c| !matches!(c, Cell::Null))
                            && best.as_ref().is_none_or(|prev| {
                                if is_max {
                                    cmp_sort(c, prev).is_gt()
                                } else {
                                    cmp_sort(c, prev).is_lt()
                                }
                            })
                        {
                            best = Some(c.clone_for_borrow());
                        }
                    }
                    return Ok(vec![single_cell_row(&fk, best.unwrap_or(Cell::Null))]);
                }
                _ => {}
            }
        }

        // 4. Generic fallback: iterate `&[Row]` without cloning!
        let col = self.store.collection(&plan.collection);
        let filter = plan.filter.as_ref();
        let max_bytes = self.quotas.max_aggregate_bytes;
        let spill = self.quotas.spill_enabled;
        match &plan.kind {
            AggKind::Count { by } => {
                let Some(by) = by else {
                    let n = match filter {
                        Some(p) => scoped_par_filter_count(col, p, now),
                        None => col.len() as i64,
                    };
                    return Ok(vec![hits_row(n)]);
                };
                agg_count_filtered(col, filter, now, by, max_bytes, spill)
            }
            AggKind::Sum { field, by } => {
                agg_sum_filtered(col, filter, now, field, by.as_ref(), max_bytes, spill)
            }
            AggKind::Avg { field, by } => {
                agg_avg_filtered(col, filter, now, field, by.as_ref(), max_bytes, spill)
            }
            AggKind::Min { field, by } => agg_extremum_filtered(
                col,
                filter,
                now,
                field,
                by.as_ref(),
                max_bytes,
                spill,
                false,
            ),
            AggKind::Max { field, by } => {
                agg_extremum_filtered(col, filter, now, field, by.as_ref(), max_bytes, spill, true)
            }
        }
    }

    pub(crate) fn try_agg(
        &self,
        q: &Query,
        bindings: &BTreeMap<String, Vec<Row>>,
        now: i64,
    ) -> Result<Option<Vec<Row>>, Error> {
        if let Source::Collection(name) = &q.source
            && bindings.get(name).is_none()
            && let Some(plan) = Self::try_extract_agg_plan(q, &self.catalog)
        {
            Ok(Some(self.execute_agg_plan(&plan, now)?))
        } else {
            Ok(None)
        }
    }

    pub(crate) fn try_filter_count(
        &self,
        q: &Query,
        bindings: &BTreeMap<String, Vec<Row>>,
        now: i64,
    ) -> Option<Vec<Row>> {
        let Source::Collection(name) = &q.source else {
            return None;
        };
        if bindings.get(name).is_some() {
            return None;
        }
        let mut filter: Option<&Pred> = None;
        let mut by: Option<Option<&Field>> = None;
        for step in &q.steps {
            match step {
                Step::Filter(p) => {
                    if filter.is_some() || by.is_some() {
                        return None;
                    }
                    filter = Some(p);
                }
                Step::Count { by: b } => {
                    if by.is_some() {
                        return None;
                    }
                    by = Some(b.as_ref());
                }
                _ => return None,
            }
        }
        self.execute_filter_count_cached(name, filter?, by?, now, None)
    }

    pub(crate) fn execute_filter_count_cached(
        &self,
        name: &str,
        pred: &Pred,
        by: Option<&Field>,
        now: i64,
        cached_idx: Option<&(Vec<crate::index::IndexUse>, bool)>,
    ) -> Option<Vec<Row>> {
        if self.store.is_disk_backed(name)
            && let Some(eng) = self.store.disk_engine_read()
        {
            if let Pred::Cmp {
                field,
                op: CmpOp::Eq,
                value,
            } = pred
                && let Some(fname) = field.leaf()
            {
                let cell = super::eval::value_cell(value, now);
                if !matches!(cell, Cell::Null) {
                    if by.is_none()
                        && let Ok(n) = eng.count_by_eq(name, fname, &cell)
                    {
                        return Some(vec![hits_row(n as i64)]);
                    }
                    if let Some(by_f) = by
                        && by_f.leaf() == Some(fname)
                        && let Ok(n) = eng.count_by_eq(name, fname, &cell)
                    {
                        if n == 0 {
                            return Some(Vec::new());
                        }
                        let mut row = crate::store::acquire_row();
                        row.insert(by_f.as_str(), cell);
                        row.insert("hits", Cell::Int(n as i64));
                        return Some(vec![row]);
                    }
                }
            }
            if by.is_none()
                && let Ok(Some((n, _))) = disk_zero_row_count(&eng, name, pred, now)
            {
                return Some(vec![hits_row(n as i64)]);
            }
            // Only the group column (if any) is needed downstream; `query_filter` adds the
            // predicate's own columns to the scan set.
            let by_leaf: Option<[&str; 1]> = by.and_then(Field::leaf).map(|l| [l]);
            let proj: Option<&[&str]> = match (by, &by_leaf) {
                (None, _) => Some(&[]),
                (Some(_), Some(l)) => Some(l.as_slice()),
                (Some(_), None) => None,
            };
            if let Ok(rows) = eng.query_filter(name, Some(pred), None, proj, now) {
                let Some(by_field) = by else {
                    return Some(vec![hits_row(rows.len() as i64)]);
                };
                let by_key = by_field.as_str();
                let mut key_slot = FieldSlot::new(&by_key);
                let mut map: FxHashMap<Cell, i64> = FxHashMap::default();
                for r in &rows {
                    let key_ref = key_slot.get(r).unwrap_or(&NULL_CELL);
                    if let Some(cnt) = map.get_mut(key_ref) {
                        *cnt += 1;
                    } else {
                        map.insert(key_ref.clone_for_borrow(), 1);
                    }
                }
                return Some(count_cell_map_to_rows(&by_key, map));
            }
        }
        if name == "docs"
            && !self.store.is_disk_backed("docs")
            && by.is_none()
            && let Pred::Contains { field, needle } = pred
            && field.leaf() == Some("title")
        {
            let n = self.store.docs_title_contains_count(needle);
            return Some(vec![hits_row(n)]);
        }

        if name == "docs"
            && !self.store.is_disk_backed("docs")
            && let Some(by_field) = by
            && let Pred::Contains { field, needle } = pred
            && field.leaf() == Some("title")
            && by_field.leaf() == Some("layer")
        {
            let map = self.store.docs_title_contains_count_by_layer(needle);
            return Some(count_arc_map_to_rows(&by_field.as_str(), map));
        }

        let Some(by_field) = by else {
            if let Some((uses, true)) = cached_idx {
                let n = self.store.index_seek_count(name, uses, now)?;
                return Some(vec![hits_row(n as i64)]);
            }
            if cached_idx.is_none()
                && let Some((uses, true)) =
                    crate::index::pick_index_with_coverage(&self.catalog, name, pred)
            {
                let n = self.store.index_seek_count(name, &uses, now)?;
                return Some(vec![hits_row(n as i64)]);
            }
            if let Some(n) = self.mirror_count(name, pred, now) {
                return Some(vec![hits_row(n as i64)]);
            }
            let col = self.store.collection(name);
            return Some(vec![hits_row(scoped_par_filter_count(col, pred, now))]);
        };
        let by_key = by_field.as_str();

        let fresh_idx = if cached_idx.is_none() {
            crate::index::pick_index_with_coverage(&self.catalog, name, pred)
        } else {
            None
        };
        let eff_idx = cached_idx.or(fresh_idx.as_ref());

        if let Some((uses, true)) = eff_idx {
            if let Some(v) = eq_value_for_field(pred, &by_key) {
                let n = self.store.index_seek_count(name, uses, now)?;
                if n == 0 {
                    return Some(Vec::new());
                }
                return Some(vec![count_row(&by_key, v, n as i64)]);
            }
            let idxs = self.store.index_seek(name, uses, now)?;
            let col_rows = self.store.collection(name);
            let mut key_slot = FieldSlot::new(&by_key);
            let mut map: FxHashMap<Cell, i64> = FxHashMap::default();
            for i in idxs {
                let Some(row) = col_rows.get(i) else {
                    continue;
                };
                let key_ref = key_slot.get(row).unwrap_or(&NULL_CELL);
                if let Some(cnt) = map.get_mut(key_ref) {
                    *cnt += 1;
                } else {
                    map.insert(key_ref.clone_for_borrow(), 1);
                }
            }
            return Some(count_cell_map_to_rows(&by_key, map));
        }

        if let Some(by_leaf) = by_field.leaf()
            && let Some(mirror) = self.get_or_build_mirror(name)
            && let Some(map) = mirror.group_count(by_leaf, Some(pred), now)
        {
            return Some(count_cell_map_to_rows(&by_key, map));
        }

        let col = self.store.collection(name);
        if let Some(idxs) = self.mirror_select(name, pred, now) {
            let mut map: FxHashMap<Cell, i64> = FxHashMap::default();
            let mut key_slot = FieldSlot::new(&by_key);
            for i in idxs {
                let Some(row) = col.get(i) else {
                    continue;
                };
                let key_ref = key_slot.get(row).unwrap_or(&NULL_CELL);
                if let Some(cnt) = map.get_mut(key_ref) {
                    *cnt += 1;
                } else {
                    map.insert(key_ref.clone_for_borrow(), 1);
                }
            }
            return Some(count_cell_map_to_rows(&by_key, map));
        }

        if let Some(map) = scoped_par_count_by(col, Some(pred), now, &by_key, usize::MAX) {
            return Some(count_cell_map_to_rows(&by_key, map));
        }
        let mut map: FxHashMap<Cell, i64> = FxHashMap::default();
        let mut flt = PredSlots::new(pred);
        let mut key_slot = FieldSlot::new(&by_key);
        for r in col {
            if flt.eval(r, now) {
                let key_ref = key_slot.get(r).unwrap_or(&NULL_CELL);
                if let Some(cnt) = map.get_mut(key_ref) {
                    *cnt += 1;
                } else {
                    map.insert(key_ref.clone_for_borrow(), 1);
                }
            }
        }
        Some(count_cell_map_to_rows(&by_key, map))
    }

    pub(crate) fn try_summarize_fast(
        &self,
        q: &Query,
        bindings: &BTreeMap<String, Vec<Row>>,
        now: i64,
    ) -> Result<Option<Vec<Row>>, Error> {
        let Source::Collection(name) = &q.source else {
            return Ok(None);
        };
        if bindings.get(name).is_some() {
            return Ok(None);
        }
        let mut q = crate::catalog::with_catalog_filter(q, &self.catalog);
        if q.needs_simplify() {
            q.to_mut().simplify_predicates();
        }
        let q = &*q;
        let Source::Collection(name) = &q.source else {
            return Ok(None);
        };

        let mut filter: Option<&Pred> = None;
        let mut summarize: Option<(&[AggSpec], &[GroupKey])> = None;
        let mut tail: Vec<AggTail> = Vec::new();
        for step in &q.steps {
            match step {
                Step::Filter(p) if filter.is_none() && summarize.is_none() => filter = Some(p),
                Step::Summarize { specs, by } if summarize.is_none() => {
                    summarize = Some((specs.as_slice(), by.as_slice()));
                }
                Step::Take { n: None } if summarize.is_some() => {}
                Step::Take { n: Some(n) } if summarize.is_some() => {
                    tail.push(AggTail::Take((*n).max(0) as usize));
                }
                Step::Skip { n } if summarize.is_some() => {
                    tail.push(AggTail::Skip((*n).max(0) as usize));
                }
                Step::Sort { field, desc } if summarize.is_some() => {
                    tail.push(AggTail::Sort {
                        field: field.clone(),
                        desc: *desc,
                    });
                }
                _ => return Ok(None),
            }
            if !tail.is_empty()
                && !matches!(
                    step,
                    Step::Take { .. } | Step::Skip { .. } | Step::Sort { .. }
                )
            {
                return Ok(None);
            }
        }
        let Some((specs, by)) = summarize else {
            return Ok(None);
        };
        if filter.is_some_and(pred_has_param) {
            return Ok(None);
        }

        let sum_f64_flags = sum_f64_flags_for(self.catalog.collections.get(name), specs);
        let max_bytes = self.quotas.max_aggregate_bytes;
        let mut rows = if self.store.is_disk_backed(name)
            && let Some(eng) = self.store.disk_engine_read()
        {
            agg_summarize_disk(&eng, name, filter, now, specs, by, &sum_f64_flags, max_bytes)?
        } else {
            let col = self.store.collection(name);
            if let Some(pred) = filter {
                if let Some((uses, covered)) =
                    crate::index::pick_index_with_coverage(&self.catalog, name, pred)
                    && let Some(idxs) = self.store.index_seek(name, &uses, now)
                {
                    let residual = if covered { None } else { Some(pred) };
                    agg_summarize_indexed(
                        col,
                        &idxs,
                        residual,
                        now,
                        specs,
                        by,
                        &sum_f64_flags,
                        max_bytes,
                    )?
                } else if let Some(idxs) = self.mirror_select(name, pred, now) {
                    agg_summarize_indexed(
                        col,
                        &idxs,
                        None,
                        now,
                        specs,
                        by,
                        &sum_f64_flags,
                        max_bytes,
                    )?
                } else {
                    agg_summarize(col, filter, now, specs, by, &sum_f64_flags, max_bytes)?
                }
            } else {
                agg_summarize(col, filter, now, specs, by, &sum_f64_flags, max_bytes)?
            }
        };

        for step in &tail {
            match step {
                AggTail::Skip(n) => {
                    rows.drain(..(*n).min(rows.len()));
                }
                AggTail::Take(n) => rows.truncate(*n),
                AggTail::Sort { field, desc } => {
                    super::eval::sort_rows_top_k(&mut rows, field, *desc, None);
                }
            }
        }
        Ok(Some(rows))
    }
}

type GroupTuple = SmallVec<[Cell; 2]>;

const DCOUNT_EXACT_LIMIT: usize = 512;
const HLL_P: usize = 12;
const HLL_M: usize = 1 << HLL_P;
const HLL_BYTES: usize = HLL_M;

const PERCENTILE_EXACT_LIMIT: usize = 2048;
const TDIGEST_DELTA: f64 = 100.0;
const TDIGEST_BUF_CAP: usize = 512;
const TDIGEST_EST_BYTES: usize = 8192;

pub(crate) fn sum_f64_flags_for(
    col: Option<&crate::catalog::Collection>,
    specs: &[AggSpec],
) -> Vec<bool> {
    specs
        .iter()
        .map(|spec| match &spec.func {
            AggFunc::Sum(f) => col
                .and_then(|c| c.field(&f.as_str()))
                .is_some_and(|fi| fi.ty == Type::F64),
            _ => false,
        })
        .collect()
}

#[inline]
fn normalize_key_cell(cell: Cell) -> Cell {
    match cell {
        Cell::Float(f) if f == 0.0 => Cell::Float(0.0),
        other => other,
    }
}

fn bin_cell(cell: &Cell, step: BinStep) -> Cell {
    if matches!(cell, Cell::Null) {
        return Cell::Null;
    }
    match step {
        BinStep::Duration(step_ms) => match cell {
            Cell::Time(t) if step_ms > 0 => Cell::Time(t.div_euclid(step_ms).saturating_mul(step_ms)),
            _ => Cell::Null,
        },
        BinStep::Int(s) => match cell {
            Cell::Int(v) if s > 0 => Cell::Int(v.div_euclid(s).saturating_mul(s)),
            Cell::Float(v) if s > 0 && v.is_finite() => {
                let sf = s as f64;
                let mut b = (v / sf).floor() * sf;
                if b == 0.0 {
                    b = 0.0;
                }
                Cell::Float(b)
            }
            _ => Cell::Null,
        },
        BinStep::Float(s) => match cell {
            Cell::Float(v) if s > 0.0 && s.is_finite() && v.is_finite() => {
                let mut b = (v / s).floor() * s;
                if b == 0.0 {
                    b = 0.0;
                }
                Cell::Float(b)
            }
            Cell::Int(v) if s > 0.0 && s.is_finite() => {
                let mut b = ((*v as f64) / s).floor() * s;
                if b == 0.0 {
                    b = 0.0;
                }
                Cell::Float(b)
            }
            _ => Cell::Null,
        },
    }
}

#[inline]
fn eval_group_cell(expr: &GroupExpr, raw: &Cell) -> Cell {
    match expr {
        GroupExpr::Field(_) => normalize_key_cell(raw.clone_for_borrow()),
        GroupExpr::Bin { step, .. } => bin_cell(raw, *step),
    }
}

fn canonical_cell_hash(cell: &Cell) -> Option<u64> {
    use std::hash::{Hash, Hasher};
    if matches!(cell, Cell::Null) {
        return None;
    }
    let mut h = rustc_hash::FxHasher::default();
    match cell {
        Cell::Null => return None,
        Cell::Text(s) => {
            1u8.hash(&mut h);
            s.as_bytes().hash(&mut h);
        }
        Cell::SmallText(s) => {
            1u8.hash(&mut h);
            s.as_str().as_bytes().hash(&mut h);
        }
        Cell::Int(n) => {
            2u8.hash(&mut h);
            n.hash(&mut h);
        }
        Cell::Float(f) => {
            3u8.hash(&mut h);
            let bits = if *f == 0.0 {
                0u64
            } else if f.is_nan() {
                f64::NAN.to_bits()
            } else {
                f.to_bits()
            };
            bits.hash(&mut h);
        }
        Cell::Bool(b) => {
            4u8.hash(&mut h);
            b.hash(&mut h);
        }
        Cell::Time(t) => {
            5u8.hash(&mut h);
            t.hash(&mut h);
        }
        Cell::Vec(v) => {
            6u8.hash(&mut h);
            v.len().hash(&mut h);
            for &x in v.iter() {
                let bits = if x == 0.0 { 0u32 } else { x.to_bits() };
                bits.hash(&mut h);
            }
        }
    }
    let mut z = h.finish().wrapping_add(0x9e37_79b9_7f4a_7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    Some(z ^ (z >> 31))
}

#[inline]
fn hll_add(regs: &mut [u8; HLL_M], hash: u64) {
    let idx = (hash >> (64 - HLL_P)) as usize;
    let w = (hash << HLL_P) | (1u64 << (HLL_P - 1));
    let rho = (w.leading_zeros() as u8) + 1;
    if rho > regs[idx] {
        regs[idx] = rho;
    }
}

#[derive(Clone)]
pub(crate) enum DCountAcc {
    Exact(FxHashSet<u64>),
    Hll(Box<[u8; HLL_M]>),
}

impl DCountAcc {
    fn new() -> Self {
        Self::Exact(FxHashSet::default())
    }

    fn push(
        &mut self,
        cell: &Cell,
        state_bytes: &mut usize,
        max_bytes: usize,
    ) -> Result<(), Error> {
        let Some(hash) = canonical_cell_hash(cell) else {
            return Ok(());
        };
        match self {
            DCountAcc::Exact(set) => {
                if set.contains(&hash) {
                    return Ok(());
                }
                if set.len() < DCOUNT_EXACT_LIMIT {
                    set.insert(hash);
                    *state_bytes = state_bytes.saturating_add(16);
                    check_agg_quota(*state_bytes, max_bytes)?;
                    return Ok(());
                }
                let old_bytes = set.len().saturating_mul(16);
                let mut regs = Box::new([0u8; HLL_M]);
                for &existing in set.iter() {
                    hll_add(&mut regs, existing);
                }
                hll_add(&mut regs, hash);
                if HLL_BYTES > old_bytes {
                    *state_bytes = state_bytes.saturating_add(HLL_BYTES - old_bytes);
                    check_agg_quota(*state_bytes, max_bytes)?;
                }
                *self = DCountAcc::Hll(regs);
                Ok(())
            }
            DCountAcc::Hll(regs) => {
                hll_add(regs, hash);
                Ok(())
            }
        }
    }

    fn finish_ref(&self) -> i64 {
        match self {
            DCountAcc::Exact(set) => set.len() as i64,
            DCountAcc::Hll(regs) => {
                let m = HLL_M as f64;
                let alpha = 0.7213 / (1.0 + 1.079 / m);
                let mut sum_inv = 0.0f64;
                let mut zeros = 0usize;
                for &r in regs.iter() {
                    if r == 0 {
                        zeros += 1;
                    }
                    sum_inv += f64::from_bits((1023u64.saturating_sub(u64::from(r))) << 52);
                }
                let raw = alpha * m * m / sum_inv;
                let est = if raw <= 2.5 * m && zeros > 0 {
                    m * (m / (zeros as f64)).ln()
                } else {
                    raw
                };
                est.round().max(0.0) as i64
            }
        }
    }

    fn finish(self) -> i64 {
        self.finish_ref()
    }

    fn merge(&mut self, other: DCountAcc, state_bytes: &mut usize, max_bytes: usize) -> Result<(), Error> {
        match (self, other) {
            (this @ DCountAcc::Exact(_), DCountAcc::Exact(b)) => {
                let mut iter = b.into_iter();
                while let Some(hash) = iter.next() {
                    let DCountAcc::Exact(a) = this else { break };
                    if a.contains(&hash) {
                        continue;
                    }
                    if a.len() < DCOUNT_EXACT_LIMIT {
                        a.insert(hash);
                        *state_bytes = state_bytes.saturating_add(16);
                        check_agg_quota(*state_bytes, max_bytes)?;
                    } else {
                        let old_bytes = a.len().saturating_mul(16);
                        let mut regs = Box::new([0u8; HLL_M]);
                        for &existing in a.iter() {
                            hll_add(&mut regs, existing);
                        }
                        hll_add(&mut regs, hash);
                        for rest_h in iter {
                            hll_add(&mut regs, rest_h);
                        }
                        if HLL_BYTES > old_bytes {
                            *state_bytes = state_bytes.saturating_add(HLL_BYTES - old_bytes);
                            check_agg_quota(*state_bytes, max_bytes)?;
                        }
                        *this = DCountAcc::Hll(regs);
                        return Ok(());
                    }
                }
                Ok(())
            }
            (DCountAcc::Hll(regs), DCountAcc::Exact(b)) => {
                for hash in b {
                    hll_add(regs, hash);
                }
                Ok(())
            }
            (this @ DCountAcc::Exact(_), DCountAcc::Hll(mut regs)) => {
                let old_bytes = if let DCountAcc::Exact(a) = &*this {
                    for &hash in a.iter() {
                        hll_add(&mut regs, hash);
                    }
                    a.len().saturating_mul(16)
                } else {
                    0
                };
                if HLL_BYTES > old_bytes {
                    *state_bytes = state_bytes.saturating_add(HLL_BYTES - old_bytes);
                    check_agg_quota(*state_bytes, max_bytes)?;
                }
                *this = DCountAcc::Hll(regs);
                Ok(())
            }
            (DCountAcc::Hll(a), DCountAcc::Hll(b)) => {
                for (ra, rb) in a.iter_mut().zip(b.iter()) {
                    if *rb > *ra {
                        *ra = *rb;
                    }
                }
                Ok(())
            }
        }
    }
}

#[derive(Clone)]
pub(crate) struct TDigest {
    centroids: Vec<(f64, f64)>,
    buffer: Vec<f64>,
    min: f64,
    max: f64,
    total: f64,
}

impl TDigest {
    fn from_exact(vals: Vec<f64>) -> Self {
        let mut td = Self {
            centroids: Vec::with_capacity(128),
            buffer: Vec::with_capacity(TDIGEST_BUF_CAP),
            min: f64::INFINITY,
            max: f64::NEG_INFINITY,
            total: 0.0,
        };
        for v in vals {
            td.min = td.min.min(v);
            td.max = td.max.max(v);
            td.total += 1.0;
            td.centroids.push((v, 1.0));
        }
        td.compress();
        td
    }

    fn push(&mut self, v: f64) {
        self.min = self.min.min(v);
        self.max = self.max.max(v);
        self.total += 1.0;
        self.buffer.push(v);
        if self.buffer.len() >= TDIGEST_BUF_CAP {
            self.compress();
        }
    }

    fn compress(&mut self) {
        if self.centroids.is_empty() && self.buffer.is_empty() {
            return;
        }
        for v in self.buffer.drain(..) {
            self.centroids.push((v, 1.0));
        }
        self.centroids.sort_by(|a, b| a.0.total_cmp(&b.0));
        let total = self.total;
        if total <= 0.0 || self.centroids.len() <= 1 {
            return;
        }
        let k_of = |q: f64| -> f64 {
            (TDIGEST_DELTA / (2.0 * std::f64::consts::PI))
                * (2.0 * q.clamp(0.0, 1.0) - 1.0).asin()
        };
        let (mut cur_m, mut cur_w) = self.centroids[0];
        let mut weight_so_far = 0.0f64;
        let mut k_limit = k_of(0.0) + 1.0;
        let mut write_idx = 0usize;
        for i in 1..self.centroids.len() {
            let (m, w) = self.centroids[i];
            let next_q = (weight_so_far + cur_w + w) / total;
            if k_of(next_q) <= k_limit {
                let new_w = cur_w + w;
                cur_m = (cur_m * cur_w + m * w) / new_w;
                cur_w = new_w;
            } else {
                self.centroids[write_idx] = (cur_m, cur_w);
                write_idx += 1;
                weight_so_far += cur_w;
                k_limit = k_of(weight_so_far / total) + 1.0;
                cur_m = m;
                cur_w = w;
            }
        }
        self.centroids[write_idx] = (cur_m, cur_w);
        self.centroids.truncate(write_idx + 1);
    }

    fn quantile(&mut self, pct: f64) -> Option<f64> {
        if !self.buffer.is_empty() {
            self.compress();
        }
        if self.centroids.is_empty() {
            return None;
        }
        if pct <= 0.0 {
            return Some(self.min);
        }
        if pct >= 100.0 {
            return Some(self.max);
        }
        if self.centroids.len() == 1 {
            return Some(self.centroids[0].0);
        }
        let target = (pct / 100.0) * self.total;
        let first = self.centroids[0];
        if target < first.1 * 0.5 {
            if first.1 <= 1.0 {
                return Some(first.0);
            }
            let frac = target / (first.1 * 0.5);
            return Some(self.min + (first.0 - self.min) * frac);
        }
        let mut prev_cum = 0.0f64;
        for i in 0..(self.centroids.len() - 1) {
            let (m0, w0) = self.centroids[i];
            let (m1, w1) = self.centroids[i + 1];
            let c0 = prev_cum + w0 * 0.5;
            let c1 = prev_cum + w0 + w1 * 0.5;
            if target <= c1 {
                let denom = c1 - c0;
                if denom <= 0.0 {
                    return Some(m0);
                }
                let frac = (target - c0) / denom;
                return Some(m0 + (m1 - m0) * frac);
            }
            prev_cum += w0;
        }
        let &(last_m, last_w) = self.centroids.last().unwrap();
        let last_c = self.total - last_w * 0.5;
        let tail_span = self.total - last_c;
        if last_w <= 1.0 || tail_span <= 0.0 {
            return Some(self.max);
        }
        let frac = ((target - last_c) / tail_span).clamp(0.0, 1.0);
        Some(last_m + (self.max - last_m) * frac)
    }

    fn merge(&mut self, mut other: TDigest) {
        if !other.buffer.is_empty() {
            other.compress();
        }
        if other.centroids.is_empty() {
            return;
        }
        if !self.buffer.is_empty() {
            self.compress();
        }
        if self.centroids.is_empty() {
            *self = other;
            return;
        }
        self.min = self.min.min(other.min);
        self.max = self.max.max(other.max);
        self.total += other.total;
        self.centroids.extend(other.centroids);
        self.compress();
    }
}

#[derive(Clone)]
pub(crate) enum PercentileAcc {
    Exact(Vec<f64>),
    Digest(TDigest),
}

impl PercentileAcc {
    fn new() -> Self {
        Self::Exact(Vec::new())
    }

    fn push(
        &mut self,
        cell: &Cell,
        state_bytes: &mut usize,
        max_bytes: usize,
    ) -> Result<(), Error> {
        let Some(v) = cell.as_f64() else {
            return Ok(());
        };
        if v.is_nan() {
            return Ok(());
        }
        let v = if v == 0.0 { 0.0 } else { v };
        match self {
            PercentileAcc::Exact(vals) => {
                if vals.len() < PERCENTILE_EXACT_LIMIT {
                    vals.push(v);
                    *state_bytes = state_bytes.saturating_add(8);
                    check_agg_quota(*state_bytes, max_bytes)?;
                    return Ok(());
                }
                let old_bytes = vals.len().saturating_mul(8);
                let mut taken = std::mem::take(vals);
                taken.push(v);
                let td = TDigest::from_exact(taken);
                if TDIGEST_EST_BYTES > old_bytes {
                    *state_bytes = state_bytes.saturating_add(TDIGEST_EST_BYTES - old_bytes);
                    check_agg_quota(*state_bytes, max_bytes)?;
                }
                *self = PercentileAcc::Digest(td);
                Ok(())
            }
            PercentileAcc::Digest(td) => {
                td.push(v);
                Ok(())
            }
        }
    }

    fn finish_ref(&mut self, pct: f64) -> Cell {
        let val = match self {
            PercentileAcc::Exact(vals) => {
                let n = vals.len();
                if n == 0 {
                    None
                } else if n == 1 {
                    Some(vals[0])
                } else if pct <= 0.0 {
                    vals.iter().copied().min_by(f64::total_cmp)
                } else if pct >= 100.0 {
                    vals.iter().copied().max_by(f64::total_cmp)
                } else {
                    let pos = (pct / 100.0) * ((n - 1) as f64);
                    let lo = pos.floor() as usize;
                    let hi = pos.ceil() as usize;
                    let (_, &mut lo_val, upper) = vals.select_nth_unstable_by(lo, f64::total_cmp);
                    if lo == hi {
                        Some(lo_val)
                    } else {
                        let hi_val = upper
                            .iter()
                            .copied()
                            .min_by(f64::total_cmp)
                            .unwrap_or(lo_val);
                        let w = pos - (lo as f64);
                        Some(lo_val + (hi_val - lo_val) * w)
                    }
                }
            }
            PercentileAcc::Digest(td) => td.quantile(pct),
        };
        match val {
            Some(v) => Cell::Float(if v == 0.0 { 0.0 } else { v }),
            None => Cell::Null,
        }
    }

    fn finish(mut self, pct: f64) -> Cell {
        self.finish_ref(pct)
    }

    fn merge(
        &mut self,
        other: PercentileAcc,
        state_bytes: &mut usize,
        max_bytes: usize,
    ) -> Result<(), Error> {
        match (self, other) {
            (this @ PercentileAcc::Exact(_), PercentileAcc::Exact(b)) => {
                let PercentileAcc::Exact(a) = this else {
                    return Ok(());
                };
                if a.len() + b.len() <= PERCENTILE_EXACT_LIMIT {
                    *state_bytes = state_bytes.saturating_add(b.len().saturating_mul(8));
                    check_agg_quota(*state_bytes, max_bytes)?;
                    a.extend(b);
                } else {
                    let old_bytes = a.len().saturating_mul(8);
                    let mut all = std::mem::take(a);
                    all.extend(b);
                    let td = TDigest::from_exact(all);
                    if TDIGEST_EST_BYTES > old_bytes {
                        *state_bytes = state_bytes.saturating_add(TDIGEST_EST_BYTES - old_bytes);
                        check_agg_quota(*state_bytes, max_bytes)?;
                    }
                    *this = PercentileAcc::Digest(td);
                }
                Ok(())
            }
            (PercentileAcc::Digest(td), PercentileAcc::Exact(b)) => {
                for v in b {
                    td.push(v);
                }
                Ok(())
            }
            (this @ PercentileAcc::Exact(_), PercentileAcc::Digest(mut td)) => {
                let PercentileAcc::Exact(a) = this else {
                    return Ok(());
                };
                let old_bytes = a.len().saturating_mul(8);
                for v in a.drain(..) {
                    td.push(v);
                }
                if TDIGEST_EST_BYTES > old_bytes {
                    *state_bytes = state_bytes.saturating_add(TDIGEST_EST_BYTES - old_bytes);
                    check_agg_quota(*state_bytes, max_bytes)?;
                }
                *this = PercentileAcc::Digest(td);
                Ok(())
            }
            (PercentileAcc::Digest(a), PercentileAcc::Digest(b)) => {
                a.merge(b);
                Ok(())
            }
        }
    }
}

type AccVec = SmallVec<[MetricAcc; 2]>;

#[derive(Clone)]
pub(crate) enum MetricAcc {
    Count(i64),
    Sum {
        exact: crate::exact_sum::ExactSum,
        saw_float: bool,
        prefer_f64: bool,
    },
    Avg {
        sum: crate::exact_sum::ExactSum,
        count: u64,
    },
    Min(Option<Cell>),
    Max(Option<Cell>),
    DCount(DCountAcc),
    Percentile {
        pct: f64,
        acc: PercentileAcc,
    },
    ArgExtremumScalar {
        is_max: bool,
        best: Option<(Cell, Cell)>,
    },
}

impl MetricAcc {
    fn from_spec(spec: &AggSpec, prefer_f64: bool) -> Self {
        match &spec.func {
            AggFunc::Count => Self::Count(0),
            AggFunc::Sum(_) => Self::Sum {
                exact: crate::exact_sum::ExactSum::default(),
                saw_float: false,
                prefer_f64,
            },
            AggFunc::Avg(_) => Self::Avg {
                sum: crate::exact_sum::ExactSum::default(),
                count: 0,
            },
            AggFunc::Min(_) => Self::Min(None),
            AggFunc::Max(_) => Self::Max(None),
            AggFunc::DCount(_) => Self::DCount(DCountAcc::new()),
            AggFunc::Percentile { pct, .. } => Self::Percentile {
                pct: *pct,
                acc: PercentileAcc::new(),
            },
            AggFunc::ArgMax { .. } => Self::ArgExtremumScalar {
                is_max: true,
                best: None,
            },
            AggFunc::ArgMin { .. } => Self::ArgExtremumScalar {
                is_max: false,
                best: None,
            },
        }
    }

    fn push(
        &mut self,
        c1: &Cell,
        c2: Option<&Cell>,
        state_bytes: &mut usize,
        max_bytes: usize,
    ) -> Result<(), Error> {
        match self {
            MetricAcc::Count(n) => {
                *n += 1;
            }
            MetricAcc::Sum {
                exact, saw_float, ..
            } => match c1 {
                Cell::Null => {}
                Cell::Int(i) => {
                    exact.add_i64(*i);
                }
                other => {
                    if let Some(f) = other.as_f64() {
                        *saw_float = true;
                        exact.add_f64(f);
                    }
                }
            },
            MetricAcc::Avg { sum, count } => match c1 {
                Cell::Null => {}
                other => {
                    if sum.add_numeric(Some(other)) {
                        *count += 1;
                    }
                }
            },
            MetricAcc::Min(best) => {
                if !matches!(c1, Cell::Null)
                    && best.as_ref().is_none_or(|prev| cmp_sort(c1, prev).is_lt())
                {
                    *best = Some(c1.clone_for_borrow());
                }
            }
            MetricAcc::Max(best) => {
                if !matches!(c1, Cell::Null)
                    && best.as_ref().is_none_or(|prev| cmp_sort(c1, prev).is_gt())
                {
                    *best = Some(c1.clone_for_borrow());
                }
            }
            MetricAcc::DCount(acc) => acc.push(c1, state_bytes, max_bytes)?,
            MetricAcc::Percentile { acc, .. } => acc.push(c1, state_bytes, max_bytes)?,
            MetricAcc::ArgExtremumScalar { is_max, best } => {
                if matches!(c1, Cell::Null) {
                    return Ok(());
                }
                let take = match best {
                    None => true,
                    Some((prev_ord, _)) => {
                        if *is_max {
                            cmp_sort(c1, prev_ord).is_gt()
                        } else {
                            cmp_sort(c1, prev_ord).is_lt()
                        }
                    }
                };
                if take {
                    let val = c2.map(Cell::clone_for_borrow).unwrap_or(Cell::Null);
                    *best = Some((c1.clone_for_borrow(), val));
                }
            }
        }
        Ok(())
    }

    fn finish_ref(&mut self) -> Cell {
        match self {
            MetricAcc::Count(n) => Cell::Int(*n),
            MetricAcc::Sum {
                exact,
                saw_float,
                prefer_f64,
            } => {
                let int_sum = exact.int_part();
                if *saw_float || int_sum < i128::from(i64::MIN) || int_sum > i128::from(i64::MAX) {
                    Cell::Float(exact.value())
                } else if int_sum == 0 && *prefer_f64 {
                    Cell::Float(0.0)
                } else {
                    Cell::Int(int_sum as i64)
                }
            }
            MetricAcc::Avg { sum, count } => {
                if *count == 0 {
                    Cell::Float(0.0)
                } else {
                    Cell::Float(sum.value() / (*count as f64))
                }
            }
            MetricAcc::Min(v) | MetricAcc::Max(v) => {
                v.as_ref().map(Cell::clone_for_borrow).unwrap_or(Cell::Null)
            }
            MetricAcc::DCount(acc) => Cell::Int(acc.finish_ref()),
            MetricAcc::Percentile { pct, acc } => acc.finish_ref(*pct),
            MetricAcc::ArgExtremumScalar { best, .. } => best
                .as_ref()
                .map(|(_, v)| v.clone_for_borrow())
                .unwrap_or(Cell::Null),
        }
    }

    fn finish(self) -> Cell {
        match self {
            MetricAcc::Count(n) => Cell::Int(n),
            MetricAcc::Sum {
                exact,
                saw_float,
                prefer_f64,
            } => {
                let int_sum = exact.int_part();
                if saw_float || int_sum < i128::from(i64::MIN) || int_sum > i128::from(i64::MAX) {
                    Cell::Float(exact.value())
                } else if int_sum == 0 && prefer_f64 {
                    Cell::Float(0.0)
                } else {
                    Cell::Int(int_sum as i64)
                }
            }
            MetricAcc::Avg { sum, count } => {
                if count == 0 {
                    Cell::Float(0.0)
                } else {
                    Cell::Float(sum.value() / (count as f64))
                }
            }
            MetricAcc::Min(v) | MetricAcc::Max(v) => v.unwrap_or(Cell::Null),
            MetricAcc::DCount(acc) => Cell::Int(acc.finish()),
            MetricAcc::Percentile { pct, acc } => acc.finish(pct),
            MetricAcc::ArgExtremumScalar { best, .. } => best.map(|(_, v)| v).unwrap_or(Cell::Null),
        }
    }

    fn merge(
        &mut self,
        other: MetricAcc,
        state_bytes: &mut usize,
        max_bytes: usize,
    ) -> Result<(), Error> {
        match (self, other) {
            (MetricAcc::Count(a), MetricAcc::Count(b)) => {
                *a = a.saturating_add(b);
                Ok(())
            }
            (
                MetricAcc::Sum {
                    exact,
                    saw_float,
                    prefer_f64,
                },
                MetricAcc::Sum {
                    exact: o_exact,
                    saw_float: o_saw,
                    prefer_f64: o_pref,
                },
            ) => {
                exact.merge(o_exact);
                *saw_float |= o_saw;
                *prefer_f64 |= o_pref;
                Ok(())
            }
            (MetricAcc::Avg { sum, count }, MetricAcc::Avg { sum: o_sum, count: o_cnt }) => {
                sum.merge(o_sum);
                *count = count.saturating_add(o_cnt);
                Ok(())
            }
            (MetricAcc::Min(best), MetricAcc::Min(other_best)) => {
                if let Some(c) = other_best
                    && best.as_ref().is_none_or(|prev| cmp_sort(&c, prev).is_lt())
                {
                    *best = Some(c);
                }
                Ok(())
            }
            (MetricAcc::Max(best), MetricAcc::Max(other_best)) => {
                if let Some(c) = other_best
                    && best.as_ref().is_none_or(|prev| cmp_sort(&c, prev).is_gt())
                {
                    *best = Some(c);
                }
                Ok(())
            }
            (MetricAcc::DCount(a), MetricAcc::DCount(b)) => a.merge(b, state_bytes, max_bytes),
            (
                MetricAcc::Percentile { acc, .. },
                MetricAcc::Percentile { acc: o_acc, .. },
            ) => acc.merge(o_acc, state_bytes, max_bytes),
            (
                MetricAcc::ArgExtremumScalar { is_max, best },
                MetricAcc::ArgExtremumScalar { best: o_best, .. },
            ) => {
                if let Some((cand_ord, cand_val)) = o_best {
                    let take = match best {
                        None => true,
                        Some((prev_ord, _)) => {
                            if *is_max {
                                cmp_sort(&cand_ord, prev_ord).is_gt()
                            } else {
                                cmp_sort(&cand_ord, prev_ord).is_lt()
                            }
                        }
                    };
                    if take {
                        *best = Some((cand_ord, cand_val));
                    }
                }
                Ok(())
            }
            _ => Err(Error::runtime(
                "mismatched partial MetricAcc variants in distributed summarize merge",
            )),
        }
    }
}

#[inline]
fn check_agg_quota(state_bytes: usize, max_bytes: usize) -> Result<(), Error> {
    if state_bytes > max_bytes {
        return Err(Error::runtime(format!(
            "aggregate state bytes {state_bytes} exceeds max_aggregate_bytes quota {max_bytes}"
        )));
    }
    Ok(())
}

#[inline]
fn group_key_bytes(keys: &[Cell], spec_count: usize) -> usize {
    let mut bytes = 64usize.saturating_add(spec_count.saturating_mul(64));
    for cell in keys {
        bytes = bytes.saturating_add(match cell {
            Cell::Text(s) => s.len() + 32,
            Cell::SmallText(s) => s.as_str().len() + 32,
            _ => 16,
        });
    }
    bytes
}

#[inline]
fn cmp_group_tuples(a: &[Cell], b: &[Cell]) -> std::cmp::Ordering {
    for (x, y) in a.iter().zip(b.iter()) {
        let ord = cmp_sort(x, y);
        if !ord.is_eq() {
            return ord;
        }
    }
    a.len().cmp(&b.len())
}

fn build_accs(specs: &[AggSpec], sum_f64_flags: &[bool]) -> AccVec {
    specs
        .iter()
        .enumerate()
        .map(|(i, s)| MetricAcc::from_spec(s, sum_f64_flags.get(i).copied().unwrap_or(false)))
        .collect()
}

struct SummarizeRowLayout {
    shape: Arc<crate::store::RowShape>,
    slots: SmallVec<[usize; 8]>,
}

impl SummarizeRowLayout {
    fn try_new(by: &[GroupKey], specs: &[AggSpec]) -> Option<Self> {
        let total = by.len() + specs.len();
        if total == 0 {
            return None;
        }
        let mut names: SmallVec<[&str; 8]> = SmallVec::with_capacity(total);
        for gk in by {
            names.push(gk.alias.as_str());
        }
        for sp in specs {
            names.push(sp.alias.as_str());
        }
        let shape = Row::shape_of(&names);
        if shape.names().len() != total {
            // Duplicate output alias: fall back to sequential `insert` so last-wins holds.
            return None;
        }
        let slots = names
            .iter()
            .map(|&nm| shape.names().iter().position(|s| s == nm).unwrap_or(0))
            .collect();
        Some(Self { shape, slots })
    }

    #[inline]
    fn build_owned(&self, keys: GroupTuple, accs: AccVec) -> Row {
        let total = self.slots.len();
        let mut cells: SmallVec<[Cell; 4]> = smallvec::smallvec![Cell::Null; total];
        let mut idx = 0usize;
        for kcell in keys {
            cells[self.slots[idx]] = kcell;
            idx += 1;
        }
        for acc in accs {
            cells[self.slots[idx]] = acc.finish();
            idx += 1;
        }
        Row::from_parts(Arc::clone(&self.shape), cells)
    }

    #[inline]
    fn build_ref(&self, keys: &[Cell], accs: &mut [MetricAcc]) -> Row {
        let total = self.slots.len();
        let mut cells: SmallVec<[Cell; 4]> = smallvec::smallvec![Cell::Null; total];
        let mut idx = 0usize;
        for kcell in keys {
            cells[self.slots[idx]] = kcell.clone_for_borrow();
            idx += 1;
        }
        for acc in accs.iter_mut() {
            cells[self.slots[idx]] = acc.finish_ref();
            idx += 1;
        }
        Row::from_parts(Arc::clone(&self.shape), cells)
    }
}

fn emit_summarize_row(by: &[GroupKey], keys: GroupTuple, specs: &[AggSpec], accs: AccVec) -> Row {
    let mut row = crate::store::acquire_row();
    for (gk, kcell) in by.iter().zip(keys.into_iter()) {
        row.insert(gk.alias.as_str(), kcell);
    }
    for (spec, acc) in specs.iter().zip(accs.into_iter()) {
        row.insert(spec.alias.as_str(), acc.finish());
    }
    row
}

fn emit_summarize_row_ref(
    by: &[GroupKey],
    keys: &[Cell],
    specs: &[AggSpec],
    accs: &mut [MetricAcc],
) -> Row {
    let mut row = crate::store::acquire_row();
    for (gk, kcell) in by.iter().zip(keys.iter()) {
        row.insert(gk.alias.as_str(), kcell.clone_for_borrow());
    }
    for (spec, acc) in specs.iter().zip(accs.iter_mut()) {
        row.insert(spec.alias.as_str(), acc.finish_ref());
    }
    row
}

enum FieldReader<'k> {
    Leaf(FieldSlot<'k>),
    Path(&'k Field),
}

impl<'k> FieldReader<'k> {
    fn new(field: &'k Field) -> Self {
        match field.leaf() {
            Some(lf) => Self::Leaf(FieldSlot::new(lf)),
            None => Self::Path(field),
        }
    }

    #[inline]
    fn get<'a>(&mut self, row: &'a Row) -> &'a Cell {
        static NULL: Cell = Cell::Null;
        match self {
            FieldReader::Leaf(slot) => slot.get(row).unwrap_or(&NULL),
            FieldReader::Path(f) => super::eval::field_cell(row, f),
        }
    }
}

enum SpecReader<'k> {
    None,
    One(FieldReader<'k>),
    Two(FieldReader<'k>, FieldReader<'k>),
}

impl<'k> SpecReader<'k> {
    fn from_spec(spec: &'k AggSpec) -> Self {
        match &spec.func {
            AggFunc::Count => Self::None,
            AggFunc::Sum(f)
            | AggFunc::Avg(f)
            | AggFunc::Min(f)
            | AggFunc::Max(f)
            | AggFunc::DCount(f)
            | AggFunc::Percentile { field: f, .. } => Self::One(FieldReader::new(f)),
            AggFunc::ArgMax { order, value } | AggFunc::ArgMin { order, value } => {
                Self::Two(FieldReader::new(order), FieldReader::new(value))
            }
        }
    }
}

#[derive(Clone)]
pub(crate) enum PartialSummarizeState {
    Single(AccVec),
    Grouped(FxHashMap<GroupTuple, AccVec>),
}

pub(crate) fn merge_partial_states(
    dst: &mut PartialSummarizeState,
    src: PartialSummarizeState,
    specs_len: usize,
    max_bytes: usize,
) -> Result<(), Error> {
    let mut state_bytes = 0usize;
    match (dst, src) {
        (PartialSummarizeState::Single(da), PartialSummarizeState::Single(sa)) => {
            for (d, s) in da.iter_mut().zip(sa.into_iter()) {
                d.merge(s, &mut state_bytes, max_bytes)?;
            }
            Ok(())
        }
        (PartialSummarizeState::Grouped(dg), PartialSummarizeState::Grouped(sg)) => {
            state_bytes = dg.len().saturating_mul(64 + specs_len * 32);
            for (gkey, accs) in sg {
                match dg.entry(gkey) {
                    std::collections::hash_map::Entry::Vacant(e) => {
                        state_bytes =
                            state_bytes.saturating_add(group_key_bytes(e.key(), specs_len));
                        check_agg_quota(state_bytes, max_bytes)?;
                        e.insert(accs);
                    }
                    std::collections::hash_map::Entry::Occupied(mut e) => {
                        for (d, s) in e.get_mut().iter_mut().zip(accs.into_iter()) {
                            d.merge(s, &mut state_bytes, max_bytes)?;
                        }
                    }
                }
            }
            Ok(())
        }
        _ => Err(Error::runtime("partial summarize state shape mismatch")),
    }
}

pub(crate) fn finish_partial_state(
    state: PartialSummarizeState,
    by: &[GroupKey],
    specs: &[AggSpec],
) -> Vec<Row> {
    let layout = SummarizeRowLayout::try_new(by, specs);
    match state {
        PartialSummarizeState::Single(accs) => vec![match &layout {
            Some(lay) => lay.build_owned(GroupTuple::new(), accs),
            None => emit_summarize_row(by, GroupTuple::new(), specs, accs),
        }],
        PartialSummarizeState::Grouped(groups) => {
            let mut entries: Vec<(GroupTuple, AccVec)> = groups.into_iter().collect();
            entries.sort_unstable_by(|(a, _), (b, _)| cmp_group_tuples(a, b));
            match &layout {
                Some(lay) => entries
                    .into_iter()
                    .map(|(k, accs)| lay.build_owned(k, accs))
                    .collect(),
                None => entries
                    .into_iter()
                    .map(|(k, accs)| emit_summarize_row(by, k, specs, accs))
                    .collect(),
            }
        }
    }
}

pub(crate) fn finish_partial_state_ref(
    state: &mut PartialSummarizeState,
    by: &[GroupKey],
    specs: &[AggSpec],
) -> Vec<Row> {
    let layout = SummarizeRowLayout::try_new(by, specs);
    match state {
        PartialSummarizeState::Single(accs) => vec![match &layout {
            Some(lay) => lay.build_ref(&[], accs.as_mut_slice()),
            None => emit_summarize_row_ref(by, &[], specs, accs.as_mut_slice()),
        }],
        PartialSummarizeState::Grouped(groups) => {
            let mut entries: Vec<(&GroupTuple, &mut AccVec)> = groups.iter_mut().collect();
            entries.sort_unstable_by(|(a, _), (b, _)| cmp_group_tuples(a, b));
            match &layout {
                Some(lay) => entries
                    .into_iter()
                    .map(|(k, accs)| lay.build_ref(k.as_slice(), accs.as_mut_slice()))
                    .collect(),
                None => entries
                    .into_iter()
                    .map(|(k, accs)| emit_summarize_row_ref(by, k.as_slice(), specs, accs))
                    .collect(),
            }
        }
    }
}

#[inline]
fn push_specs_from_row(
    r: &Row,
    accs: &mut [MetricAcc],
    spec_readers: &mut [SpecReader<'_>],
    state_bytes: &mut usize,
    max_bytes: usize,
) -> Result<(), Error> {
    for (acc, sr) in accs.iter_mut().zip(spec_readers.iter_mut()) {
        let (c1, c2) = match sr {
            SpecReader::None => (&NULL_CELL, None),
            SpecReader::One(r1) => (r1.get(r), None),
            SpecReader::Two(r1, r2) => (r1.get(r), Some(r2.get(r))),
        };
        acc.push(c1, c2, state_bytes, max_bytes)?;
    }
    Ok(())
}

fn fold_summarize_rows_seq(
    rows: &[Row],
    filter: Option<&Pred>,
    now: i64,
    specs: &[AggSpec],
    by: &[GroupKey],
    sum_f64_flags: &[bool],
    max_bytes: usize,
) -> Result<PartialSummarizeState, Error> {
    let mut flt = filter.map(PredSlots::new);
    let mut spec_readers: Vec<SpecReader<'_>> = specs.iter().map(SpecReader::from_spec).collect();
    let mut state_bytes = 0usize;

    if by.is_empty() {
        let mut accs = build_accs(specs, sum_f64_flags);
        for r in rows {
            if !flt.as_mut().is_none_or(|f| f.eval(r, now)) {
                continue;
            }
            push_specs_from_row(r, &mut accs, &mut spec_readers, &mut state_bytes, max_bytes)?;
        }
        return Ok(PartialSummarizeState::Single(accs));
    }

    let mut groups: FxHashMap<GroupTuple, AccVec> = FxHashMap::default();
    if by.len() == 1 {
        let gk0 = &by[0];
        let mut br0 = FieldReader::new(gk0.source_field());
        let is_plain_field = matches!(gk0.expr, GroupExpr::Field(_));
        for r in rows {
            if !flt.as_mut().is_none_or(|f| f.eval(r, now)) {
                continue;
            }
            let raw = br0.get(r);
            let accs = if is_plain_field
                && !matches!(raw, Cell::Float(f) if *f == 0.0 && f.is_sign_negative())
            {
                if let Some(accs) = groups.get_mut(std::slice::from_ref(raw)) {
                    accs
                } else {
                    let gkey: GroupTuple = smallvec::smallvec![raw.clone_for_borrow()];
                    state_bytes = state_bytes.saturating_add(group_key_bytes(&gkey, specs.len()));
                    check_agg_quota(state_bytes, max_bytes)?;
                    groups.entry(gkey).or_insert_with(|| build_accs(specs, sum_f64_flags))
                }
            } else {
                let kcell = eval_group_cell(&gk0.expr, raw);
                if let Some(accs) = groups.get_mut(std::slice::from_ref(&kcell)) {
                    accs
                } else {
                    let gkey: GroupTuple = smallvec::smallvec![kcell];
                    state_bytes = state_bytes.saturating_add(group_key_bytes(&gkey, specs.len()));
                    check_agg_quota(state_bytes, max_bytes)?;
                    groups.entry(gkey).or_insert_with(|| build_accs(specs, sum_f64_flags))
                }
            };
            push_specs_from_row(r, accs, &mut spec_readers, &mut state_bytes, max_bytes)?;
        }
        return Ok(PartialSummarizeState::Grouped(groups));
    }

    let mut by_readers: Vec<FieldReader<'_>> = by
        .iter()
        .map(|gk| FieldReader::new(gk.source_field()))
        .collect();
    let mut gkey_buf: GroupTuple = SmallVec::with_capacity(by.len());
    for r in rows {
        if !flt.as_mut().is_none_or(|f| f.eval(r, now)) {
            continue;
        }
        gkey_buf.clear();
        for (gk, br) in by.iter().zip(by_readers.iter_mut()) {
            gkey_buf.push(eval_group_cell(&gk.expr, br.get(r)));
        }
        let accs = if let Some(accs) = groups.get_mut(gkey_buf.as_slice()) {
            accs
        } else {
            state_bytes = state_bytes.saturating_add(group_key_bytes(&gkey_buf, specs.len()));
            check_agg_quota(state_bytes, max_bytes)?;
            let owned_key = gkey_buf.clone();
            groups
                .entry(owned_key)
                .or_insert_with(|| build_accs(specs, sum_f64_flags))
        };
        push_specs_from_row(r, accs, &mut spec_readers, &mut state_bytes, max_bytes)?;
    }

    Ok(PartialSummarizeState::Grouped(groups))
}

pub(crate) fn fold_summarize_rows(
    rows: &[Row],
    filter: Option<&Pred>,
    now: i64,
    specs: &[AggSpec],
    by: &[GroupKey],
    sum_f64_flags: &[bool],
    max_bytes: usize,
) -> Result<PartialSummarizeState, Error> {
    #[cfg(feature = "parallel")]
    if rows.len() >= PAR_AGG_MIN_ROWS
        && !specs
            .iter()
            .any(|s| matches!(s.func, AggFunc::Percentile { .. }))
    {
        use rayon::prelude::*;
        let parts: Vec<Result<PartialSummarizeState, Error>> = rows
            .par_chunks(PAR_AGG_CHUNK_ROWS)
            .map(|chunk| {
                fold_summarize_rows_seq(chunk, filter, now, specs, by, sum_f64_flags, max_bytes)
            })
            .collect();
        let mut iter = parts.into_iter();
        if let Some(Ok(mut acc_state)) = iter.next() {
            let mut ok = true;
            for part_res in iter {
                match part_res {
                    Ok(part) => {
                        if merge_partial_states(&mut acc_state, part, specs.len(), max_bytes)
                            .is_err()
                        {
                            ok = false;
                            break;
                        }
                    }
                    Err(_) => {
                        ok = false;
                        break;
                    }
                }
            }
            if ok {
                return Ok(acc_state);
            }
        }
    }
    fold_summarize_rows_seq(rows, filter, now, specs, by, sum_f64_flags, max_bytes)
}

#[allow(clippy::too_many_arguments)]
fn agg_summarize_indexed(
    col: &[Row],
    idxs: &[usize],
    filter: Option<&Pred>,
    now: i64,
    specs: &[AggSpec],
    by: &[GroupKey],
    sum_f64_flags: &[bool],
    max_bytes: usize,
) -> Result<Vec<Row>, Error> {
    let mut flt = filter.map(PredSlots::new);
    let mut spec_readers: Vec<SpecReader<'_>> = specs.iter().map(SpecReader::from_spec).collect();
    let mut state_bytes = 0usize;

    let state = if by.is_empty() {
        let mut accs = build_accs(specs, sum_f64_flags);
        for &i in idxs {
            let Some(r) = col.get(i) else { continue };
            if !flt.as_mut().is_none_or(|f| f.eval(r, now)) {
                continue;
            }
            push_specs_from_row(r, &mut accs, &mut spec_readers, &mut state_bytes, max_bytes)?;
        }
        PartialSummarizeState::Single(accs)
    } else if by.len() == 1 {
        let gk0 = &by[0];
        let mut br0 = FieldReader::new(gk0.source_field());
        let is_plain_field = matches!(gk0.expr, GroupExpr::Field(_));
        let mut groups: FxHashMap<GroupTuple, AccVec> = FxHashMap::default();
        for &i in idxs {
            let Some(r) = col.get(i) else { continue };
            if !flt.as_mut().is_none_or(|f| f.eval(r, now)) {
                continue;
            }
            let raw = br0.get(r);
            let accs = if is_plain_field
                && !matches!(raw, Cell::Float(f) if *f == 0.0 && f.is_sign_negative())
            {
                if let Some(accs) = groups.get_mut(std::slice::from_ref(raw)) {
                    accs
                } else {
                    let gkey: GroupTuple = smallvec::smallvec![raw.clone_for_borrow()];
                    state_bytes = state_bytes.saturating_add(group_key_bytes(&gkey, specs.len()));
                    check_agg_quota(state_bytes, max_bytes)?;
                    groups.entry(gkey).or_insert_with(|| build_accs(specs, sum_f64_flags))
                }
            } else {
                let kcell = eval_group_cell(&gk0.expr, raw);
                if let Some(accs) = groups.get_mut(std::slice::from_ref(&kcell)) {
                    accs
                } else {
                    let gkey: GroupTuple = smallvec::smallvec![kcell];
                    state_bytes = state_bytes.saturating_add(group_key_bytes(&gkey, specs.len()));
                    check_agg_quota(state_bytes, max_bytes)?;
                    groups.entry(gkey).or_insert_with(|| build_accs(specs, sum_f64_flags))
                }
            };
            push_specs_from_row(r, accs, &mut spec_readers, &mut state_bytes, max_bytes)?;
        }
        PartialSummarizeState::Grouped(groups)
    } else {
        let mut by_readers: Vec<FieldReader<'_>> = by
            .iter()
            .map(|gk| FieldReader::new(gk.source_field()))
            .collect();
        let mut groups: FxHashMap<GroupTuple, AccVec> = FxHashMap::default();
        let mut gkey_buf: GroupTuple = SmallVec::with_capacity(by.len());
        for &i in idxs {
            let Some(r) = col.get(i) else { continue };
            if !flt.as_mut().is_none_or(|f| f.eval(r, now)) {
                continue;
            }
            gkey_buf.clear();
            for (gk, br) in by.iter().zip(by_readers.iter_mut()) {
                gkey_buf.push(eval_group_cell(&gk.expr, br.get(r)));
            }
            let accs = if let Some(accs) = groups.get_mut(gkey_buf.as_slice()) {
                accs
            } else {
                state_bytes = state_bytes.saturating_add(group_key_bytes(&gkey_buf, specs.len()));
                check_agg_quota(state_bytes, max_bytes)?;
                let owned_key = gkey_buf.clone();
                groups
                    .entry(owned_key)
                    .or_insert_with(|| build_accs(specs, sum_f64_flags))
            };
            push_specs_from_row(r, accs, &mut spec_readers, &mut state_bytes, max_bytes)?;
        }
        PartialSummarizeState::Grouped(groups)
    };
    Ok(finish_partial_state(state, by, specs))
}

pub(crate) fn agg_summarize(
    rows: &[Row],
    filter: Option<&Pred>,
    now: i64,
    specs: &[AggSpec],
    by: &[GroupKey],
    sum_f64_flags: &[bool],
    max_bytes: usize,
) -> Result<Vec<Row>, Error> {
    let state = fold_summarize_rows(rows, filter, now, specs, by, sum_f64_flags, max_bytes)?;
    Ok(finish_partial_state(state, by, specs))
}

#[allow(clippy::too_many_arguments)]
fn fold_summarize_disk(
    eng: &crate::disk::DiskEngine,
    table: &str,
    filter: Option<&Pred>,
    now: i64,
    specs: &[AggSpec],
    by: &[GroupKey],
    sum_f64_flags: &[bool],
    max_bytes: usize,
) -> Result<PartialSummarizeState, Error> {
    fn push_field<'a>(needed: &mut Vec<&'a str>, f: &'a Field) -> Option<usize> {
        let lf = f.leaf()?;
        if let Some(pos) = needed.iter().position(|&c| c == lf) {
            Some(pos)
        } else {
            let pos = needed.len();
            needed.push(lf);
            Some(pos)
        }
    }
    let mut needed: Vec<&str> = Vec::new();
    let mut by_pos = Vec::with_capacity(by.len());
    let mut all_leaf = true;
    for gk in by {
        if let Some(pos) = push_field(&mut needed, gk.source_field()) {
            by_pos.push(pos);
        } else {
            all_leaf = false;
            break;
        }
    }
    let mut spec_pos: Vec<Option<(usize, Option<usize>)>> = Vec::with_capacity(specs.len());
    if all_leaf {
        for spec in specs {
            match &spec.func {
                AggFunc::Count => spec_pos.push(None),
                AggFunc::Sum(f)
                | AggFunc::Avg(f)
                | AggFunc::Min(f)
                | AggFunc::Max(f)
                | AggFunc::DCount(f)
                | AggFunc::Percentile { field: f, .. } => {
                    let Some(p) = push_field(&mut needed, f) else {
                        all_leaf = false;
                        break;
                    };
                    spec_pos.push(Some((p, None)));
                }
                AggFunc::ArgMax { order, value } | AggFunc::ArgMin { order, value } => {
                    let (Some(p1), Some(p2)) =
                        (push_field(&mut needed, order), push_field(&mut needed, value))
                    else {
                        all_leaf = false;
                        break;
                    };
                    spec_pos.push(Some((p1, Some(p2))));
                }
            }
        }
    }

    if !all_leaf {
        let rows = eng.query_filter(table, filter, None, None, now)?;
        return fold_summarize_rows(&rows, None, now, specs, by, sum_f64_flags, max_bytes);
    }

    static NULL: Cell = Cell::Null;
    let mut state_bytes = 0usize;
    let mut single = if by.is_empty() {
        Some(build_accs(specs, sum_f64_flags))
    } else {
        None
    };
    let mut groups: FxHashMap<GroupTuple, AccVec> = FxHashMap::default();
    let mut gkey_buf: GroupTuple = SmallVec::with_capacity(by.len());
    let mut loop_err: Option<Error> = None;

    let scanned = eng.scan_cells(table, filter, &needed, now, |_, cells| {
        let accs: &mut [MetricAcc] = if let Some(ref mut s) = single {
            s.as_mut_slice()
        } else if by.len() == 1 {
            let gk0 = &by[0];
            let raw = cells.get(by_pos[0]).unwrap_or(&NULL);
            let kcell = eval_group_cell(&gk0.expr, raw);
            if groups.contains_key(std::slice::from_ref(&kcell)) {
                groups
                    .get_mut(std::slice::from_ref(&kcell))
                    .unwrap()
                    .as_mut_slice()
            } else {
                let gkey: GroupTuple = smallvec::smallvec![kcell];
                state_bytes = state_bytes.saturating_add(group_key_bytes(&gkey, specs.len()));
                if let Err(err) = check_agg_quota(state_bytes, max_bytes) {
                    loop_err = Some(err);
                    return false;
                }
                groups
                    .entry(gkey)
                    .or_insert_with(|| build_accs(specs, sum_f64_flags))
                    .as_mut_slice()
            }
        } else {
            gkey_buf.clear();
            for (gk, &idx) in by.iter().zip(by_pos.iter()) {
                let raw = cells.get(idx).unwrap_or(&NULL);
                gkey_buf.push(eval_group_cell(&gk.expr, raw));
            }
            if groups.contains_key(gkey_buf.as_slice()) {
                groups.get_mut(gkey_buf.as_slice()).unwrap().as_mut_slice()
            } else {
                state_bytes = state_bytes.saturating_add(group_key_bytes(&gkey_buf, specs.len()));
                if let Err(err) = check_agg_quota(state_bytes, max_bytes) {
                    loop_err = Some(err);
                    return false;
                }
                groups
                    .entry(gkey_buf.clone())
                    .or_insert_with(|| build_accs(specs, sum_f64_flags))
                    .as_mut_slice()
            }
        };
        for (acc, pos) in accs.iter_mut().zip(spec_pos.iter()) {
            let (c1, c2) = match pos {
                None => (&NULL, None),
                Some((p1, None)) => (cells.get(*p1).unwrap_or(&NULL), None),
                Some((p1, Some(p2))) => (
                    cells.get(*p1).unwrap_or(&NULL),
                    Some(cells.get(*p2).unwrap_or(&NULL)),
                ),
            };
            if let Err(err) = acc.push(c1, c2, &mut state_bytes, max_bytes) {
                loop_err = Some(err);
                return false;
            }
        }
        true
    })?;

    if let Some(err) = loop_err {
        return Err(err);
    }
    if !scanned {
        let rows = eng.query_filter(table, filter, None, Some(&needed), now)?;
        return fold_summarize_rows(&rows, None, now, specs, by, sum_f64_flags, max_bytes);
    }

    Ok(match single {
        Some(accs) => PartialSummarizeState::Single(accs),
        None => PartialSummarizeState::Grouped(groups),
    })
}

#[allow(clippy::too_many_arguments)]
fn agg_summarize_disk(
    eng: &crate::disk::DiskEngine,
    table: &str,
    filter: Option<&Pred>,
    now: i64,
    specs: &[AggSpec],
    by: &[GroupKey],
    sum_f64_flags: &[bool],
    max_bytes: usize,
) -> Result<Vec<Row>, Error> {
    let state =
        fold_summarize_disk(eng, table, filter, now, specs, by, sum_f64_flags, max_bytes)?;
    Ok(finish_partial_state(state, by, specs))
}

#[inline]
fn arg_extremum_better(is_max: bool, cand: &Cell, prev: &Cell) -> bool {
    match (cand, prev) {
        (Cell::Null, _) => false,
        (_, Cell::Null) => true,
        _ => {
            if is_max {
                cmp_sort(cand, prev).is_gt()
            } else {
                cmp_sort(cand, prev).is_lt()
            }
        }
    }
}

pub(crate) fn agg_arg_extremum(
    rows: &[Row],
    filter: Option<&Pred>,
    now: i64,
    is_max: bool,
    order: &Field,
    by: &[GroupKey],
    max_bytes: usize,
) -> Result<Vec<Row>, Error> {
    let mut flt = filter.map(PredSlots::new);
    let mut order_reader = FieldReader::new(order);

    if by.is_empty() {
        let mut best: Option<(&Cell, &Row)> = None;
        for r in rows {
            if !flt.as_mut().is_none_or(|f| f.eval(r, now)) {
                continue;
            }
            let ord = order_reader.get(r);
            if best.is_none_or(|(prev, _)| arg_extremum_better(is_max, ord, prev)) {
                best = Some((ord, r));
            }
        }
        return Ok(best.into_iter().map(|(_, r)| r.clone()).collect());
    }

    let mut groups: FxHashMap<GroupTuple, (Cell, &Row)> = FxHashMap::default();
    let mut state_bytes = 0usize;

    if by.len() == 1 {
        let gk0 = &by[0];
        let mut br0 = FieldReader::new(gk0.source_field());
        let is_plain_field = matches!(gk0.expr, GroupExpr::Field(_));
        for r in rows {
            if !flt.as_mut().is_none_or(|f| f.eval(r, now)) {
                continue;
            }
            let raw = br0.get(r);
            let ord = order_reader.get(r);
            if is_plain_field
                && !matches!(raw, Cell::Float(f) if *f == 0.0 && f.is_sign_negative())
            {
                if let Some(entry) = groups.get_mut(std::slice::from_ref(raw)) {
                    if arg_extremum_better(is_max, ord, &entry.0) {
                        *entry = (ord.clone_for_borrow(), r);
                    }
                } else {
                    let gkey: GroupTuple = smallvec::smallvec![raw.clone_for_borrow()];
                    state_bytes = state_bytes.saturating_add(group_key_bytes(&gkey, 1));
                    check_agg_quota(state_bytes, max_bytes)?;
                    groups.insert(gkey, (ord.clone_for_borrow(), r));
                }
            } else {
                let kcell = eval_group_cell(&gk0.expr, raw);
                if let Some(entry) = groups.get_mut(std::slice::from_ref(&kcell)) {
                    if arg_extremum_better(is_max, ord, &entry.0) {
                        *entry = (ord.clone_for_borrow(), r);
                    }
                } else {
                    let gkey: GroupTuple = smallvec::smallvec![kcell];
                    state_bytes = state_bytes.saturating_add(group_key_bytes(&gkey, 1));
                    check_agg_quota(state_bytes, max_bytes)?;
                    groups.insert(gkey, (ord.clone_for_borrow(), r));
                }
            }
        }
    } else {
        let mut by_readers: Vec<FieldReader<'_>> = by
            .iter()
            .map(|gk| FieldReader::new(gk.source_field()))
            .collect();
        let mut gkey_buf: GroupTuple = SmallVec::with_capacity(by.len());
        for r in rows {
            if !flt.as_mut().is_none_or(|f| f.eval(r, now)) {
                continue;
            }
            gkey_buf.clear();
            for (gk, br) in by.iter().zip(by_readers.iter_mut()) {
                gkey_buf.push(eval_group_cell(&gk.expr, br.get(r)));
            }
            let ord = order_reader.get(r);
            if let Some(entry) = groups.get_mut(gkey_buf.as_slice()) {
                if arg_extremum_better(is_max, ord, &entry.0) {
                    *entry = (ord.clone_for_borrow(), r);
                }
            } else {
                state_bytes = state_bytes.saturating_add(group_key_bytes(&gkey_buf, 1));
                check_agg_quota(state_bytes, max_bytes)?;
                groups.insert(gkey_buf.clone(), (ord.clone_for_borrow(), r));
            }
        }
    }

    let mut entries: Vec<(GroupTuple, (Cell, &Row))> = groups.into_iter().collect();
    entries.sort_unstable_by(|(a, _), (b, _)| cmp_group_tuples(a, b));
    Ok(entries
        .into_iter()
        .map(|(k, (_, r))| {
            let mut out = r.clone();
            for (gk, kcell) in by.iter().zip(k.into_iter()) {
                out.insert(gk.alias.as_str(), kcell);
            }
            out
        })
        .collect())
}

// ============================================================================
// Distributed Two-Phase Summarize (`LAGG\x01`), Post-Steps & Db Remote Bridges
// ============================================================================

const LAGG_MAGIC: &[u8; 5] = b"LAGG\x01";
const LAGG_CRC_TAG: &[u8; 4] = b"LCRC";

fn read_u16_le(cur: &mut &[u8]) -> Result<u16, Error> {
    let s = crate::batch::read_slice(cur, 2)?;
    Ok(u16::from_le_bytes([s[0], s[1]]))
}

fn read_i128_le(cur: &mut &[u8]) -> Result<i128, Error> {
    let s = crate::batch::read_slice(cur, 16)?;
    let arr: [u8; 16] = s.try_into().map_err(|_| Error::runtime("invalid i128 slice"))?;
    Ok(i128::from_le_bytes(arr))
}

fn read_f64_le(cur: &mut &[u8]) -> Result<f64, Error> {
    Ok(f64::from_bits(crate::batch::read_u64_le(cur)?))
}

fn write_metric_acc(out: &mut Vec<u8>, acc: &MetricAcc) -> Result<(), Error> {
    match acc {
        MetricAcc::Count(n) => {
            out.push(0x01);
            out.extend_from_slice(&n.to_le_bytes());
        }
        MetricAcc::Sum {
            exact,
            saw_float,
            prefer_f64,
        } => {
            out.push(0x02);
            let (ex_i, ex_f, ex_c) = exact.into_parts();
            out.extend_from_slice(&ex_i.to_le_bytes());
            out.extend_from_slice(&ex_i.to_le_bytes());
            out.extend_from_slice(&ex_f.to_bits().to_le_bytes());
            out.extend_from_slice(&ex_c.to_bits().to_le_bytes());
            let flags = (*saw_float as u8) | ((*prefer_f64 as u8) << 2);
            out.push(flags);
        }
        MetricAcc::Avg { sum, count } => {
            out.push(0x03);
            let (ex_i, ex_f, ex_c) = sum.into_parts();
            out.extend_from_slice(&ex_i.to_le_bytes());
            out.extend_from_slice(&ex_f.to_bits().to_le_bytes());
            out.extend_from_slice(&ex_c.to_bits().to_le_bytes());
            out.extend_from_slice(&count.to_le_bytes());
        }
        MetricAcc::Min(v) => {
            out.push(0x04);
            match v {
                Some(c) => {
                    out.push(1);
                    crate::batch::write_wire_cell(out, c);
                }
                None => out.push(0),
            }
        }
        MetricAcc::Max(v) => {
            out.push(0x05);
            match v {
                Some(c) => {
                    out.push(1);
                    crate::batch::write_wire_cell(out, c);
                }
                None => out.push(0),
            }
        }
        MetricAcc::DCount(dc) => {
            out.push(0x06);
            match dc {
                DCountAcc::Exact(set) => {
                    out.push(0);
                    out.extend_from_slice(&(set.len() as u32).to_le_bytes());
                    for &h in set {
                        out.extend_from_slice(&h.to_le_bytes());
                    }
                }
                DCountAcc::Hll(regs) => {
                    out.push(1);
                    out.extend_from_slice(regs.as_slice());
                }
            }
        }
        MetricAcc::Percentile { pct, acc } => {
            out.push(0x07);
            out.extend_from_slice(&pct.to_bits().to_le_bytes());
            match acc {
                PercentileAcc::Exact(vals) => {
                    out.push(0);
                    out.extend_from_slice(&(vals.len() as u32).to_le_bytes());
                    for &v in vals {
                        out.extend_from_slice(&v.to_bits().to_le_bytes());
                    }
                }
                PercentileAcc::Digest(td) => {
                    out.push(1);
                    out.extend_from_slice(&td.min.to_bits().to_le_bytes());
                    out.extend_from_slice(&td.max.to_bits().to_le_bytes());
                    out.extend_from_slice(&td.total.to_bits().to_le_bytes());
                    out.extend_from_slice(&(td.buffer.len() as u32).to_le_bytes());
                    for &v in &td.buffer {
                        out.extend_from_slice(&v.to_bits().to_le_bytes());
                    }
                    out.extend_from_slice(&(td.centroids.len() as u32).to_le_bytes());
                    for &(m, w) in &td.centroids {
                        out.extend_from_slice(&m.to_bits().to_le_bytes());
                        out.extend_from_slice(&w.to_bits().to_le_bytes());
                    }
                }
            }
        }
        MetricAcc::ArgExtremumScalar { is_max, best } => {
            out.push(0x08);
            out.push(u8::from(*is_max));
            match best {
                Some((ord, val)) => {
                    out.push(1);
                    crate::batch::write_wire_cell(out, ord);
                    crate::batch::write_wire_cell(out, val);
                }
                None => out.push(0),
            }
        }
    }
    Ok(())
}

fn read_metric_acc(cur: &mut &[u8]) -> Result<MetricAcc, Error> {
    let tag = crate::batch::read_u8(cur)?;
    match tag {
        0x01 => Ok(MetricAcc::Count(crate::batch::read_i64_le(cur)?)),
        0x02 => {
            let _int_sum = read_i128_le(cur)?;
            let ex_i = read_i128_le(cur)?;
            let ex_f = read_f64_le(cur)?;
            let ex_c = read_f64_le(cur)?;
            let flags = crate::batch::read_u8(cur)?;
            Ok(MetricAcc::Sum {
                exact: crate::exact_sum::ExactSum::from_parts(ex_i, ex_f, ex_c),
                saw_float: (flags & 3) != 0,
                prefer_f64: (flags & 4) != 0,
            })
        }
        0x03 => {
            let ex_i = read_i128_le(cur)?;
            let ex_f = read_f64_le(cur)?;
            let ex_c = read_f64_le(cur)?;
            let count = crate::batch::read_u64_le(cur)?;
            Ok(MetricAcc::Avg {
                sum: crate::exact_sum::ExactSum::from_parts(ex_i, ex_f, ex_c),
                count,
            })
        }
        0x04 | 0x05 => {
            let present = crate::batch::read_u8(cur)?;
            let val = if present != 0 {
                Some(crate::batch::read_wire_cell(cur)?)
            } else {
                None
            };
            if tag == 0x04 {
                Ok(MetricAcc::Min(val))
            } else {
                Ok(MetricAcc::Max(val))
            }
        }
        0x06 => {
            let kind = crate::batch::read_u8(cur)?;
            match kind {
                0 => {
                    let len = crate::batch::read_u32_le(cur)? as usize;
                    let mut set = FxHashSet::with_capacity_and_hasher(
                        len.min(DCOUNT_EXACT_LIMIT),
                        Default::default(),
                    );
                    for _ in 0..len {
                        set.insert(crate::batch::read_u64_le(cur)?);
                    }
                    Ok(MetricAcc::DCount(DCountAcc::Exact(set)))
                }
                1 => {
                    let raw = crate::batch::read_slice(cur, HLL_M)?;
                    let mut regs = Box::new([0u8; HLL_M]);
                    regs.copy_from_slice(raw);
                    Ok(MetricAcc::DCount(DCountAcc::Hll(regs)))
                }
                _ => Err(Error::runtime("invalid DCount tag in LAGG payload")),
            }
        }
        0x07 => {
            let pct = read_f64_le(cur)?;
            let kind = crate::batch::read_u8(cur)?;
            let acc = match kind {
                0 => {
                    let len = crate::batch::read_u32_le(cur)? as usize;
                    let mut vals = Vec::with_capacity(len.min(PERCENTILE_EXACT_LIMIT));
                    for _ in 0..len {
                        vals.push(read_f64_le(cur)?);
                    }
                    PercentileAcc::Exact(vals)
                }
                1 => {
                    let min = read_f64_le(cur)?;
                    let max = read_f64_le(cur)?;
                    let total = read_f64_le(cur)?;
                    let buf_len = crate::batch::read_u32_le(cur)? as usize;
                    let mut buffer = Vec::with_capacity(buf_len.min(TDIGEST_BUF_CAP));
                    for _ in 0..buf_len {
                        buffer.push(read_f64_le(cur)?);
                    }
                    let cent_len = crate::batch::read_u32_le(cur)? as usize;
                    let mut centroids = Vec::with_capacity(cent_len.min(512));
                    for _ in 0..cent_len {
                        centroids.push((read_f64_le(cur)?, read_f64_le(cur)?));
                    }
                    PercentileAcc::Digest(TDigest {
                        centroids,
                        buffer,
                        min,
                        max,
                        total,
                    })
                }
                _ => return Err(Error::runtime("invalid Percentile tag in LAGG payload")),
            };
            Ok(MetricAcc::Percentile { pct, acc })
        }
        0x08 => {
            let is_max = crate::batch::read_u8(cur)? != 0;
            let present = crate::batch::read_u8(cur)?;
            let best = if present != 0 {
                let ord = crate::batch::read_wire_cell(cur)?;
                let val = crate::batch::read_wire_cell(cur)?;
                Some((ord, val))
            } else {
                None
            };
            Ok(MetricAcc::ArgExtremumScalar { is_max, best })
        }
        other => Err(Error::runtime(format!("unknown LAGG metric tag {other:#04x}"))),
    }
}

fn encode_partial_groups(state: &PartialSummarizeState) -> Result<Vec<u8>, Error> {
    let mut out = Vec::with_capacity(256);
    out.extend_from_slice(LAGG_MAGIC);
    match state {
        PartialSummarizeState::Single(accs) => {
            out.push(0);
            out.extend_from_slice(&1u32.to_le_bytes());
            out.extend_from_slice(&(accs.len() as u32).to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            for acc in accs {
                write_metric_acc(&mut out, acc)?;
            }
        }
        PartialSummarizeState::Grouped(groups) => {
            out.push(1);
            out.extend_from_slice(&(groups.len() as u32).to_le_bytes());
            let spec_count = groups.values().next().map_or(0, SmallVec::len);
            out.extend_from_slice(&(spec_count as u32).to_le_bytes());
            for (gkey, accs) in groups {
                let klen = u16::try_from(gkey.len())
                    .map_err(|_| Error::runtime("group key tuple too long for LAGG"))?;
                out.extend_from_slice(&klen.to_le_bytes());
                for cell in gkey {
                    crate::batch::write_wire_cell(&mut out, cell);
                }
                for acc in accs {
                    write_metric_acc(&mut out, acc)?;
                }
            }
        }
    }
    let crc = crate::crc::hardware_crc32c(&out);
    out.extend_from_slice(LAGG_CRC_TAG);
    out.extend_from_slice(&crc.to_le_bytes());
    Ok(out)
}

enum DecodedPartialGroups {
    Single(AccVec),
    Grouped(Vec<(GroupTuple, AccVec)>),
}

fn decode_partial_groups(bytes: &[u8]) -> Result<DecodedPartialGroups, Error> {
    if bytes.len() < LAGG_MAGIC.len() + 9 + LAGG_CRC_TAG.len() + 4 {
        return Err(Error::runtime("truncated LAGG partial aggregate payload"));
    }
    if &bytes[..LAGG_MAGIC.len()] != LAGG_MAGIC {
        return Err(Error::runtime("invalid LAGG magic"));
    }
    let crc_off = bytes.len() - 8;
    if &bytes[crc_off..crc_off + 4] != LAGG_CRC_TAG {
        return Err(Error::runtime("missing LCRC trailer in LAGG payload"));
    }
    let want_crc = u32::from_le_bytes([
        bytes[crc_off + 4],
        bytes[crc_off + 5],
        bytes[crc_off + 6],
        bytes[crc_off + 7],
    ]);
    let got_crc = crate::crc::hardware_crc32c(&bytes[..crc_off]);
    if want_crc != got_crc {
        return Err(Error::runtime(format!(
            "corrupt LAGG payload (crc expected {want_crc:#010x}, got {got_crc:#010x})"
        )));
    }

    let mut cur = &bytes[LAGG_MAGIC.len()..crc_off];
    let has_by = crate::batch::read_u8(&mut cur)? != 0;
    let group_count = crate::batch::read_u32_le(&mut cur)? as usize;
    let spec_count = crate::batch::read_u32_le(&mut cur)? as usize;
    let mut decoded = Vec::with_capacity(group_count.min(4096));
    for _ in 0..group_count {
        let klen = read_u16_le(&mut cur)? as usize;
        let mut gkey = GroupTuple::with_capacity(klen);
        for _ in 0..klen {
            gkey.push(crate::batch::read_wire_cell(&mut cur)?);
        }
        let mut accs = AccVec::with_capacity(spec_count);
        for _ in 0..spec_count {
            accs.push(read_metric_acc(&mut cur)?);
        }
        decoded.push((gkey, accs));
    }
    if !cur.is_empty() {
        return Err(Error::runtime("trailing bytes in LAGG payload"));
    }
    if !has_by {
        let (_, accs) = decoded
            .into_iter()
            .next()
            .ok_or_else(|| Error::runtime("missing single group in LAGG payload"))?;
        Ok(DecodedPartialGroups::Single(accs))
    } else {
        Ok(DecodedPartialGroups::Grouped(decoded))
    }
}

pub(crate) fn eval_partial_summarize_on_reader(
    reader: &super::ReadDb,
    collection: &str,
    filter: Option<&Pred>,
    now: i64,
    specs: &[AggSpec],
    by: &[GroupKey],
) -> Result<Vec<u8>, Error> {
    let store = &reader.inner.store;
    let col_def = reader.inner.catalog.collections.get(collection);
    let sum_flags = sum_f64_flags_for(col_def, specs);
    let max_bytes = reader.inner.quotas.max_aggregate_bytes;
    let state = if store.is_disk_backed(collection) {
        let tier = store
            .disk
            .as_ref()
            .ok_or_else(|| Error::runtime("missing disk tier on disk-backed collection"))?;
        fold_summarize_disk(&tier.engine, collection, filter, now, specs, by, &sum_flags, max_bytes)?
    } else {
        let rows = store.collection(collection);
        fold_summarize_rows(&rows, filter, now, specs, by, &sum_flags, max_bytes)?
    };
    encode_partial_groups(&state)
}

pub(crate) fn empty_partial_summarize_wire(
    specs: &[AggSpec],
    by: &[GroupKey],
    sum_f64_flags: &[bool],
) -> Result<Vec<u8>, Error> {
    let state = if by.is_empty() {
        PartialSummarizeState::Single(build_accs(specs, sum_f64_flags))
    } else {
        PartialSummarizeState::Grouped(FxHashMap::default())
    };
    encode_partial_groups(&state)
}

pub(crate) fn merge_partial_summarize_wire(
    payloads: &[Vec<u8>],
    specs: &[AggSpec],
    by: &[GroupKey],
    sum_f64_flags: &[bool],
    max_bytes: usize,
) -> Result<Vec<Row>, Error> {
    let mut state_bytes = 0usize;
    if by.is_empty() {
        let mut merged = build_accs(specs, sum_f64_flags);
        for raw in payloads {
            let DecodedPartialGroups::Single(accs) = decode_partial_groups(raw)? else {
                return Err(Error::runtime(
                    "expected ungrouped partial LAGG state, got grouped",
                ));
            };
            if accs.len() != merged.len() {
                return Err(Error::runtime("partial summarize metric count mismatch"));
            }
            for (dst, src) in merged.iter_mut().zip(accs.into_iter()) {
                dst.merge(src, &mut state_bytes, max_bytes)?;
            }
        }
        return Ok(finish_partial_state(
            PartialSummarizeState::Single(merged),
            by,
            specs,
        ));
    }

    let mut merged: FxHashMap<GroupTuple, AccVec> = FxHashMap::default();
    for raw in payloads {
        let DecodedPartialGroups::Grouped(items) = decode_partial_groups(raw)? else {
            return Err(Error::runtime(
                "expected grouped partial LAGG state, got ungrouped",
            ));
        };
        for (gkey, accs) in items {
            if accs.len() != specs.len() {
                return Err(Error::runtime("partial summarize metric count mismatch"));
            }
            match merged.entry(gkey) {
                std::collections::hash_map::Entry::Vacant(e) => {
                    state_bytes = state_bytes.saturating_add(group_key_bytes(e.key(), specs.len()));
                    check_agg_quota(state_bytes, max_bytes)?;
                    e.insert(accs);
                }
                std::collections::hash_map::Entry::Occupied(mut e) => {
                    for (dst, src) in e.get_mut().iter_mut().zip(accs.into_iter()) {
                        dst.merge(src, &mut state_bytes, max_bytes)?;
                    }
                }
            }
        }
    }
    Ok(finish_partial_state(
        PartialSummarizeState::Grouped(merged),
        by,
        specs,
    ))
}

pub(crate) fn apply_coordinator_post_steps(
    mut rows: Vec<Row>,
    steps: &[Step],
    now: i64,
    quotas: &crate::Quotas,
) -> Result<Vec<Row>, Error> {
    for (idx, step) in steps.iter().enumerate() {
        match step {
            Step::Filter(pred) => {
                let mut flt = PredSlots::new(pred);
                rows.retain(|r| flt.eval(r, now));
            }
            Step::Project(fields) => {
                rows = super::eval::project_owned(rows, fields);
            }
            Step::Without(fields) => {
                rows = super::eval::without_rows(rows, fields);
            }
            Step::Sort { field, desc } => {
                let top_k = match steps.get(idx + 1) {
                    Some(Step::Take { n: Some(limit) }) => {
                        Some(usize::try_from((*limit).max(0)).unwrap_or(0))
                    }
                    _ => None,
                };
                super::eval::sort_rows_top_k(&mut rows, field, *desc, top_k);
            }
            Step::Skip { n } => {
                let drop_n = usize::try_from((*n).max(0)).unwrap_or(usize::MAX);
                if drop_n >= rows.len() {
                    rows.clear();
                } else if drop_n > 0 {
                    rows.drain(..drop_n);
                }
            }
            Step::Take { n } => {
                if let Some(limit) = n {
                    let keep = usize::try_from((*limit).max(0)).unwrap_or(0);
                    rows.truncate(keep);
                }
            }
            Step::Summarize { specs, by } => {
                let flags = vec![false; specs.len()];
                rows = agg_summarize(&rows, None, now, specs, by, &flags, quotas.max_aggregate_bytes)?;
            }
            Step::ArgExtremum { is_max, order, by } => {
                rows = agg_arg_extremum(
                    &rows,
                    None,
                    now,
                    *is_max,
                    order,
                    by,
                    quotas.max_aggregate_bytes,
                )?;
            }
            Step::Count { by } => {
                rows = match by {
                    Some(f) => agg_count(
                        &rows,
                        f,
                        quotas.max_aggregate_bytes,
                        quotas.spill_enabled,
                    )?,
                    None => vec![hits_row(rows.len() as i64)],
                };
            }
            Step::Sum { field, by } => {
                rows = agg_sum(
                    &rows,
                    field,
                    by.as_ref(),
                    quotas.max_aggregate_bytes,
                    quotas.spill_enabled,
                )?;
            }
            Step::Min { field, by } => {
                rows = agg_min(
                    &rows,
                    field,
                    by.as_ref(),
                    quotas.max_aggregate_bytes,
                    quotas.spill_enabled,
                )?;
            }
            Step::Max { field, by } => {
                rows = agg_max(
                    &rows,
                    field,
                    by.as_ref(),
                    quotas.max_aggregate_bytes,
                    quotas.spill_enabled,
                )?;
            }
            Step::Avg { field, by } => {
                rows = agg_avg(
                    &rows,
                    field,
                    by.as_ref(),
                    quotas.max_aggregate_bytes,
                    quotas.spill_enabled,
                )?;
            }
            Step::Window {
                func,
                field,
                by,
                order,
                frame,
                as_field,
            } => {
                rows = super::eval::eval_window(
                    rows,
                    func,
                    field.as_ref(),
                    by,
                    order,
                    frame.as_ref(),
                    as_field,
                );
            }
            other => {
                return Err(Error::runtime(format!(
                    "distributed coordinator post-step {other:?} requires single-node execution"
                )));
            }
        }
    }
    Ok(rows)
}

pub(crate) fn eval_partial_summarize_query_on_reader(
    reader: &super::ReadDb,
    q: &Query,
) -> Result<Vec<u8>, Error> {
    let q_cat = crate::catalog::with_catalog_filter(q, &reader.inner.catalog);
    let Some(Step::Summarize { specs, by }) = q_cat.steps.last() else {
        return Err(Error::runtime(
            "partial summarize requires query ending with summarize",
        ));
    };
    let pre_steps = &q_cat.steps[..q_cat.steps.len() - 1];
    let now = super::now_ms();
    if let Source::Collection(col) = &q_cat.source
        && pre_steps.iter().all(|s| matches!(s, Step::Filter(_)))
    {
        let mut combined: Option<Pred> = None;
        for step in pre_steps {
            if let Step::Filter(p) = step {
                combined = Some(match combined {
                    Some(prev) => prev.and(p.clone()),
                    None => p.clone(),
                });
            }
        }
        return eval_partial_summarize_on_reader(reader, col, combined.as_ref(), now, specs, by);
    }

    let mut steps = q.steps[..q.steps.len() - 1].to_vec();
    steps.push(Step::Take { n: None });
    let pre_q = Query {
        source: q.source.clone(),
        steps,
        explain: None,
        ignore_filter: q.ignore_filter,
    };
    let h = reader.run_stmt(crate::ast::Stmt::Query(pre_q))?;
    let col_def = match &q.source {
        Source::Collection(c) => reader.inner.catalog.collections.get(c),
        _ => None,
    };
    let sum_flags = sum_f64_flags_for(col_def, specs);
    let state = fold_summarize_rows(
        &h.rows,
        None,
        now,
        specs,
        by,
        &sum_flags,
        reader.inner.quotas.max_aggregate_bytes,
    )?;
    encode_partial_groups(&state)
}

pub(crate) fn coordinator_agg_arg_extremum(
    rows: &[Row],
    is_max: bool,
    order: &Field,
    by: &[GroupKey],
    max_bytes: usize,
) -> Result<Vec<Row>, Error> {
    agg_arg_extremum(rows, None, super::now_ms(), is_max, order, by, max_bytes)
}

#[inline]
fn rows_to_union_batch(rows: &[Row]) -> crate::batch::RecordBatch {
    crate::batch::RecordBatch::from_rows(rows)
}

pub(crate) fn rows_to_batch_for_query(rows: &[Row], q: &Query) -> crate::batch::RecordBatch {
    let mut has_proj = false;
    for step in &q.steps {
        match step {
            Step::Project(_) => has_proj = true,
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
            | Step::Union(_) => has_proj = false,
            _ => {}
        }
    }
    if has_proj {
        super::rows_to_query_batch(rows, q)
    } else {
        rows_to_union_batch(rows)
    }
}

impl super::ReadDb {
    pub(crate) fn run_prepared_batch_lossless(
        &self,
        prepared: &super::Prepared,
    ) -> Result<crate::batch::RecordBatch, Error> {
        if prepared.writes {
            return Err(Error::runtime("run_batch is read-only"));
        }
        if prepared.stmts.len() == 1
            && let crate::ast::Stmt::Query(q) = &prepared.stmts[0]
            && let Some(batch) = self.inner.try_columnar_query_batch(q)
        {
            return Ok(batch);
        }
        let h = self.run_prepared(prepared)?;
        Ok(match &prepared.stmts[..] {
            [crate::ast::Stmt::Query(q)] => rows_to_batch_for_query(&h.rows, q),
            _ => rows_to_union_batch(&h.rows),
        })
    }

    pub(crate) fn run_batch_lossless_with_stmts(
        &self,
        linql: &str,
        stmts: Vec<crate::ast::Stmt>,
    ) -> Result<crate::batch::RecordBatch, Error> {
        let prepared = self.prepare_with_stmts(linql, stmts)?;
        self.run_prepared_batch_lossless(&prepared)
    }
}
