// Persistent worker for scripts/bench-peers.py. Timers exclude JSON IPC/setup.
use lin::{Cell, Db};
use serde_json::{Value, json};
use std::hint::black_box;
use std::io::{self, BufRead, Write};
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
                                Some(Cell::SmallText(v)) => json!(v.as_str()),
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

fn seed(n: usize) -> Result<Db, lin::Error> {
    let mut db = Db::empty();
    db.run("index docs [wing]")?;
    for start in (0..n).step_by(500) {
        let records = (start..(start+500).min(n)).map(|i| {
            let title = if i % 10 == 0 { format!("doc {i} wal note") } else { format!("doc {i} plain") };
            format!(r#"{{ id: "d-{i}", uri: "bench://{i}", wing: "{}", title: {}, body: "body {i}" }}"#,
                if i % 2 == 0 { "rag" } else { "sys" }, json!(title))
        }).collect::<Vec<_>>();
        db.run(&format!("insert docs [{}]", records.join(",")))?;
    }
    let users = (n / 10).max(1);
    for start in (0..users).step_by(500) {
        let records = (start..(start + 500).min(users))
            .map(|i| format!(r#"{{ id: "u-{i}", email: "u{i}@bench.local" }}"#))
            .collect::<Vec<_>>();
        db.run(&format!("insert users [{}]", records.join(",")))?;
    }
    for start in (0..n).step_by(500) {
        let records = (start..(start + 500).min(n))
            .map(|i| {
                format!(
                    r#"{{ id: "o-{i}", user_id: "u-{}", total: {} }}"#,
                    i % users,
                    i % 200
                )
            })
            .collect::<Vec<_>>();
        db.run(&format!("insert orders [{}]", records.join(",")))?;
    }
    Ok(db)
}

fn measure_insert(
    n: usize,
    samples: usize,
    timestamp: i64,
    durable: Option<&std::path::Path>,
    column_batch: bool,
) -> Result<Value, Box<dyn std::error::Error>> {
    if n == 0 || samples == 0 {
        return Err("empty insert measurement".into());
    }
    let mut records = Vec::with_capacity(n);
    let fields = ["id", "uri", "wing", "title", "ts", "body"].map(String::from);
    let mut batch = column_batch.then(|| {
        let mut names = fields.to_vec();
        names.push("layer".to_string());
        lin::RecordBatch::with_capacity(names, n)
    });
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
        if let Some(batch) = batch.as_mut() {
            batch.push_cells([
                Cell::Text(id.as_str().into()), Cell::Text(uri.as_str().into()),
                Cell::Text(wing.into()), Cell::Text(title.as_str().into()),
                Cell::Time(timestamp), Cell::Text(body.as_str().into()), Cell::Text("wiki".into()),
            ]);
        } else {
            records.push(format!(
            r#"{{id:{},uri:{},wing:{},title:{},body:{},layer:"wiki",ts:timestamp({timestamp})}}"#,
            json!(id),
            json!(uri),
            json!(wing),
            json!(title),
            json!(body)
        ));
        }
        expected.push(json!([id, uri, wing, title, timestamp, body]));
    }
    // Respect the per-statement AST quota without relaxing production limits.
    // Large batches use the public memory transaction API with bounded statements.
    let sources = records
        .chunks(10_000)
        .map(|chunk| format!("insert docs [{}]", chunk.join(",")))
        .collect::<Vec<_>>();
    let mut timings = Vec::with_capacity(samples);
    let mut result = Value::Null;
    let mut embedding_hash = None;
    for _ in 0..samples {
        let mut db = match durable {
            Some(path) => Db::open_with(path, lin::OpenOpts { sync: lin::SyncMode::Full, cold: false })?,
            None => Db::empty(),
        };
        db.run("index docs [wing, ts]")?;
        let prepared = sources
            .iter()
            .map(|source| db.prepare(source))
            .collect::<Result<Vec<_>, _>>()?;
        let start = Instant::now();
        let affected = if let Some(batch) = &batch {
            black_box(db.insert_batch("docs", batch)?).n
        } else if prepared.len() == 1 {
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
        if let Some(path) = durable {
            drop(db);
            db = Db::open(path)?;
        }
        let rows = db.store.collection("docs");
        if rows.len() != n
            || rows
                .iter()
                .any(|r| r.get("embedding").and_then(Cell::as_vec).is_none())
        {
            return Err("insert rows or default embeddings missing".into());
        }
        if column_batch {
            use std::hash::{Hash, Hasher};
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            for row in rows {
                row.get("hash").and_then(Cell::text).hash(&mut hasher);
                for value in row.get("embedding").and_then(Cell::as_vec).unwrap() {
                    value.to_bits().hash(&mut hasher);
                }
            }
            embedding_hash = Some(hasher.finish());
        }
        result = canonical(rows, &fields);
        if result != Value::Array(expected.clone()) {
            return Err("insert exact values mismatch".into());
        }
    }
    Ok(json!({"samples_ns":timings,"result":result,"embedding_hash":embedding_hash,
        "statement_rows_limit":10_000,"statements":sources.len(),
        "api":if column_batch {"column batch"} else if sources.len()==1 {"prepared run"} else {"prepared chunks in one memory transaction"}}))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let stdin = io::stdin();
    let mut out = io::stdout().lock();
    let mut db = None;
    for line in stdin.lock().lines() {
        let request: Value = serde_json::from_str(&line?)?;
        if let Some(n) = request.get("insert_rows").and_then(Value::as_u64) {
            let samples = request["samples"].as_u64().ok_or("samples missing")?;
            let timestamp = request["timestamp_ms"]
                .as_i64()
                .ok_or("timestamp missing")?;
            let durable = request.get("durable_dir").and_then(Value::as_str).map(std::path::Path::new);
            if durable.is_some() && (samples != 1 || n > 10_000) {
                return Err("durable INSERT needs one sample of at most 10,000 rows per fresh directory".into());
            }
            writeln!(
                out,
                "{}",
                measure_insert(n as usize, samples as usize, timestamp, durable, request["column_batch"].as_bool().unwrap_or(false))?
            )?;
        } else if request.get("worker_version").and_then(Value::as_bool) == Some(true) {
            writeln!(out, "{}", json!({"version":lin::VERSION}))?;
        } else if let Some(n) = request.get("rows").and_then(Value::as_u64) {
            db = Some(seed(n as usize)?);
            writeln!(out, "{}", json!({"ready": true, "version": lin::VERSION}))?;
        } else {
            let db = db.as_mut().ok_or("worker not initialized")?;
            let query = db.prepare(request["source"].as_str().ok_or("source missing")?)?;
            let iterations = request["iterations"].as_u64().ok_or("iterations missing")?;
            let samples = request["samples"].as_u64().ok_or("samples missing")?;
            if iterations == 0 || samples == 0 {
                return Err("empty measurement".into());
            }
            let fields: Vec<String> = serde_json::from_value(request["fields"].clone())?;
            let before = canonical(&query.run(db)?.rows, &fields);
            for _ in 0..3 {
                black_box(query.run(db)?);
            }
            let mut timings = Vec::new();
            for _ in 0..samples {
                let start = Instant::now();
                for _ in 0..iterations {
                    black_box(query.run(db)?);
                }
                timings.push(start.elapsed().as_nanos() as f64 / iterations as f64);
            }
            let after = canonical(&query.run(db)?.rows, &fields);
            if before != after {
                return Err("result changed during measurement".into());
            }
            writeln!(out, "{}", json!({"samples_ns": timings, "result": after}))?;
        }
        out.flush()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hundred_thousand_native_rows_respect_default_statement_quotas() {
        // A single AST for this fixture exceeds the default 8 MiB quota.
        // Exact six-field readback and default embeddings are checked by measure_insert.
        let measured = measure_insert(100_000, 1, 123456, None, false).unwrap();
        assert_eq!(measured["statements"], 10);
        assert_eq!(measured["samples_ns"].as_array().unwrap().len(), 1);
        assert_eq!(measured["result"].as_array().unwrap().len(), 100_000);
        assert_eq!(measured["result"][99_999][0], "d-99999");
        assert_eq!(measured["result"][99_999][4], 123456);
    }
}
