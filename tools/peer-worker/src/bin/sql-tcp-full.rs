use postgres::binary_copy::BinaryCopyInWriter;
use postgres::types::Type;
use std::{env, error::Error, time::Instant};
use tiberius::{AuthMethod, Config, IntoSql, TokenRow};
use tokio_util::compat::TokioAsyncWriteCompatExt;

type Result<T> = std::result::Result<T, Box<dyn Error>>;
type Doc = (String, String, String, String, i64, String, String);

fn fixture(n: usize) -> Vec<Doc> {
    (0..n)
        .map(|i| {
            let text = if i % 10 == 0 { "wal note" } else { "plain" };
            (
                format!("d-{i}"),
                format!("bench://{i}"),
                if i % 2 == 0 { "rag" } else { "sys" }.into(),
                format!("doc {i} {text}"),
                123456,
                format!("body {i}"),
                "wiki".into(),
            )
        })
        .collect()
}

fn pg(rows: &[Doc]) -> Result<u128> {
    let mut db = postgres::Client::connect(&env::var("LIN_BENCH_PG_URL")?, postgres::NoTls)?;
    for s in ["fsync", "synchronous_commit", "full_page_writes"] {
        assert_eq!(
            db.query_one(&format!("SHOW {s}"), &[])?.get::<_, String>(0),
            "on"
        );
    }
    db.batch_execute("CREATE TABLE docs(id text PRIMARY KEY,uri text UNIQUE,wing text,title text,ts bigint,body text,layer text); CREATE INDEX docs_wing_ts ON docs(wing,ts)")?;
    let start = Instant::now();
    for chunk in rows.chunks(10000) {
        let mut tx = db.transaction()?;
        let mut writer = BinaryCopyInWriter::new(
            tx.copy_in("COPY docs FROM STDIN BINARY")?,
            &[
                Type::TEXT,
                Type::TEXT,
                Type::TEXT,
                Type::TEXT,
                Type::INT8,
                Type::TEXT,
                Type::TEXT,
            ],
        );
        for (id, uri, wing, title, ts, body, layer) in chunk {
            writer.write(&[id, uri, wing, title, ts, body, layer])?;
        }
        assert_eq!(writer.finish()? as usize, chunk.len());
        tx.commit()?;
    }
    let ns = start.elapsed().as_nanos();
    let mut got: Vec<Doc> = db
        .query("SELECT * FROM docs", &[])?
        .into_iter()
        .map(|r| {
            (
                r.get(0),
                r.get(1),
                r.get(2),
                r.get(3),
                r.get(4),
                r.get(5),
                r.get(6),
            )
        })
        .collect();
    verify(&mut got, rows);
    db.batch_execute("DROP TABLE docs")?;
    Ok(ns)
}

fn verify(got: &mut [Doc], rows: &[Doc]) {
    let mut want = rows.to_vec();
    want.sort_unstable();
    got.sort_unstable();
    assert_eq!(got, want);
}

async fn mssql(rows: &[Doc]) -> Result<u128> {
    let mut cfg = Config::new();
    cfg.host(env::var("LIN_BENCH_MSSQL_HOST")?);
    cfg.port(env::var("LIN_BENCH_MSSQL_PORT")?.parse()?);
    cfg.database(env::var("LIN_BENCH_MSSQL_DATABASE")?);
    cfg.authentication(AuthMethod::sql_server(
        env::var("LIN_BENCH_MSSQL_USER")?,
        env::var("LIN_BENCH_MSSQL_PASSWORD")?,
    ));
    cfg.trust_cert();
    let tcp = tokio::net::TcpStream::connect(cfg.get_addr()).await?;
    tcp.set_nodelay(true)?;
    let mut db = tiberius::Client::connect(cfg, tcp.compat_write()).await?;
    db.simple_query("CREATE TABLE dbo.docs(id nvarchar(64) PRIMARY KEY,uri nvarchar(128) UNIQUE,wing nvarchar(16),title nvarchar(128),ts bigint,body nvarchar(128),layer nvarchar(16)); CREATE INDEX docs_wing_ts ON dbo.docs(wing,ts)").await?.into_results().await?;
    let start = Instant::now();
    for chunk in rows.chunks(10000) {
        db.simple_query("BEGIN TRANSACTION")
            .await?
            .into_results()
            .await?;
        let mut bulk = db.bulk_insert("dbo.docs").await?;
        for (id, uri, wing, title, ts, body, layer) in chunk {
            let mut r = TokenRow::new();
            for s in [id, uri, wing, title] {
                r.push(s.as_str().into_sql());
            }
            r.push((*ts).into_sql());
            for s in [body, layer] {
                r.push(s.as_str().into_sql());
            }
            bulk.send(r).await?;
        }
        bulk.finalize().await?;
        db.simple_query("COMMIT TRANSACTION")
            .await?
            .into_results()
            .await?;
    }
    let ns = start.elapsed().as_nanos();
    let mut got: Vec<Doc> = db
        .simple_query("SELECT * FROM dbo.docs")
        .await?
        .into_first_result()
        .await?
        .into_iter()
        .map(|r| {
            let s = |i: usize| r.get::<&str, _>(i).unwrap().to_owned();
            (s(0), s(1), s(2), s(3), r.get(4).unwrap(), s(5), s(6))
        })
        .collect();
    verify(&mut got, rows);
    db.simple_query("DROP TABLE dbo.docs")
        .await?
        .into_results()
        .await?;
    Ok(ns)
}

