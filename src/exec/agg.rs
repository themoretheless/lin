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

use super::Db;
use super::eval::{PredSlots, cmp_sort, eval_pred, pred_has_param, pred_needs_now};
use crate::store::row::FieldSlot;
use super::prepared::{AggKind, AggPlan, AggTail};

static SPILL_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

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

fn spill_partition_aggregate<'a, I, P, S, Extract, Fold>(
    tag: &str,
    rows: I,
    mut extract: Extract,
    init_state: impl Fn() -> S,
    mut fold: Fold,
) -> Result<Vec<(Cell, S)>, Error>
where
    I: IntoIterator<Item = &'a Row>,
    P: serde::Serialize + serde::de::DeserializeOwned,
    Extract: FnMut(&'a Row) -> (Cell, P),
    Fold: FnMut(&mut S, P),
{
    let num_partitions = 16usize;
    let temp_dir = std::env::temp_dir();
    let run_id = SPILL_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let mut partition_paths = Vec::with_capacity(num_partitions);
    let mut partition_files = Vec::with_capacity(num_partitions);

    for p in 0..num_partitions {
        let path = temp_dir.join(format!("lin_spill_{tag}_{run_id}_{p}.bin"));
        let file = std::fs::File::create(&path).map_err(|e| Error::runtime(e.to_string()))?;
        partition_paths.push(path);
        partition_files.push(std::io::BufWriter::new(file));
    }
    let guard = SpillPathsGuard(partition_paths);

    for r in rows {
        let (cell, payload) = extract(r);
        let part_idx = (cell_hash(&cell) as usize) % num_partitions;
        rmp_serde::encode::write_named(&mut partition_files[part_idx], &(cell, payload))
            .map_err(|e| Error::runtime(e.to_string()))?;
    }

    for mut writer in partition_files {
        use std::io::Write;
        writer.flush().map_err(|e| Error::runtime(e.to_string()))?;
    }

    let mut final_map: FxHashMap<Cell, S> = FxHashMap::default();
    for path in &guard.0 {
        if let Ok(file) = std::fs::File::open(path) {
            let mut reader = std::io::BufReader::new(file);
            while let Ok((cell, payload)) = rmp_serde::from_read::<_, (Cell, P)>(&mut reader) {
                let entry = final_map.entry(cell).or_insert_with(&init_state);
                fold(entry, payload);
            }
        }
    }

    let mut entries: Vec<(Cell, S)> = final_map.into_iter().collect();
    entries.sort_unstable_by(|(a, _), (b, _)| cmp_sort(a, b));
    Ok(entries)
}

pub(crate) fn spill_agg_count<'a>(
    rows: impl IntoIterator<Item = &'a Row>,
    by: &Field,
) -> Result<Vec<Row>, Error> {
    let k = by.as_str();
    let entries = spill_partition_aggregate(
        "count",
        rows,
        |r| {
            let cell = r.get(&k).map(Cell::clone_for_borrow).unwrap_or(Cell::Null);
            (cell, ())
        },
        || 0i64,
        |acc, ()| *acc += 1,
    )?;
    Ok(entries
        .into_iter()
        .map(|(cell, n)| {
            let mut row = crate::store::acquire_row();
            row.insert(k.clone(), cell);
            row.insert("hits", Cell::Int(n));
            row
        })
        .collect())
}

