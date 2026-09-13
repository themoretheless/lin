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
//! - Lin `hop`, hybrid `search`, and CAS are not claimed here.
//! - SQL `LIKE '%wal%'` ≈ Lin `title ~ "wal"` (substring), not `has` / FTS.
//! - Server DBs are not in-process; network/IPC cost is part of their number.

use std::collections::HashMap;
use std::hint::black_box;
use std::time::Duration;

use mysql::prelude::Queryable;
use rbench::{Config, DropPolicy, Fixture, Suite};

const N: usize = 10_000;
const INSERT_1K: usize = 1_000;
const INSERT_10K: usize = 10_000;
/// Probe row index inside the seeded set (wing=rag, title contains "wal").
const PROBE: usize = 20;

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
        .prepare(r#"docs | title ~ "wal" | count by layer"#)
        .expect("lin prepare text_substr");
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
    let mat = materialize.run(&mut db).expect("lin materialize sanity");
    assert_eq!(mat.done.n, n / 2, "materialize should take all rag rows");
    LinWarm {
        db,
        point_get,
        filter_eq,
        filter_range,
        text_substr,
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
            stmt.execute(duckdb::params![
                d.id, d.uri, d.wing, d.title, d.ts, d.body
            ])
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
                .execute(
                    &stmt,
                    &[&d.id, &d.uri, &d.wing, &d.title, &d.ts, &d.body],
                )
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
            conn.exec_drop(
                &stmt,
                (&d.id, &d.uri, &d.wing, &d.title, d.ts, &d.body),
            )
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

fn empty_lin() -> lin::Db {
    let mut db = lin::Db::empty();
    db.run("index docs [wing, ts]").expect("lin index");
    db
}

struct LinInsert {
    db: lin::Db,
    prepared: lin::Prepared,
}

fn setup_lin_insert(n: usize) -> LinInsert {
    let mut db = empty_lin();
    let src = lin_insert_src(&docs(n));
    let prepared = db.prepare(&src).expect("lin prepare insert");
    LinInsert { db, prepared }
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
        stmt.execute(duckdb::params![
            d.id, d.uri, d.wing, d.title, d.ts, d.body
        ])
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
        tx.execute(
            &stmt,
            &[&d.id, &d.uri, &d.wing, &d.title, &d.ts, &d.body],
        )
        .expect("pg insert");
    }
    tx.commit().expect("pg commit");
}

fn fill_mysql(conn: &mut mysql::Conn, n: usize) {
    let rows = docs(n);
    let mut tx = conn.start_transaction(mysql::TxOpts::default()).expect("mysql tx");
    let stmt = tx
        .prep("INSERT INTO docs_bulk (id, uri, wing, title, ts, body) VALUES (?,?,?,?,?,?)")
        .expect("mysql prepare");
    for d in &rows {
        tx.exec_drop(
            &stmt,
            (&d.id, &d.uri, &d.wing, &d.title, d.ts, &d.body),
        )
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
    let dir = fresh_tmp("append");
    let mut db = lin::Db::open(&dir.0).expect("lin durable open");
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

fn main() -> rbench::Result<()> {
    let pg = connect_pg();
    let mysql = connect_mysql();

    let mut engines = String::from(
        "Lin | SQLite(rusqlite bundled) | DuckDB(bundled) | HashMap(point get only)",
    );
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
         Lin reads: prepare once / run many; filters use count (fair vs SQL COUNT(*));\n\
         substring: Lin `title ~ \"wal\"` vs SQL LIKE '%wal%';\n\
         materialize: Lin `wing==rag | {{id,title}} | take all` vs SQL SELECT id,title;\n\
         append_log: Lin `append facts` vs SQL INSERT INTO logs (append-only shape)"
    );

    let mut suite = Suite::new("compare");
    suite.config(Config::profile("quick")?);

    let lin = Fixture::new(|| seed_lin(N));
    let sql = Fixture::new(|| seed_sqlite(N));
    let duck = Fixture::new(|| seed_duck(N));
    let map = Fixture::new(|| seed_map(N));
    let (pg_fix, pg_url) = match pg {
        Some((url, client)) => (
            Some(Fixture::new(move || seed_pg(N, client))),
            Some(url),
        ),
        None => (None, None),
    };
    let (mysql_fix, mysql_url) = match mysql {
        Some((url, conn)) => (
            Some(Fixture::new(move || seed_mysql(N, conn))),
            Some(url),
        ),
        None => (None, None),
    };

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
            black_box(lin_hits(
                &s.filter_eq.run(&mut s.db).expect("lin filter"),
            ))
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
            black_box(lin_hits(
                &s.filter_range.run(&mut s.db).expect("lin range"),
            ))
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
            black_box(lin_hits(
                &s.text_substr.run(&mut s.db).expect("lin ~"),
            ))
        })
        .tag("text_substr")
        .tag("lin")
        .parameter("n", N);
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
                let rows = s
                    .client
                    .query(&s.materialize, &[])
                    .expect("pg mat");
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
                let rows: Vec<(String, String)> = s
                    .conn
                    .exec(&s.materialize, ())
                    .expect("mysql mat");
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

    let args: Vec<String> = std::env::args()
        .skip(1)
        .filter(|a| a != "--bench")
        .collect();
    run_suite(suite, &args)
}

