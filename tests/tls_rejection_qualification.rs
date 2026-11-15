//! WAL shipping over TLS refuses every connection whose server it cannot authenticate:
//! a certificate for another host name, an expired certificate, a certificate from an
//! authority the client does not trust, a plaintext server when the client expects TLS,
//! and a TLS server when the client speaks plaintext. None of them yields frames.

use std::net::TcpListener;

use lin::Db;
use lin::ship::{PullOpts, ServeOpts, TlsClient, TlsServer, pull_with, serve_one_opts};

fn tmp() -> std::path::PathBuf {
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let d = std::env::temp_dir().join(format!("lin-tls-rej-{}-{}", std::process::id(), SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn self_signed(names: &[&str], expired: bool) -> TlsServer {
    let mut params = rcgen::CertificateParams::new(names.iter().map(|s| s.to_string()).collect::<Vec<_>>()).unwrap();
    if expired {
        params.not_before = rcgen::date_time_ymd(2000, 1, 1);
        params.not_after = rcgen::date_time_ymd(2001, 1, 1);
    }
    let key = rcgen::KeyPair::generate().unwrap();
    let cert = params.self_signed(&key).unwrap();
    TlsServer { cert_pem: cert.pem().into_bytes(), key_pem: key.serialize_pem().into_bytes() }
}

/// Serve one connection with `server_tls`, pull with `client`; returns the pull result.
fn attempt(server_tls: Option<TlsServer>, client: PullOpts) -> Result<Vec<u8>, lin::Error> {
    let dir = tmp();
    let mut db = Db::open(&dir).unwrap();
    db.run(r#"insert docs { uri: "raw://tls/x", title: "X", layer: "raw" }"#).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let opts = ServeOpts { token: None, tls: server_tls };
    let server = std::thread::spawn(move || {
        let _ = serve_one_opts(&db, &listener, &opts);
    });
    let res = pull_with(addr, 0, &client);
    let _ = server.join();
    res
}

#[test]
fn unauthenticated_servers_are_refused() {
    let good = self_signed(&["localhost"], false);
    let trust_good = TlsClient::CaPem(good.cert_pem.clone());

    // Baseline: the right certificate and name work.
    assert!(attempt(Some(self_signed_clone(&good)), PullOpts { tls: trust_good.clone(), ..Default::default() }).is_ok(), "baseline TLS pull failed");

    // Certificate valid only for "localhost", client verifies another name.
    let r = attempt(Some(self_signed_clone(&good)), PullOpts { tls: trust_good.clone(), server_name: Some("db.example.com".into()), ..Default::default() });
    assert!(r.is_err(), "host name mismatch accepted");

    // Expired certificate, trusted directly.
    let expired = self_signed(&["localhost"], true);
    let r = attempt(Some(self_signed_clone(&expired)), PullOpts { tls: TlsClient::CaPem(expired.cert_pem.clone()), ..Default::default() });
    assert!(r.is_err(), "expired certificate accepted");

    // Server certificate from an authority the client does not trust.
    let other = self_signed(&["localhost"], false);
    let r = attempt(Some(self_signed_clone(&other)), PullOpts { tls: trust_good.clone(), ..Default::default() });
    assert!(r.is_err(), "untrusted authority accepted");

    // Client expects TLS, server speaks plaintext (no certificate at all).
    let r = attempt(None, PullOpts { tls: trust_good.clone(), ..Default::default() });
    assert!(r.is_err(), "plaintext server accepted by a TLS client");

    // Client speaks plaintext, server requires TLS.
    let r = attempt(Some(self_signed_clone(&good)), PullOpts::default());
    assert!(r.is_err(), "TLS server accepted a plaintext client");
}

fn self_signed_clone(t: &TlsServer) -> TlsServer {
    TlsServer { cert_pem: t.cert_pem.clone(), key_pem: t.key_pem.clone() }
}
