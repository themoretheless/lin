use std::collections::BTreeMap;
use std::sync::Arc;

use rustc_hash::FxHashMap;

use crate::ast::{CmpOp, Field, Pred, Query, Source, Step, Value};
use crate::error::Error;
use crate::store::{Cell, Row};

use super::Db;
use super::eval::{cmp_sort, eval_pred, pred_has_param, pred_needs_now};
use super::prepared::{AggKind, AggPlan};

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
            row.insert("hits".into(), Cell::Int(n));
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
            let n = r.get(&fk).and_then(Cell::as_f64).unwrap_or(0.0);
            (cell, n)
        },
        || 0.0f64,
        |acc, n| *acc += n,
    )?;
    let group_col = bk.as_deref().unwrap_or("");
    Ok(entries
        .into_iter()
        .map(|(cell, n)| {
            let mut row = crate::store::acquire_row();
            if by.is_some() {
                row.insert(group_col.to_string(), cell);
            }
            row.insert(fk.clone(), Cell::Float(n));
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
        || (0.0f64, 0usize),
        |entry, v_opt| {
            if let Some(v) = v_opt {
                entry.0 += v;
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
            let avg = if count > 0 { sum / count as f64 } else { 0.0 };
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

pub(crate) fn agg_count_filtered(
    rows: &[Row],
    filter: Option<&Pred>,
    now: i64,
    by: &Field,
    max_bytes: usize,
    spill: bool,
) -> Result<Vec<Row>, Error> {
    let k = by.as_str();
    let mut map: FxHashMap<Cell, i64> = FxHashMap::default();
    let mut state_bytes = 0usize;
    for r in rows {
        if !filter.is_none_or(|p| eval_pred(p, r, now)) {
            continue;
        }
        let cell = r.get(&k).map(Cell::clone_for_borrow).unwrap_or(Cell::Null);
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
            row.insert("hits".into(), Cell::Int(n));
            row
        })
        .collect()
}

pub(crate) fn count_arc_map_to_rows(by: &str, map: BTreeMap<std::sync::Arc<str>, i64>) -> Vec<Row> {
    map.into_iter()
        .map(|(g, n)| {
            let mut row = crate::store::acquire_row();
            row.insert(by.to_string(), Cell::Text(g));
            row.insert("hits".into(), Cell::Int(n));
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
    row.insert("hits".into(), Cell::Int(n));
    row
}

pub(crate) fn count_row(by: &str, value: &str, n: i64) -> Row {
    let mut row = crate::store::acquire_row();
    row.insert(by.to_string(), Cell::text_arc(value));
    row.insert("hits".into(), Cell::Int(n));
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
        let mut total = 0.0f64;
        for r in rows {
            if filter.is_none_or(|p| eval_pred(p, r, now)) {
                total += r.get(&fk).and_then(Cell::as_f64).unwrap_or(0.0);
            }
        }
        let mut row = crate::store::acquire_row();
        row.insert(fk, Cell::Float(total));
        return Ok(vec![row]);
    };
    let bk = by.as_str();
    let mut map: FxHashMap<Cell, f64> = FxHashMap::default();
    let mut state_bytes = 0usize;
    for r in rows {
        if !filter.is_none_or(|p| eval_pred(p, r, now)) {
            continue;
        }
        let cell = r.get(&bk).map(Cell::clone_for_borrow).unwrap_or(Cell::Null);
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
        let n = r.get(&fk).and_then(Cell::as_f64).unwrap_or(0.0);
        *map.entry(cell).or_insert(0.0) += n;
    }
    let mut entries: Vec<(Cell, f64)> = map.into_iter().collect();
    entries.sort_unstable_by(|(a, _), (b, _)| cmp_sort(a, b));
    Ok(entries
        .into_iter()
        .map(|(cell, n)| {
            let mut row = crate::store::acquire_row();
            row.insert(bk.clone(), cell);
            row.insert(fk.clone(), Cell::Float(n));
            row
        })
        .collect())
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
        let mut best: Option<Cell> = None;
        for r in rows {
            if filter.is_none_or(|p| eval_pred(p, r, now))
                && let Some(c) = r.get(&fk).filter(|c| !matches!(c, Cell::Null))
            {
                best = Some(match best {
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
        let mut row = crate::store::acquire_row();
        row.insert(fk, best.unwrap_or(Cell::Null));
        return Ok(vec![row]);
    };
    let bk = by.as_str();
    let mut map: FxHashMap<Cell, Option<Cell>> = FxHashMap::default();
    let mut state_bytes = 0usize;
    for r in rows {
        if !filter.is_none_or(|p| eval_pred(p, r, now)) {
            continue;
        }
        let cell = r.get(&bk).map(Cell::clone_for_borrow).unwrap_or(Cell::Null);
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
        let f_val = r.get(&fk).filter(|c| !matches!(c, Cell::Null));
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
        let mut sum = 0.0f64;
        let mut count = 0usize;
        for r in rows {
            if filter.is_none_or(|p| eval_pred(p, r, now))
                && let Some(v) = r.get(&fk).and_then(Cell::as_f64)
            {
                sum += v;
                count += 1;
            }
        }
        let avg = if count == 0 {
            0.0
        } else {
            sum / (count as f64)
        };
        let mut row = crate::store::acquire_row();
        row.insert(fk, Cell::Float(avg));
        return Ok(vec![row]);
    };
    let bk = by.as_str();
    let mut map: FxHashMap<Cell, (f64, usize)> = FxHashMap::default();
    let mut state_bytes = 0usize;
    for r in rows {
        if !filter.is_none_or(|p| eval_pred(p, r, now)) {
            continue;
        }
        let cell = r.get(&bk).map(Cell::clone_for_borrow).unwrap_or(Cell::Null);
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
        let v_opt = r.get(&fk).and_then(Cell::as_f64);
        let entry = map.entry(cell).or_insert((0.0, 0));
        if let Some(v) = v_opt {
            entry.0 += v;
            entry.1 += 1;
        }
    }
    let mut entries: Vec<(Cell, (f64, usize))> = map.into_iter().collect();
    entries.sort_unstable_by(|(a, _), (b, _)| cmp_sort(a, b));
    Ok(entries
        .into_iter()
        .map(|(gcell, (sum, count))| {
            let avg = if count == 0 {
                0.0
            } else {
                sum / (count as f64)
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
                _ => return None,
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
                    row.insert("hits".into(), Cell::Int(n as i64));
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
                    row.insert("hits".into(), Cell::Int(n as i64));
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
                        row.insert("total".into(), Cell::Float(total));
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
                                    row.insert("user_id".into(), uid_cell);
                                    row.insert("total".into(), Cell::Float(sum));
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
                                    row.insert("user_id".into(), Cell::Text(uid));
                                    row.insert("total".into(), Cell::Float(tot));
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
                                        row.insert("user_id".into(), Cell::Text(uid));
                                        row.insert("total".into(), Cell::Float(tot));
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
                    row.insert("total".into(), Cell::Float(avg));
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
                            row.insert("wing".into(), Cell::Text(w));
                            row.insert("hits".into(), Cell::Int(count));
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
                            row.insert("wing".into(), Cell::Text(w));
                            row.insert("hits".into(), Cell::Int(count));
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
            match &plan.kind {
                AggKind::Sum { field, by: None } => {
                    let fk = field.as_str();
                    let mut total = 0.0f64;
                    for i in idxs {
                        if let Some(r) = col.get(i)
                            && pred_opt.is_none_or(|p| eval_pred(p, r, now))
                        {
                            total += r.get(&fk).and_then(Cell::as_f64).unwrap_or(0.0);
                        }
                    }
                    let mut row = crate::store::acquire_row();
                    row.insert(fk, Cell::Float(total));
                    return Ok(vec![row]);
                }
                AggKind::Avg { field, by: None } => {
                    let fk = field.as_str();
                    let mut sum = 0.0f64;
                    let mut count = 0usize;
                    for i in idxs {
                        if let Some(r) = col.get(i)
                            && pred_opt.is_none_or(|p| eval_pred(p, r, now))
                            && let Some(v) = r.get(&fk).and_then(Cell::as_f64)
                        {
                            sum += v;
                            count += 1;
                        }
                    }
                    let avg = if count == 0 {
                        0.0
                    } else {
                        sum / (count as f64)
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
                            && pred_opt.is_none_or(|p| eval_pred(p, r, now))
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
                        for r in col {
                            if eval_pred(p, r, now) {
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
                        row.insert("hits".into(), Cell::Int(n as i64));
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
            let col = self.store.collection(name);
            let mut n = 0i64;
            for row in col {
                if eval_pred(pred, row, now) {
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

        let col = self.store.collection(name);
        let mut map: FxHashMap<Cell, i64> = FxHashMap::default();
        for r in col {
            if eval_pred(pred, r, now) {
                let cell = r
                    .get(&by_key)
                    .map(Cell::clone_for_borrow)
                    .unwrap_or(Cell::Null);
                *map.entry(cell).or_insert(0) += 1;
            }
        }
        Some(count_cell_map_to_rows(&by_key, map))
    }
}
