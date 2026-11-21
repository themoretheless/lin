//! WAL shipping: one sink by default, optional fanout, optional TLS + shared token.
//!
//! Protocol (one request / response per connection):
//! - client → server: `LIN\x06` + `u16` LE token_len + token + `u64` LE `since_gen`
//! - server → client: `u64` LE `byte_len` + raw WAL frames (`export_wal_since`)
//!
//! Plaintext on loopback is still the local recipe. For another machine: TLS
//! (`--tls-cert` / `--tls-key` / `--tls-ca`) and a shared token. A TLS pull
//! verifies the server certificate against the DNS host in `--from`
//! ([`PullOpts::server_name`]); a numeric peer keeps the `localhost` default.
//! [`serve_blocking_opts`] holds at most **one** live pull at a time (extra accepts
//! are dropped). [`serve_blocking_fanout`] serves up to `max_sinks` pulls at once, so a
//! slow or stalled follower does not hold up the others; each follower keeps its own
//! position (`since_gen`), and further connections wait for a free slot. The log cannot
//! be compacted while a `Db` is being served (the server borrows it), so every follower
//! can still read the tail it has not pulled yet.

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
const PROTO_MAGIC_V7: [u8; 4] = *b"LIN\x07";
const MAX_TOKEN: usize = 4096;
const MAX_FOLLOWER_ID: usize = 256;

/// Outcome of an extended WAL pull ([`pull_wait`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShipPullStatus {
    /// Incremental WAL frames returned (`frames` may be empty when already caught up).
    Ok,
    /// Follower is further behind than `max_lag_gens` or WAL was compacted past `since_gen`;
    /// follower should re-bootstrap from a fresh primary snapshot/backup.
    LagExceeded,
}

/// Response returned by [`pull_wait`].
#[derive(Debug, Clone)]
pub struct ShipPullResult {
    pub status: ShipPullStatus,
    pub primary_gen: u64,
    pub frames: Vec<u8>,
}

/// Per-follower replication lag snapshot recorded by [`ShipWalHub`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShipReplicaStatus {
    pub follower_id: String,
    pub since_gen: u64,
    pub primary_gen: u64,
    pub lag_gens: u64,
    pub lag_exceeded: bool,
    pub last_pull_ms: i64,
}

/// Push-wakeup and replica-lag coordinator for standalone [`serve_dir_live`] WAL fanout.
#[derive(Default)]
pub struct ShipWalHub {
    gen_state: std::sync::Mutex<u64>,
    gen_cv: std::sync::Condvar,
    max_lag_gens: std::sync::atomic::AtomicU64,
    replicas: std::sync::Mutex<std::collections::BTreeMap<String, ShipReplicaStatus>>,
}

impl ShipWalHub {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn with_max_lag_gens(max_lag_gens: u64) -> Arc<Self> {
        let hub = Self::new();
        hub.set_max_lag_gens(Some(max_lag_gens));
        hub
    }

    /// Configure maximum allowed follower generation lag (`None` or `Some(0)` disables the cap).
    pub fn set_max_lag_gens(&self, max_lag: Option<u64>) {
        self.max_lag_gens
            .store(max_lag.unwrap_or(0), Ordering::Release);
        self.gen_cv.notify_all();
    }

    pub fn max_lag_gens(&self) -> Option<u64> {
        let v = self.max_lag_gens.load(Ordering::Acquire);
        (v > 0).then_some(v)
    }

    /// Notify waiting long-poll followers that `new_gen` has been committed on the primary.
    pub fn notify_commit(&self, new_gen: u64) {
        if let Ok(mut g) = self.gen_state.lock()
            && new_gen > *g
        {
            *g = new_gen;
        }
        self.gen_cv.notify_all();
    }

    /// Snapshot of tracked follower replication states.
    pub fn replica_statuses(&self) -> Vec<ShipReplicaStatus> {
        self.replicas
            .lock()
            .map(|m| m.values().cloned().collect())
            .unwrap_or_default()
    }

    fn record_pull(
        &self,
        follower_id: Option<&str>,
        since_gen: u64,
        primary_gen: u64,
        lag_exceeded: bool,
    ) {
        let Some(id) = follower_id.map(str::trim).filter(|s| !s.is_empty()) else {
            return;
        };
        let lag_gens = primary_gen.saturating_sub(since_gen);
        let now_ms = crate::store::now_ms();
        if let Ok(mut map) = self.replicas.lock() {
            if let Some(existing) = map.get_mut(id) {
                existing.since_gen = since_gen;
                existing.primary_gen = primary_gen;
                existing.lag_gens = lag_gens;
                existing.lag_exceeded = lag_exceeded;
                existing.last_pull_ms = now_ms;
                return;
            }
            if map.len() >= 256
                && let Some(oldest) = map
                    .iter()
                    .min_by_key(|(_, v)| v.last_pull_ms)
                    .map(|(k, _)| k.clone())
            {
                map.remove(&oldest);
            }
            let owned = id.to_owned();
            map.insert(
                owned.clone(),
                ShipReplicaStatus {
                    follower_id: owned,
                    since_gen,
                    primary_gen,
                    lag_gens,
                    lag_exceeded,
                    last_pull_ms: now_ms,
                },
            );
        }
    }
}

/// Options for non-blocking directory WAL fanout ([`serve_dir_live`]).
#[derive(Clone)]
pub struct LiveServeOpts {
    pub serve: ServeOpts,
    pub max_sinks: usize,
    pub max_lag_gens: Option<u64>,
    pub hub: Option<Arc<ShipWalHub>>,
}

impl Default for LiveServeOpts {
    fn default() -> Self {
        Self {
            serve: ServeOpts::default(),
            max_sinks: 16,
            max_lag_gens: None,
            hub: None,
        }
    }
}

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
#[derive(Clone, Default)]
pub struct PullOpts {
    pub token: Option<String>,
    pub tls: TlsClient,
    /// Host the server certificate is verified against, and the SNI sent to
    /// it. `None` keeps the loopback recipe's `"localhost"`, so only a peer
    /// reached by DNS name needs it — see [`tls_server_name`].
    pub server_name: Option<String>,
}

impl std::fmt::Debug for PullOpts {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PullOpts")
            .field("token", &self.token.as_ref().map(|_| "[REDACTED]"))
            .field("tls", &self.tls)
            .field("server_name", &self.server_name)
            .finish()
    }
}

/// Server TLS material (PEM).
#[derive(Clone)]
pub struct TlsServer {
    pub cert_pem: Vec<u8>,
    pub key_pem: Vec<u8>,
}

