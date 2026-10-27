use lin::{Db, OpenOpts, SyncMode};
use rusqlite::Connection;
use serde::Serialize;
use std::{path::PathBuf, time::Instant};
const TXS: usize = 1000;
const OPS: usize = 5;
#[derive(Serialize)]
struct Sample {
    transaction: usize,
    elapsed_ns: u64,
    checkpoint: Option<bool>,
    writes_before: Option<u32>,
    writes_after: Option<u32>,
}
fn verify_lin(db: &Db) {
    let rows = db.store.collection("orders");
    assert_eq!(rows.len(), TXS * OPS);
    let mut actual = std::collections::BTreeMap::new();
    for row in rows {
        let id = row["id"].text().unwrap().to_owned();
        let parent = row["user_id"].text().unwrap().to_owned();
        let total = match row["total"] { lin::Cell::Int(v) => v, _ => panic!("integer total") };
        assert!(actual.insert(id, (parent, total)).is_none());
    }
    for tx in 0..TXS {
        for op in 0..OPS {
            assert_eq!(actual.remove(&format!("ord_{tx}_{op}")), Some(("u1".to_owned(), (op * 10) as i64)));
        }
    }
    assert!(actual.is_empty());
    assert_eq!(db.store.collection("users").len(), 1);
    assert_eq!(db.store.get_by_id("users", "u1").unwrap()["email"].text(), Some("u1@test.com"));
    assert_eq!(db.store.r#gen, (TXS + 1) as u64);
}
fn main() {
    let args: Vec<String> = std::env::args().collect();
    assert_eq!(args.len(), 3);
    let engine = &args[1];
    let dir = PathBuf::from(&args[2]);
    assert!(!dir.exists());
    std::fs::create_dir(&dir).unwrap();
    let mut samples = Vec::with_capacity(TXS);
    let mut sqlite_settings = None;
    match engine.as_str() {
        "lin" => {
            let mut db = Db::open_with(&dir, OpenOpts { sync: SyncMode::Normal, cold: false }).unwrap();
            db.run(r#"insert users [{id:"u1",email:"u1@test.com"}]"#).unwrap();
            assert!(db.stats().sync_normal);
            for tx_index in 0..TXS {
                let before = db.stats().writes_since_snapshot;
                let started = Instant::now();
                let mut tx = db.transaction().unwrap();
                for op in 0..OPS {
                    let id = format!("ord_{tx_index}_{op}");
                    tx.run(&format!(r#"insert orders [{{id:"{id}",user_id:"u1",total:{}}}]"#, op * 10)).unwrap();
                }
                tx.commit().unwrap();
                let elapsed_ns = started.elapsed().as_nanos().try_into().unwrap();
                let after = db.stats().writes_since_snapshot;
                let checkpoint = after == 0;
                assert_eq!(after, if before + 1 == lin::persist::SNAPSHOT_EVERY { 0 } else { before + 1 });
                samples.push(Sample { transaction: tx_index, elapsed_ns, checkpoint: Some(checkpoint), writes_before: Some(before), writes_after: Some(after) });
            }
            verify_lin(&db);
            assert_eq!(samples.iter().filter(|s| s.checkpoint == Some(true)).count(), 31);
            // Normal close/checkpoint is outside the operation timer, as in the original test.
            drop(db);
            let db = Db::open_with(&dir, OpenOpts { sync: SyncMode::Normal, cold: false }).unwrap();
            verify_lin(&db);
            drop(db);
        }
        "sqlite" => {
            let path = dir.join("db.sqlite");
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; CREATE TABLE users(id TEXT PRIMARY KEY,email TEXT); CREATE TABLE orders(id TEXT PRIMARY KEY,user_id TEXT,total INT); INSERT INTO users VALUES('u1','u1@test.com');").unwrap();
            let journal: String = conn.query_row("PRAGMA journal_mode", [], |r|r.get(0)).unwrap();
            let sync: i64 = conn.query_row("PRAGMA synchronous", [], |r|r.get(0)).unwrap();
            let auto: i64 = conn.query_row("PRAGMA wal_autocheckpoint", [], |r|r.get(0)).unwrap();
            assert_eq!(journal, "wal"); assert_eq!(sync, 1); assert_eq!(auto, 1000);
            sqlite_settings = Some(serde_json::json!({"journal":journal,"synchronous":sync,"wal_autocheckpoint_pages":auto}));
            for tx_index in 0..TXS {
                let started = Instant::now();
                let tx = conn.unchecked_transaction().unwrap();
                for op in 0..OPS {
                    let id = format!("ord_{tx_index}_{op}");
                    tx.execute("INSERT INTO orders VALUES(?,?,?)",rusqlite::params![id,"u1",(op*10)as i64]).unwrap();
                }
                tx.commit().unwrap();
                let elapsed_ns = started.elapsed().as_nanos().try_into().unwrap();
                samples.push(Sample { transaction: tx_index, elapsed_ns, checkpoint: None, writes_before: None, writes_after: None });
            }
            conn.close().unwrap();
            let conn = Connection::open(path).unwrap();
            let mut statement = conn.prepare("SELECT id,user_id,total FROM orders ORDER BY id").unwrap();
            let actual: Vec<(String,String,i64)> = statement.query_map([],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap().map(Result::unwrap).collect();
            let mut expected: Vec<_> = (0..TXS).flat_map(|tx|(0..OPS).map(move|op|(format!("ord_{tx}_{op}"),"u1".to_owned(),(op*10)as i64))).collect();
            expected.sort(); assert_eq!(actual, expected);
            assert_eq!(conn.query_row("SELECT email FROM users WHERE id='u1'",[],|r|r.get::<_,String>(0)).unwrap(),"u1@test.com");
        }
        _ => panic!("unknown engine"),
    }
    println!("{}", serde_json::json!({"engine":engine,"transactions":TXS,"operations_per_transaction":OPS,"full_values_after_reopen":true,"sqlite_settings":sqlite_settings,"samples":samples}));
}
