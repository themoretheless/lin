//! Stages after an aggregate work on its output rows, like any pipeline stage: a filter
//! on the aggregate's output field acts as HAVING, sorting and projection see the output
//! names (`hits` for counts, the summed field's name for sums), and a second aggregate
//! aggregates the grouped rows. Checked against values computed here.

use lin::Db;

fn seed() -> Db {
    let mut db = Db::empty();
    db.run("col t { id: text, g: i64, v: i64 }").unwrap();
    let rows = (0..20).map(|i| format!(r#"{{ id: "r{i}", g: {}, v: {i} }}"#, i % 3)).collect::<Vec<_>>().join(",");
    db.run(&format!("insert t [{rows}]")).unwrap();
    db
}

fn pairs(db: &mut Db, q: &str, val: &str) -> Vec<(i64, f64)> {
    let mut v: Vec<(i64, f64)> = db.run(q).unwrap_or_else(|e| panic!("{q}: {e}")).rows.iter()
        .map(|r| (r.get("g").and_then(|c| c.as_f64()).unwrap() as i64, r.get(val).and_then(|c| c.as_f64()).unwrap()))
        .collect();
    v.sort_by_key(|p| p.0);
    v
}

#[test]
fn stages_after_aggregates() {
    let mut db = seed();
    let sum_g = |g: i64| (0..20).filter(|i| i % 3 == g).sum::<i64>() as f64;
    let cnt_g = |g: i64| (0..20).filter(|i| i % 3 == g).count() as f64;
    assert_eq!(pairs(&mut db, "t | count by g | hits > 6 | take all", "hits"), [(0, cnt_g(0)), (1, cnt_g(1))]);
    assert_eq!(pairs(&mut db, "t | sum v by g | v > 60 | take all", "v"), [(0, sum_g(0)), (1, sum_g(1))]);
    assert_eq!(pairs(&mut db, "t | sum v by g | g == 1 | take all", "v"), [(1, sum_g(1))]);
    let top = db.run("t | sum v by g | sort v desc | take 1").unwrap().rows.clone();
    assert_eq!(top[0].get("g").and_then(|c| c.as_f64()), Some(1.0));
    let n = |db: &mut Db, q: &str, f: &str| db.run(q).unwrap().rows[0].get(f).and_then(|c| c.as_f64()).unwrap();
    assert_eq!(n(&mut db, "t | count by g | sum hits", "hits"), 20.0);
    assert_eq!(n(&mut db, "t | count by g | count", "hits"), 3.0);
    assert_eq!(n(&mut db, "t | count | count", "hits"), 1.0);
    assert_eq!(db.run("t | count by g | { g } | take all").unwrap().rows.len(), 3);
    assert!(db.run("t | count by g | v > 1 | take all").is_err(), "filter on a field the aggregate dropped");
}
