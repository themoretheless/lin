//! WAL shipping: one sink, optional TLS + shared token.
//!
//! Protocol (one request / response per connection):
//! - client → server: `LIN\x06` + `u16` LE token_len + token + `u64` LE `since_gen`
//! - server → client: `u64` LE `byte_len` + raw WAL frames (`export_wal_since`)
//!
//! Plaintext on loopback is still the local recipe. For another machine: TLS
//! (`--tls-cert` / `--tls-key` / `--tls-ca`) and a shared token. The server
//! holds at most **one** live pull at a time (extra accepts are dropped).

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream, ToSocketAddrs};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::server::ServerConfig;
use rustls::{ClientConfig, RootCertStore, ServerConnection, StreamOwned};

use crate::error::Error;
use crate::exec::Db;

/// Cap on a single pull payload (bytes).
pub const MAX_PULL_BYTES: u64 = 256 * 1024 * 1024;
const PROTO_MAGIC: [u8; 4] = *b"LIN\x06";
const MAX_TOKEN: usize = 4096;

fn io_err(e: impl std::fmt::Display) -> Error {
    Error::runtime(format!("wal ship: {e}"))
}

/// How the client authenticates the server certificate.
#[derive(Clone, Debug, Default)]
pub enum TlsClient {
    #[default]
    Off,
    /// PEM CA (or server cert) to trust.
    CaPem(Vec<u8>),
    /// Tests / lab only — do not use on an untrusted network.
    Insecure,
}

/// Client pull options.
#[derive(Clone, Debug, Default)]
pub struct PullOpts {
    pub token: Option<String>,
    pub tls: TlsClient,
}

/// Server TLS material (PEM).
#[derive(Clone, Debug)]
pub struct TlsServer {
    pub cert_pem: Vec<u8>,
    pub key_pem: Vec<u8>,
}

/// `wal-serve` options.
#[derive(Clone, Debug, Default)]
pub struct ServeOpts {
    pub token: Option<String>,
    pub tls: Option<TlsServer>,
}

/// Pull WAL frames with `gen > since` (plaintext, empty token).
pub fn pull(addr: impl ToSocketAddrs, since: u64) -> Result<Vec<u8>, Error> {
    pull_with(addr, since, &PullOpts::default())
}

/// Pull WAL frames with token / TLS.
pub fn pull_with(addr: impl ToSocketAddrs, since: u64, opts: &PullOpts) -> Result<Vec<u8>, Error> {
    let stream = TcpStream::connect(addr).map_err(io_err)?;
    stream
        .set_read_timeout(Some(Duration::from_secs(30)))
        .map_err(io_err)?;
    stream
        .set_write_timeout(Some(Duration::from_secs(30)))
        .map_err(io_err)?;
    match &opts.tls {
        TlsClient::Off => pull_io(stream, since, opts.token.as_deref()),
        other => {
            let cfg = client_config(other)?;
            let name = ServerName::try_from("localhost").map_err(io_err)?;
            let conn = rustls::ClientConnection::new(cfg, name).map_err(io_err)?;
            let mut tls = StreamOwned::new(conn, stream);
            pull_io(&mut tls, since, opts.token.as_deref())
        }
    }
}

fn pull_io<S: Read + Write>(
    mut stream: S,
    since: u64,
    token: Option<&str>,
) -> Result<Vec<u8>, Error> {
    write_hello(&mut stream, token.unwrap_or(""), since)?;
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

fn write_hello<W: Write>(w: &mut W, token: &str, since: u64) -> Result<(), Error> {
    if token.len() > MAX_TOKEN {
        return Err(Error::runtime("wal ship: token too long"));
    }
    w.write_all(&PROTO_MAGIC).map_err(io_err)?;
    w.write_all(&(token.len() as u16).to_le_bytes())
        .map_err(io_err)?;
    w.write_all(token.as_bytes()).map_err(io_err)?;
    w.write_all(&since.to_le_bytes()).map_err(io_err)?;
    w.flush().map_err(io_err)?;
    Ok(())
}

fn read_hello<R: Read>(r: &mut R) -> Result<(String, u64), Error> {
    let mut mag = [0u8; 4];
    r.read_exact(&mut mag).map_err(io_err)?;
    if mag != PROTO_MAGIC {
        return Err(Error::runtime(
            "wal ship: bad protocol magic (need LIN\\x06)",
        ));
    }
    let mut nbuf = [0u8; 2];
    r.read_exact(&mut nbuf).map_err(io_err)?;
    let n = u16::from_le_bytes(nbuf) as usize;
    if n > MAX_TOKEN {
        return Err(Error::runtime("wal ship: token too long"));
    }
    let mut tok = vec![0u8; n];
    if n > 0 {
        r.read_exact(&mut tok).map_err(io_err)?;
    }
    let token = String::from_utf8(tok).map_err(io_err)?;
    let mut since_buf = [0u8; 8];
    r.read_exact(&mut since_buf).map_err(io_err)?;
    Ok((token, u64::from_le_bytes(since_buf)))
}

fn token_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.bytes()
        .zip(b.bytes())
        .fold(0u8, |acc, (x, y)| acc | (x ^ y))
        == 0
}

