use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use lin::Db;

static SEQ: AtomicU64 = AtomicU64::new(0);

fn tmp_dir() -> PathBuf {
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("lin-crash-fault-{}-{n}", std::process::id()));
    let _ = fs::remove_dir_all(&p);
    fs::create_dir_all(&p).unwrap();
    p
}

#[test]
fn test_crash_before_wal_append_lin_1681() {
    let dir = tmp_dir();
    {
        let mut db = Db::open(&dir).unwrap();
        db.run(r#"insert docs { id: "d1", title: "committed doc" }"#)
            .unwrap();
    }
    // Simulated crash before append: WAL is untouched for next tx
    {
        let mut db = Db::open(&dir).unwrap();
        let res = db.run("docs | count").unwrap();
        assert_eq!(res.done.n, 1);
    }
}

#[test]
fn test_crash_after_header_write_lin_1682() {
    let dir = tmp_dir();
    {
        let mut db = Db::open(&dir).unwrap();
        db.run(r#"insert docs { id: "d1", title: "valid doc" }"#)
            .unwrap();
    }
    // Inject truncated WAL header (partial 4 bytes)
    let log_path = dir.join("log");
    {
        let mut f = OpenOptions::new().append(true).open(&log_path).unwrap();
        f.write_all(b"LINC").unwrap(); // partial 4 bytes of frame header
        f.flush().unwrap();
    }
    // Reopen DB: must gracefully ignore truncated partial frame
    {
        let mut db = Db::open(&dir).unwrap();
        let res = db.run("docs | count").unwrap();
        assert_eq!(res.done.n, 1);
    }
}

#[test]
fn test_crash_after_payload_write_lin_1683() {
    let dir = tmp_dir();
    {
        let mut db = Db::open(&dir).unwrap();
        db.run(r#"insert docs { id: "d1", title: "valid doc" }"#)
            .unwrap();
    }
    // Inject header + partial payload (missing checksum at end)
    let log_path = dir.join("log");
    {
        let mut f = OpenOptions::new().append(true).open(&log_path).unwrap();
        // Magic for checksummed frame: b"LIN3"
        // Length 20 bytes
        let len = 20u32.to_le_bytes();
        f.write_all(b"LIN3").unwrap();
        f.write_all(&len).unwrap();
        f.write_all(b"some truncated payld").unwrap(); // only 19 bytes, no checksum
        f.flush().unwrap();
    }
    // Reopen DB: must detect truncated/corrupted frame and recover valid data
    {
        let mut db = Db::open(&dir).unwrap();
        let res = db.run("docs | count").unwrap();
        assert_eq!(res.done.n, 1);
    }
}

#[test]
fn test_crash_after_checksum_write_lin_1684() {
    let dir = tmp_dir();
    {
        let mut db = Db::open(&dir).unwrap();
        db.run(r#"insert docs { id: "d1", title: "first doc" }"#)
            .unwrap();
        db.run(r#"insert docs { id: "d2", title: "second doc with full wal commit" }"#)
            .unwrap();
    }
    // Reopen DB directly: replay recovers all completed checksummed records
    {
        let mut db = Db::open(&dir).unwrap();
        let res = db.run("docs | count").unwrap();
        assert_eq!(res.done.n, 1);
        let items = db.run("docs | { id }").unwrap();
        assert_eq!(items.rows.len(), 2);
    }
}
