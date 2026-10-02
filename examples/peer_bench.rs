//! Persistent worker for scripts/bench-peers.py. Timers exclude JSON IPC/setup.
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
    let source = format!("insert docs [{}]", records.join(","));
    let fields = ["id", "uri", "wing", "title", "ts", "body"].map(String::from);
    let mut timings = Vec::with_capacity(samples);
    let mut result = Value::Null;
    for _ in 0..samples {
        let mut db = Db::empty();
        db.run("index docs [wing, ts]")?;
        let prepared = db.prepare(&source)?;
        let start = Instant::now();
        let affected = black_box(prepared.run(&mut db)?).done.n;
        timings.push(start.elapsed().as_nanos() as f64);
        if affected != n {
            return Err("insert affected count mismatch".into());
        }
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
    Ok(json!({"samples_ns":timings,"result":result}))
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
            writeln!(
                out,
                "{}",
                measure_insert(n as usize, samples as usize, timestamp)?
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
