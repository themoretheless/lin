//! Grouping, sorting and joining against oracles computed here from the same rows:
//! aggregates over empty and partial input, groups for missing keys, stable sorting of
//! equal keys, Top-K equal to the prefix of a full sort, limits at their extremes, join
//! multiplicity with duplicate keys, and rows whose join key is missing.

use std::collections::BTreeMap;

use lin::{Cell, Db};

struct Row {
    id: String,
    g: Option<i64>,
    v: Option<f64>,
    k: Option<i64>,
}

fn data() -> Vec<Row> {
    let mut x: u64 = 0x1234_5678_9ABC_DEF1;
    let mut next = move || {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        x
    };
    (0..600)
        .map(|i| Row {
            id: format!("r{i:04}"),
            g: (next() % 9 != 0).then(|| (next() % 6) as i64),
            v: (next() % 11 != 0).then(|| (next() % 400) as f64 / 4.0),
            // Few distinct keys: many ties for the stability checks.
            k: (next() % 13 != 0).then(|| (next() % 5) as i64),
        })
        .collect()
}

fn load(db: &mut Db, rows: &[Row]) {
    db.run("col t { id: text, g: i64, v: f64, k: i64 }").unwrap();
    db.run("col empty { id: text, v: f64 }").unwrap();
    let mut src = String::from("insert t [");
    for (i, r) in rows.iter().enumerate() {
        if i > 0 {
            src.push(',');
        }
        src.push_str(&format!(r#"{{ id: "{}""#, r.id));
        if let Some(g) = r.g {
            src.push_str(&format!(", g: {g}"));
        }
        if let Some(v) = r.v {
            src.push_str(&format!(", v: {v:?}"));
        }
        if let Some(k) = r.k {
            src.push_str(&format!(", k: {k}"));
        }
        src.push_str(" }");
    }
    src.push(']');
    db.run(&src).unwrap();
}

fn num(c: Option<&Cell>) -> Option<f64> {
    c.and_then(|c| c.as_f64())
}

#[test]
fn aggregates_match_an_oracle() {
    let rows = data();
    let mut db = Db::empty();
    load(&mut db, &rows);

    // Whole-table aggregates.
    let present: Vec<f64> = rows.iter().filter_map(|r| r.v).collect();
    let scalar = |db: &mut Db, q: &str, f: &str| num(db.run(q).unwrap().rows[0].get(f));
    assert_eq!(scalar(&mut db, "t | count", "hits"), Some(rows.len() as f64));
    assert_eq!(scalar(&mut db, "t | sum v", "v"), Some(present.iter().sum()));
    assert_eq!(scalar(&mut db, "t | min v", "v"), present.iter().copied().reduce(f64::min));
    assert_eq!(scalar(&mut db, "t | max v", "v"), present.iter().copied().reduce(f64::max));
    let avg = present.iter().sum::<f64>() / present.len() as f64;
    assert_eq!(scalar(&mut db, "t | avg v", "v"), Some(avg));

    // Over no rows: a count of zero, a zero sum, no minimum or maximum.
    assert_eq!(scalar(&mut db, "empty | count", "hits"), Some(0.0));
    assert_eq!(scalar(&mut db, "t | v > 1000 | count", "hits"), Some(0.0));
    assert_eq!(scalar(&mut db, "t | v > 1000 | sum v", "v"), Some(0.0));
    for agg in ["min", "max"] {
        let rows_out = db.run(&format!("t | v > 1000 | {agg} v")).unwrap().rows.clone();
        assert!(rows_out.is_empty() || rows_out[0].get("v").is_none_or(|c| matches!(c, Cell::Null)), "{agg}: {rows_out:?}");
    }
    let grouped = db.run("t | v > 1000 | count by g | take all").unwrap().rows.clone();
    assert!(grouped.is_empty(), "groups over no rows: {grouped:?}");

    // Grouped: rows without the key form one group of their own.
    let mut counts: BTreeMap<Option<i64>, i64> = BTreeMap::new();
    let mut sums: BTreeMap<Option<i64>, f64> = BTreeMap::new();
    for r in &rows {
        *counts.entry(r.g).or_default() += 1;
        *sums.entry(r.g).or_default() += r.v.unwrap_or(0.0);
    }
    let key = |c: Option<&Cell>| c.and_then(|c| match c {
        Cell::Int(n) => Some(*n),
        _ => None,
    });
    let got: BTreeMap<Option<i64>, i64> = db
        .run("t | count by g | take all")
        .unwrap()
        .rows
        .iter()
        .map(|r| (key(r.get("g")), num(r.get("hits")).unwrap() as i64))
        .collect();
    assert_eq!(got, counts);
    let got: BTreeMap<Option<i64>, f64> = db
        .run("t | sum v by g | take all")
        .unwrap()
        .rows
        .iter()
        .map(|r| (key(r.get("g")), num(r.get("v")).unwrap()))
        .collect();
    assert_eq!(got, sums);

    // Integer sums past i64 are not wrapped.
    db.run("col big { id: text, n: i64 }").unwrap();
    db.run(r#"insert big [{ id: "a", n: 9223372036854775807 }, { id: "b", n: 9223372036854775807 }]"#).unwrap();
    let s = scalar(&mut db, "big | sum n", "n").unwrap();
    assert!(s > 1.8e19, "sum wrapped: {s}");
}

#[test]
fn sorting_is_stable_and_top_k_is_a_prefix() {
    let rows = data();
    let mut db = Db::empty();
    load(&mut db, &rows);
    let ids = |db: &mut Db, q: &str| -> Vec<String> {
        db.run(q)
            .unwrap_or_else(|e| panic!("{q}: {e}"))
            .rows
            .iter()
            .map(|r| r.get("id").and_then(|c| c.text()).unwrap().to_string())
            .collect()
    };
    for (field, desc) in [("k", false), ("k", true), ("v", false), ("v", true), ("g", false)] {
        let dir = if desc { " desc" } else { "" };
        let full = ids(&mut db, &format!("t | sort {field}{dir} | {{ id }} | take all"));
        assert_eq!(full.len(), rows.len());
        // Equal keys keep insertion order (ids are generated in insertion order).
        let value = |id: &str| -> Option<f64> {
            let r = rows.iter().find(|r| r.id == id).unwrap();
            match field {
                "k" => r.k.map(|n| n as f64),
                "g" => r.g.map(|n| n as f64),
                _ => r.v,
            }
        };
        for w in full.windows(2) {
            if value(&w[0]) == value(&w[1]) {
                assert!(w[0] < w[1], "sort {field}{dir}: equal keys out of insertion order: {w:?}");
            }
        }
        // Missing values sit together at one end.
        let nulls: Vec<usize> = full.iter().enumerate().filter(|(_, id)| value(id).is_none()).map(|(i, _)| i).collect();
        if let (Some(&first), Some(&last)) = (nulls.first(), nulls.last()) {
            assert!(first == 0 || last == full.len() - 1, "sort {field}{dir}: nulls in the middle");
            assert_eq!(last - first + 1, nulls.len(), "sort {field}{dir}: nulls not contiguous");
        }
        // Top-K and skip are slices of the full order.
        for k in [0usize, 1, 7, 50, 599, 600, 10_000] {
            let top = ids(&mut db, &format!("t | sort {field}{dir} | {{ id }} | take {k}"));
            assert_eq!(top, full[..k.min(full.len())], "sort {field}{dir} | take {k}");
        }
        for (skip, take) in [(0usize, 10usize), (595, 10), (600, 5), (9_223_372_036_854_775_807, 5)] {
            let page = ids(&mut db, &format!("t | sort {field}{dir} | {{ id }} | skip {skip} | take {take}"));
            let lo = skip.min(full.len());
            let hi = skip.saturating_add(take).min(full.len());
            assert_eq!(page, full[lo..hi], "sort {field}{dir} | skip {skip} | take {take}");
        }
    }
}

#[test]
fn join_multiplicity_and_missing_keys() {
    let mut db = Db::empty();
    db.run("col p { id: text, label: text }").unwrap();
    db.run("col c { id: text, pid: text, n: i64 }").unwrap();
    db.run("fk c.pid -> p.id").unwrap();
    db.run(r#"insert p [{ id: "p1", label: "one" }, { id: "p2", label: "two" }, { id: "p3", label: "three" }]"#).unwrap();
    // Several children per parent, children without a parent key.
    let mut src = String::from("insert c [");
    let mut expected: BTreeMap<String, usize> = BTreeMap::new();
    let mut orphans = 0usize;
    for i in 0..90 {
        if i > 0 {
            src.push(',');
        }
        if i % 10 == 9 {
            src.push_str(&format!(r#"{{ id: "c{i}", n: {i} }}"#));
            orphans += 1;
        } else {
            let pid = format!("p{}", 1 + i % 3);
            *expected.entry(pid.clone()).or_default() += 1;
            src.push_str(&format!(r#"{{ id: "c{i}", pid: "{pid}", n: {i} }}"#));
        }
    }
    src.push(']');
    db.run(&src).unwrap();
    let inner = db.run("c | join p on pid | take all").unwrap().rows.clone();
    assert_eq!(inner.len(), expected.values().sum::<usize>());
    let mut per: BTreeMap<String, usize> = BTreeMap::new();
    for r in &inner {
        *per.entry(r.get("pid").and_then(|c| c.text()).unwrap().to_string()).or_default() += 1;
        assert!(r.get("p.label").is_some() || r.iter().any(|(k, _)| k.ends_with("label")), "joined row without parent fields: {r:?}");
    }
    assert_eq!(per, expected);
    let left = db.run("c | join left p on pid | take all").unwrap().rows.clone();
    assert_eq!(left.len(), 90, "left join must keep orphans");
    let kept: usize = left.iter().filter(|r| r.get("pid").is_none()).count();
    assert_eq!(kept, orphans);
    // The projected and cursor paths agree with the row path.
    let projected = db.run("c | join p on pid | { id, p.label } | take all").unwrap().rows.len();
    assert_eq!(projected, inner.len());
    let lin::Stmt::Query(q) = lin::parse("c | join p on pid | take all").unwrap() else { unreachable!() };
    assert_eq!(db.cursor(&q).unwrap().count(), inner.len());
}