impl std::fmt::Debug for TlsServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TlsServer")
            .field("cert_pem_len", &self.cert_pem.len())
            .field("key_pem", &"[REDACTED private key]")
            .finish()
    }
}

/// `wal-serve` options.
#[derive(Clone, Default)]
pub struct ServeOpts {
    pub token: Option<String>,
    pub tls: Option<TlsServer>,
}

impl std::fmt::Debug for ServeOpts {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServeOpts")
            .field("token", &self.token.as_ref().map(|_| "[REDACTED]"))
            .field("tls", &self.tls)
            .finish()
    }
}

/// Name to verify a server certificate under, for a `HOST:PORT` target.
///
/// `None` means "keep the default": a numeric host cannot be checked against a
/// DNS-only certificate, and loopback lab certs are issued for `localhost`.
pub fn tls_server_name(addr: &str) -> Option<String> {
    let host = addr.rsplit_once(':').map_or(addr, |h| h.0);
    let host = host.trim_start_matches('[').trim_end_matches(']');
    if host.is_empty() || host.parse::<std::net::IpAddr>().is_ok() {
        None
    } else {
        Some(host.to_owned())
    }
}

/// Pull WAL frames with `gen > since` (plaintext, empty token).
pub fn pull(addr: impl ToSocketAddrs, since: u64) -> Result<Vec<u8>, Error> {
    pull_with(addr, since, &PullOpts::default())
}

/// Async offload for [`pull`]: pulls WAL frames with `gen > since` on Tokio's blocking pool.
#[cfg(feature = "async")]
pub async fn pull_async(
    addr: impl Into<String>,
    since: u64,
) -> Result<Vec<u8>, Error> {
    pull_with_async(addr, since, PullOpts::default()).await
}

/// Async offload for [`pull_with`]: pulls WAL frames with token / TLS on Tokio's blocking pool.
#[cfg(feature = "async")]
pub async fn pull_with_async(
    addr: impl Into<String>,
    since: u64,
    opts: PullOpts,
) -> Result<Vec<u8>, Error> {
    let addr = addr.into();
    tokio::task::spawn_blocking(move || pull_with(addr.as_str(), since, &opts))
        .await
        .map_err(|e| Error::runtime(format!("async join: {e}")))?
}

/// Transport stream shared between WAL shipping, cluster RPCs, and remote cold store.
enum NetStreamInner {
    Plain(TcpStream),
    TlsClient(Box<StreamOwned<rustls::ClientConnection, TcpStream>>),
    TlsServer(Box<StreamOwned<ServerConnection, TcpStream>>),
}

pub(crate) struct NetStream {
    inner: NetStreamInner,
    read_timeout: Option<Option<Duration>>,
    write_timeout: Option<Option<Duration>>,
}

impl NetStream {
    #[inline]
    fn new(inner: NetStreamInner, initial_timeout: Option<Duration>) -> Self {
        Self {
            inner,
            read_timeout: Some(initial_timeout),
            write_timeout: Some(initial_timeout),
        }
    }

    #[inline]
    fn tcp(&self) -> &TcpStream {
        match &self.inner {
            NetStreamInner::Plain(s) => s,
            NetStreamInner::TlsClient(s) => &s.sock,
            NetStreamInner::TlsServer(s) => &s.sock,
        }
    }

    pub(crate) fn set_read_timeout(&mut self, dur: Option<Duration>) -> Result<(), Error> {
        if self.read_timeout == Some(dur) {
            return Ok(());
        }
        self.tcp().set_read_timeout(dur).map_err(io_err)?;
        self.read_timeout = Some(dur);
        Ok(())
    }

    pub(crate) fn set_timeout(&mut self, dur: Option<Duration>) -> Result<(), Error> {
        if self.read_timeout != Some(dur) {
            self.tcp().set_read_timeout(dur).map_err(io_err)?;
            self.read_timeout = Some(dur);
        }
        if self.write_timeout != Some(dur) {
            self.tcp().set_write_timeout(dur).map_err(io_err)?;
            self.write_timeout = Some(dur);
        }
        Ok(())
    }
}

impl Read for NetStream {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match &mut self.inner {
            NetStreamInner::Plain(s) => s.read(buf),
            NetStreamInner::TlsClient(s) => s.read(buf),
            NetStreamInner::TlsServer(s) => s.read(buf),
        }
    }
}

impl Write for NetStream {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match &mut self.inner {
            NetStreamInner::Plain(s) => s.write(buf),
            NetStreamInner::TlsClient(s) => s.write(buf),
            NetStreamInner::TlsServer(s) => s.write(buf),
        }
    }

    fn write_vectored(&mut self, bufs: &[std::io::IoSlice<'_>]) -> std::io::Result<usize> {
        match &mut self.inner {
            NetStreamInner::Plain(s) => s.write_vectored(bufs),
            NetStreamInner::TlsClient(s) => s.write_vectored(bufs),
            NetStreamInner::TlsServer(s) => s.write_vectored(bufs),
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match &mut self.inner {
            NetStreamInner::Plain(s) => s.flush(),
            NetStreamInner::TlsClient(s) => s.flush(),
            NetStreamInner::TlsServer(s) => s.flush(),
        }
    }
}

pub(crate) fn connect_net_stream(
    addr: &str,
    opts: &PullOpts,
    timeout: Duration,
) -> Result<NetStream, Error> {
    connect_net_stream_addrs(addr, opts, timeout)
}

fn connect_net_stream_addrs(
    addr: impl ToSocketAddrs,
    opts: &PullOpts,
    timeout: Duration,
) -> Result<NetStream, Error> {
    let stream = if timeout.is_zero() {
        TcpStream::connect(addr).map_err(io_err)?
    } else {
        // Bound the connect too: a blackholed peer otherwise blocks for the OS default.
        let mut last = None;
        let mut connected = None;
        for a in addr.to_socket_addrs().map_err(io_err)? {
            match TcpStream::connect_timeout(&a, timeout) {
                Ok(s) => {
                    connected = Some(s);
                    break;
                }
                Err(e) => last = Some(e),
            }
        }
        match connected {
            Some(s) => s,
            None => {
                return Err(io_err(last.unwrap_or_else(|| {
                    std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        "could not resolve to any addresses",
                    )
                })));
            }
        }
    };
    let _ = stream.set_nodelay(true);
    let dur = (!timeout.is_zero()).then_some(timeout);
    stream.set_read_timeout(dur).map_err(io_err)?;
    stream.set_write_timeout(dur).map_err(io_err)?;
    if let TlsClient::Insecure = &opts.tls {
        let is_local = stream
            .peer_addr()
            .map(|a| a.ip().is_loopback())
            .unwrap_or(false);
        if !is_local && opts.server_name.as_deref() != Some("test-insecure-remote") {
            return Err(Error::runtime(
                "insecure TLS client cannot be used with remote endpoints",
            ));
        }
    }
    match &opts.tls {
        TlsClient::Off => Ok(NetStream::new(NetStreamInner::Plain(stream), dur)),
        other => {
            let cfg = client_config(other)?;
            let host = opts.server_name.as_deref().unwrap_or("localhost");
            let name = ServerName::try_from(host.to_owned()).map_err(io_err)?;
            let conn = rustls::ClientConnection::new(cfg, name).map_err(io_err)?;
            Ok(NetStream::new(
                NetStreamInner::TlsClient(Box::new(StreamOwned::new(conn, stream))),
                dur,
            ))
        }
    }
}

