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
                                Some(cell) if let Some(text) = cell.text() => json!(text),
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

#[cfg(target_os = "linux")]
fn process_usage() -> Result<libc::rusage, std::io::Error> {
    let mut usage = std::mem::MaybeUninit::<libc::rusage>::uninit();
    // getrusage initializes the output record on success.
    if unsafe { libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr()) } == 0 {
        Ok(unsafe { usage.assume_init() })
    } else {
        Err(std::io::Error::last_os_error())
    }
}

fn measure_insert(
    n: usize,
    samples: usize,
    timestamp: i64,
    durable: Option<&std::path::Path>,
    wal_dictionary: bool,
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
    let mut open_ns = Vec::new();
    let mut reopen_ns = Vec::new();
    let mut wal_bytes = Vec::new();
    let mut embedding_hash = None;
    #[allow(unused_mut)]
    let mut usage = Vec::<Value>::new();
    #[cfg(target_os = "linux")]
    let profile_usage = std::env::var_os("LIN_BENCH_INSERT_RUSAGE").is_some();
    let profile_stages = std::env::var_os("LIN_BENCH_INSERT_STAGES").is_some();
    let mut stages = Vec::new();
    for _ in 0..samples {
        let open = Instant::now();
        let mut db = match durable {
            Some(path) => Db::open_with(path, lin::OpenOpts { sync: lin::SyncMode::Full, cold: false })?,
            None => Db::empty(),
        };
        open_ns.push(open.elapsed().as_nanos() as u64);
        if durable.is_some() && wal_dictionary {
            db.set_wal_dictionary(Some(true));
            assert!(db.wal_dictionary_enabled());
        }
        db.run("index docs [wing, ts]")?;
        let prepared = sources
            .iter()
            .map(|source| db.prepare(source))
            .collect::<Result<Vec<_>, _>>()?;
        #[cfg(target_os = "linux")]
        let before = profile_usage.then(process_usage).transpose()?;
        #[cfg(target_os = "linux")]
        let record_pages = |phase: &str| -> std::io::Result<()> {
            if std::env::var_os("LIN_BENCH_INSERT_SMAPS").is_some() {
                let rollup = std::fs::read_to_string("/proc/self/smaps_rollup")?;
                for line in rollup.lines().filter(|line| {
                    line.starts_with("AnonHugePages:") || line.starts_with("Anonymous:")
                }) {
                    eprintln!("insert_pages {phase} {line}");
                }
            }
            Ok(())
        };
        #[cfg(target_os = "linux")]
        record_pages("before")?;
        let start = Instant::now();
        let affected = if prepared.len() == 1 {
            black_box(prepared[0].run(&mut db)?).done.n
        } else {
            let mut boundaries = Vec::with_capacity(if profile_stages { prepared.len() + 2 } else { 0 });
            let mut tx = db.transaction()?;
            if profile_stages { boundaries.push(start.elapsed().as_nanos() as u64); }
            let mut affected = 0;
            for statement in &prepared {
                affected += black_box(tx.run_prepared(statement)?).done.n;
                if profile_stages { boundaries.push(start.elapsed().as_nanos() as u64); }
            }
            tx.commit()?;
            if profile_stages {
                boundaries.push(start.elapsed().as_nanos() as u64);
                stages.push(boundaries);
            }
            affected
        };
        timings.push(start.elapsed().as_nanos() as f64);
        #[cfg(target_os = "linux")]
        if let Some(before) = before {
            let after = process_usage()?;
            let micros = |t: libc::timeval| t.tv_sec * 1_000_000 + t.tv_usec;
            usage.push(json!({
                "minor_faults": after.ru_minflt - before.ru_minflt,
                "major_faults": after.ru_majflt - before.ru_majflt,
                "user_us": micros(after.ru_utime) - micros(before.ru_utime),
                "system_us": micros(after.ru_stime) - micros(before.ru_stime),
                "voluntary_switches": after.ru_nvcsw - before.ru_nvcsw,
                "involuntary_switches": after.ru_nivcsw - before.ru_nivcsw,
                "peak_rss_kib": after.ru_maxrss,
            }));
        }

        #[cfg(target_os = "linux")]
        record_pages("after")?;

        if affected != n {
            return Err("insert affected count mismatch".into());
        }
        if let Some(path) = durable {
            let recovery_dir = path.join("recovery");
            std::fs::create_dir(&recovery_dir)?;
            let log = lin::persist::log_path(path);
            wal_bytes.push(log.metadata()?.len());
            std::fs::copy(&log, lin::persist::log_path(&recovery_dir))?;
            let head = lin::persist::head_path(path);
            if head.exists() {
                std::fs::copy(&head, lin::persist::head_path(&recovery_dir))?;
            }
            let snapshot = lin::persist::snapshot_path(path);
            if snapshot.exists() {
                std::fs::copy(&snapshot, lin::persist::snapshot_path(&recovery_dir))?;
            }
            let reopen = Instant::now();
            let recovered = Db::open(&recovery_dir)?;
            reopen_ns.push(reopen.elapsed().as_nanos() as u64);
            let recovered_rows = recovered.store.collection("docs");
            if canonical(recovered_rows, &fields) != Value::Array(expected.clone()) {
                return Err("WAL recovery exact values mismatch".into());
            }
            use std::hash::Hasher;
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            for row in recovered_rows {
                let vector = row.get("embedding").and_then(Cell::as_vec).ok_or("recovered embedding missing")?;
                hasher.write_usize(vector.len());
                for value in vector {
                    hasher.write_u32(value.to_bits());
                }
            }
            embedding_hash = Some(hasher.finish());
            drop(recovered);
            std::fs::remove_dir_all(recovery_dir)?;
            db.close()?;
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
        result = canonical(rows, &fields);
        if result != Value::Array(expected.clone()) {
            return Err("insert exact values mismatch".into());
        }
    }
    Ok(json!({"samples_ns":timings,"result":result, "insert_usage": usage, "stage_end_ns":stages, "reopen_ns":reopen_ns, "wal_bytes":wal_bytes, "open_ns":open_ns, "embedding_hash":embedding_hash,
        "statement_rows_limit":10_000,"statements":sources.len(),
        "api":if sources.len()==1 {"prepared run"} else {"prepared chunks in one memory transaction"}}))
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
                measure_insert(n as usize, samples as usize, timestamp, durable, request["wal_dictionary"].as_bool().unwrap_or(false))?
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
