//! Where a filter sits must not change what it means: a filter on left-side fields gives
//! the same rows before or after a join; a filter on right-side fields after a join
//! keeps exactly the matching pairs; a filter after a window sees the window's output
//! computed over the unfiltered rows; and a filter after `count by` filters groups.

use lin::Db;

fn seed() -> Db {
    let mut db = Db::empty();
    db.run("col p { id: text, label: text, w: i64 }").unwrap();
    db.run("col c { id: text, pid: text, k: i64 }").unwrap();
    db.run("fk c.pid -> p.id").unwrap();
    db.run(r#"insert p [{ id: "p1", label: "one", w: 10 }, { id: "p2", label: "two", w: 20 }, { id: "p3", label: "three", w: 30 }]"#).unwrap();
    let rows = (0..90).map(|i| format!(r#"{{ id: "c{i:02}", pid: "p{}", k: {} }}"#, 1 + i % 3, i % 10)).collect::<Vec<_>>().join(",");
    db.run(&format!("insert c [{rows}]")).unwrap();
    db
}

fn ids(db: &mut Db, q: &str) -> Vec<String> {
    let mut v: Vec<String> = db.run(q).unwrap_or_else(|e| panic!("{q}: {e}")).rows.iter().map(|r| r.get("id").and_then(|c| c.text()).unwrap().to_string()).collect();
    v.sort();
    v
}

#[test]
fn filters_mean_the_same_wherever_they_sit() {
    let mut db = seed();
    assert_eq!(ids(&mut db, "c | k > 6 | join p on pid | take all"), ids(&mut db, "c | join p on pid | k > 6 | take all"));
    let want: Vec<String> = (0..90).filter(|i| i % 3 == 1).map(|i| format!("c{i:02}")).collect();
    assert_eq!(ids(&mut db, r#"c | join p on pid | p.label == "two" | take all"#), want);
    assert_eq!(ids(&mut db, "c | join p on pid | p.w > 15 and k == 0 | take all"), ["c10", "c20", "c40", "c50", "c70", "c80"]);
    assert!(ids(&mut db, r#"c | join p on pid | p.label == "none" | take all"#).is_empty());

    // Window then filter: row numbers come from all rows, the filter keeps some of them.
    let rows = db.run("c | window row_number sort id asc as rn | k == 3 | pick { id, rn } | take all").unwrap().rows.clone();
    assert_eq!(rows.len(), 9);
    for r in &rows {
        let id = r.get("id").and_then(|c| c.text()).unwrap();
        let rn = r.get("rn").and_then(|c| c.as_f64()).unwrap() as usize;
        let idx: usize = id[1..].parse().unwrap();
        assert_eq!(rn, idx + 1, "{id}: window computed after the filter");
    }

    // Filter then group vs group of the filtered rows.
    let by = db.run("c | k < 5 | count by pid").unwrap().rows.clone();
    assert_eq!(by.len(), 3);
    for r in &by {
        let n = r.iter().find(|(k, _)| k.as_str() != "pid").and_then(|(_, v)| v.as_f64()).unwrap();
        assert_eq!(n, 15.0, "{r:?}");
    }
}
