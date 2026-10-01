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
                                Some(Cell::Int(v)) => json!(v),
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

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let stdin = io::stdin();
    let mut out = io::stdout().lock();
    let mut db = None;
    for line in stdin.lock().lines() {
        let request: Value = serde_json::from_str(&line?)?;
        if let Some(n) = request.get("rows").and_then(Value::as_u64) {
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
