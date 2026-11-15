//! Concurrent readers and the async API: a reader's snapshot never changes while a
//! writer commits, many readers on threads each see one consistent generation, a reader
//! thread that panics affects no one else, a failed write leaves snapshots untouched,
//! and through `AsyncDb` reads follow writes, parallel streams agree, a stream dropped
//! half way leaves the database usable, and an aborted transaction is all or nothing.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use lin::{AsyncDb, Db};

fn count(rows: &lin::Handle) -> i64 {
    rows.rows[0].get("hits").and_then(|c| c.as_f64()).unwrap() as i64
}

fn seed(db: &mut Db) {
    db.run("col t { id: text, g: i64, n: i64 }").unwrap();
    db.run("rel link stub").unwrap();
    let rows = (0..500).map(|i| format!(r#"{{ id: "t{i}", g: {}, n: {i} }}"#, i % 5)).collect::<Vec<_>>().join(",");
    db.run(&format!("insert t [{rows}]")).unwrap();
    db.run(r#"insert docs [{ uri: "raw://c/a", title: "A", layer: "raw" }, { uri: "raw://c/b", title: "B", layer: "raw" }]"#).unwrap();
}

#[test]
fn snapshots_stay_fixed_while_a_writer_commits() {
    let mut db = Db::empty();
    seed(&mut db);
    let stop = Arc::new(AtomicBool::new(false));
    let mut readers = Vec::new();
    for r in 0..6 {
        let snap = db.reader();
        let stop = Arc::clone(&stop);
        readers.push(std::thread::spawn(move || {
            let first = count(&snap.run("t | count").unwrap());
            let sum = |s: &lin::ReadDb| s.run("t | sum n").unwrap().rows[0].get("n").and_then(|c| c.as_f64()).unwrap();
            let first_sum = sum(&snap);
            let mut rounds = 0;
            while !stop.load(Ordering::Relaxed) || rounds < 20 {
                assert_eq!(count(&snap.run("t | count").unwrap()), first, "reader {r}: count moved");
                assert_eq!(sum(&snap), first_sum, "reader {r}: sum moved");
                assert_eq!(count(&snap.run("t | g == 2 | count").unwrap()) * 5, first, "reader {r}: groups inconsistent");
                rounds += 1;
            }
            first
        }));
        // Writes between taking readers: each reader froze a different generation.
        let rows = (0..5).map(|i| format!(r#"{{ id: "w{r}-{i}", g: {i}, n: 1 }}"#)).collect::<Vec<_>>().join(",");
        db.run(&format!("insert t [{rows}]")).unwrap();
    }
    // Keep writing while the readers check their snapshots.
    for k in 0..50 {
        let rows = (0..5).map(|i| format!(r#"{{ id: "x{k}-{i}", g: {i}, n: 2 }}"#)).collect::<Vec<_>>().join(",");
        db.run(&format!("insert t [{rows}]")).unwrap();
        db.run(&format!(r#"update t[id == "t{k}"] cas each {{ n: -1 }}"#)).unwrap();
    }
    stop.store(true, Ordering::Relaxed);
    let seen: Vec<i64> = readers.into_iter().map(|h| h.join().expect("reader thread failed")).collect();
    assert_eq!(seen, (0..6).map(|r| 500 + 5 * r).collect::<Vec<_>>());
    assert_eq!(count(&db.run("t | count").unwrap()), 500 + 30 + 250);
}

#[test]
fn reader_panic_and_failed_writes_affect_no_one() {
    let mut db = Db::empty();
    seed(&mut db);
    let snap = db.reader();
    let crashing = {
        let snap = snap.clone();
        std::thread::spawn(move || {
            let _ = snap.run("t | count");
            panic!("reader thread fails");
        })
    };
    assert!(crashing.join().is_err());
    // The same snapshot and the writer still work.
    assert_eq!(count(&snap.run("t | count").unwrap()), 500);
    db.run(r#"insert t { id: "after-panic", g: 0, n: 0 }"#).unwrap();
    // A failed write changes no snapshot, old or new.
    let before = db.reader();
    assert!(db.run(r#"insert t [{ id: "ok-1", g: 1, n: 1 }, { id: "t1", g: 1, n: 1 }]"#).is_err());
    let after = db.reader();
    assert_eq!(count(&before.run("t | count").unwrap()), 501);
    assert_eq!(count(&after.run("t | count").unwrap()), 501);
    assert_eq!(count(&after.run(r#"t | id == "ok-1" | count"#).unwrap()), 0);
    // Graph edges in a snapshot are those of its moment.
    let a = db.run(r#"docs | uri == "raw://c/a" | { id }"#).unwrap().rows[0].get("id").and_then(|c| c.text()).unwrap().to_string();
    let b = db.run(r#"docs | uri == "raw://c/b" | { id }"#).unwrap().rows[0].get("id").and_then(|c| c.text()).unwrap().to_string();
    let no_edge = db.reader();
    db.run(&format!(r#"append edge link "{a}" -> "{b}""#)).unwrap();
    let with_edge = db.reader();
    let hop = |s: &lin::ReadDb| s.run(&format!(r#"docs | id == "{a}" | hop link | {{ id }} | take all"#)).unwrap().rows.len();
    assert_eq!(hop(&no_edge), 0);
    assert_eq!(hop(&with_edge), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn async_reads_streams_and_aborted_transactions() {
    use futures_util::StreamExt;
    let mut db = Db::empty();
    seed(&mut db);
    let adb = AsyncDb::new(db);
    // Read after write.
    for i in 0..20 {
        adb.execute(format!(r#"insert t {{ id: "a{i}", g: 0, n: {i} }}"#)).await.unwrap();
        let h = adb.run_read(format!(r#"t | id == "a{i}" | count"#)).await.unwrap();
        assert_eq!(count(&h), 1, "write {i} not visible to the next read");
    }
    // Parallel streams of the same query agree.
    let mut tasks = Vec::new();
    for _ in 0..6 {
        let adb = adb.clone();
        tasks.push(tokio::spawn(async move {
            let mut ids: Vec<String> = adb
                .stream("t | g == 3 | { id } | take all")
                .await
                .map(|r| r.unwrap().get("id").and_then(|c| c.text()).unwrap().to_string())
                .collect()
                .await;
            ids.sort();
            ids
        }));
    }
    let mut results = Vec::new();
    for t in tasks {
        results.push(t.await.unwrap());
    }
    assert!(results.windows(2).all(|w| w[0] == w[1]));
    assert_eq!(results[0].len(), 100);
    // A stream dropped after a few rows leaves everything usable.
    {
        let mut s = Box::pin(adb.stream("t | take all").await);
        for _ in 0..3 {
            s.next().await.unwrap().unwrap();
        }
    }
    adb.execute(r#"insert t { id: "after-drop", g: 0, n: 0 }"#).await.unwrap();
    // A transaction whose future is abandoned commits all of it or none of it.
    for k in 0..20 {
        let fut = adb.transaction(move |tx| {
            tx.run(&format!(r#"insert t {{ id: "tx{k}-a", g: 9, n: 1 }}"#))?;
            tx.run(&format!(r#"insert t {{ id: "tx{k}-b", g: 9, n: 1 }}"#))?;
            Ok(())
        });
        let _ = tokio::time::timeout(std::time::Duration::from_micros(k * 50), fut).await;
    }
    // Let any transaction still in flight finish.
    adb.execute(r#"insert t { id: "barrier", g: 0, n: 0 }"#).await.unwrap();
    for k in 0..20 {
        let n = count(&adb.run_read(format!(r#"t | id == "tx{k}-a" or id == "tx{k}-b" | count"#)).await.unwrap());
        assert!(n == 0 || n == 2, "transaction {k} half applied: {n}");
    }
    // A transaction that fails is rolled back.
    let err = adb
        .transaction(|tx| {
            tx.run(r#"insert t { id: "rolled", g: 1, n: 1 }"#)?;
            tx.run(r#"insert t { id: "t3", g: 1, n: 1 }"#)?;
            Ok(())
        })
        .await;
    assert!(err.is_err());
    assert_eq!(count(&adb.run_read(r#"t | id == "rolled" | count"#).await.unwrap()), 0);
}

#[test]
fn snapshots_keep_their_catalog() {
    let mut db = Db::empty();
    seed(&mut db);
    let old = db.reader();
    db.run("col late { id: text, v: i64 }").unwrap();
    db.run(r#"insert late { id: "l1", v: 1 }"#).unwrap();
    let new = db.reader();
    assert!(old.run("late | count").is_err(), "old snapshot sees a collection created after it");
    assert_eq!(count(&new.run("late | count").unwrap()), 1);
    assert_eq!(count(&old.run("t | count").unwrap()), 500);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn dropped_futures_and_panicking_transactions_leave_the_db_usable() {
    let mut db = Db::empty();
    seed(&mut db);
    let adb = AsyncDb::new(db);
    for k in 0..30u64 {
        let fut = adb.execute(format!(r#"insert t [{{ id: "d{k}-a", g: 8, n: 1 }}, {{ id: "d{k}-b", g: 8, n: 1 }}]"#));
        let _ = tokio::time::timeout(std::time::Duration::from_micros(k * 30), fut).await;
    }
    adb.execute(r#"insert t { id: "barrier2", g: 0, n: 0 }"#).await.unwrap();
    for k in 0..30 {
        let n = count(&adb.run_read(format!(r#"t | id == "d{k}-a" or id == "d{k}-b" | count"#)).await.unwrap());
        assert!(n == 0 || n == 2, "insert {k} half applied: {n}");
    }
    let r = adb
        .transaction(|tx| -> Result<(), lin::Error> {
            tx.run(r#"insert t { id: "panicked", g: 1, n: 1 }"#)?;
            panic!("closure fails");
        })
        .await;
    assert!(r.is_err(), "a panicking transaction must surface as an error");
    assert_eq!(count(&adb.run_read(r#"t | id == "panicked" | count"#).await.unwrap()), 0);
    adb.execute(r#"insert t { id: "after-panic-tx", g: 0, n: 0 }"#).await.unwrap();
    assert_eq!(count(&adb.run_read(r#"t | id == "after-panic-tx" | count"#).await.unwrap()), 1);
}

/// A reader snapshot keeps the vector-search answers of its moment: updates that
/// re-embed documents, deletes and inserts after the snapshot change only newer readers.
#[test]
fn snapshot_vector_search_is_isolated() {
    let mut db = Db::empty();
    let titles = ["river stones", "mountain pass", "river delta", "desert wind", "river bend"];
    for (i, t) in titles.iter().enumerate() {
        db.run(&format!(r#"insert docs {{ uri: "raw://v/{i}", title: "{t}", layer: "raw" }}"#)).unwrap();
    }
    let q = r#"docs | search vec "river" | { uri } | take all"#;
    let uris = |h: lin::Handle| -> Vec<String> { h.rows.iter().map(|r| r.get("uri").and_then(|c| c.text()).unwrap().to_string()).collect() };
    let old = db.reader();
    let old_answer = uris(old.run(q).unwrap());
    assert!(!old_answer.is_empty());
    db.run(r#"update docs[uri == "raw://v/0"] cas each { title: "mountain stones" }"#).unwrap();
    db.run(r#"delete docs[uri == "raw://v/2"] cas each"#).unwrap();
    db.run(r#"insert docs { uri: "raw://v/9", title: "river river", layer: "raw" }"#).unwrap();
    let new_answer = uris(db.reader().run(q).unwrap());
    assert_ne!(new_answer, old_answer, "the changes did not affect a new reader");
    for _ in 0..3 {
        assert_eq!(uris(old.run(q).unwrap()), old_answer, "old snapshot's vector answer changed");
    }
}
