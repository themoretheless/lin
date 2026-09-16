//! Experimental TCP WAL shipping (no TLS).
//!
//! Protocol (one request / response per connection):
//! - client → server: `u64` LE `since_gen`
//! - server → client: `u64` LE `byte_len` + raw WAL frames (`export_wal_since`)

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream, ToSocketAddrs};
use std::path::Path;
use std::time::Duration;

use crate::error::Error;
use crate::exec::Db;

/// Cap on a single pull payload (bytes).
pub const MAX_PULL_BYTES: u64 = 256 * 1024 * 1024;

fn io_err(e: impl std::fmt::Display) -> Error {
    Error::runtime(format!("wal ship: {e}"))
}

/// Pull WAL frames with `gen > since` from a `wal-serve` endpoint.
pub fn pull(addr: impl ToSocketAddrs, since: u64) -> Result<Vec<u8>, Error> {
    let mut stream = TcpStream::connect(addr).map_err(io_err)?;
    stream
        .set_read_timeout(Some(Duration::from_secs(30)))
        .map_err(io_err)?;
    stream
        .set_write_timeout(Some(Duration::from_secs(30)))
        .map_err(io_err)?;
    stream.write_all(&since.to_le_bytes()).map_err(io_err)?;
    stream.flush().map_err(io_err)?;
    let mut len_buf = [0u8; 8];
    stream.read_exact(&mut len_buf).map_err(io_err)?;
    let len = u64::from_le_bytes(len_buf);
    if len > MAX_PULL_BYTES {
        return Err(Error::runtime(format!(
            "wal ship: pull payload {len} exceeds max {MAX_PULL_BYTES}"
        )));
    }
    let mut frames = vec![0u8; len as usize];
    if len > 0 {
        stream.read_exact(&mut frames).map_err(io_err)?;
    }
    Ok(frames)
}

/// Serve WAL pulls from an already-open durable primary [`Db`].
///
/// Blocks forever accepting connections. Holds the writer lock for `db`'s data
/// dir (exclusive with another writer process).
pub fn serve_blocking(db: &Db, listener: TcpListener) -> Result<(), Error> {
    if !db.is_durable() {
        return Err(Error::runtime(
            "wal ship: serve requires durable primary (--data)",
        ));
    }
    listener.set_nonblocking(false).map_err(io_err)?;
    loop {
        let (stream, _) = listener.accept().map_err(io_err)?;
        if let Err(e) = handle_conn(db, stream) {
            // One bad client must not kill the serve loop.
            eprintln!("wal ship: client error: {e}");
        }
    }
}

/// Accept and handle a single pull connection (tests / controlled one-shot).
pub fn serve_one(db: &Db, listener: &TcpListener) -> Result<(), Error> {
    if !db.is_durable() {
        return Err(Error::runtime(
            "wal ship: serve requires durable primary (--data)",
        ));
    }
    let (stream, _) = listener.accept().map_err(io_err)?;
    handle_conn(db, stream)
}

/// Open `dir` as durable primary and serve on `listener` (blocks forever).
pub fn serve_dir(dir: impl AsRef<Path>, listener: TcpListener) -> Result<(), Error> {
    let db = Db::open(dir.as_ref())?;
    serve_blocking(&db, listener)
}

/// Bind `addr` (e.g. `127.0.0.1:9876`), open `dir`, serve forever.
pub fn serve_blocking_addr(dir: impl AsRef<Path>, addr: impl ToSocketAddrs) -> Result<(), Error> {
    let listener = TcpListener::bind(addr).map_err(io_err)?;
    serve_dir(dir, listener)
}

fn handle_conn(db: &Db, mut stream: TcpStream) -> Result<(), Error> {
    stream
        .set_read_timeout(Some(Duration::from_secs(30)))
        .map_err(io_err)?;
    stream
        .set_write_timeout(Some(Duration::from_secs(30)))
        .map_err(io_err)?;
    let mut since_buf = [0u8; 8];
    stream.read_exact(&mut since_buf).map_err(io_err)?;
    let since = u64::from_le_bytes(since_buf);
    let frames = db.export_wal_since(since)?;
    if frames.len() as u64 > MAX_PULL_BYTES {
        return Err(Error::runtime(format!(
            "wal ship: export {} exceeds max {MAX_PULL_BYTES}",
            frames.len()
        )));
    }
    stream
        .write_all(&(frames.len() as u64).to_le_bytes())
        .map_err(io_err)?;
    if !frames.is_empty() {
        stream.write_all(&frames).map_err(io_err)?;
    }
    stream.flush().map_err(io_err)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::SocketAddr;
    use std::sync::{Arc, Barrier};
    use std::thread;
    use std::time::Duration;

    #[test]
    fn pull_roundtrip_ephemeral() {
        let dir = std::env::temp_dir().join(format!(
            "lin-ship-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let mut db = Db::open(&dir).unwrap();
        db.run(r#"insert docs { uri: "raw://ship", title: "Ship", layer: "wiki" }"#)
            .unwrap();
        // Keep open: close/checkpoint would compact the log to empty.

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr: SocketAddr = listener.local_addr().unwrap();
        let ready = Arc::new(Barrier::new(2));
        let ready2 = Arc::clone(&ready);
        let handle = thread::spawn(move || {
            ready2.wait();
            serve_one(&db, &listener).unwrap();
            let _ = db.close();
        });

        ready.wait();
        thread::sleep(Duration::from_millis(20));
        let frames = pull(addr, 0).unwrap();
        assert!(!frames.is_empty());

        let mut mem = Db::empty();
        let n = mem.apply_wal(&frames).unwrap();
        assert_eq!(n, 1);
        assert_eq!(mem.run(r#"docs | uri == "raw://ship""#).unwrap().done.n, 1);

        handle.join().unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }
}
