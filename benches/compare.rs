//! Comparative hot-path benches: Lin vs SQLite vs DuckDB vs Postgres vs MySQL
//! (+ HashMap point-get baseline).
//!
//! Warm reads use a shared fixture (setup excluded from timing).
//! Bulk inserts: schema/index in setup; only row writes are timed.
//!
//! Postgres / MySQL: optional peers. Set `LIN_BENCH_PG_URL` / `LIN_BENCH_MYSQL_URL`,
//! or leave unset to probe documented local Docker ports; skip gracefully if unreachable.
//!
//! Fairness notes (also in README):
//! - Same N, same columns (id/uri/wing/title/ts/body).
//! - Lin point_get / filter / insert use **prepare once, run many** (like SQL prepared).
//! - Filter benches: Lin `count by …` vs SQL `COUNT(*)` (same shape: return a count).
//! - Point get / equality / range / LIKE-style substring are comparable.
//! - Join: inner `orders ⋈ users` (FK `orders.user_id → users.id`) — Lin `run_batch`
//!   (SoA + [`RecordBatch`]), Lin lazy cursor, SQL `INNER JOIN`. Filter+join: `total > 100`.
//!   Row-API `run` still materializes `Vec<Row>` from the same path.
//!   DuckDB may still win (mature OLAP); Lin now has an in-process columnar join path.
//! - FTS cases isolate selective/common/miss `FtsSeek`, hybrid, and index rebuild.
//! - Phase cases decompose insert variants, cursor open/scan/join, and reopen rebuilds.
//! - Lin `hop` and CAS are not claimed here.
//! - SQL `LIKE '%wal%'` ≈ Lin `title ~ "wal"` (substring), not `has` / FTS.
//! - Server DBs are not in-process; network/IPC cost is part of their number.

use std::collections::HashMap;
use std::hint::black_box;
use std::time::Duration;

use airbug_bench::{Config, DropPolicy, Fixture, Suite};
use lin::Queryable as LinQueryable;
use lin::query::pred;
use mysql::prelude::Queryable;

const N: usize = 10_000;
const INSERT_1K: usize = 1_000;
const INSERT_10K: usize = 10_000;
/// Probe row index inside the seeded set (wing=rag, title contains "wal").
const PROBE: usize = 20;
const JOIN_USERS: usize = 1_000;
const JOIN_ORDERS: usize = 10_000;
const JOIN_FILTER_N: usize = JOIN_ORDERS / 2;
const JOIN_INNER_SQL: &str =
    "SELECT o.id, u.email, o.total FROM orders o INNER JOIN users u ON o.user_id = u.id";
const JOIN_FILTER_SQL: &str = "SELECT o.id, u.email, o.total FROM orders o INNER JOIN users u ON o.user_id = u.id WHERE o.total > 100";

/// Prefer env, else probe these (lin compose → dbill compose → local brew defaults).
const PG_CANDIDATES: &[&str] = &[
    "postgresql://lin:lin@127.0.0.1:55432/lin", // docker-compose.bench.yml
    "postgresql://postgres@127.0.0.1:5432/postgres", // brew postgresql@17 (trust/peer)
];
const MYSQL_CANDIDATES: &[&str] = &[
    "mysql://lin:lin@127.0.0.1:53306/lin", // docker-compose.bench.yml
    "mysql://dbill:dbill@127.0.0.1:33306/dbill_smoke", // Sources/dbill/docker-compose.yml
];

#[derive(Clone)]
struct Doc {
    id: String,
    uri: String,
    wing: String,
    title: String,
    ts: i64,
    body: String,
}

fn docs(n: usize) -> Vec<Doc> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    (0..n)
        .map(|i| {
            let wal = i % 10 == 0;
            Doc {
                id: format!("d-{i}"),
                uri: format!("bench://{i}"),
                wing: if i % 2 == 0 { "rag" } else { "sys" }.into(),
                title: if wal {
                    format!("doc {i} wal note")
                } else {
                    format!("doc {i} plain")
                },
                ts: if i % 2 == 0 {
                    now - 86_400_000
                } else {
                    now - 30 * 86_400_000
                },
                body: format!("body {i}"),
            }
        })
        .collect()
}

fn escape_lin(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

fn lin_insert_src(rows: &[Doc]) -> String {
    let mut out = String::from("insert docs [\n");
    for (i, d) in rows.iter().enumerate() {
        if i > 0 {
            out.push_str(",\n");
        }
        let ago = if i % 2 == 0 { "ago 1d" } else { "ago 30d" };
        out.push_str(&format!(
            r#"  {{ id: "{}", uri: "{}", title: "{}", layer: "wiki", wing: "{}", body: "{}", ts: {} }}"#,
            escape_lin(&d.id),
            escape_lin(&d.uri),
            escape_lin(&d.title),
            escape_lin(&d.wing),
            escape_lin(&d.body),
            ago
        ));
    }
    out.push_str("\n]");
    out
}

fn range_cutoff_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
        - 7 * 86_400_000
}

fn env_url(env_key: &str) -> Option<String> {
    std::env::var(env_key).ok().and_then(|u| {
        let t = u.trim();
        if t.is_empty() {
            None
        } else {
            Some(t.to_string())
        }
    })
}

fn try_pg_client(url: &str) -> Result<postgres::Client, String> {
    postgres::Client::connect(url, postgres::NoTls).map_err(|e| e.to_string())
}

fn try_mysql_conn(url: &str) -> Result<mysql::Conn, String> {
    let opts = mysql::Opts::from_url(url).map_err(|e| e.to_string())?;
    mysql::Conn::new(opts).map_err(|e| e.to_string())
}

fn connect_pg() -> Option<(String, postgres::Client)> {
    let mut tried = Vec::new();
    let mut urls: Vec<String> = Vec::new();
    if let Some(u) = env_url("LIN_BENCH_PG_URL") {
        urls.push(u);
    } else {
        urls.extend(PG_CANDIDATES.iter().map(|s| (*s).to_string()));
    }
    for url in urls {
        match try_pg_client(&url) {
            Ok(c) => return Some((url, c)),
            Err(e) => tried.push(format!("{url} ({e})")),
        }
    }
    eprintln!(
        "skip postgres: no reachable server\n  tried: {}\n  set LIN_BENCH_PG_URL or:\n  docker compose -f docker-compose.bench.yml up -d postgres\n  # brew postgresql@17 via Sources/ppduster macos-stack-postgres; start service for :5432",
        tried.join("; ")
    );
    None
}

fn connect_mysql() -> Option<(String, mysql::Conn)> {
    let mut tried = Vec::new();
    let mut urls: Vec<String> = Vec::new();
    if let Some(u) = env_url("LIN_BENCH_MYSQL_URL") {
        urls.push(u);
    } else {
        urls.extend(MYSQL_CANDIDATES.iter().map(|s| (*s).to_string()));
    }
    for url in urls {
        match try_mysql_conn(&url) {
            Ok(c) => return Some((url, c)),
            Err(e) => tried.push(format!("{url} ({e})")),
        }
    }
    eprintln!(
        "skip mysql: no reachable server\n  tried: {}\n  set LIN_BENCH_MYSQL_URL or:\n  docker compose -f docker-compose.bench.yml up -d mysql\n  # or sibling dbill: docker compose -f ../dbill/docker-compose.yml up -d mysql",
        tried.join("; ")
    );
    None
}

struct LinWarm {
    db: lin::Db,
    point_get: lin::Prepared,
    filter_eq: lin::Prepared,
    filter_range: lin::Prepared,
    text_substr: lin::Prepared,
    fts_selective: lin::Prepared,
    fts_common: lin::Prepared,
    fts_miss: lin::Prepared,
    fts_hybrid: lin::Prepared,
    materialize: lin::Prepared,
}

