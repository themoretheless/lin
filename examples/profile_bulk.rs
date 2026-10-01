//! Sampling workload; not a replacement for fresh-fixture native benchmarks.
use lin::Db;
use std::time::{Duration, Instant};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let n = 10_000;
    let rows = (0..n).map(|i| format!(r#"{{id: "b-{i}", uri: "bulk://{i}", title: "doc {i} {}", body: "body {i}", wing: "{}", layer: "wiki", ts: ago {}d}}"#,
        if i%10==0 {"wal"} else {"plain"}, if i%2==0 {"rag"} else {"sys"}, if i%2==0 {1} else {30})).collect::<Vec<_>>();
    let source = format!("insert docs [{}]", rows.join(","));
    let prepared = Db::empty().prepare(&source)?;
    eprintln!("PROFILE_READY pid={}", std::process::id());
    let start = Instant::now();
    let mut batches = 0;
    while start.elapsed() < Duration::from_secs(20) {
        let mut db = Db::empty();
        db.run("index docs [wing, ts]")?;
        assert_eq!(db.run_prepared(&prepared)?.done.n, n);
        assert_eq!(db.store.collection("docs").len(), n);
        batches += 1;
    }
    eprintln!(
        "PROFILE_DONE batches={batches} elapsed={:.3}",
        start.elapsed().as_secs_f64()
    );
    Ok(())
}
