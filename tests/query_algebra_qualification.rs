//! Pipeline stages against an oracle computed directly in Rust from the inserted rows:
//! filter / sort / skip / take in every order the language allows, huge skips, take
//! larger than the input, and stage pairs that must commute (filter with projection,
//! two filters, skip after take). Sorting is by a unique key so the oracle is exact.

use lin::Db;

const N: i64 = 240;

fn seed() -> Db {
    let mut db = Db::empty();
    db.run("col t { id: text, k: i64, g: i64, s: text }").unwrap();
    let rows = (0..N)
        .map(|i| format!(r#"{{ id: "r{i:03}", k: {}, g: {}, s: "w{}" }}"#, (i * 37) % N, i % 7, i % 5))
        .collect::<Vec<_>>()
        .join(",");
    db.run(&format!("insert t [{rows}]")).unwrap();
    db
}

/// (id, k, g) for every row, in insertion order.
fn data() -> Vec<(String, i64, i64)> {
    (0..N).map(|i| (format!("r{i:03}"), (i * 37) % N, i % 7)).collect()
}

fn ids(db: &mut Db, q: &str) -> Vec<String> {
    db.run(q).unwrap_or_else(|e| panic!("{q}: {e}")).rows.iter().map(|r| r.get("id").and_then(|c| c.text()).unwrap().to_string()).collect()
}

#[test]
fn sort_skip_take_match_the_oracle() {
    let mut db = seed();
    let d = data();
    for g in [0, 3, 6] {
        for (skip, take) in [(0, 5), (3, 4), (0, 1000), (30, 10), (1000, 5), (34, 1), (0, 0)] {
            let mut want: Vec<(String, i64, i64)> = d.iter().filter(|r| r.2 == g).cloned().collect();
            want.sort_by_key(|r| r.1);
            let asc: Vec<String> = want.iter().skip(skip).take(take).map(|r| r.0.clone()).collect();
            want.reverse();
            let desc: Vec<String> = want.iter().skip(skip).take(take).map(|r| r.0.clone()).collect();
            let q = |dir: &str| format!("t | g == {g} | sort k{dir} | skip {skip} | take {take} | {{ id }}");
            assert_eq!(ids(&mut db, &q("")), asc, "{}", q(""));
            assert_eq!(ids(&mut db, &q(" desc")), desc, "{}", q(" desc"));
        }
    }
}

#[test]
fn huge_skip_and_take_are_bounded() {
    let mut db = seed();
    assert!(ids(&mut db, "t | sort k | skip 9223372036854775807 | take 5 | { id }").is_empty());
    assert_eq!(ids(&mut db, "t | sort k | take 9223372036854775807 | { id }").len(), N as usize);
    assert_eq!(ids(&mut db, "t | skip 239 | take all | { id }").len(), 1);
}

#[test]
fn commuting_stages_agree() {
    let mut db = seed();
    let pairs = [
        ("t | g == 2 | { id, k } | sort id | take all", "t | { id, k, g } | g == 2 | { id, k } | sort id | take all"),
        ("t | g == 2 | k > 100 | sort id | { id } | take all", "t | k > 100 | g == 2 | sort id | { id } | take all"),
        ("t | g == 2 and k > 100 | sort id | { id } | take all", "t | k > 100 | g == 2 | sort id | { id } | take all"),
        ("t | sort id | take 50 | skip 10 | { id }", "t | sort id | skip 10 | take 40 | { id }"),
        ("t | sort k | take 20 | sort id | { id }", "t | k < 20 | sort id | { id } | take all"),
        ("t | g == 1 or g == 1 | sort id | { id } | take all", "t | g == 1 | sort id | { id } | take all"),
        ("t | not (g != 4) | sort id | { id } | take all", "t | g == 4 | sort id | { id } | take all"),
    ];
    for (a, b) in pairs {
        assert_eq!(ids(&mut db, a), ids(&mut db, b), "\n{a}\n{b}");
    }
}

#[test]
fn counts_and_extremes_match_the_oracle() {
    let mut db = seed();
    let d = data();
    let one = |db: &mut Db, q: &str, f: &str| db.run(q).unwrap().rows[0].get(f).and_then(|c| c.as_f64()).unwrap();
    for g in 0..7 {
        let rows: Vec<_> = d.iter().filter(|r| r.2 == g).collect();
        assert_eq!(one(&mut db, &format!("t | g == {g} | count"), "hits"), rows.len() as f64);
        assert_eq!(one(&mut db, &format!("t | g == {g} | min k"), "k"), rows.iter().map(|r| r.1).min().unwrap() as f64);
        assert_eq!(one(&mut db, &format!("t | g == {g} | max k"), "k"), rows.iter().map(|r| r.1).max().unwrap() as f64);
        assert_eq!(one(&mut db, &format!("t | g == {g} | sum k"), "k"), rows.iter().map(|r| r.1).sum::<i64>() as f64);
    }
    // count by agrees with per-group counts.
    let by = db.run("t | count by g").unwrap().rows.clone();
    assert_eq!(by.len(), 7);
    for r in &by {
        let g = r.get("g").and_then(|c| c.as_f64()).unwrap() as i64;
        let n = r.iter().find(|(k, _)| k.as_str() != "g").and_then(|(_, v)| v.as_f64()).unwrap();
        assert_eq!(n, d.iter().filter(|x| x.2 == g).count() as f64, "count by g={g}");
    }
}
