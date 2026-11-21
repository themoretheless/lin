//! Networked Lin server, primary/follower replication, group commit, consistency routing,
//! workload-group governance, and keep-alive `LINS\x01` + `LCRC` binary RPC client/pool.

use std::collections::BTreeMap;
use std::fmt;
use std::io::{ErrorKind, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, RwLock, mpsc};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use rustls::ServerConfig as RustlsServerConfig;

use crate::ast::Stmt;
use crate::batch::{RecordBatch, read_slice, read_u8, read_u32_le, read_u64_le};
use crate::crc::Hasher as CrcHasher;
use crate::error::Error;
use crate::exec::{Db, ReadDb, eval_partial_summarize_query_on_reader};
use crate::ship::{
    NetStream, PullOpts, TlsClient, TlsServer, accept_net_stream, build_server_tls,
    connect_net_stream, token_eq,
};

const WIRE_MAGIC: &[u8; 5] = b"LINS\x01";
const WIRE_CRC_TAG: &[u8; 4] = b"LCRC";
const MAX_FRAME_BYTES: usize = 256 * 1024 * 1024;
/// Largest frame accepted before a connection has authenticated: enough for the hello,
/// far below what an anonymous peer could otherwise make the server allocate.
const PREAUTH_MAX_FRAME_BYTES: usize = 64 * 1024;
const IDLE_POLL_SLICE: Duration = Duration::from_millis(80);
const MAX_POOLED_CONNS: usize = 16;

const OP_HELLO: u8 = 0x01;
const OP_EXECUTE: u8 = 0x02;
const OP_INSERT_BATCH: u8 = 0x03;
const OP_PARTIAL_SUMMARIZE: u8 = 0x04;
const OP_PULL_SYNC: u8 = 0x05;
const OP_PING: u8 = 0x06;

const ERR_RUNTIME: u8 = 0;
const ERR_UNAUTHORIZED: u8 = 1;
const ERR_STALE_REPLICA: u8 = 2;
const ERR_READ_ONLY_FOLLOWER: u8 = 3;
const ERR_WORKLOAD_LIMIT: u8 = 4;

fn io_err(e: impl fmt::Display) -> Error {
    Error::runtime(format!("lin server io: {e}"))
}

fn wire_err(code: u8, msg: &str) -> Error {
    match code {
        ERR_UNAUTHORIZED => Error::runtime(format!("unauthorized: {msg}")),
        ERR_STALE_REPLICA => Error::runtime(format!("stale_replica: {msg}")),
        ERR_READ_ONLY_FOLLOWER => Error::runtime(format!("read_only_follower: {msg}")),
        ERR_WORKLOAD_LIMIT => Error::runtime(format!("workload_limit: {msg}")),
        _ => Error::runtime(msg),
    }
}

/// Server role for a running [`LinServer`] instance.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum NodeRole {
    #[default]
    Primary,
    Follower,
}

/// Read-your-writes consistency guarantee for client queries.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Consistency {
    /// Any live follower (lowest latency, may lag behind primary).
    Weak,
    /// Read at least the caller's latest observed write/read generation.
    #[default]
    Session,
    /// Require server generation >= the supplied minimum generation.
    AtLeastGen(u64),
    /// Always route to primary and guarantee latest committed generation.
    Strong,
}

/// Per-tenant / per-query resource governance policy enforced on the server.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkloadGroup {
    pub name: String,
    pub max_result_bytes: usize,
    pub max_scan_rows: Option<usize>,
    pub query_timeout: Option<Duration>,
}

impl Default for WorkloadGroup {
    fn default() -> Self {
        Self {
            name: "default".to_string(),
            max_result_bytes: 64 * 1024 * 1024,
            max_scan_rows: None,
            query_timeout: None,
        }
    }
}

/// Client connection configuration for [`ServerClient`] and [`ReplicaPoolClient`].
#[derive(Clone)]
pub struct ClientOpts {
    pub token: Option<String>,
    pub tls: TlsClient,
    pub server_name: Option<String>,
    pub consistency: Consistency,
    pub workload_group: Option<String>,
    pub timeout: Duration,
}

impl Default for ClientOpts {
    fn default() -> Self {
        Self {
            token: None,
            tls: TlsClient::Off,
            server_name: None,
            consistency: Consistency::Session,
            workload_group: None,
            timeout: Duration::from_secs(30),
        }
    }
}

impl fmt::Debug for ClientOpts {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ClientOpts")
            .field("token", &self.token.as_ref().map(|_| "[REDACTED]"))
            .field("tls", &self.tls)
            .field("server_name", &self.server_name)
            .field("consistency", &self.consistency)
            .field("workload_group", &self.workload_group)
            .field("timeout", &self.timeout)
            .finish()
    }
}

impl ClientOpts {
    fn to_pull_opts(&self) -> PullOpts {
        PullOpts {
            token: self.token.clone(),
            tls: self.tls.clone(),
            server_name: self.server_name.clone(),
        }
    }
}

/// Replication settings for a [`NodeRole::Follower`] server.
#[derive(Clone, Debug)]
pub struct FollowerSyncOpts {
    pub primary_addr: String,
    pub client: ClientOpts,
    pub poll_interval: Duration,
}

impl FollowerSyncOpts {
    pub fn new(primary_addr: impl Into<String>, client: ClientOpts) -> Self {
        Self {
            primary_addr: primary_addr.into(),
            client,
            poll_interval: Duration::from_millis(15),
        }
    }
}

/// Configuration for starting a [`LinServer`].
#[derive(Clone)]
pub struct ServerConfig {
    pub bind_addr: String,
    pub role: NodeRole,
    pub follower: Option<FollowerSyncOpts>,
    pub token: Option<String>,
    pub tls: Option<TlsServer>,
    pub group_commit_window: Duration,
    pub max_group_size: usize,
    pub max_group_bytes: usize,
    pub consistency_wait_timeout: Duration,
    pub workload_groups: BTreeMap<String, WorkloadGroup>,
    pub default_workload_group: WorkloadGroup,
    pub io_timeout: Duration,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind_addr: "127.0.0.1:0".to_string(),
            role: NodeRole::Primary,
            follower: None,
            token: None,
            tls: None,
            group_commit_window: Duration::from_millis(2),
            max_group_size: 64,
            max_group_bytes: 4 * 1024 * 1024,
            consistency_wait_timeout: Duration::from_millis(500),
            workload_groups: BTreeMap::new(),
            default_workload_group: WorkloadGroup::default(),
            io_timeout: Duration::from_secs(30),
        }
    }
}

impl fmt::Debug for ServerConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ServerConfig")
            .field("bind_addr", &self.bind_addr)
            .field("role", &self.role)
            .field("follower", &self.follower)
            .field("token", &self.token.as_ref().map(|_| "[REDACTED]"))
            .field("tls", &self.tls)
            .field("group_commit_window", &self.group_commit_window)
            .field("max_group_size", &self.max_group_size)
            .field("max_group_bytes", &self.max_group_bytes)
            .field("consistency_wait_timeout", &self.consistency_wait_timeout)
            .field("workload_groups", &self.workload_groups)
            .field("default_workload_group", &self.default_workload_group)
            .field("io_timeout", &self.io_timeout)
            .finish()
    }
}

/// Receipt returned from RPC statement execution or batch ingestion.
#[derive(Clone, Debug, PartialEq)]
pub struct ExecutionReceipt {
    pub r#gen: u64,
    pub affected: usize,
    pub elapsed_us: u64,
    pub group_size: usize,
    pub batch: Option<RecordBatch>,
    pub message: Option<String>,
}

/// Incremental WAL or full snapshot+cold bundle pulled by a follower.
/// Observed replication progress of a connected follower on a primary [`LinServer`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplicaStatus {
    pub follower_id: String,
    pub reported_gen: u64,
    pub lag_gens: u64,
    pub last_seen_ms: u64,
}

#[derive(Clone, Debug)]
pub enum PullSyncPayload {
    UpToDate {
        r#gen: u64,
    },
    Wal {
        r#gen: u64,
        wal_bytes: Vec<u8>,
    },
    Bootstrap {
        r#gen: u64,
        snap_bytes: Vec<u8>,
        cold_manifest_json: Vec<u8>,
        cold_tombstones: Vec<u8>,
        cold_segments: Vec<(u64, Vec<u8>)>,
    },
}

// =========================================================================
// Wire encoding & LINS\x01 + LCRC framing helpers
// =========================================================================

fn push_u32_bytes(out: &mut Vec<u8>, bytes: &[u8]) -> Result<(), Error> {
    let len = u32::try_from(bytes.len())
        .map_err(|_| Error::runtime("wire payload length exceeds u32"))?;
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(bytes);
    Ok(())
}

fn push_str(out: &mut Vec<u8>, s: &str) -> Result<(), Error> {
    push_u32_bytes(out, s.as_bytes())
}

fn push_opt_str(out: &mut Vec<u8>, s: Option<&str>) -> Result<(), Error> {
    match s {
        Some(v) => {
            out.push(1);
            push_str(out, v)?;
        }
        None => out.push(0),
    }
    Ok(())
}

fn read_len_bytes<'a>(cur: &mut &'a [u8]) -> Result<&'a [u8], Error> {
    let len = read_u32_le(cur)? as usize;
    read_slice(cur, len)
}

fn read_str_ref<'a>(cur: &mut &'a [u8]) -> Result<&'a str, Error> {
    let bytes = read_len_bytes(cur)?;
    std::str::from_utf8(bytes).map_err(|e| Error::runtime(format!("invalid utf-8: {e}")))
}

fn read_opt_str_ref<'a>(cur: &mut &'a [u8]) -> Result<Option<&'a str>, Error> {
    match read_u8(cur)? {
        0 => Ok(None),
        1 => Ok(Some(read_str_ref(cur)?)),
        other => Err(Error::runtime(format!("invalid optional string tag {other}"))),
    }
}

fn read_str(cur: &mut &[u8]) -> Result<String, Error> {
    read_str_ref(cur).map(str::to_owned)
}

fn read_opt_str(cur: &mut &[u8]) -> Result<Option<String>, Error> {
    Ok(read_opt_str_ref(cur)?.map(str::to_owned))
}