fn mysql_insert(rows: &[Doc]) -> Result<u128> {
    use mysql::{Conn, Opts, Params, TxOpts, prelude::Queryable};
    let mut db = Conn::new(Opts::from_url(&env::var("LIN_BENCH_MYSQL_URL")?)?)?;
    assert_eq!(
        db.query_first::<u64, _>("SELECT @@innodb_flush_log_at_trx_commit")?,
        Some(1)
    );
    db.query_drop("CREATE TABLE docs(id varchar(64) PRIMARY KEY, uri varchar(128) UNIQUE,wing varchar(16),title varchar(128),ts bigint,body varchar(128),layer varchar(16),INDEX docs_wing_ts(wing,ts)) ENGINE=InnoDB")?;
    let start = Instant::now();
    for chunk in rows.chunks(10000) {
        let mut tx = db.start_transaction(TxOpts::default())?;
        // Keep the number of bound parameters below the server limit.
        for group in chunk.chunks(1000) {
            let sql = format!(
                "INSERT INTO docs VALUES {}",
                vec!["(?,?,?,?,?,?,?)"; group.len()].join(",")
            );
            let mut values = Vec::with_capacity(group.len() * 7);
            for (id, uri, wing, title, ts, body, layer) in group {
                values.extend([
                    id.as_str().into(),
                    uri.as_str().into(),
                    wing.as_str().into(),
                    title.as_str().into(),
                    (*ts).into(),
                    body.as_str().into(),
                    layer.as_str().into(),
                ]);
            }
            tx.exec_drop(sql, Params::Positional(values))?;
        }
        tx.commit()?;
    }
    let ns = start.elapsed().as_nanos();
    let mut got: Vec<Doc> = db.query("SELECT * FROM docs")?;
    verify(&mut got, rows);
    db.query_drop("DROP TABLE docs")?;
    Ok(ns)
}

fn mongo_insert(rows: &[Doc]) -> Result<u128> {
    use mongodb::{
        IndexModel,
        bson::{Document, doc},
        options::{Acknowledgment, WriteConcern},
        sync::Client,
    };
    let client = Client::with_uri_str(env::var("LIN_BENCH_MONGO_URL")?)?;
    let db = client.database("lin_native_bench");
    let coll = db.collection::<Document>("docs");
    db.create_collection("docs").run()?;
    coll.create_index(
        IndexModel::builder()
            .keys(doc! {"uri":1})
            .options(
                mongodb::options::IndexOptions::builder()
                    .unique(true)
                    .build(),
            )
            .build(),
    )
    .run()?;
    coll.create_index(IndexModel::builder().keys(doc! {"wing":1,"ts":1}).build())
        .run()?;
    let mut session = client.start_session().run()?;
    let wc = WriteConcern::builder()
        .w(Acknowledgment::Majority)
        .journal(true)
        .build();
    let start = Instant::now();
    for chunk in rows.chunks(10000) {
        session
            .start_transaction()
            .write_concern(wc.clone())
            .run()?;
        let docs=chunk.iter().map(|(id,uri,wing,title,ts,body,layer)|doc!{"_id":id,"uri":uri,"wing":wing,"title":title,"ts":ts,"body":body,"layer":layer});
        coll.insert_many(docs).session(&mut session).run()?;
        session.commit_transaction().run()?;
    }
    let ns = start.elapsed().as_nanos();
    let mut got = Vec::with_capacity(rows.len());
    for item in coll.find(doc! {}).run()? {
        let d = item?;
        got.push((
            d.get_str("_id")?.into(),
            d.get_str("uri")?.into(),
            d.get_str("wing")?.into(),
            d.get_str("title")?.into(),
            d.get_i64("ts")?,
            d.get_str("body")?.into(),
            d.get_str("layer")?.into(),
        ));
    }
    verify(&mut got, rows);
    coll.drop().run()?;
    Ok(ns)
}

fn main() -> Result<()> {
    let args = env::args().collect::<Vec<_>>();
    let rows = fixture(args[2].parse()?);
    let ns = match args[1].as_str() {
        "postgres" => pg(&rows)?,
        "mysql" => mysql_insert(&rows)?,
        "mongo" => mongo_insert(&rows)?,
        "mssql" => tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?
            .block_on(mssql(&rows))?,
        _ => return Err("unsupported engine".into()),
    };
    println!("{ns}");
    Ok(())
}
