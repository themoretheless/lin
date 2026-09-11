//! Comparative hot-path benches: Lin vs SQLite vs DuckDB (+ HashMap point-get baseline).
//!
//! Warm reads use a shared in-memory fixture (setup excluded from timing).
//! Bulk inserts: schema/index in setup; only row writes are timed.
//!
//! Fairness notes (also in README):
//! - Same N, same columns (id/uri/wing/title/ts/body), in-memory only.
//! - Point get / equality / range / LIKE-style substring are comparable.
//! - Lin `hop`, hybrid `search`, and CAS are not claimed here.
//! - SQLite/DuckDB `LIKE '%wal%'` ≈ Lin `title ~ "wal"` (substring), not `has` / FTS.

use std::collections::HashMap;
use std::hint::black_box;

use rbench::{Config, DropPolicy, Fixture, Suite};

const N: usize = 10_000;
const INSERT_1K: usize = 1_000;
const INSERT_10K: usize = 10_000;
/// Probe row index inside the seeded set (wing=rag, title contains "wal").
const PROBE: usize = 20;

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

struct LinWarm {
    db: lin::Db,
    probe_id: String,
}

fn seed_lin(n: usize) -> LinWarm {
    let rows = docs(n);
    let mut db = lin::Db::empty();
    db.run("index docs [wing, ts]").expect("lin index");
    for chunk in rows.chunks(500) {
        db.run(&lin_insert_src(chunk)).expect("lin seed");
    }
    LinWarm {
        probe_id: rows[PROBE].id.clone(),
        db,
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

fn empty_lin() -> lin::Db {
    let mut db = lin::Db::empty();
    db.run("index docs [wing, ts]").expect("lin index");
    db
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

fn fill_lin(db: &mut lin::Db, n: usize) {
    let rows = docs(n);
    for chunk in rows.chunks(500) {
        db.run(&lin_insert_src(chunk)).expect("lin insert");
    }
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

fn main() -> rbench::Result<()> {
    eprintln!(
        "lin compare: N={N} warm reads (fixture), bulk insert (schema outside timing)\n\
         engines: Lin | SQLite(rusqlite bundled) | DuckDB(bundled) | HashMap(point get only)\n\
         substring: Lin `title ~ \"wal\"` vs SQL LIKE '%wal%'"
    );

    let mut suite = Suite::new("compare");
    suite.config(Config::profile("quick")?);

    let lin = Fixture::new(|| seed_lin(N));
    let sql = Fixture::new(|| seed_sqlite(N));
    let duck = Fixture::new(|| seed_duck(N));
    let map = Fixture::new(|| seed_map(N));

    suite
        .bench_fixture("point_get/lin", lin.clone(), |s| {
            let q = format!(r#"docs | id == "{}" | {{ id, title }}"#, s.probe_id);
            black_box(s.db.run(&q).expect("lin get").done.n)
        })
        .tag("point_get")
        .tag("lin")
        .parameter("n", N);
    suite
        .bench_fixture("point_get/sqlite", sql.clone(), |s| {
            let n: i64 = s
                .conn
                .query_row(
                    "SELECT COUNT(*) FROM docs WHERE id = ?1",
                    rusqlite::params![s.probe_id],
                    |r| r.get(0),
                )
                .expect("sqlite get");
            black_box(n)
        })
        .tag("point_get")
        .tag("sqlite")
        .parameter("n", N);
    suite
        .bench_fixture("point_get/duckdb", duck.clone(), |s| {
            let n: i64 = s
                .conn
                .query_row(
                    "SELECT COUNT(*) FROM docs WHERE id = ?",
                    duckdb::params![s.probe_id],
                    |r| r.get(0),
                )
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

    suite
        .bench_fixture("filter_eq/lin", lin.clone(), |s| {
            black_box(
                s.db
                    .run(r#"docs | wing == "rag" | take all"#)
                    .expect("lin filter")
                    .done
                    .n,
            )
        })
        .tag("filter_eq")
        .tag("lin")
        .parameter("n", N);
    suite
        .bench_fixture("filter_eq/sqlite", sql.clone(), |s| {
            let n: i64 = s
                .conn
                .query_row(
                    "SELECT COUNT(*) FROM docs WHERE wing = 'rag'",
                    [],
                    |r| r.get(0),
                )
                .expect("sqlite filter");
            black_box(n)
        })
        .tag("filter_eq")
        .tag("sqlite")
        .parameter("n", N);
    suite
        .bench_fixture("filter_eq/duckdb", duck.clone(), |s| {
            let n: i64 = s
                .conn
                .query_row(
                    "SELECT COUNT(*) FROM docs WHERE wing = 'rag'",
                    [],
                    |r| r.get(0),
                )
                .expect("duck filter");
            black_box(n)
        })
        .tag("filter_eq")
        .tag("duckdb")
        .parameter("n", N);

    suite
        .bench_fixture("filter_range/lin", lin.clone(), |s| {
            black_box(
                s.db
                    .run(r#"docs | wing == "rag" and ts > ago 7d | take all"#)
                    .expect("lin range")
                    .done
                    .n,
            )
        })
        .tag("filter_range")
        .tag("lin")
        .parameter("n", N);
    suite
        .bench_fixture("filter_range/sqlite", sql.clone(), |s| {
            let cutoff = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as i64
                - 7 * 86_400_000;
            let n: i64 = s
                .conn
                .query_row(
                    "SELECT COUNT(*) FROM docs WHERE wing = 'rag' AND ts > ?1",
                    rusqlite::params![cutoff],
                    |r| r.get(0),
                )
                .expect("sqlite range");
            black_box(n)
        })
        .tag("filter_range")
        .tag("sqlite")
        .parameter("n", N);
    suite
        .bench_fixture("filter_range/duckdb", duck.clone(), |s| {
            let cutoff = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as i64
                - 7 * 86_400_000;
            let n: i64 = s
                .conn
                .query_row(
                    "SELECT COUNT(*) FROM docs WHERE wing = 'rag' AND ts > ?",
                    duckdb::params![cutoff],
                    |r| r.get(0),
                )
                .expect("duck range");
            black_box(n)
        })
        .tag("filter_range")
        .tag("duckdb")
        .parameter("n", N);

    suite
        .bench_fixture("text_substr/lin", lin, |s| {
            black_box(
                s.db
                    .run(r#"docs | title ~ "wal" | take all"#)
                    .expect("lin ~")
                    .done
                    .n,
            )
        })
        .tag("text_substr")
        .tag("lin")
        .parameter("n", N);
    suite
        .bench_fixture("text_substr/sqlite", sql, |s| {
            let n: i64 = s
                .conn
                .query_row(
                    "SELECT COUNT(*) FROM docs WHERE title LIKE '%wal%'",
                    [],
                    |r| r.get(0),
                )
                .expect("sqlite like");
            black_box(n)
        })
        .tag("text_substr")
        .tag("sqlite")
        .parameter("n", N);
    suite
        .bench_fixture("text_substr/duckdb", duck, |s| {
            let n: i64 = s
                .conn
                .query_row(
                    "SELECT COUNT(*) FROM docs WHERE title LIKE '%wal%'",
                    [],
                    |r| r.get(0),
                )
                .expect("duck like");
            black_box(n)
        })
        .tag("text_substr")
        .tag("duckdb")
        .parameter("n", N);

    macro_rules! insert_bulk {
        ($label:expr, $n:expr) => {{
            let n = $n;
            suite
                .bench_with_input(
                    &format!("insert_bulk_{}/lin", $label),
                    empty_lin,
                    move |db| fill_lin(db, n),
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
        }};
    }
    insert_bulk!("1k", INSERT_1K);
    insert_bulk!("10k", INSERT_10K);

    // `cargo bench` appends `--bench`; rbench Suite::main rejects unknown flags.
    let args: Vec<String> = std::env::args()
        .skip(1)
        .filter(|a| a != "--bench")
        .collect();
    run_suite(suite, &args)
}

/// Minimal CLI compatible with `Suite::main`, ignoring cargo's injected `--bench`.
fn run_suite(mut suite: Suite<'_>, args: &[String]) -> rbench::Result<()> {
    use std::time::Duration;
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
