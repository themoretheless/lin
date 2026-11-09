use std::collections::BTreeMap;

use rustc_hash::FxHashMap;

use crate::ast::{CmpOp, Field, Pred, Value};
use crate::error::Error;
use crate::store::{Cell, Row};

use super::eval::{cmp_sort, eval_pred};

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
                        rows.iter().filter(|r| filter.is_none_or(|p| eval_pred(p, r, now))),
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
                        rows.iter().filter(|r| filter.is_none_or(|p| eval_pred(p, r, now))),
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
                        rows.iter().filter(|r| filter.is_none_or(|p| eval_pred(p, r, now))),
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
                        rows.iter().filter(|r| filter.is_none_or(|p| eval_pred(p, r, now))),
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
