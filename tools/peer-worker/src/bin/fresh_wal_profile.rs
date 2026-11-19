// Persistent worker for scripts/bench-peers.py. Timers exclude JSON IPC/setup.
use lin::{Cell, Db};
use serde_json::{Value, json};
use std::hint::black_box;
use std::time::Instant;

fn canonical(rows: &[lin::Row], fields: &[String]) -> Value {
    Value::Array(
        rows.iter()
            .map(|row| {
                Value::Array(
                    fields
                        .iter()
                        .map(|field| {
                            let cell = if field == "$count" {
                                row.values().next()
                            } else {
                                row.get(field)
                            };
                            match cell {
                                Some(Cell::Text(v)) => json!(v.as_ref()),
                                Some(Cell::Int(v)) | Some(Cell::Time(v)) => json!(v),
                                Some(Cell::Float(v)) => json!(v),
                                Some(Cell::Bool(v)) => json!(v),
                                _ => Value::Null,
                            }
                        })
                        .collect(),
                )
            })
            .collect(),
    )
}

fn measure_insert(
    n: usize,
    samples: usize, path: &std::path::Path,
    timestamp: i64,
) -> Result<Value, Box<dyn std::error::Error>> {
    if n == 0 || samples == 0 {
        return Err("empty insert measurement".into());
    }
    let mut records = Vec::with_capacity(n);
    let mut expected = Vec::with_capacity(n);
    for i in 0..n {
        let id = format!("d-{i}");
        let uri = format!("bench://{i}");
        let wing = if i % 2 == 0 { "rag" } else { "sys" };
        let title = if i % 10 == 0 {
            format!("doc {i} wal note")
        } else {
            format!("doc {i} plain")
        };
        let body = format!("body {i}");
        records.push(format!(
            r#"{{id:{},uri:{},wing:{},title:{},body:{},layer:"wiki",ts:timestamp({timestamp})}}"#,
            json!(id),
            json!(uri),
            json!(wing),
            json!(title),
            json!(body)
        ));
        expected.push(json!([id, uri, wing, title, timestamp, body]));
    }
    // Respect the per-statement AST quota without relaxing production limits.
    // Large batches use the public memory transaction API with bounded statements.
    let sources = records
        .chunks(10_000)
        .map(|chunk| format!("insert docs [{}]", chunk.join(",")))
        .collect::<Vec<_>>();
    let fields = ["id", "uri", "wing", "title", "ts", "body"].map(String::from);
    let mut timings = Vec::with_capacity(samples);
    let mut result = Value::Null;
    for _ in 0..samples {
        let mut db = Db::open_with(path, lin::OpenOpts {sync: lin::SyncMode::Full, cold: false})?;
        db.run("index docs [wing, ts]")?;
        let prepared = sources
            .iter()
            .map(|source| db.prepare(source))
            .collect::<Result<Vec<_>, _>>()?;
        let start = Instant::now();
        let affected = if prepared.len() == 1 {
            black_box(prepared[0].run(&mut db)?).done.n
        } else {
            let mut tx = db.transaction()?;
            let mut affected = 0;
            for statement in &prepared {
                affected += black_box(tx.run_prepared(statement)?).done.n;
            }
            tx.commit()?;
            affected
        };
        timings.push(start.elapsed().as_nanos() as f64);
        if affected != n {
            return Err("insert affected count mismatch".into());
        }
        drop(db);
        let db = Db::open_with(path, lin::OpenOpts {sync: lin::SyncMode::Full, cold: false})?;
        let rows = db.store.collection("docs");
        if rows.len() != n
            || rows
                .iter()
                .any(|r| r.get("embedding").and_then(Cell::as_vec).is_none())
        {
            return Err("insert rows or default embeddings missing".into());
        }
        result = canonical(rows, &fields);
        if result != Value::Array(expected.clone()) {
            return Err("insert exact values mismatch".into());
        }
    }
    Ok(json!({"samples_ns":timings,"result":result,
        "statement_rows_limit":10_000,"statements":sources.len(),
        "api":if sources.len()==1 {"prepared run"} else {"prepared chunks in one memory transaction"}}))
}


fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().collect::<Vec<_>>();
    let result = measure_insert(args[1].parse()?, 1, std::path::Path::new(&args[2]), 123456)?;
    println!("{}", result["samples_ns"][0]);
    Ok(())
}
