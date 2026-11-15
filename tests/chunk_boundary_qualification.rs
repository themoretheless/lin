//! Above the parallel threshold (65 536 rows, chunks of 16 384) joins, sorts with
//! skip/take across chunk boundaries, grouped counts and cursors give exactly the answers
//! computed here from the inserted rows.

use lin::Db;

const N: usize = 70_000;

#[test]
fn large_inputs_cross_chunk_boundaries_exactly() {
    let mut db = Db::empty();
    db.run("col p { id: text, w: i64 }").unwrap();
    db.run("col c { id: text, pid: text, k: i64 }").unwrap();
    db.run("fk c.pid -> p.id").unwrap();
    db.run(r#"insert p [{ id: "p0", w: 0 }, { id: "p1", w: 1 }, { id: "p2", w: 2 }]"#).unwrap();
    for chunk in 0..(N / 10_000) {
        let rows = (chunk * 10_000..(chunk + 1) * 10_000).map(|i| format!(r#"{{ id: "c{i:06}", pid: "p{}", k: {} }}"#, i % 3, (i * 7919) % N)).collect::<Vec<_>>().join(",");
        db.run(&format!("insert c [{rows}]")).unwrap();
    }
    let n = |db: &mut Db, q: &str| db.run(q).unwrap().rows[0].get("hits").and_then(|c| c.as_f64()).unwrap() as usize;
    assert_eq!(n(&mut db, "c | join p on pid | count"), N);
    assert_eq!(n(&mut db, "c | join p on pid | p.w == 1 | count"), (0..N).filter(|i| i % 3 == 1).count());
    // k is a permutation of 0..N: sorted by k, row r has k == r.
    for (skip, take) in [(16_380usize, 10usize), (32_765, 6), (65_530, 12), (N - 3, 10)] {
        let rows = db.run(&format!("c | sort k | skip {skip} | take {take} | {{ k }}")).unwrap().rows.clone();
        let got: Vec<usize> = rows.iter().map(|r| r.get("k").and_then(|c| c.as_f64()).unwrap() as usize).collect();
        let want: Vec<usize> = (skip..(skip + take).min(N)).collect();
        assert_eq!(got, want, "sort skip {skip} take {take}");
    }
    let by = db.run("c | count by pid").unwrap().rows.clone();
    for r in &by {
        let pid = r.get("pid").and_then(|c| c.text()).unwrap();
        let g: usize = pid[1..].parse().unwrap();
        let hits = r.get("hits").and_then(|c| c.as_f64()).unwrap() as usize;
        assert_eq!(hits, (0..N).filter(|i| i % 3 == g).count(), "count by {pid}");
    }
    let lin::Stmt::Query(q) = lin::parse("c | k < 20000 | take all").unwrap() else { unreachable!() };
    assert_eq!(db.cursor(&q).unwrap().count(), 20_000, "cursor across chunks");
}