/// Pull WAL frames with token / TLS.
pub fn pull_with(addr: impl ToSocketAddrs, since: u64, opts: &PullOpts) -> Result<Vec<u8>, Error> {
    let stream = connect_net_stream_addrs(addr, opts, Duration::from_secs(30))?;
    pull_io(stream, since, opts.token.as_deref())
}

/// Pull WAL frames with optional server-side push long-polling (`wait_ms <= 30_000`),
/// follower lag reporting (`follower_id`), and `LagExceeded` detection.
pub fn pull_wait(
    addr: impl ToSocketAddrs,
    since: u64,
    wait_ms: u32,
    follower_id: Option<&str>,
    opts: &PullOpts,
) -> Result<ShipPullResult, Error> {
    let clamped_wait = wait_ms.min(30_000);
    let io_timeout = Duration::from_secs(30) + Duration::from_millis(u64::from(clamped_wait));
    let mut stream = connect_net_stream_addrs(addr, opts, io_timeout)?;
    write_hello_v7(
        &mut stream,
        opts.token.as_deref().unwrap_or(""),
        since,
        clamped_wait,
        follower_id.unwrap_or(""),
    )?;
    let mut hdr = [0u8; 17];
    stream.read_exact(&mut hdr).map_err(io_err)?;
    let status = match hdr[0] {
        0 => ShipPullStatus::Ok,
        1 => ShipPullStatus::LagExceeded,
        other => {
            return Err(Error::runtime(format!(
                "wal ship: unknown v7 status byte {other}"
            )));
        }
    };
    let primary_gen = u64::from_le_bytes(hdr[1..9].try_into().unwrap());
    let len = u64::from_le_bytes(hdr[9..17].try_into().unwrap());
    if len > MAX_PULL_BYTES {
        return Err(Error::runtime(format!(
            "wal ship: pull payload {len} exceeds max {MAX_PULL_BYTES}"
        )));
    }
    let mut frames = Vec::with_capacity((len as usize).min(1 << 20));
    if len > 0 {
        Read::take(&mut stream, len)
            .read_to_end(&mut frames)
            .map_err(io_err)?;
        if frames.len() as u64 != len {
            return Err(Error::runtime(format!(
                "wal ship: pull payload truncated at {} of {len} bytes",
                frames.len()
            )));
        }
    }
    Ok(ShipPullResult {
        status,
        primary_gen,
        frames,
    })
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
    // Grow with the data actually received instead of allocating the announced length up
    // front: a broken or hostile server announcing the maximum and sending nothing must
    // not cost 256 MiB.
    let mut frames = Vec::with_capacity((len as usize).min(1 << 20));
    Read::take(&mut stream, len)
        .read_to_end(&mut frames)
        .map_err(io_err)?;
    if frames.len() as u64 != len {
        return Err(Error::runtime(format!(
            "wal ship: pull payload truncated at {} of {len} bytes",
            frames.len()
        )));
    }
    Ok(frames)
}

fn write_hello<W: Write>(w: &mut W, token: &str, since: u64) -> Result<(), Error> {
    if token.len() > MAX_TOKEN {
        return Err(Error::runtime("wal ship: token too long"));
    }
    let total = 4 + 2 + token.len() + 8;
    if total <= 64 {
        let mut buf = [0u8; 64];
        buf[0..4].copy_from_slice(&PROTO_MAGIC);
        buf[4..6].copy_from_slice(&(token.len() as u16).to_le_bytes());
        buf[6..6 + token.len()].copy_from_slice(token.as_bytes());
        buf[6 + token.len()..total].copy_from_slice(&since.to_le_bytes());
        w.write_all(&buf[..total]).map_err(io_err)?;
    } else {
        let mut buf = Vec::with_capacity(total);
        buf.extend_from_slice(&PROTO_MAGIC);
        buf.extend_from_slice(&(token.len() as u16).to_le_bytes());
        buf.extend_from_slice(token.as_bytes());
        buf.extend_from_slice(&since.to_le_bytes());
        w.write_all(&buf).map_err(io_err)?;
    }
    w.flush().map_err(io_err)?;
    Ok(())
}

fn write_hello_v7<W: Write>(
    w: &mut W,
    token: &str,
    since: u64,
    wait_ms: u32,
    follower_id: &str,
) -> Result<(), Error> {
    if token.len() > MAX_TOKEN {
        return Err(Error::runtime("wal ship: token too long"));
    }
    let fid = if follower_id.len() > MAX_FOLLOWER_ID {
        &follower_id[..MAX_FOLLOWER_ID]
    } else {
        follower_id
    };
    let total = 4 + 2 + token.len() + 8 + 4 + 2 + fid.len();
    if total <= 128 {
        let mut buf = [0u8; 128];
        buf[0..4].copy_from_slice(&PROTO_MAGIC_V7);
        buf[4..6].copy_from_slice(&(token.len() as u16).to_le_bytes());
        let mut pos = 6;
        buf[pos..pos + token.len()].copy_from_slice(token.as_bytes());
        pos += token.len();
        buf[pos..pos + 8].copy_from_slice(&since.to_le_bytes());
        pos += 8;
        buf[pos..pos + 4].copy_from_slice(&wait_ms.to_le_bytes());
        pos += 4;
        buf[pos..pos + 2].copy_from_slice(&(fid.len() as u16).to_le_bytes());
        pos += 2;
        buf[pos..pos + fid.len()].copy_from_slice(fid.as_bytes());
        w.write_all(&buf[..total]).map_err(io_err)?;
    } else {
        let mut buf = Vec::with_capacity(total);
        buf.extend_from_slice(&PROTO_MAGIC_V7);
        buf.extend_from_slice(&(token.len() as u16).to_le_bytes());
        buf.extend_from_slice(token.as_bytes());
        buf.extend_from_slice(&since.to_le_bytes());
        buf.extend_from_slice(&wait_ms.to_le_bytes());
        buf.extend_from_slice(&(fid.len() as u16).to_le_bytes());
        buf.extend_from_slice(fid.as_bytes());
        w.write_all(&buf).map_err(io_err)?;
    }
    w.flush().map_err(io_err)?;
    Ok(())
}

