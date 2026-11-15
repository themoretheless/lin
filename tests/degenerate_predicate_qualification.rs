//! Degenerate predicates: contradictions (`k > 5 and k < 3`, `k == 1 and k == 2`) select
//! nothing and tautologies over present values (`k < 5 or k >= 5`) select every row with
//! a value, identically with and without an index, and the empty result composes with
//! later stages (count, sort, aggregates).

use lin::Db;

fn seed(indexed: bool) -> Db {
    let mut db = Db::empty();
    db.run("col t { id: text, k: i64 }").unwrap();
    if indexed {
        db.run("index t [k]").unwrap();
    }
    let rows = (0..100).map(|i| if i % 10 == 0 { format!(r#"{{ id: "r{i}" }}"#) } else { format!(r#"{{ id: "r{i}", k: {} }}"#, i % 10) }).collect::<Vec<_>>().join(",");
    db.run(&format!("insert t [{rows}]")).unwrap();
    db
}

fn n(db: &mut Db, q: &str) -> f64 {
    db.run(q).unwrap_or_else(|e| panic!("{q}: {e}")).rows[0].get("hits").and_then(|c| c.as_f64()).unwrap()
}

#[test]
fn contradictions_and_tautologies() {
    for indexed in [false, true] {
        let mut db = seed(indexed);
        for q in ["t | k > 5 and k < 3 | count", "t | k == 1 and k == 2 | count", "t | k > 9 | count", "t | k in [100, 200] | count"] {
            assert_eq!(n(&mut db, q), 0.0, "indexed={indexed}: {q}");
        }
        assert_eq!(n(&mut db, "t | k < 5 or k >= 5 | count"), 90.0, "indexed={indexed}");
        assert_eq!(n(&mut db, "t | not (k > 5 and k < 3) | count"), 100.0, "indexed={indexed}");
        assert!(db.run("t | k > 5 and k < 3 | sort k | take all").unwrap().rows.is_empty());
        let s = db.run("t | k > 5 and k < 3 | sum k").unwrap().rows.clone();
        assert!(s.is_empty() || s[0].get("k").and_then(|c| c.as_f64()) == Some(0.0), "indexed={indexed}: {s:?}");
        assert!(db.run("t | k > 5 and k < 3 | count by k").unwrap().rows.is_empty());
    }
}
