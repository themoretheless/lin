//! `avg` accumulates integers exactly, like `sum` and `summarize avg()` do. Rounding each
//! value to `f64` first loses the low bits of integers beyond 2^53.

const BIG: i64 = (1 << 53) + 1;

fn seeded() -> lin::Db {
    let mut db = lin::Db::empty();
    db.run("col nums { grp: i64, v: i64 }").unwrap();
    // 2^53 + 1 is not representable as f64; three of them plus -(2^53 + 1) * 3 + 6 sum to 6.
    db.run(&format!(
        "insert nums [ {{ grp: 1, v: {BIG} }}, {{ grp: 1, v: {BIG} }}, {{ grp: 1, v: {BIG} }}, {{ grp: 1, v: {} }} ]",
        -(BIG * 3) + 6
    ))
    .unwrap();
    db
}

fn first_f64(db: &mut lin::Db, src: &str, field: &str) -> f64 {
    let h = db.run(src).unwrap();
    h.rows[0]
        .iter()
        .find_map(|(k, v)| k.contains(field).then(|| v.as_f64()))
        .flatten()
        .unwrap_or_else(|| panic!("no {field} in {:?}", h.rows[0]))
}

#[test]
fn avg_step_is_exact_for_integers_beyond_2_pow_53() {
    let mut db = seeded();
    assert_eq!(first_f64(&mut db, "nums | avg v", "v"), 1.5);
    assert_eq!(first_f64(&mut db, "nums | avg v by grp | take all", "v"), 1.5);
}

#[test]
fn avg_step_agrees_with_summarize_avg() {
    let mut db = seeded();
    let step = first_f64(&mut db, "nums | avg v", "v");
    let summarized = first_f64(&mut db, "nums | summarize { a: avg(v) }", "a");
    assert_eq!(step, summarized);
}
