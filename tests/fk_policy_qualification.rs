//! Foreign keys: declaring one over a missing collection or field is a compile error; an
//! insert or update whose reference has no target is rejected and changes nothing; a
//! cycle of keys (a → b, b → a) can be declared and filled by inserting one side without
//! the reference first and setting it afterwards; and a row that is still referenced
//! cannot be deleted (on every delete path), while unreferenced rows and rows deleted
//! together with everything that references them can.

use lin::Db;

fn count(db: &mut Db, c: &str) -> f64 {
    db.run(&format!("{c} | count")).unwrap().rows[0].get("hits").and_then(|c| c.as_f64()).unwrap()
}

#[test]
fn foreign_key_declarations_and_references() {
    let mut db = Db::empty();
    db.run("col a { id: text, b: text }").unwrap();
    db.run("col b { id: text, a: text }").unwrap();
    for bad in ["fk a.b -> nosuch.id", "fk a.nofield -> b.id", "fk a.b -> b.nofield"] {
        assert!(db.run(bad).is_err(), "accepted: {bad}");
    }
    db.run("fk a.b -> b.id").unwrap();
    db.run("fk b.a -> a.id").unwrap();
    assert!(db.run(r#"insert a { id: "a1", b: "b1" }"#).is_err(), "dangling reference accepted on insert");
    assert_eq!(count(&mut db, "a"), 0.0);
    db.run(r#"insert a { id: "a1" }"#).unwrap();
    db.run(r#"insert b { id: "b1", a: "a1" }"#).unwrap();
    assert!(db.run(r#"update a[id == "a1"] cas each { b: "missing" }"#).is_err(), "dangling reference accepted on update");
    db.run(r#"update a[id == "a1"] cas each { b: "b1" }"#).unwrap();
    let joined = db.run("a | join b on b | take all").unwrap().rows.len();
    assert_eq!(joined, 1);
}

#[test]
fn referenced_rows_cannot_be_deleted() {
    let mut db = Db::empty();
    db.run("col p { id: text, label: text }").unwrap();
    db.run("col c { id: text, pid: text }").unwrap();
    db.run("fk c.pid -> p.id").unwrap();
    db.run(r#"insert p [{ id: "p1", label: "used" }, { id: "p2", label: "free" }]"#).unwrap();
    db.run(r#"insert c [{ id: "c1", pid: "p1" }, { id: "c2", pid: "p1" }]"#).unwrap();

    let e = db.run(r#"delete p[id == "p1"] cas each"#).err().expect("deleted a referenced row");
    assert!(e.to_string().contains("still references"), "{e}");
    assert_eq!(count(&mut db, "p"), 2.0);
    // Through a broader predicate that also matches the referenced row: nothing is deleted.
    assert!(db.run(r#"delete p[label == "used" or label == "free"] cas each"#).is_err());
    assert_eq!(count(&mut db, "p"), 2.0);
    // Through a prepared statement and inside a transaction.
    let prep = db.prepare(r#"delete p[id == "p1"] cas each"#).unwrap();
    assert!(prep.execute(&mut db).is_err());
    {
        let mut tx = db.transaction().unwrap();
        assert!(tx.run(r#"delete p[id == "p1"] cas each"#).is_err());
        tx.rollback().unwrap();
    }
    assert_eq!(count(&mut db, "p"), 2.0);
    // An unreferenced row can go.
    db.run(r#"delete p[id == "p2"] cas each"#).unwrap();
    // Once the references are gone, so can the target.
    db.run(r#"delete c[pid == "p1"] cas each"#).unwrap();
    db.run(r#"delete p[id == "p1"] cas each"#).unwrap();
    assert_eq!(count(&mut db, "p"), 0.0);
    // Repointing a reference also frees the old target.
    db.run(r#"insert p [{ id: "p3", label: "x" }, { id: "p4", label: "y" }]"#).unwrap();
    db.run(r#"insert c { id: "c3", pid: "p3" }"#).unwrap();
    db.run(r#"update c[id == "c3"] cas each { pid: "p4" }"#).unwrap();
    db.run(r#"delete p[id == "p3"] cas each"#).unwrap();
    assert!(db.run(r#"delete p[id == "p4"] cas each"#).is_err());
}

#[test]
fn self_referencing_rows_can_be_deleted_together() {
    let mut db = Db::empty();
    db.run("col n { id: text, parent: text }").unwrap();
    db.run("fk n.parent -> n.id").unwrap();
    db.run(r#"insert n { id: "root" }"#).unwrap();
    db.run(r#"insert n [{ id: "kid", parent: "root" }, { id: "other" }]"#).unwrap();
    assert!(db.run(r#"delete n[id == "root"] cas each"#).is_err(), "deleted a parent with a child");
    db.run(r#"delete n[id == "root" or id == "kid"] cas each"#).unwrap();
    assert_eq!(count(&mut db, "n"), 1.0);
}

/// The same refusal on the cold tier, with both collections disk-backed.
#[test]
fn referenced_cold_rows_cannot_be_deleted() {
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!("lin-fk-cold-{}-{}", std::process::id(), SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let opts = || lin::OpenOpts { sync: lin::SyncMode::Normal, cold: true };
    {
        let mut db = Db::open_with(&dir, opts()).unwrap();
        db.run("col p { id: text, label: text }").unwrap();
        db.run("col c { id: text, pid: text }").unwrap();
        let ps = (0..64).map(|i| format!(r#"{{ id: "p{i}", label: "l{i}" }}"#)).collect::<Vec<_>>().join(",");
        db.run(&format!("insert p [{ps}]")).unwrap();
        let cs = (0..64).map(|i| format!(r#"{{ id: "c{i}", pid: "p{}" }}"#, i % 8)).collect::<Vec<_>>().join(",");
        db.run(&format!("insert c [{cs}]")).unwrap();
        db.checkpoint().unwrap();
        db.close().unwrap();
    }
    let mut db = Db::open_with(&dir, opts()).unwrap();
    db.run("fk c.pid -> p.id").unwrap();
    assert!(db.store.is_disk_backed("p") && db.store.is_disk_backed("c"));
    assert!(db.run(r#"delete p[id == "p3"] cas each"#).is_err(), "deleted a referenced cold row");
    assert_eq!(count(&mut db, "p"), 64.0);
    db.run(r#"delete p[id == "p20"] cas each"#).unwrap();
    assert_eq!(count(&mut db, "p"), 63.0);
}
