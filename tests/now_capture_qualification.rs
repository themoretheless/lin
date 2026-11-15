//! `now` is captured once per statement: every row of a bulk insert gets the same stamp,
//! a filter comparing against `now` sees one instant for the whole scan, and statements
//! later in time get stamps that never go backwards.

use lin::{Cell, Db};

fn stamps(db: &mut Db, q: &str) -> Vec<i64> {
    db.run(q).unwrap().rows.iter().map(|r| match r.get("stamp") {
        Some(Cell::Time(t)) => *t,
        other => panic!("not a time: {other:?}"),
    }).collect()
}

#[test]
fn now_is_one_instant_per_statement() {
    let mut db = Db::empty();
    db.run("col s { id: text, stamp: time, b: i64 }").unwrap();
    let mut last = i64::MIN;
    for batch in 0..20 {
        let rows = (0..500).map(|i| format!(r#"{{ id: "b{batch}-{i}", stamp: now, b: {batch} }}"#)).collect::<Vec<_>>().join(",");
        db.run(&format!("insert s [{rows}]")).unwrap();
        let got = stamps(&mut db, &format!("s | b == {batch} | {{ stamp }} | take all"));
        assert_eq!(got.len(), 500);
        assert!(got.iter().all(|t| *t == got[0]), "batch {batch}: stamps differ within one statement");
        assert!(got[0] >= last, "batch {batch}: stamp went backwards");
        last = got[0];
    }
    // Every stored stamp is at or before the instant of the next statement, and a filter
    // against `now - 1d` keeps all of them; one against a later `now` excludes none twice.
    let n = |db: &mut Db, q: &str| db.run(q).unwrap().rows[0].get("hits").and_then(|c| c.as_f64()).unwrap();
    assert_eq!(n(&mut db, "s | stamp <= now | count"), 10_000.0);
    assert_eq!(n(&mut db, "s | stamp > now - 1d | count"), 10_000.0);
    assert_eq!(n(&mut db, "s | stamp > now | count"), 0.0);
}