fn encode_consistency(out: &mut Vec<u8>, c: Consistency) {
    match c {
        Consistency::Weak => {
            out.push(0);
            out.extend_from_slice(&0u64.to_le_bytes());
        }
        Consistency::Session => {
            out.push(1);
            out.extend_from_slice(&0u64.to_le_bytes());
        }
        Consistency::AtLeastGen(g) => {
            out.push(2);
            out.extend_from_slice(&g.to_le_bytes());
        }
        Consistency::Strong => {
            out.push(3);
            out.extend_from_slice(&0u64.to_le_bytes());
        }
    }
}

fn decode_consistency(cur: &mut &[u8]) -> Result<Consistency, Error> {
    let tag = read_u8(cur)?;
    let arg = read_u64_le(cur)?;
    match tag {
        0 => Ok(Consistency::Weak),
        1 => Ok(Consistency::Session),
        2 => Ok(Consistency::AtLeastGen(arg)),
        3 => Ok(Consistency::Strong),
        _ => Err(Error::runtime(format!("unknown consistency tag {tag}"))),
    }
}

const RETAIN_BUF_CAP: usize = 4 * 1024 * 1024;
const SHRINK_BUF_TO: usize = 64 * 1024;

#[inline]
fn begin_frame_buf(write_buf: &mut Vec<u8>, op: u8) {
    write_buf.clear();
    let mut hdr = [0u8; 10];
    hdr[..5].copy_from_slice(WIRE_MAGIC);
    hdr[5] = op;
    write_buf.extend_from_slice(&hdr);
}

fn write_err_into_frame(write_buf: &mut Vec<u8>, op: u8, code: u8, msg: &str) -> Result<(), Error> {
    begin_frame_buf(write_buf, op);
    write_buf.push(1);
    write_buf.push(code);
    push_str(write_buf, msg)
}

fn finish_and_write_frame(stream: &mut NetStream, write_buf: &mut Vec<u8>) -> Result<(), Error> {
    debug_assert!(write_buf.len() >= 10);
    let payload_len = write_buf.len() - 10;
    if payload_len > MAX_FRAME_BYTES {
        return Err(Error::runtime(format!(
            "frame payload {payload_len} exceeds max {MAX_FRAME_BYTES}"
        )));
    }
    let len_u32 = u32::try_from(payload_len)
        .map_err(|_| Error::runtime("frame payload length exceeds u32"))?;
    write_buf[6..10].copy_from_slice(&len_u32.to_le_bytes());

    let mut hasher = CrcHasher::new();
    hasher.update(write_buf);
    let crc = hasher.finalize();

    write_buf.extend_from_slice(WIRE_CRC_TAG);
    write_buf.extend_from_slice(&crc.to_le_bytes());

    stream.write_all(write_buf).map_err(io_err)?;
    stream.flush().map_err(io_err)?;

    if write_buf.capacity() > RETAIN_BUF_CAP && write_buf.len() < SHRINK_BUF_TO {
        write_buf.clear();
        write_buf.shrink_to(SHRINK_BUF_TO);
    }
    Ok(())
}

enum ReadFrameStatus {
    Frame(u8),
    Eof,
}

fn read_frame_into(
    stream: &mut NetStream,
    shutdown: Option<&AtomicBool>,
    io_timeout: Duration,
    max_frame_bytes: usize,
    read_buf: &mut Vec<u8>,
) -> Result<ReadFrameStatus, Error> {
    if read_buf.capacity() > RETAIN_BUF_CAP {
        read_buf.clear();
        read_buf.shrink_to(SHRINK_BUF_TO);
    }
    let mut hdr = [0u8; 10];
    let mut read_hdr = 0usize;
    let idle_slice = IDLE_POLL_SLICE.min(io_timeout);

    while read_hdr < 10 {
        if shutdown.is_some() && read_hdr == 0 {
            let _ = stream.set_read_timeout(Some(idle_slice));
        }
        match stream.read(&mut hdr[read_hdr..]) {
            Ok(0) => {
                if read_hdr == 0 {
                    return Ok(ReadFrameStatus::Eof);
                }
                return Err(io_err("unexpected EOF in frame header"));
            }
            Ok(n) => {
                if read_hdr == 0 && shutdown.is_some() {
                    let _ = stream.set_read_timeout(Some(io_timeout));
                }
                read_hdr += n;
            }
            Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                if let Some(flag) = shutdown {
                    if flag.load(Ordering::Acquire) {
                        return Ok(ReadFrameStatus::Eof);
                    }
                    if read_hdr == 0 {
                        continue;
                    }
                }
                return Err(io_err(e));
            }
            Err(e) if e.kind() == ErrorKind::Interrupted => continue,
            Err(e) if e.kind() == ErrorKind::ConnectionReset && read_hdr == 0 => {
                return Ok(ReadFrameStatus::Eof);
            }
            Err(e) => return Err(io_err(e)),
        }
    }

    if &hdr[..5] != WIRE_MAGIC {
        return Err(Error::runtime("bad LINS frame magic"));
    }
    let op = hdr[5];
    let len = u32::from_le_bytes(hdr[6..10].try_into().unwrap()) as usize;
    if len > max_frame_bytes {
        return Err(Error::runtime(format!(
            "frame payload {len} exceeds max {max_frame_bytes}"
        )));
    }

    let wire_tail_len = len + 8;
    if wire_tail_len <= (1 << 20) || read_buf.capacity() >= wire_tail_len {
        read_buf.resize(wire_tail_len, 0);
        stream.read_exact(&mut read_buf[..wire_tail_len]).map_err(io_err)?;
    } else {
        read_buf.clear();
        read_buf.reserve(1 << 20);
        Read::take(&mut *stream, wire_tail_len as u64)
            .read_to_end(read_buf)
            .map_err(io_err)?;
        if read_buf.len() != wire_tail_len {
            return Err(io_err("unexpected EOF in frame payload"));
        }
    }

    if &read_buf[len..len + 4] != WIRE_CRC_TAG {
        return Err(Error::runtime("missing LCRC frame trailer"));
    }
    let expected_crc = u32::from_le_bytes(read_buf[len + 4..len + 8].try_into().unwrap());
    read_buf.truncate(len);

    let mut hasher = CrcHasher::new();
    hasher.update(&hdr);
    hasher.update(read_buf);
    let actual_crc = hasher.finalize();
    if actual_crc != expected_crc {
        return Err(Error::runtime(format!(
            "LCRC mismatch: got {actual_crc:#x}, expected {expected_crc:#x}"
        )));
    }

    Ok(ReadFrameStatus::Frame(op))
}

// =========================================================================
// Server implementation & group commit engine
// =========================================================================

enum WriteOp {
    Linql {
        src: String,
        stmts: Vec<Stmt>,
    },
    InsertBatch {
        collection: String,
        batch: RecordBatch,
    },
    Shutdown,
}

struct WriteAck {
    r#gen: u64,
    affected: usize,
    group_size: usize,
}

struct WriteReq {
    op: WriteOp,
    approx_bytes: usize,
    resp: mpsc::Sender<Result<WriteAck, Error>>,
}

struct ServerState {
    db: Mutex<Option<Db>>,
    cached_reader: RwLock<Option<Arc<ReadDb>>>,
    cold_unhydrated: AtomicBool,
    current_gen: AtomicU64,
    gen_state: Mutex<u64>,
    gen_cv: Condvar,
    shutdown: AtomicBool,
    listen_addr: String,
    replicas: Mutex<BTreeMap<String, (u64, Instant)>>,
    max_replica_lag_gens: AtomicU64,
    config: ServerConfig,
    writer_tx: Option<mpsc::Sender<WriteReq>>,
}

impl ServerState {
    fn refresh_reader_locked(&self, db: &mut Db) {
        let cur_gen = db.r#gen();
        let cold = db.has_unhydrated_cold_tables();
        // Only drop the snapshot here: rebuilding it copies the memory image, and doing
        // that under the writer mutex after every commit group taxed writes that no read
        // followed. The first read rebuilds it.
        if let Ok(mut guard) = self.cached_reader.write() {
            *guard = None;
        }
        self.cold_unhydrated.store(cold, Ordering::Release);
        self.current_gen.store(cur_gen, Ordering::Release);
        if let Ok(mut g) = self.gen_state.lock() {
            *g = cur_gen;
            self.gen_cv.notify_all();
        }
    }

    #[inline]
    fn current_gen(&self) -> u64 {
        self.current_gen.load(Ordering::Acquire)
    }

    fn ensure_consistency(&self, consistency: Consistency, deadline: Instant) -> Result<(), (u8, String)> {
        let min_gen = match consistency {
            Consistency::Weak => return Ok(()),
            Consistency::Session => 0,
            Consistency::AtLeastGen(g) => g,
            Consistency::Strong => {
                if self.config.role == NodeRole::Follower {
                    return Err((
                        ERR_STALE_REPLICA,
                        "strong consistency requires primary".to_string(),
                    ));
                }
                return Ok(());
            }
        };
        if min_gen == 0 || self.current_gen() >= min_gen {
            return Ok(());
        }
        let Ok(mut cur) = self.gen_state.lock() else {
            return Err((ERR_RUNTIME, "generation lock poisoned".to_string()));
        };
        while *cur < min_gen {
            if self.shutdown.load(Ordering::Acquire) {
                return Err((ERR_STALE_REPLICA, "server shutting down".to_string()));
            }
            let now = Instant::now();
            if now >= deadline {
                return Err((
                    ERR_STALE_REPLICA,
                    format!("replica at gen {}, requires >= {min_gen}", *cur),
                ));
            }
            let wait = (deadline - now).min(Duration::from_millis(50));
            let (guard, _) = self
                .gen_cv
                .wait_timeout(cur, wait)
                .map_err(|_| (ERR_RUNTIME, "generation condvar poisoned".to_string()))?;
            cur = guard;
        }
        Ok(())
    }

