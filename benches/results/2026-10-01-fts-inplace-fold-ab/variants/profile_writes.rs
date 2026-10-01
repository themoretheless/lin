//! Diagnostic workload for sampling prepared writes; not a peer benchmark.
//! cargo run --release --example profile_writes -- --rows 10000 --seconds 15
use lin::Db;
use std::hint::black_box;
use std::time::{Duration, Instant};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let argument = |name: &str, default: usize| -> usize {
        args.windows(2)
            .find(|a| a[0] == name)
            .and_then(|a| a[1].parse().ok())
            .unwrap_or(default)
    };
    let n = argument("--rows", 10_000).max(2);
    let duration = Duration::from_secs(argument("--seconds", 15) as u64);
    let mut db = Db::empty();
    db.run("index docs [wing, ts]")?;
    let record = |i| {
        format!(
            r#"{{id: "p-{i}", uri: "profile://{i}", title: "profile wal", body: "profile body", wing: "rag", ts: now - {i}s}}"#
        )
    };
    for start in (0..n).step_by(500) {
        let records = (start..(start + 500).min(n))
            .map(&record)
            .collect::<Vec<_>>();
        db.run(&format!("insert docs [{}]", records.join(",")))?;
    }
    let hash = db.run(r#"docs | id == "p-0" | {hash}"#)?.rows[0]["hash"]
        .text()
        .unwrap()
        .to_owned();
    let operations = (0..n)
        .map(|i| {
            Ok((
                db.prepare(&format!(r#"delete docs[id == "p-{i}"] cas "{hash}""#))?,
                db.prepare(&format!("insert docs [{}]", record(i)))?,
            ))
        })
        .collect::<Result<Vec<_>, lin::Error>>()?;
    eprintln!("PROFILE_READY pid={} rows={n}", std::process::id());
    let start = Instant::now();
    let mut pairs = 0usize;
    while start.elapsed() < duration {
        let (delete, insert) = &operations[pairs % n];
        assert_eq!(black_box(db.run_prepared(delete)?).done.n, 1);
        assert_eq!(black_box(db.run_prepared(insert)?).done.n, 1);
        pairs += 1;
    }
    let elapsed = start.elapsed();
    assert_eq!(db.store.collection("docs").len(), n);
    for i in 0..n {
        assert!(db.store.get_by_id("docs", &format!("p-{i}")).is_some());
        assert!(db.store.get_by_uri(&format!("profile://{i}")).is_some());
    }
    let hits = db.run(r#"docs | search lex "profile" | {id} | take all"#)?;
    let ids = hits
        .rows
        .iter()
        .map(|row| row["id"].text().unwrap().to_owned())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(ids, (0..n).map(|i| format!("p-{i}")).collect());
    assert_eq!(hits.rows.len(), n);
    eprintln!(
        "PROFILE_DONE pairs={pairs} elapsed_seconds={:.3}",
        elapsed.as_secs_f64()
    );
    Ok(())
}
