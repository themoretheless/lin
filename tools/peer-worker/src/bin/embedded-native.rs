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
            "sqlite" => {
                let path = std::path::Path::new(&args[4]).join(format!("db-{sample}"));
                let mut db = rusqlite::Connection::open(&path)?;
                db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;")?;
                assert_eq!(
                    db.query_row("PRAGMA journal_mode", [], |r| r.get::<_, String>(0))?,
                    "wal"
                );
                assert_eq!(
                    db.query_row("PRAGMA synchronous", [], |r| r.get::<_, i64>(0))?,
                    2
                );
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
                drop(db);
                let db = rusqlite::Connection::open(path)?;
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
