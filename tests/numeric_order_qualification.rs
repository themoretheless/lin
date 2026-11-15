//! Ordering of numbers that a float comparison would get wrong: integers above 2^53 that
//! differ by one, integers next to floats of nearly the same value, and a column that
//! mixes both kinds. Sort, min / max, comparison filters and index lookups must all
//! agree with exact arithmetic, and the sort order must be a total, transitive order.

use lin::Db;

const BIG: i64 = 9_007_199_254_740_992; // 2^53

fn seed(indexed: bool) -> Db {
    let mut db = Db::empty();
    db.run("col t { id: text, v: i64 }").unwrap();
    if indexed {
        db.run("index t [v]").unwrap();
    }
    let vals: Vec<(String, String)> = vec![
        ("a".into(), format!("{}", BIG + 1)),
        ("b".into(), format!("{}", BIG)),
        ("c".into(), format!("{}", BIG + 2)),
        ("d".into(), format!("{}", BIG - 1)),
        ("e".into(), "1.5".into()),
        ("f".into(), "1".into()),
        ("g".into(), "2".into()),
        ("h".into(), "-0.5".into()),
        ("i".into(), format!("{}", i64::MAX)),
        ("j".into(), format!("{}", i64::MAX - 1)),
    ];
    let rows = vals.iter().map(|(id, v)| format!(r#"{{ id: "{id}", v: {v} }}"#)).collect::<Vec<_>>().join(",");
    db.run(&format!("insert t [{rows}]")).unwrap();
    db
}

fn ids(db: &mut Db, q: &str) -> Vec<String> {
    db.run(q).unwrap_or_else(|e| panic!("{q}: {e}")).rows.iter().map(|r| r.get("id").and_then(|c| c.text()).unwrap().to_string()).collect()
}

#[test]
fn large_integers_and_mixed_numbers_order_exactly() {
    for indexed in [false, true] {
        let mut db = seed(indexed);
        let asc = ids(&mut db, "t | sort v | { id } | take all");
        assert_eq!(asc, ["h", "f", "e", "g", "d", "b", "a", "c", "j", "i"], "indexed={indexed}");
        let desc = ids(&mut db, "t | sort v desc | { id } | take all");
        assert_eq!(desc, asc.iter().rev().cloned().collect::<Vec<_>>(), "indexed={indexed}");
        assert_eq!(ids(&mut db, &format!("t | v == {} | {{ id }}", BIG + 1)), ["a"], "indexed={indexed}");
        assert_eq!(ids(&mut db, &format!("t | v > {} | sort v | {{ id }} | take all", BIG)), ["a", "c", "j", "i"], "indexed={indexed}");
        assert_eq!(ids(&mut db, &format!("t | v >= {} and v < {} | sort v | {{ id }} | take all", BIG, BIG + 2)), ["b", "a"], "indexed={indexed}");
        assert_eq!(ids(&mut db, "t | v > 1 and v < 2 | { id } | take all"), ["e"], "indexed={indexed}");
        assert_eq!(ids(&mut db, &format!("t | v == {} | {{ id }}", i64::MAX - 1)), ["j"], "indexed={indexed}");
        let max = db.run("t | max v").unwrap().rows[0].get("v").cloned().unwrap();
        assert_eq!(max, lin::Cell::Int(i64::MAX), "indexed={indexed}");
        let min = db.run("t | min v").unwrap().rows[0].get("v").and_then(|c| c.as_f64()).unwrap();
        assert_eq!(min, -0.5, "indexed={indexed}");
    }
}