/// Minimal CLI compatible with `Suite::main`, ignoring cargo's injected `--bench`.
fn run_suite(mut suite: Suite<'_>, args: &[String]) -> rbench::Result<()> {
    let mut config = Config::profile("quick")?;
    let mut profile = None;
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--profile" {
            i += 1;
            profile = Some(
                args.get(i)
                    .ok_or_else(|| rbench::error("--profile requires value"))?
                    .clone(),
            );
        }
        i += 1;
    }
    if let Some(p) = &profile {
        config = Config::profile(p)?;
    }
    let mut selection = rbench::Selection::default();
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
                    .ok_or_else(|| rbench::error("--filter requires value"))?
                    .clone();
            }
            "--tag" => {
                i += 1;
                selection.tags.push(
                    args.get(i)
                        .ok_or_else(|| rbench::error("--tag requires value"))?
                        .clone(),
                );
            }
            "--exclude" => {
                i += 1;
                selection.exclude.push(
                    args.get(i)
                        .ok_or_else(|| rbench::error("--exclude requires glob"))?
                        .clone(),
                );
            }
            "--samples" => {
                i += 1;
                config.samples = args
                    .get(i)
                    .ok_or_else(|| rbench::error("--samples requires value"))?
                    .parse()?;
            }
            "--sample-ms" => {
                i += 1;
                config.sample_time = Duration::from_millis(
                    args.get(i)
                        .ok_or_else(|| rbench::error("--sample-ms requires value"))?
                        .parse()?,
                );
            }
            "--warmup-ms" => {
                i += 1;
                config.warmup = Duration::from_millis(
                    args.get(i)
                        .ok_or_else(|| rbench::error("--warmup-ms requires value"))?
                        .parse()?,
                );
            }
            "--output" => {
                i += 1;
                output = Some(
                    args.get(i)
                        .ok_or_else(|| rbench::error("--output requires directory"))?
                        .clone(),
                );
            }
            "--help" | "-h" => {
                println!(
                    "--list --profile quick|normal|thorough --filter TEXT [--exact|--glob] --exclude GLOB --tag TAG --samples N --sample-ms N --warmup-ms N --json --output NEW_DIRECTORY"
                );
                return Ok(());
            }
            other => return Err(rbench::error(format!("unknown argument {other}"))),
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
        return Err(rbench::error(
            "benchmarks require an optimized build; use cargo bench or cargo run --release",
        ));
    }
    let run = suite.run_selected(&selection)?;
    if let Some(p) = output {
        run.save_new(p)?;
    }
    if json {
        println!("RBENCH_RESULT={}", serde_json::to_string(&run)?);
    } else {
        println!("{}", rbench::report::markdown(&run)?);
    }
    Ok(())
}
