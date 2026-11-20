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
//! - Audit group (`update_1row_*` / `delete_1row_*` / `plan_dnf_*` / `graph_depth_*` /
//!   `reader_snapshot`): Lin CAS writes vs SQLite single-row DML, planner DNF growth,
//!   graph walks, snapshot open. `graph_depth` / `plan_dnf` are Lin-only (no SQL analogue).
//! - SQL `LIKE '%wal%'` ≈ Lin `title ~ "wal"` (substring), not `has` / FTS.
//! - Server DBs are not in-process; network/IPC cost is part of their number.

use std::collections::HashMap;
use std::hint::black_box;
use std::time::Duration;

use airbug_bench::{Config, DropPolicy, Fixture, Suite};
use lin::Queryable as LinQueryable;
use lin::query::pred;
use mysql::prelude::Queryable;

fn bench_n() -> usize {
    std::env::var("LIN_BENCH_N")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(10_000)
}
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
    // Optional mixed-case fixture exercises FTS normalization in every engine.
    let mixed_case = std::env::var_os("LIN_BENCH_MIXED_CASE").is_some();
    // All engines and fresh fixtures in this process share a timestamp anchor.
    static FIXTURE_NOW: std::sync::OnceLock<i64> = std::sync::OnceLock::new();
    let now = *FIXTURE_NOW.get_or_init(|| {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64
    });
    (0..n)
        .map(|i| {
            let wal = i % 10 == 0;
            Doc {
                id: format!("d-{i}"),
                uri: format!("bench://{i}"),
                wing: if i % 2 == 0 { "rag" } else { "sys" }.into(),
                title: if mixed_case && wal {
                    format!("Doc {i} WAL Note")
                } else if mixed_case {
                    format!("Doc {i} Plain")
                } else if wal {
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
        out.push_str(&format!(
            r#"  {{ id: "{}", uri: "{}", title: "{}", layer: "wiki", wing: "{}", body: "{}", ts: timestamp({}) }}"#,
            escape_lin(&d.id),
            escape_lin(&d.uri),
            escape_lin(&d.title),
            escape_lin(&d.wing),
            escape_lin(&d.body),
            d.ts
        ));
    }
    out.push_str("\n]");
    out
}

/// Verify exact input values and scalar-index shape before measuring warm work.
fn validate_lin_fixture_timestamps(db: &lin::Db, fixtures: &[Doc]) {
    let rows = db.store.collection("docs");
    assert_eq!(rows.len(), fixtures.len());
    let step = if fixtures.len() >= 100_000 { 100 } else { 1 };
    for (row, fixture) in rows.iter().step_by(step).zip(fixtures.iter().step_by(step)) {
        assert_eq!(
            row.get("id").and_then(lin::Cell::text),
            Some(fixture.id.as_str())
        );
        assert_eq!(row.get("ts"), Some(&lin::Cell::Time(fixture.ts)));
    }
    for index in db
        .store
        .indexes
        .values()
        .filter(|index| index.def.collection == "docs")
    {
        assert_eq!(
            index.forward.len(),
            if fixtures.len() >= 2 {
                2
            } else {
                fixtures.len()
            }
        );
    }
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

/// A fixture owns one persistent schema; default/public tables are never touched.
struct PgBenchClient {
    client: postgres::Client,
    schema: String,
}

impl PgBenchClient {
    fn new(mut client: postgres::Client) -> Result<Self, postgres::Error> {
        static SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let sequence = SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let schema = format!("lin_bench_{}_{}_{}", std::process::id(), stamp, sequence);
        client.batch_execute(&format!("CREATE SCHEMA \"{schema}\""))?;
        let mut owned = Self { client, schema };
        owned
            .client
            .batch_execute(&format!("SET search_path TO \"{}\"", owned.schema))?;
        Ok(owned)
    }
}

impl std::ops::Deref for PgBenchClient {
    type Target = postgres::Client;
    fn deref(&self) -> &Self::Target {
        &self.client
    }
}

impl std::ops::DerefMut for PgBenchClient {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.client
    }
}

impl Drop for PgBenchClient {
    fn drop(&mut self) {
        if let Err(error) = self
            .client
            .batch_execute(&format!("DROP SCHEMA \"{}\" CASCADE", self.schema))
        {
            eprintln!("failed to clean owned PostgreSQL benchmark schema: {error}");
        }
    }
}

fn try_mysql_conn(url: &str) -> Result<mysql::Conn, String> {
    let opts = mysql::Opts::from_url(url).map_err(|e| e.to_string())?;
    mysql::Conn::new(opts).map_err(|e| e.to_string())
}

fn connect_pg() -> Option<(String, postgres::Client)> {
    if std::env::var_os("LIN_BENCH_SKIP_PG").is_some() {
        eprintln!("skip postgres: LIN_BENCH_SKIP_PG is set");
        return None;
    }
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
    if std::env::var_os("LIN_BENCH_SKIP_MYSQL").is_some() {
        eprintln!("skip mysql: LIN_BENCH_SKIP_MYSQL is set");
        return None;
    }
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
    count_by_wing: lin::Prepared,
}

fn seed_lin(n: usize) -> LinWarm {
    let rows = docs(n);
    let mut db = if n >= 1_000_000 {
        lin::Db::empty().with_quotas(lin::Quotas {
            max_output_bytes: 4 * 1024 * 1024 * 1024,
            max_execution_work: 500_000_000,
            ..Default::default()
        })
    } else {
        lin::Db::empty()
    };
    db.run("index docs [wing, ts]").expect("lin index");
    let chunk_size = if n >= 1_000_000 {
        10_000
    } else if n >= 100_000 {
        5_000
    } else {
        500
    };
    for chunk in rows.chunks(chunk_size) {
        db.run(&lin_insert_src(chunk)).expect("lin seed");
    }
    validate_lin_fixture_timestamps(&db, &rows);
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
    let count_by_wing = db
        .prepare(r#"docs | count by wing"#)
        .expect("lin prepare count_by_wing");
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
        count_by_wing,
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
    conn.execute_batch("BEGIN TRANSACTION;")
        .expect("sqlite begin");
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
    conn.execute_batch("COMMIT;").expect("sqlite commit");
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
    conn.execute_batch("BEGIN TRANSACTION;")
        .expect("duck begin");
    {
        let mut stmt = conn
            .prepare("INSERT INTO docs (id, uri, wing, title, ts, body) VALUES (?,?,?,?,?,?)")
            .expect("duck prepare");
        for d in &rows {
            stmt.execute(duckdb::params![d.id, d.uri, d.wing, d.title, d.ts, d.body])
                .expect("duck seed");
        }
    }
    conn.execute_batch("COMMIT;").expect("duck commit");
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
    client: PgBenchClient,
    probe_id: String,
    point_get: postgres::Statement,
    filter_eq: postgres::Statement,
    filter_range: postgres::Statement,
    text_substr: postgres::Statement,
    materialize: postgres::Statement,
}

fn seed_pg(n: usize, client: postgres::Client) -> PgWarm {
    let mut client = PgBenchClient::new(client).expect("pg owned schema");
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

/// Each fixture owns regular InnoDB tables with a unique, bounded identifier.
struct MysqlBenchConn {
    conn: mysql::Conn,
    prefix: String,
}

impl MysqlBenchConn {
    fn new(conn: mysql::Conn) -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64;
        let seq = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Self {
            conn,
            prefix: format!("lin_bench_{:x}_{stamp:x}_{seq:x}", std::process::id()),
        }
    }

    // Only trusted benchmark SQL templates use this identifier substitution.
    fn owned_sql(&self, sql: &str) -> String {
        let mut out = String::with_capacity(sql.len() + self.prefix.len());
        let bytes = sql.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            let start = i;
            if matches!(bytes[i], b'\'' | b'"' | b'`') {
                let quote = bytes[i];
                i += 1;
                while i < bytes.len() {
                    if bytes[i] == b'\\' {
                        i = (i + 2).min(bytes.len());
                        continue;
                    }
                    if bytes[i] == quote {
                        i += 1;
                        if i < bytes.len() && bytes[i] == quote {
                            i += 1;
                            continue;
                        }
                        break;
                    }
                    i += 1;
                }
                out.push_str(&sql[start..i]);
            } else if bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_' {
                i += 1;
                while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                    i += 1;
                }
                self.push_identifier(&mut out, &sql[start..i]);
            } else {
                let ch = sql[i..].chars().next().unwrap();
                out.push(ch);
                i += ch.len_utf8();
            }
        }
        out
    }

    fn push_identifier(&self, out: &mut String, token: &str) {
        if matches!(
            token,
            "docs" | "docs_bulk" | "users" | "orders" | "logs_bulk"
        ) {
            out.push('`');
            out.push_str(&self.prefix);
            out.push('_');
            out.push_str(token);
            out.push('`');
        } else {
            out.push_str(token);
        }
    }

    fn query_drop(&mut self, sql: impl AsRef<str>) -> mysql::Result<()> {
        let sql = self.owned_sql(sql.as_ref());
        self.conn.query_drop(sql)
    }

    fn prep(&mut self, sql: impl AsRef<str>) -> mysql::Result<mysql::Statement> {
        let sql = self.owned_sql(sql.as_ref());
        self.conn.prep(sql)
    }
}

impl std::ops::Deref for MysqlBenchConn {
    type Target = mysql::Conn;
    fn deref(&self) -> &Self::Target {
        &self.conn
    }
}
impl std::ops::DerefMut for MysqlBenchConn {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.conn
    }
}
impl Drop for MysqlBenchConn {
    fn drop(&mut self) {
        for table in ["orders", "users", "docs_bulk", "docs", "logs_bulk"] {
            let sql = self.owned_sql(&format!("DROP TABLE IF EXISTS {table}"));
            if let Err(error) = self.conn.query_drop(sql) {
                eprintln!("mysql owned table cleanup failed: {error}");
            }
        }
    }
}

struct MysqlWarm {
    conn: MysqlBenchConn,
    probe_id: String,
    point_get: mysql::Statement,
    filter_eq: mysql::Statement,
    filter_range: mysql::Statement,
    text_substr: mysql::Statement,
    materialize: mysql::Statement,
}

