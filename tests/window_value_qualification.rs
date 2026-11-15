//! Value window functions against an oracle per partition: `first_value` / `last_value`
//! over the whole partition and over explicit frames, `lag` / `lead` at the partition
//! edges (absent), with descending order, and single-row partitions.

use std::collections::BTreeMap;

use lin::Db;

fn seed() -> (Db, Vec<(String, i64, i64, i64)>) {
    let mut db = Db::empty();
    db.run("col w { id: text, g: i64, k: i64, v: i64 }").unwrap();
    // g = 4 has a single row.
    let rows: Vec<(String, i64, i64, i64)> = (0..41).map(|i| (format!("r{i:02}"), if i == 40 { 4 } else { i % 4 }, (i * 13) % 41, i * 10)).collect();
    let lit = rows.iter().map(|(id, g, k, v)| format!(r#"{{ id: "{id}", g: {g}, k: {k}, v: {v} }}"#)).collect::<Vec<_>>().join(",");
    db.run(&format!("insert w [{lit}]")).unwrap();
    (db, rows)
}

fn window(db: &mut Db, q: &str) -> BTreeMap<String, Option<i64>> {
    db.run(q).unwrap_or_else(|e| panic!("{q}: {e}")).rows.iter().map(|r| {
        (r.get("id").and_then(|c| c.text()).unwrap().to_string(), r.get("x").and_then(|c| c.as_f64()).map(|f| f as i64))
    }).collect()
}

#[test]
fn value_functions_match_partition_oracle() {
    let (mut db, rows) = seed();
    let mut parts: BTreeMap<i64, Vec<&(String, i64, i64, i64)>> = BTreeMap::new();
    for r in &rows {
        parts.entry(r.1).or_default().push(r);
    }
    for (dir, desc) in [("asc", false), ("desc", true)] {
        let mut want_first = BTreeMap::new();
        let mut want_last = BTreeMap::new();
        let mut want_lag = BTreeMap::new();
        let mut want_lead = BTreeMap::new();
        let mut want_next_last = BTreeMap::new();
        for p in parts.values() {
            let mut p = p.clone();
            p.sort_by_key(|r| r.2);
            if desc {
                p.reverse();
            }
            for (i, r) in p.iter().enumerate() {
                want_first.insert(r.0.clone(), Some(p[0].3));
                want_lag.insert(r.0.clone(), if i == 0 { None } else { Some(p[i - 1].3) });
                want_lead.insert(r.0.clone(), p.get(i + 1).map(|x| x.3));
                want_last.insert(r.0.clone(), Some(p[p.len() - 1].3));
                want_next_last.insert(r.0.clone(), Some(p[(i + 1).min(p.len() - 1)].3));
            }
        }
        let q = |f: &str, frame: &str| format!("w | window {f} v by g sort k {dir}{frame} as x | pick {{ id, x }} | take all");
        assert_eq!(window(&mut db, &q("first_value", "")), want_first, "first_value {dir}");
        assert_eq!(window(&mut db, &q("lag", "")), want_lag, "lag {dir}");
        assert_eq!(window(&mut db, &q("lead", "")), want_lead, "lead {dir}");
        assert_eq!(window(&mut db, &q("last_value", " rows between current row and unbounded following")), want_last, "last_value to the end {dir}");
        assert_eq!(window(&mut db, &q("last_value", " rows between current row and 1 following")), want_next_last, "last_value one ahead {dir}");
    }
}