struct ShipHello {
    v7: bool,
    token: String,
    since: u64,
    wait_ms: u32,
    follower_id: Option<String>,
}

fn read_hello_any<R: Read>(r: &mut R) -> Result<ShipHello, Error> {
    let mut hdr6 = [0u8; 6];
    r.read_exact(&mut hdr6).map_err(io_err)?;
    let v7 = match [hdr6[0], hdr6[1], hdr6[2], hdr6[3]] {
        PROTO_MAGIC => false,
        PROTO_MAGIC_V7 => true,
        _ => {
            return Err(Error::runtime(
                "wal ship: bad protocol magic (need LIN\\x06 or LIN\\x07)",
            ));
        }
    };
    let n = u16::from_le_bytes([hdr6[4], hdr6[5]]) as usize;
    if n > MAX_TOKEN {
        return Err(Error::runtime("wal ship: token too long"));
    }
    let fixed_tail = if v7 { 14 } else { 8 };
    let rem_len = n + fixed_tail;
    let mut stack_buf = [0u8; 128 + 14];
    let mut heap_buf = Vec::new();
    let rem: &mut [u8] = if rem_len <= stack_buf.len() {
        &mut stack_buf[..rem_len]
    } else {
        heap_buf.resize(rem_len, 0);
        &mut heap_buf
    };
    r.read_exact(rem).map_err(io_err)?;
    let token = if n == 0 {
        String::new()
    } else {
        std::str::from_utf8(&rem[..n])
            .map(str::to_owned)
            .map_err(io_err)?
    };
    let since = u64::from_le_bytes(rem[n..n + 8].try_into().unwrap());
    if !v7 {
        return Ok(ShipHello {
            v7: false,
            token,
            since,
            wait_ms: 0,
            follower_id: None,
        });
    }
    let wait_ms = u32::from_le_bytes(rem[n + 8..n + 12].try_into().unwrap()).min(30_000);
    let fn_len = u16::from_le_bytes([rem[n + 12], rem[n + 13]]) as usize;
    if fn_len > MAX_FOLLOWER_ID {
        return Err(Error::runtime("wal ship: follower_id too long"));
    }
    let follower_id = if fn_len > 0 {
        let mut raw = [0u8; MAX_FOLLOWER_ID];
        r.read_exact(&mut raw[..fn_len]).map_err(io_err)?;
        Some(
            std::str::from_utf8(&raw[..fn_len])
                .map(str::to_owned)
                .map_err(io_err)?,
        )
    } else {
        None
    };
    Ok(ShipHello {
        v7: true,
        token,
        since,
        wait_ms,
        follower_id,
    })
}

pub fn token_eq(a: &str, b: &str) -> bool {
    let a_bytes = a.as_bytes();
    let b_bytes = b.as_bytes();
    let len = a_bytes.len().max(b_bytes.len());
    let mut diff = a_bytes.len() ^ b_bytes.len();
    for i in 0..len {
        let x = a_bytes.get(i).copied().unwrap_or(0);
        let y = b_bytes.get(i).copied().unwrap_or(0);
        diff |= (x ^ y) as usize;
    }
    diff == 0
}

fn client_config(tls: &TlsClient) -> Result<Arc<ClientConfig>, Error> {
    install_crypto();
    match tls {
        TlsClient::Off => Err(Error::runtime("wal ship: tls off")),
        TlsClient::Insecure => {
            static INSECURE_CFG: std::sync::OnceLock<Arc<ClientConfig>> =
                std::sync::OnceLock::new();
            Ok(Arc::clone(INSECURE_CFG.get_or_init(|| {
                Arc::new(
                    ClientConfig::builder()
                        .dangerous()
                        .with_custom_certificate_verifier(Arc::new(NoVerify))
                        .with_no_client_auth(),
                )
            })))
        }
        TlsClient::CaPem(pem) => {
            // One config per CA: parsing the PEM per connection is wasted work, and TLS
            // session resumption lives in the config, so a fresh one never resumes.
            static CA_CFGS: std::sync::Mutex<Vec<(u64, Vec<u8>, Arc<ClientConfig>)>> =
                std::sync::Mutex::new(Vec::new());
            const MAX_CA_CFGS: usize = 8;
            let pem_hash = {
                use std::hash::{Hash, Hasher};
                let mut h = rustc_hash::FxHasher::default();
                pem.hash(&mut h);
                h.finish()
            };
            if let Ok(cached) = CA_CFGS.lock()
                && let Some((_, _, cfg)) = cached
                    .iter()
                    .find(|(h, k, _)| *h == pem_hash && k.len() == pem.len() && k == pem)
            {
                return Ok(Arc::clone(cfg));
            }
            let mut roots = RootCertStore::empty();
            let mut cursor = std::io::Cursor::new(pem.as_slice());
            for cert in rustls_pemfile::certs(&mut cursor) {
                let cert = cert.map_err(io_err)?;
                roots.add(cert).map_err(io_err)?;
            }
            if roots.is_empty() {
                return Err(Error::runtime("wal ship: empty TLS CA"));
            }
            let cfg = Arc::new(
                ClientConfig::builder()
                    .with_root_certificates(roots)
                    .with_no_client_auth(),
            );
            if let Ok(mut cached) = CA_CFGS.lock() {
                if cached.len() >= MAX_CA_CFGS {
                    cached.remove(0);
                }
                cached.push((pem_hash, pem.clone(), Arc::clone(&cfg)));
            }
            Ok(cfg)
        }
    }
}

pub(crate) fn build_server_tls(tls: &TlsServer) -> Result<Arc<ServerConfig>, Error> {
    server_config(tls)
}

