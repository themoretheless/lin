//! Results outlive what produced them: rows returned inside a transaction stay readable
//! after the transaction commits or rolls back (and reflect the statement, not the final
//! state), and result rows can be moved to another thread and read there while the
//! database keeps changing.

use lin::Db;

#[test]
fn transaction_results_outlive_the_transaction() {
    let mut db = Db::empty();
    db.run("col t { id: text, v: i64 }").unwrap();
    db.run(r#"insert t [{ id: "a", v: 1 }, { id: "b", v: 2 }]"#).unwrap();
    let (committed, rolled_back) = {
        let mut tx = db.transaction().unwrap();
        let seen = tx.run("t | sort id | take all").unwrap();
        tx.run(r#"insert t { id: "c", v: 3 }"#).unwrap();
        tx.commit().unwrap();
        let mut tx = db.transaction().unwrap();
        tx.run(r#"insert t { id: "d", v: 4 }"#).unwrap();
        let inside = tx.run("t | sort id | take all").unwrap();
        tx.rollback().unwrap();
        (seen, inside)
    };
    let ids = |h: &lin::Handle| -> Vec<String> { h.rows.iter().map(|r| r.get("id").and_then(|c| c.text()).unwrap().to_string()).collect() };
    assert_eq!(ids(&committed), ["a", "b"]);
    assert_eq!(ids(&rolled_back), ["a", "b", "c", "d"], "result from inside the rolled-back transaction");
    assert_eq!(db.run("t | count").unwrap().rows[0].get("hits").and_then(|c| c.as_f64()), Some(3.0));
}

#[test]
fn result_rows_move_across_threads() {
    let mut db = Db::empty();
    db.run("col t { id: text, s: text }").unwrap();
    let long = "z".repeat(80);
    db.run(&format!(r#"insert t [{{ id: "a", s: "{long}" }}, {{ id: "b", s: "short" }}]"#)).unwrap();
    let rows = db.run("t | sort id | take all").unwrap().rows.clone();
    let reader = std::thread::spawn(move || rows.iter().map(|r| r.get("s").and_then(|c| c.text()).unwrap().to_string()).collect::<Vec<_>>());
    db.run(r#"delete t[id == "a"] cas each"#).unwrap();
    db.run(r#"insert t { id: "c", s: "new" }"#).unwrap();
    assert_eq!(reader.join().unwrap(), [long, "short".to_string()]);
}
