//! Float aggregates are deterministic: `sum` and `avg` over the same rows give the same
//! bits on every run and through every path (sequential, parallel chunks above the
//! parallel threshold, grouped, through an index), and agree with a sequential sum in row
//! order within one rounding per chunk. Integer columns stay exact whatever the path.

use lin::Db;

fn seed(n: usize) -> (Db, Vec<f64>) {
    let mut db = Db::empty();
    db.run("col t { id: text, f: f64, g: i64 }").unwrap();
    db.run("index t [g]").unwrap();
    let vals: Vec<f64> = (0..n).map(|i| ((i * 7919) % 1000) as f64 * 0.1 + 1e-3 * (i % 13) as f64).collect();
    for chunk in vals.chunks(5000).enumerate() {
        let (c, vs) = chunk;
        let rows = vs.iter().enumerate().map(|(j, v)| format!(r#"{{ id: "r{}", f: {v:?}, g: {} }}"#, c * 5000 + j, (c * 5000 + j) % 4)).collect::<Vec<_>>().join(",");
        db.run(&format!("insert t [{rows}]")).unwrap();
    }
    (db, vals)
}

fn agg(db: &mut Db, q: &str, f: &str) -> f64 {
    db.run(q).unwrap().rows[0].get(f).and_then(|c| c.as_f64()).unwrap()
}

#[test]
fn float_sums_are_bit_stable_and_close_to_sequential() {
    for n in [1000usize, 70_000] {
        let (mut db, vals) = seed(n);
        let seq: f64 = vals.iter().sum();
        let first = agg(&mut db, "t | sum f", "f");
        for _ in 0..10 {
            assert_eq!(agg(&mut db, "t | sum f", "f").to_bits(), first.to_bits(), "n={n}: sum changed between runs");
        }
        assert!((first - seq).abs() <= seq.abs() * 1e-12, "n={n}: {first} vs sequential {seq}");
        let avg = agg(&mut db, "t | avg f", "f");
        for _ in 0..5 {
            assert_eq!(agg(&mut db, "t | avg f", "f").to_bits(), avg.to_bits(), "n={n}: avg changed between runs");
        }
        assert!((avg - seq / n as f64).abs() <= 1e-9, "n={n}: avg {avg}");
        // Per group: the filtered (index) path and the grouped path agree with each other
        // and with the sequential per-group sum.
        let grouped = db.run("t | sum f by g").unwrap().rows.clone();
        for g in 0..4 {
            let want: f64 = vals.iter().enumerate().filter(|(i, _)| i % 4 == g).map(|(_, v)| v).sum();
            let filtered = agg(&mut db, &format!("t | g == {g} | sum f"), "f");
            let by = grouped.iter().find(|r| r.get("g").and_then(|c| c.as_f64()) == Some(g as f64)).and_then(|r| r.get("f")).and_then(|c| c.as_f64()).unwrap();
            assert!((filtered - want).abs() <= want.abs() * 1e-12, "n={n} g={g}: filtered {filtered} vs {want}");
            assert!((by - want).abs() <= want.abs() * 1e-12, "n={n} g={g}: grouped {by} vs {want}");
        }
        let gsum = agg(&mut db, "t | sum g", "g");
        assert_eq!(gsum, (0..n).map(|i| (i % 4) as f64).sum::<f64>(), "n={n}: integer sum");
    }
}

/// A small value between two large values that cancel survives `sum`, through every
/// path: plain, filtered through an index, and grouped.
#[test]
fn cancelling_float_sums_keep_small_terms() {
    let mut db = Db::empty();
    db.run("col c { id: text, f: f64, g: i64 }").unwrap();
    db.run("index c [g]").unwrap();
    db.run(r#"insert c [{ id: "a", f: 10000000000000000.0, g: 1 }, { id: "b", f: 1.0, g: 1 }, { id: "c", f: -10000000000000000.0, g: 1 }]"#).unwrap();
    assert_eq!(agg(&mut db, "c | sum f", "f"), 1.0);
    assert_eq!(agg(&mut db, "c | g == 1 | sum f", "f"), 1.0);
    let by = db.run("c | sum f by g").unwrap().rows[0].get("f").and_then(|c| c.as_f64()).unwrap();
    assert_eq!(by, 1.0);
    assert!((agg(&mut db, "c | avg f", "f") - 1.0 / 3.0).abs() < 1e-12);
}
