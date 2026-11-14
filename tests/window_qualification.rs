//! Window functions against oracles computed here: rank and dense_rank over ties,
//! frames under descending order, and running sums of integers near the i64 limit.

use lin::{Cell, Db};

fn ints(db: &mut Db, q: &str) -> Vec<(String, Option<i64>)> {
    let mut out: Vec<(String, Option<i64>)> = db
        .run(q)
        .unwrap_or_else(|e| panic!("{q}: {e}"))
        .rows
        .iter()
        .map(|r| {
            let id = r.get("id").and_then(|c| c.text()).unwrap().to_string();
            let w = match r.get("w") {
                Some(Cell::Int(n)) => Some(*n),
                Some(Cell::Float(f)) => Some(*f as i64),
                _ => None,
            };
            (id, w)
        })
        .collect();
    out.sort();
    out
}

#[test]
fn rank_and_dense_rank_over_ties() {
    let mut db = Db::empty();
    db.run("col w { id: text, g: text, k: i64 }").unwrap();
    // Keys with long runs of ties inside each partition.
    let keys: Vec<(String, String, i64)> = (0..120)
        .map(|i| (format!("r{i:03}"), format!("g{}", i % 3), [5i64, 5, 7, 7, 7, 9, 1, 1][i % 8]))
        .collect();
    let src = keys
        .iter()
        .map(|(id, g, k)| format!(r#"{{ id: "{id}", g: "{g}", k: {k} }}"#))
        .collect::<Vec<_>>()
        .join(",");
    db.run(&format!("insert w [{src}]")).unwrap();
    for desc in [false, true] {
        let dir = if desc { "desc" } else { "asc" };
        let mut want_rank = Vec::new();
        let mut want_dense = Vec::new();
        for g in ["g0", "g1", "g2"] {
            let mut part: Vec<&(String, String, i64)> = keys.iter().filter(|(_, pg, _)| pg == g).collect();
            part.sort_by_key(|(_, _, k)| if desc { -k } else { *k });
            for (id, _, k) in &part {
                let rank = 1 + part.iter().filter(|(_, _, o)| if desc { o > k } else { o < k }).count() as i64;
                let mut lower: Vec<i64> = part
                    .iter()
                    .map(|(_, _, o)| *o)
                    .filter(|o| if desc { o > k } else { o < k })
                    .collect();
                lower.sort();
                lower.dedup();
                want_rank.push((id.clone(), Some(rank)));
                want_dense.push((id.clone(), Some(1 + lower.len() as i64)));
            }
        }
        want_rank.sort();
        want_dense.sort();
        let q = |f: &str| format!("w | window {f} by g sort k {dir} as w | pick {{ id, w }} | take all");
        assert_eq!(ints(&mut db, &q("rank")), want_rank, "rank {dir}");
        assert_eq!(ints(&mut db, &q("dense_rank")), want_dense, "dense_rank {dir}");
        // The row pipeline (no projection) agrees.
        let rows = format!("w | window rank by g sort k {dir} as w | take all");
        assert_eq!(ints(&mut db, &rows), want_rank, "rank {dir}, row pipeline");
    }
}

#[test]
fn frames_under_descending_order_and_large_sums() {
    let mut db = Db::empty();
    db.run("col f { id: text, k: i64, v: i64 }").unwrap();
    let vals: Vec<(String, i64, i64)> = (0..40)
        .map(|i| (format!("f{i:02}"), i as i64, (i as i64 * 37) % 23 - 11))
        .collect();
    let src = vals.iter().map(|(id, k, v)| format!(r#"{{ id: "{id}", k: {k}, v: {v} }}"#)).collect::<Vec<_>>().join(",");
    db.run(&format!("insert f [{src}]")).unwrap();
    // Descending by k: "1 preceding" is the row with the next higher k.
    let mut desc: Vec<&(String, i64, i64)> = vals.iter().collect();
    desc.sort_by_key(|(_, k, _)| -k);
    let mut want: Vec<(String, Option<i64>)> = desc
        .iter()
        .enumerate()
        .map(|(i, (id, _, _))| {
            let lo = i.saturating_sub(1);
            let hi = (i + 2).min(desc.len() - 1);
            (id.clone(), Some(desc[lo..=hi].iter().map(|(_, _, v)| *v).sum()))
        })
        .collect();
    want.sort();
    let got = ints(&mut db, "f | window sum v sort k desc rows between 1 preceding and 2 following as w | pick { id, w } | take all");
    assert_eq!(got, want);

    // Running sums near the i64 limit: no panic, no wrap-around to a small or negative
    // number.
    db.run("col big { id: text, k: i64, v: i64 }").unwrap();
    db.run(r#"insert big [{ id: "a", k: 1, v: 9223372036854775807 }, { id: "b", k: 2, v: 9223372036854775807 }, { id: "c", k: 3, v: -9223372036854775808 }]"#).unwrap();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        db.run("big | window sum v sort k asc rows between unbounded preceding and current row as w | pick { id, w } | take all")
            .map(|h| h.rows.clone())
    }));
    let rows = result.expect("window sum panicked").expect("window sum failed");
    let second = rows.iter().find(|r| r.get("id").and_then(|c| c.text()) == Some("b")).unwrap();
    match second.get("w") {
        Some(Cell::Int(n)) => panic!("running sum wrapped to {n}"),
        Some(Cell::Float(f)) => assert!(*f > 1.8e19, "running sum {f}"),
        other => panic!("running sum: {other:?}"),
    }
}