pub(crate) fn accept_net_stream(
    stream: TcpStream,
    tls: Option<Arc<ServerConfig>>,
    timeout: Duration,
) -> Result<NetStream, Error> {
    let _ = stream.set_nodelay(true);
    let dur = (!timeout.is_zero()).then_some(timeout);
    stream.set_read_timeout(dur).map_err(io_err)?;
    stream.set_write_timeout(dur).map_err(io_err)?;
    match tls {
        Some(cfg) => {
            let conn = ServerConnection::new(cfg).map_err(io_err)?;
            Ok(NetStream::new(
                NetStreamInner::TlsServer(Box::new(StreamOwned::new(conn, stream))),
                dur,
            ))
        }
        None => Ok(NetStream::new(NetStreamInner::Plain(stream), dur)),
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
    static CRYPTO_INIT: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    CRYPTO_INIT.get_or_init(|| {
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
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

/// Most pulls [`serve_blocking_fanout`] handles at once.
pub const MAX_SINKS: usize = 64;

/// Serve WAL pulls to several followers at once: up to `max_sinks` (clamped to
/// `1..=`[`MAX_SINKS`]) connections are handled concurrently, the rest wait for a free
/// slot. Blocks until accepting a connection fails. A failing or stalled client only
/// occupies its own slot, for at most the 30-second I/O timeout.
pub fn serve_blocking_fanout(
    db: &Db,
    listener: TcpListener,
    opts: &ServeOpts,
    max_sinks: usize,
) -> Result<(), Error> {
    serve_fanout(db, &listener, opts, max_sinks, None)
}

/// [`serve_blocking_fanout`] that returns after `max_conns` connections were accepted
/// and handled, when given.
fn serve_fanout(
    db: &Db,
    listener: &TcpListener,
    opts: &ServeOpts,
    max_sinks: usize,
    max_conns: Option<usize>,
) -> Result<(), Error> {
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
    let workers = max_sinks.clamp(1, MAX_SINKS);
    let token = opts.token.as_deref();
    // Rendezvous channel: `send` returns once a worker took the connection, so at most
    // `workers` pulls are in flight and the accept loop itself applies the backpressure.
    let (tx, rx) = std::sync::mpsc::sync_channel::<TcpStream>(0);
    let rx = std::sync::Mutex::new(rx);
    std::thread::scope(|scope| {
        for _ in 0..workers {
            let tls = tls.clone();
            let rx = &rx;
            scope.spawn(move || {
                loop {
                    // The guard is dropped before the connection is handled.
                    let next = match rx.lock() {
                        Ok(guard) => guard.recv(),
                        Err(_) => return,
                    };
                    let Ok(stream) = next else {
                        return;
                    };
                    if let Err(e) = handle_conn(db, stream, token, tls.clone()) {
                        eprintln!("wal ship: client error: {e}");
                    }
                }
            });
        }
        let mut accepted = 0usize;
        let result = loop {
            if max_conns.is_some_and(|max| accepted >= max) {
                break Ok(());
            }
            match listener.accept() {
                Ok((stream, _)) => {
                    accepted += 1;
                    if tx.send(stream).is_err() {
                        break Err(Error::runtime("wal ship: no serving thread left"));
                    }
                }
                Err(e) => break Err(io_err(e)),
            }
        };
        // Closing the channel lets the workers finish their current pull and exit.
        drop(tx);
        result
    })
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

/// Serve WAL pulls directly from durable `dir` **without holding the writer lock**, so a
/// primary [`Db`] or [`crate::server::LinServer`] can write concurrently while up to
/// `opts.max_sinks` followers pull WAL frames with push long-polling and lag bounds.
pub fn serve_dir_live(
    dir: impl AsRef<Path>,
    listener: TcpListener,
    opts: &LiveServeOpts,
) -> Result<(), Error> {
    serve_dir_live_limit(dir, &listener, opts, None)
}

/// Bounded variant of [`serve_dir_live`] returning after `max_conns` accepted connections.
pub fn serve_dir_live_limit(
    dir: impl AsRef<Path>,
    listener: &TcpListener,
    opts: &LiveServeOpts,
    max_conns: Option<usize>,
) -> Result<(), Error> {
    let dir = dir.as_ref();
    crate::persist::validate_data_dir(dir)?;
    if !dir.exists()
        || (!crate::persist::head_path(dir).exists()
            && !crate::persist::snapshot_path(dir).exists()
            && !crate::persist::log_path(dir).exists())
    {
        return Err(Error::runtime(
            "wal ship: serve_dir_live requires an initialized durable data directory",
        ));
    }
    listener.set_nonblocking(false).map_err(io_err)?;
    let tls = match &opts.serve.tls {
        Some(t) => Some(server_config(t)?),
        None => None,
    };
    let has_push_hub = opts.hub.is_some();
    let hub = match &opts.hub {
        Some(h) => {
            if let Some(m) = opts.max_lag_gens {
                h.set_max_lag_gens(Some(m));
            }
            Arc::clone(h)
        }
        None => {
            let h = ShipWalHub::new();
            if let Some(m) = opts.max_lag_gens {
                h.set_max_lag_gens(Some(m));
            }
            h
        }
    };
    let workers = opts.max_sinks.clamp(1, MAX_SINKS);
    let token = opts.serve.token.as_deref();
    let (tx, rx) = std::sync::mpsc::sync_channel::<TcpStream>(0);
    let rx = std::sync::Mutex::new(rx);
    std::thread::scope(|scope| {
        for _ in 0..workers {
            let tls = tls.clone();
            let hub = Arc::clone(&hub);
            let rx = &rx;
            scope.spawn(move || {
                loop {
                    let next = match rx.lock() {
                        Ok(guard) => guard.recv(),
                        Err(_) => return,
                    };
                    let Ok(stream) = next else {
                        return;
                    };
                    if let Err(e) =
                        handle_dir_conn(dir, stream, token, tls.clone(), &hub, has_push_hub)
                    {
                        eprintln!("wal ship: client error: {e}");
                    }
                }
            });
        }
        let mut accepted = 0usize;
        let result = loop {
            if max_conns.is_some_and(|max| accepted >= max) {
                break Ok(());
            }
            match listener.accept() {
                Ok((stream, _)) => {
                    accepted += 1;
                    if tx.send(stream).is_err() {
                        break Err(Error::runtime("wal ship: no serving thread left"));
                    }
                }
                Err(e) => break Err(io_err(e)),
            }
        };
        drop(tx);
        result
    })
}

fn handle_dir_conn(
    dir: &Path,
    stream: TcpStream,
    expect_token: Option<&str>,
    tls: Option<Arc<ServerConfig>>,
    hub: &ShipWalHub,
    has_push_hub: bool,
) -> Result<(), Error> {
    let mut net = accept_net_stream(stream, tls, Duration::from_secs(30))?;
    let hello = read_hello_any(&mut net)?;
    let want = expect_token.unwrap_or("");
    if !token_eq(&hello.token, want) {
        return Err(Error::runtime("wal ship: bad token"));
    }
    if hello.wait_ms > 0 {
        net.set_timeout(Some(
            Duration::from_secs(30) + Duration::from_millis(u64::from(hello.wait_ms)),
        ))?;
    }
    let deadline = std::time::Instant::now() + Duration::from_millis(u64::from(hello.wait_ms));
    let mut max_lag = hub.max_lag_gens.load(Ordering::Acquire);

    let (mut hg, mut fwg, mut pg) = crate::persist::peek_wal_bounds(dir)?;
    let mut last_seen_gen = pg.max(hello.since);
    let mut last_disk_check = std::time::Instant::now();
    let wait_slice = if has_push_hub {
        Duration::from_millis(50)
    } else {
        Duration::from_millis(15)
    };

    let (head_gen, first_wal_gen, primary_gen) = loop {
        let gap = hello.since < hg && fwg.is_none_or(|fg| fg > hello.since.saturating_add(1));
        let lag_over = max_lag > 0 && pg.saturating_sub(hello.since) > max_lag;
        if pg > hello.since || gap || lag_over || hello.wait_ms == 0 {
            break (hg, fwg, pg);
        }
        let now = std::time::Instant::now();
        if now >= deadline {
            break (hg, fwg, pg);
        }
        let slice = (deadline - now).min(wait_slice);
        let mut hub_advanced = false;
        if let Ok(guard) = hub.gen_state.lock() {
            if *guard > last_seen_gen {
                last_seen_gen = *guard;
                hub_advanced = true;
            } else if let Ok((g, _)) = hub.gen_cv.wait_timeout(guard, slice)
                && *g > last_seen_gen
            {
                last_seen_gen = *g;
                hub_advanced = true;
            }
        } else {
            std::thread::sleep(slice);
        }
        max_lag = hub.max_lag_gens.load(Ordering::Acquire);
        let after = std::time::Instant::now();
        if !has_push_hub
            || hub_advanced
            || after >= deadline
            || last_disk_check.elapsed() >= Duration::from_millis(200)
        {
            (hg, fwg, pg) = crate::persist::peek_wal_bounds(dir)?;
            last_seen_gen = last_seen_gen.max(pg);
            last_disk_check = after;
        }
    };

    let compacted_gap =
        hello.since < head_gen && first_wal_gen.is_none_or(|fg| fg > hello.since.saturating_add(1));
    let lag_exceeded =
        compacted_gap || (max_lag > 0 && primary_gen.saturating_sub(hello.since) > max_lag);
    hub.record_pull(
        hello.follower_id.as_deref(),
        hello.since,
        primary_gen,
        lag_exceeded,
    );

    if lag_exceeded {
        if hello.v7 {
            write_v7_response(&mut net, ShipPullStatus::LagExceeded, primary_gen, &[])?;
            return Ok(());
        }
        return Err(Error::runtime(format!(
            "wal ship: replica lag {} (since={}, primary={primary_gen}) exceeds max_lag_gens or compacted WAL",
            primary_gen.saturating_sub(hello.since),
            hello.since
        )));
    }

    let frames = if primary_gen <= hello.since {
        Vec::new()
    } else {
        crate::persist::export_wal_since(dir, hello.since)?
    };
    if frames.len() as u64 > MAX_PULL_BYTES {
        return Err(Error::runtime(format!(
            "wal ship: export {} exceeds max {MAX_PULL_BYTES}",
            frames.len()
        )));
    }
    if hello.v7 {
        write_v7_response(&mut net, ShipPullStatus::Ok, primary_gen, &frames)
    } else {
        write_v6_response(&mut net, &frames)
    }
}

fn handle_conn(
    db: &Db,
    stream: TcpStream,
    expect_token: Option<&str>,
    tls: Option<Arc<ServerConfig>>,
) -> Result<(), Error> {
    let net = accept_net_stream(stream, tls, Duration::from_secs(30))?;
    handle_io(db, net, expect_token)
}

fn handle_io<S: Read + Write>(
    db: &Db,
    mut stream: S,
    expect_token: Option<&str>,
) -> Result<(), Error> {
    let hello = read_hello_any(&mut stream)?;
    let want = expect_token.unwrap_or("");
    if !token_eq(&hello.token, want) {
        return Err(Error::runtime("wal ship: bad token"));
    }
    let frames = db.export_wal_since(hello.since)?;
    if frames.len() as u64 > MAX_PULL_BYTES {
        return Err(Error::runtime(format!(
            "wal ship: export {} exceeds max {MAX_PULL_BYTES}",
            frames.len()
        )));
    }
    if hello.v7 {
        write_v7_response(&mut stream, ShipPullStatus::Ok, db.r#gen(), &frames)
    } else {
        write_v6_response(&mut stream, &frames)
    }
}

fn write_v7_response<S: Write>(
    stream: &mut S,
    status: ShipPullStatus,
    primary_gen: u64,
    frames: &[u8],
) -> Result<(), Error> {
    let mut hdr = [0u8; 17];
    hdr[0] = match status {
        ShipPullStatus::Ok => 0,
        ShipPullStatus::LagExceeded => 1,
    };
    hdr[1..9].copy_from_slice(&primary_gen.to_le_bytes());
    hdr[9..17].copy_from_slice(&(frames.len() as u64).to_le_bytes());
    if frames.is_empty() {
        stream.write_all(&hdr).map_err(io_err)?;
    } else {
        let mut head_done = 0usize;
        let mut body_done = 0usize;
        while head_done < 17 || body_done < frames.len() {
            let n = if head_done < 17 {
                let slices = [
                    std::io::IoSlice::new(&hdr[head_done..]),
                    std::io::IoSlice::new(&frames[body_done..]),
                ];
                stream.write_vectored(&slices).map_err(io_err)?
            } else {
                stream.write(&frames[body_done..]).map_err(io_err)?
            };
            if n == 0 {
                return Err(Error::runtime("wal ship: write zero"));
            }
            let head_rem = 17 - head_done;
            if n <= head_rem {
                head_done += n;
            } else {
                head_done = 17;
                body_done += n - head_rem;
            }
        }
    }
    stream.flush().map_err(io_err)?;
    Ok(())
}

fn write_v6_response<S: Write>(stream: &mut S, frames: &[u8]) -> Result<(), Error> {
    let len_bytes = (frames.len() as u64).to_le_bytes();
    if frames.is_empty() {
        stream.write_all(&len_bytes).map_err(io_err)?;
    } else {
        let mut head_done = 0usize;
        let mut body_done = 0usize;
        while head_done < 8 || body_done < frames.len() {
            let n = if head_done < 8 {
                let slices = [
                    std::io::IoSlice::new(&len_bytes[head_done..]),
                    std::io::IoSlice::new(&frames[body_done..]),
                ];
                stream.write_vectored(&slices).map_err(io_err)?
            } else {
                stream.write(&frames[body_done..]).map_err(io_err)?
            };
            if n == 0 {
                return Err(Error::runtime("wal ship: write zero"));
            }
            let head_rem = 8 - head_done;
            if n <= head_rem {
                head_done += n;
            } else {
                head_done = 8;
                body_done += n - head_rem;
            }
        }
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

    /// A client that connects and never sends its request must not hold up the other
    /// followers: with one sink they would wait out its 30-second timeout.
    #[test]
    fn fanout_serves_followers_past_a_stalled_one() {
        let dir = std::env::temp_dir().join(format!(
            "lin-ship-fanout-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let mut db = Db::open(&dir).unwrap();
        for i in 0..3 {
            db.run(&format!(
                r#"insert docs {{ uri: "raw://fan/{i}", title: "Fan {i}", layer: "wiki" }}"#
            ))
            .unwrap();
        }

        // What a follower at each position must receive.
        let expected: Vec<Vec<u8>> = (0..2).map(|since| db.export_wal_since(since).unwrap()).collect();
        assert!(!expected[0].is_empty());
        let expected = Arc::new(expected);

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr: SocketAddr = listener.local_addr().unwrap();
        const FOLLOWERS: usize = 6;
        let server = thread::spawn(move || {
            // The stalled connection plus every follower.
            serve_fanout(&db, &listener, &ServeOpts::default(), 3, Some(1 + FOLLOWERS)).unwrap();
            let _ = db.close();
        });

        // Occupies one of the three slots until it is dropped.
        let stalled = TcpStream::connect(addr).unwrap();
        thread::sleep(Duration::from_millis(50));

        let started = std::time::Instant::now();
        let pulls: Vec<_> = (0..FOLLOWERS)
            .map(|i| {
                let expected = Arc::clone(&expected);
                thread::spawn(move || {
                    // Followers at different positions get different tails.
                    let since = (i % 2) as u64;
                    let frames = pull(addr, since).unwrap();
                    assert_eq!(frames, expected[since as usize], "since {since}");
                    if since == 0 {
                        let mut mem = Db::empty();
                        assert_eq!(mem.apply_wal(&frames).unwrap(), 3);
                    }
                })
            })
            .collect();
        for p in pulls {
            p.join().unwrap();
        }
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "followers waited for the stalled client: {:?}",
            started.elapsed()
        );

        drop(stalled);
        server.join().unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Clients that send garbage, an oversized token length, their request one byte at
    /// a time, half-close their side, or vanish mid-connection are each handled without
    /// disturbing the others, and a normal pull still gets the right frames.
    #[test]
    fn fanout_survives_hostile_and_sloppy_clients() {
        use std::io::{Read, Write};
        use std::net::Shutdown;
        let dir = std::env::temp_dir().join(format!(
            "lin-ship-hostile-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut db = Db::open(&dir).unwrap();
        for i in 0..3 {
            db.run(&format!(
                r#"insert docs {{ uri: "raw://hostile/{i}", title: "H {i}", layer: "wiki" }}"#
            ))
            .unwrap();
        }
        let expected = db.export_wal_since(0).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr: SocketAddr = listener.local_addr().unwrap();
        const CLIENTS: usize = 7;
        let server = thread::spawn(move || {
            serve_fanout(&db, &listener, &ServeOpts::default(), 4, Some(CLIENTS)).unwrap();
            let _ = db.close();
        });

        let hello = {
            let mut h = Vec::new();
            write_hello(&mut h, "", 0).unwrap();
            h
        };
        // 1. Garbage instead of the magic.
        let mut garbage = TcpStream::connect(addr).unwrap();
        garbage.write_all(b"GET / HTTP/1.1\r\n\r\n").unwrap();
        // 2. A token length far past the limit.
        let mut oversized = TcpStream::connect(addr).unwrap();
        oversized.write_all(&PROTO_MAGIC).unwrap();
        oversized.write_all(&u16::MAX.to_le_bytes()).unwrap();
        // 3. Connect and vanish.
        drop(TcpStream::connect(addr).unwrap());
        // 4. The request one byte at a time.
        let slow = thread::spawn({
            let hello = hello.clone();
            move || {
                let mut s = TcpStream::connect(addr).unwrap();
                for b in &hello {
                    s.write_all(&[*b]).unwrap();
                    thread::sleep(Duration::from_millis(2));
                }
                let mut len = [0u8; 8];
                s.read_exact(&mut len).unwrap();
                let mut body = vec![0u8; u64::from_le_bytes(len) as usize];
                s.read_exact(&mut body).unwrap();
                body
            }
        });
        // 5. Request, then half-close the sending side.
        let half = thread::spawn({
            let hello = hello.clone();
            move || {
                let mut s = TcpStream::connect(addr).unwrap();
                s.write_all(&hello).unwrap();
                s.shutdown(Shutdown::Write).unwrap();
                let mut out = Vec::new();
                s.read_to_end(&mut out).unwrap();
                out
            }
        });
        // 6. Request, then reset before reading the answer.
        {
            let mut s = TcpStream::connect(addr).unwrap();
            s.write_all(&hello).unwrap();
        }
        // 7. A normal pull.
        let frames = pull(addr, 0).unwrap();
        assert_eq!(frames, expected);
        assert_eq!(slow.join().unwrap(), expected);
        let half_out = half.join().unwrap();
        assert_eq!(&half_out[8..], &expected[..]);
        drop(garbage);
        drop(oversized);
        server.join().unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A server that announces a huge payload and then closes is reported as a truncated
    /// pull, not trusted for its announced size.
    #[test]
    fn pull_rejects_a_server_that_lies_about_the_payload_size() {
        use std::io::Write;
        for announced in [MAX_PULL_BYTES, 1024u64, MAX_PULL_BYTES + 1] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let addr: SocketAddr = listener.local_addr().unwrap();
            let server = thread::spawn(move || {
                let (mut s, _) = listener.accept().unwrap();
                let _ = read_hello_any(&mut s);
                s.write_all(&announced.to_le_bytes()).unwrap();
                s.write_all(b"only a few bytes").unwrap();
                // Closing here ends the payload long before the announced size.
            });
            let started = std::time::Instant::now();
            let err = pull(addr, 0).err().expect("a short payload must fail");
            assert!(
                err.to_string().contains("truncated") || err.to_string().contains("exceeds"),
                "{announced}: {err}"
            );
            assert!(started.elapsed() < Duration::from_secs(10));
            server.join().unwrap();
        }
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
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(err.to_string().contains("wal ship"), "{err}");
        let _ = serve.join();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn tls_server_name_only_comes_from_a_dns_host() {
        assert_eq!(
            tls_server_name("db.example.com:7001").as_deref(),
            Some("db.example.com")
        );
        assert_eq!(
            tls_server_name("localhost:7001").as_deref(),
            Some("localhost")
        );
        assert_eq!(tls_server_name("127.0.0.1:7001"), None);
        assert_eq!(tls_server_name("[::1]:7001"), None);
        assert_eq!(tls_server_name(":7001"), None);
    }

    /// A peer reached by DNS name must verify under that name: `server_name`
    /// replaces the loopback default, so a cert issued for `ship.internal`
    /// works over a numeric socket.
    #[test]
    fn pull_verifies_a_named_server_certificate() {
        let dir = std::env::temp_dir().join(format!(
            "lin-ship-name-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut db = Db::open(&dir).unwrap();
        db.run(r#"insert docs { uri: "raw://named", title: "N", layer: "wiki" }"#)
            .unwrap();

        let cert = rcgen::generate_simple_self_signed(["ship.internal".into()]).unwrap();
        let tls = TlsServer {
            cert_pem: cert.cert.pem().into_bytes(),
            key_pem: cert.key_pair.serialize_pem().into_bytes(),
        };
        let ca = tls.cert_pem.clone();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let opts = ServeOpts {
            token: None,
            tls: Some(tls),
        };
        let serve = thread::spawn(move || {
            let r = serve_one_opts(&db, &listener, &opts);
            let _ = db.close();
            r
        });
        thread::sleep(Duration::from_millis(40));

        let frames = pull_with(
            addr,
            0,
            &PullOpts {
                tls: TlsClient::CaPem(ca),
                server_name: Some("ship.internal".into()),
                ..Default::default()
            },
        )
        .expect("named server verifies under its own name");
        assert!(!frames.is_empty());
        let _ = serve.join().unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn serve_dir_live_concurrent_writer_push_wakeup_and_lag_bound() {
        let dir = std::env::temp_dir().join(format!(
            "lin-ship-live-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        // Keep primary Db open (holding writer lock!) while serve_dir_live_limit serves from `dir`.
        let mut primary = Db::open(&dir).unwrap();
        primary
            .run(r#"insert docs { uri: "raw://live-1", title: "Live 1", layer: "wiki" }"#)
            .unwrap();
        let g1 = primary.r#gen();

        let hub = ShipWalHub::with_max_lag_gens(2);
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let dir_srv = dir.clone();
        let hub_srv = Arc::clone(&hub);
        let srv = thread::spawn(move || {
            let live_opts = LiveServeOpts {
                serve: ServeOpts::default(),
                max_sinks: 4,
                max_lag_gens: Some(2),
                hub: Some(hub_srv),
            };
            serve_dir_live_limit(&dir_srv, &listener, &live_opts, Some(3))
        });

        // 1. Initial pull from since=0 (lag=1 <= max_lag=2): succeeds immediately.
        let r1 = pull_wait(addr, 0, 0, Some("f-alpha"), &PullOpts::default()).unwrap();
        assert_eq!(r1.status, ShipPullStatus::Ok);
        assert_eq!(r1.primary_gen, g1);
        assert!(!r1.frames.is_empty());
        let mut follower = Db::empty();
        follower.apply_wal(&r1.frames).unwrap();
        assert_eq!(follower.r#gen(), g1);

        // 2. Long-poll wait (`wait_ms = 2500`) wakes quickly when primary commits + hub notifies.
        let hub_writer = Arc::clone(&hub);
        let waiter = thread::spawn(move || {
            let started = std::time::Instant::now();
            let res = pull_wait(addr, g1, 2500, Some("f-alpha"), &PullOpts::default()).unwrap();
            (res, started.elapsed())
        });
        thread::sleep(Duration::from_millis(25));
        primary
            .run(r#"insert docs { uri: "raw://live-2", title: "Live 2", layer: "wiki" }"#)
            .unwrap();
        let g2 = primary.r#gen();
        hub_writer.notify_commit(g2);

        let (r2, elapsed) = waiter.join().unwrap();
        assert_eq!(r2.status, ShipPullStatus::Ok);
        assert_eq!(r2.primary_gen, g2);
        assert!(!r2.frames.is_empty());
        assert!(
            elapsed < Duration::from_millis(800),
            "push wakeup took too long: {elapsed:?}"
        );
        follower.apply_wal(&r2.frames).unwrap();
        assert_eq!(follower.r#gen(), g2);

        // 3. Advance primary to gen 4 so a stale follower at since=0 has lag 4 > max_lag=2.
        primary
            .run(r#"insert docs { uri: "raw://live-3", title: "Live 3", layer: "wiki" }"#)
            .unwrap();
        primary
            .run(r#"insert docs { uri: "raw://live-4", title: "Live 4", layer: "wiki" }"#)
            .unwrap();
        let g4 = primary.r#gen();
        hub.notify_commit(g4);

        let r3 = pull_wait(addr, 0, 0, Some("f-stale"), &PullOpts::default()).unwrap();
        assert_eq!(r3.status, ShipPullStatus::LagExceeded);
        assert_eq!(r3.primary_gen, g4);
        assert!(r3.frames.is_empty());

        srv.join().unwrap().unwrap();
        let statuses = hub.replica_statuses();
        assert_eq!(statuses.len(), 2);
        let stale = statuses.iter().find(|s| s.follower_id == "f-stale").unwrap();
        assert!(stale.lag_exceeded);
        assert_eq!(stale.lag_gens, g4);

        primary.close().unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }
}
