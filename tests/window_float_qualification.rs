//! Sliding-frame window sums over floats: a frame holding only small values must sum
//! exactly those values, even when a huge value appeared earlier in the column (computing
//! frames by subtracting running prefix sums would lose them to the huge prefix).

use lin::Db;

fn naive(vals: &[f64], i: usize, before: usize, after: usize) -> f64 {
    let lo = i.saturating_sub(before);
    let hi = (i + after).min(vals.len() - 1);
    vals[lo..=hi].iter().sum()
}

#[test]
fn sliding_float_sums_match_naive_frames() {
    let mut db = Db::empty();
    db.run("col f { id: text, k: i64, v: f64 }").unwrap();
    // One huge value first, then small values that are exact in f64 on their own.
    let vals: Vec<f64> = (0..60).map(|i| match i {
        0 => 1e16,
        _ if i % 4 == 0 => -0.0,
        _ if i % 2 == 0 => 0.5,
        _ => 1.0,
    }).collect();
    let rows = vals.iter().enumerate().map(|(i, v)| format!(r#"{{ id: "r{i:02}", k: {i}, v: {v:.1} }}"#)).collect::<Vec<_>>().join(",");
    db.run(&format!("insert f [{rows}]")).unwrap();
    for (before, after) in [(1usize, 1usize), (0, 2), (2, 0), (3, 3)] {
        let q = format!("f | window sum v sort k asc rows between {before} preceding and {after} following as w | pick {{ id, w }} | take all");
        let got = db.run(&q).unwrap_or_else(|e| panic!("{q}: {e}")).rows.clone();
        assert_eq!(got.len(), vals.len());
        for r in &got {
            let i: usize = r.get("id").and_then(|c| c.text()).unwrap()[1..].parse().unwrap();
            let w = r.get("w").and_then(|c| c.as_f64()).unwrap();
            // Frames that include the huge first row are not checked bit-for-bit.
            if i <= before {
                continue;
            }
            let want = naive(&vals, i, before, after);
            assert_eq!(w, want, "{before}/{after} row {i}: frame of small values");
        }
    }
}