    fn resolve_workload_group<'a>(
        &'a self,
        req_name: Option<&str>,
        conn_name: Option<&str>,
    ) -> Result<&'a WorkloadGroup, (u8, String)> {
        let chosen = req_name.or(conn_name);
        match chosen {
            None | Some("default") | Some("") => Ok(&self.config.default_workload_group),
            Some(name) => self
                .config
                .workload_groups
                .get(name)
                .ok_or_else(|| (ERR_WORKLOAD_LIMIT, format!("unknown workload group {name:?}"))),
        }
    }

    fn get_hydrated_reader(&self) -> Result<Arc<ReadDb>, Error> {
        if let Some(reader) = self
            .cached_reader
            .read()
            .map_err(|_| Error::runtime("cached reader lock poisoned"))?
            .as_ref()
        {
            return Ok(Arc::clone(reader));
        }
        let mut db_guard = self
            .db
            .lock()
            .map_err(|_| Error::runtime("db lock poisoned"))?;
        let db = db_guard
            .as_mut()
            .ok_or_else(|| Error::runtime("server database closed"))?;
        let mut cached = self
            .cached_reader
            .write()
            .map_err(|_| Error::runtime("cached reader lock poisoned"))?;
        if let Some(reader) = cached.as_ref() {
            return Ok(Arc::clone(reader));
        }
        let reader = Arc::new(db.reader());
        *cached = Some(Arc::clone(&reader));
        Ok(reader)
    }

    fn get_reader_for_stmts(&self, stmts: &[Stmt]) -> Result<Arc<ReadDb>, Error> {
        if self.cold_unhydrated.load(Ordering::Acquire) {
            let mut db_guard = self
                .db
                .lock()
                .map_err(|_| Error::runtime("db lock poisoned"))?;
            let db = db_guard
                .as_mut()
                .ok_or_else(|| Error::runtime("server database closed"))?;
            let hydrated = db.hydrate_stmts_cold_tables(stmts)?;
            if hydrated {
                self.refresh_reader_locked(db);
            }
        }
        self.get_hydrated_reader()
    }
}

/// Running networked Lin server.
pub struct LinServer;

/// Lifecycle handle for a running [`LinServer`].
pub struct ServerHandle {
    addr: SocketAddr,
    state: Arc<ServerState>,
    accept_thread: Option<JoinHandle<()>>,
    writer_thread: Option<JoinHandle<()>>,
    sync_thread: Option<JoinHandle<()>>,
    client_threads: Arc<Mutex<Vec<JoinHandle<()>>>>,
}

impl LinServer {
    /// Start a networked Lin server wrapping `db` using `config`.
    pub fn start(mut db: Db, config: ServerConfig) -> Result<ServerHandle, Error> {
        if config.role == NodeRole::Follower && !db.is_follower() && config.follower.is_some() {
            // Note: follower sync uses apply_wal which accepts in-memory Db or open_follower.
            // If durable primary accidentally configured as Follower without open_follower, fail upfront cleanly.
            if db.is_durable() {
                return Err(Error::runtime(
                    "Follower role with durable storage requires Db::open_follower",
                ));
            }
        }

        let listener = TcpListener::bind(&config.bind_addr).map_err(io_err)?;
        let addr = listener.local_addr().map_err(io_err)?;
        let tls_cfg: Option<Arc<RustlsServerConfig>> = match &config.tls {
            Some(tls) => Some(build_server_tls(tls)?),
            None => None,
        };

        let initial_gen = db.r#gen();
        let cold_unhydrated = db.has_unhydrated_cold_tables();
        let initial_reader = Arc::new(db.reader());

        let (writer_tx, writer_rx) = if config.role == NodeRole::Primary {
            let (tx, rx) = mpsc::channel::<WriteReq>();
            (Some(tx), Some(rx))
        } else {
            (None, None)
        };

        let state = Arc::new(ServerState {
            db: Mutex::new(Some(db)),
            cached_reader: RwLock::new(Some(initial_reader)),
            cold_unhydrated: AtomicBool::new(cold_unhydrated),
            current_gen: AtomicU64::new(initial_gen),
            gen_state: Mutex::new(initial_gen),
            gen_cv: Condvar::new(),
            shutdown: AtomicBool::new(false),
            listen_addr: addr.to_string(),
            replicas: Mutex::new(BTreeMap::new()),
            max_replica_lag_gens: AtomicU64::new(0),
            config: config.clone(),
            writer_tx,
        });

        let writer_thread = writer_rx.map(|rx| {
            let st = Arc::clone(&state);
            thread::spawn(move || run_group_commit_writer(st, rx))
        });

        let sync_thread = if config.role == NodeRole::Follower {
            config.follower.as_ref().map(|sync_opts| {
                let st = Arc::clone(&state);
                let opts = sync_opts.clone();
                thread::spawn(move || run_follower_sync_loop(st, opts))
            })
        } else {
            None
        };

        let client_threads = Arc::new(Mutex::new(Vec::new()));
        let accept_state = Arc::clone(&state);
        let accept_clients = Arc::clone(&client_threads);
        let accept_thread = thread::spawn(move || {
            run_accept_loop(listener, tls_cfg, accept_state, accept_clients);
        });

        Ok(ServerHandle {
            addr,
            state,
            accept_thread: Some(accept_thread),
            writer_thread,
            sync_thread,
            client_threads,
        })
    }
}

impl ServerHandle {
    pub fn local_addr(&self) -> SocketAddr {
        self.addr
    }

    pub fn r#gen(&self) -> u64 {
        self.state.current_gen()
    }

    /// Wait until the server reaches at least `target_gen` or `timeout` expires.
    pub fn wait_gen(&self, target_gen: u64, timeout: Duration) -> Result<u64, Error> {
        let fast = self.state.current_gen();
        if fast >= target_gen {
            return Ok(fast);
        }
        let deadline = Instant::now() + timeout;
        let mut cur = self
            .state
            .gen_state
            .lock()
            .map_err(|_| Error::runtime("generation lock poisoned"))?;
        while *cur < target_gen {
            let now = Instant::now();
            if now >= deadline {
                return Err(Error::runtime(format!(
                    "timed out waiting for gen {target_gen}, current gen is {}",
                    *cur
                )));
            }
            let (guard, _) = self
                .state
                .gen_cv
                .wait_timeout(cur, (deadline - now).min(Duration::from_millis(50)))
                .map_err(|_| Error::runtime("generation condvar poisoned"))?;
            cur = guard;
        }
        Ok(*cur)
    }

    /// Trigger an immediate checkpoint on the hosted database.
    pub fn checkpoint(&self) -> Result<(), Error> {
        let mut db_guard = self
            .state
            .db
            .lock()
            .map_err(|_| Error::runtime("db lock poisoned"))?;
        let db = db_guard
            .as_mut()
            .ok_or_else(|| Error::runtime("server database closed"))?;
        db.checkpoint()?;
        self.state.refresh_reader_locked(db);
        Ok(())
    }

    /// Perform one synchronous pull-sync step (only meaningful on a configured follower).
    pub fn sync_once(&self) -> Result<u64, Error> {
        let opts = self
            .state
            .config
            .follower
            .as_ref()
            .ok_or_else(|| Error::runtime("sync_once requires FollowerSyncOpts"))?;
        let client = ServerClient::connect(&opts.primary_addr, opts.client.clone())?;
        let follower_id = format!("follower-{}", self.state.listen_addr);
        pull_and_apply_wait(&self.state, &client, 0, Some(&follower_id)).map(|(g, _)| g)
    }

    /// Configure the maximum WAL generation lag permitted before forcing a full snapshot +
    /// cold-segment bootstrap (`None` or `Some(0)` disables the lag cutoff).
    pub fn set_max_replica_lag_gens(&self, max_lag: Option<u64>) {
        self.state
            .max_replica_lag_gens
            .store(max_lag.unwrap_or(0), Ordering::Release);
    }

    /// Current maximum replica lag cutoff (`None` when unlimited).
    pub fn max_replica_lag_gens(&self) -> Option<u64> {
        match self.state.max_replica_lag_gens.load(Ordering::Acquire) {
            0 => None,
            n => Some(n),
        }
    }

    /// Snapshot of connected followers and their reported generation lag relative to this node.
    pub fn replica_statuses(&self) -> Vec<ReplicaStatus> {
        let cur_gen = self.state.current_gen();
        let now = Instant::now();
        let Ok(guard) = self.state.replicas.lock() else {
            return Vec::new();
        };
        guard
            .iter()
            .map(|(id, &(reported_gen, seen_at))| ReplicaStatus {
                follower_id: id.clone(),
                reported_gen,
                lag_gens: cur_gen.saturating_sub(reported_gen),
                last_seen_ms: u64::try_from(now.saturating_duration_since(seen_at).as_millis())
                    .unwrap_or(u64::MAX),
            })
            .collect()
    }

    /// Gracefully stop accepting new connections, drain group-commit writes, join threads,
    /// and return the hosted [`Db`].
    pub fn shutdown(mut self) -> Result<Db, Error> {
        self.state.shutdown.store(true, Ordering::Release);
        if let Ok(mut g) = self.state.gen_state.lock() {
            self.state.gen_cv.notify_all();
            let _ = &mut *g;
        }
        // Wake blocking TcpListener::accept().
        let _ = TcpStream::connect_timeout(&self.addr, Duration::from_millis(100));

        if let Some(tx) = &self.state.writer_tx {
            let (resp_tx, resp_rx) = mpsc::channel();
            let _ = tx.send(WriteReq {
                op: WriteOp::Shutdown,
                approx_bytes: 0,
                resp: resp_tx,
            });
            let _ = resp_rx.recv_timeout(Duration::from_secs(2));
        }

        if let Some(h) = self.accept_thread.take() {
            let _ = h.join();
        }
        if let Some(h) = self.writer_thread.take() {
            let _ = h.join();
        }
        if let Some(h) = self.sync_thread.take() {
            let _ = h.join();
        }
        if let Ok(mut guard) = self.client_threads.lock() {
            for h in guard.drain(..) {
                let _ = h.join();
            }
        }

        let mut db_guard = self
            .state
            .db
            .lock()
            .map_err(|_| Error::runtime("db lock poisoned"))?;
        db_guard
            .take()
            .ok_or_else(|| Error::runtime("database already taken during shutdown"))
    }
}

impl Drop for ServerHandle {
    fn drop(&mut self) {
        self.state.shutdown.store(true, Ordering::Release);
        if self.accept_thread.is_some() {
            let _ = TcpStream::connect_timeout(&self.addr, Duration::from_millis(50));
        }
        if let Some(tx) = &self.state.writer_tx {
            let (resp_tx, _resp_rx) = mpsc::channel();
            let _ = tx.send(WriteReq {
                op: WriteOp::Shutdown,
                approx_bytes: 0,
                resp: resp_tx,
            });
        }
        if let Some(h) = self.accept_thread.take() {
            let _ = h.join();
        }
        if let Some(h) = self.writer_thread.take() {
            let _ = h.join();
        }
        if let Some(h) = self.sync_thread.take() {
            let _ = h.join();
        }
        if let Ok(mut guard) = self.client_threads.lock() {
            for h in guard.drain(..) {
                let _ = h.join();
            }
        }
    }
}

