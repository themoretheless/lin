//! Source/resource qualification harness: fresh SUM after each orders mutation.
use lin::{Cell, Db};
use serde_json::{Value, json};
use std::hint::black_box;
use std::time::Instant;

fn sum_value(rows: &[lin::Row]) -> f64 {
    assert_eq!(rows.len(), 1);
    match rows[0].get("total").expect("SUM total") {
        Cell::Float(value) => *value,
        Cell::Int(value) => *value as f64,
        other => panic!("unexpected SUM cell: {other:?}"),
    }
}

fn checked_rows(db: &Db, n: usize, updated: bool, deleted: bool) -> Value {
    let rows = db.store.collection("orders");
    assert_eq!(rows.len(), n - usize::from(deleted));
    let expected = (0..n - usize::from(deleted))
        .map(|i| {
            json!([
                format!("o-{i}"),
                "u",
                if updated && i == 0 { 201 } else { i % 200 }
            ])
        })
        .collect::<Vec<_>>();
    let actual = rows
        .iter()
        .map(|row| {
            let id = row.get("id").and_then(Cell::text).expect("id");
            let user = row.get("user_id").and_then(Cell::text).expect("user_id");
            let total = match row.get("total").expect("total") {
                Cell::Int(value) => *value,
                other => panic!("unexpected row total: {other:?}"),
            };
            json!([id, user, total])
        })
        .collect::<Vec<_>>();
    assert_eq!(actual, expected);
    Value::Array(actual)
}

fn probe(n: usize, samples: usize) -> Result<Value, Box<dyn std::error::Error>> {
    if n < 2 || samples == 0 {
        return Err("requires rows >= 2 and samples > 0".into());
    }
    let sources = (0..n)
        .step_by(500)
        .map(|start| {
            let records = (start..(start + 500).min(n))
                .map(|i| format!(r#"{{id:"o-{i}",user_id:"u",total:{}}}"#, i % 200))
                .collect::<Vec<_>>();
            format!("insert orders [{}]", records.join(","))
        })
        .collect::<Vec<_>>();
    let base_sum = (0..n).map(|i| (i % 200) as f64).sum::<f64>();
    let fields = [
        "insert",
        "first_sum",
        "warm_sum",
        "update",
        "after_update_sum",
        "delete",
        "after_delete_sum",
    ];
    let mut measured = Vec::with_capacity(samples);
    for _ in 0..samples {
        let mut db = Db::empty();
        db.run(r#"insert users [{id:"u",email:"u@bench.local"}]"#)?;
        let inserts = sources
            .iter()
            .map(|source| db.prepare(source))
            .collect::<Result<Vec<_>, _>>()?;
        let sum = db.prepare("orders | sum total")?;
        let update = db.prepare(r#"update orders[id == "o-0"] cas each {total:201}"#)?;
        let delete = db.prepare(&format!(r#"delete orders[id == "o-{}"] cas each"#, n - 1))?;
        let mut durations = Vec::new();
        let start = Instant::now();
        let mut tx = db.transaction()?;
        let mut affected = 0;
        for statement in &inserts {
            affected += black_box(tx.run_prepared(statement)?).done.n;
        }
        tx.commit()?;
        durations.push(start.elapsed().as_nanos());
        assert_eq!(affected, n);
        let mut sums = Vec::new();
        let start = Instant::now();
        let result = black_box(sum.run(&mut db)?);
        durations.push(start.elapsed().as_nanos());
        sums.push(sum_value(&result.rows));
        assert_eq!(sums[0].to_bits(), base_sum.to_bits());
        let start = Instant::now();
        let result = black_box(sum.run(&mut db)?);
        durations.push(start.elapsed().as_nanos());
        assert_eq!(sum_value(&result.rows).to_bits(), base_sum.to_bits());
        let inserted = checked_rows(&db, n, false, false);
        let start = Instant::now();
        let result = black_box(update.run(&mut db)?);
        durations.push(start.elapsed().as_nanos());
        assert_eq!(result.done.n, 1);
        let start = Instant::now();
        let result = black_box(sum.run(&mut db)?);
        durations.push(start.elapsed().as_nanos());
        sums.push(sum_value(&result.rows));
        assert_eq!(sums[1].to_bits(), (base_sum + 201.0).to_bits());
        let updated = checked_rows(&db, n, true, false);
        let start = Instant::now();
        let result = black_box(delete.run(&mut db)?);
        durations.push(start.elapsed().as_nanos());
        assert_eq!(result.done.n, 1);
        let start = Instant::now();
        let result = black_box(sum.run(&mut db)?);
        durations.push(start.elapsed().as_nanos());
        sums.push(sum_value(&result.rows));
        assert_eq!(
            sums[2].to_bits(),
            (base_sum + 201.0 - ((n - 1) % 200) as f64).to_bits()
        );
        let deleted = checked_rows(&db, n, true, true);
        assert_eq!(durations.len(), fields.len());
        assert!(durations.iter().all(|n| *n > 0));
        measured.push(json!({"phase_ns":fields.iter().zip(durations).map(|(k,v)|(k.to_string(),json!(v))).collect::<serde_json::Map<_,_>>(),"sums":sums,"states":[inserted,updated,deleted]}));
    }
    Ok(json!({"rows":n,"samples":measured,"statement_rows_limit":500}))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().collect::<Vec<_>>();
    if args.len() != 3 {
        return Err("usage: orders_probe ROWS SAMPLES".into());
    }
    println!("{}", probe(args[1].parse()?, args[2].parse()?)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mutation_and_chunk_boundaries_have_exact_states() {
        for n in [2, 499, 500, 501, 1000] {
            let value = probe(n, 1).unwrap();
            assert_eq!(
                value["samples"][0]["states"][2].as_array().unwrap().len(),
                n - 1
            );
        }
        assert!(probe(1, 1).is_err());
        assert!(probe(2, 0).is_err());
    }
}