pub(crate) fn spill_agg_sum<'a>(
    rows: impl IntoIterator<Item = &'a Row>,
    field: &Field,
    by: Option<&Field>,
) -> Result<Vec<Row>, Error> {
    let fk = field.as_str();
    let bk = by.map(Field::as_str);
    let entries = spill_partition_aggregate(
        "sum",
        rows,
        |r| {
            let cell = bk
                .as_ref()
                .and_then(|k| r.get(k).map(Cell::clone_for_borrow))
                .unwrap_or(Cell::Null);
            let mut n = crate::exact_sum::ExactSum::default();
            n.add_cell(r.get(&fk));
            (cell, n)
        },
        crate::exact_sum::ExactSum::default,
        |acc, n| acc.merge(n),
    )?;
    let group_col = bk.as_deref().unwrap_or("");
    Ok(entries
        .into_iter()
        .map(|(cell, n)| {
            let mut row = crate::store::acquire_row();
            if by.is_some() {
                row.insert(group_col.to_string(), cell);
            }
            row.insert(fk.clone(), Cell::Float(n.value()));
            row
        })
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
    let entries = spill_partition_aggregate(
        "minmax",
        rows,
        |r| {
            let cell = bk
                .as_ref()
                .and_then(|k| r.get(k).map(Cell::clone_for_borrow))
                .unwrap_or(Cell::Null);
            let f_val = r
                .get(&fk)
                .filter(|c| !matches!(c, Cell::Null))
                .map(Cell::clone_for_borrow);
            (cell, f_val)
        },
        || None::<Cell>,
        |entry, val| {
            if let Some(c) = val {
                *entry = Some(match entry.take() {
                    None => c,
                    Some(prev) => {
                        let is_better = if is_max {
                            cmp_sort(&c, &prev).is_gt()
                        } else {
                            cmp_sort(&c, &prev).is_lt()
                        };
                        if is_better { c } else { prev }
                    }
                });
            }
        },
    )?;
    let group_col = bk.as_deref().unwrap_or("");
    Ok(entries
        .into_iter()
        .map(|(gcell, val)| {
            let mut row = crate::store::acquire_row();
            if by.is_some() {
                row.insert(group_col.to_string(), gcell);
            }
            row.insert(fk.clone(), val.unwrap_or(Cell::Null));
            row
        })
        .collect())
}

pub(crate) fn spill_agg_avg<'a>(
    rows: impl IntoIterator<Item = &'a Row>,
    field: &Field,
    by: Option<&Field>,
) -> Result<Vec<Row>, Error> {
    let fk = field.as_str();
    let bk = by.map(Field::as_str);
    let entries = spill_partition_aggregate(
        "avg",
        rows,
        |r| {
            let cell = bk
                .as_ref()
                .and_then(|k| r.get(k).map(Cell::clone_for_borrow))
                .unwrap_or(Cell::Null);
            let v_opt = r.get(&fk).and_then(Cell::as_f64);
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
    let group_col = bk.as_deref().unwrap_or("");
    Ok(entries
        .into_iter()
        .map(|(cell, (sum, count))| {
            let mut row = crate::store::acquire_row();
            if by.is_some() {
                row.insert(group_col.to_string(), cell);
            }
            let avg = if count > 0 { sum.value() / count as f64 } else { 0.0 };
            row.insert(fk.clone(), Cell::Float(avg));
            row
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

/// Grouped aggregate over materialized rows on the rayon pool: each fixed-size chunk
/// folds into its own map, and the partials merge in chunk order, so the result does not
/// depend on scheduling. Returns `None` when the merged state exceeds `max_bytes`; the
/// caller then runs the sequential path, which owns the quota error and the spill.
#[cfg(feature = "parallel")]
fn par_group_fold<V: Send + Default>(
    rows: &[Row],
    filter: Option<&Pred>,
    now: i64,
    key: &str,
    value: Option<&str>,
    max_bytes: usize,
    add: impl Fn(&mut V, Option<&Cell>) + Sync,
    merge: impl Fn(&mut V, V),
) -> Option<FxHashMap<Cell, V>> {
    use rayon::prelude::*;
    let parts: Vec<FxHashMap<Cell, V>> = rows
        .par_chunks(PAR_AGG_CHUNK_ROWS)
        .map(|chunk| {
            let mut map: FxHashMap<Cell, V> = FxHashMap::default();
            let mut flt = filter.map(PredSlots::new);
            let mut key_slot = FieldSlot::new(key);
            let mut value_slot = value.map(FieldSlot::new);
            for r in chunk {
                if !flt.as_mut().is_none_or(|f| f.eval(r, now)) {
                    continue;
                }
                let cell = key_slot.get(r).map(Cell::clone_for_borrow).unwrap_or(Cell::Null);
                let v = value_slot.as_mut().and_then(|s| s.get(r));
                add(map.entry(cell).or_default(), v);
            }
            map
        })
        .collect();
    let mut merged: FxHashMap<Cell, V> = FxHashMap::default();
    let mut state_bytes = 0usize;
    for part in parts {
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
    #[cfg(feature = "parallel")]
    if rows.len() >= PAR_AGG_MIN_ROWS
        && let Some(map) = par_group_fold::<i64>(
            rows,
            filter,
            now,
            &k,
            None,
            max_bytes,
            |n, _| *n += 1,
            |n, m| *n += m,
        )
    {
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
        let cell = key_slot.get(r).map(Cell::clone_for_borrow).unwrap_or(Cell::Null);
        if !map.contains_key(&cell) {
            let key_size = match &cell {
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
        }
        *map.entry(cell).or_insert(0) += 1;
    }
    Ok(count_cell_map_to_rows(&k, map))
}

pub(crate) fn count_cell_map_to_rows(by: &str, map: FxHashMap<Cell, i64>) -> Vec<Row> {
    let mut entries: Vec<(Cell, i64)> = map.into_iter().collect();
    entries.sort_unstable_by(|(a, _), (b, _)| cmp_sort(a, b));
    entries
        .into_iter()
        .map(|(cell, n)| {
            let mut row = crate::store::acquire_row();
            row.insert(by.to_string(), cell);
            row.insert("hits", Cell::Int(n));
            row
        })
        .collect()
}

pub(crate) fn count_arc_map_to_rows(by: &str, map: BTreeMap<std::sync::Arc<str>, i64>) -> Vec<Row> {
    map.into_iter()
        .map(|(g, n)| {
            let mut row = crate::store::acquire_row();
            row.insert(by.to_string(), Cell::Text(g));
            row.insert("hits", Cell::Int(n));
            row
        })
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

pub(crate) fn hits_row(n: i64) -> Row {
    let mut row = crate::store::acquire_row();
    row.insert("hits", Cell::Int(n));
    row
}

pub(crate) fn count_row(by: &str, value: &str, n: i64) -> Row {
    let mut row = crate::store::acquire_row();
    row.insert(by.to_string(), Cell::text_arc(value));
    row.insert("hits", Cell::Int(n));
    row
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
        let total = sum_rows(rows);
        let mut row = crate::store::acquire_row();
        row.insert(fk, Cell::Float(total.value()));
        return Ok(vec![row]);
    };
    let bk = by.as_str();
    let sum_rows_out = |map: FxHashMap<Cell, crate::exact_sum::ExactSum>| -> Vec<Row> {
        let mut entries: Vec<(Cell, crate::exact_sum::ExactSum)> = map.into_iter().collect();
        entries.sort_unstable_by(|(a, _), (b, _)| cmp_sort(a, b));
        entries
            .into_iter()
            .map(|(cell, n)| {
                let mut row = crate::store::acquire_row();
                row.insert(bk.clone(), cell);
                row.insert(fk.clone(), Cell::Float(n.value()));
                row
            })
            .collect()
    };
    #[cfg(feature = "parallel")]
    if rows.len() >= PAR_AGG_MIN_ROWS
        && let Some(map) = par_group_fold::<crate::exact_sum::ExactSum>(
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
        let cell = key_slot.get(r).map(Cell::clone_for_borrow).unwrap_or(Cell::Null);
        if !map.contains_key(&cell) {
            let key_size = match &cell {
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
        }
        map.entry(cell).or_default().add_cell(value_slot.get(r));
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
        let mut row = crate::store::acquire_row();
        row.insert(fk, best.unwrap_or(Cell::Null));
        return Ok(vec![row]);
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
        && let Some(folded) = par_group_fold::<Option<Cell>>(
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
        let cell = key_slot.get(r).map(Cell::clone_for_borrow).unwrap_or(Cell::Null);
        if !map.contains_key(&cell) {
            let key_size = match &cell {
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
        }
        let f_val = value_slot.get(r).filter(|c| !matches!(c, Cell::Null));
        let entry = map.entry(cell).or_insert(None);
        if let Some(c) = f_val {
            *entry = Some(match entry.take() {
                None => c.clone_for_borrow(),
                Some(prev) => {
                    if is_better(c, &prev) {
                        c.clone_for_borrow()
                    } else {
                        prev
                    }
                }
            });
        }
    }
    let mut entries: Vec<(Cell, Option<Cell>)> = map.into_iter().collect();
    entries.sort_unstable_by(|(a, _), (b, _)| cmp_sort(a, b));
    Ok(entries
        .into_iter()
        .map(|(gcell, val)| {
            let mut row = crate::store::acquire_row();
            row.insert(bk.clone(), gcell);
            row.insert(fk.clone(), val.unwrap_or(Cell::Null));
            row
        })
        .collect())
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
                    && let Some(v) = value_slot.get(r).and_then(Cell::as_f64)
                {
                    sum.add_f64(v);
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
        let (sum, count) = fold(rows);
        let avg = if count == 0 {
            0.0
        } else {
            sum.value() / (count as f64)
        };
        let mut row = crate::store::acquire_row();
        row.insert(fk, Cell::Float(avg));
        return Ok(vec![row]);
    };
    let bk = by.as_str();
    let mut map: FxHashMap<Cell, (crate::exact_sum::ExactSum, usize)> = FxHashMap::default();
    let mut state_bytes = 0usize;
    #[allow(unused_mut)]
    let mut seq_rows: &[Row] = rows;
    #[cfg(feature = "parallel")]
    if rows.len() >= PAR_AGG_MIN_ROWS
        && let Some(folded) = par_group_fold::<(crate::exact_sum::ExactSum, usize)>(
            rows,
            filter,
            now,
            &bk,
            Some(&fk),
            max_bytes,
            |acc, v| {
                if let Some(v) = v.and_then(Cell::as_f64) {
                    acc.0.add_f64(v);
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
        let cell = key_slot.get(r).map(Cell::clone_for_borrow).unwrap_or(Cell::Null);
        if !map.contains_key(&cell) {
            let key_size = match &cell {
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
        }
        let v_opt = value_slot.get(r).and_then(Cell::as_f64);
        let entry = map.entry(cell).or_default();
        if let Some(v) = v_opt {
            entry.0.add_f64(v);
            entry.1 += 1;
        }
    }
    let mut entries: Vec<(Cell, (crate::exact_sum::ExactSum, usize))> = map.into_iter().collect();
    entries.sort_unstable_by(|(a, _), (b, _)| cmp_sort(a, b));
    Ok(entries
        .into_iter()
        .map(|(gcell, (sum, count))| {
            let avg = if count == 0 {
                0.0
            } else {
                sum.value() / (count as f64)
            };
            let mut row = crate::store::acquire_row();
            row.insert(bk.clone(), gcell);
            row.insert(fk.clone(), Cell::Float(avg));
            row
        })
        .collect())
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
        Some(AggPlan {
            collection: name,
            filter,
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
                    *map.entry(Arc::clone(w_arc)).or_insert(0) += 1;
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
                        && let Some(map) = eng.sum_f64_by_col(col, fname, by_name)?
                    {
                        let state_bytes: usize = map
                            .keys()
                            .map(|cell| match cell {
                                Cell::Text(s) => s.len() + 32 + 64,
                                _ => 16 + 64,
                            })
                            .sum();
                        // Over quota: let the general path decide between spill and error.
                        if state_bytes <= max_bytes {
                            let bk = by_field.as_str();
                            let fk = field.as_str();
                            let mut entries: Vec<(Cell, f64)> = map.into_iter().collect();
                            entries.sort_unstable_by(|(a, _), (b, _)| cmp_sort(a, b));
                            return Ok(Some(
                                entries
                                    .into_iter()
                                    .map(|(cell, n)| {
                                        let mut row = crate::store::acquire_row();
                                        row.insert(bk.clone(), cell);
                                        row.insert(fk.clone(), Cell::Float(n));
                                        row
                                    })
                                    .collect(),
                            ));
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
            let cached_or_fresh = plan.index_plan.clone().or_else(|| {
                crate::index::pick_index_with_coverage(&self.catalog, &plan.collection, pred)
            });
            if let Some((uses, true)) = cached_or_fresh
                && let Some(n) = self.store.index_seek_count(&plan.collection, &uses, now)
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
            match &plan.kind {
                AggKind::Sum { field, by } if field.leaf() == Some("total") => {
                    if by.is_none() && plan.filter.is_none() {
                        let total: f64 = self.store.orders_total().iter().sum();
                        let mut row = crate::store::acquire_row();
                        row.insert("total", Cell::Float(total));
                        return Ok(vec![row]);
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
                                let mut out = Vec::with_capacity(live_idx.forward.len());
                                for (key, list) in &live_idx.forward {
                                    let uid_cell = match key.0.first() {
                                        Some(crate::index::IndexPart::Text(s)) => {
                                            Cell::Text(Arc::clone(s))
                                        }
                                        Some(crate::index::IndexPart::SmallText(s)) => {
                                            Cell::text_arc(s.as_str())
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
                                    let mut row = crate::store::acquire_row();
                                    row.insert("user_id", uid_cell);
                                    row.insert("total", Cell::Float(sum));
                                    out.push(row);
                                }
                                return Ok(out);
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
                            return Ok(entries
                                .into_iter()
                                .map(|(uid, tot)| {
                                    let mut row = crate::store::acquire_row();
                                    row.insert("user_id", Cell::Text(uid));
                                    row.insert("total", Cell::Float(tot));
                                    row
                                })
                                .collect());
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
                                return Ok(entries
                                    .into_iter()
                                    .map(|(uid, tot)| {
                                        let mut row = crate::store::acquire_row();
                                        row.insert("user_id", Cell::Text(uid));
                                        row.insert("total", Cell::Float(tot));
                                        row
                                    })
                                    .collect());
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
                    let mut row = crate::store::acquire_row();
                    row.insert("total", Cell::Float(avg));
                    return Ok(vec![row]);
                }
                _ => {}
            }
        }

        // 2. SoA paths for docs
        if plan.collection == "docs"
            && !self.store.is_disk_backed("docs")
            && self.store.docs_wing().len() == self.store.collection("docs").len()
        {
            if let AggKind::Count { by: Some(by_field) } = &plan.kind
                && by_field.leaf() == Some("wing")
                && plan.filter.is_none()
            {
                let wings = self.store.docs_wing();
                if let Some((indices, dict)) = wings.as_indices() {
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
                    let mut entries: Vec<(Arc<str>, i64)> = Vec::with_capacity(dict.len());
                    for (idx, arc) in dict.iter().enumerate() {
                        let c = counts0[idx] + counts1[idx] + counts2[idx] + counts3[idx];
                        if c > 0 {
                            entries.push((Arc::clone(arc), c));
                        }
                    }
                    entries.sort_unstable_by(|(a, _), (b, _)| a.cmp(b));
                    return Ok(entries
                        .into_iter()
                        .map(|(w, count)| {
                            let mut row = crate::store::acquire_row();
                            row.insert("wing", Cell::Text(w));
                            row.insert("hits", Cell::Int(count));
                            row
                        })
                        .collect());
                } else {
                    let wings_vec = wings.to_vec();
                    let (s0, s1, overflow) = Self::count_arc_slice_low_cardinality(&wings_vec);
                    let mut combined: FxHashMap<Arc<str>, i64> = overflow.unwrap_or_default();
                    if let Some((k, v)) = s0 {
                        *combined.entry(k).or_insert(0) += v;
                    }
                    if let Some((k, v)) = s1 {
                        *combined.entry(k).or_insert(0) += v;
                    }
                    let mut entries: Vec<(Arc<str>, i64)> = combined.into_iter().collect();
                    entries.sort_unstable_by(|(a, _), (b, _)| a.cmp(b));
                    return Ok(entries
                        .into_iter()
                        .map(|(w, count)| {
                            let mut row = crate::store::acquire_row();
                            row.insert("wing", Cell::Text(w));
                            row.insert("hits", Cell::Int(count));
                            row
                        })
                        .collect());
                }
            }
        }

        // 3. Index-sought ungrouped Sum / Avg / Min / Max fast path (no full table scan, no Row clone!)
        if let Some((ref uses, covered)) = plan.index_plan
            && let Some(idxs) = self.store.index_seek(&plan.collection, uses, now)
        {
            let col = self.store.collection(&plan.collection);
            let pred_opt = if covered { None } else { plan.filter.as_ref() };
            let mut residual = pred_opt.map(PredSlots::new);
            match &plan.kind {
                AggKind::Sum { field, by: None } => {
                    let fk = field.as_str();
                    let mut total = crate::exact_sum::ExactSum::default();
                    for i in idxs {
                        if let Some(r) = col.get(i)
                            && residual.as_mut().is_none_or(|f| f.eval(r, now))
                        {
                            total.add_cell(r.get(&fk));
                        }
                    }
                    let mut row = crate::store::acquire_row();
                    row.insert(fk, Cell::Float(total.value()));
                    return Ok(vec![row]);
                }
                AggKind::Avg { field, by: None } => {
                    let fk = field.as_str();
                    let mut sum = crate::exact_sum::ExactSum::default();
                    let mut count = 0usize;
                    for i in idxs {
                        if let Some(r) = col.get(i)
                            && residual.as_mut().is_none_or(|f| f.eval(r, now))
                            && let Some(v) = r.get(&fk).and_then(Cell::as_f64)
                        {
                            sum.add_f64(v);
                            count += 1;
                        }
                    }
                    let avg = if count == 0 {
                        0.0
                    } else {
                        sum.value() / (count as f64)
                    };
                    let mut row = crate::store::acquire_row();
                    row.insert(fk, Cell::Float(avg));
                    return Ok(vec![row]);
                }
                AggKind::Min { field, by: None } | AggKind::Max { field, by: None } => {
                    let is_max = matches!(plan.kind, AggKind::Max { .. });
                    let fk = field.as_str();
                    let mut best: Option<Cell> = None;
                    for i in idxs {
                        if let Some(r) = col.get(i)
                            && residual.as_mut().is_none_or(|f| f.eval(r, now))
                            && let Some(c) = r.get(&fk).filter(|c| !matches!(c, Cell::Null))
                        {
                            best = Some(match best {
                                None => c.clone_for_borrow(),
                                Some(prev) => {
                                    let better = if is_max {
                                        cmp_sort(c, &prev).is_gt()
                                    } else {
                                        cmp_sort(c, &prev).is_lt()
                                    };
                                    if better { c.clone_for_borrow() } else { prev }
                                }
                            });
                        }
                    }
                    let mut row = crate::store::acquire_row();
                    row.insert(fk, best.unwrap_or(Cell::Null));
                    return Ok(vec![row]);
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
                    let mut n = 0i64;
                    if let Some(p) = filter {
                        // Cached field positions per row shape instead of a search by
                        // name for every field of the predicate in every row.
                        let mut flt = PredSlots::new(p);
                        for r in col {
                            if flt.eval(r, now) {
                                n += 1;
                            }
                        }
                    } else {
                        n = col.len() as i64;
                    }
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
                let mut map: FxHashMap<Cell, i64> = FxHashMap::default();
                for r in &rows {
                    let cell = r
                        .get(&by_key)
                        .map(Cell::clone_for_borrow)
                        .unwrap_or(Cell::Null);
                    *map.entry(cell).or_insert(0) += 1;
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
            let mut flt = PredSlots::new(pred);
            let mut n = 0i64;
            for row in col {
                if flt.eval(row, now) {
                    n += 1;
                }
            }
            return Some(vec![hits_row(n)]);
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
            let mut map: FxHashMap<Cell, i64> = FxHashMap::default();
            for i in idxs {
                let Some(row) = self.store.get_by_idx(name, i) else {
                    continue;
                };
                let cell = row
                    .get(&by_key)
                    .map(Cell::clone_for_borrow)
                    .unwrap_or(Cell::Null);
                *map.entry(cell).or_insert(0) += 1;
            }
            return Some(count_cell_map_to_rows(&by_key, map));
        }

        if let Some(idxs) = self.mirror_select(name, pred, now) {
            let mut map: FxHashMap<Cell, i64> = FxHashMap::default();
            let mut key_slot = FieldSlot::new(&by_key);
            for i in idxs {
                let Some(row) = self.store.get_by_idx(name, i) else {
                    continue;
                };
                let cell = key_slot
                    .get(row)
                    .map(Cell::clone_for_borrow)
                    .unwrap_or(Cell::Null);
                *map.entry(cell).or_insert(0) += 1;
            }
            return Some(count_cell_map_to_rows(&by_key, map));
        }

        let col = self.store.collection(name);
        let mut map: FxHashMap<Cell, i64> = FxHashMap::default();
        let mut flt = PredSlots::new(pred);
        let mut key_slot = FieldSlot::new(&by_key);
        for r in col {
            if flt.eval(r, now) {
                let cell = key_slot.get(r).map(Cell::clone_for_borrow).unwrap_or(Cell::Null);
                *map.entry(cell).or_insert(0) += 1;
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
            agg_summarize(col, filter, now, specs, by, &sum_f64_flags, max_bytes)?
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

enum DCountAcc {
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

    fn finish(self) -> i64 {
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
                    sum_inv += 2.0f64.powi(-(i32::from(r)));
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
}

struct TDigest {
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
        let mut merged: Vec<(f64, f64)> = Vec::with_capacity(self.centroids.len().min(128));
        let mut it = self.centroids.drain(..);
        let (mut cur_m, mut cur_w) = it.next().unwrap();
        let mut weight_so_far = 0.0f64;
        let mut k_limit = k_of(0.0) + 1.0;
        for (m, w) in it {
            let next_q = (weight_so_far + cur_w + w) / total;
            if k_of(next_q) <= k_limit {
                let new_w = cur_w + w;
                cur_m = (cur_m * cur_w + m * w) / new_w;
                cur_w = new_w;
            } else {
                merged.push((cur_m, cur_w));
                weight_so_far += cur_w;
                k_limit = k_of(weight_so_far / total) + 1.0;
                cur_m = m;
                cur_w = w;
            }
        }
        merged.push((cur_m, cur_w));
        self.centroids = merged;
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
}

enum PercentileAcc {
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

    fn finish(self, pct: f64) -> Cell {
        let val = match self {
            PercentileAcc::Exact(mut vals) => {
                if vals.is_empty() {
                    None
                } else {
                    vals.sort_by(f64::total_cmp);
                    let n = vals.len();
                    if pct <= 0.0 {
                        Some(vals[0])
                    } else if pct >= 100.0 {
                        Some(vals[n - 1])
                    } else {
                        let pos = (pct / 100.0) * ((n - 1) as f64);
                        let lo = pos.floor() as usize;
                        let hi = pos.ceil() as usize;
                        let w = pos - (lo as f64);
                        Some(vals[lo] + (vals[hi] - vals[lo]) * w)
                    }
                }
            }
            PercentileAcc::Digest(mut td) => td.quantile(pct),
        };
        match val {
            Some(v) => Cell::Float(if v == 0.0 { 0.0 } else { v }),
            None => Cell::Null,
        }
    }
}

enum MetricAcc {
    Count(i64),
    Sum {
        int_sum: i128,
        exact: crate::exact_sum::ExactSum,
        saw_float: bool,
        int_overflow: bool,
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
                int_sum: 0,
                exact: crate::exact_sum::ExactSum::default(),
                saw_float: false,
                int_overflow: false,
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
                int_sum,
                exact,
                saw_float,
                int_overflow,
                ..
            } => match c1 {
                Cell::Null => {}
                Cell::Int(i) => {
                    exact.add_i64(*i);
                    match int_sum.checked_add(i128::from(*i)) {
                        Some(s) => *int_sum = s,
                        None => *int_overflow = true,
                    }
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
                Cell::Int(i) => {
                    sum.add_i64(*i);
                    *count += 1;
                }
                other => {
                    if let Some(f) = other.as_f64() {
                        sum.add_f64(f);
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

    fn finish(self) -> Cell {
        match self {
            MetricAcc::Count(n) => Cell::Int(n),
            MetricAcc::Sum {
                int_sum,
                exact,
                saw_float,
                int_overflow,
                prefer_f64,
            } => {
                if saw_float
                    || int_overflow
                    || int_sum < i128::from(i64::MIN)
                    || int_sum > i128::from(i64::MAX)
                {
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

fn build_accs(specs: &[AggSpec], sum_f64_flags: &[bool]) -> Vec<MetricAcc> {
    specs
        .iter()
        .enumerate()
        .map(|(i, s)| MetricAcc::from_spec(s, sum_f64_flags.get(i).copied().unwrap_or(false)))
        .collect()
}

fn emit_summarize_row(by: &[GroupKey], keys: GroupTuple, specs: &[AggSpec], accs: Vec<MetricAcc>) -> Row {
    let mut row = crate::store::acquire_row();
    for (gk, kcell) in by.iter().zip(keys.into_iter()) {
        row.insert(gk.alias.clone(), kcell);
    }
    for (spec, acc) in specs.iter().zip(accs.into_iter()) {
        row.insert(spec.alias.clone(), acc.finish());
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

pub(crate) fn agg_summarize(
    rows: &[Row],
    filter: Option<&Pred>,
    now: i64,
    specs: &[AggSpec],
    by: &[GroupKey],
    sum_f64_flags: &[bool],
    max_bytes: usize,
) -> Result<Vec<Row>, Error> {
    static NULL: Cell = Cell::Null;
    let mut flt = filter.map(PredSlots::new);
    let mut by_readers: Vec<FieldReader<'_>> = by
        .iter()
        .map(|gk| FieldReader::new(gk.source_field()))
        .collect();
    let mut spec_readers: Vec<SpecReader<'_>> = specs.iter().map(SpecReader::from_spec).collect();
    let mut state_bytes = 0usize;

    if by.is_empty() {
        let mut accs = build_accs(specs, sum_f64_flags);
        for r in rows {
            if !flt.as_mut().is_none_or(|f| f.eval(r, now)) {
                continue;
            }
            for (acc, sr) in accs.iter_mut().zip(spec_readers.iter_mut()) {
                let (c1, c2) = match sr {
                    SpecReader::None => (&NULL, None),
                    SpecReader::One(r1) => (r1.get(r), None),
                    SpecReader::Two(r1, r2) => (r1.get(r), Some(r2.get(r))),
                };
                acc.push(c1, c2, &mut state_bytes, max_bytes)?;
            }
        }
        return Ok(vec![emit_summarize_row(by, GroupTuple::new(), specs, accs)]);
    }

    let mut groups: FxHashMap<GroupTuple, Vec<MetricAcc>> = FxHashMap::default();
    for r in rows {
        if !flt.as_mut().is_none_or(|f| f.eval(r, now)) {
            continue;
        }
        let mut gkey: GroupTuple = SmallVec::with_capacity(by.len());
        for (gk, br) in by.iter().zip(by_readers.iter_mut()) {
            gkey.push(eval_group_cell(&gk.expr, br.get(r)));
        }
        let accs = match groups.entry(gkey) {
            std::collections::hash_map::Entry::Occupied(e) => e.into_mut(),
            std::collections::hash_map::Entry::Vacant(e) => {
                state_bytes = state_bytes.saturating_add(group_key_bytes(e.key(), specs.len()));
                check_agg_quota(state_bytes, max_bytes)?;
                e.insert(build_accs(specs, sum_f64_flags))
            }
        };
        for (acc, sr) in accs.iter_mut().zip(spec_readers.iter_mut()) {
            let (c1, c2) = match sr {
                SpecReader::None => (&NULL, None),
                SpecReader::One(r1) => (r1.get(r), None),
                SpecReader::Two(r1, r2) => (r1.get(r), Some(r2.get(r))),
            };
            acc.push(c1, c2, &mut state_bytes, max_bytes)?;
        }
    }

    let mut entries: Vec<(GroupTuple, Vec<MetricAcc>)> = groups.into_iter().collect();
    entries.sort_unstable_by(|(a, _), (b, _)| cmp_group_tuples(a, b));
    Ok(entries
        .into_iter()
        .map(|(k, accs)| emit_summarize_row(by, k, specs, accs))
        .collect())
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
        return agg_summarize(&rows, None, now, specs, by, sum_f64_flags, max_bytes);
    }

    static NULL: Cell = Cell::Null;
    let mut state_bytes = 0usize;
    let mut single = if by.is_empty() {
        Some(build_accs(specs, sum_f64_flags))
    } else {
        None
    };
    let mut groups: FxHashMap<GroupTuple, Vec<MetricAcc>> = FxHashMap::default();
    let mut loop_err: Option<Error> = None;

    let scanned = eng.scan_cells(table, filter, &needed, now, |_, cells| {
        let accs: &mut [MetricAcc] = if let Some(ref mut s) = single {
            s.as_mut_slice()
        } else {
            let mut gkey: GroupTuple = SmallVec::with_capacity(by.len());
            for (gk, &idx) in by.iter().zip(by_pos.iter()) {
                let raw = cells.get(idx).unwrap_or(&NULL);
                gkey.push(eval_group_cell(&gk.expr, raw));
            }
            match groups.entry(gkey) {
                std::collections::hash_map::Entry::Occupied(e) => e.into_mut(),
                std::collections::hash_map::Entry::Vacant(e) => {
                    state_bytes = state_bytes.saturating_add(group_key_bytes(e.key(), specs.len()));
                    if let Err(err) = check_agg_quota(state_bytes, max_bytes) {
                        loop_err = Some(err);
                        return false;
                    }
                    e.insert(build_accs(specs, sum_f64_flags))
                }
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
        return agg_summarize(&rows, None, now, specs, by, sum_f64_flags, max_bytes);
    }

    if let Some(accs) = single {
        return Ok(vec![emit_summarize_row(by, GroupTuple::new(), specs, accs)]);
    }
    let mut entries: Vec<(GroupTuple, Vec<MetricAcc>)> = groups.into_iter().collect();
    entries.sort_unstable_by(|(a, _), (b, _)| cmp_group_tuples(a, b));
    Ok(entries
        .into_iter()
        .map(|(k, accs)| emit_summarize_row(by, k, specs, accs))
        .collect())
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

    let mut by_readers: Vec<FieldReader<'_>> = by
        .iter()
        .map(|gk| FieldReader::new(gk.source_field()))
        .collect();
    let mut groups: FxHashMap<GroupTuple, (Cell, &Row)> = FxHashMap::default();
    let mut state_bytes = 0usize;

    for r in rows {
        if !flt.as_mut().is_none_or(|f| f.eval(r, now)) {
            continue;
        }
        let mut gkey: GroupTuple = SmallVec::with_capacity(by.len());
        for (gk, br) in by.iter().zip(by_readers.iter_mut()) {
            gkey.push(eval_group_cell(&gk.expr, br.get(r)));
        }
        let ord = order_reader.get(r);
        match groups.entry(gkey) {
            std::collections::hash_map::Entry::Vacant(e) => {
                state_bytes = state_bytes.saturating_add(group_key_bytes(e.key(), 1));
                check_agg_quota(state_bytes, max_bytes)?;
                e.insert((ord.clone_for_borrow(), r));
            }
            std::collections::hash_map::Entry::Occupied(mut e) => {
                let (prev, _) = e.get();
                if arg_extremum_better(is_max, ord, prev) {
                    *e.get_mut() = (ord.clone_for_borrow(), r);
                }
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
                out.insert(gk.alias.clone(), kcell);
            }
            out
        })
        .collect())
}
