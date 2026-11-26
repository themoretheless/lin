//! A composite index over (g, k) answers equality on the prefix plus a range on the
//! second column, ascending and descending, with skip / take, exactly like the same
//! query without the index and like an oracle computed here.

use lin::Db;

fn seed(indexed: bool) -> Db {
    let mut db = Db::empty();
    db.run("col t { id: text, g: i64, k: i64 }").unwrap();
    if indexed {
        db.run("index t [g, k]").unwrap();
    }
    let rows = (0..600).map(|i| format!(r#"{{ id: "r{i:03}", g: {}, k: {} }}"#, i % 6, (i * 7) % 97)).collect::<Vec<_>>().join(",");
    db.run(&format!("insert t [{rows}]")).unwrap();
    db
}

fn ids(db: &mut Db, q: &str) -> Vec<String> {
    db.run(q).unwrap_or_else(|e| panic!("{q}: {e}")).rows.iter().map(|r| r.get("id").and_then(|c| c.text()).unwrap().to_string()).collect()
}

#[test]
fn composite_bounds_in_both_directions() {
    let mut with = seed(true);
    let mut without = seed(false);
    let data: Vec<(String, i64, i64)> = (0..600).map(|i| (format!("r{i:03}"), (i % 6) as i64, ((i * 7) % 97) as i64)).collect();
    for g in [0i64, 3, 5] {
        for (lo, hi) in [(10i64, 40i64), (0, 96), (50, 50), (90, 200), (-5, 3)] {
            for dir in ["", " desc"] {
                for (skip, take) in [(0usize, 1000usize), (2, 5)] {
                    // Sorting by id and then (stably) by k makes ties in k come out in id order.
                    let q = format!("t | g == {g} and k >= {lo} and k <= {hi} | sort id | sort k{dir} | skip {skip} | take {take} | {{ id }}");
                    let mut want: Vec<&(String, i64, i64)> = data.iter().filter(|r| r.1 == g && r.2 >= lo && r.2 <= hi).collect();
                    want.sort_by(|a, b| a.0.cmp(&b.0));
                    want.sort_by_key(|r| r.2);
                    if dir == " desc" {
                        want.sort_by(|a, b| b.2.cmp(&a.2));
                    }
                    let want: Vec<String> = want.iter().skip(skip).take(take).map(|r| r.0.clone()).collect();
                    assert_eq!(ids(&mut with, &q), want, "indexed: {q}");
                    assert_eq!(ids(&mut without, &q), want, "plain: {q}");
                }
            }
        }
    }
}

#[test]
fn composite_index_seeks_both_range_bounds() {
    let mut db = Db::empty();
    db.run("col events { id: text, tenant: text, tick: i64, tag: text }")
        .unwrap();
    for (id, tenant, tick) in [
        ("a", "x", 10),
        ("b", "x", 20),
        ("c", "x", 30),
        ("d", "y", 20),
    ] {
        db.run(&format!(
            r#"insert events {{ id: "{id}", tenant: "{tenant}", tick: {tick}, tag: "ok" }}"#
        ))
        .unwrap();
    }
    let queries = [
        (r#"tenant == "x" and tick > 10 and tick <= 20"#, 1),
        (r#"tenant == "x" and tick <= 20 and tick > 10"#, 1),
        (r#"tenant == "x" and tick >= 20 and tick <= 20"#, 1),
        (r#"tenant == "x" and tick > 20 and tick <= 20"#, 0),
        (r#"tenant == "x" and tick > 30 and tick < 10"#, 0),
    ];
    db.run("index events [tenant, tick, tag]").unwrap();
    for (filter, count) in queries {
        let result = db.run(&format!("events | {filter} | count")).unwrap();
        assert_eq!(
            result.rows[0].get("hits"),
            Some(&lin::Cell::Int(count)),
            "{filter}"
        );
    }
}

#[test]
fn descending_prefix_top_k_preserves_ties_and_residuals() {
    let mut indexed = seed(true);
    let mut plain = seed(false);
    for dir in ["", " desc"] {
        for extra in ["", " and k >= 10 and k < 50", r#" and id != "r007""#] {
            let query = format!("t | g == 1{extra} | sort k{dir} | take 20");
            assert_eq!(ids(&mut indexed, &query), ids(&mut plain, &query));
        }
    }
}
