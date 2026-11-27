use lin::{Cell, ClientOpts, Db, LinServer, OpenOpts, RecordBatch, ServerClient, ServerConfig, SyncMode};
use std::{path::PathBuf, time::Instant};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let dir = PathBuf::from(args.next().ok_or("missing directory")?);
    let n: usize = args.next().ok_or("missing size")?.parse()?;
    let db = Db::open_with(&dir, OpenOpts { sync: SyncMode::Full, cold: false })?;
    let server = LinServer::start(db, ServerConfig {
        bind_addr: "127.0.0.1:0".into(), ..Default::default()
    })?;
    let client = ServerClient::connect(server.local_addr().to_string(), ClientOpts::default())?;
    client.execute("index docs [wing, ts]")?;
    let names = ["id", "uri", "wing", "title", "ts", "body", "layer"].map(String::from);
    let mut cols = vec![Vec::with_capacity(n); names.len()];
    for i in 0..n {
        let text = if i % 10 == 0 { "wal note" } else { "plain" };
        let cells = [Cell::text_arc(format!("d-{i}")), Cell::text_arc(format!("bench://{i}")),
            Cell::text_arc(if i % 2 == 0 { "rag" } else { "sys" }), Cell::text_arc(format!("doc {i} {text}")),
            Cell::Time(123456), Cell::text_arc(format!("body {i}")), Cell::text_arc("wiki")];
        for (col, cell) in cols.iter_mut().zip(cells) { col.push(cell); }
    }
    let batch = RecordBatch::from_columns(names.to_vec(), cols);
    let start = Instant::now();
    let receipt = client.insert_batch("docs", &batch)?;
    let elapsed = start.elapsed().as_nanos();
    assert_eq!(receipt.affected, n);
    drop(client);
    drop(server.shutdown()?);
    let db = Db::open(&dir)?;
    let rows = db.store.collection("docs");
    assert_eq!(rows.len(), n);
    for (stored, source) in rows.iter().zip(batch.to_rows()) {
        for key in &names { assert_eq!(stored.get(key), source.get(key)); }
    }
    println!("{elapsed}");
    Ok(())
}