fn run_accept_loop(
    listener: TcpListener,
    tls_cfg: Option<Arc<RustlsServerConfig>>,
    state: Arc<ServerState>,
    client_threads: Arc<Mutex<Vec<JoinHandle<()>>>>,
) {
    for incoming in listener.incoming() {
        if state.shutdown.load(Ordering::Acquire) {
            break;
        }
        let tcp = match incoming {
            Ok(s) => s,
            Err(_) => continue,
        };
        if state.shutdown.load(Ordering::Acquire) {
            break;
        }
        let _ = tcp.set_nodelay(true);
        let tls = tls_cfg.clone();
        let st = Arc::clone(&state);
        let handle = thread::spawn(move || {
            let Ok(mut stream) = accept_net_stream(tcp, tls, st.config.io_timeout) else {
                return;
            };
            handle_client_connection(&mut stream, &st);
        });
        if let Ok(mut list) = client_threads.lock() {
            list.retain(|h| !h.is_finished());
            list.push(handle);
        }
    }
}

fn run_group_commit_writer(state: Arc<ServerState>, rx: mpsc::Receiver<WriteReq>) {
    let mut pending_first: Option<WriteReq> = None;
    let max_size = state.config.max_group_size.max(1);
    let max_bytes = state.config.max_group_bytes.max(1);
    let window = state.config.group_commit_window;

    loop {
        let first = if let Some(p) = pending_first.take() {
            p
        } else {
            match rx.recv() {
                Ok(req) => req,
                Err(_) => break,
            }
        };

        if matches!(first.op, WriteOp::Shutdown) {
            let _ = first.resp.send(Ok(WriteAck {
                r#gen: state.current_gen(),
                affected: 0,
                group_size: 0,
            }));
            break;
        }

        let mut total_bytes = first.approx_bytes;
        let mut batch = vec![first];

        if max_size > 1 && total_bytes < max_bytes {
            let deadline = Instant::now() + window;
            while batch.len() < max_size && total_bytes < max_bytes {
                match rx.try_recv() {
                    Ok(next) => {
                        if matches!(next.op, WriteOp::Shutdown)
                            || total_bytes.saturating_add(next.approx_bytes) > max_bytes
                        {
                            pending_first = Some(next);
                            break;
                        }
                        total_bytes = total_bytes.saturating_add(next.approx_bytes);
                        batch.push(next);
                        continue;
                    }
                    Err(mpsc::TryRecvError::Disconnected) => break,
                    Err(mpsc::TryRecvError::Empty) => {}
                }
                let now = Instant::now();
                if now >= deadline {
                    break;
                }
                match rx.recv_timeout(deadline - now) {
                    Ok(next) => {
                        if matches!(next.op, WriteOp::Shutdown)
                            || total_bytes.saturating_add(next.approx_bytes) > max_bytes
                        {
                            pending_first = Some(next);
                            break;
                        }
                        total_bytes = total_bytes.saturating_add(next.approx_bytes);
                        batch.push(next);
                    }
                    Err(mpsc::RecvTimeoutError::Timeout | mpsc::RecvTimeoutError::Disconnected) => {
                        break;
                    }
                }
            }
        }

        let group_size = batch.len();
        let mut item_results: Vec<Result<(u64, usize), Error>> = Vec::with_capacity(group_size);
        let lock_res = state.db.lock();
        let sync_res = match lock_res {
            Ok(mut guard) => match guard.as_mut() {
                Some(db) => {
                    let sres = db.with_group_sync(group_size, |db| {
                        for req in &mut batch {
                            let res = match std::mem::replace(&mut req.op, WriteOp::Shutdown) {
                                WriteOp::Linql { src, stmts } => db
                                    .run_preparsed_ext(&src, stmts, false)
                                    .map(|h| (h.done.r#gen, h.done.n)),
                                WriteOp::InsertBatch { collection, batch } => {
                                    db.insert_batch(&collection, &batch).map(|d| (d.r#gen, d.n))
                                }
                                WriteOp::Shutdown => Ok((db.r#gen(), 0)),
                            };
                            item_results.push(res);
                        }
                    });
                    state.refresh_reader_locked(db);
                    sres
                }
                None => Err(Error::runtime("server database closed")),
            },
            Err(_) => Err(Error::runtime("db lock poisoned")),
        };

        for (idx, req) in batch.into_iter().enumerate() {
            let reply = match &sync_res {
                Err(sync_err) => Err(Error::runtime(sync_err.to_string())),
                Ok(()) => match item_results.get(idx) {
                    Some(Ok((r#gen, affected))) => Ok(WriteAck {
                        r#gen: *r#gen,
                        affected: *affected,
                        group_size,
                    }),
                    Some(Err(e)) => Err(Error::runtime(e.to_string())),
                    None => Err(Error::runtime("missing group commit result")),
                },
            };
            let _ = req.resp.send(reply);
        }
    }
}

fn run_follower_sync_loop(state: Arc<ServerState>, opts: FollowerSyncOpts) {
    let mut cached_client: Option<ServerClient> = None;
    let poll = opts.poll_interval.max(Duration::from_millis(2));
    // Fast-sync followers (`poll_interval <= 500 ms`, including the 15 ms default) use
    // server-side `gen_cv` long-polling for sub-millisecond push wakeup; callers that set
    // a multi-second `poll_interval` (e.g. to simulate replica lag in tests) keep explicit
    // sleep intervals between non-blocking pulls.
    let long_poll = poll <= Duration::from_millis(500);
    let max_wait_ms = if long_poll {
        (opts.client.timeout.as_millis() / 2).min(200).max(25) as u64
    } else {
        0
    };
    let follower_id = format!("follower-{}", state.listen_addr);
    while !state.shutdown.load(Ordering::Acquire) {
        if cached_client.is_none()
            && let Ok(c) = ServerClient::connect(&opts.primary_addr, opts.client.clone())
        {
            cached_client = Some(c);
        }
        if let Some(client) = cached_client.as_ref() {
            match pull_and_apply_wait(&state, client, max_wait_ms, Some(&follower_id)) {
                Ok((_, true)) if long_poll => continue,
                Ok((_, false)) if long_poll => continue,
                Ok(_) => {}
                Err(_) => cached_client = None,
            }
        }
        let sleep_dur = if long_poll {
            poll.min(Duration::from_millis(15))
        } else {
            poll
        };
        let mut slept = Duration::ZERO;
        while slept < sleep_dur && !state.shutdown.load(Ordering::Acquire) {
            let chunk = (sleep_dur - slept).min(Duration::from_millis(15));
            thread::sleep(chunk);
            slept += chunk;
        }
    }
}

fn pull_and_apply_wait(
    state: &Arc<ServerState>,
    primary: &ServerClient,
    wait_ms: u64,
    follower_id: Option<&str>,
) -> Result<(u64, bool), Error> {
    let cur_gen = state.current_gen();
    let payload = primary.pull_sync_wait(cur_gen, false, wait_ms, follower_id)?;
    let progressed = !matches!(payload, PullSyncPayload::UpToDate { .. });
    let new_gen = apply_sync_payload(state, primary, payload, follower_id)?;
    Ok((new_gen, progressed))
}

fn apply_sync_payload(
    state: &Arc<ServerState>,
    primary: &ServerClient,
    payload: PullSyncPayload,
    follower_id: Option<&str>,
) -> Result<u64, Error> {
    match payload {
        PullSyncPayload::UpToDate { r#gen } => Ok(r#gen),
        PullSyncPayload::Wal { wal_bytes, .. } => {
            let mut db_guard = state
                .db
                .lock()
                .map_err(|_| Error::runtime("db lock poisoned"))?;
            let db = db_guard
                .as_mut()
                .ok_or_else(|| Error::runtime("follower db closed"))?;
            match db.apply_wal(&wal_bytes) {
                Ok(_) => {
                    state.refresh_reader_locked(db);
                    Ok(db.r#gen())
                }
                Err(e) if e.to_string().contains("gap") => {
                    let cur_gen = db.r#gen();
                    drop(db_guard);
                    let boot = primary.pull_sync_wait(cur_gen, true, 0, follower_id)?;
                    apply_sync_payload(state, primary, boot, follower_id)
                }
                Err(e) => Err(e),
            }
        }
        PullSyncPayload::Bootstrap {
            snap_bytes,
            cold_manifest_json,
            cold_tombstones,
            cold_segments,
            ..
        } => {
            let mut db_guard = state
                .db
                .lock()
                .map_err(|_| Error::runtime("db lock poisoned"))?;
            let db = db_guard
                .as_mut()
                .ok_or_else(|| Error::runtime("follower db closed"))?;
            let new_gen = db.apply_bootstrap_bundle(
                &snap_bytes,
                &cold_manifest_json,
                &cold_tombstones,
                &cold_segments,
            )?;
            state.refresh_reader_locked(db);
            Ok(new_gen)
        }
    }
}

type WriteAckChan = (
    mpsc::Sender<Result<WriteAck, Error>>,
    mpsc::Receiver<Result<WriteAck, Error>>,
);

fn send_write_and_wait(
    state: &ServerState,
    op: WriteOp,
    approx_bytes: usize,
    ack_chan: &mut Option<WriteAckChan>,
) -> Result<WriteAck, (u8, String)> {
    let tx = state
        .writer_tx
        .as_ref()
        .ok_or_else(|| (ERR_RUNTIME, "writer channel unavailable".to_string()))?;
    let (resp_tx, resp_rx) = ack_chan.get_or_insert_with(mpsc::channel);
    while resp_rx.try_recv().is_ok() {}
    tx.send(WriteReq {
        op,
        approx_bytes,
        resp: resp_tx.clone(),
    })
    .map_err(|_| (ERR_RUNTIME, "writer channel closed".to_string()))?;
    resp_rx
        .recv()
        .map_err(|_| (ERR_RUNTIME, "writer dropped response".to_string()))?
        .map_err(|e| (ERR_RUNTIME, e.to_string()))
}

fn handle_client_connection(stream: &mut NetStream, state: &Arc<ServerState>) {
    let mut authed = state.config.token.is_none();
    let mut conn_wg: Option<String> = None;
    let mut read_buf = Vec::with_capacity(512);
    let mut write_buf = Vec::with_capacity(512);
    let mut ack_chan: Option<WriteAckChan> = None;

    loop {
        if state.shutdown.load(Ordering::Acquire) {
            break;
        }
        let op = match read_frame_into(
            stream,
            Some(&state.shutdown),
            state.config.io_timeout,
            if authed {
                MAX_FRAME_BYTES
            } else {
                PREAUTH_MAX_FRAME_BYTES
            },
            &mut read_buf,
        ) {
            Ok(ReadFrameStatus::Eof) => break,
            Ok(ReadFrameStatus::Frame(op)) => op,
            Err(_) => break,
        };

        begin_frame_buf(&mut write_buf, op);
        if let Err((code, msg)) = dispatch_rpc(
            op,
            &read_buf,
            state,
            &mut authed,
            &mut conn_wg,
            &mut ack_chan,
            &mut write_buf,
        ) && write_err_into_frame(&mut write_buf, op, code, &msg).is_err()
        {
            break;
        }
        if finish_and_write_frame(stream, &mut write_buf).is_err() {
            break;
        }
    }
}

fn dispatch_rpc(
    op: u8,
    payload: &[u8],
    state: &Arc<ServerState>,
    authed: &mut bool,
    conn_wg: &mut Option<String>,
    ack_chan: &mut Option<WriteAckChan>,
    out: &mut Vec<u8>,
) -> Result<(), (u8, String)> {
    let mut cur = payload;
    if op == OP_HELLO {
        let token = read_str_ref(&mut cur).map_err(|e| (ERR_RUNTIME, e.to_string()))?;
        let wg = read_opt_str(&mut cur).map_err(|e| (ERR_RUNTIME, e.to_string()))?;
        if let Some(expected) = &state.config.token
            && !token_eq(token, expected)
        {
            *authed = false;
            return Err((ERR_UNAUTHORIZED, "invalid authentication token".to_string()));
        }
        *authed = true;
        *conn_wg = wg;
        out.push(0);
        out.push(match state.config.role {
            NodeRole::Primary => 0,
            NodeRole::Follower => 1,
        });
        out.extend_from_slice(&state.current_gen().to_le_bytes());
        return Ok(());
    }

    if !*authed {
        return Err((
            ERR_UNAUTHORIZED,
            "connection not authenticated".to_string(),
        ));
    }

    match op {
        OP_PING => {
            out.push(0);
            out.push(match state.config.role {
                NodeRole::Primary => 0,
                NodeRole::Follower => 1,
            });
            out.extend_from_slice(&state.current_gen().to_le_bytes());
            Ok(())
        }
        OP_EXECUTE => handle_execute(cur, state, conn_wg.as_deref(), ack_chan, out),
        OP_INSERT_BATCH => handle_insert_batch(cur, state, ack_chan, out),
        OP_PARTIAL_SUMMARIZE => handle_partial_summarize(cur, state, conn_wg.as_deref(), out),
        OP_PULL_SYNC => handle_pull_sync(cur, state, out),
        other => Err((ERR_RUNTIME, format!("unknown rpc op {other:#x}"))),
    }
}

fn finish_read_execute(
    t0: Instant,
    read_gen: u64,
    batch: RecordBatch,
    wg: &WorkloadGroup,
    out: &mut Vec<u8>,
) -> Result<(), (u8, String)> {
    if let Some(qto) = wg.query_timeout
        && t0.elapsed() > qto
    {
        return Err((
            ERR_WORKLOAD_LIMIT,
            format!(
                "query elapsed {:?} exceeds workload group {:?} timeout {:?}",
                t0.elapsed(),
                wg.name,
                qto
            ),
        ));
    }
    if let Some(max_rows) = wg.max_scan_rows
        && batch.n() > max_rows
    {
        return Err((
            ERR_WORKLOAD_LIMIT,
            format!(
                "result rows {} exceeds workload group {:?} max_scan_rows {}",
                batch.n(),
                wg.name,
                max_rows
            ),
        ));
    }
    let batch_wire = batch.to_wire_bytes();
    if batch_wire.len() > wg.max_result_bytes || batch.byte_size() > wg.max_result_bytes {
        return Err((
            ERR_WORKLOAD_LIMIT,
            format!(
                "result bytes {} exceeds workload group {:?} max_result_bytes {}",
                batch_wire.len().max(batch.byte_size()),
                wg.name,
                wg.max_result_bytes
            ),
        ));
    }
    let elapsed_us = u64::try_from(t0.elapsed().as_micros()).unwrap_or(u64::MAX);
    encode_execute_ok(out, read_gen, 0, elapsed_us, 1, Some(&batch_wire), None)
}

fn handle_execute(
    mut cur: &[u8],
    state: &Arc<ServerState>,
    conn_wg: Option<&str>,
    ack_chan: &mut Option<WriteAckChan>,
    out: &mut Vec<u8>,
) -> Result<(), (u8, String)> {
    let consistency = decode_consistency(&mut cur).map_err(|e| (ERR_RUNTIME, e.to_string()))?;
    let wg_name = read_opt_str_ref(&mut cur).map_err(|e| (ERR_RUNTIME, e.to_string()))?;
    let timeout_ms = read_u64_le(&mut cur).map_err(|e| (ERR_RUNTIME, e.to_string()))?;
    let linql = read_str_ref(&mut cur).map_err(|e| (ERR_RUNTIME, e.to_string()))?;

    let wg = state.resolve_workload_group(wg_name, conn_wg)?;
    let mut wait_budget = state.config.consistency_wait_timeout;
    if let Some(qto) = wg.query_timeout {
        wait_budget = wait_budget.min(qto);
    }
    if timeout_ms > 0 {
        wait_budget = wait_budget.min(Duration::from_millis(timeout_ms));
    }
    let t0 = Instant::now();

    // Zero-parse fast path: if all cold tables are hydrated and the query is already compiled
    // as a pure read-only query in ReadDb's shared plan cache, skip parsing `linql` completely.
    if !state.cold_unhydrated.load(Ordering::Acquire)
        && let Ok(guard) = state.cached_reader.read()
        && let Some(reader) = guard.as_ref().cloned()
        && let Some(prep) = reader.get_cached_prepared(linql)
        && !prep.writes
        && !prep.stmts.is_empty()
        && prep.stmts.iter().all(|s| matches!(s, Stmt::Query(_)))
    {
        let gen_before = reader.r#gen();
        state.ensure_consistency(consistency, t0 + wait_budget)?;
        let reader =
            if state.current_gen() != gen_before || state.cold_unhydrated.load(Ordering::Acquire) {
                state
                    .get_reader_for_stmts(&prep.stmts)
                    .map_err(|e| (ERR_RUNTIME, e.to_string()))?
            } else {
                reader
            };
        let read_gen = reader.r#gen();
        let batch = reader
            .run_prepared_batch_lossless(&prep)
            .map_err(|e| (ERR_RUNTIME, e.to_string()))?;
        return finish_read_execute(t0, read_gen, batch, wg, out);
    }

    let stmts = crate::parse::parse_program(linql).map_err(|e| (ERR_RUNTIME, e.to_string()))?;
    let is_read_only = !stmts.is_empty() && stmts.iter().all(|s| matches!(s, Stmt::Query(_)));

    if is_read_only {
        state.ensure_consistency(consistency, t0 + wait_budget)?;

        let reader = state
            .get_reader_for_stmts(&stmts)
            .map_err(|e| (ERR_RUNTIME, e.to_string()))?;
        let read_gen = reader.r#gen();
        let batch = reader
            .run_batch_lossless_with_stmts(linql, stmts)
            .map_err(|e| (ERR_RUNTIME, e.to_string()))?;
        finish_read_execute(t0, read_gen, batch, wg, out)
    } else {
        if state.config.role == NodeRole::Follower {
            return Err((
                ERR_READ_ONLY_FOLLOWER,
                "follower is read-only; writes must target primary".to_string(),
            ));
        }
        let approx_bytes = linql.len().max(1);
        let ack = send_write_and_wait(
            state,
            WriteOp::Linql {
                src: linql.to_owned(),
                stmts,
            },
            approx_bytes,
            ack_chan,
        )?;
        let elapsed_us = u64::try_from(t0.elapsed().as_micros()).unwrap_or(u64::MAX);
        encode_execute_ok(out, ack.r#gen, ack.affected, elapsed_us, ack.group_size, None, None)
    }
}

fn encode_execute_ok(
    out: &mut Vec<u8>,
    r#gen: u64,
    affected: usize,
    elapsed_us: u64,
    group_size: usize,
    batch_wire: Option<&[u8]>,
    message: Option<&str>,
) -> Result<(), (u8, String)> {
    out.reserve(48 + batch_wire.map_or(0, |b| b.len()));
    out.push(0);
    out.extend_from_slice(&r#gen.to_le_bytes());
    out.extend_from_slice(&(affected as u64).to_le_bytes());
    out.extend_from_slice(&elapsed_us.to_le_bytes());
    out.extend_from_slice(&(group_size as u32).to_le_bytes());
    match batch_wire {
        Some(b) => {
            out.push(1);
            push_u32_bytes(out, b).map_err(|e| (ERR_RUNTIME, e.to_string()))?;
        }
        None => out.push(0),
    }
    push_opt_str(out, message).map_err(|e| (ERR_RUNTIME, e.to_string()))?;
    Ok(())
}

fn handle_insert_batch(
    mut cur: &[u8],
    state: &Arc<ServerState>,
    ack_chan: &mut Option<WriteAckChan>,
    out: &mut Vec<u8>,
) -> Result<(), (u8, String)> {
    if state.config.role == NodeRole::Follower {
        return Err((
            ERR_READ_ONLY_FOLLOWER,
            "follower is read-only; writes must target primary".to_string(),
        ));
    }
    let collection = read_str(&mut cur).map_err(|e| (ERR_RUNTIME, e.to_string()))?;
    let wire_bytes = read_len_bytes(&mut cur).map_err(|e| (ERR_RUNTIME, e.to_string()))?;
    let batch =
        RecordBatch::from_wire_bytes(wire_bytes).map_err(|e| (ERR_RUNTIME, e.to_string()))?;
    let approx_bytes = collection
        .len()
        .saturating_add(wire_bytes.len().max(batch.byte_size()))
        .max(1);

    let t0 = Instant::now();
    let ack = send_write_and_wait(
        state,
        WriteOp::InsertBatch { collection, batch },
        approx_bytes,
        ack_chan,
    )?;
    let elapsed_us = u64::try_from(t0.elapsed().as_micros()).unwrap_or(u64::MAX);
    encode_execute_ok(out, ack.r#gen, ack.affected, elapsed_us, ack.group_size, None, None)
}

fn handle_partial_summarize(
    mut cur: &[u8],
    state: &Arc<ServerState>,
    conn_wg: Option<&str>,
    out: &mut Vec<u8>,
) -> Result<(), (u8, String)> {
    let consistency = decode_consistency(&mut cur).map_err(|e| (ERR_RUNTIME, e.to_string()))?;
    let wg_name = read_opt_str_ref(&mut cur).map_err(|e| (ERR_RUNTIME, e.to_string()))?;
    let linql = read_str_ref(&mut cur).map_err(|e| (ERR_RUNTIME, e.to_string()))?;

    let wg = state.resolve_workload_group(wg_name, conn_wg)?;
    let t0 = Instant::now();
    let wait_budget = match wg.query_timeout {
        Some(qto) => state.config.consistency_wait_timeout.min(qto),
        None => state.config.consistency_wait_timeout,
    };
    state.ensure_consistency(consistency, t0 + wait_budget)?;

    let (read_gen, lagg_bytes) = if !state.cold_unhydrated.load(Ordering::Acquire) {
        let reader = state
            .get_hydrated_reader()
            .map_err(|e| (ERR_RUNTIME, e.to_string()))?;
        let prep = reader
            .prepare(linql)
            .map_err(|e| (ERR_RUNTIME, e.to_string()))?;
        let [Stmt::Query(q)] = &prep.stmts[..] else {
            return Err((
                ERR_RUNTIME,
                "partial_summarize requires a query statement".to_string(),
            ));
        };
        let bytes = eval_partial_summarize_query_on_reader(&reader, q)
            .map_err(|e| (ERR_RUNTIME, e.to_string()))?;
        (reader.r#gen(), bytes)
    } else {
        let stmt = crate::parse::parse(linql).map_err(|e| (ERR_RUNTIME, e.to_string()))?;
        if !matches!(stmt, Stmt::Query(_)) {
            return Err((
                ERR_RUNTIME,
                "partial_summarize requires a query statement".to_string(),
            ));
        }
        let reader = state
            .get_reader_for_stmts(std::slice::from_ref(&stmt))
            .map_err(|e| (ERR_RUNTIME, e.to_string()))?;
        let Stmt::Query(q) = stmt else {
            unreachable!()
        };
        let bytes = eval_partial_summarize_query_on_reader(&reader, &q)
            .map_err(|e| (ERR_RUNTIME, e.to_string()))?;
        (reader.r#gen(), bytes)
    };

    if lagg_bytes.len() > wg.max_result_bytes {
        return Err((
            ERR_WORKLOAD_LIMIT,
            format!(
                "partial summarize state bytes {} exceeds max_result_bytes {}",
                lagg_bytes.len(),
                wg.max_result_bytes
            ),
        ));
    }
    out.reserve(16 + lagg_bytes.len());
    out.push(0);
    out.extend_from_slice(&read_gen.to_le_bytes());
    push_u32_bytes(out, &lagg_bytes).map_err(|e| (ERR_RUNTIME, e.to_string()))?;
    Ok(())
}

fn record_replica_progress(state: &ServerState, follower_id: Option<&str>, reported_gen: u64) {
    let Some(id) = follower_id.filter(|s| !s.is_empty() && s.len() <= 256) else {
        return;
    };
    if let Ok(mut guard) = state.replicas.lock() {
        if let Some(entry) = guard.get_mut(id) {
            *entry = (reported_gen, Instant::now());
        } else if guard.len() < 256 {
            guard.insert(id.to_string(), (reported_gen, Instant::now()));
        }
    }
}

fn read_wal_window_lockfree(
    dir: &std::path::Path,
    start_offset: u64,
    end_offset: u64,
    since_gen: u64,
    cur_gen: u64,
) -> Option<Vec<u8>> {
    use std::io::{Read, Seek, SeekFrom};
    if end_offset <= start_offset {
        return None;
    }
    let len = usize::try_from(end_offset - start_offset).ok()?;
    if len > MAX_FRAME_BYTES.saturating_sub(64) {
        return None;
    }
    let mut f = std::fs::File::open(dir.join(crate::persist::LOG_NAME)).ok()?;
    if f.metadata().ok()?.len() < end_offset {
        return None;
    }
    f.seek(SeekFrom::Start(start_offset)).ok()?;
    let mut wal_bytes = vec![0u8; len];
    f.read_exact(&mut wal_bytes).ok()?;
    let mut first_gen = None;
    let mut last_gen = None;
    let scan_ok = crate::persist::for_each_wal_frame_raw(&wal_bytes, |_, rec| {
        if first_gen.is_none() {
            first_gen = Some(rec.r#gen);
        }
        last_gen = Some(rec.r#gen);
        Ok(())
    })
    .is_ok();
    if scan_ok
        && first_gen.is_some_and(|fg| fg <= since_gen.saturating_add(1))
        && last_gen == Some(cur_gen)
    {
        Some(wal_bytes)
    } else {
        None
    }
}

fn handle_pull_sync(
    mut cur: &[u8],
    state: &Arc<ServerState>,
    out: &mut Vec<u8>,
) -> Result<(), (u8, String)> {
    let since_gen = read_u64_le(&mut cur).map_err(|e| (ERR_RUNTIME, e.to_string()))?;
    let force_bootstrap = read_u8(&mut cur).map_err(|e| (ERR_RUNTIME, e.to_string()))? != 0;
    let wait_ms = if cur.len() >= 8 {
        read_u64_le(&mut cur).map_err(|e| (ERR_RUNTIME, e.to_string()))?
    } else {
        0
    };
    let follower_id = if !cur.is_empty() {
        Some(read_str_ref(&mut cur).map_err(|e| (ERR_RUNTIME, e.to_string()))?)
    } else {
        None
    };

    record_replica_progress(state, follower_id, since_gen);

    // Idle follower polls answer from the published atomic generation (or wait on `gen_cv`
    // when long-polling) so they never touch the primary writer mutex while caught up.
    let mut published = state.current_gen();
    if !force_bootstrap && since_gen >= published && wait_ms > 0 {
        let cap = Duration::from_millis(wait_ms.min(250));
        let deadline = Instant::now() + cap;
        let mut g = state
            .gen_state
            .lock()
            .map_err(|_| (ERR_RUNTIME, "generation lock poisoned".to_string()))?;
        while *g <= since_gen && !state.shutdown.load(Ordering::Acquire) {
            let now = Instant::now();
            if now >= deadline {
                break;
            }
            let (guard, _) = state
                .gen_cv
                .wait_timeout(g, deadline - now)
                .map_err(|_| (ERR_RUNTIME, "generation condvar poisoned".to_string()))?;
            g = guard;
        }
        published = *g;
    }

    if !force_bootstrap && since_gen >= published {
        out.push(0);
        out.push(0); // UpToDate
        out.extend_from_slice(&published.to_le_bytes());
        return Ok(());
    }

    let max_lag = state.max_replica_lag_gens.load(Ordering::Acquire);

    // Fast lock-free WAL tail export: inspect `wal_index` in O(log N) under a brief lock,
    // then drop `state.db` before reading `log` from disk and scanning frame CRCs so
    // primary writes continue unimpeded while followers pull WAL frames.
    if !force_bootstrap {
        let window_opt = {
            let db_guard = state
                .db
                .lock()
                .map_err(|_| (ERR_RUNTIME, "db lock poisoned".to_string()))?;
            let db = db_guard
                .as_ref()
                .ok_or_else(|| (ERR_RUNTIME, "server database closed".to_string()))?;
            let cur_gen = db.r#gen();
            if since_gen >= cur_gen {
                out.push(0);
                out.push(0); // UpToDate
                out.extend_from_slice(&cur_gen.to_le_bytes());
                return Ok(());
            }
            if max_lag > 0 && cur_gen.saturating_sub(since_gen) > max_lag {
                None
            } else {
                db.wal_export_window(since_gen)
            }
        };
        if let Some((dir, start_off, end_off, cur_gen)) = window_opt
            && let Some(wal_bytes) =
                read_wal_window_lockfree(&dir, start_off, end_off, since_gen, cur_gen)
        {
            out.reserve(16 + wal_bytes.len());
            out.push(0);
            out.push(1); // Wal
            out.extend_from_slice(&cur_gen.to_le_bytes());
            push_u32_bytes(out, &wal_bytes).map_err(|e| (ERR_RUNTIME, e.to_string()))?;
            return Ok(());
        }
    }

    let mut db_guard = state
        .db
        .lock()
        .map_err(|_| (ERR_RUNTIME, "db lock poisoned".to_string()))?;
    let db = db_guard
        .as_mut()
        .ok_or_else(|| (ERR_RUNTIME, "server database closed".to_string()))?;
    let cur_gen = db.r#gen();

    if !force_bootstrap && since_gen >= cur_gen {
        out.push(0);
        out.push(0); // UpToDate
        out.extend_from_slice(&cur_gen.to_le_bytes());
        return Ok(());
    }

    let lag_exceeded = max_lag > 0 && cur_gen.saturating_sub(since_gen) > max_lag;
    if !force_bootstrap && !lag_exceeded && db.is_durable() {
        let wal_bytes = db
            .export_wal_since(since_gen)
            .map_err(|e| (ERR_RUNTIME, e.to_string()))?;
        if !wal_bytes.is_empty() {
            let mut first_gen = None;
            let mut last_gen = None;
            let scan_ok = crate::persist::for_each_wal_frame_raw(&wal_bytes, |_, rec| {
                if first_gen.is_none() {
                    first_gen = Some(rec.r#gen);
                }
                last_gen = Some(rec.r#gen);
                Ok(())
            })
            .is_ok();
            if scan_ok
                && first_gen.is_some_and(|fg| fg <= since_gen.saturating_add(1))
                && last_gen == Some(cur_gen)
            {
                out.reserve(16 + wal_bytes.len());
                out.push(0);
                out.push(1); // Wal
                out.extend_from_slice(&cur_gen.to_le_bytes());
                push_u32_bytes(out, &wal_bytes).map_err(|e| (ERR_RUNTIME, e.to_string()))?;
                return Ok(());
            }
        }
    }

    let (bundle_gen, snap_bytes, cold_manifest_json, cold_tombstones, cold_segments) = db
        .capture_bootstrap_bundle()
        .map_err(|e| (ERR_RUNTIME, e.to_string()))?;
    let seg_bytes: usize = cold_segments.iter().map(|(_, b)| 12 + b.len()).sum();
    out.reserve(
        32 + snap_bytes.len() + cold_manifest_json.len() + cold_tombstones.len() + seg_bytes,
    );
    out.push(0);
    out.push(2); // Bootstrap
    out.extend_from_slice(&bundle_gen.to_le_bytes());
    push_u32_bytes(out, &snap_bytes).map_err(|e| (ERR_RUNTIME, e.to_string()))?;
    push_u32_bytes(out, &cold_manifest_json).map_err(|e| (ERR_RUNTIME, e.to_string()))?;
    push_u32_bytes(out, &cold_tombstones).map_err(|e| (ERR_RUNTIME, e.to_string()))?;
    let seg_cnt = u32::try_from(cold_segments.len())
        .map_err(|_| (ERR_RUNTIME, "too many cold segments".to_string()))?;
    out.extend_from_slice(&seg_cnt.to_le_bytes());
    for (seg_id, lseg) in cold_segments {
        out.extend_from_slice(&seg_id.to_le_bytes());
        push_u32_bytes(out, &lseg).map_err(|e| (ERR_RUNTIME, e.to_string()))?;
    }
    Ok(())
}

// =========================================================================
// Keep-alive TCP/TLS RPC Client & ReplicaPoolClient
// =========================================================================

struct ClientConn {
    stream: NetStream,
    read_buf: Vec<u8>,
    write_buf: Vec<u8>,
}

/// Thread-safe, keep-alive RPC client to a single [`LinServer`].
pub struct ServerClient {
    addr: String,
    opts: ClientOpts,
    pool: Mutex<Vec<ClientConn>>,
    session_gen: AtomicU64,
}

impl fmt::Debug for ServerClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ServerClient")
            .field("addr", &self.addr)
            .field("opts", &self.opts)
            .field("session_gen", &self.session_gen.load(Ordering::Relaxed))
            .finish()
    }
}

impl ServerClient {
    /// Connect and authenticate against a [`LinServer`] at `addr`.
    pub fn connect(addr: impl Into<String>, opts: ClientOpts) -> Result<Self, Error> {
        let addr = addr.into();
        let client = Self {
            addr,
            opts,
            pool: Mutex::new(Vec::new()),
            session_gen: AtomicU64::new(0),
        };
        let (conn, cur_gen) = client.open_authenticated()?;
        client.advance_session_gen(cur_gen);
        client.return_conn(conn);
        Ok(client)
    }

    pub fn addr(&self) -> &str {
        &self.addr
    }

    pub fn session_gen(&self) -> u64 {
        self.session_gen.load(Ordering::Acquire)
    }

    #[inline]
    pub fn advance_session_gen(&self, r#gen: u64) {
        if r#gen > self.session_gen.load(Ordering::Relaxed) {
            self.session_gen.fetch_max(r#gen, Ordering::AcqRel);
        }
    }

    fn open_authenticated(&self) -> Result<(ClientConn, u64), Error> {
        let pull = self.opts.to_pull_opts();
        let mut stream = connect_net_stream(&self.addr, &pull, self.opts.timeout)?;
        stream.set_timeout(Some(self.opts.timeout))?;
        let mut write_buf = Vec::with_capacity(512);
        let mut read_buf = Vec::with_capacity(512);
        begin_frame_buf(&mut write_buf, OP_HELLO);
        push_str(&mut write_buf, self.opts.token.as_deref().unwrap_or(""))?;
        push_opt_str(&mut write_buf, self.opts.workload_group.as_deref())?;
        finish_and_write_frame(&mut stream, &mut write_buf)?;
        match read_frame_into(&mut stream, None, self.opts.timeout, MAX_FRAME_BYTES, &mut read_buf)? {
            ReadFrameStatus::Frame(op) if op == OP_HELLO => {}
            ReadFrameStatus::Frame(op) => {
                return Err(Error::runtime(format!("unexpected hello reply op {op}")));
            }
            ReadFrameStatus::Eof => return Err(io_err("server closed connection during handshake")),
        }
        let mut cur = read_buf.as_slice();
        match read_u8(&mut cur)? {
            0 => {
                let _role = read_u8(&mut cur)?;
                let cur_gen = read_u64_le(&mut cur)?;
                Ok((
                    ClientConn {
                        stream,
                        read_buf,
                        write_buf,
                    },
                    cur_gen,
                ))
            }
            _ => {
                let code = read_u8(&mut cur)?;
                let msg = read_str_ref(&mut cur)?;
                Err(wire_err(code, msg))
            }
        }
    }

    fn take_stream(&self) -> Result<(ClientConn, bool), Error> {
        if let Ok(mut guard) = self.pool.lock()
            && let Some(conn) = guard.pop()
        {
            return Ok((conn, true));
        }
        let (conn, cur_gen) = self.open_authenticated()?;
        self.advance_session_gen(cur_gen);
        Ok((conn, false))
    }

    fn return_conn(&self, conn: ClientConn) {
        if let Ok(mut guard) = self.pool.lock()
            && guard.len() < MAX_POOLED_CONNS
        {
            guard.push(conn);
        }
    }

    fn rpc_call<R>(
        &self,
        op: u8,
        encode_req: impl Fn(&mut Vec<u8>) -> Result<(), Error>,
        retry_stale: impl FnOnce() -> bool,
        decode_resp: impl FnOnce(&[u8]) -> Result<R, Error>,
    ) -> Result<R, Error> {
        let (mut conn, from_pool) = self.take_stream()?;
        match self.send_and_recv(&mut conn, op, &encode_req) {
            Ok(()) => {
                let decoded = decode_resp(&conn.read_buf);
                self.return_conn(conn);
                decoded
            }
            Err(first_err) if from_pool && retry_stale() => {
                drop(conn);
                let (mut fresh, cur_gen) = self.open_authenticated()?;
                self.advance_session_gen(cur_gen);
                self.send_and_recv(&mut fresh, op, &encode_req)
                    .map_err(|_| first_err)?;
                let decoded = decode_resp(&fresh.read_buf);
                self.return_conn(fresh);
                decoded
            }
            Err(err) => Err(err),
        }
    }

    fn send_and_recv(
        &self,
        conn: &mut ClientConn,
        op: u8,
        encode_req: &impl Fn(&mut Vec<u8>) -> Result<(), Error>,
    ) -> Result<(), Error> {
        begin_frame_buf(&mut conn.write_buf, op);
        encode_req(&mut conn.write_buf)?;
        finish_and_write_frame(&mut conn.stream, &mut conn.write_buf)?;
        match read_frame_into(
            &mut conn.stream,
            None,
            self.opts.timeout,
            MAX_FRAME_BYTES,
            &mut conn.read_buf,
        )? {
            ReadFrameStatus::Frame(resp_op) => {
                if resp_op != op {
                    return Err(Error::runtime(format!(
                        "mismatched rpc reply op: sent {op}, got {resp_op}"
                    )));
                }
                Ok(())
            }
            ReadFrameStatus::Eof => Err(io_err("unexpected EOF waiting for RPC response")),
        }
    }

    fn effective_consistency(&self, c: Consistency) -> Consistency {
        match c {
            Consistency::Session => {
                let g = self.session_gen.load(Ordering::Acquire);
                if g > 0 {
                    Consistency::AtLeastGen(g)
                } else {
                    Consistency::Session
                }
            }
            other => other,
        }
    }

    pub fn ping(&self) -> Result<(NodeRole, u64), Error> {
        let (role, cur_gen) = self.rpc_call(
            OP_PING,
            |_| Ok(()),
            || true,
            |resp| {
                let mut cur = resp;
                if read_u8(&mut cur)? != 0 {
                    let code = read_u8(&mut cur)?;
                    let msg = read_str_ref(&mut cur)?;
                    return Err(wire_err(code, msg));
                }
                let role = match read_u8(&mut cur)? {
                    0 => NodeRole::Primary,
                    _ => NodeRole::Follower,
                };
                let cur_gen = read_u64_le(&mut cur)?;
                Ok((role, cur_gen))
            },
        )?;
        self.advance_session_gen(cur_gen);
        Ok((role, cur_gen))
    }

    pub fn execute(&self, linql: &str) -> Result<ExecutionReceipt, Error> {
        self.execute_with_consistency(linql, self.opts.consistency)
    }

    pub fn execute_with_consistency(
        &self,
        linql: &str,
        consistency: Consistency,
    ) -> Result<ExecutionReceipt, Error> {
        let eff = self.effective_consistency(consistency);
        let to_ms = u64::try_from(self.opts.timeout.as_millis()).unwrap_or(u64::MAX);
        let wg = self.opts.workload_group.as_deref();
        let receipt = self.rpc_call(
            OP_EXECUTE,
            |req| {
                req.reserve(32 + linql.len());
                encode_consistency(req, eff);
                push_opt_str(req, wg)?;
                req.extend_from_slice(&to_ms.to_le_bytes());
                push_str(req, linql)
            },
            || {
                crate::parse::parse_program(linql).is_ok_and(|stmts| {
                    !stmts.is_empty() && stmts.iter().all(|s| matches!(s, Stmt::Query(_)))
                })
            },
            decode_execute_resp,
        )?;
        self.advance_session_gen(receipt.r#gen);
        Ok(receipt)
    }

    pub fn query(&self, linql: &str) -> Result<RecordBatch, Error> {
        Ok(self
            .query_with_consistency(linql, self.opts.consistency)?
            .0)
    }

    pub fn query_with_consistency(
        &self,
        linql: &str,
        consistency: Consistency,
    ) -> Result<(RecordBatch, ExecutionReceipt), Error> {
        let mut receipt = self.execute_with_consistency(linql, consistency)?;
        let batch = receipt
            .batch
            .take()
            .unwrap_or_else(|| RecordBatch::empty(Vec::<String>::new()));
        Ok((batch, receipt))
    }

    pub fn insert_batch(
        &self,
        collection: &str,
        batch: &RecordBatch,
    ) -> Result<ExecutionReceipt, Error> {
        let wire = batch.to_wire_bytes();
        let receipt = self.rpc_call(
            OP_INSERT_BATCH,
            |req| {
                req.reserve(16 + collection.len() + wire.len());
                push_str(req, collection)?;
                push_u32_bytes(req, &wire)
            },
            || false,
            decode_execute_resp,
        )?;
        self.advance_session_gen(receipt.r#gen);
        Ok(receipt)
    }

    pub fn partial_summarize(
        &self,
        linql: &str,
        consistency: Consistency,
    ) -> Result<(u64, Vec<u8>), Error> {
        let eff = self.effective_consistency(consistency);
        let wg = self.opts.workload_group.as_deref();
        let (r#gen, bytes) = self.rpc_call(
            OP_PARTIAL_SUMMARIZE,
            |req| {
                req.reserve(24 + linql.len());
                encode_consistency(req, eff);
                push_opt_str(req, wg)?;
                push_str(req, linql)
            },
            || true,
            |resp| {
                let mut cur = resp;
                if read_u8(&mut cur)? != 0 {
                    let code = read_u8(&mut cur)?;
                    let msg = read_str_ref(&mut cur)?;
                    return Err(wire_err(code, msg));
                }
                let r#gen = read_u64_le(&mut cur)?;
                let bytes = read_len_bytes(&mut cur)?.to_vec();
                Ok((r#gen, bytes))
            },
        )?;
        self.advance_session_gen(r#gen);
        Ok((r#gen, bytes))
    }

    pub fn pull_sync(
        &self,
        since_gen: u64,
        force_bootstrap: bool,
    ) -> Result<PullSyncPayload, Error> {
        self.pull_sync_wait(since_gen, force_bootstrap, 0, None)
    }

    pub fn pull_sync_wait(
        &self,
        since_gen: u64,
        force_bootstrap: bool,
        wait_ms: u64,
        follower_id: Option<&str>,
    ) -> Result<PullSyncPayload, Error> {
        self.rpc_call(
            OP_PULL_SYNC,
            |req| {
                req.reserve(21 + follower_id.map_or(0, str::len));
                req.extend_from_slice(&since_gen.to_le_bytes());
                req.push(u8::from(force_bootstrap));
                if wait_ms > 0 || follower_id.is_some() {
                    req.extend_from_slice(&wait_ms.to_le_bytes());
                    if let Some(fid) = follower_id {
                        push_str(req, fid)?;
                    }
                }
                Ok(())
            },
            || true,
            |resp| {
                let mut cur = resp;
                if read_u8(&mut cur)? != 0 {
                    let code = read_u8(&mut cur)?;
                    let msg = read_str_ref(&mut cur)?;
                    return Err(wire_err(code, msg));
                }
                let kind = read_u8(&mut cur)?;
                let r#gen = read_u64_le(&mut cur)?;
                match kind {
                    0 => Ok(PullSyncPayload::UpToDate { r#gen }),
                    1 => {
                        let wal_bytes = read_len_bytes(&mut cur)?.to_vec();
                        Ok(PullSyncPayload::Wal { r#gen, wal_bytes })
                    }
                    2 => {
                        let snap_bytes = read_len_bytes(&mut cur)?.to_vec();
                        let cold_manifest_json = read_len_bytes(&mut cur)?.to_vec();
                        let cold_tombstones = read_len_bytes(&mut cur)?.to_vec();
                        let num_segs = read_u32_le(&mut cur)? as usize;
                        let mut cold_segments = Vec::with_capacity(num_segs);
                        for _ in 0..num_segs {
                            let seg_id = read_u64_le(&mut cur)?;
                            let lseg = read_len_bytes(&mut cur)?.to_vec();
                            cold_segments.push((seg_id, lseg));
                        }
                        Ok(PullSyncPayload::Bootstrap {
                            r#gen,
                            snap_bytes,
                            cold_manifest_json,
                            cold_tombstones,
                            cold_segments,
                        })
                    }
                    other => Err(Error::runtime(format!("unknown pull_sync kind {other}"))),
                }
            },
        )
    }
}

fn decode_execute_resp(resp: &[u8]) -> Result<ExecutionReceipt, Error> {
    let mut cur = resp;
    if read_u8(&mut cur)? != 0 {
        let code = read_u8(&mut cur)?;
        let msg = read_str_ref(&mut cur)?;
        return Err(wire_err(code, msg));
    }
    let r#gen = read_u64_le(&mut cur)?;
    let affected = read_u64_le(&mut cur)? as usize;
    let elapsed_us = read_u64_le(&mut cur)?;
    let group_size = read_u32_le(&mut cur)? as usize;
    let batch = match read_u8(&mut cur)? {
        0 => None,
        1 => {
            let wire = read_len_bytes(&mut cur)?;
            Some(RecordBatch::from_wire_bytes(wire)?)
        }
        other => return Err(Error::runtime(format!("invalid batch presence tag {other}"))),
    };
    let message = read_opt_str(&mut cur)?;
    Ok(ExecutionReceipt {
        r#gen,
        affected,
        elapsed_us,
        group_size,
        batch,
        message,
    })
}

/// Primary + replica pool router providing session-consistent read-your-writes scaling.
pub struct ReplicaPoolClient {
    primary: ServerClient,
    replicas: Vec<ServerClient>,
    default_consistency: Consistency,
    next_rr: AtomicUsize,
    session_gen: AtomicU64,
}

impl fmt::Debug for ReplicaPoolClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ReplicaPoolClient")
            .field("primary", &self.primary.addr())
            .field(
                "replicas",
                &self.replicas.iter().map(|r| r.addr()).collect::<Vec<_>>(),
            )
            .field("default_consistency", &self.default_consistency)
            .field("session_gen", &self.session_gen.load(Ordering::Relaxed))
            .finish()
    }
}

impl ReplicaPoolClient {
    pub fn connect(
        primary_addr: impl Into<String>,
        replica_addrs: &[String],
        opts: ClientOpts,
    ) -> Result<Self, Error> {
        let default_consistency = opts.consistency;
        let primary = ServerClient::connect(primary_addr, opts.clone())?;
        let mut session = primary.session_gen();
        let mut replicas = Vec::with_capacity(replica_addrs.len());
        for addr in replica_addrs {
            if let Ok(c) = ServerClient::connect(addr.clone(), opts.clone()) {
                session = session.max(c.session_gen());
                replicas.push(c);
            }
        }
        Ok(Self {
            primary,
            replicas,
            default_consistency,
            next_rr: AtomicUsize::new(0),
            session_gen: AtomicU64::new(session),
        })
    }

    pub fn session_gen(&self) -> u64 {
        self.session_gen.load(Ordering::Acquire)
    }

    #[inline]
    pub fn advance_session_gen(&self, r#gen: u64) {
        if r#gen <= self.session_gen.load(Ordering::Relaxed) {
            return;
        }
        self.session_gen.fetch_max(r#gen, Ordering::AcqRel);
        self.primary.advance_session_gen(r#gen);
        for r in &self.replicas {
            r.advance_session_gen(r#gen);
        }
    }

    pub fn primary(&self) -> &ServerClient {
        &self.primary
    }

    pub fn replicas(&self) -> &[ServerClient] {
        &self.replicas
    }

    pub fn execute(&self, linql: &str) -> Result<ExecutionReceipt, Error> {
        let is_read = crate::parse::parse_program(linql)
            .is_ok_and(|stmts| !stmts.is_empty() && stmts.iter().all(|s| matches!(s, Stmt::Query(_))));
        if is_read {
            let (batch, mut receipt) = self.query_with_consistency(linql, self.default_consistency)?;
            receipt.batch = Some(batch);
            Ok(receipt)
        } else {
            let receipt = self.primary.execute_with_consistency(linql, Consistency::Strong)?;
            self.advance_session_gen(receipt.r#gen);
            Ok(receipt)
        }
    }

    pub fn insert_batch(
        &self,
        collection: &str,
        batch: &RecordBatch,
    ) -> Result<ExecutionReceipt, Error> {
        let receipt = self.primary.insert_batch(collection, batch)?;
        self.advance_session_gen(receipt.r#gen);
        Ok(receipt)
    }

    pub fn query(&self, linql: &str) -> Result<RecordBatch, Error> {
        Ok(self
            .query_with_consistency(linql, self.default_consistency)?
            .0)
    }

    pub fn query_with_consistency(
        &self,
        linql: &str,
        consistency: Consistency,
    ) -> Result<(RecordBatch, ExecutionReceipt), Error> {
        let target_consistency = match consistency {
            Consistency::Session => {
                let s = self.session_gen();
                if s > 0 {
                    Consistency::AtLeastGen(s)
                } else {
                    Consistency::Session
                }
            }
            other => other,
        };

        if matches!(target_consistency, Consistency::Strong) || self.replicas.is_empty() {
            let res = self
                .primary
                .query_with_consistency(linql, target_consistency)?;
            self.advance_session_gen(res.1.r#gen);
            return Ok(res);
        }

        let start = self.next_rr.fetch_add(1, Ordering::Relaxed) % self.replicas.len();
        let mut last_workload_err: Option<Error> = None;
        for offset in 0..self.replicas.len() {
            let replica = &self.replicas[(start + offset) % self.replicas.len()];
            match replica.query_with_consistency(linql, target_consistency) {
                Ok(res) => {
                    self.advance_session_gen(res.1.r#gen);
                    return Ok(res);
                }
                Err(e) if is_retryable_replica_err(&e) => continue,
                Err(e) => {
                    last_workload_err = Some(e);
                    break;
                }
            }
        }
        if let Some(err) = last_workload_err {
            return Err(err);
        }

        let res = self
            .primary
            .query_with_consistency(linql, target_consistency)?;
        self.advance_session_gen(res.1.r#gen);
        Ok(res)
    }

    pub fn partial_summarize(
        &self,
        linql: &str,
        consistency: Consistency,
    ) -> Result<(u64, Vec<u8>), Error> {
        let target_consistency = match consistency {
            Consistency::Session => {
                let s = self.session_gen();
                if s > 0 {
                    Consistency::AtLeastGen(s)
                } else {
                    Consistency::Session
                }
            }
            other => other,
        };

        if matches!(target_consistency, Consistency::Strong) || self.replicas.is_empty() {
            let res = self.primary.partial_summarize(linql, target_consistency)?;
            self.advance_session_gen(res.0);
            return Ok(res);
        }

        let start = self.next_rr.fetch_add(1, Ordering::Relaxed) % self.replicas.len();
        for offset in 0..self.replicas.len() {
            let replica = &self.replicas[(start + offset) % self.replicas.len()];
            match replica.partial_summarize(linql, target_consistency) {
                Ok(res) => {
                    self.advance_session_gen(res.0);
                    return Ok(res);
                }
                Err(e) if is_retryable_replica_err(&e) => continue,
                Err(e) => return Err(e),
            }
        }

        let res = self.primary.partial_summarize(linql, target_consistency)?;
        self.advance_session_gen(res.0);
        Ok(res)
    }
}

fn is_retryable_replica_err(e: &Error) -> bool {
    let s = e.to_string();
    s.contains("stale_replica") || s.contains("lin server io") || s.contains("wal ship")
}
