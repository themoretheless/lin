//! Sampling workload for durable inserts, not a peer benchmark.
use lin::Db;
use std::time::{Duration, Instant};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let n = 10_000usize;
    let rows = (0..n).map(|i| format!(r#"{{id:"d-{i}",uri:"bench://{i}",title:"doc {i} {}",layer:"wiki",wing:"{}",body:"body {i}",ts:timestamp({})}}"#,
        if i%10==0 {"wal note"} else {"plain"}, if i%2==0 {"rag"} else {"sys"},
        1_790_000_000_000i64 - if i%2==0 {86_400_000} else {30*86_400_000})).collect::<Vec<_>>();
    let prepared = Db::empty().prepare(&format!("insert docs [{}]", rows.join(",")))?;
    let root = std::env::temp_dir().join(format!("lin-profile-durable-{}", std::process::id()));
    std::fs::create_dir(&root)?;
    eprintln!("PROFILE_READY pid={}", std::process::id());
    let start = Instant::now();
    let mut batches = 0usize;
    let mut insert_time = Duration::ZERO;
    let mut setup_time = Duration::ZERO;
    while start.elapsed() < Duration::from_secs(25) {
        let dir = root.join(batches.to_string());
        let setup = Instant::now();
        let mut db = Db::open(&dir)?;
        db.run("index docs [wing, ts]")?;
        setup_time += setup.elapsed();
        let timer = Instant::now();
        let result = prepared.run(&mut db)?;
        insert_time += timer.elapsed();
        assert_eq!(result.done.n, n);
        assert_eq!(db.store.collection("docs").len(), n);
        drop(db);
        std::fs::remove_dir_all(&dir)?;
        batches += 1;
    }
    std::fs::remove_dir(&root)?;
    eprintln!(
        "PROFILE_DONE batches={batches} insert_ms={:.3} setup_ms={:.3} total_ms={:.3}",
        insert_time.as_secs_f64() * 1000.0,
        setup_time.as_secs_f64() * 1000.0,
        start.elapsed().as_secs_f64() * 1000.0
    );
    Ok(())
}
