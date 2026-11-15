//! Order where keys tie: a sort is stable (equal keys keep insertion order), `row_number`
//! over tied sort keys follows that same order on every run and after reopen, and an
//! inner join emits rows in the order of the left side.

use lin::Db;

fn tmp() -> std::path::PathBuf {
    let n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let d = std::env::temp_dir().join(format!("lin-stable-{}-{n}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn ids(db: &mut Db, q: &str) -> Vec<String> {
    db.run(q).unwrap_or_else(|e| panic!("{q}: {e}")).rows.iter().map(|r| r.get("id").and_then(|c| c.text()).unwrap().to_string()).collect()
}

#[test]
fn ties_keep_insertion_order_everywhere() {
    let dir = tmp();
    let mut db = Db::open(&dir).unwrap();
    db.run("col p { id: text, label: text }").unwrap();
    db.run("col c { id: text, pid: text, k: i64 }").unwrap();
    db.run("fk c.pid -> p.id").unwrap();
    db.run(r#"insert p [{ id: "p1", label: "a" }, { id: "p2", label: "b" }]"#).unwrap();
    // Insertion order deliberately differs from id order; k has many ties.
    let order: Vec<usize> = (0..300).map(|i| (i * 113) % 300).collect();
    let rows = order.iter().map(|i| format!(r#"{{ id: "c{i:03}", pid: "p{}", k: {} }}"#, 1 + i % 2, i % 4)).collect::<Vec<_>>().join(",");
    db.run(&format!("insert c [{rows}]")).unwrap();
    let inserted: Vec<String> = order.iter().map(|i| format!("c{i:03}")).collect();

    let mut want_sorted = Vec::new();
    for k in 0..4 {
        want_sorted.extend(order.iter().filter(|i| *i % 4 == k).map(|i| format!("c{i:03}")));
    }
    let q_sort = "c | sort k | { id } | take all";
    let q_rn = "c | window row_number sort k asc as rn | sort rn | pick { id } | take all";
    let q_join = "c | join p on pid | { id } | take all";
    for round in 0..3 {
        assert_eq!(ids(&mut db, q_sort), want_sorted, "round {round}: sort is not stable");
        assert_eq!(ids(&mut db, q_rn), want_sorted, "round {round}: row_number over ties");
        assert_eq!(ids(&mut db, q_join), inserted, "round {round}: join order");
    }
    drop(db);
    let mut db = Db::open(&dir).unwrap();
    // `fk` is a session statement: declared again for the new session.
    db.run("fk c.pid -> p.id").unwrap();
    assert_eq!(ids(&mut db, q_sort), want_sorted, "after reopen: sort");
    assert_eq!(ids(&mut db, q_rn), want_sorted, "after reopen: row_number");
    assert_eq!(ids(&mut db, q_join), inserted, "after reopen: join");
}
