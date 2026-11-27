use std::{env, error::Error, time::Instant};
type Doc = (String, String, String, String, i64, String);
type Result<T> = std::result::Result<T, Box<dyn Error>>;
const DDL: &str = "CREATE TABLE docs(id VARCHAR PRIMARY KEY,uri VARCHAR UNIQUE,wing VARCHAR,title VARCHAR,ts BIGINT,body VARCHAR); CREATE INDEX docs_wing_ts ON docs(wing,ts)";
const READ: &str = "SELECT id,uri,wing,title,ts,body FROM docs ORDER BY id";
fn main() -> Result<()> {
    let args: Vec<_> = env::args().collect();
    let n = args[2].parse::<usize>()?;
    let rows: Vec<Doc> = (0..n)
        .map(|i| {
            (
                format!("d-{i}"),
                format!("bench://{i}"),
                if i % 2 == 0 { "rag" } else { "sys" }.into(),
                format!("doc {i} {}", if i % 10 == 0 { "wal note" } else { "plain" }),
                123456,
                format!("body {i}"),
            )
        })
        .collect();
    let mut expected = rows.clone();
    expected.sort_unstable();
    let mut times = Vec::new();
    for sample in 0..args[3].parse()? {
        let start;
        let elapsed;
        match args[1].as_str() {
            "lin-memory" | "lin-wal" => {
                use lin::{Cell, Db, RecordBatch};
                let path = std::path::Path::new(&args[4]).join(format!("db-{sample}"));
                let durable = args[1] == "lin-wal";
                let mut db = if durable {
                    Db::open_with(
                        &path,
                        lin::OpenOpts {
                            sync: lin::SyncMode::Full,
                            cold: false,
                        },
                    )?
                } else {
                    Db::empty()
                };
                db.run("index docs [wing, ts]")?;
                start = Instant::now();
                let names = ["id", "uri", "wing", "title", "ts", "body", "layer"].map(String::from);
                let mut batch = RecordBatch::with_capacity(names.to_vec(), n);
                for (id, uri, wing, title, ts, body) in &rows {
                    batch.push_cells([
                        Cell::from(id.as_str()),
                        Cell::from(uri.as_str()),
                        Cell::from(wing.as_str()),
                        Cell::from(title.as_str()),
                        Cell::Time(*ts),
                        Cell::from(body.as_str()),
                        Cell::from("wiki"),
                    ]);
                }
                let done = db.insert_batch("docs", &batch)?;
                elapsed = start.elapsed();
                assert_eq!(done.n, n);
                if durable {
                    if let Some(target) = std::env::var_os("LIN_BENCH_EXPORT_WAL") {
                        std::fs::copy(lin::persist::log_path(&path), target)?;
                    }
                    let recovery = path.join("recovery");
                    std::fs::create_dir(&recovery)?;
                    for file in [
                        lin::persist::log_path(&path),
                        lin::persist::head_path(&path),
                        lin::persist::snapshot_path(&path),
                    ] {
                        if file.exists() {
                            std::fs::copy(&file, recovery.join(file.file_name().unwrap()))?;
                        }
                    }
                    db = Db::open(&recovery)?;
                }
                let actual = db.store.collection("docs");
                assert_eq!(actual.len(), rows.len());
                for (row, (id, uri, wing, title, ts, body)) in actual.iter().zip(&rows) {
                    for (name, value) in [
                        ("id", id),
                        ("uri", uri),
                        ("wing", wing),
                        ("title", title),
                        ("body", body),
                    ] {
                        assert_eq!(row.get(name).and_then(Cell::text), Some(value.as_str()));
                    }
                    assert_eq!(row.get("ts"), Some(&Cell::Time(*ts)));
                    assert!(row.get("embedding").and_then(Cell::as_vec).is_some());
                }
            }
            "duckdb" => {
                let mut db = duckdb::Connection::open_in_memory()?;
                db.execute_batch(DDL)?;
                start = Instant::now();
                let tx = db.transaction()?;
                {
                    let mut app = tx.appender("docs")?;
                    for (id, uri, wing, title, ts, body) in &rows {
                        app.append_row(duckdb::params![id, uri, wing, title, ts, body])?;
                    }
                    app.flush()?;
                }
                tx.commit()?;
                elapsed = start.elapsed();
                let got = db
                    .prepare(READ)?
                    .query_map([], |r| {
                        Ok((
                            r.get(0)?,
                            r.get(1)?,
                            r.get(2)?,
                            r.get(3)?,
                            r.get(4)?,
                            r.get(5)?,
                        ))
                    })?
                    .collect::<std::result::Result<Vec<Doc>, _>>()?;
                assert_eq!(got, expected);
            }
            "sqlite" | "sqlite-memory" => {
                let path = std::path::Path::new(&args[4]).join(format!("db-{sample}"));
                let durable = args[1] == "sqlite";
                let mut db = if durable {
                    let db = rusqlite::Connection::open(&path)?;
                    db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;")?;
                    assert_eq!(
                        db.query_row("PRAGMA journal_mode", [], |r| r.get::<_, String>(0))?,
                        "wal"
                    );
                    assert_eq!(
                        db.query_row("PRAGMA synchronous", [], |r| r.get::<_, i64>(0))?,
                        2
                    );
                    db
                } else {
                    rusqlite::Connection::open_in_memory()?
                };
                db.execute_batch(DDL)?;
                db.prepare_cached("INSERT INTO docs VALUES(?,?,?,?,?,?)")?;
                start = Instant::now();
                let tx = db.transaction()?;
                {
                    let mut ins = tx.prepare_cached("INSERT INTO docs VALUES(?,?,?,?,?,?)")?;
                    for (id, uri, wing, title, ts, body) in &rows {
                        ins.execute(rusqlite::params![id, uri, wing, title, ts, body])?;
                    }
                }
                tx.commit()?;
                elapsed = start.elapsed();
                if durable {
                    drop(db);
                    db = rusqlite::Connection::open(path)?;
                }
                let got = db
                    .prepare(READ)?
                    .query_map([], |r| {
                        Ok((
                            r.get(0)?,
                            r.get(1)?,
                            r.get(2)?,
                            r.get(3)?,
                            r.get(4)?,
                            r.get(5)?,
                        ))
                    })?
                    .collect::<std::result::Result<Vec<Doc>, _>>()?;
                assert_eq!(got, expected);
            }
            _ => return Err("unknown engine".into()),
        }
        times.push(elapsed.as_nanos() as u64);
    }
    println!("{}", serde_json::to_string(&times)?);
    Ok(())
}