fn seed_mysql(n: usize, conn: mysql::Conn) -> MysqlWarm {
    let mut conn = MysqlBenchConn::new(conn);
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
         ) ENGINE=InnoDB",
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
    agg_sum: lin::Prepared,
    agg_sum_by: lin::Prepared,
    window_sum: lin::Prepared,
    window_rank: lin::Prepared,
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
    let agg_sum = db
        .prepare(r#"orders | sum total"#)
        .expect("lin prepare agg_sum");
    let agg_sum_by = db
        .prepare(r#"orders | sum total by user_id"#)
        .expect("lin prepare agg_sum_by");
    let window_sum = db
        .prepare(
            r#"orders | window sum total by user_id sort id asc as running | pick { id, running } | take all"#,
        )
        .expect("lin prepare window_sum");
    let window_rank = db
        .prepare(
            r#"orders | window rank by user_id sort total desc as rnk | pick { id, rnk } | take all"#,
        )
        .expect("lin prepare window_rank");
    assert_eq!(
        window_sum.run(&mut db).expect("lin window_sum sanity").done.n,
        JOIN_ORDERS
    );
    assert_eq!(
        window_rank.run(&mut db).expect("lin window_rank sanity").done.n,
        JOIN_ORDERS
    );
    JoinLin {
        db,
        inner,
        filter,
        inner_q,
        filter_q,
        orders_q,
        agg_sum,
        agg_sum_by,
        window_sum,
        window_rank,
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
    let conn = rusqlite::Connection::open_in_memory().expect("sqlite open");
    fill_join_sqlite(&conn);
    JoinSql { conn }
}

fn fill_join_sqlite(conn: &rusqlite::Connection) {
    let (users, orders) = join_data();
    conn.execute_batch("BEGIN TRANSACTION;")
        .expect("sqlite join begin");
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
    conn.execute_batch("COMMIT;").expect("sqlite join commit");
    let _ = conn.prepare_cached(JOIN_INNER_SQL);
    let _ = conn.prepare_cached(JOIN_FILTER_SQL);
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
    let conn = duckdb::Connection::open_in_memory().expect("duckdb open");
    fill_join_duck(&conn);
    JoinDuck { conn }
}

fn fill_join_duck(conn: &duckdb::Connection) {
    let (users, orders) = join_data();
    conn.execute_batch("BEGIN TRANSACTION;")
        .expect("duck join begin");
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
    conn.execute_batch("COMMIT;").expect("duck join commit");
}

/// Disk-backed `orders ⋈ users` fixtures: Lin cold tier vs file-backed SQLite / DuckDB.
struct ColdJoinLin {
    _dir: TmpKeep,
    db: lin::Db,
    inner: lin::Prepared,
    filter: lin::Prepared,
    agg_sum: lin::Prepared,
    agg_sum_by: lin::Prepared,
}

fn seed_cold_join_lin() -> ColdJoinLin {
    let (users, orders) = join_data();
    let dir = fresh_tmp("cold_join");
    let opts = || lin::OpenOpts {
        sync: lin::SyncMode::Full,
        cold: true,
    };
    {
        let mut db = lin::Db::open_with(&dir.0, opts()).expect("open cold join");
        db.run("index orders [user_id]").expect("lin cold orders index");
        for chunk in users.chunks(500) {
            db.run(&lin_insert_users(chunk)).expect("lin cold seed users");
        }
        for chunk in orders.chunks(500) {
            db.run(&lin_insert_orders(chunk)).expect("lin cold seed orders");
        }
        db.checkpoint().expect("cold join checkpoint");
        db.close().expect("cold join close");
    }
    let mut db = lin::Db::open_with(&dir.0, opts()).expect("reopen cold join");
    let inner = db
        .prepare(r#"orders | join users on user_id | { id, users.email, total } | take all"#)
        .expect("lin cold prepare join inner");
    let filter = db
        .prepare(
            r#"orders | total > 100 | join users on user_id | { id, users.email, total } | take all"#,
        )
        .expect("lin cold prepare join filter");
    let agg_sum = db
        .prepare(r#"orders | sum total"#)
        .expect("lin cold prepare agg_sum");
    let agg_sum_by = db
        .prepare(r#"orders | sum total by user_id"#)
        .expect("lin cold prepare agg_sum_by");
    assert_eq!(
        inner.run_batch(&mut db).expect("lin cold join sanity").n(),
        JOIN_ORDERS
    );
    assert_eq!(
        filter.run_batch(&mut db).expect("lin cold join filter sanity").n(),
        JOIN_FILTER_N
    );
    assert_eq!(
        agg_sum_by.run(&mut db).expect("lin cold sum by sanity").done.n,
        JOIN_USERS
    );
    let _ = agg_sum.run(&mut db).expect("lin cold sum sanity");
    for c in ["orders", "users"] {
        assert!(
            db.store.is_disk_backed(c),
            "cold join queries must not promote {c}"
        );
    }
    ColdJoinLin {
        _dir: dir,
        db,
        inner,
        filter,
        agg_sum,
        agg_sum_by,
    }
}

struct ColdJoinSql {
    _dir: TmpKeep,
    conn: rusqlite::Connection,
}

fn seed_cold_join_sqlite() -> ColdJoinSql {
    let dir = fresh_tmp("sqlite_cold_join");
    let conn = rusqlite::Connection::open(dir.0.join("join.sqlite")).expect("sqlite cold join");
    fill_join_sqlite(&conn);
    ColdJoinSql { _dir: dir, conn }
}

struct ColdJoinDuck {
    _dir: TmpKeep,
    conn: duckdb::Connection,
}

fn seed_cold_join_duck() -> ColdJoinDuck {
    let dir = fresh_tmp("duck_cold_join");
    let conn = duckdb::Connection::open(dir.0.join("join.duckdb")).expect("duck cold join");
    fill_join_duck(&conn);
    conn.execute_batch("CHECKPOINT;").expect("duck cold join checkpoint");
    ColdJoinDuck { _dir: dir, conn }
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
    client: PgBenchClient,
    inner: postgres::Statement,
    filter: postgres::Statement,
}

fn seed_join_pg(client: postgres::Client) -> JoinPg {
    let mut client = PgBenchClient::new(client).expect("pg owned join schema");
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
    conn: MysqlBenchConn,
    inner: mysql::Statement,
    filter: mysql::Statement,
}

fn seed_join_mysql(conn: mysql::Conn) -> JoinMysql {
    let mut conn = MysqlBenchConn::new(conn);
    let (users, orders) = join_data();
    conn.query_drop("DROP TABLE IF EXISTS orders")
        .expect("mysql drop orders");
    conn.query_drop("DROP TABLE IF EXISTS users")
        .expect("mysql drop users");
    conn.query_drop("CREATE TABLE users (id VARCHAR(64) PRIMARY KEY, email VARCHAR(255) NOT NULL) ENGINE=InnoDB")
        .expect("mysql users");
    conn.query_drop(
        "CREATE TABLE orders (
            id VARCHAR(64) PRIMARY KEY,
            user_id VARCHAR(64) NOT NULL,
            total DOUBLE NOT NULL,
            INDEX orders_user_id (user_id)
         ) ENGINE=InnoDB",
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
fn empty_pg(url: String) -> PgBenchClient {
    let mut client = PgBenchClient::new(try_pg_client(&url).expect("pg reconnect"))
        .expect("pg owned bulk schema");
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

fn empty_mysql(url: String) -> MysqlBenchConn {
    let mut conn = MysqlBenchConn::new(try_mysql_conn(&url).expect("mysql reconnect"));
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
         ) ENGINE=InnoDB",
    )
    .expect("mysql bulk schema");
    conn
}

fn fill_lin(ins: &mut LinInsert) -> usize {
    ins.prepared.run(&mut ins.db).expect("lin insert").done.n
}

fn validate_lin_insert(ins: &LinInsert, n: usize) {
    validate_lin_rows(&ins.db, n);
}

fn validate_lin_rows(db: &lin::Db, n: usize) {
    let rows = db.store.collection("docs");
    assert_eq!(rows.len(), n);
    let fixtures = docs(n);
    for (i, row) in rows.iter().enumerate() {
        let text = |key: &str| row.get(key).and_then(lin::Cell::text);
        assert_eq!(text("id"), Some(format!("d-{i}").as_str()));
        assert_eq!(text("uri"), Some(format!("bench://{i}").as_str()));
        assert_eq!(text("wing"), Some(if i % 2 == 0 { "rag" } else { "sys" }));
        assert_eq!(text("title"), Some(fixtures[i].title.as_str()));
        assert_eq!(text("body"), Some(format!("body {i}").as_str()));
        assert_eq!(row.get("ts"), Some(&lin::Cell::Time(fixtures[i].ts)));
        assert!(row.get("embedding").and_then(lin::Cell::as_vec).is_some());
    }
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
    fill_sqlite_rows(conn, &docs(n));
}

fn validate_sqlite_docs(conn: &rusqlite::Connection, expected: &[Doc]) -> airbug_bench::Result<()> {
    let mut stmt = conn
        .prepare("SELECT id, uri, wing, title, ts, body FROM docs ORDER BY id")
        .unwrap();
    let got = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, i64>(4)?,
                r.get::<_, String>(5)?,
            ))
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    let mut want = expected
        .iter()
        .map(|d| {
            (
                d.id.clone(),
                d.uri.clone(),
                d.wing.clone(),
                d.title.clone(),
                d.ts,
                d.body.clone(),
            )
        })
        .collect::<Vec<_>>();
    want.sort();
    assert_eq!(got, want, "SQLite native ingestion readback");
    Ok(())
}

fn fill_sqlite_rows(conn: &rusqlite::Connection, rows: &[Doc]) {
    let tx = conn.unchecked_transaction().expect("tx");
    {
        let mut stmt = tx
            .prepare("INSERT INTO docs (id, uri, wing, title, ts, body) VALUES (?1,?2,?3,?4,?5,?6)")
            .expect("prepare");
        for d in rows {
            stmt.execute(rusqlite::params![
                d.id, d.uri, d.wing, d.title, d.ts, d.body
            ])
            .expect("insert");
        }
    }
    tx.commit().expect("commit");
}

fn fill_duck_appender(conn: &duckdb::Connection, rows: &[Doc]) {
    let mut appender = conn.appender("docs").expect("duck appender");
    for d in rows {
        appender
            .append_row(duckdb::params![d.id, d.uri, d.wing, d.title, d.ts, d.body])
            .expect("duck append row");
    }
    appender.flush().expect("duck appender flush");
}

fn fill_duck_rows(conn: &duckdb::Connection, rows: &[Doc]) {
    let mut stmt = conn
        .prepare("INSERT INTO docs (id, uri, wing, title, ts, body) VALUES (?,?,?,?,?,?)")
        .expect("prepare");
    for d in rows {
        stmt.execute(duckdb::params![d.id, d.uri, d.wing, d.title, d.ts, d.body])
            .expect("insert");
    }
}

fn fill_pg_rows(client: &mut postgres::Client, rows: &[Doc]) {
    let mut tx = client.transaction().expect("pg tx");
    let stmt = tx
        .prepare(
            "INSERT INTO docs_bulk (id, uri, wing, title, ts, body) VALUES ($1,$2,$3,$4,$5,$6)",
        )
        .expect("pg prepare");
    for d in rows {
        tx.execute(&stmt, &[&d.id, &d.uri, &d.wing, &d.title, &d.ts, &d.body])
            .expect("pg insert");
    }
    tx.commit().expect("pg commit");
}

fn fill_pg_copy(client: &mut postgres::Client, rows: &[Doc]) -> u64 {
    use postgres::binary_copy::BinaryCopyInWriter;
    use postgres::types::Type;
    let mut tx = client.transaction().expect("pg copy tx");
    let count = {
        let sink = tx
            .copy_in("COPY docs_bulk (id, uri, wing, title, ts, body) FROM STDIN BINARY")
            .expect("pg copy start");
        let mut writer = BinaryCopyInWriter::new(
            sink,
            &[
                Type::TEXT,
                Type::TEXT,
                Type::TEXT,
                Type::TEXT,
                Type::INT8,
                Type::TEXT,
            ],
        );
        for d in rows {
            writer
                .write(&[&d.id, &d.uri, &d.wing, &d.title, &d.ts, &d.body])
                .expect("pg copy row");
        }
        writer.finish().expect("pg copy finish")
    };
    tx.commit().expect("pg copy commit");
    count
}

fn validate_pg_docs(
    client: &PgBenchClient,
    url: &str,
    expected: &[Doc],
) -> airbug_bench::Result<()> {
    let mut reader = try_pg_client(url).expect("pg readback connection");
    let sql = format!(
        "SELECT id, uri, wing, title, ts, body FROM \"{}\".docs_bulk",
        client.schema
    );
    let mut got = reader
        .query(&sql, &[])
        .expect("pg readback")
        .into_iter()
        .map(|r| {
            (
                r.get::<_, String>(0),
                r.get::<_, String>(1),
                r.get::<_, String>(2),
                r.get::<_, String>(3),
                r.get::<_, i64>(4),
                r.get::<_, String>(5),
            )
        })
        .collect::<Vec<_>>();
    let mut want = expected
        .iter()
        .map(|d| {
            (
                d.id.clone(),
                d.uri.clone(),
                d.wing.clone(),
                d.title.clone(),
                d.ts,
                d.body.clone(),
            )
        })
        .collect::<Vec<_>>();
    got.sort();
    want.sort();
    assert_eq!(got, want, "PostgreSQL native ingestion readback");
    Ok(())
}

fn fill_mysql_rows(conn: &mut MysqlBenchConn, rows: &[Doc]) {
    let sql = conn
        .owned_sql("INSERT INTO docs_bulk (id, uri, wing, title, ts, body) VALUES (?,?,?,?,?,?)");
    let mut tx = conn
        .start_transaction(mysql::TxOpts::default())
        .expect("mysql tx");
    let stmt = tx.prep(sql).expect("mysql prepare");
    for d in rows {
        tx.exec_drop(&stmt, (&d.id, &d.uri, &d.wing, &d.title, d.ts, &d.body))
            .expect("mysql insert");
    }
    tx.commit().expect("mysql commit");
}

fn validate_mysql_docs(
    conn: &MysqlBenchConn,
    url: &str,
    expected: &[Doc],
) -> airbug_bench::Result<()> {
    let mut reader = try_mysql_conn(url).expect("mysql validation connection");
    let sql = conn.owned_sql("SELECT id, uri, wing, title, ts, body FROM docs_bulk");
    let mut got: Vec<(String, String, String, String, i64, String)> =
        reader.query(sql).expect("mysql exact readback");
    let mut want = expected
        .iter()
        .map(|d| {
            (
                d.id.clone(),
                d.uri.clone(),
                d.wing.clone(),
                d.title.clone(),
                d.ts,
                d.body.clone(),
            )
        })
        .collect::<Vec<_>>();
    got.sort();
    want.sort();
    assert_eq!(got, want);
    Ok(())
}

struct MysqlNativeInsert {
    conn: MysqlBenchConn,
    statement: mysql::Statement,
    rows: Vec<Doc>,
    url: String,
}

fn setup_mysql_native(url: String, n: usize) -> MysqlNativeInsert {
    assert!(
        n > 0 && n * 6 <= 65535,
        "mysql native fixture exceeds prepared parameter limit"
    );
    let mut conn = empty_mysql(url.clone());
    let values = std::iter::repeat_n("(?,?,?,?,?,?)", n)
        .collect::<Vec<_>>()
        .join(",");
    let statement = conn
        .prep(format!(
            "INSERT INTO docs_bulk (id, uri, wing, title, ts, body) VALUES {values}"
        ))
        .expect("mysql native prepare");
    MysqlNativeInsert {
        conn,
        statement,
        rows: docs(n),
        url,
    }
}

fn fill_mysql_native(input: &mut MysqlNativeInsert) -> usize {
    let mut values = Vec::with_capacity(input.rows.len() * 6);
    for d in &input.rows {
        values.extend([
            mysql::Value::Bytes(d.id.as_bytes().to_vec()),
            mysql::Value::Bytes(d.uri.as_bytes().to_vec()),
            mysql::Value::Bytes(d.wing.as_bytes().to_vec()),
            mysql::Value::Bytes(d.title.as_bytes().to_vec()),
            mysql::Value::Int(d.ts),
            mysql::Value::Bytes(d.body.as_bytes().to_vec()),
        ]);
    }
    let mut tx = input
        .conn
        .start_transaction(mysql::TxOpts::default())
        .expect("mysql native tx");
    tx.exec_drop(&input.statement, mysql::Params::Positional(values))
        .expect("mysql native insert");
    let affected = tx.affected_rows() as usize;
    tx.commit().expect("mysql native commit");
    affected
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

struct LinColdWarm {
    _dir: TmpKeep,
    db: lin::Db,
    point_get: lin::Prepared,
    filter_eq: lin::Prepared,
    materialize: lin::Prepared,
    count_by_wing: lin::Prepared,
    filter_range: lin::Prepared,
    text_substr: lin::Prepared,
    range_project: lin::Prepared,
    update_1row: lin::Prepared,
    delete_1row: lin::Prepared,
}

fn seed_lin_cold(n: usize) -> LinColdWarm {
    let rows = docs(n);
    let dir = fresh_tmp("cold_warm");
    let chunk_size = if n >= 100_000 { 5_000 } else { 500 };
    {
        let mut db = lin::Db::open_with(
            &dir.0,
            lin::OpenOpts {
                sync: lin::SyncMode::Full,
                cold: true,
            },
        )
        .expect("open cold");
        db.run("index docs [wing, ts]").expect("lin cold index");
        for chunk in rows.chunks(chunk_size) {
            db.run(&lin_insert_src(chunk)).expect("lin cold seed");
        }
        db.checkpoint().expect("cold checkpoint");
        db.close().expect("cold close");
    }
    let mut db = lin::Db::open_with(
        &dir.0,
        lin::OpenOpts {
            sync: lin::SyncMode::Full,
            cold: true,
        },
    )
    .expect("reopen cold");
    assert!(db.stats().cold, "expected cold mode");
    assert_eq!(db.stats().docs, n, "expected all docs in cold tier");
    assert!(
        db.store.is_disk_backed("docs"),
        "expected docs to be disk-backed after cold reopen"
    );
    let probe_id = &rows[PROBE].id;
    let point_get = db
        .prepare(&format!(
            r#"docs | id == "{}" | {{ id, title }}"#,
            escape_lin(probe_id)
        ))
        .expect("lin cold prepare point_get");
    let filter_eq = db
        .prepare(r#"docs | wing == "rag" | count by wing"#)
        .expect("lin cold prepare filter_eq");
    let materialize = db
        .prepare(r#"docs | wing == "rag" | { id, title } | take all"#)
        .expect("lin cold prepare materialize");
    let count_by_wing = db
        .prepare(r#"docs | count by wing"#)
        .expect("lin cold prepare count_by_wing");
    let filter_range = db
        .prepare(r#"docs | wing == "rag" and ts > ago 7d | count by wing"#)
        .expect("lin cold prepare filter_range");
    let text_substr = db
        .prepare(r#"docs | title ~ "wal" | count"#)
        .expect("lin cold prepare text_substr");
    let range_project = db
        .prepare(r#"docs | ts > ago 7d | { id, title } | take all"#)
        .expect("lin cold prepare range_project");
    let update_1row = db
        .prepare(&format!(
            r#"update docs[id == "{}"] cas each {{ title: "updated title" }}"#,
            escape_lin(probe_id)
        ))
        .expect("lin cold prepare update_1row");
    let delete_1row = db
        .prepare(&format!(
            r#"delete docs[id == "{}"] cas each"#,
            escape_lin(probe_id)
        ))
        .expect("lin cold prepare delete_1row");
    assert_eq!(
        lin_hits(&filter_range.run(&mut db).expect("lin cold filter_range sanity")),
        (n / 2) as i64
    );
    let expected_substr = if std::env::var_os("LIN_BENCH_MIXED_CASE").is_some() {
        0
    } else {
        n.div_ceil(10) as i64
    };
    assert_eq!(
        lin_hits(&text_substr.run(&mut db).expect("lin cold text_substr sanity")),
        expected_substr
    );
    assert_eq!(
        range_project
            .run(&mut db)
            .expect("lin cold range_project sanity")
            .done
            .n,
        n / 2
    );
    assert!(
        db.store.is_disk_backed("docs"),
        "cold queries must not promote docs"
    );
    let mat = materialize
        .run(&mut db)
        .expect("lin cold materialize sanity");
    assert_eq!(
        mat.done.n,
        n / 2,
        "cold materialize should take all rag rows"
    );
    assert_eq!(
        lin_hits(&filter_eq.run(&mut db).expect("lin cold filter_eq sanity")),
        (n / 2) as i64
    );
    LinColdWarm {
        _dir: dir,
        db,
        point_get,
        filter_eq,
        materialize,
        count_by_wing,
        filter_range,
        text_substr,
        range_project,
        update_1row,
        delete_1row,
    }
}

struct SqlColdWarm {
    _dir: TmpKeep,
    conn: rusqlite::Connection,
    probe_id: String,
}

fn seed_sqlite_cold(n: usize) -> SqlColdWarm {
    let rows = docs(n);
    let dir = fresh_tmp("sqlite_cold");
    let path = dir.0.join("cold.sqlite");
    let conn = rusqlite::Connection::open(&path).expect("sqlite cold open");
    conn.execute_batch(
        "PRAGMA synchronous = FULL;
         PRAGMA journal_mode = DELETE;",
    )
    .expect("sqlite cold pragma");
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
    .expect("sqlite cold schema");
    conn.execute_batch("BEGIN TRANSACTION;")
        .expect("sqlite cold begin");
    {
        let mut stmt = conn
            .prepare("INSERT INTO docs (id, uri, wing, title, ts, body) VALUES (?1,?2,?3,?4,?5,?6)")
            .expect("sqlite cold prepare");
        for d in &rows {
            stmt.execute(rusqlite::params![
                d.id, d.uri, d.wing, d.title, d.ts, d.body
            ])
            .expect("sqlite cold seed");
        }
    }
    conn.execute_batch("COMMIT;").expect("sqlite cold commit");
    {
        let _ = conn.prepare_cached("SELECT COUNT(*) FROM docs WHERE id = ?1");
        let _ = conn.prepare_cached("SELECT COUNT(*) FROM docs WHERE wing = 'rag'");
        let _ = conn.prepare_cached("SELECT wing, COUNT(*) FROM docs GROUP BY wing");
        let _ = conn.prepare_cached("SELECT id, title FROM docs WHERE wing = 'rag'");
        let _ = conn.prepare_cached("UPDATE docs SET title = 'updated title' WHERE id = ?1");
        let _ = conn.prepare_cached("DELETE FROM docs WHERE id = ?1");
    }
    SqlColdWarm {
        _dir: dir,
        probe_id: rows[PROBE].id.clone(),
        conn,
    }
}

fn seed_sqlite_cold_fullsync(n: usize) -> SqlColdWarm {
    let s = seed_sqlite_cold(n);
    s.conn
        .execute_batch("PRAGMA fullfsync = ON;")
        .expect("sqlite cold fullfsync");
    s
}

struct DuckColdWarm {
    _dir: TmpKeep,
    conn: duckdb::Connection,
    probe_id: String,
}

fn seed_duck_cold(n: usize) -> DuckColdWarm {
    let rows = docs(n);
    let dir = fresh_tmp("duck_cold");
    let path = dir.0.join("cold.duckdb");
    let conn = duckdb::Connection::open(&path).expect("duckdb cold open");
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
    .expect("duckdb cold schema");
    conn.execute_batch("BEGIN TRANSACTION;")
        .expect("duckdb cold begin");
    {
        let mut stmt = conn
            .prepare("INSERT INTO docs (id, uri, wing, title, ts, body) VALUES (?,?,?,?,?,?)")
            .expect("duckdb cold prepare");
        for d in &rows {
            stmt.execute(duckdb::params![d.id, d.uri, d.wing, d.title, d.ts, d.body])
                .expect("duckdb cold seed");
        }
    }
    conn.execute_batch("COMMIT; CHECKPOINT;")
        .expect("duckdb cold commit");
    DuckColdWarm {
        _dir: dir,
        probe_id: rows[PROBE].id.clone(),
        conn,
    }
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

fn empty_pg_logs(url: String) -> PgBenchClient {
    let mut client = PgBenchClient::new(try_pg_client(&url).expect("pg reconnect"))
        .expect("pg owned logs schema");
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

fn empty_mysql_logs(url: String) -> MysqlBenchConn {
    let mut conn = MysqlBenchConn::new(try_mysql_conn(&url).expect("mysql reconnect"));
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

fn fill_mysql_logs(conn: &mut MysqlBenchConn, n: usize) {
    let sql = conn.owned_sql("INSERT INTO logs_bulk (id, ts, msg) VALUES (?, ?, ?)");
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    let mut tx = conn
        .start_transaction(mysql::TxOpts::default())
        .expect("mysql tx");
    let stmt = tx.prep(sql).expect("mysql prep");
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
    let wal_dict = std::env::var("LIN_BENCH_WAL_DICT")
        .map(|v| matches!(v.as_str(), "1" | "true" | "on"))
        .unwrap_or(false);
    lin::persist::set_wal_dictionary_writer(wal_dict);
    let pg = connect_pg();
    let mysql = connect_mysql();

    let sqlite_ver: String = rusqlite::Connection::open_in_memory()
        .ok()
        .and_then(|c| c.query_row("SELECT sqlite_version()", [], |r| r.get(0)).ok())
        .unwrap_or_else(|| "bundled".to_string());
    let duck_ver: String = duckdb::Connection::open_in_memory()
        .ok()
        .and_then(|c| c.query_row("SELECT version()", [], |r| r.get(0)).ok())
        .unwrap_or_else(|| "bundled".to_string());
    let mut engines = format!(
        "Lin({}) | SQLite({sqlite_ver}) | DuckDB({duck_ver}) | HashMap(point get only)",
        lin::VERSION
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

    #[allow(non_snake_case)]
    let N = bench_n();
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

    let lin = Fixture::new(move || seed_lin(N));
    let lin_rebuild = Fixture::new(|| seed_lin_rebuild(5_000));
    let sql = Fixture::new(move || seed_sqlite(N));
    let duck = Fixture::new(move || seed_duck(N));
    let map = Fixture::new(move || seed_map(N));
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
        .bench_fixture("materialize/lin", lin.clone(), |s| {
            black_box(s.materialize.run(&mut s.db).expect("lin mat").done.n)
        })
        .tag("materialize")
        .tag("lin")
        .parameter("n", N);
    suite
        .bench_fixture("materialize/sqlite", sql.clone(), |s| {
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
        .bench_fixture("materialize/duckdb", duck.clone(), |s| {
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
        .bench_fixture("join_filter/lin_cursor", lin_join.clone(), |s| {
            black_box(consume_join_cursor(&s.db, &s.filter_q))
        })
        .tag("join_filter")
        .tag("lin_cursor")
        .parameter("users", JOIN_USERS)
        .parameter("orders", JOIN_ORDERS)
        .work_units("rows", JOIN_FILTER_N as u64);
    suite
        .bench_fixture("join_filter/sqlite", sql_join.clone(), |s| {
            black_box(sqlite_join_n(&s.conn, JOIN_FILTER_SQL))
        })
        .tag("join_filter")
        .tag("sqlite")
        .parameter("users", JOIN_USERS)
        .parameter("orders", JOIN_ORDERS)
        .work_units("rows", JOIN_FILTER_N as u64);
    suite
        .bench_fixture("join_filter/duckdb", duck_join.clone(), |s| {
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

    // Heavy Aggregates: zero-copy SoA folds vs SQLite / DuckDB
    suite
        .bench_fixture("agg_sum/lin", lin_join.clone(), |s| {
            let res = s.agg_sum.run(&mut s.db).expect("lin sum");
            black_box(res.done.n)
        })
        .tag("agg")
        .tag("lin")
        .parameter("orders", JOIN_ORDERS)
        .work_units("rows", JOIN_ORDERS as u64);
    suite
        .bench_fixture("agg_sum/sqlite", sql_join.clone(), |s| {
            let mut stmt = s
                .conn
                .prepare_cached("SELECT SUM(total) FROM orders")
                .expect("sqlite prep");
            let sum: f64 = stmt.query_row([], |r| r.get(0)).expect("sqlite sum");
            black_box(sum)
        })
        .tag("agg")
        .tag("sqlite")
        .parameter("orders", JOIN_ORDERS)
        .work_units("rows", JOIN_ORDERS as u64);
    suite
        .bench_fixture("agg_sum/duckdb", duck_join.clone(), |s| {
            let mut stmt = s
                .conn
                .prepare_cached("SELECT SUM(total) FROM orders")
                .expect("duck prep");
            let sum: f64 = stmt.query_row([], |r| r.get(0)).expect("duck sum");
            black_box(sum)
        })
        .tag("agg")
        .tag("duckdb")
        .parameter("orders", JOIN_ORDERS)
        .work_units("rows", JOIN_ORDERS as u64);

    suite
        .bench_fixture("window_running_sum/lin", lin_join.clone(), |s| {
            black_box(s.window_sum.run(&mut s.db).expect("lin window_running_sum").done.n)
        })
        .tag("window")
        .tag("lin")
        .parameter("orders", JOIN_ORDERS)
        .work_units("rows", JOIN_ORDERS as u64);
    suite
        .bench_fixture("window_running_sum/lin_batch", lin_join.clone(), |s| {
            black_box(s.window_sum.run_batch(&mut s.db).expect("lin window_running_sum batch").n())
        })
        .tag("window")
        .tag("lin")
        .parameter("orders", JOIN_ORDERS)
        .work_units("rows", JOIN_ORDERS as u64);
    suite
        .bench_fixture("window_running_sum/sqlite", sql_join.clone(), |s| {
            let mut stmt = s
                .conn
                .prepare_cached("SELECT id, SUM(total) OVER (PARTITION BY user_id ORDER BY id) FROM orders")
                .expect("sqlite window_running_sum prep");
            let mut rows = stmt.query([]).expect("sqlite window_running_sum");
            let mut n = 0usize;
            while let Some(row) = rows.next().expect("sqlite window_running_sum row") {
                let _id: String = row.get(0).expect("id");
                let _v: f64 = row.get(1).expect("v");
                n += 1;
            }
            black_box(n)
        })
        .tag("window")
        .tag("sqlite")
        .parameter("orders", JOIN_ORDERS)
        .work_units("rows", JOIN_ORDERS as u64);
    suite
        .bench_fixture("window_running_sum/duckdb", duck_join.clone(), |s| {
            let mut stmt = s
                .conn
                .prepare_cached("SELECT id, SUM(total) OVER (PARTITION BY user_id ORDER BY id) FROM orders")
                .expect("duckdb window_running_sum prep");
            let mut rows = stmt.query([]).expect("duckdb window_running_sum");
            let mut n = 0usize;
            while let Some(row) = rows.next().expect("duckdb window_running_sum row") {
                let _id: String = row.get(0).expect("id");
                let _v: f64 = row.get(1).expect("v");
                n += 1;
            }
            black_box(n)
        })
        .tag("window")
        .tag("duckdb")
        .parameter("orders", JOIN_ORDERS)
        .work_units("rows", JOIN_ORDERS as u64);
    suite
        .bench_fixture("window_rank/lin", lin_join.clone(), |s| {
            black_box(s.window_rank.run(&mut s.db).expect("lin window_rank").done.n)
        })
        .tag("window")
        .tag("lin")
        .parameter("orders", JOIN_ORDERS)
        .work_units("rows", JOIN_ORDERS as u64);
    suite
        .bench_fixture("window_rank/lin_batch", lin_join.clone(), |s| {
            black_box(s.window_rank.run_batch(&mut s.db).expect("lin window_rank batch").n())
        })
        .tag("window")
        .tag("lin")
        .parameter("orders", JOIN_ORDERS)
        .work_units("rows", JOIN_ORDERS as u64);
    suite
        .bench_fixture("window_rank/sqlite", sql_join.clone(), |s| {
            let mut stmt = s
                .conn
                .prepare_cached("SELECT id, RANK() OVER (PARTITION BY user_id ORDER BY total DESC) FROM orders")
                .expect("sqlite window_rank prep");
            let mut rows = stmt.query([]).expect("sqlite window_rank");
            let mut n = 0usize;
            while let Some(row) = rows.next().expect("sqlite window_rank row") {
                let _id: String = row.get(0).expect("id");
                let _v: i64 = row.get(1).expect("v");
                n += 1;
            }
            black_box(n)
        })
        .tag("window")
        .tag("sqlite")
        .parameter("orders", JOIN_ORDERS)
        .work_units("rows", JOIN_ORDERS as u64);
    suite
        .bench_fixture("window_rank/duckdb", duck_join.clone(), |s| {
            let mut stmt = s
                .conn
                .prepare_cached("SELECT id, RANK() OVER (PARTITION BY user_id ORDER BY total DESC) FROM orders")
                .expect("duckdb window_rank prep");
            let mut rows = stmt.query([]).expect("duckdb window_rank");
            let mut n = 0usize;
            while let Some(row) = rows.next().expect("duckdb window_rank row") {
                let _id: String = row.get(0).expect("id");
                let _v: i64 = row.get(1).expect("v");
                n += 1;
            }
            black_box(n)
        })
        .tag("window")
        .tag("duckdb")
        .parameter("orders", JOIN_ORDERS)
        .work_units("rows", JOIN_ORDERS as u64);

    suite
        .bench_fixture("agg_sum_by/lin", lin_join, |s| {
            let res = s.agg_sum_by.run(&mut s.db).expect("lin sum by");
            black_box(res.done.n)
        })
        .tag("agg")
        .tag("lin")
        .parameter("orders", JOIN_ORDERS)
        .work_units("rows", JOIN_ORDERS as u64);
    suite
        .bench_fixture("agg_sum_by/sqlite", sql_join, |s| {
            let mut stmt = s
                .conn
                .prepare_cached("SELECT user_id, SUM(total) FROM orders GROUP BY user_id")
                .expect("sqlite prep");
            let mut rows = stmt.query([]).expect("sqlite sum by");
            let mut count = 0usize;
            while let Some(row) = rows.next().expect("sqlite row") {
                let _user: String = row.get(0).expect("user");
                let _sum: f64 = row.get(1).expect("sum");
                count += 1;
            }
            black_box(count)
        })
        .tag("agg")
        .tag("sqlite")
        .parameter("orders", JOIN_ORDERS)
        .work_units("rows", JOIN_ORDERS as u64);
    suite
        .bench_fixture("agg_sum_by/duckdb", duck_join, |s| {
            let mut stmt = s
                .conn
                .prepare_cached("SELECT user_id, SUM(total) FROM orders GROUP BY user_id")
                .expect("duck prep");
            let mut rows = stmt.query([]).expect("duck sum by");
            let mut count = 0usize;
            while let Some(row) = rows.next().expect("duck row") {
                let _user: String = row.get(0).expect("user");
                let _sum: f64 = row.get(1).expect("sum");
                count += 1;
            }
            black_box(count)
        })
        .tag("agg")
        .tag("duckdb")
        .parameter("orders", JOIN_ORDERS)
        .work_units("rows", JOIN_ORDERS as u64);

    suite
        .bench_fixture("agg_count_by/lin", lin, |s| {
            let res = s.count_by_wing.run(&mut s.db).expect("lin count by");
            black_box(res.done.n)
        })
        .tag("agg")
        .tag("lin")
        .parameter("docs", N)
        .work_units("rows", N as u64);
    suite
        .bench_fixture("agg_count_by/sqlite", sql, |s| {
            let mut stmt = s
                .conn
                .prepare_cached("SELECT wing, COUNT(*) FROM docs GROUP BY wing")
                .expect("sqlite prep");
            let mut rows = stmt.query([]).expect("sqlite count by");
            let mut count = 0usize;
            while let Some(row) = rows.next().expect("sqlite row") {
                let _wing: String = row.get(0).expect("wing");
                let _cnt: i64 = row.get(1).expect("cnt");
                count += 1;
            }
            black_box(count)
        })
        .tag("agg")
        .tag("sqlite")
        .parameter("docs", N)
        .work_units("rows", N as u64);
    suite
        .bench_fixture("agg_count_by/duckdb", duck, |s| {
            let mut stmt = s
                .conn
                .prepare_cached("SELECT wing, COUNT(*) FROM docs GROUP BY wing")
                .expect("duck prep");
            let mut rows = stmt.query([]).expect("duck count by");
            let mut count = 0usize;
            while let Some(row) = rows.next().expect("duck row") {
                let _wing: String = row.get(0).expect("wing");
                let _cnt: i64 = row.get(1).expect("cnt");
                count += 1;
            }
            black_box(count)
        })
        .tag("agg")
        .tag("duckdb")
        .parameter("docs", N)
        .work_units("rows", N as u64);

    macro_rules! insert_bulk {
        ($label:expr, $n:expr) => {{
            let n = $n;
            suite
                .bench_checked(
                    &format!("insert_bulk_{}/lin", $label),
                    move || setup_lin_insert(n),
                    move |ins| fill_lin(ins),
                    move |ins, affected| {
                        assert_eq!(*affected, n);
                        validate_lin_insert(ins, n);
                        Ok(())
                    },
                    DropPolicy::InsideTiming,
                )
                .tag("insert")
                .tag("lin")
                .parameter("rows", n)
                .work_units("rows", n as u64);
            suite
                .bench_checked(
                    &format!("insert_bulk_{}/sqlite", $label),
                    move || (empty_sqlite(), docs(n)),
                    |(conn, rows)| fill_sqlite_rows(conn, rows),
                    |(conn, expected), _| validate_sqlite_docs(conn, expected),
                    DropPolicy::InsideTiming,
                )
                .tag("insert")
                .tag("sqlite")
                .parameter("rows", n)
                .work_units("rows", n as u64);
            suite
                .bench_with_input(
                    &format!("insert_bulk_{}/duckdb", $label),
                    move || (empty_duck(), docs(n)),
                    |(conn, rows)| fill_duck_rows(conn, rows),
                    DropPolicy::InsideTiming,
                )
                .tag("insert")
                .tag("duckdb")
                .parameter("rows", n)
                .work_units("rows", n as u64);
            if let Some(url) = pg_url.clone() {
                let rowwise_url = url.clone();
                suite
                    .bench_checked(
                        &format!("insert_bulk_{}/postgres", $label),
                        move || (empty_pg(rowwise_url.clone()), docs(n), rowwise_url.clone()),
                        |(client, rows, _)| fill_pg_rows(client, rows),
                        |(client, expected, url), _| validate_pg_docs(client, url, expected),
                        DropPolicy::InsideTiming,
                    )
                    .tag("insert")
                    .tag("postgres")
                    .parameter("rows", n)
                    .work_units("rows", n as u64);
                suite
                    .bench_checked(
                        &format!("insert_native_{}/postgres_copy", $label),
                        move || (empty_pg(url.clone()), docs(n), url.clone()),
                        |(client, rows, _)| fill_pg_copy(client, rows),
                        |(client, expected, url), count| {
                            assert_eq!(*count, expected.len() as u64);
                            validate_pg_docs(client, url, expected)
                        },
                        DropPolicy::InsideTiming,
                    )
                    .tag("insert")
                    .tag("native")
                    .tag("postgres")
                    .parameter("rows", n)
                    .work_units("rows", n as u64);
            }
            if let Some(url) = mysql_url.clone() {
                suite
                    .bench_checked(
                        &format!("insert_bulk_{}/mysql", $label),
                        {
                            let url = url.clone();
                            move || (empty_mysql(url.clone()), docs(n), url.clone())
                        },
                        |(conn, rows, _)| fill_mysql_rows(conn, rows),
                        |(conn, rows, url), _| validate_mysql_docs(conn, url, rows),
                        DropPolicy::InsideTiming,
                    )
                    .tag("insert")
                    .tag("mysql")
                    .parameter("rows", n)
                    .work_units("rows", n as u64);
                suite
                    .bench_checked(
                        &format!("insert_native_{}/mysql_batch", $label),
                        move || setup_mysql_native(url.clone(), n),
                        fill_mysql_native,
                        |input, affected| {
                            assert_eq!(*affected, input.rows.len());
                            validate_mysql_docs(&input.conn, &input.url, &input.rows)
                        },
                        DropPolicy::InsideTiming,
                    )
                    .tag("insert")
                    .tag("native")
                    .tag("mysql")
                    .parameter("rows", n)
                    .work_units("rows", n as u64);
            }
        }};
    }
    insert_bulk!("1k", INSERT_1K);
    insert_bulk!("10k", INSERT_10K);
    // Optimized ingestion is a separate competitor from SQL row-by-row inserts.
    for (label, n) in [("1k", INSERT_1K), ("10k", INSERT_10K)] {
        suite
            .bench_checked(
                &format!("insert_native_{label}/duckdb_appender"),
                move || (empty_duck(), docs(n)),
                |(conn, rows)| fill_duck_appender(conn, rows),
                |(conn, expected), _| {
                    let mut stmt = conn
                        .prepare("SELECT id, uri, wing, title, ts, body FROM docs ORDER BY id")
                        .unwrap();
                    let got = stmt
                        .query_map([], |r| {
                            Ok((
                                r.get::<_, String>(0)?,
                                r.get::<_, String>(1)?,
                                r.get::<_, String>(2)?,
                                r.get::<_, String>(3)?,
                                r.get::<_, i64>(4)?,
                                r.get::<_, String>(5)?,
                            ))
                        })
                        .unwrap()
                        .collect::<Result<Vec<_>, _>>()
                        .unwrap();
                    let mut want = expected
                        .iter()
                        .map(|d| {
                            (
                                d.id.clone(),
                                d.uri.clone(),
                                d.wing.clone(),
                                d.title.clone(),
                                d.ts,
                                d.body.clone(),
                            )
                        })
                        .collect::<Vec<_>>();
                    want.sort();
                    assert_eq!(got, want, "native ingestion readback");
                    Ok(())
                },
                DropPolicy::OutsideTiming,
            )
            .tag("insert")
            .tag("native")
            .tag("duckdb")
            .parameter("rows", n)
            .work_units("rows", n as u64);
    }

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
            .bench_checked(
                &format!("durable_insert_{label}/lin"),
                move || setup_lin_durable_insert(n),
                move |ins| fill_lin_durable_insert(ins),
                move |ins, handle| {
                    assert_eq!(handle.done.n, n);
                    validate_lin_rows(&ins.db, n);
                    Ok(())
                },
                DropPolicy::OutsideTiming,
            )
            .tag("durable")
            .tag("insert")
            .tag("lin")
            .parameter("rows", n)
            .work_units("rows", n as u64);
        suite
            .bench_checked(
                &format!("durable_insert_{label}/sqlite"),
                move || setup_sqlite_durable("docs", n),
                move |s| fill_sqlite_durable(s),
                move |s, _| validate_sqlite_docs(&s.conn, &docs(n)),
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
                lin::Db::open_with(
                    &s.dir.0,
                    lin::OpenOpts {
                        sync: lin::SyncMode::Full,
                        cold: true,
                    },
                )
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
            move |s| lin::Db::open(&s.dir.0),
            DropPolicy::OutsideTiming,
        )
        .tag("cold")
        .tag("phase")
        .tag("reopen")
        .tag("lin")
        .parameter("rows", COLD_N)
        .work_units("rows", COLD_N as u64);

    // Out-of-core DiskEngine (cold: true) vs file-backed SQLite / DuckDB (10k docs).
    const COLD_QUERY_N: usize = 10_000;
    let lin_cold = Fixture::new(|| seed_lin_cold(COLD_QUERY_N));
    let sql_cold = Fixture::new(|| seed_sqlite_cold(COLD_QUERY_N));
    let duck_cold = Fixture::new(|| seed_duck_cold(COLD_QUERY_N));

    suite
        .bench_fixture("cold_point_get/lin", lin_cold.clone(), |s| {
            black_box(s.point_get.run(&mut s.db).expect("lin cold get").done.n)
        })
        .tag("cold")
        .tag("point_get")
        .tag("lin")
        .parameter("n", COLD_QUERY_N);
    suite
        .bench_fixture("cold_point_get/sqlite", sql_cold.clone(), |s| {
            let mut stmt = s
                .conn
                .prepare_cached("SELECT COUNT(*) FROM docs WHERE id = ?1")
                .expect("sqlite cold prep");
            let n: i64 = stmt
                .query_row(rusqlite::params![s.probe_id], |r| r.get(0))
                .expect("sqlite cold get");
            black_box(n)
        })
        .tag("cold")
        .tag("point_get")
        .tag("sqlite")
        .parameter("n", COLD_QUERY_N);
    suite
        .bench_fixture("cold_point_get/duckdb", duck_cold.clone(), |s| {
            let mut stmt = s
                .conn
                .prepare_cached("SELECT COUNT(*) FROM docs WHERE id = ?")
                .expect("duck cold prep");
            let n: i64 = stmt
                .query_row(duckdb::params![s.probe_id], |r| r.get(0))
                .expect("duck cold get");
            black_box(n)
        })
        .tag("cold")
        .tag("point_get")
        .tag("duckdb")
        .parameter("n", COLD_QUERY_N);

    suite
        .bench_fixture("cold_filter_eq/lin", lin_cold.clone(), |s| {
            black_box(lin_hits(
                &s.filter_eq.run(&mut s.db).expect("lin cold filter"),
            ))
        })
        .tag("cold")
        .tag("filter_eq")
        .tag("lin")
        .parameter("n", COLD_QUERY_N);
    suite
        .bench_fixture("cold_filter_eq/sqlite", sql_cold.clone(), |s| {
            let mut stmt = s
                .conn
                .prepare_cached("SELECT COUNT(*) FROM docs WHERE wing = 'rag'")
                .expect("sqlite cold prep");
            let n: i64 = stmt
                .query_row([], |r| r.get(0))
                .expect("sqlite cold filter");
            black_box(n)
        })
        .tag("cold")
        .tag("filter_eq")
        .tag("sqlite")
        .parameter("n", COLD_QUERY_N);
    suite
        .bench_fixture("cold_filter_eq/duckdb", duck_cold.clone(), |s| {
            let mut stmt = s
                .conn
                .prepare_cached("SELECT COUNT(*) FROM docs WHERE wing = 'rag'")
                .expect("duck cold prep");
            let n: i64 = stmt.query_row([], |r| r.get(0)).expect("duck cold filter");
            black_box(n)
        })
        .tag("cold")
        .tag("filter_eq")
        .tag("duckdb")
        .parameter("n", COLD_QUERY_N);

    suite
        .bench_fixture("cold_agg_count_by/lin", lin_cold.clone(), |s| {
            let res = s.count_by_wing.run(&mut s.db).expect("lin cold count by");
            black_box(res.done.n)
        })
        .tag("cold")
        .tag("agg")
        .tag("lin")
        .parameter("docs", COLD_QUERY_N)
        .work_units("rows", COLD_QUERY_N as u64);
    suite
        .bench_fixture("cold_agg_count_by/sqlite", sql_cold.clone(), |s| {
            let mut stmt = s
                .conn
                .prepare_cached("SELECT wing, COUNT(*) FROM docs GROUP BY wing")
                .expect("sqlite cold prep");
            let mut rows = stmt.query([]).expect("sqlite cold count by");
            let mut count = 0usize;
            while let Some(row) = rows.next().expect("sqlite cold row") {
                let _wing: String = row.get(0).expect("wing");
                let _cnt: i64 = row.get(1).expect("cnt");
                count += 1;
            }
            black_box(count)
        })
        .tag("cold")
        .tag("agg")
        .tag("sqlite")
        .parameter("docs", COLD_QUERY_N)
        .work_units("rows", COLD_QUERY_N as u64);
    suite
        .bench_fixture("cold_agg_count_by/duckdb", duck_cold.clone(), |s| {
            let mut stmt = s
                .conn
                .prepare_cached("SELECT wing, COUNT(*) FROM docs GROUP BY wing")
                .expect("duck cold prep");
            let mut rows = stmt.query([]).expect("duck cold count by");
            let mut count = 0usize;
            while let Some(row) = rows.next().expect("duck cold row") {
                let _wing: String = row.get(0).expect("wing");
                let _cnt: i64 = row.get(1).expect("cnt");
                count += 1;
            }
            black_box(count)
        })
        .tag("cold")
        .tag("agg")
        .tag("duckdb")
        .parameter("docs", COLD_QUERY_N)
        .work_units("rows", COLD_QUERY_N as u64);

    suite
        .bench_fixture("cold_filter_range/lin", lin_cold.clone(), |s| {
            black_box(lin_hits(&s.filter_range.run(&mut s.db).expect("lin cold_filter_range")))
        })
        .tag("cold")
        .tag("filter_range")
        .tag("lin")
        .parameter("n", COLD_QUERY_N);
    suite
        .bench_fixture("cold_filter_range/sqlite", sql_cold.clone(), |s| {
            let mut stmt = s
                .conn
                .prepare_cached("SELECT COUNT(*) FROM docs WHERE wing = 'rag' AND ts > ?1")
                .expect("sqlite cold_filter_range prep");
            let n: i64 = stmt
                .query_row(rusqlite::params![range_cutoff_ms()], |r| r.get(0))
                .expect("sqlite cold_filter_range");
            black_box(n)
        })
        .tag("cold")
        .tag("filter_range")
        .tag("sqlite")
        .parameter("n", COLD_QUERY_N);
    suite
        .bench_fixture("cold_filter_range/duckdb", duck_cold.clone(), |s| {
            let mut stmt = s
                .conn
                .prepare_cached("SELECT COUNT(*) FROM docs WHERE wing = 'rag' AND ts > ?")
                .expect("duckdb cold_filter_range prep");
            let n: i64 = stmt
                .query_row(duckdb::params![range_cutoff_ms()], |r| r.get(0))
                .expect("duckdb cold_filter_range");
            black_box(n)
        })
        .tag("cold")
        .tag("filter_range")
        .tag("duckdb")
        .parameter("n", COLD_QUERY_N);
    suite
        .bench_fixture("cold_text_substr/lin", lin_cold.clone(), |s| {
            black_box(lin_hits(&s.text_substr.run(&mut s.db).expect("lin cold_text_substr")))
        })
        .tag("cold")
        .tag("text_substr")
        .tag("lin")
        .parameter("n", COLD_QUERY_N);
    suite
        .bench_fixture("cold_text_substr/sqlite", sql_cold.clone(), |s| {
            let mut stmt = s
                .conn
                .prepare_cached("SELECT COUNT(*) FROM docs WHERE title LIKE '%wal%'")
                .expect("sqlite cold_text_substr prep");
            let n: i64 = stmt
                .query_row([], |r| r.get(0))
                .expect("sqlite cold_text_substr");
            black_box(n)
        })
        .tag("cold")
        .tag("text_substr")
        .tag("sqlite")
        .parameter("n", COLD_QUERY_N);
    suite
        .bench_fixture("cold_text_substr/duckdb", duck_cold.clone(), |s| {
            let mut stmt = s
                .conn
                .prepare_cached("SELECT COUNT(*) FROM docs WHERE title LIKE '%wal%'")
                .expect("duckdb cold_text_substr prep");
            let n: i64 = stmt
                .query_row([], |r| r.get(0))
                .expect("duckdb cold_text_substr");
            black_box(n)
        })
        .tag("cold")
        .tag("text_substr")
        .tag("duckdb")
        .parameter("n", COLD_QUERY_N);
    suite
        .bench_fixture("cold_range_project/lin", lin_cold.clone(), |s| {
            black_box(s.range_project.run(&mut s.db).expect("lin cold_range_project").done.n)
        })
        .tag("cold")
        .tag("range_project")
        .tag("lin")
        .parameter("n", COLD_QUERY_N);
    suite
        .bench_fixture("cold_range_project/lin_batch", lin_cold.clone(), |s| {
            black_box(
                s.range_project
                    .run_batch(&mut s.db)
                    .expect("lin cold_range_project batch")
                    .n(),
            )
        })
        .tag("cold")
        .tag("range_project")
        .tag("lin")
        .tag("batch")
        .parameter("n", COLD_QUERY_N);
    suite
        .bench_fixture("cold_range_project/sqlite", sql_cold.clone(), |s| {
            let mut stmt = s
                .conn
                .prepare_cached("SELECT id, title FROM docs WHERE ts > ?1")
                .expect("sqlite cold_range_project prep");
            let mut rows = stmt.query(rusqlite::params![range_cutoff_ms()]).expect("sqlite cold_range_project");
            let mut n = 0usize;
            while let Some(row) = rows.next().expect("sqlite cold_range_project row") {
                let id: String = row.get(0).expect("id");
                let title: String = row.get(1).expect("title");
                black_box((id, title));
                n += 1;
            }
            black_box(n)
        })
        .tag("cold")
        .tag("range_project")
        .tag("sqlite")
        .parameter("n", COLD_QUERY_N);
    suite
        .bench_fixture("cold_range_project/duckdb", duck_cold.clone(), |s| {
            let mut stmt = s
                .conn
                .prepare_cached("SELECT id, title FROM docs WHERE ts > ?")
                .expect("duckdb cold_range_project prep");
            let mut rows = stmt.query(duckdb::params![range_cutoff_ms()]).expect("duckdb cold_range_project");
            let mut n = 0usize;
            while let Some(row) = rows.next().expect("duckdb cold_range_project row") {
                let id: String = row.get(0).expect("id");
                let title: String = row.get(1).expect("title");
                black_box((id, title));
                n += 1;
            }
            black_box(n)
        })
        .tag("cold")
        .tag("range_project")
        .tag("duckdb")
        .parameter("n", COLD_QUERY_N);

    let lin_cold_join = Fixture::new(seed_cold_join_lin);
    let sql_cold_join = Fixture::new(seed_cold_join_sqlite);
    let duck_cold_join = Fixture::new(seed_cold_join_duck);
    suite
        .bench_fixture("cold_join_inner/lin", lin_cold_join.clone(), |s| {
            black_box(s.inner.run_batch(&mut s.db).expect("lin cold join_inner").n())
        })
        .tag("cold")
        .tag("join_inner")
        .tag("lin")
        .parameter("users", JOIN_USERS)
        .parameter("orders", JOIN_ORDERS)
        .work_units("rows", JOIN_ORDERS as u64);
    suite
        .bench_fixture("cold_join_inner/sqlite", sql_cold_join.clone(), |s| {
            black_box(sqlite_join_n(&s.conn, JOIN_INNER_SQL))
        })
        .tag("cold")
        .tag("join_inner")
        .tag("sqlite")
        .parameter("users", JOIN_USERS)
        .parameter("orders", JOIN_ORDERS)
        .work_units("rows", JOIN_ORDERS as u64);
    suite
        .bench_fixture("cold_join_inner/duckdb", duck_cold_join.clone(), |s| {
            black_box(duck_join_n(&s.conn, JOIN_INNER_SQL))
        })
        .tag("cold")
        .tag("join_inner")
        .tag("duckdb")
        .parameter("users", JOIN_USERS)
        .parameter("orders", JOIN_ORDERS)
        .work_units("rows", JOIN_ORDERS as u64);
    suite
        .bench_fixture("cold_join_filter/lin", lin_cold_join.clone(), |s| {
            black_box(s.filter.run_batch(&mut s.db).expect("lin cold join_filter").n())
        })
        .tag("cold")
        .tag("join_filter")
        .tag("lin")
        .parameter("users", JOIN_USERS)
        .parameter("orders", JOIN_ORDERS)
        .work_units("rows", JOIN_FILTER_N as u64);
    suite
        .bench_fixture("cold_join_filter/sqlite", sql_cold_join.clone(), |s| {
            black_box(sqlite_join_n(&s.conn, JOIN_FILTER_SQL))
        })
        .tag("cold")
        .tag("join_filter")
        .tag("sqlite")
        .parameter("users", JOIN_USERS)
        .parameter("orders", JOIN_ORDERS)
        .work_units("rows", JOIN_FILTER_N as u64);
    suite
        .bench_fixture("cold_join_filter/duckdb", duck_cold_join.clone(), |s| {
            black_box(duck_join_n(&s.conn, JOIN_FILTER_SQL))
        })
        .tag("cold")
        .tag("join_filter")
        .tag("duckdb")
        .parameter("users", JOIN_USERS)
        .parameter("orders", JOIN_ORDERS)
        .work_units("rows", JOIN_FILTER_N as u64);
    suite
        .bench_fixture("cold_agg_sum/lin", lin_cold_join.clone(), |s| {
            black_box(s.agg_sum.run(&mut s.db).expect("lin cold sum").done.n)
        })
        .tag("cold")
        .tag("agg")
        .tag("lin")
        .parameter("users", JOIN_USERS)
        .parameter("orders", JOIN_ORDERS)
        .work_units("rows", JOIN_ORDERS as u64);
    suite
        .bench_fixture("cold_agg_sum/sqlite", sql_cold_join.clone(), |s| {
            let mut stmt = s
                .conn
                .prepare_cached("SELECT SUM(total) FROM orders")
                .expect("sqlite cold prep");
            let sum: f64 = stmt.query_row([], |r| r.get(0)).expect("sqlite cold sum");
            black_box(sum)
        })
        .tag("cold")
        .tag("agg")
        .tag("sqlite")
        .parameter("users", JOIN_USERS)
        .parameter("orders", JOIN_ORDERS)
        .work_units("rows", JOIN_ORDERS as u64);
    suite
        .bench_fixture("cold_agg_sum/duckdb", duck_cold_join.clone(), |s| {
            let mut stmt = s
                .conn
                .prepare_cached("SELECT SUM(total) FROM orders")
                .expect("duckdb cold prep");
            let sum: f64 = stmt.query_row([], |r| r.get(0)).expect("duckdb cold sum");
            black_box(sum)
        })
        .tag("cold")
        .tag("agg")
        .tag("duckdb")
        .parameter("users", JOIN_USERS)
        .parameter("orders", JOIN_ORDERS)
        .work_units("rows", JOIN_ORDERS as u64);
    suite
        .bench_fixture("cold_agg_sum_by/lin", lin_cold_join, |s| {
            black_box(s.agg_sum_by.run(&mut s.db).expect("lin cold sum by").done.n)
        })
        .tag("cold")
        .tag("agg")
        .tag("lin")
        .parameter("users", JOIN_USERS)
        .parameter("orders", JOIN_ORDERS)
        .work_units("rows", JOIN_ORDERS as u64);
    suite
        .bench_fixture("cold_agg_sum_by/sqlite", sql_cold_join, |s| {
            let mut stmt = s
                .conn
                .prepare_cached("SELECT user_id, SUM(total) FROM orders GROUP BY user_id")
                .expect("sqlite cold prep");
            let mut rows = stmt.query([]).expect("sqlite cold sum by");
            let mut count = 0usize;
            while let Some(row) = rows.next().expect("sqlite cold row") {
                let _user: String = row.get(0).expect("user");
                let _sum: f64 = row.get(1).expect("sum");
                count += 1;
            }
            black_box(count)
        })
        .tag("cold")
        .tag("agg")
        .tag("sqlite")
        .parameter("users", JOIN_USERS)
        .parameter("orders", JOIN_ORDERS)
        .work_units("rows", JOIN_ORDERS as u64);
    suite
        .bench_fixture("cold_agg_sum_by/duckdb", duck_cold_join, |s| {
            let mut stmt = s
                .conn
                .prepare_cached("SELECT user_id, SUM(total) FROM orders GROUP BY user_id")
                .expect("duckdb cold prep");
            let mut rows = stmt.query([]).expect("duckdb cold sum by");
            let mut count = 0usize;
            while let Some(row) = rows.next().expect("duckdb cold row") {
                let _user: String = row.get(0).expect("user");
                let _sum: f64 = row.get(1).expect("sum");
                count += 1;
            }
            black_box(count)
        })
        .tag("cold")
        .tag("agg")
        .tag("duckdb")
        .parameter("users", JOIN_USERS)
        .parameter("orders", JOIN_ORDERS)
        .work_units("rows", JOIN_ORDERS as u64);

    // One-row durable mutations on a 10k-row cold table. Lin keeps `docs` on disk.
    // On Darwin:
    // - Lin (`SyncMode::Full`) flushes with `fcntl(F_BARRIERFSYNC)`.
    // - Bundled upstream SQLite's default `PRAGMA synchronous = FULL` (`fullfsync = OFF`) calls
    //   plain `fsync(2)` without a drive barrier (`cold_*_1row_10k/sqlite`), while
    //   `sqlite_fullfsync` sets `PRAGMA fullfsync = ON` for barrier/full-sync durability.
    // - DuckDB uses its default checkpointed WAL sync.
    {
        suite
            .bench_checked(
                "cold_update_1row_10k/lin",
                || seed_lin_cold(COLD_QUERY_N),
                |s| {
                    assert!(s.db.store.is_disk_backed("docs"), "docs cold before update");
                    s.update_1row.execute(&mut s.db).expect("lin cold update").n
                },
                |s, n| {
                    assert_eq!(*n, 1);
                    assert!(s.db.store.is_disk_backed("docs"), "update promoted docs");
                    assert_eq!(s.db.stats().docs, COLD_QUERY_N);
                    Ok(())
                },
                DropPolicy::OutsideTiming,
            )
            .tag("cold")
            .tag("update")
            .tag("lin")
            .parameter("n", COLD_QUERY_N);
        suite
            .bench_checked(
                "cold_delete_1row_10k/lin",
                || seed_lin_cold(COLD_QUERY_N),
                |s| {
                    assert!(s.db.store.is_disk_backed("docs"), "docs cold before delete");
                    s.delete_1row.execute(&mut s.db).expect("lin cold delete").n
                },
                |s, n| {
                    assert_eq!(*n, 1);
                    assert!(s.db.store.is_disk_backed("docs"), "delete promoted docs");
                    assert_eq!(s.db.stats().docs, COLD_QUERY_N - 1);
                    Ok(())
                },
                DropPolicy::OutsideTiming,
            )
            .tag("cold")
            .tag("delete")
            .tag("lin")
            .parameter("n", COLD_QUERY_N);
        suite
            .bench_checked(
                "cold_update_1row_10k/sqlite",
                || seed_sqlite_cold(COLD_QUERY_N),
                |s| {
                    let mut stmt = s
                        .conn
                        .prepare_cached("UPDATE docs SET title = 'updated title' WHERE id = ?1")
                        .expect("sqlite cold prepare update");
                    stmt.execute(rusqlite::params![s.probe_id])
                        .expect("sqlite cold update")
                },
                |_, n| {
                    assert_eq!(*n, 1);
                    Ok(())
                },
                DropPolicy::OutsideTiming,
            )
            .tag("cold")
            .tag("update")
            .tag("sqlite")
            .parameter("n", COLD_QUERY_N);
        suite
            .bench_checked(
                "cold_delete_1row_10k/sqlite",
                || seed_sqlite_cold(COLD_QUERY_N),
                |s| {
                    let mut stmt = s
                        .conn
                        .prepare_cached("DELETE FROM docs WHERE id = ?1")
                        .expect("sqlite cold prepare delete");
                    stmt.execute(rusqlite::params![s.probe_id])
                        .expect("sqlite cold delete")
                },
                |_, n| {
                    assert_eq!(*n, 1);
                    Ok(())
                },
                DropPolicy::OutsideTiming,
            )
            .tag("cold")
            .tag("delete")
            .tag("sqlite")
            .parameter("n", COLD_QUERY_N);
        suite
            .bench_checked(
                "cold_update_1row_10k/duckdb",
                || seed_duck_cold(COLD_QUERY_N),
                |s| {
                    let mut stmt = s
                        .conn
                        .prepare_cached("UPDATE docs SET title = 'updated title' WHERE id = ?")
                        .expect("duckdb cold prepare update");
                    stmt.execute(duckdb::params![s.probe_id])
                        .expect("duckdb cold update")
                },
                |_, n| {
                    assert_eq!(*n, 1);
                    Ok(())
                },
                DropPolicy::OutsideTiming,
            )
            .tag("cold")
            .tag("update")
            .tag("duckdb")
            .parameter("n", COLD_QUERY_N);
        suite
            .bench_checked(
                "cold_delete_1row_10k/duckdb",
                || seed_duck_cold(COLD_QUERY_N),
                |s| {
                    let mut stmt = s
                        .conn
                        .prepare_cached("DELETE FROM docs WHERE id = ?")
                        .expect("duckdb cold prepare delete");
                    stmt.execute(duckdb::params![s.probe_id])
                        .expect("duckdb cold delete")
                },
                |_, n| {
                    assert_eq!(*n, 1);
                    Ok(())
                },
                DropPolicy::OutsideTiming,
            )
            .tag("cold")
            .tag("delete")
            .tag("duckdb")
            .parameter("n", COLD_QUERY_N);
        suite
            .bench_checked(
                "cold_update_1row_10k/sqlite_fullfsync",
                || seed_sqlite_cold_fullsync(COLD_QUERY_N),
                |s| {
                    let mut stmt = s
                        .conn
                        .prepare_cached("UPDATE docs SET title = 'updated title' WHERE id = ?1")
                        .expect("sqlite cold fullfsync prepare update");
                    stmt.execute(rusqlite::params![s.probe_id])
                        .expect("sqlite cold fullfsync update")
                },
                |_, n| {
                    assert_eq!(*n, 1);
                    Ok(())
                },
                DropPolicy::OutsideTiming,
            )
            .tag("cold")
            .tag("update")
            .tag("sqlite_fullfsync")
            .parameter("n", COLD_QUERY_N);
        suite
            .bench_checked(
                "cold_delete_1row_10k/sqlite_fullfsync",
                || seed_sqlite_cold_fullsync(COLD_QUERY_N),
                |s| {
                    let mut stmt = s
                        .conn
                        .prepare_cached("DELETE FROM docs WHERE id = ?1")
                        .expect("sqlite cold fullfsync prepare delete");
                    stmt.execute(rusqlite::params![s.probe_id])
                        .expect("sqlite cold fullfsync delete")
                },
                |_, n| {
                    assert_eq!(*n, 1);
                    Ok(())
                },
                DropPolicy::OutsideTiming,
            )
            .tag("cold")
            .tag("delete")
            .tag("sqlite_fullfsync")
            .parameter("n", COLD_QUERY_N);
    }

    suite
        .bench_fixture("cold_materialize/lin", lin_cold, |s| {
            black_box(s.materialize.run(&mut s.db).expect("lin cold mat").done.n)
        })
        .tag("cold")
        .tag("materialize")
        .tag("lin")
        .parameter("n", COLD_QUERY_N);
    suite
        .bench_fixture("cold_materialize/sqlite", sql_cold, |s| {
            let mut stmt = s
                .conn
                .prepare_cached("SELECT id, title FROM docs WHERE wing = 'rag'")
                .expect("sqlite cold prep");
            let mut rows = stmt.query([]).expect("sqlite cold mat");
            let mut n = 0usize;
            while let Some(row) = rows.next().expect("sqlite cold row") {
                let id: String = row.get(0).expect("id");
                let title: String = row.get(1).expect("title");
                black_box((id, title));
                n += 1;
            }
            black_box(n)
        })
        .tag("cold")
        .tag("materialize")
        .tag("sqlite")
        .parameter("n", COLD_QUERY_N);
    suite
        .bench_fixture("cold_materialize/duckdb", duck_cold, |s| {
            let mut stmt = s
                .conn
                .prepare_cached("SELECT id, title FROM docs WHERE wing = 'rag'")
                .expect("duck cold prep");
            let mut rows = stmt.query([]).expect("duck cold mat");
            let mut n = 0usize;
            while let Some(row) = rows.next().expect("duck cold row") {
                let id: String = row.get(0).expect("id");
                let title: String = row.get(1).expect("title");
                black_box((id, title));
                n += 1;
            }
            black_box(n)
        })
        .tag("cold")
        .tag("materialize")
        .tag("duckdb")
        .parameter("n", COLD_QUERY_N);

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

    // Prepared single-row writes, sampled read-back validation outside timing.
    // Setup and fixture destruction are excluded by bench_checked's input lifecycle.
    for (label, n) in [
        ("1k", 1_000usize),
        ("10k", 10_000),
        ("100k", 100_000),
        ("1m", 1_000_000),
    ] {
        suite
            .bench_checked(
                &format!("update_1row_{label}/lin"),
                move || setup_lin_write(n),
                write_update_lin,
                |s, affected| {
                    assert_eq!(*affected, 1);
                    assert_eq!(
                        s.db.store.get_by_id("docs", &s.probe_id).unwrap()["wing"].text(),
                        Some("sys")
                    );
                    Ok(())
                },
                DropPolicy::OutsideTiming,
            )
            .tag("audit")
            .tag("write")
            .tag("undo")
            .tag("lin")
            .parameter("rows", n)
            .work_units("stmts", 1);
        suite
            .bench_checked(
                &format!("update_1row_{label}/sqlite"),
                move || setup_sqlite_write(n),
                write_update_sqlite,
                |s, affected| {
                    assert_eq!(*affected, 1);
                    let wing: String = s
                        .conn
                        .query_row("SELECT wing FROM docs WHERE id = ?1", [&s.probe_id], |r| {
                            r.get(0)
                        })
                        .unwrap();
                    assert_eq!(wing, "sys");
                    Ok(())
                },
                DropPolicy::OutsideTiming,
            )
            .tag("audit")
            .tag("write")
            .tag("sqlite")
            .parameter("rows", n)
            .work_units("stmts", 1);
        suite
            .bench_checked(
                &format!("delete_1row_{label}/lin"),
                move || setup_lin_write(n),
                write_delete_lin,
                move |s, affected| {
                    assert_eq!(*affected, 1);
                    assert!(s.db.store.get_by_id("docs", &s.probe_id).is_none());
                    assert_eq!(s.db.store.collection("docs").len(), n - 1);
                    Ok(())
                },
                DropPolicy::OutsideTiming,
            )
            .tag("audit")
            .tag("write")
            .tag("undo")
            .tag("lin")
            .parameter("rows", n)
            .work_units("stmts", 1);
        suite
            .bench_checked(
                &format!("delete_1row_{label}/sqlite"),
                move || setup_sqlite_write(n),
                write_delete_sqlite,
                |s, affected| {
                    assert_eq!(*affected, 1);
                    let count: i64 = s
                        .conn
                        .query_row(
                            "SELECT COUNT(*) FROM docs WHERE id = ?1",
                            [&s.probe_id],
                            |r| r.get(0),
                        )
                        .unwrap();
                    assert_eq!(count, 0);
                    Ok(())
                },
                DropPolicy::OutsideTiming,
            )
            .tag("audit")
            .tag("write")
            .tag("sqlite")
            .parameter("rows", n)
            .work_units("stmts", 1);
    }

    for (label, n) in [("1k", 1_000usize), ("10k", 10_000)] {
        suite
            .bench_checked(
                &format!("tx_commit_1row_{label}/lin"),
                move || setup_lin_write(n),
                tx_update_commit_lin,
                |s, affected| {
                    assert_eq!(*affected, 1);
                    assert_eq!(
                        s.db.store.get_by_id("docs", &s.probe_id).unwrap()["wing"].text(),
                        Some("sys")
                    );
                    Ok(())
                },
                DropPolicy::OutsideTiming,
            )
            .tag("transaction")
            .tag("write")
            .tag("lin")
            .parameter("rows", n)
            .work_units("tx", 1);
        suite
            .bench_checked(
                &format!("tx_commit_1row_{label}/sqlite"),
                move || setup_sqlite_write(n),
                tx_update_commit_sqlite,
                |s, affected| {
                    assert_eq!(*affected, 1);
                    let wing: String = s
                        .conn
                        .query_row("SELECT wing FROM docs WHERE id = ?1", [&s.probe_id], |r| {
                            r.get(0)
                        })
                        .unwrap();
                    assert_eq!(wing, "sys");
                    Ok(())
                },
                DropPolicy::OutsideTiming,
            )
            .tag("transaction")
            .tag("write")
            .tag("sqlite")
            .parameter("rows", n)
            .work_units("tx", 1);

        suite
            .bench_checked(
                &format!("tx_rollback_1row_{label}/lin"),
                move || setup_lin_write(n),
                tx_update_rollback_lin,
                |s, affected| {
                    assert_eq!(*affected, 1);
                    assert_ne!(
                        s.db.store.get_by_id("docs", &s.probe_id).unwrap()["wing"].text(),
                        Some("sys")
                    );
                    Ok(())
                },
                DropPolicy::OutsideTiming,
            )
            .tag("transaction")
            .tag("rollback")
            .tag("lin")
            .parameter("rows", n)
            .work_units("tx", 1);
        suite
            .bench_checked(
                &format!("tx_rollback_1row_{label}/sqlite"),
                move || setup_sqlite_write(n),
                tx_update_rollback_sqlite,
                |s, affected| {
                    assert_eq!(*affected, 1);
                    let wing: String = s
                        .conn
                        .query_row("SELECT wing FROM docs WHERE id = ?1", [&s.probe_id], |r| {
                            r.get(0)
                        })
                        .unwrap();
                    assert_ne!(wing, "sys");
                    Ok(())
                },
                DropPolicy::OutsideTiming,
            )
            .tag("transaction")
            .tag("rollback")
            .tag("sqlite")
            .parameter("rows", n)
            .work_units("tx", 1);
    }

    for groups in [6usize, 10, 14] {
        suite
            .bench_with_input(
                &format!("plan_dnf_{groups}/lin"),
                move || setup_lin_plan(groups),
                |s| {
                    s.db.clear_plan_cache();
                    black_box(s.db.prepare(&s.src).expect("lin prepare dnf"));
                },
                DropPolicy::InsideTiming,
            )
            .tag("audit")
            .tag("planner")
            .tag("dnf")
            .tag("lin")
            .parameter("or_groups", groups);
    }

    let lin_graph = Fixture::new(|| setup_lin_graph(GRAPH_N));
    for depth in [1usize, 2, 3] {
        suite
            .bench_fixture(
                &format!("graph_depth_{depth}/lin"),
                lin_graph.clone(),
                move |s| black_box(s.graph[depth - 1].run(&mut s.db).expect("lin graph").done.n),
            )
            .tag("audit")
            .tag("graph")
            .tag("lin")
            .parameter("rows", GRAPH_N)
            .parameter("depth", depth);
    }

    let lin_snapshot = Fixture::new(move || seed_lin(N));
    suite
        .bench_fixture("reader_snapshot/lin", lin_snapshot, |s| {
            black_box(
                s.db.reader()
                    .run(r#"docs | { id } | take 10"#)
                    .expect("reader run")
                    .done
                    .n,
            )
        })
        .tag("audit")
        .tag("snapshot")
        .tag("lin")
        .parameter("rows", N);

    let args: Vec<String> = std::env::args()
        .skip(1)
        .filter(|a| a != "--bench")
        .collect();
    run_suite(suite, &args)
}

const GRAPH_N: usize = 5_000;

struct LinWrite {
    db: lin::Db,
    probe_id: String,
    update: lin::Prepared,
    delete: lin::Prepared,
}

/// Same shape as `seed_lin`, kept for the write path (CAS hash resolved once).
fn setup_lin_write(n: usize) -> LinWrite {
    let rows = docs(n);
    let mut db = lin::Db::empty();
    db.run("index docs [wing, ts]").expect("lin index");
    let chunk_size = if n >= 1_000_000 {
        10_000
    } else if n >= 100_000 {
        5_000
    } else {
        500
    };
    for chunk in rows.chunks(chunk_size) {
        db.run(&lin_insert_src(chunk)).expect("lin seed");
    }
    validate_lin_fixture_timestamps(&db, &rows);
    let probe_id = rows[PROBE].id.clone();
    let hash = db
        .run(&format!(
            r#"docs | id == "{}" | {{ hash }}"#,
            escape_lin(&probe_id)
        ))
        .expect("lin hash")
        .rows[0]
        .get("hash")
        .and_then(|c| c.text())
        .unwrap_or("")
        .to_string();
    assert!(hash.starts_with("h:"), "unexpected cas hash: {hash}");
    let update = db
        .prepare(&format!(
            r#"update docs[id == "{}"] cas "{}" {{ wing: "sys" }}"#,
            escape_lin(&probe_id),
            escape_lin(&hash)
        ))
        .expect("prepare update");
    let delete = db
        .prepare(&format!(
            r#"delete docs[id == "{}"] cas "{}""#,
            escape_lin(&probe_id),
            escape_lin(&hash)
        ))
        .expect("prepare delete");
    LinWrite {
        db,
        probe_id,
        update,
        delete,
    }
}

fn write_update_lin(s: &mut LinWrite) -> usize {
    black_box(s.update.run(&mut s.db).expect("lin update").done.n)
}

fn write_delete_lin(s: &mut LinWrite) -> usize {
    black_box(s.delete.run(&mut s.db).expect("lin delete").done.n)
}

fn setup_sqlite_write(n: usize) -> SqlWarm {
    let s = seed_sqlite(n);
    s.conn
        .prepare_cached("UPDATE docs SET wing = ?1 WHERE id = ?2")
        .unwrap();
    s.conn
        .prepare_cached("DELETE FROM docs WHERE id = ?1")
        .unwrap();
    s
}

fn write_update_sqlite(s: &mut SqlWarm) -> usize {
    let mut stmt = s
        .conn
        .prepare_cached("UPDATE docs SET wing = ?1 WHERE id = ?2")
        .expect("sqlite prep");
    black_box(
        stmt.execute(rusqlite::params!["sys", s.probe_id])
            .expect("sqlite update"),
    )
}

fn write_delete_sqlite(s: &mut SqlWarm) -> usize {
    let mut stmt = s
        .conn
        .prepare_cached("DELETE FROM docs WHERE id = ?1")
        .expect("sqlite prep");
    black_box(
        stmt.execute(rusqlite::params![s.probe_id])
            .expect("sqlite delete"),
    )
}

fn tx_update_commit_lin(s: &mut LinWrite) -> usize {
    let mut tx = s.db.transaction().expect("lin tx");
    let n = tx.run_prepared(&s.update).expect("lin update").done.n;
    tx.commit().expect("lin commit");
    black_box(n)
}

fn tx_update_commit_sqlite(s: &mut SqlWarm) -> usize {
    let tx = s.conn.unchecked_transaction().expect("sqlite tx");
    let affected = {
        let mut stmt = tx
            .prepare_cached("UPDATE docs SET wing = ?1 WHERE id = ?2")
            .expect("sqlite prep");
        stmt.execute(rusqlite::params!["sys", s.probe_id])
            .expect("sqlite update")
    };
    tx.commit().expect("sqlite commit");
    black_box(affected)
}

fn tx_update_rollback_lin(s: &mut LinWrite) -> usize {
    let mut tx = s.db.transaction().expect("lin tx");
    let n = tx.run_prepared(&s.update).expect("lin update").done.n;
    tx.rollback().expect("lin rollback");
    black_box(n)
}

fn tx_update_rollback_sqlite(s: &mut SqlWarm) -> usize {
    let tx = s.conn.unchecked_transaction().expect("sqlite tx");
    let affected = {
        let mut stmt = tx
            .prepare_cached("UPDATE docs SET wing = ?1 WHERE id = ?2")
            .expect("sqlite prep");
        stmt.execute(rusqlite::params!["sys", s.probe_id])
            .expect("sqlite update")
    };
    tx.rollback().expect("sqlite rollback");
    black_box(affected)
}

struct LinPlan {
    db: lin::Db,
    src: String,
}

/// `wing == "rag" and (… or …) × groups` → `dnf()` cross-product = 2^groups branches.
fn setup_lin_plan(groups: usize) -> LinPlan {
    let src = format!(
        r#"docs | wing == "rag" and {} | count"#,
        vec![r#"(title == "a" or title == "b")"#; groups].join(" and ")
    );
    LinPlan {
        db: lin::Db::empty(),
        src,
    }
}

struct LinGraph {
    db: lin::Db,
    graph: Vec<lin::Prepared>,
}

/// Chain of `GRAPH_N` docs; `graph wikilink depth=D` has no adjacency index to use.
fn setup_lin_graph(n: usize) -> LinGraph {
    let rows = docs(n);
    let mut db = lin::Db::empty();
    for chunk in rows.chunks(500) {
        db.run(&lin_insert_src(chunk)).expect("lin seed");
    }
    let edges: Vec<String> = rows
        .windows(2)
        .map(|w| {
            format!(
                r#"wikilink "{}" -> "{}""#,
                escape_lin(&w[0].id),
                escape_lin(&w[1].id)
            )
        })
        .collect();
    db.run(&format!("append edges [{}]", edges.join(", ")))
        .expect("lin edges");
    let head = escape_lin(&rows[0].id);
    let graph = [1, 2, 3]
        .iter()
        .map(|d| {
            db.prepare(&format!(
                r#"docs | id == "{head}" | graph wikilink depth={d} | take all"#
            ))
            .expect("lin prepare graph")
        })
        .collect();
    LinGraph { db, graph }
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
    let mut json = std::env::var_os("AIRBUG_BENCH_RUNNER").is_some_and(|v| v == "1");
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
            "--max-iterations" => {
                i += 1;
                config.max_iterations = args
                    .get(i)
                    .ok_or_else(|| airbug_bench::error("--max-iterations requires value"))?
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
                    "--list --profile quick|normal|thorough --filter TEXT [--exact|--glob] --exclude GLOB --tag TAG --samples N --max-iterations N --sample-ms N --warmup-ms N --json --output NEW_DIRECTORY"
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
    let runner_mode = std::env::var_os("AIRBUG_BENCH_RUNNER").is_some_and(|v| v == "1");
    let otlp = OtlpEmitter::from_env(output.is_some());
    let selected_ids: Vec<String> = suite
        .list_selected(&selection)
        .into_iter()
        .map(str::to_string)
        .collect();
    let total = selected_ids.len();

    if let Some(p) = output.as_ref() {
        let dir = std::path::Path::new(p);
        if dir.join("run.json").exists() {
            return Err(airbug_bench::error(format!(
                "run.json already exists in {}",
                dir.display()
            )));
        }
        std::fs::create_dir_all(dir.join("logs"))
            .map_err(|e| airbug_bench::error(e.to_string()))?;
        write_progress_atomic(dir, "running", 0, total, "compare")?;
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

    let run = if let (Some(p), false) = (output.as_ref(), runner_mode || selected_ids.is_empty()) {
        let dir = std::path::Path::new(p);
        let mut combined = airbug_bench::Run::new();
        combined.status = airbug_bench::Status::Incomplete;

        for (idx, case_id) in selected_ids.iter().enumerate() {
            write_progress_atomic(
                dir,
                "running",
                idx,
                total,
                &format!("{case_id} ({}/{total})", idx + 1),
            )?;
            append_compare_log(dir, &format!("[{}/{total}] running {case_id}\n", idx + 1));

            let mut single_sel = airbug_bench::Selection::default();
            single_sel.pattern = case_id.clone();
            single_sel.exact = true;
            single_sel.include_ignored = selection.include_ignored;
            single_sel.only_ignored = selection.only_ignored;

            let start_ns = now_unix_nanos();
            let part_res = suite.run_selected(&single_sel);
            let end_ns = now_unix_nanos().max(start_ns.saturating_add(1));

            let part = match part_res {
                Ok(part) => part,
                Err(err) => {
                    let msg = err.to_string();
                    append_compare_log(
                        dir,
                        &format!("[{}/{total}] failed  {case_id}: {msg}\n", idx + 1),
                    );
                    let _ = write_status_final_atomic(dir, "failed", Some(&msg));
                    let _ = write_progress_atomic(dir, "failed", idx, total, case_id);
                    return Err(err);
                }
            };

            let (median_ns, throughput) = summarize_single_case(&part);
            merge_bench_run(&mut combined, part);

            write_atomic(
                dir,
                "run.json",
                serde_json::to_string_pretty(&combined)?.as_bytes(),
            )?;
            write_progress_atomic(
                dir,
                "running",
                idx + 1,
                total,
                &format!("done {case_id} ({}/{total})", idx + 1),
            )?;
            let summary_line = format_case_log(idx + 1, total, case_id, median_ns, throughput.as_ref());
            append_compare_log(dir, &summary_line);
            otlp.emit_case(
                idx as u64 + 1,
                case_id,
                start_ns,
                end_ns,
                median_ns,
                throughput.as_ref().map(|(r, u)| (*r, u.as_str())),
            );
        }

        combined.status = airbug_bench::Status::Complete;
        combined.validate()?;
        write_atomic(
            dir,
            "run.json",
            serde_json::to_string_pretty(&combined)?.as_bytes(),
        )?;
        let html = airbug_bench::report::html_run(&combined)?;
        write_atomic(dir, "report.html", html.as_bytes())?;
        write_progress_atomic(dir, "complete", total, total, "complete")?;
        write_status_final_atomic(dir, "complete", None)?;
        combined
    } else {
        let start_ns = now_unix_nanos();
        let run_result = suite.run_selected(&selection);
        let end_ns = now_unix_nanos().max(start_ns.saturating_add(1));
        if let Some(p) = output.as_ref() {
            let dir = std::path::Path::new(p);
            let final_state = if run_result.is_ok() { "complete" } else { "failed" };
            let err_str = run_result.as_ref().err().map(|e| e.to_string());
            let _ = write_status_final_atomic(dir, final_state, err_str.as_deref());
            let _ = write_progress_atomic(dir, final_state, total, total, "compare");
        }
        let run = run_result?;
        emit_batch_otlp(&otlp, &run, start_ns, end_ns);
        if let Some(p) = output.as_ref() {
            let dir = std::path::Path::new(p);
            write_atomic(
                dir,
                "run.json",
                serde_json::to_string_pretty(&run)?.as_bytes(),
            )?;
            let html = airbug_bench::report::html_run(&run)?;
            write_atomic(dir, "report.html", html.as_bytes())?;
        }
        run
    };

    if json {
        println!("BENCH_RESULT={}", serde_json::to_string(&run)?);
    } else {
        println!("{}", airbug_bench::report::markdown(&run)?);
    }
    Ok(())
}

fn now_unix_nanos() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}

fn write_atomic(dir: &std::path::Path, name: &str, bytes: &[u8]) -> airbug_bench::Result<()> {
    let tmp = dir.join(format!(".{name}.tmp"));
    std::fs::write(&tmp, bytes).map_err(|e| airbug_bench::error(e.to_string()))?;
    std::fs::rename(&tmp, dir.join(name)).map_err(|e| airbug_bench::error(e.to_string()))
}

fn write_progress_atomic(
    dir: &std::path::Path,
    state: &str,
    completed: usize,
    total: usize,
    variant: &str,
) -> airbug_bench::Result<()> {
    let progress = serde_json::json!({
        "state": state,
        "completed": completed,
        "total": total,
        "variant": variant
    });
    write_atomic(
        dir,
        "progress.json",
        serde_json::to_string_pretty(&progress)?.as_bytes(),
    )
}

fn write_status_final_atomic(
    dir: &std::path::Path,
    state: &str,
    error: Option<&str>,
) -> airbug_bench::Result<()> {
    write_atomic(
        dir,
        "status-final.json",
        serde_json::to_string_pretty(&serde_json::json!({
            "state": state,
            "error": error,
        }))?
        .as_bytes(),
    )
}

fn append_compare_log(dir: &std::path::Path, line: &str) {
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("logs").join("compare.log"))
    {
        let _ = f.write_all(line.as_bytes());
    }
}

fn merge_bench_run(dst: &mut airbug_bench::Run, src: airbug_bench::Run) {
    dst.provenance.extend(src.provenance);
    dst.cases.extend(src.cases);
    dst.observations.extend(src.observations);
    dst.worker_allocations.extend(src.worker_allocations);
    dst.worker_timings.extend(src.worker_timings);
    for note in src.notes {
        if !dst.notes.contains(&note) {
            dst.notes.push(note);
        }
    }
}

fn summarize_case_by_id(run: &airbug_bench::Run, case: &airbug_bench::Case) -> (Option<f64>, Option<(f64, String)>) {
    let is_batch_total = case
        .metrics
        .iter()
        .find(|m| m.id == "wall")
        .is_some_and(|m| m.statistic == "batch_total");
    let mut vals: Vec<f64> = run
        .observations
        .iter()
        .filter(|o| {
            o.case == case.id
                && o.metric == "wall"
                && o.availability == airbug_bench::Availability::Available
        })
        .filter_map(|o| {
            let v = o.number().ok().flatten()?;
            let per_op = if is_batch_total {
                v / (o.operations.max(1) as f64)
            } else {
                v
            };
            (per_op.is_finite() && per_op > 0.0).then_some(per_op)
        })
        .collect();
    if vals.is_empty() {
        return (None, None);
    }
    vals.sort_by(f64::total_cmp);
    let mid = vals.len() / 2;
    let med = if vals.len() % 2 == 1 {
        vals[mid]
    } else {
        (vals[mid - 1] + vals[mid]) * 0.5
    };
    let throughput = case.contract.get("work.unit").and_then(|unit| {
        let count = case
            .contract
            .get("work.count")
            .and_then(|s| s.parse::<f64>().ok())?;
        if count > 0.0 && med > 0.0 {
            Some((count * 1_000_000_000.0 / med, format!("{unit}/s")))
        } else {
            None
        }
    });
    (Some(med), throughput)
}

fn summarize_single_case(run: &airbug_bench::Run) -> (Option<f64>, Option<(f64, String)>) {
    let Some(case) = run.cases.first() else {
        return (None, None);
    };
    summarize_case_by_id(run, case)
}

fn format_case_log(
    step: usize,
    total: usize,
    case_id: &str,
    median_ns: Option<f64>,
    throughput: Option<&(f64, String)>,
) -> String {
    let med = match median_ns {
        Some(ns) if ns >= 1_000_000.0 => format!("{:.2} ms", ns / 1_000_000.0),
        Some(ns) if ns >= 1_000.0 => format!("{:.2} us", ns / 1_000.0),
        Some(ns) => format!("{ns:.2} ns"),
        None => "n/a".into(),
    };
    match throughput {
        Some((rate, unit)) => format!("[{step}/{total}] done    {case_id} · median={med} · {rate:.1} {unit}\n"),
        None => format!("[{step}/{total}] done    {case_id} · median={med}\n"),
    }
}

fn emit_batch_otlp(otlp: &OtlpEmitter, run: &airbug_bench::Run, start_ns: u128, end_ns: u128) {
    if !otlp.is_enabled() {
        return;
    }
    let count = run.cases.len().max(1) as u128;
    let span = end_ns.saturating_sub(start_ns).max(count);
    for (idx, case) in run.cases.iter().enumerate() {
        let s_ns = start_ns + (span * idx as u128) / count;
        let e_ns = (start_ns + (span * (idx as u128 + 1)) / count).max(s_ns.saturating_add(1));
        let (median_ns, throughput) = summarize_case_by_id(run, case);
        otlp.emit_case(
            idx as u64 + 1,
            &case.id,
            s_ns,
            e_ns,
            median_ns,
            throughput.as_ref().map(|(r, u)| (*r, u.as_str())),
        );
    }
}

struct OtlpEmitter {
    addrs: Vec<std::net::SocketAddr>,
    host_header: String,
    base_path: String,
    resource_attrs: Vec<serde_json::Value>,
}

impl OtlpEmitter {
    fn from_env(has_output: bool) -> Self {
        let raw_endpoint = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .or_else(|| {
                std::env::var("AIRBUG_HUB")
                    .ok()
                    .filter(|s| !s.trim().is_empty())
            })
            .or_else(|| {
                (has_output && std::env::var_os("AIRBUG_HUB").is_none())
                    .then(|| "http://127.0.0.1:8790".to_string())
            });

        let mut addrs = Vec::new();
        let mut host_header = String::new();
        let mut base_path = String::new();
        if let Some((hh, bp, resolved)) = raw_endpoint.as_deref().and_then(parse_http_endpoint) {
            host_header = hh;
            base_path = bp;
            addrs = resolved;
        }

        let mut attr_map = std::collections::BTreeMap::<String, String>::new();
        attr_map.insert("service.name".into(), "lin-bench".into());
        if let Ok(raw_attrs) = std::env::var("OTEL_RESOURCE_ATTRIBUTES") {
            for pair in raw_attrs.split(',') {
                if let Some((k, v)) = pair.split_once('=') {
                    let k = k.trim();
                    let v = v.trim();
                    if !k.is_empty() && !v.is_empty() {
                        attr_map.insert(k.to_string(), v.to_string());
                    }
                }
            }
        }
        let resource_attrs = attr_map
            .into_iter()
            .map(|(k, v)| serde_json::json!({ "key": k, "value": { "stringValue": v } }))
            .collect();

        Self {
            addrs,
            host_header,
            base_path,
            resource_attrs,
        }
    }

    fn is_enabled(&self) -> bool {
        !self.addrs.is_empty()
    }

    fn emit_case(
        &self,
        seq: u64,
        case_id: &str,
        start_ns: u128,
        end_ns: u128,
        median_ns: Option<f64>,
        throughput: Option<(f64, &str)>,
    ) {
        if self.addrs.is_empty() {
            return;
        }
        let (trace_id, span_id) = trace_ids_for(seq, case_id, end_ns);
        let mut span_attrs = vec![serde_json::json!({
            "key": "bench.case",
            "value": { "stringValue": case_id }
        })];
        if let Some(m) = median_ns {
            span_attrs.push(serde_json::json!({
                "key": "bench.median_ns",
                "value": { "doubleValue": m }
            }));
        }
        let trace_body = serde_json::json!({
            "resourceSpans": [{
                "resource": { "attributes": self.resource_attrs },
                "scopeSpans": [{
                    "scope": { "name": "lin.bench", "version": env!("CARGO_PKG_VERSION") },
                    "spans": [{
                        "traceId": trace_id,
                        "spanId": span_id,
                        "name": case_id,
                        "kind": 1,
                        "startTimeUnixNano": start_ns.to_string(),
                        "endTimeUnixNano": end_ns.to_string(),
                        "status": { "code": 1 },
                        "attributes": span_attrs
                    }]
                }]
            }]
        });
        if let Ok(bytes) = serde_json::to_vec(&trace_body) {
            self.post_json("/v1/traces", &bytes);
        }

        if let Some(m) = median_ns {
            let mut metrics = vec![serde_json::json!({
                "name": "lin.bench.median_ns",
                "unit": "ns",
                "gauge": {
                    "dataPoints": [{
                        "timeUnixNano": end_ns.to_string(),
                        "asDouble": m,
                        "attributes": [{ "key": "bench.case", "value": { "stringValue": case_id } }]
                    }]
                }
            })];
            if let Some((rate, unit)) = throughput {
                metrics.push(serde_json::json!({
                    "name": "lin.bench.throughput",
                    "unit": unit,
                    "gauge": {
                        "dataPoints": [{
                            "timeUnixNano": end_ns.to_string(),
                            "asDouble": rate,
                            "attributes": [{ "key": "bench.case", "value": { "stringValue": case_id } }]
                        }]
                    }
                }));
            }
            let metric_body = serde_json::json!({
                "resourceMetrics": [{
                    "resource": { "attributes": self.resource_attrs },
                    "scopeMetrics": [{
                        "scope": { "name": "lin.bench", "version": env!("CARGO_PKG_VERSION") },
                        "metrics": metrics
                    }]
                }]
            });
            if let Ok(bytes) = serde_json::to_vec(&metric_body) {
                self.post_json("/v1/metrics", &bytes);
            }
        }

        let log_msg = match (median_ns, throughput) {
            (Some(m), Some((rate, unit))) => {
                format!("{case_id}: median {m:.2} ns ({rate:.1} {unit})")
            }
            (Some(m), None) => format!("{case_id}: median {m:.2} ns"),
            (None, _) => format!("{case_id}: completed"),
        };
        let log_body = serde_json::json!({
            "resourceLogs": [{
                "resource": { "attributes": self.resource_attrs },
                "scopeLogs": [{
                    "scope": { "name": "lin.bench", "version": env!("CARGO_PKG_VERSION") },
                    "logRecords": [{
                        "timeUnixNano": end_ns.to_string(),
                        "severityText": "INFO",
                        "traceId": trace_id,
                        "spanId": span_id,
                        "body": { "stringValue": log_msg },
                        "attributes": [{ "key": "bench.case", "value": { "stringValue": case_id } }]
                    }]
                }]
            }]
        });
        if let Ok(bytes) = serde_json::to_vec(&log_body) {
            self.post_json("/v1/logs", &bytes);
        }
    }

    fn post_json(&self, suffix: &str, body: &[u8]) {
        use std::io::{Read, Write};
        let path = format!("{}{}", self.base_path, suffix);
        let Some(&addr) = self.addrs.first() else {
            return;
        };
        let Ok(mut stream) =
            std::net::TcpStream::connect_timeout(&addr, Duration::from_millis(100))
        else {
            return;
        };
        let _ = stream.set_read_timeout(Some(Duration::from_millis(150)));
        let _ = stream.set_write_timeout(Some(Duration::from_millis(150)));
        let head = format!(
            "POST {path} HTTP/1.1\r\nHost: {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            self.host_header,
            body.len()
        );
        if stream.write_all(head.as_bytes()).is_err() || stream.write_all(body).is_err() {
            return;
        }
        let mut sink = [0u8; 256];
        let _ = stream.read(&mut sink);
    }
}

fn parse_http_endpoint(raw: &str) -> Option<(String, String, Vec<std::net::SocketAddr>)> {
    use std::net::ToSocketAddrs;
    let rest = raw.trim().strip_prefix("http://")?;
    let (host_port, base_path) = match rest.find('/') {
        Some(pos) => (&rest[..pos], rest[pos..].trim_end_matches('/').to_string()),
        None => (rest, String::new()),
    };
    if host_port.is_empty() {
        return None;
    }
    let lookup = if host_port.starts_with('[') {
        if host_port.contains("]:") {
            host_port.to_string()
        } else {
            format!("{host_port}:80")
        }
    } else if host_port.contains(':') {
        host_port.to_string()
    } else {
        format!("{host_port}:80")
    };
    let addrs: Vec<std::net::SocketAddr> = lookup.to_socket_addrs().ok()?.collect();
    (!addrs.is_empty()).then_some((host_port.to_string(), base_path, addrs))
}

fn fnv1a_64(mut h: u64, bytes: &[u8]) -> u64 {
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x1000_0000_01b3);
    }
    h
}

fn trace_ids_for(seq: u64, case_id: &str, ns: u128) -> (String, String) {
    let seed = (ns as u64)
        ^ ((std::process::id() as u64).rotate_left(17))
        ^ seq.wrapping_mul(0x9e37_79b9_7f4a_7c15);
    let lo = fnv1a_64(seed ^ 0xcbf2_9ce4_8422_2325, case_id.as_bytes()) | 1;
    let hi = fnv1a_64(lo.rotate_left(29), &(ns as u64).to_le_bytes()) | 1;
    let span = fnv1a_64(hi ^ seq, case_id.as_bytes()) | 1;
    (format!("{hi:016x}{lo:016x}"), format!("{span:016x}"))
}