fn seed_lin(n: usize) -> LinWarm {
    let rows = docs(n);
    let mut db = lin::Db::empty();
    db.run("index docs [wing, ts]").expect("lin index");
    for chunk in rows.chunks(500) {
        db.run(&lin_insert_src(chunk)).expect("lin seed");
    }
    let probe_id = &rows[PROBE].id;
    let point_get = db
        .prepare(&format!(
            r#"docs | id == "{}" | {{ id, title }}"#,
            escape_lin(probe_id)
        ))
        .expect("lin prepare point_get");
    let filter_eq = db
        .prepare(r#"docs | wing == "rag" | count by wing"#)
        .expect("lin prepare filter_eq");
    let filter_range = db
        .prepare(r#"docs | wing == "rag" and ts > ago 7d | count by wing"#)
        .expect("lin prepare filter_range");
    let text_substr = db
        .prepare(r#"docs | title ~ "wal" | count"#)
        .expect("lin prepare text_substr");
    let fts_selective = db
        .prepare(r#"docs | search lex "9999" | take 20"#)
        .expect("lin prepare selective FTS");
    let fts_common = db
        .prepare(r#"docs | search lex "wal" | take 20"#)
        .expect("lin prepare common FTS");
    let fts_miss = db
        .prepare(r#"docs | search lex "not-in-corpus" | take 20"#)
        .expect("lin prepare miss FTS");
    let fts_hybrid = db
        .prepare(r#"docs | search "wal" | take 20"#)
        .expect("lin prepare hybrid FTS");
    let materialize = db
        .prepare(r#"docs | wing == "rag" | { id, title } | take all"#)
        .expect("lin prepare materialize");
    let eq_plan = db
        .explain_as(r#"docs | wing == "rag" | count by wing"#, None)
        .expect("explain eq");
    assert!(
        eq_plan.contains("index=docs[wing,ts]"),
        "expected IndexSeek, got:\n{eq_plan}"
    );
    let fts_plan = db
        .explain_as(r#"docs | search lex "wal" | take 20"#, None)
        .expect("explain FTS");
    assert!(
        fts_plan.contains("FtsSeek"),
        "expected FtsSeek, got:\n{fts_plan}"
    );
    let mat = materialize.run(&mut db).expect("lin materialize sanity");
    assert_eq!(mat.done.n, n / 2, "materialize should take all rag rows");
    LinWarm {
        db,
        point_get,
        filter_eq,
        filter_range,
        text_substr,
        fts_selective,
        fts_common,
        fts_miss,
        fts_hybrid,
        materialize,
    }
}

struct SqlWarm {
    conn: rusqlite::Connection,
    probe_id: String,
}

fn seed_sqlite(n: usize) -> SqlWarm {
    let rows = docs(n);
    let conn = rusqlite::Connection::open_in_memory().expect("sqlite open");
    conn.execute_batch(
        "CREATE TABLE docs (
            id TEXT PRIMARY KEY,
            uri TEXT UNIQUE NOT NULL,
            wing TEXT NOT NULL,
            title TEXT NOT NULL,
            ts INTEGER NOT NULL,
            body TEXT NOT NULL
         );
         CREATE INDEX docs_wing_ts ON docs(wing, ts);",
    )
    .expect("sqlite schema");
    {
        let mut stmt = conn
            .prepare("INSERT INTO docs (id, uri, wing, title, ts, body) VALUES (?1,?2,?3,?4,?5,?6)")
            .expect("sqlite prepare");
        for d in &rows {
            stmt.execute(rusqlite::params![
                d.id, d.uri, d.wing, d.title, d.ts, d.body
            ])
            .expect("sqlite seed");
        }
    }
    {
        let _ = conn.prepare_cached("SELECT COUNT(*) FROM docs WHERE id = ?1");
        let _ = conn.prepare_cached("SELECT COUNT(*) FROM docs WHERE wing = 'rag'");
        let _ = conn.prepare_cached("SELECT COUNT(*) FROM docs WHERE wing = 'rag' AND ts > ?1");
        let _ = conn.prepare_cached("SELECT COUNT(*) FROM docs WHERE title LIKE '%wal%'");
        let _ = conn.prepare_cached("SELECT id, title FROM docs WHERE wing = 'rag'");
    }
    SqlWarm {
        probe_id: rows[PROBE].id.clone(),
        conn,
    }
}

struct DuckWarm {
    conn: duckdb::Connection,
    probe_id: String,
}

fn seed_duck(n: usize) -> DuckWarm {
    let rows = docs(n);
    let conn = duckdb::Connection::open_in_memory().expect("duckdb open");
    conn.execute_batch(
        "CREATE TABLE docs (
            id VARCHAR PRIMARY KEY,
            uri VARCHAR UNIQUE NOT NULL,
            wing VARCHAR NOT NULL,
            title VARCHAR NOT NULL,
            ts BIGINT NOT NULL,
            body VARCHAR NOT NULL
         );
         CREATE INDEX docs_wing_ts ON docs(wing, ts);",
    )
    .expect("duckdb schema");
    {
        let mut stmt = conn
            .prepare("INSERT INTO docs (id, uri, wing, title, ts, body) VALUES (?,?,?,?,?,?)")
            .expect("duck prepare");
        for d in &rows {
            stmt.execute(duckdb::params![d.id, d.uri, d.wing, d.title, d.ts, d.body])
                .expect("duck seed");
        }
    }
    DuckWarm {
        probe_id: rows[PROBE].id.clone(),
        conn,
    }
}

struct MapWarm {
    by_id: HashMap<String, Doc>,
    probe_id: String,
}

fn seed_map(n: usize) -> MapWarm {
    let rows = docs(n);
    let probe_id = rows[PROBE].id.clone();
    let mut by_id = HashMap::with_capacity(n);
    for d in rows {
        by_id.insert(d.id.clone(), d);
    }
    MapWarm { by_id, probe_id }
}

struct PgWarm {
    client: postgres::Client,
    probe_id: String,
    point_get: postgres::Statement,
    filter_eq: postgres::Statement,
    filter_range: postgres::Statement,
    text_substr: postgres::Statement,
    materialize: postgres::Statement,
}

fn seed_pg(n: usize, mut client: postgres::Client) -> PgWarm {
    let rows = docs(n);
    client
        .batch_execute(
            "DROP TABLE IF EXISTS docs;
             CREATE TABLE docs (
                id TEXT PRIMARY KEY,
                uri TEXT UNIQUE NOT NULL,
                wing TEXT NOT NULL,
                title TEXT NOT NULL,
                ts BIGINT NOT NULL,
                body TEXT NOT NULL
             );
             CREATE INDEX docs_wing_ts ON docs(wing, ts);",
        )
        .expect("pg schema");
    {
        let stmt = client
            .prepare("INSERT INTO docs (id, uri, wing, title, ts, body) VALUES ($1,$2,$3,$4,$5,$6)")
            .expect("pg prepare insert");
        for d in &rows {
            client
                .execute(&stmt, &[&d.id, &d.uri, &d.wing, &d.title, &d.ts, &d.body])
                .expect("pg seed");
        }
    }
    let point_get = client
        .prepare("SELECT COUNT(*)::bigint FROM docs WHERE id = $1")
        .expect("pg prepare point_get");
    let filter_eq = client
        .prepare("SELECT COUNT(*)::bigint FROM docs WHERE wing = 'rag'")
        .expect("pg prepare filter_eq");
    let filter_range = client
        .prepare("SELECT COUNT(*)::bigint FROM docs WHERE wing = 'rag' AND ts > $1")
        .expect("pg prepare filter_range");
    let text_substr = client
        .prepare("SELECT COUNT(*)::bigint FROM docs WHERE title LIKE '%wal%'")
        .expect("pg prepare text_substr");
    let materialize = client
        .prepare("SELECT id, title FROM docs WHERE wing = 'rag'")
        .expect("pg prepare materialize");
    PgWarm {
        client,
        probe_id: rows[PROBE].id.clone(),
        point_get,
        filter_eq,
        filter_range,
        text_substr,
        materialize,
    }
}

struct MysqlWarm {
    conn: mysql::Conn,
    probe_id: String,
    point_get: mysql::Statement,
    filter_eq: mysql::Statement,
    filter_range: mysql::Statement,
    text_substr: mysql::Statement,
    materialize: mysql::Statement,
}

fn seed_mysql(n: usize, mut conn: mysql::Conn) -> MysqlWarm {
    let rows = docs(n);
    conn.query_drop(
        "CREATE TABLE IF NOT EXISTS docs (
            id VARCHAR(64) PRIMARY KEY,
            uri VARCHAR(255) UNIQUE NOT NULL,
            wing VARCHAR(64) NOT NULL,
            title VARCHAR(255) NOT NULL,
            ts BIGINT NOT NULL,
            body TEXT NOT NULL,
            INDEX docs_wing_ts (wing, ts)
         )",
    )
    .expect("mysql schema create");
    conn.query_drop("TRUNCATE TABLE docs")
        .expect("mysql truncate");
    {
        let stmt = conn
            .prep("INSERT INTO docs (id, uri, wing, title, ts, body) VALUES (?,?,?,?,?,?)")
            .expect("mysql prepare insert");
        for d in &rows {
            conn.exec_drop(&stmt, (&d.id, &d.uri, &d.wing, &d.title, d.ts, &d.body))
                .expect("mysql seed");
        }
    }
    let point_get = conn
        .prep("SELECT COUNT(*) FROM docs WHERE id = ?")
        .expect("mysql prepare point_get");
    let filter_eq = conn
        .prep("SELECT COUNT(*) FROM docs WHERE wing = 'rag'")
        .expect("mysql prepare filter_eq");
    let filter_range = conn
        .prep("SELECT COUNT(*) FROM docs WHERE wing = 'rag' AND ts > ?")
        .expect("mysql prepare filter_range");
    let text_substr = conn
        .prep("SELECT COUNT(*) FROM docs WHERE title LIKE '%wal%'")
        .expect("mysql prepare text_substr");
    let materialize = conn
        .prep("SELECT id, title FROM docs WHERE wing = 'rag'")
        .expect("mysql prepare materialize");
    MysqlWarm {
        conn,
        probe_id: rows[PROBE].id.clone(),
        point_get,
        filter_eq,
        filter_range,
        text_substr,
        materialize,
    }
}

struct JoinUser {
    id: String,
    email: String,
}

struct JoinOrder {
    id: String,
    user_id: String,
    total: f64,
}

fn join_data() -> (Vec<JoinUser>, Vec<JoinOrder>) {
    let users: Vec<JoinUser> = (0..JOIN_USERS)
        .map(|i| JoinUser {
            id: format!("u-{i}"),
            email: format!("u{i}@b.dev"),
        })
        .collect();
    let orders: Vec<JoinOrder> = (0..JOIN_ORDERS)
        .map(|i| JoinOrder {
            id: format!("o-{i}"),
            user_id: format!("u-{}", i % JOIN_USERS),
            total: ((i % 200) + 1) as f64,
        })
        .collect();
    (users, orders)
}

fn lin_insert_users(rows: &[JoinUser]) -> String {
    let mut out = String::from("insert users [\n");
    for (i, u) in rows.iter().enumerate() {
        if i > 0 {
            out.push_str(",\n");
        }
        out.push_str(&format!(
            r#"  {{ id: "{}", email: "{}" }}"#,
            escape_lin(&u.id),
            escape_lin(&u.email)
        ));
    }
    out.push_str("\n]");
    out
}

fn lin_insert_orders(rows: &[JoinOrder]) -> String {
    let mut out = String::from("insert orders [\n");
    for (i, o) in rows.iter().enumerate() {
        if i > 0 {
            out.push_str(",\n");
        }
        out.push_str(&format!(
            r#"  {{ id: "{}", user_id: "{}", total: {}, ts: ago 1d }}"#,
            escape_lin(&o.id),
            escape_lin(&o.user_id),
            o.total
        ));
    }
    out.push_str("\n]");
    out
}

struct JoinLin {
    db: lin::Db,
    inner: lin::Prepared,
    filter: lin::Prepared,
    inner_q: LinQueryable,
    filter_q: LinQueryable,
    orders_q: LinQueryable,
}

fn seed_join_lin() -> JoinLin {
    let (users, orders) = join_data();
    let mut db = lin::Db::empty();
    db.run("index orders [user_id]").expect("lin orders index");
    for chunk in users.chunks(500) {
        db.run(&lin_insert_users(chunk)).expect("lin seed users");
    }
    for chunk in orders.chunks(500) {
        db.run(&lin_insert_orders(chunk)).expect("lin seed orders");
    }
    let inner = db
        .prepare(r#"orders | join users on user_id | { id, users.email, total } | take all"#)
        .expect("lin prepare join inner");
    let filter = db
        .prepare(
            r#"orders | total > 100 | join users on user_id | { id, users.email, total } | take all"#,
        )
        .expect("lin prepare join filter");
    let got = inner.run(&mut db).expect("lin join sanity");
    assert_eq!(got.done.n, JOIN_ORDERS, "inner join should keep all orders");
    let got_f = filter.run(&mut db).expect("lin join filter sanity");
    assert_eq!(got_f.done.n, JOIN_FILTER_N);
    let batch_n = inner.run_batch(&mut db).expect("lin join batch").n();
    assert_eq!(batch_n, JOIN_ORDERS);
    let inner_q = LinQueryable::from("orders")
        .join("users", "user_id")
        .select(["id", "users.email", "total"])
        .take_all();
    let filter_q = LinQueryable::from("orders")
        .filter(pred::gt("total", 100.0))
        .join("users", "user_id")
        .select(["id", "users.email", "total"])
        .take_all();
    let orders_q = LinQueryable::from("orders")
        .select(["id", "user_id", "total"])
        .take_all();
    let cur_n = inner_q
        .cursor(&db)
        .expect("lin join cursor")
        .map(|r| r.expect("row"))
        .count();
    assert_eq!(cur_n, JOIN_ORDERS);
    assert!(inner_q.cursor(&db).expect("lazy").is_lazy());
    JoinLin {
        db,
        inner,
        filter,
        inner_q,
        filter_q,
        orders_q,
    }
}

fn consume_join_cursor(db: &lin::Db, q: &LinQueryable) -> usize {
    let mut n = 0usize;
    for row in q.cursor(db).expect("lin join cursor") {
        black_box(row.expect("row"));
        n += 1;
    }
    n
}

fn consume_projected_cursor(db: &lin::Db, q: &LinQueryable) -> usize {
    let mut cursor = q.cursor(db).expect("lin projected cursor");
    let mut n = 0usize;
    while let Some(row) = cursor.next_projected() {
        black_box(row.expect("projected row"));
        n += 1;
    }
    n
}

struct JoinSql {
    conn: rusqlite::Connection,
}

fn seed_join_sqlite() -> JoinSql {
    let (users, orders) = join_data();
    let conn = rusqlite::Connection::open_in_memory().expect("sqlite open");
    conn.execute_batch(
        "CREATE TABLE users (id TEXT PRIMARY KEY, email TEXT NOT NULL);
         CREATE TABLE orders (
            id TEXT PRIMARY KEY,
            user_id TEXT NOT NULL,
            total REAL NOT NULL
         );
         CREATE INDEX orders_user_id ON orders(user_id);",
    )
    .expect("sqlite join schema");
    {
        let mut u = conn
            .prepare("INSERT INTO users (id, email) VALUES (?1, ?2)")
            .expect("sqlite users");
        for x in &users {
            u.execute(rusqlite::params![x.id, x.email])
                .expect("sqlite user");
        }
        let mut o = conn
            .prepare("INSERT INTO orders (id, user_id, total) VALUES (?1, ?2, ?3)")
            .expect("sqlite orders");
        for x in &orders {
            o.execute(rusqlite::params![x.id, x.user_id, x.total])
                .expect("sqlite order");
        }
    }
    let _ = conn.prepare_cached(JOIN_INNER_SQL);
    let _ = conn.prepare_cached(JOIN_FILTER_SQL);
    JoinSql { conn }
}

fn sqlite_join_n(conn: &rusqlite::Connection, sql: &str) -> usize {
    let mut stmt = conn.prepare_cached(sql).expect("sqlite join prep");
    let mut rows = stmt.query([]).expect("sqlite join");
    let mut n = 0usize;
    while let Some(row) = rows.next().expect("sqlite row") {
        let id: String = row.get(0).expect("id");
        let email: String = row.get(1).expect("email");
        let total: f64 = row.get(2).expect("total");
        black_box((id, email, total));
        n += 1;
    }
    n
}

struct JoinDuck {
    conn: duckdb::Connection,
}

fn seed_join_duck() -> JoinDuck {
    let (users, orders) = join_data();
    let conn = duckdb::Connection::open_in_memory().expect("duckdb open");
    conn.execute_batch(
        "CREATE TABLE users (id VARCHAR PRIMARY KEY, email VARCHAR NOT NULL);
         CREATE TABLE orders (
            id VARCHAR PRIMARY KEY,
            user_id VARCHAR NOT NULL,
            total DOUBLE NOT NULL
         );
         CREATE INDEX orders_user_id ON orders(user_id);",
    )
    .expect("duck join schema");
    {
        let mut u = conn
            .prepare("INSERT INTO users (id, email) VALUES (?, ?)")
            .expect("duck users");
        for x in &users {
            u.execute(duckdb::params![x.id, x.email])
                .expect("duck user");
        }
        let mut o = conn
            .prepare("INSERT INTO orders (id, user_id, total) VALUES (?, ?, ?)")
            .expect("duck orders");
        for x in &orders {
            o.execute(duckdb::params![x.id, x.user_id, x.total])
                .expect("duck order");
        }
    }
    JoinDuck { conn }
}

fn duck_join_n(conn: &duckdb::Connection, sql: &str) -> usize {
    let mut stmt = conn.prepare_cached(sql).expect("duck join prep");
    let mut rows = stmt.query([]).expect("duck join");
    let mut n = 0usize;
    while let Some(row) = rows.next().expect("duck row") {
        let id: String = row.get(0).expect("id");
        let email: String = row.get(1).expect("email");
        let total: f64 = row.get(2).expect("total");
        black_box((id, email, total));
        n += 1;
    }
    n
}

struct JoinPg {
    client: postgres::Client,
    inner: postgres::Statement,
    filter: postgres::Statement,
}

fn seed_join_pg(mut client: postgres::Client) -> JoinPg {
    let (users, orders) = join_data();
    client
        .batch_execute(
            "DROP TABLE IF EXISTS orders;
             DROP TABLE IF EXISTS users;
             CREATE TABLE users (id TEXT PRIMARY KEY, email TEXT NOT NULL);
             CREATE TABLE orders (
                id TEXT PRIMARY KEY,
                user_id TEXT NOT NULL,
                total DOUBLE PRECISION NOT NULL
             );
             CREATE INDEX orders_user_id ON orders(user_id);",
        )
        .expect("pg join schema");
    for x in &users {
        client
            .execute(
                "INSERT INTO users (id, email) VALUES ($1, $2)",
                &[&x.id, &x.email],
            )
            .expect("pg user");
    }
    for x in &orders {
        client
            .execute(
                "INSERT INTO orders (id, user_id, total) VALUES ($1, $2, $3)",
                &[&x.id, &x.user_id, &x.total],
            )
            .expect("pg order");
    }
    let inner = client.prepare(JOIN_INNER_SQL).expect("pg join inner");
    let filter = client.prepare(JOIN_FILTER_SQL).expect("pg join filter");
    JoinPg {
        client,
        inner,
        filter,
    }
}

fn pg_join_n(client: &mut postgres::Client, stmt: &postgres::Statement) -> usize {
    let rows = client.query(stmt, &[]).expect("pg join");
    let mut n = 0usize;
    for row in rows {
        let id: String = row.get(0);
        let email: String = row.get(1);
        let total: f64 = row.get(2);
        black_box((id, email, total));
        n += 1;
    }
    n
}

struct JoinMysql {
    conn: mysql::Conn,
    inner: mysql::Statement,
    filter: mysql::Statement,
}

fn seed_join_mysql(mut conn: mysql::Conn) -> JoinMysql {
    let (users, orders) = join_data();
    conn.query_drop("DROP TABLE IF EXISTS orders")
        .expect("mysql drop orders");
    conn.query_drop("DROP TABLE IF EXISTS users")
        .expect("mysql drop users");
    conn.query_drop("CREATE TABLE users (id VARCHAR(64) PRIMARY KEY, email VARCHAR(255) NOT NULL)")
        .expect("mysql users");
    conn.query_drop(
        "CREATE TABLE orders (
            id VARCHAR(64) PRIMARY KEY,
            user_id VARCHAR(64) NOT NULL,
            total DOUBLE NOT NULL,
            INDEX orders_user_id (user_id)
         )",
    )
    .expect("mysql orders");
    {
        let ust = conn
            .prep("INSERT INTO users (id, email) VALUES (?, ?)")
            .expect("mysql users prep");
        for x in &users {
            conn.exec_drop(&ust, (&x.id, &x.email)).expect("mysql user");
        }
        let ost = conn
            .prep("INSERT INTO orders (id, user_id, total) VALUES (?, ?, ?)")
            .expect("mysql orders prep");
        for x in &orders {
            conn.exec_drop(&ost, (&x.id, &x.user_id, x.total))
                .expect("mysql order");
        }
    }
    let inner = conn.prep(JOIN_INNER_SQL).expect("mysql join inner");
    let filter = conn.prep(JOIN_FILTER_SQL).expect("mysql join filter");
    JoinMysql {
        conn,
        inner,
        filter,
    }
}

fn mysql_join_n(conn: &mut mysql::Conn, stmt: &mysql::Statement) -> usize {
    let rows: Vec<(String, String, f64)> = conn.exec(stmt, ()).expect("mysql join");
    let n = rows.len();
    for row in rows {
        black_box(row);
    }
    n
}

struct LinInsert {
    db: lin::Db,
    prepared: lin::Prepared,
}

fn setup_lin_insert_variant(n: usize, embed: bool, scalar_index: bool, fts: bool) -> LinInsert {
    let mut db = if embed {
        lin::Db::empty()
    } else {
        lin::Db::empty().without_embedder()
    };
    if !fts {
        let docs = db
            .catalog
            .collections
            .get_mut("docs")
            .expect("docs catalog");
        for field in docs.fields.values_mut() {
            field.fts = false;
        }
        db.store.rebuild_fts(&db.catalog);
    }
    if scalar_index {
        db.run("index docs [wing, ts]").expect("lin index");
    }
    let src = lin_insert_src(&docs(n));
    let prepared = db.prepare(&src).expect("lin prepare insert");
    LinInsert { db, prepared }
}

fn setup_lin_insert(n: usize) -> LinInsert {
    setup_lin_insert_variant(n, true, true, true)
}

fn empty_sqlite() -> rusqlite::Connection {
    let conn = rusqlite::Connection::open_in_memory().expect("sqlite open");
    conn.execute_batch(
        "CREATE TABLE docs (
            id TEXT PRIMARY KEY,
            uri TEXT UNIQUE NOT NULL,
            wing TEXT NOT NULL,
            title TEXT NOT NULL,
            ts INTEGER NOT NULL,
            body TEXT NOT NULL
         );
         CREATE INDEX docs_wing_ts ON docs(wing, ts);",
    )
    .expect("sqlite schema");
    conn
}

fn empty_duck() -> duckdb::Connection {
    let conn = duckdb::Connection::open_in_memory().expect("duck open");
    conn.execute_batch(
        "CREATE TABLE docs (
            id VARCHAR PRIMARY KEY,
            uri VARCHAR UNIQUE NOT NULL,
            wing VARCHAR NOT NULL,
            title VARCHAR NOT NULL,
            ts BIGINT NOT NULL,
            body VARCHAR NOT NULL
         );
         CREATE INDEX docs_wing_ts ON docs(wing, ts);",
    )
    .expect("duck schema");
    conn
}

/// Fresh bulk table on a dedicated connection (does not touch warm `docs`).
fn empty_pg(url: String) -> postgres::Client {
    let mut client = try_pg_client(&url).expect("pg reconnect");
    client
        .batch_execute(
            "DROP TABLE IF EXISTS docs_bulk;
             CREATE TABLE docs_bulk (
                id TEXT PRIMARY KEY,
                uri TEXT UNIQUE NOT NULL,
                wing TEXT NOT NULL,
                title TEXT NOT NULL,
                ts BIGINT NOT NULL,
                body TEXT NOT NULL
             );
             CREATE INDEX docs_bulk_wing_ts ON docs_bulk(wing, ts);",
        )
        .expect("pg bulk schema");
    client
}

fn empty_mysql(url: String) -> mysql::Conn {
    let mut conn = try_mysql_conn(&url).expect("mysql reconnect");
    conn.query_drop("DROP TABLE IF EXISTS docs_bulk")
        .expect("mysql drop bulk");
    conn.query_drop(
        "CREATE TABLE docs_bulk (
            id VARCHAR(64) PRIMARY KEY,
            uri VARCHAR(255) UNIQUE NOT NULL,
            wing VARCHAR(64) NOT NULL,
            title VARCHAR(255) NOT NULL,
            ts BIGINT NOT NULL,
            body TEXT NOT NULL,
            INDEX docs_bulk_wing_ts (wing, ts)
         )",
    )
    .expect("mysql bulk schema");
    conn
}

fn fill_lin(ins: &mut LinInsert) {
    ins.prepared.run(&mut ins.db).expect("lin insert");
}

struct LinRebuildWarm {
    db: lin::Db,
}

fn seed_lin_rebuild(n: usize) -> LinRebuildWarm {
    let mut db = lin::Db::empty().without_embedder();
    db.run("index docs [wing, ts]").expect("lin index");
    for chunk in docs(n).chunks(500) {
        db.run(&lin_insert_src(chunk)).expect("lin rebuild seed");
    }
    LinRebuildWarm { db }
}

fn lin_hits(h: &lin::Handle) -> i64 {
    h.rows
        .first()
        .and_then(|r| match r.get("hits") {
            Some(lin::Cell::Int(n)) => Some(*n),
            _ => None,
        })
        .unwrap_or(h.done.n as i64)
}

fn fill_sqlite(conn: &rusqlite::Connection, n: usize) {
    let rows = docs(n);
    let tx = conn.unchecked_transaction().expect("tx");
    {
        let mut stmt = tx
            .prepare("INSERT INTO docs (id, uri, wing, title, ts, body) VALUES (?1,?2,?3,?4,?5,?6)")
            .expect("prepare");
        for d in &rows {
            stmt.execute(rusqlite::params![
                d.id, d.uri, d.wing, d.title, d.ts, d.body
            ])
            .expect("insert");
        }
    }
    tx.commit().expect("commit");
}

fn fill_duck(conn: &duckdb::Connection, n: usize) {
    let rows = docs(n);
    let mut stmt = conn
        .prepare("INSERT INTO docs (id, uri, wing, title, ts, body) VALUES (?,?,?,?,?,?)")
        .expect("prepare");
    for d in &rows {
        stmt.execute(duckdb::params![d.id, d.uri, d.wing, d.title, d.ts, d.body])
            .expect("insert");
    }
}

fn fill_pg(client: &mut postgres::Client, n: usize) {
    let rows = docs(n);
    let mut tx = client.transaction().expect("pg tx");
    let stmt = tx
        .prepare(
            "INSERT INTO docs_bulk (id, uri, wing, title, ts, body) VALUES ($1,$2,$3,$4,$5,$6)",
        )
        .expect("pg prepare");
    for d in &rows {
        tx.execute(&stmt, &[&d.id, &d.uri, &d.wing, &d.title, &d.ts, &d.body])
            .expect("pg insert");
    }
    tx.commit().expect("pg commit");
}

fn fill_mysql(conn: &mut mysql::Conn, n: usize) {
    let rows = docs(n);
    let mut tx = conn
        .start_transaction(mysql::TxOpts::default())
        .expect("mysql tx");
    let stmt = tx
        .prep("INSERT INTO docs_bulk (id, uri, wing, title, ts, body) VALUES (?,?,?,?,?,?)")
        .expect("mysql prepare");
    for d in &rows {
        tx.exec_drop(&stmt, (&d.id, &d.uri, &d.wing, &d.title, d.ts, &d.body))
            .expect("mysql insert");
    }
    tx.commit().expect("mysql commit");
}

/// Append-only log lines (Lin `append facts` vs SQL `INSERT INTO logs`).
fn lin_append_log_src(n: usize) -> String {
    let mut out = String::from("append facts [\n");
    for i in 0..n {
        if i > 0 {
            out.push_str(",\n");
        }
        out.push_str(&format!(
            r#"  {{ s: "log-{i}", p: tagged, o: "msg {i} wal event" }}"#
        ));
    }
    out.push_str("\n]");
    out
}

struct LinAppendLog {
    db: lin::Db,
    prepared: lin::Prepared,
}

fn setup_lin_append_log(n: usize) -> LinAppendLog {
    let mut db = lin::Db::empty();
    let prepared = db
        .prepare(&lin_append_log_src(n))
        .expect("lin prepare append facts");
    LinAppendLog { db, prepared }
}

fn fill_lin_append_log(ins: &mut LinAppendLog) {
    ins.prepared.run(&mut ins.db).expect("lin append facts");
}

/// Temp dir kept alive for the timed durable run.
struct TmpKeep(std::path::PathBuf);

impl Drop for TmpKeep {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn fresh_tmp(label: &str) -> TmpKeep {
    let p = std::env::temp_dir().join(format!(
        "lin-bench-{label}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).expect("tmpdir");
    TmpKeep(p)
}

struct LinDurableAppend {
    _dir: TmpKeep,
    db: lin::Db,
    prepared: lin::Prepared,
}

fn setup_lin_durable_append(n: usize) -> LinDurableAppend {
    setup_lin_durable_append_sync(n, lin::SyncMode::Full)
}

fn setup_lin_durable_append_sync(n: usize, sync: lin::SyncMode) -> LinDurableAppend {
    let dir = fresh_tmp("append");
    let mut db =
        lin::Db::open_with(&dir.0, lin::OpenOpts { sync, cold: false }).expect("lin durable open");
    let prepared = db
        .prepare(&lin_append_log_src(n))
        .expect("lin prepare durable append");
    LinDurableAppend {
        _dir: dir,
        db,
        prepared,
    }
}

fn fill_lin_durable_append(ins: &mut LinDurableAppend) -> lin::Handle {
    // Return Handle so DropPolicy::OutsideTiming excludes row-map teardown from wall time.
    ins.prepared.run(&mut ins.db).expect("lin durable append")
}

struct LinDurableInsert {
    _dir: TmpKeep,
    db: lin::Db,
    prepared: lin::Prepared,
}

fn setup_lin_durable_insert(n: usize) -> LinDurableInsert {
    let dir = fresh_tmp("insert");
    let mut db = lin::Db::open(&dir.0).expect("lin durable open");
    db.run("index docs [wing, ts]").expect("lin index");
    let prepared = db
        .prepare(&lin_insert_src(&docs(n)))
        .expect("lin prepare durable insert");
    LinDurableInsert {
        _dir: dir,
        db,
        prepared,
    }
}

fn fill_lin_durable_insert(ins: &mut LinDurableInsert) -> lin::Handle {
    ins.prepared.run(&mut ins.db).expect("lin durable insert")
}

const GROUP_COMMITS: usize = 16;

struct LinGroupCommit {
    _dir: TmpKeep,
    db: lin::Db,
    singles: Vec<lin::Prepared>,
    grouped: lin::Prepared,
}

fn setup_lin_group_commit() -> LinGroupCommit {
    let dir = fresh_tmp("group-commit");
    let db = lin::Db::open(&dir.0).expect("lin durable open");
    let mut statements = Vec::with_capacity(GROUP_COMMITS);
    let mut singles = Vec::with_capacity(GROUP_COMMITS);
    for i in 0..GROUP_COMMITS {
        let source = format!(r#"append facts {{ s: "group", p: "item", o: "{i}" }}"#);
        let statement = lin::parse(&source).expect("parse grouped commit");
        singles.push(
            db.prepare_stmt(statement.clone())
                .expect("prepare single commit"),
        );
        statements.push(statement);
    }
    let grouped = db
        .prepare_stmts(statements)
        .expect("prepare grouped commit");
    LinGroupCommit {
        _dir: dir,
        db,
        singles,
        grouped,
    }
}

fn fill_lin_sequential_commits(group: &mut LinGroupCommit) {
    for prepared in &group.singles {
        black_box(prepared.run(&mut group.db).expect("single durable commit"));
    }
}

fn fill_lin_group_commit(group: &mut LinGroupCommit) -> lin::Handle {
    group
        .grouped
        .run(&mut group.db)
        .expect("grouped durable commit")
}

struct LinColdReopen {
    dir: TmpKeep,
}

fn setup_lin_cold_reopen(n: usize) -> LinColdReopen {
    let dir = fresh_tmp("cold");
    let mut db = lin::Db::open_with(
        &dir.0,
        lin::OpenOpts {
            sync: lin::SyncMode::Full,
            cold: true,
        },
    )
    .expect("open cold");
    db.run(&lin_insert_src(&docs(n))).expect("seed");
    db.checkpoint().expect("cold checkpoint");
    db.close().expect("close");
    LinColdReopen { dir }
}

fn setup_lin_hot_reopen(n: usize) -> LinColdReopen {
    let dir = fresh_tmp("hot");
    let mut db = lin::Db::open(&dir.0).expect("open");
    db.run(&lin_insert_src(&docs(n))).expect("seed");
    db.checkpoint().expect("checkpoint");
    db.close().expect("close");
    LinColdReopen { dir }
}

struct LinWalShip {
    frames: Vec<u8>,
}

fn setup_lin_wal_ship(n: usize) -> LinWalShip {
    let src_dir = fresh_tmp("wal_src");
    let mut src = lin::Db::open(&src_dir.0).expect("src");
    src.run(&lin_append_log_src(n)).expect("seed wal");
    let frames = src.export_wal_since(0).expect("export");
    // Drop src without close/checkpoint so we keep frames; dir can go.
    drop(src);
    LinWalShip { frames }
}

struct SqliteDurable {
    _dir: TmpKeep,
    conn: rusqlite::Connection,
    n: usize,
    kind: &'static str,
}

fn setup_sqlite_durable(kind: &'static str, n: usize) -> SqliteDurable {
    let dir = fresh_tmp("sqlite");
    let path = dir.0.join("db.sqlite");
    let conn = rusqlite::Connection::open(&path).expect("sqlite open");
    conn.execute_batch(
        "PRAGMA synchronous = FULL;
         PRAGMA journal_mode = DELETE;",
    )
    .expect("sqlite pragma");
    match kind {
        "logs" => {
            conn.execute_batch(
                "CREATE TABLE logs (
                    id TEXT PRIMARY KEY,
                    ts INTEGER NOT NULL,
                    msg TEXT NOT NULL
                 );",
            )
            .expect("sqlite logs");
        }
        _ => {
            conn.execute_batch(
                "CREATE TABLE docs (
                    id TEXT PRIMARY KEY,
                    uri TEXT UNIQUE NOT NULL,
                    wing TEXT NOT NULL,
                    title TEXT NOT NULL,
                    ts INTEGER NOT NULL,
                    body TEXT NOT NULL
                 );
                 CREATE INDEX docs_wing_ts ON docs(wing, ts);",
            )
            .expect("sqlite docs");
        }
    }
    SqliteDurable {
        _dir: dir,
        conn,
        n,
        kind,
    }
}

fn fill_sqlite_durable(s: &mut SqliteDurable) {
    match s.kind {
        "logs" => fill_sqlite_logs(&s.conn, s.n),
        _ => fill_sqlite(&s.conn, s.n),
    }
}

fn empty_sqlite_logs() -> rusqlite::Connection {
    let conn = rusqlite::Connection::open_in_memory().expect("sqlite open");
    conn.execute_batch(
        "CREATE TABLE logs (
            id TEXT PRIMARY KEY,
            ts INTEGER NOT NULL,
            msg TEXT NOT NULL
         );",
    )
    .expect("sqlite logs schema");
    conn
}

fn empty_duck_logs() -> duckdb::Connection {
    let conn = duckdb::Connection::open_in_memory().expect("duck open");
    conn.execute_batch(
        "CREATE TABLE logs (
            id VARCHAR PRIMARY KEY,
            ts BIGINT NOT NULL,
            msg VARCHAR NOT NULL
         );",
    )
    .expect("duck logs schema");
    conn
}

fn empty_pg_logs(url: String) -> postgres::Client {
    let mut client = try_pg_client(&url).expect("pg reconnect");
    client
        .batch_execute(
            "DROP TABLE IF EXISTS logs_bulk;
             CREATE TABLE logs_bulk (
                id TEXT PRIMARY KEY,
                ts BIGINT NOT NULL,
                msg TEXT NOT NULL
             );",
        )
        .expect("pg logs schema");
    client
}

fn empty_mysql_logs(url: String) -> mysql::Conn {
    let mut conn = try_mysql_conn(&url).expect("mysql reconnect");
    conn.query_drop("DROP TABLE IF EXISTS logs_bulk")
        .expect("mysql drop logs");
    conn.query_drop(
        "CREATE TABLE logs_bulk (
            id VARCHAR(64) PRIMARY KEY,
            ts BIGINT NOT NULL,
            msg TEXT NOT NULL
         ) ENGINE=InnoDB",
    )
    .expect("mysql logs schema");
    conn
}

fn fill_sqlite_logs(conn: &rusqlite::Connection, n: usize) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    let tx = conn.unchecked_transaction().expect("sqlite tx");
    {
        let mut stmt = tx
            .prepare("INSERT INTO logs (id, ts, msg) VALUES (?1, ?2, ?3)")
            .expect("sqlite prep");
        for i in 0..n {
            stmt.execute(rusqlite::params![
                format!("log-{i}"),
                now + i as i64,
                format!("msg {i} wal event")
            ])
            .expect("sqlite log insert");
        }
    }
    tx.commit().expect("sqlite commit");
}

fn fill_duck_logs(conn: &duckdb::Connection, n: usize) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    let mut app = conn.appender("logs").expect("duck appender");
    for i in 0..n {
        app.append_row(duckdb::params![
            format!("log-{i}"),
            now + i as i64,
            format!("msg {i} wal event")
        ])
        .expect("duck log insert");
    }
    drop(app);
}

fn fill_pg_logs(client: &mut postgres::Client, n: usize) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    let mut tx = client.transaction().expect("pg tx");
    let stmt = tx
        .prepare("INSERT INTO logs_bulk (id, ts, msg) VALUES ($1, $2, $3)")
        .expect("pg prep");
    for i in 0..n {
        tx.execute(
            &stmt,
            &[
                &format!("log-{i}"),
                &(now + i as i64),
                &format!("msg {i} wal event"),
            ],
        )
        .expect("pg log insert");
    }
    tx.commit().expect("pg commit");
}