fn client_config(tls: &TlsClient) -> Result<Arc<ClientConfig>, Error> {
    install_crypto();
    match tls {
        TlsClient::Off => Err(Error::runtime("wal ship: tls off")),
        TlsClient::Insecure => {
            let cfg = ClientConfig::builder()
                .dangerous()
                .with_custom_certificate_verifier(Arc::new(NoVerify))
                .with_no_client_auth();
            Ok(Arc::new(cfg))
        }
        TlsClient::CaPem(pem) => {
            let mut roots = RootCertStore::empty();
            let mut cursor = std::io::Cursor::new(pem.as_slice());
            for cert in rustls_pemfile::certs(&mut cursor) {
                let cert = cert.map_err(io_err)?;
                roots.add(cert).map_err(io_err)?;
            }
            if roots.is_empty() {
                return Err(Error::runtime("wal ship: empty TLS CA"));
            }
            Ok(Arc::new(
                ClientConfig::builder()
                    .with_root_certificates(roots)
                    .with_no_client_auth(),
            ))
        }
    }
}

fn server_config(tls: &TlsServer) -> Result<Arc<ServerConfig>, Error> {
    install_crypto();
    let mut cert_cursor = std::io::Cursor::new(tls.cert_pem.as_slice());
    let certs: Vec<CertificateDer<'static>> = rustls_pemfile::certs(&mut cert_cursor)
        .collect::<Result<Vec<_>, _>>()
        .map_err(io_err)?;
    let mut key_cursor = std::io::Cursor::new(tls.key_pem.as_slice());
    let key = rustls_pemfile::private_key(&mut key_cursor)
        .map_err(io_err)?
        .ok_or_else(|| Error::runtime("wal ship: no TLS private key"))?;
    let cfg = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .map_err(io_err)?;
    Ok(Arc::new(cfg))
}

fn install_crypto() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

#[derive(Debug)]
struct NoVerify;

impl rustls::client::danger::ServerCertVerifier for NoVerify {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp: &[u8],
        _now: UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &rustls::crypto::ring::default_provider().signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &rustls::crypto::ring::default_provider().signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        rustls::crypto::ring::default_provider()
            .signature_verification_algorithms
            .supported_schemes()
    }
}

/// Serve WAL pulls from an already-open durable primary [`Db`].
///
/// Blocks forever accepting connections. Holds the writer lock for `db`'s data
/// dir (exclusive with another writer process). At most one live sink.
pub fn serve_blocking(db: &Db, listener: TcpListener) -> Result<(), Error> {
    serve_blocking_opts(db, listener, &ServeOpts::default())
}

pub fn serve_blocking_opts(db: &Db, listener: TcpListener, opts: &ServeOpts) -> Result<(), Error> {
    if !db.is_durable() {
        return Err(Error::runtime(
            "wal ship: serve requires durable primary (--data)",
        ));
    }
    listener.set_nonblocking(false).map_err(io_err)?;
    let tls = match &opts.tls {
        Some(t) => Some(server_config(t)?),
        None => None,
    };
    let busy = AtomicBool::new(false);
    loop {
        let (stream, _) = listener.accept().map_err(io_err)?;
        if busy.swap(true, Ordering::AcqRel) {
            drop(stream);
            continue;
        }
        let r = handle_conn(db, stream, opts.token.as_deref(), tls.clone());
        busy.store(false, Ordering::Release);
        if let Err(e) = r {
            eprintln!("wal ship: client error: {e}");
        }
    }
}

/// Accept and handle a single pull connection (tests / controlled one-shot).
pub fn serve_one(db: &Db, listener: &TcpListener) -> Result<(), Error> {
    serve_one_opts(db, listener, &ServeOpts::default())
}

pub fn serve_one_opts(db: &Db, listener: &TcpListener, opts: &ServeOpts) -> Result<(), Error> {
    if !db.is_durable() {
        return Err(Error::runtime(
            "wal ship: serve requires durable primary (--data)",
        ));
    }
    let tls = match &opts.tls {
        Some(t) => Some(server_config(t)?),
        None => None,
    };
    let (stream, _) = listener.accept().map_err(io_err)?;
    handle_conn(db, stream, opts.token.as_deref(), tls)
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

fn handle_conn(
    db: &Db,
    stream: TcpStream,
    expect_token: Option<&str>,
    tls: Option<Arc<ServerConfig>>,
) -> Result<(), Error> {
    stream
        .set_read_timeout(Some(Duration::from_secs(30)))
        .map_err(io_err)?;
    stream
        .set_write_timeout(Some(Duration::from_secs(30)))
        .map_err(io_err)?;
    match tls {
        Some(cfg) => {
            let conn = ServerConnection::new(cfg).map_err(io_err)?;
            let mut tls = StreamOwned::new(conn, stream);
            handle_io(db, &mut tls, expect_token)
        }
        None => handle_io(db, stream, expect_token),
    }
}

fn handle_io<S: Read + Write>(
    db: &Db,
    mut stream: S,
    expect_token: Option<&str>,
) -> Result<(), Error> {
    let (got, since) = read_hello(&mut stream)?;
    let want = expect_token.unwrap_or("");
    if !token_eq(&got, want) {
        return Err(Error::runtime("wal ship: bad token"));
    }
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

    #[test]
    fn pull_rejects_bad_token() {
        let dir = std::env::temp_dir().join(format!(
            "lin-ship-tok-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut db = Db::open(&dir).unwrap();
        db.run(r#"insert docs { uri: "raw://t", title: "T", layer: "wiki" }"#)
            .unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let opts = ServeOpts {
            token: Some("secret".into()),
            tls: None,
        };
        let serve = thread::spawn(move || {
            serve_one_opts(&db, &listener, &opts).unwrap();
            let _ = db.close();
        });
        thread::sleep(Duration::from_millis(30));
        let err = pull_with(
            addr,
            0,
            &PullOpts {
                token: Some("nope".into()),
                tls: TlsClient::Off,
            },
        )
        .unwrap_err();
        assert!(err.to_string().contains("wal ship"), "{err}");
        let _ = serve.join();
        let _ = std::fs::remove_dir_all(&dir);
    }
}
