use lin::{Db, OpenOpts, SyncMode};
use rusqlite::Connection;
use std::time::Instant;

#[test]
fn bench_transactions() {
    println!("\n=== LIN vs SQLITE TRANSACTION BENCHMARKS ===");

    // 1. In-memory tiny transaction (1 op)
    {
        let mut lin_db = Db::empty();

        let sqlite_conn = Connection::open_in_memory().unwrap();
        sqlite_conn
            .execute("CREATE TABLE users (id TEXT PRIMARY KEY, email TEXT)", [])
            .unwrap();

        let iters = 2_000;

        // SQLite 1-op transaction commit
        let start = Instant::now();
        for i in 0..iters {
            let tx = sqlite_conn.unchecked_transaction().unwrap();
            tx.execute(
                "INSERT INTO users VALUES (?, ?)",
                rusqlite::params![format!("id_{i}"), "alice@example.com"],
            )
            .unwrap();
            tx.commit().unwrap();
        }
        let sqlite_1op_commit = start.elapsed();
        let sqlite_1op_commit_ns = sqlite_1op_commit.as_nanos() / iters as u128;

        // Lin 1-op transaction commit (ad-hoc parse & run)
        let start = Instant::now();
        for i in 0..iters {
            let mut tx = lin_db.transaction().unwrap();
            tx.run(&format!(
                r#"insert users [{{ id: "id_{i}", email: "alice@example.com" }}]"#
            ))
            .unwrap();
            tx.commit().unwrap();
        }
        let lin_1op_commit = start.elapsed();
        let lin_1op_commit_ns = lin_1op_commit.as_nanos() / iters as u128;

        println!("--- 1. In-Memory Tiny Transaction (1 write per tx, 2,000 iterations) ---");
        println!(
            "Lin (dynamic AST):  {:>6} ns/tx  ({:.0} tx/sec)",
            lin_1op_commit_ns,
            1e9 / lin_1op_commit_ns as f64
        );
        println!(
            "SQLite (params):    {:>6} ns/tx  ({:.0} tx/sec)",
            sqlite_1op_commit_ns,
            1e9 / sqlite_1op_commit_ns as f64
        );
    }

    // 2. Multi-write transaction (10 writes per tx)
    {
        let mut lin_db = Db::empty();
        lin_db
            .run(r#"insert users [{ id: "u1", email: "u1@test.com" }]"#)
            .unwrap();

        let sqlite_conn = Connection::open_in_memory().unwrap();
        sqlite_conn
            .execute("CREATE TABLE users (id TEXT PRIMARY KEY, email TEXT)", [])
            .unwrap();
        sqlite_conn
            .execute(
                "CREATE TABLE orders (id TEXT PRIMARY KEY, user_id TEXT, total INT)",
                [],
            )
            .unwrap();
        sqlite_conn
            .execute("INSERT INTO users VALUES ('u1', 'u1@test.com')", [])
            .unwrap();

        let tx_count = 1_000;
        let ops_per_tx = 10;

        // SQLite 10-op transaction commit
        let start = Instant::now();
        for tx_idx in 0..tx_count {
            let tx = sqlite_conn.unchecked_transaction().unwrap();
            for op in 0..ops_per_tx {
                let id = format!("ord_{}_{}", tx_idx, op);
                tx.execute(
                    "INSERT INTO orders VALUES (?, ?, ?)",
                    rusqlite::params![id, "u1", op * 10],
                )
                .unwrap();
            }
            tx.commit().unwrap();
        }
        let sqlite_10op = start.elapsed();
        let sqlite_10op_ns = sqlite_10op.as_nanos() / tx_count as u128;

        // Lin 10-op transaction commit
        let start = Instant::now();
        for tx_idx in 0..tx_count {
            let mut tx = lin_db.transaction().unwrap();
            for op in 0..ops_per_tx {
                let id = format!("ord_{}_{}", tx_idx, op);
                tx.run(&format!(
                    r#"insert orders [{{ id: "{id}", user_id: "u1", total: {} }}]"#,
                    op * 10
                ))
                .unwrap();
            }
            tx.commit().unwrap();
        }
        let lin_10op = start.elapsed();
        let lin_10op_ns = lin_10op.as_nanos() / tx_count as u128;

        println!(
            "\n--- 2. In-Memory Multi-Write Transaction (10 writes per tx, 1,000 transactions) ---"
        );
        println!(
            "Lin:    {:>6} ns/tx  ({:.0} tx/sec, {:.0} ops/sec)",
            lin_10op_ns,
            1e9 / lin_10op_ns as f64,
            1e9 * ops_per_tx as f64 / lin_10op_ns as f64
        );
        println!(
            "SQLite: {:>6} ns/tx  ({:.0} tx/sec, {:.0} ops/sec)",
            sqlite_10op_ns,
            1e9 / sqlite_10op_ns as f64,
            1e9 * ops_per_tx as f64 / sqlite_10op_ns as f64
        );
    }

    // 3. Transaction Rollback (10 writes per tx, aborted)
    {
        let mut lin_db = Db::empty();
        lin_db
            .run(r#"insert users [{ id: "u1", email: "u1@test.com" }]"#)
            .unwrap();

        let sqlite_conn = Connection::open_in_memory().unwrap();
        sqlite_conn
            .execute("CREATE TABLE users (id TEXT PRIMARY KEY, email TEXT)", [])
            .unwrap();
        sqlite_conn
            .execute(
                "CREATE TABLE orders (id TEXT PRIMARY KEY, user_id TEXT, total INT)",
                [],
            )
            .unwrap();
        sqlite_conn
            .execute("INSERT INTO users VALUES ('u1', 'u1@test.com')", [])
            .unwrap();

        let tx_count = 1_000;
        let ops_per_tx = 10;

        // SQLite rollback
        let start = Instant::now();
        for tx_idx in 0..tx_count {
            let tx = sqlite_conn.unchecked_transaction().unwrap();
            for op in 0..ops_per_tx {
                let id = format!("ord_{}_{}", tx_idx, op);
                tx.execute(
                    "INSERT INTO orders VALUES (?, ?, ?)",
                    rusqlite::params![id, "u1", op * 10],
                )
                .unwrap();
            }
            tx.rollback().unwrap();
        }
        let sqlite_rb = start.elapsed();
        let sqlite_rb_ns = sqlite_rb.as_nanos() / tx_count as u128;

        // Lin rollback
        let start = Instant::now();
        for tx_idx in 0..tx_count {
            let mut tx = lin_db.transaction().unwrap();
            for op in 0..ops_per_tx {
                let id = format!("ord_{}_{}", tx_idx, op);
                tx.run(&format!(
                    r#"insert orders [{{ id: "{id}", user_id: "u1", total: {} }}]"#,
                    op * 10
                ))
                .unwrap();
            }
            tx.rollback().unwrap();
        }
        let lin_rb = start.elapsed();
        let lin_rb_ns = lin_rb.as_nanos() / tx_count as u128;

        println!("\n--- 3. In-Memory Transaction Rollback (10 writes per tx, rolled back) ---");
        println!(
            "Lin:    {:>6} ns/tx  ({:.0} tx/sec)",
            lin_rb_ns,
            1e9 / lin_rb_ns as f64
        );
        println!(
            "SQLite: {:>6} ns/tx  ({:.0} tx/sec)",
            sqlite_rb_ns,
            1e9 / sqlite_rb_ns as f64
        );
    }

    // 4. Multi-statement with Reads & Writes (interactive business logic)
    {
        let mut lin_db = Db::empty();
        lin_db.run("col accounts {id: text, balance: i64}").unwrap();
        lin_db
            .run(r#"insert accounts [{ id: "acc1", balance: 1000 }, { id: "acc2", balance: 500 }]"#)
            .unwrap();

        let sqlite_conn = Connection::open_in_memory().unwrap();
        sqlite_conn
            .execute(
                "CREATE TABLE accounts (id TEXT PRIMARY KEY, balance INT)",
                [],
            )
            .unwrap();
        sqlite_conn
            .execute(
                "INSERT INTO accounts VALUES ('acc1', 1000), ('acc2', 500)",
                [],
            )
            .unwrap();

        let iters = 5_000;

        // SQLite transfer transaction: read acc1 -> check balance -> update acc1 -> update acc2
        let start = Instant::now();
        for _ in 0..iters {
            let tx = sqlite_conn.unchecked_transaction().unwrap();
            let b1: i64 = tx
                .query_row("SELECT balance FROM accounts WHERE id = 'acc1'", [], |r| {
                    r.get(0)
                })
                .unwrap();
            if b1 >= 10 {
                tx.execute(
                    "UPDATE accounts SET balance = balance - 10 WHERE id = 'acc1'",
                    [],
                )
                .unwrap();
                tx.execute(
                    "UPDATE accounts SET balance = balance + 10 WHERE id = 'acc2'",
                    [],
                )
                .unwrap();
            }
            tx.commit().unwrap();
        }
        let sqlite_transfer_ns = start.elapsed().as_nanos() / iters as u128;

        // Lin transfer transaction: read acc1 -> update acc1 -> update acc2
        let start = Instant::now();
        for _ in 0..iters {
            let mut tx = lin_db.transaction().unwrap();
            let b1 = tx
                .scalar(r#"accounts | id == "acc1" | {balance}"#)
                .unwrap()
                .as_i64()
                .unwrap();
            if b1 >= 10 {
                tx.execute(&format!(
                    r#"update accounts[id == "acc1"] cas each {{ balance: {} }}"#,
                    b1 - 10
                ))
                .unwrap();
                tx.execute(r#"update accounts[id == "acc2"] cas each { balance: 510 }"#)
                    .unwrap();
            }
            tx.commit().unwrap();
        }
        let lin_transfer_ns = start.elapsed().as_nanos() / iters as u128;

        println!(
            "\n--- 4. Interactive Bank Transfer (Read -> Validate -> 2 Updates -> Commit) ---"
        );
        println!(
            "Lin:    {:>6} ns/tx  ({:.0} tx/sec)",
            lin_transfer_ns,
            1e9 / lin_transfer_ns as f64
        );
        println!(
            "SQLite: {:>6} ns/tx  ({:.0} tx/sec)",
            sqlite_transfer_ns,
            1e9 / sqlite_transfer_ns as f64
        );
    }

    // 5. Durable Disk WAL Transactions (SyncMode::Normal vs SQLite WAL + synchronous=NORMAL)
    {
        let pid = std::process::id();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let lin_dir = std::env::temp_dir().join(format!("lin-bench-tx-{pid}-{now}"));
        let _ = std::fs::remove_dir_all(&lin_dir);
        let _ = std::fs::create_dir_all(&lin_dir);

        let mut lin_db = Db::open_with(
            lin_dir.join("lin.db"),
            OpenOpts {
                sync: SyncMode::Normal,
                ..Default::default()
            },
        )
        .unwrap();
        lin_db
            .run(r#"insert users [{ id: "u1", email: "u1@test.com" }]"#)
            .unwrap();

        let sqlite_path = std::env::temp_dir().join(format!("sqlite-bench-tx-{pid}-{now}.db"));
        let _ = std::fs::remove_file(&sqlite_path);
        let sqlite_conn = Connection::open(&sqlite_path).unwrap();
        sqlite_conn
            .execute_batch(
                "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             CREATE TABLE users (id TEXT PRIMARY KEY, email TEXT);
             CREATE TABLE orders (id TEXT PRIMARY KEY, user_id TEXT, total INT);
             INSERT INTO users VALUES ('u1', 'u1@test.com');",
            )
            .unwrap();

        let tx_count = 1_000;
        let ops_per_tx = 5;

        // SQLite durable commit
        let start = Instant::now();
        for tx_idx in 0..tx_count {
            let tx = sqlite_conn.unchecked_transaction().unwrap();
            for op in 0..ops_per_tx {
                let id = format!("ord_{}_{}", tx_idx, op);
                tx.execute(
                    "INSERT INTO orders VALUES (?, ?, ?)",
                    rusqlite::params![id, "u1", op * 10],
                )
                .unwrap();
            }
            tx.commit().unwrap();
        }
        let sqlite_durable_ns = start.elapsed().as_nanos() / tx_count as u128;

        // Lin durable commit
        let start = Instant::now();
        for tx_idx in 0..tx_count {
            let mut tx = lin_db.transaction().unwrap();
            for op in 0..ops_per_tx {
                let id = format!("ord_{}_{}", tx_idx, op);
                tx.run(&format!(
                    r#"insert orders [{{ id: "{id}", user_id: "u1", total: {} }}]"#,
                    op * 10
                ))
                .unwrap();
            }
            tx.commit().unwrap();
        }
        let lin_durable_ns = start.elapsed().as_nanos() / tx_count as u128;

        let _ = std::fs::remove_dir_all(&lin_dir);
        let _ = std::fs::remove_file(&sqlite_path);

        println!(
            "\n--- 5. Durable Disk WAL Transactions (5 writes per tx, 1,000 transactions, WAL on disk) ---"
        );
        println!(
            "Lin (WAL SyncMode::Normal): {:>6} ns/tx  ({:.0} tx/sec, {:.0} ops/sec)",
            lin_durable_ns,
            1e9 / lin_durable_ns as f64,
            1e9 * ops_per_tx as f64 / lin_durable_ns as f64
        );
        println!(
            "SQLite (WAL sync=NORMAL):   {:>6} ns/tx  ({:.0} tx/sec, {:.0} ops/sec)",
            sqlite_durable_ns,
            1e9 / sqlite_durable_ns as f64,
            1e9 * ops_per_tx as f64 / sqlite_durable_ns as f64
        );
    }
}