fn fill_mysql_logs(conn: &mut mysql::Conn, n: usize) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    let mut tx = conn
        .start_transaction(mysql::TxOpts::default())
        .expect("mysql tx");
    let stmt = tx
        .prep("INSERT INTO logs_bulk (id, ts, msg) VALUES (?, ?, ?)")
        .expect("mysql prep");
    for i in 0..n {
        tx.exec_drop(
            &stmt,
            (
                format!("log-{i}"),
                now + i as i64,
                format!("msg {i} wal event"),
            ),
        )
        .expect("mysql log insert");
    }
    tx.commit().expect("mysql commit");
}

fn main() -> airbug_bench::Result<()> {
    let pg = connect_pg();
    let mysql = connect_mysql();

    let mut engines =
        String::from("Lin | SQLite(rusqlite bundled) | DuckDB(bundled) | HashMap(point get only)");
    if let Some((ref url, _)) = pg {
        engines.push_str(&format!(" | Postgres({url})"));
    } else {
        engines.push_str(" | Postgres(skipped)");
    }
    if let Some((ref url, _)) = mysql {
        engines.push_str(&format!(" | MySQL({url})"));
    } else {
        engines.push_str(" | MySQL(skipped)");
    }

    eprintln!(
        "lin compare: N={N} warm reads (fixture), bulk insert (schema outside timing)\n\
         engines: {engines}\n\
         airbug dash: http://127.0.0.1:8790/\n\
         Lin reads: prepare once / run many; filters use count (fair vs SQL COUNT(*));\n\
         substring: Lin `title ~ \"wal\" | count` vs SQL LIKE '%wal%' COUNT(*);\n\
         materialize: Lin `wing==rag | {{id,title}} | take all` vs SQL SELECT id,title;\n\
         join: {JOIN_ORDERS} orders ⋈ {JOIN_USERS} users (Lin run_batch SoA / cursor vs SQL INNER JOIN);\n\
         append_log: Lin `append facts` vs SQL INSERT INTO logs (append-only shape)"
    );

    let mut suite = Suite::new("compare");
    suite.config(Config::profile("quick")?);

    let lin = Fixture::new(|| seed_lin(N));
    let lin_rebuild = Fixture::new(|| seed_lin_rebuild(5_000));
    let sql = Fixture::new(|| seed_sqlite(N));
    let duck = Fixture::new(|| seed_duck(N));
    let map = Fixture::new(|| seed_map(N));
    let (pg_fix, pg_url) = match pg {
        Some((url, client)) => (Some(Fixture::new(move || seed_pg(N, client))), Some(url)),
        None => (None, None),
    };
    let (mysql_fix, mysql_url) = match mysql {
        Some((url, conn)) => (Some(Fixture::new(move || seed_mysql(N, conn))), Some(url)),
        None => (None, None),
    };

    let lin_join = Fixture::new(seed_join_lin);
    let sql_join = Fixture::new(seed_join_sqlite);
    let duck_join = Fixture::new(seed_join_duck);
    let pg_join = pg_url.clone().map(|url| {
        Fixture::new(move || {
            let c = try_pg_client(&url).unwrap_or_else(|e| panic!("postgres join: {e}"));
            seed_join_pg(c)
        })
    });
    let mysql_join = mysql_url.clone().map(|url| {
        Fixture::new(move || {
            let c = try_mysql_conn(&url).unwrap_or_else(|e| panic!("mysql join: {e}"));
            seed_join_mysql(c)
        })
    });

    suite
        .bench_fixture("point_get/lin", lin.clone(), |s| {
            black_box(s.point_get.run(&mut s.db).expect("lin get").done.n)
        })
        .tag("point_get")
        .tag("lin")
        .parameter("n", N);
    suite
        .bench_fixture("point_get/sqlite", sql.clone(), |s| {
            let mut stmt = s
                .conn
                .prepare_cached("SELECT COUNT(*) FROM docs WHERE id = ?1")
                .expect("sqlite prep");
            let n: i64 = stmt
                .query_row(rusqlite::params![s.probe_id], |r| r.get(0))
                .expect("sqlite get");
            black_box(n)
        })
        .tag("point_get")
        .tag("sqlite")
        .parameter("n", N);
    suite
        .bench_fixture("point_get/duckdb", duck.clone(), |s| {
            let mut stmt = s
                .conn
                .prepare_cached("SELECT COUNT(*) FROM docs WHERE id = ?")
                .expect("duck prep");
            let n: i64 = stmt
                .query_row(duckdb::params![s.probe_id], |r| r.get(0))
                .expect("duck get");
            black_box(n)
        })
        .tag("point_get")
        .tag("duckdb")
        .parameter("n", N);
    suite
        .bench_fixture("point_get/hashmap", map, |s| {
            black_box(s.by_id.get(&s.probe_id).map(|d| d.title.len()).unwrap_or(0))
        })
        .tag("point_get")
        .tag("hashmap")
        .parameter("n", N);
    if let Some(pg) = pg_fix.clone() {
        suite
            .bench_fixture("point_get/postgres", pg, |s| {
                let n: i64 = s
                    .client
                    .query_one(&s.point_get, &[&s.probe_id])
                    .expect("pg get")
                    .get(0);
                black_box(n)
            })
            .tag("point_get")
            .tag("postgres")
            .parameter("n", N);
    }
    if let Some(mysql) = mysql_fix.clone() {
        suite
            .bench_fixture("point_get/mysql", mysql, |s| {
                let n: i64 = s
                    .conn
                    .exec_first(&s.point_get, (&s.probe_id,))
                    .expect("mysql get")
                    .expect("mysql get row");
                black_box(n)
            })
            .tag("point_get")
            .tag("mysql")
            .parameter("n", N);
    }

    suite
        .bench_fixture("filter_eq/lin", lin.clone(), |s| {
            black_box(lin_hits(&s.filter_eq.run(&mut s.db).expect("lin filter")))
        })
        .tag("filter_eq")
        .tag("lin")
        .parameter("n", N);
    suite
        .bench_fixture("filter_eq/sqlite", sql.clone(), |s| {
            let mut stmt = s
                .conn
                .prepare_cached("SELECT COUNT(*) FROM docs WHERE wing = 'rag'")
                .expect("sqlite prep");
            let n: i64 = stmt.query_row([], |r| r.get(0)).expect("sqlite filter");
            black_box(n)
        })
        .tag("filter_eq")
        .tag("sqlite")
        .parameter("n", N);
    suite
        .bench_fixture("filter_eq/duckdb", duck.clone(), |s| {
            let mut stmt = s
                .conn
                .prepare_cached("SELECT COUNT(*) FROM docs WHERE wing = 'rag'")
                .expect("duck prep");
            let n: i64 = stmt.query_row([], |r| r.get(0)).expect("duck filter");
            black_box(n)
        })
        .tag("filter_eq")
        .tag("duckdb")
        .parameter("n", N);
    if let Some(pg) = pg_fix.clone() {
        suite
            .bench_fixture("filter_eq/postgres", pg, |s| {
                let n: i64 = s
                    .client
                    .query_one(&s.filter_eq, &[])
                    .expect("pg filter")
                    .get(0);
                black_box(n)
            })
            .tag("filter_eq")
            .tag("postgres")
            .parameter("n", N);
    }
    if let Some(mysql) = mysql_fix.clone() {
        suite
            .bench_fixture("filter_eq/mysql", mysql, |s| {
                let n: i64 = s
                    .conn
                    .exec_first(&s.filter_eq, ())
                    .expect("mysql filter")
                    .expect("mysql filter row");
                black_box(n)
            })
            .tag("filter_eq")
            .tag("mysql")
            .parameter("n", N);
    }

    suite
        .bench_fixture("filter_range/lin", lin.clone(), |s| {
            black_box(lin_hits(&s.filter_range.run(&mut s.db).expect("lin range")))
        })
        .tag("filter_range")
        .tag("lin")
        .parameter("n", N);
    suite
        .bench_fixture("filter_range/sqlite", sql.clone(), |s| {
            let cutoff = range_cutoff_ms();
            let mut stmt = s
                .conn
                .prepare_cached("SELECT COUNT(*) FROM docs WHERE wing = 'rag' AND ts > ?1")
                .expect("sqlite prep");
            let n: i64 = stmt
                .query_row(rusqlite::params![cutoff], |r| r.get(0))
                .expect("sqlite range");
            black_box(n)
        })
        .tag("filter_range")
        .tag("sqlite")
        .parameter("n", N);
    suite
        .bench_fixture("filter_range/duckdb", duck.clone(), |s| {
            let cutoff = range_cutoff_ms();
            let mut stmt = s
                .conn
                .prepare_cached("SELECT COUNT(*) FROM docs WHERE wing = 'rag' AND ts > ?")
                .expect("duck prep");
            let n: i64 = stmt
                .query_row(duckdb::params![cutoff], |r| r.get(0))
                .expect("duck range");
            black_box(n)
        })
        .tag("filter_range")
        .tag("duckdb")
        .parameter("n", N);
    if let Some(pg) = pg_fix.clone() {
        suite
            .bench_fixture("filter_range/postgres", pg, |s| {
                let cutoff = range_cutoff_ms();
                let n: i64 = s
                    .client
                    .query_one(&s.filter_range, &[&cutoff])
                    .expect("pg range")
                    .get(0);
                black_box(n)
            })
            .tag("filter_range")
            .tag("postgres")
            .parameter("n", N);
    }
    if let Some(mysql) = mysql_fix.clone() {
        suite
            .bench_fixture("filter_range/mysql", mysql, |s| {
                let cutoff = range_cutoff_ms();
                let n: i64 = s
                    .conn
                    .exec_first(&s.filter_range, (cutoff,))
                    .expect("mysql range")
                    .expect("mysql range row");
                black_box(n)
            })
            .tag("filter_range")
            .tag("mysql")
            .parameter("n", N);
    }

    suite
        .bench_fixture("text_substr/lin", lin.clone(), |s| {
            black_box(lin_hits(&s.text_substr.run(&mut s.db).expect("lin ~")))
        })
        .tag("text_substr")
        .tag("lin")
        .parameter("n", N);

    suite
        .bench_fixture("fts_lex_selective/lin", lin.clone(), |s| {
            black_box(
                s.fts_selective
                    .run(&mut s.db)
                    .expect("lin selective FTS")
                    .done
                    .n,
            )
        })
        .tag("fts")
        .tag("fts_selective")
        .tag("lin")
        .parameter("n", N)
        .parameter("hits", 1);
    suite
        .bench_fixture("fts_lex_common/lin", lin.clone(), |s| {
            black_box(s.fts_common.run(&mut s.db).expect("lin common FTS").done.n)
        })
        .tag("fts")
        .tag("fts_common")
        .tag("lin")
        .parameter("n", N)
        .parameter("candidates", N / 10);
    suite
        .bench_fixture("fts_lex_miss/lin", lin.clone(), |s| {
            black_box(s.fts_miss.run(&mut s.db).expect("lin miss FTS").done.n)
        })
        .tag("fts")
        .tag("fts_miss")
        .tag("lin")
        .parameter("n", N)
        .parameter("hits", 0);
    suite
        .bench_fixture("fts_hybrid_common/lin", lin.clone(), |s| {
            black_box(s.fts_hybrid.run(&mut s.db).expect("lin hybrid FTS").done.n)
        })
        .tag("fts")
        .tag("hybrid")
        .tag("lin")
        .parameter("n", N)
        .parameter("candidates", N / 10);
    suite
        .bench_fixture("text_substr/sqlite", sql.clone(), |s| {
            let mut stmt = s
                .conn
                .prepare_cached("SELECT COUNT(*) FROM docs WHERE title LIKE '%wal%'")
                .expect("sqlite prep");
            let n: i64 = stmt.query_row([], |r| r.get(0)).expect("sqlite like");
            black_box(n)
        })
        .tag("text_substr")
        .tag("sqlite")
        .parameter("n", N);
    suite
        .bench_fixture("text_substr/duckdb", duck.clone(), |s| {
            let mut stmt = s
                .conn
                .prepare_cached("SELECT COUNT(*) FROM docs WHERE title LIKE '%wal%'")
                .expect("duck prep");
            let n: i64 = stmt.query_row([], |r| r.get(0)).expect("duck like");
            black_box(n)
        })
        .tag("text_substr")
        .tag("duckdb")
        .parameter("n", N);
    if let Some(pg) = pg_fix.clone() {
        suite
            .bench_fixture("text_substr/postgres", pg, |s| {
                let n: i64 = s
                    .client
                    .query_one(&s.text_substr, &[])
                    .expect("pg like")
                    .get(0);
                black_box(n)
            })
            .tag("text_substr")
            .tag("postgres")
            .parameter("n", N);
    }
    if let Some(mysql) = mysql_fix.clone() {
        suite
            .bench_fixture("text_substr/mysql", mysql, |s| {
                let n: i64 = s
                    .conn
                    .exec_first(&s.text_substr, ())
                    .expect("mysql like")
                    .expect("mysql like row");
                black_box(n)
            })
            .tag("text_substr")
            .tag("mysql")
            .parameter("n", N);
    }

    suite
        .bench_fixture("materialize/lin", lin, |s| {
            black_box(s.materialize.run(&mut s.db).expect("lin mat").done.n)
        })
        .tag("materialize")
        .tag("lin")
        .parameter("n", N);
    suite
        .bench_fixture("materialize/sqlite", sql, |s| {
            let mut stmt = s
                .conn
                .prepare_cached("SELECT id, title FROM docs WHERE wing = 'rag'")
                .expect("sqlite prep");
            let mut rows = stmt.query([]).expect("sqlite mat");
            let mut n = 0usize;
            while let Some(row) = rows.next().expect("sqlite row") {
                let id: String = row.get(0).expect("id");
                let title: String = row.get(1).expect("title");
                black_box((id, title));
                n += 1;
            }
            black_box(n)
        })
        .tag("materialize")
        .tag("sqlite")
        .parameter("n", N);
    suite
        .bench_fixture("materialize/duckdb", duck, |s| {
            let mut stmt = s
                .conn
                .prepare_cached("SELECT id, title FROM docs WHERE wing = 'rag'")
                .expect("duck prep");
            let mut rows = stmt.query([]).expect("duck mat");
            let mut n = 0usize;
            while let Some(row) = rows.next().expect("duck row") {
                let id: String = row.get(0).expect("id");
                let title: String = row.get(1).expect("title");
                black_box((id, title));
                n += 1;
            }
            black_box(n)
        })
        .tag("materialize")
        .tag("duckdb")
        .parameter("n", N);
    if let Some(pg) = pg_fix {
        suite
            .bench_fixture("materialize/postgres", pg, |s| {
                let rows = s.client.query(&s.materialize, &[]).expect("pg mat");
                let mut n = 0usize;
                for row in rows {
                    let id: String = row.get(0);
                    let title: String = row.get(1);
                    black_box((id, title));
                    n += 1;
                }
                black_box(n)
            })
            .tag("materialize")
            .tag("postgres")
            .parameter("n", N);
    }
    if let Some(mysql) = mysql_fix {
        suite
            .bench_fixture("materialize/mysql", mysql, |s| {
                let rows: Vec<(String, String)> =
                    s.conn.exec(&s.materialize, ()).expect("mysql mat");
                let mut n = 0usize;
                for (id, title) in rows {
                    black_box((id, title));
                    n += 1;
                }
                black_box(n)
            })
            .tag("materialize")
            .tag("mysql")
            .parameter("n", N);
    }

    suite
        .bench_fixture("join_inner/lin", lin_join.clone(), |s| {
            black_box(s.inner.run_batch(&mut s.db).expect("lin join").n())
        })
        .tag("join_inner")
        .tag("phase")
        .tag("lin")
        .parameter("users", JOIN_USERS)
        .parameter("orders", JOIN_ORDERS)
        .work_units("rows", JOIN_ORDERS as u64);
    suite
        .bench_fixture("join_inner/lin_cursor", lin_join.clone(), |s| {
            black_box(consume_join_cursor(&s.db, &s.inner_q))
        })
        .tag("join_inner")
        .tag("phase")
        .tag("lin_cursor")
        .parameter("users", JOIN_USERS)
        .parameter("orders", JOIN_ORDERS)
        .work_units("rows", JOIN_ORDERS as u64);
    suite
        .bench_fixture("join_inner/lin_cursor_projected", lin_join.clone(), |s| {
            black_box(consume_projected_cursor(&s.db, &s.inner_q))
        })
        .tag("join_inner")
        .tag("phase")
        .tag("lin_cursor")
        .tag("projected")
        .parameter("users", JOIN_USERS)
        .parameter("orders", JOIN_ORDERS)
        .work_units("rows", JOIN_ORDERS as u64);
    suite
        .bench_fixture("join_phase/lin_cursor_open", lin_join.clone(), |s| {
            let cur = s.inner_q.cursor(&s.db).expect("lin join cursor open");
            black_box(cur.is_lazy())
        })
        .tag("phase")
        .tag("cursor")
        .tag("lin")
        .parameter("orders", JOIN_ORDERS);
    suite
        .bench_fixture(
            "join_phase/lin_cursor_scan_project",
            lin_join.clone(),
            |s| black_box(consume_join_cursor(&s.db, &s.orders_q)),
        )
        .tag("phase")
        .tag("cursor")
        .tag("lin")
        .parameter("orders", JOIN_ORDERS)
        .work_units("rows", JOIN_ORDERS as u64);
    suite
        .bench_fixture(
            "join_phase/lin_cursor_scan_projected_row",
            lin_join.clone(),
            |s| black_box(consume_projected_cursor(&s.db, &s.orders_q)),
        )
        .tag("phase")
        .tag("cursor")
        .tag("lin")
        .tag("projected")
        .parameter("orders", JOIN_ORDERS)
        .work_units("rows", JOIN_ORDERS as u64);
    suite
        .bench_fixture("join_inner/sqlite", sql_join.clone(), |s| {
            black_box(sqlite_join_n(&s.conn, JOIN_INNER_SQL))
        })
        .tag("join_inner")
        .tag("sqlite")
        .parameter("users", JOIN_USERS)
        .parameter("orders", JOIN_ORDERS)
        .work_units("rows", JOIN_ORDERS as u64);
    suite
        .bench_fixture("join_inner/duckdb", duck_join.clone(), |s| {
            black_box(duck_join_n(&s.conn, JOIN_INNER_SQL))
        })
        .tag("join_inner")
        .tag("duckdb")
        .parameter("users", JOIN_USERS)
        .parameter("orders", JOIN_ORDERS)
        .work_units("rows", JOIN_ORDERS as u64);
    if let Some(pg) = pg_join.clone() {
        suite
            .bench_fixture("join_inner/postgres", pg, |s| {
                black_box(pg_join_n(&mut s.client, &s.inner))
            })
            .tag("join_inner")
            .tag("postgres")
            .parameter("users", JOIN_USERS)
            .parameter("orders", JOIN_ORDERS)
            .work_units("rows", JOIN_ORDERS as u64);
    }
    if let Some(mysql) = mysql_join.clone() {
        suite
            .bench_fixture("join_inner/mysql", mysql, |s| {
                black_box(mysql_join_n(&mut s.conn, &s.inner))
            })
            .tag("join_inner")
            .tag("mysql")
            .parameter("users", JOIN_USERS)
            .parameter("orders", JOIN_ORDERS)
            .work_units("rows", JOIN_ORDERS as u64);
    }

    suite
        .bench_fixture("join_filter/lin", lin_join.clone(), |s| {
            black_box(s.filter.run_batch(&mut s.db).expect("lin join filter").n())
        })
        .tag("join_filter")
        .tag("lin")
        .parameter("users", JOIN_USERS)
        .parameter("orders", JOIN_ORDERS)
        .work_units("rows", JOIN_FILTER_N as u64);
    suite
        .bench_fixture("join_filter/lin_cursor", lin_join, |s| {
            black_box(consume_join_cursor(&s.db, &s.filter_q))
        })
        .tag("join_filter")
        .tag("lin_cursor")
        .parameter("users", JOIN_USERS)
        .parameter("orders", JOIN_ORDERS)
        .work_units("rows", JOIN_FILTER_N as u64);
    suite
        .bench_fixture("join_filter/sqlite", sql_join, |s| {
            black_box(sqlite_join_n(&s.conn, JOIN_FILTER_SQL))
        })
        .tag("join_filter")
        .tag("sqlite")
        .parameter("users", JOIN_USERS)
        .parameter("orders", JOIN_ORDERS)
        .work_units("rows", JOIN_FILTER_N as u64);
    suite
        .bench_fixture("join_filter/duckdb", duck_join, |s| {
            black_box(duck_join_n(&s.conn, JOIN_FILTER_SQL))
        })
        .tag("join_filter")
        .tag("duckdb")
        .parameter("users", JOIN_USERS)
        .parameter("orders", JOIN_ORDERS)
        .work_units("rows", JOIN_FILTER_N as u64);
    if let Some(pg) = pg_join {
        suite
            .bench_fixture("join_filter/postgres", pg, |s| {
                black_box(pg_join_n(&mut s.client, &s.filter))
            })
            .tag("join_filter")
            .tag("postgres")
            .parameter("users", JOIN_USERS)
            .parameter("orders", JOIN_ORDERS)
            .work_units("rows", JOIN_FILTER_N as u64);
    }
    if let Some(mysql) = mysql_join {
        suite
            .bench_fixture("join_filter/mysql", mysql, |s| {
                black_box(mysql_join_n(&mut s.conn, &s.filter))
            })
            .tag("join_filter")
            .tag("mysql")
            .parameter("users", JOIN_USERS)
            .parameter("orders", JOIN_ORDERS)
            .work_units("rows", JOIN_FILTER_N as u64);
    }

    macro_rules! insert_bulk {
        ($label:expr, $n:expr) => {{
            let n = $n;
            suite
                .bench_with_input(
                    &format!("insert_bulk_{}/lin", $label),
                    move || setup_lin_insert(n),
                    move |ins| fill_lin(ins),
                    DropPolicy::InsideTiming,
                )
                .tag("insert")
                .tag("lin")
                .parameter("rows", n)
                .work_units("rows", n as u64);
            suite
                .bench_with_input(
                    &format!("insert_bulk_{}/sqlite", $label),
                    empty_sqlite,
                    move |conn| fill_sqlite(conn, n),
                    DropPolicy::InsideTiming,
                )
                .tag("insert")
                .tag("sqlite")
                .parameter("rows", n)
                .work_units("rows", n as u64);
            suite
                .bench_with_input(
                    &format!("insert_bulk_{}/duckdb", $label),
                    empty_duck,
                    move |conn| fill_duck(conn, n),
                    DropPolicy::InsideTiming,
                )
                .tag("insert")
                .tag("duckdb")
                .parameter("rows", n)
                .work_units("rows", n as u64);
            if let Some(url) = pg_url.clone() {
                suite
                    .bench_with_input(
                        &format!("insert_bulk_{}/postgres", $label),
                        move || empty_pg(url.clone()),
                        move |client| fill_pg(client, n),
                        DropPolicy::InsideTiming,
                    )
                    .tag("insert")
                    .tag("postgres")
                    .parameter("rows", n)
                    .work_units("rows", n as u64);
            }
            if let Some(url) = mysql_url.clone() {
                suite
                    .bench_with_input(
                        &format!("insert_bulk_{}/mysql", $label),
                        move || empty_mysql(url.clone()),
                        move |conn| fill_mysql(conn, n),
                        DropPolicy::InsideTiming,
                    )
                    .tag("insert")
                    .tag("mysql")
                    .parameter("rows", n)
                    .work_units("rows", n as u64);
            }
        }};
    }
    insert_bulk!("1k", INSERT_1K);
    insert_bulk!("10k", INSERT_10K);

    suite
        .bench_with_input(
            "insert_phase_10k/lin_full",
            move || setup_lin_insert_variant(INSERT_10K, true, true, true),
            fill_lin,
            DropPolicy::InsideTiming,
        )
        .tag("phase")
        .tag("insert")
        .tag("lin")
        .parameter("rows", INSERT_10K)
        .parameter("embed", 1)
        .parameter("scalar_index", 1)
        .parameter("fts", 1)
        .work_units("rows", INSERT_10K as u64);
    suite
        .bench_with_input(
            "insert_phase_10k/lin_no_embed",
            move || setup_lin_insert_variant(INSERT_10K, false, true, true),
            fill_lin,
            DropPolicy::InsideTiming,
        )
        .tag("phase")
        .tag("insert")
        .tag("lin")
        .parameter("rows", INSERT_10K)
        .parameter("embed", 0)
        .parameter("scalar_index", 1)
        .parameter("fts", 1)
        .work_units("rows", INSERT_10K as u64);
    suite
        .bench_with_input(
            "insert_phase_10k/lin_no_embed_no_scalar_index",
            move || setup_lin_insert_variant(INSERT_10K, false, false, true),
            fill_lin,
            DropPolicy::InsideTiming,
        )
        .tag("phase")
        .tag("insert")
        .tag("lin")
        .parameter("rows", INSERT_10K)
        .parameter("embed", 0)
        .parameter("scalar_index", 0)
        .parameter("fts", 1)
        .work_units("rows", INSERT_10K as u64);
    suite
        .bench_with_input(
            "insert_phase_10k/lin_no_embed_no_scalar_no_fts",
            move || setup_lin_insert_variant(INSERT_10K, false, false, false),
            fill_lin,
            DropPolicy::InsideTiming,
        )
        .tag("phase")
        .tag("insert")
        .tag("lin")
        .parameter("rows", INSERT_10K)
        .parameter("embed", 0)
        .parameter("scalar_index", 0)
        .parameter("fts", 0)
        .work_units("rows", INSERT_10K as u64);

    macro_rules! append_log {
        ($label:expr, $n:expr) => {{
            let n = $n;
            suite
                .bench_with_input(
                    &format!("append_log_{}/lin", $label),
                    move || setup_lin_append_log(n),
                    move |ins| fill_lin_append_log(ins),
                    DropPolicy::InsideTiming,
                )
                .tag("append_log")
                .tag("lin")
                .parameter("rows", n)
                .work_units("rows", n as u64);
            suite
                .bench_with_input(
                    &format!("append_log_{}/sqlite", $label),
                    empty_sqlite_logs,
                    move |conn| fill_sqlite_logs(conn, n),
                    DropPolicy::InsideTiming,
                )
                .tag("append_log")
                .tag("sqlite")
                .parameter("rows", n)
                .work_units("rows", n as u64);
            suite
                .bench_with_input(
                    &format!("append_log_{}/duckdb", $label),
                    empty_duck_logs,
                    move |conn| fill_duck_logs(conn, n),
                    DropPolicy::InsideTiming,
                )
                .tag("append_log")
                .tag("duckdb")
                .parameter("rows", n)
                .work_units("rows", n as u64);
            if let Some(url) = pg_url.clone() {
                suite
                    .bench_with_input(
                        &format!("append_log_{}/postgres", $label),
                        move || empty_pg_logs(url.clone()),
                        move |client| fill_pg_logs(client, n),
                        DropPolicy::InsideTiming,
                    )
                    .tag("append_log")
                    .tag("postgres")
                    .parameter("rows", n)
                    .work_units("rows", n as u64);
            }
            if let Some(url) = mysql_url.clone() {
                suite
                    .bench_with_input(
                        &format!("append_log_{}/mysql", $label),
                        move || empty_mysql_logs(url.clone()),
                        move |conn| fill_mysql_logs(conn, n),
                        DropPolicy::InsideTiming,
                    )
                    .tag("append_log")
                    .tag("mysql")
                    .parameter("rows", n)
                    .work_units("rows", n as u64);
            }
        }};
    }
    append_log!("1k", INSERT_1K);
    append_log!("10k", INSERT_10K);

    // Durable path: Lin WAL sync_data vs SQLite synchronous=FULL (same machine, temp files).
    // OutsideTiming: Lin Handle holds N row maps — dropping them is not part of write+fsync cost
    // (SQLite fill returns ()). Fair wall = mutate + durable commit only.
    for (label, n) in [("1k", INSERT_1K), ("10k", INSERT_10K)] {
        suite
            .bench_with_input(
                &format!("durable_append_{label}/lin"),
                move || setup_lin_durable_append(n),
                move |ins| fill_lin_durable_append(ins),
                DropPolicy::OutsideTiming,
            )
            .tag("durable")
            .tag("append_log")
            .tag("lin")
            .parameter("rows", n)
            .work_units("rows", n as u64);
        suite
            .bench_with_input(
                &format!("durable_append_{label}/sqlite"),
                move || setup_sqlite_durable("logs", n),
                move |s| fill_sqlite_durable(s),
                DropPolicy::OutsideTiming,
            )
            .tag("durable")
            .tag("append_log")
            .tag("sqlite")
            .parameter("rows", n)
            .work_units("rows", n as u64);
        suite
            .bench_with_input(
                &format!("durable_insert_{label}/lin"),
                move || setup_lin_durable_insert(n),
                move |ins| fill_lin_durable_insert(ins),
                DropPolicy::OutsideTiming,
            )
            .tag("durable")
            .tag("insert")
            .tag("lin")
            .parameter("rows", n)
            .work_units("rows", n as u64);
        suite
            .bench_with_input(
                &format!("durable_insert_{label}/sqlite"),
                move || setup_sqlite_durable("docs", n),
                move |s| fill_sqlite_durable(s),
                DropPolicy::OutsideTiming,
            )
            .tag("durable")
            .tag("insert")
            .tag("sqlite")
            .parameter("rows", n)
            .work_units("rows", n as u64);
    }

    // SyncMode::Normal (flush on checkpoint only) vs Full — same 1k append.
    suite
        .bench_with_input(
            "durable_append_1k/lin_normal_unsync",
            move || setup_lin_durable_append_sync(INSERT_1K, lin::SyncMode::Normal),
            move |ins| fill_lin_durable_append(ins),
            DropPolicy::OutsideTiming,
        )
        .tag("durable")
        .tag("sync_normal")
        .tag("lin")
        .parameter("rows", INSERT_1K)
        .work_units("rows", INSERT_1K as u64);

    suite
        .bench_with_input(
            "group_commit_16/lin_sequential_full",
            setup_lin_group_commit,
            fill_lin_sequential_commits,
            DropPolicy::OutsideTiming,
        )
        .tag("phase")
        .tag("durable")
        .tag("group_commit")
        .tag("lin")
        .parameter("commits", GROUP_COMMITS)
        .work_units("commits", GROUP_COMMITS as u64);
    suite
        .bench_with_input(
            "group_commit_16/lin_grouped_full",
            setup_lin_group_commit,
            fill_lin_group_commit,
            DropPolicy::OutsideTiming,
        )
        .tag("phase")
        .tag("durable")
        .tag("group_commit")
        .tag("lin")
        .parameter("commits", 1)
        .parameter("statements", GROUP_COMMITS)
        .work_units("statements", GROUP_COMMITS as u64);

    // Cold mmap checkpoint + reopen (50k docs).
    const COLD_N: usize = 5_000;
    suite
        .bench_with_input(
            "cold_reopen_5k/lin",
            move || setup_lin_cold_reopen(COLD_N),
            move |s| {
                let _ = black_box(lin::Db::open_with(
                    &s.dir.0,
                    lin::OpenOpts {
                        sync: lin::SyncMode::Full,
                        cold: true,
                    },
                ));
            },
            DropPolicy::OutsideTiming,
        )
        .tag("cold")
        .tag("phase")
        .tag("reopen")
        .tag("lin")
        .parameter("rows", COLD_N)
        .work_units("rows", COLD_N as u64);
    suite
        .bench_with_input(
            "hot_reopen_5k/lin",
            move || setup_lin_hot_reopen(COLD_N),
            move |s| {
                let _ = black_box(lin::Db::open(&s.dir.0));
            },
            DropPolicy::OutsideTiming,
        )
        .tag("cold")
        .tag("phase")
        .tag("reopen")
        .tag("lin")
        .parameter("rows", COLD_N)
        .work_units("rows", COLD_N as u64);

    suite
        .bench_fixture(
            "reopen_phase_5k/rebuild_row_maps",
            lin_rebuild.clone(),
            |s| {
                s.db.store.rebuild_row_maps();
                black_box(s.db.store.row_count())
            },
        )
        .tag("phase")
        .tag("reopen")
        .tag("lin")
        .parameter("rows", COLD_N)
        .work_units("rows", COLD_N as u64);
    suite
        .bench_fixture(
            "reopen_phase_5k/rebuild_scalar_indexes",
            lin_rebuild.clone(),
            |s| {
                s.db.store.rebuild_indexes();
                black_box(s.db.store.row_count())
            },
        )
        .tag("phase")
        .tag("reopen")
        .tag("lin")
        .parameter("rows", COLD_N)
        .work_units("rows", COLD_N as u64);
    suite
        .bench_fixture("reopen_phase_5k/rebuild_fts", lin_rebuild, |s| {
            s.db.store.rebuild_fts(&s.db.catalog);
            black_box(s.db.store.row_count())
        })
        .tag("phase")
        .tag("reopen")
        .tag("fts")
        .tag("lin")
        .parameter("rows", COLD_N)
        .work_units("rows", COLD_N as u64);

    // WAL ship: export + apply 1k-row commit frames.
    suite
        .bench_with_input(
            "wal_ship_1k/lin",
            move || setup_lin_wal_ship(INSERT_1K),
            move |s| {
                let mut dst = lin::Db::empty();
                let n = dst.apply_wal(&s.frames).expect("apply");
                black_box(n);
            },
            DropPolicy::OutsideTiming,
        )
        .tag("wal")
        .tag("lin")
        .parameter("rows", INSERT_1K)
        .work_units("rows", INSERT_1K as u64);

    let args: Vec<String> = std::env::args()
        .skip(1)
        .filter(|a| a != "--bench")
        .collect();
    run_suite(suite, &args)
}

