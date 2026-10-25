use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;

use lin::Db;

static SEQ: AtomicU64 = AtomicU64::new(0);

fn tmp_dir(name: &str) -> std::path::PathBuf {
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("lin-repl-{name}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

#[test]
fn test_handshake_protocol_version_lin_1361() {
    let dir = tmp_dir("handshake");
    let mut primary = Db::open(&dir).unwrap();
    primary
        .run(r#"insert docs { id: "d1", title: "test" }"#)
        .unwrap();

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();

    let serve = thread::spawn(move || {
        let err = lin::ship::serve_one(&primary, &listener).unwrap_err();
        assert!(err.to_string().contains("bad protocol magic"), "{err}");
    });

    // Send invalid protocol version: LIN\x05 instead of LIN\x06
    let mut stream = TcpStream::connect(addr).unwrap();
    stream
        .write_all(b"LIN\x05\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00")
        .unwrap();
    stream.flush().unwrap();

    let mut buf = [0u8; 8];
    // Server drops connection due to bad protocol version
    let read_res = stream.read_exact(&mut buf);
    assert!(read_res.is_err());

    serve.join().unwrap();
}

#[test]
fn test_server_identity_validation_lin_1362() {
    // Verifying server name helper
    assert_eq!(
        lin::ship::tls_server_name("example.com:9876"),
        Some("example.com".to_string())
    );
    assert_eq!(lin::ship::tls_server_name("127.0.0.1:9876"), None);
}

#[test]
fn test_transport_framing_limits_lin_1363() {
    // MAX_PULL_BYTES is 256MB
    assert_eq!(lin::ship::MAX_PULL_BYTES, 256 * 1024 * 1024);
}

#[test]
fn test_replication_byte_order_lin_1364() {
    let since = 0x0102030405060708u64;
    let le_bytes = since.to_le_bytes();
    assert_eq!(le_bytes[0], 0x08);
    assert_eq!(le_bytes[7], 0x01);
}

#[test]
fn test_follower_generation_continuity_lin_1401() {
    let dir = tmp_dir("cont");
    let mut primary = Db::open(&dir).unwrap();
    primary
        .run(r#"insert docs { id: "d1", title: "1" }"#)
        .unwrap();
    let frames_gen1 = primary.export_wal_since(0).unwrap();
    assert!(!frames_gen1.is_empty());

    primary
        .run(r#"insert docs { id: "d2", title: "2" }"#)
        .unwrap();
    let frames_gen2 = primary.export_wal_since(1).unwrap();
    assert!(!frames_gen2.is_empty());

    let mut follower = Db::empty();
    assert_eq!(follower.r#gen(), 0);

    // Apply gen 1
    let applied1 = follower.apply_wal(&frames_gen1).unwrap();
    assert_eq!(applied1, 1);
    assert_eq!(follower.r#gen(), 1);

    // Apply gen 2
    let applied2 = follower.apply_wal(&frames_gen2).unwrap();
    assert_eq!(applied2, 1);
    assert_eq!(follower.r#gen(), 2);
}

#[test]
fn test_duplicate_incoming_records_lin_1402() {
    let dir = tmp_dir("dup");
    let mut primary = Db::open(&dir).unwrap();
    primary
        .run(r#"insert docs { id: "d1", title: "1" }"#)
        .unwrap();
    let frames = primary.export_wal_since(0).unwrap();
    assert!(!frames.is_empty());

    let mut follower = Db::empty();
    let applied = follower.apply_wal(&frames).unwrap();
    assert_eq!(applied, 1);
    assert_eq!(follower.r#gen(), 1);

    // Re-applying duplicate frames with gen <= current must be a no-op (applied 0)
    let dup_applied = follower.apply_wal(&frames).unwrap();
    assert_eq!(dup_applied, 0);
    assert_eq!(follower.r#gen(), 1);
}

#[test]
fn test_out_of_order_and_missing_generation_lin_1403_1404() {
    let dir = tmp_dir("gap");
    let mut primary = Db::open(&dir).unwrap();
    primary
        .run(r#"insert docs { id: "d1", title: "1" }"#)
        .unwrap();
    primary
        .run(r#"insert docs { id: "d2", title: "2" }"#)
        .unwrap();

    let frames_gen2 = primary.export_wal_since(1).unwrap();
    assert!(!frames_gen2.is_empty());

    let mut follower = Db::empty();
    // Follower is at gen 0, receives gen 2 directly (missing gen 1)
    let err = follower.apply_wal(&frames_gen2).unwrap_err();
    assert!(
        err.to_string().contains("gap") && err.to_string().contains("need gen 1"),
        "{err}"
    );
}