/// Minimal CLI compatible with `Suite::main`, ignoring cargo's injected `--bench`.
fn run_suite(mut suite: Suite<'_>, args: &[String]) -> airbug_bench::Result<()> {
    let mut config = Config::profile("quick")?;
    let mut profile = None;
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--profile" {
            i += 1;
            profile = Some(
                args.get(i)
                    .ok_or_else(|| airbug_bench::error("--profile requires value"))?
                    .clone(),
            );
        }
        i += 1;
    }
    if let Some(p) = &profile {
        config = Config::profile(p)?;
    }
    let mut selection = airbug_bench::Selection::default();
    let mut list = false;
    let mut json = false;
    let mut output = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--profile" => {
                i += 1;
            }
            "--list" => list = true,
            "--json" => json = true,
            "--exact" => selection.exact = true,
            "--glob" => selection.glob = true,
            "--filter" => {
                i += 1;
                selection.pattern = args
                    .get(i)
                    .ok_or_else(|| airbug_bench::error("--filter requires value"))?
                    .clone();
            }
            "--tag" => {
                i += 1;
                selection.tags.push(
                    args.get(i)
                        .ok_or_else(|| airbug_bench::error("--tag requires value"))?
                        .clone(),
                );
            }
            "--exclude" => {
                i += 1;
                selection.exclude.push(
                    args.get(i)
                        .ok_or_else(|| airbug_bench::error("--exclude requires glob"))?
                        .clone(),
                );
            }
            "--samples" => {
                i += 1;
                config.samples = args
                    .get(i)
                    .ok_or_else(|| airbug_bench::error("--samples requires value"))?
                    .parse()?;
            }
            "--sample-ms" => {
                i += 1;
                config.sample_time = Duration::from_millis(
                    args.get(i)
                        .ok_or_else(|| airbug_bench::error("--sample-ms requires value"))?
                        .parse()?,
                );
            }
            "--warmup-ms" => {
                i += 1;
                config.warmup = Duration::from_millis(
                    args.get(i)
                        .ok_or_else(|| airbug_bench::error("--warmup-ms requires value"))?
                        .parse()?,
                );
            }
            "--output" => {
                i += 1;
                output = Some(
                    args.get(i)
                        .ok_or_else(|| airbug_bench::error("--output requires directory"))?
                        .clone(),
                );
            }
            "--help" | "-h" => {
                println!(
                    "--list --profile quick|normal|thorough --filter TEXT [--exact|--glob] --exclude GLOB --tag TAG --samples N --sample-ms N --warmup-ms N --json --output NEW_DIRECTORY"
                );
                return Ok(());
            }
            other => return Err(airbug_bench::error(format!("unknown argument {other}"))),
        }
        i += 1;
    }
    suite.config(config);
    selection.validate()?;
    if list {
        for id in suite.list_selected(&selection) {
            println!("{id}");
        }
        return Ok(());
    }
    if cfg!(debug_assertions) {
        return Err(airbug_bench::error(
            "benchmarks require an optimized build; use cargo bench or cargo run --release",
        ));
    }
    let selected = suite.list_selected(&selection);
    let total = selected.len();
    if let Some(p) = output.as_ref() {
        let dir = std::path::Path::new(p);
        if let Some(parent) = dir.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent).map_err(|e| airbug_bench::error(e.to_string()))?;
            }
        }
        if !dir.exists() {
            std::fs::create_dir(dir).map_err(|e| airbug_bench::error(e.to_string()))?;
        }
        let progress = serde_json::json!({
            "state": "running",
            "completed": 0,
            "total": total,
            "variant": "compare"
        });
        std::fs::write(
            dir.join("progress.json"),
            serde_json::to_string_pretty(&progress)?,
        )
        .map_err(|e| airbug_bench::error(e.to_string()))?;
        if let Ok(url) = std::env::var("AIRBUG_DASH_URL") {
            eprintln!("airbug dash: {url}");
        } else if let Ok(hub) = std::env::var("AIRBUG_HUB") {
            eprintln!("airbug dash: {hub}/#/bench (see register response)");
        } else {
            eprintln!("airbug dash: http://127.0.0.1:8790/#/bench");
        }
    } else {
        eprintln!("airbug dash: http://127.0.0.1:8790/#/bench");
    }
    let run_result = suite.run_selected(&selection);
    if let Some(p) = output.as_ref() {
        let dir = std::path::Path::new(p);
        let final_state = if run_result.is_ok() {
            "complete"
        } else {
            "failed"
        };
        let _ = std::fs::write(
            dir.join("status-final.json"),
            serde_json::to_string_pretty(&serde_json::json!({
                "state": final_state,
                "error": run_result.as_ref().err().map(|e| e.to_string()),
            }))?,
        );
        let progress = serde_json::json!({
            "state": final_state,
            "completed": total,
            "total": total,
            "variant": "compare"
        });
        let _ = std::fs::write(
            dir.join("progress.json"),
            serde_json::to_string_pretty(&progress)?,
        );
    }
    let run = run_result?;
    if let Some(p) = output {
        let dir = std::path::Path::new(&p);
        // Hub may have already created the directory; write artifacts in place.
        let run_path = dir.join("run.json");
        if run_path.exists() {
            return Err(airbug_bench::error(format!(
                "run.json already exists in {}",
                dir.display()
            )));
        }
        std::fs::write(&run_path, serde_json::to_string_pretty(&run)?)
            .map_err(|e| airbug_bench::error(e.to_string()))?;
        let html = airbug_bench::report::html_run(&run)?;
        std::fs::write(dir.join("report.html"), html)
            .map_err(|e| airbug_bench::error(e.to_string()))?;
    }
    if json {
        println!("RBENCH_RESULT={}", serde_json::to_string(&run)?);
    } else {
        println!("{}", airbug_bench::report::markdown(&run)?);
    }
    Ok(())
}
