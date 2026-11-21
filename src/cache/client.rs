use std::borrow::Cow;
use std::io::{BufReader, BufWriter};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use rustc_hash::FxHashMap;
use smallvec::SmallVec;

use crate::error::Error;

use super::cache_err;
use super::protocol::{CacheRequest, CacheResponse, read_frame_buf, write_frame_buf};

const MAX_POOLED_PER_PEER: usize = 8;
const MAX_TRACKED_PEERS: usize = 64;

struct PooledPeerConn {
    reader: BufReader<TcpStream>,
    writer: BufWriter<TcpStream>,
    read_buf: Vec<u8>,
    write_buf: Vec<u8>,
}

impl PooledPeerConn {
    fn connect(addr: SocketAddr) -> Result<Self, Error> {
        let stream = TcpStream::connect_timeout(&addr, Duration::from_secs(2)).map_err(cache_err)?;
        let _ = stream.set_nodelay(true);
        let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
        let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));
        let reader_sock = stream.try_clone().map_err(cache_err)?;
        Ok(Self {
            reader: BufReader::with_capacity(8 * 1024, reader_sock),
            writer: BufWriter::with_capacity(8 * 1024, stream),
            read_buf: Vec::with_capacity(256),
            write_buf: Vec::with_capacity(256),
        })
    }

    fn roundtrip(&mut self, req: &CacheRequest) -> Result<CacheResponse, Error> {
        write_frame_buf(&mut self.writer, req, &mut self.write_buf)?;
        read_frame_buf(&mut self.reader, &mut self.read_buf)
    }
}

fn peer_pool() -> &'static Mutex<FxHashMap<SocketAddr, Vec<PooledPeerConn>>> {
    static POOL: OnceLock<Mutex<FxHashMap<SocketAddr, Vec<PooledPeerConn>>>> = OnceLock::new();
    POOL.get_or_init(|| Mutex::new(FxHashMap::default()))
}

fn take_peer_conn(addr: SocketAddr) -> Option<PooledPeerConn> {
    peer_pool()
        .lock()
        .ok()
        .and_then(|mut map| map.get_mut(&addr).and_then(Vec::pop))
}

fn return_peer_conn(addr: SocketAddr, conn: PooledPeerConn) {
    if let Ok(mut map) = peer_pool().lock() {
        if let Some(list) = map.get_mut(&addr) {
            if list.len() < MAX_POOLED_PER_PEER {
                list.push(conn);
            }
            return;
        }
        if map.len() >= MAX_TRACKED_PEERS {
            map.clear();
        }
        map.insert(addr, vec![conn]);
    }
}

pub(crate) fn evict_peer_pool_addr(addr: SocketAddr) {
    if let Ok(mut map) = peer_pool().lock() {
        map.remove(&addr);
    }
}

fn send_rpc_addr(addr: SocketAddr, req: &CacheRequest) -> Result<CacheResponse, Error> {
    if let Some(mut pooled) = take_peer_conn(addr) {
        if let Ok(resp) = pooled.roundtrip(req) {
            return_peer_conn(addr, pooled);
            return Ok(resp);
        }
        // Stale socket: drop and retry once on a fresh TCP connection.
    }
    let mut fresh = PooledPeerConn::connect(addr)?;
    let resp = fresh.roundtrip(req)?;
    return_peer_conn(addr, fresh);
    Ok(resp)
}

/// Fast internal peer RPC avoiding `ToSocketAddrs` resolution when `addr` is already `IP:PORT`.
pub(crate) fn send_peer_rpc(addr: &str, req: &CacheRequest) -> Result<CacheResponse, Error> {
    if let Ok(sock_addr) = addr.parse::<SocketAddr>() {
        return send_rpc_addr(sock_addr, req);
    }
    send_rpc(addr, req)
}

/// Send a single request-response RPC to a `CacheNode` over TCP (backed by keep-alive peer pool).
pub fn send_rpc(addr: impl ToSocketAddrs, req: &CacheRequest) -> Result<CacheResponse, Error> {
    let addr_resolved: SocketAddr = addr
        .to_socket_addrs()
        .map_err(cache_err)?
        .next()
        .ok_or_else(|| cache_err("unable to resolve address"))?;
    send_rpc_addr(addr_resolved, req)
}

/// Persistent TCP client for `CacheNode`.
pub struct CacheClient {
    addr: String,
    reader: BufReader<TcpStream>,
    writer: BufWriter<TcpStream>,
    read_buf: Vec<u8>,
    write_buf: Vec<u8>,
}

impl CacheClient {
    pub fn connect(addr: impl Into<String>) -> Result<Self, Error> {
        let addr = addr.into();
        let stream = TcpStream::connect(&addr).map_err(cache_err)?;
        let _ = stream.set_nodelay(true);
        let reader_sock = stream.try_clone().map_err(cache_err)?;
        Ok(Self {
            addr,
            reader: BufReader::with_capacity(8 * 1024, reader_sock),
            writer: BufWriter::with_capacity(8 * 1024, stream),
            read_buf: Vec::with_capacity(256),
            write_buf: Vec::with_capacity(256),
        })
    }

    pub fn addr(&self) -> &str {
        &self.addr
    }

    pub fn send(&mut self, req: &CacheRequest) -> Result<CacheResponse, Error> {
        write_frame_buf(&mut self.writer, req, &mut self.write_buf)?;
        read_frame_buf(&mut self.reader, &mut self.read_buf)
    }

    pub fn read_next(&mut self) -> Result<CacheResponse, Error> {
        read_frame_buf(&mut self.reader, &mut self.read_buf)
    }

    pub fn execute_line(&mut self, line: &str) -> Result<CacheResponse, Error> {
        let req = parse_cli_command(line)?;
        self.send(&req)
    }
}

/// Tokenize a CLI command line respecting double/single quotes.
pub fn tokenize_cli(line: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut cur = String::new();
    let mut in_quote: Option<char> = None;
    let mut chars = line.chars();

    while let Some(c) = chars.next() {
        if let Some(q) = in_quote {
            if c == q {
                in_quote = None;
            } else if c == '\\' {
                if let Some(next_c) = chars.next() {
                    cur.push(next_c);
                }
            } else {
                cur.push(c);
            }
        } else if c == '"' || c == '\'' {
            in_quote = Some(c);
        } else if c.is_whitespace() {
            if !cur.is_empty() {
                tokens.push(std::mem::take(&mut cur));
            }
        } else {
            cur.push(c);
        }
    }
    if !cur.is_empty() {
        tokens.push(cur);
    }
    tokens
}

fn tokenize_cli_cow(line: &str) -> SmallVec<[Cow<'_, str>; 8]> {
    if !line.bytes().any(|b| b == b'"' || b == b'\'' || b == b'\\') {
        return line.split_whitespace().map(Cow::Borrowed).collect();
    }
    let mut tokens: SmallVec<[Cow<'_, str>; 8]> = SmallVec::new();
    let mut cur = String::new();
    let mut in_quote: Option<char> = None;
    let mut chars = line.chars();

    while let Some(c) = chars.next() {
        if let Some(q) = in_quote {
            if c == q {
                in_quote = None;
            } else if c == '\\' {
                if let Some(next_c) = chars.next() {
                    cur.push(next_c);
                }
            } else {
                cur.push(c);
            }
        } else if c == '"' || c == '\'' {
            in_quote = Some(c);
        } else if c.is_whitespace() {
            if !cur.is_empty() {
                tokens.push(Cow::Owned(std::mem::take(&mut cur)));
            }
        } else {
            cur.push(c);
        }
    }
    if !cur.is_empty() {
        tokens.push(Cow::Owned(cur));
    }
    tokens
}

#[inline]
fn ascii_upper_16(s: &str) -> Option<([u8; 16], usize)> {
    let b = s.as_bytes();
    if b.len() > 16 {
        return None;
    }
    let mut out = [0u8; 16];
    for (i, &ch) in b.iter().enumerate() {
        out[i] = ch.to_ascii_uppercase();
    }
    Some((out, b.len()))
}

#[inline]
fn cow_into_string(c: &Cow<'_, str>) -> String {
    c.as_ref().to_owned()
}

/// Parse a CLI command string (e.g., `SET k v EX 10`, `HSET user name Alice`, `LIN docs | take 5`) into a `CacheRequest`.
pub fn parse_cli_command(line: &str) -> Result<CacheRequest, Error> {
    // Fast pre-check for `LIN` / `QUERY` before tokenizing the entire pipe query!
    let trimmed = line.trim_start();
    if let Some((first_tok, rest_raw)) = trimmed.split_once(char::is_whitespace)
        && (first_tok.eq_ignore_ascii_case("LIN") || first_tok.eq_ignore_ascii_case("QUERY"))
    {
        let rest = rest_raw.trim();
        if rest.is_empty() {
            return Err(cache_err("usage: LIN <lin-pipe-query>"));
        }
        return Ok(CacheRequest::LinQuery {
            query: rest.to_owned(),
        });
    }

    let tokens = tokenize_cli_cow(line);
    let Some(cmd_tok) = tokens.first() else {
        return Err(cache_err("empty command"));
    };
    let cmd_str = cmd_tok.as_ref();
    let cmd_up = ascii_upper_16(cmd_str);
    let cmd_bytes = cmd_up.as_ref().map(|(b, n)| &b[..*n]);

    match cmd_bytes {
        Some(b"PING") => Ok(CacheRequest::Ping(tokens.get(1).map(cow_into_string))),
        Some(b"GET") => {
            let key = tokens
                .get(1)
                .ok_or_else(|| cache_err("usage: GET <key>"))
                .map(cow_into_string)?;
            Ok(CacheRequest::Get { key })
        }
        Some(b"SET") => {
            let key = tokens
                .get(1)
                .ok_or_else(|| cache_err("usage: SET <key> <value> [EX <sec> | PX <ms>]"))
                .map(cow_into_string)?;
            let val = tokens
                .get(2)
                .ok_or_else(|| cache_err("usage: SET <key> <value> [EX <sec> | PX <ms>]"))?
                .as_bytes()
                .to_vec();
            let mut ttl_ms = None;
            if let Some(opt) = tokens.get(3) {
                let n: u64 = tokens
                    .get(4)
                    .ok_or_else(|| cache_err("missing TTL number"))?
                    .parse()
                    .map_err(cache_err)?;
                let opt_s = opt.as_ref();
                if opt_s.eq_ignore_ascii_case("EX") {
                    ttl_ms = Some(n * 1000);
                } else if opt_s.eq_ignore_ascii_case("PX") {
                    ttl_ms = Some(n);
                } else {
                    return Err(cache_err(format!(
                        "unknown SET option {}",
                        opt_s.to_ascii_uppercase()
                    )));
                }
            }
            Ok(CacheRequest::Set {
                key,
                value: val,
                ttl_ms,
            })
        }
        Some(b"DEL") => {
            if tokens.len() < 2 {
                return Err(cache_err("usage: DEL <key> [key ...]"));
            }
            Ok(CacheRequest::Del {
                keys: tokens[1..].iter().map(cow_into_string).collect(),
            })
        }
        Some(b"EXISTS") => {
            let key = tokens
                .get(1)
                .ok_or_else(|| cache_err("usage: EXISTS <key>"))
                .map(cow_into_string)?;
            Ok(CacheRequest::Exists { key })
        }
        Some(b"INCR") => {
            let key = tokens
                .get(1)
                .ok_or_else(|| cache_err("usage: INCR <key>"))
                .map(cow_into_string)?;
            Ok(CacheRequest::IncrBy { key, delta: 1 })
        }
        Some(b"DECR") => {
            let key = tokens
                .get(1)
                .ok_or_else(|| cache_err("usage: DECR <key>"))
                .map(cow_into_string)?;
            Ok(CacheRequest::IncrBy { key, delta: -1 })
        }
        Some(b"INCRBY") => {
            let key = tokens
                .get(1)
                .ok_or_else(|| cache_err("usage: INCRBY <key> <delta>"))
                .map(cow_into_string)?;
            let delta: i64 = tokens
                .get(2)
                .ok_or_else(|| cache_err("usage: INCRBY <key> <delta>"))?
                .parse()
                .map_err(cache_err)?;
            Ok(CacheRequest::IncrBy { key, delta })
        }
        Some(b"EXPIRE") => {
            let key = tokens
                .get(1)
                .ok_or_else(|| cache_err("usage: EXPIRE <key> <seconds>"))
                .map(cow_into_string)?;
            let secs: u64 = tokens
                .get(2)
                .ok_or_else(|| cache_err("usage: EXPIRE <key> <seconds>"))?
                .parse()
                .map_err(cache_err)?;
            Ok(CacheRequest::Expire {
                key,
                ttl_ms: secs * 1000,
            })
        }
        Some(b"PEXPIRE") => {
            let key = tokens
                .get(1)
                .ok_or_else(|| cache_err("usage: PEXPIRE <key> <ms>"))
                .map(cow_into_string)?;
            let ttl_ms: u64 = tokens
                .get(2)
                .ok_or_else(|| cache_err("usage: PEXPIRE <key> <ms>"))?
                .parse()
                .map_err(cache_err)?;
            Ok(CacheRequest::Expire { key, ttl_ms })
        }
        Some(b"PERSIST") => {
            let key = tokens
                .get(1)
                .ok_or_else(|| cache_err("usage: PERSIST <key>"))
                .map(cow_into_string)?;
            Ok(CacheRequest::Persist { key })
        }
        Some(b"TTL" | b"PTTL") => {
            let key = tokens
                .get(1)
                .ok_or_else(|| cache_err("usage: TTL <key>"))
                .map(cow_into_string)?;
            Ok(CacheRequest::Ttl { key })
        }
        Some(b"KEYS" | b"SCAN") => {
            let prefix = tokens
                .get(1)
                .map_or_else(|| "*".to_owned(), cow_into_string);
            Ok(CacheRequest::Keys { prefix })
        }
        Some(b"FLUSHALL" | b"FLUSHDB") => Ok(CacheRequest::FlushAll),

        // Hashes
        Some(b"HSET") => {
            let key = tokens
                .get(1)
                .ok_or_else(|| cache_err("usage: HSET <key> <field> <value>"))
                .map(cow_into_string)?;
            let field = tokens
                .get(2)
                .ok_or_else(|| cache_err("usage: HSET <key> <field> <value>"))
                .map(cow_into_string)?;
            let value = tokens
                .get(3)
                .ok_or_else(|| cache_err("usage: HSET <key> <field> <value>"))?
                .as_bytes()
                .to_vec();
            Ok(CacheRequest::HSet { key, field, value })
        }
        Some(b"HGET") => {
            let key = tokens
                .get(1)
                .ok_or_else(|| cache_err("usage: HGET <key> <field>"))
                .map(cow_into_string)?;
            let field = tokens
                .get(2)
                .ok_or_else(|| cache_err("usage: HGET <key> <field>"))
                .map(cow_into_string)?;
            Ok(CacheRequest::HGet { key, field })
        }
        Some(b"HDEL") => {
            if tokens.len() < 3 {
                return Err(cache_err("usage: HDEL <key> <field> [field ...]"));
            }
            Ok(CacheRequest::HDel {
                key: cow_into_string(&tokens[1]),
                fields: tokens[2..].iter().map(cow_into_string).collect(),
            })
        }
        Some(b"HGETALL") => {
            let key = tokens
                .get(1)
                .ok_or_else(|| cache_err("usage: HGETALL <key>"))
                .map(cow_into_string)?;
            Ok(CacheRequest::HGetAll { key })
        }

        // Lists
        Some(b"LPUSH") => {
            if tokens.len() < 3 {
                return Err(cache_err("usage: LPUSH <key> <val> [val ...]"));
            }
            Ok(CacheRequest::LPush {
                key: cow_into_string(&tokens[1]),
                values: tokens[2..].iter().map(|s| s.as_bytes().to_vec()).collect(),
            })
        }
        Some(b"RPUSH") => {
            if tokens.len() < 3 {
                return Err(cache_err("usage: RPUSH <key> <val> [val ...]"));
            }
            Ok(CacheRequest::RPush {
                key: cow_into_string(&tokens[1]),
                values: tokens[2..].iter().map(|s| s.as_bytes().to_vec()).collect(),
            })
        }
        Some(b"LPOP") => {
            let key = tokens
                .get(1)
                .ok_or_else(|| cache_err("usage: LPOP <key>"))
                .map(cow_into_string)?;
            Ok(CacheRequest::LPop { key })
        }
        Some(b"RPOP") => {
            let key = tokens
                .get(1)
                .ok_or_else(|| cache_err("usage: RPOP <key>"))
                .map(cow_into_string)?;
            Ok(CacheRequest::RPop { key })
        }
        Some(b"LRANGE") => {
            let key = tokens
                .get(1)
                .ok_or_else(|| cache_err("usage: LRANGE <key> <start> <stop>"))
                .map(cow_into_string)?;
            let start: isize = tokens
                .get(2)
                .ok_or_else(|| cache_err("usage: LRANGE <key> <start> <stop>"))?
                .parse()
                .map_err(cache_err)?;
            let stop: isize = tokens
                .get(3)
                .ok_or_else(|| cache_err("usage: LRANGE <key> <start> <stop>"))?
                .parse()
                .map_err(cache_err)?;
            Ok(CacheRequest::LRange { key, start, stop })
        }

        // Sets
        Some(b"SADD") => {
            if tokens.len() < 3 {
                return Err(cache_err("usage: SADD <key> <member> [member ...]"));
            }
            Ok(CacheRequest::SAdd {
                key: cow_into_string(&tokens[1]),
                members: tokens[2..].iter().map(cow_into_string).collect(),
            })
        }
        Some(b"SREM") => {
            if tokens.len() < 3 {
                return Err(cache_err("usage: SREM <key> <member> [member ...]"));
            }
            Ok(CacheRequest::SRem {
                key: cow_into_string(&tokens[1]),
                members: tokens[2..].iter().map(cow_into_string).collect(),
            })
        }
        Some(b"SMEMBERS") => {
            let key = tokens
                .get(1)
                .ok_or_else(|| cache_err("usage: SMEMBERS <key>"))
                .map(cow_into_string)?;
            Ok(CacheRequest::SMembers { key })
        }
        Some(b"SISMEMBER") => {
            let key = tokens
                .get(1)
                .ok_or_else(|| cache_err("usage: SISMEMBER <key> <member>"))
                .map(cow_into_string)?;
            let member = tokens
                .get(2)
                .ok_or_else(|| cache_err("usage: SISMEMBER <key> <member>"))
                .map(cow_into_string)?;
            Ok(CacheRequest::SIsMember { key, member })
        }

        // Pub/Sub
        Some(b"PUBLISH") => {
            let channel = tokens
                .get(1)
                .ok_or_else(|| cache_err("usage: PUBLISH <channel> <message>"))
                .map(cow_into_string)?;
            let message = tokens
                .get(2)
                .ok_or_else(|| cache_err("usage: PUBLISH <channel> <message>"))?
                .as_bytes()
                .to_vec();
            Ok(CacheRequest::Publish { channel, message })
        }
        Some(b"SUBSCRIBE") => {
            if tokens.len() < 2 {
                return Err(cache_err("usage: SUBSCRIBE <channel> [channel ...]"));
            }
            Ok(CacheRequest::Subscribe {
                channels: tokens[1..].iter().map(cow_into_string).collect(),
            })
        }

        // Native Lin Pipe Query (empty query case when no trailing argument)
        Some(b"LIN" | b"QUERY") => {
            let rest = trimmed
                .split_once(char::is_whitespace)
                .map(|x| x.1.trim())
                .unwrap_or("");
            if rest.is_empty() {
                return Err(cache_err("usage: LIN <lin-pipe-query>"));
            }
            Ok(CacheRequest::LinQuery {
                query: rest.to_owned(),
            })
        }

        // Cluster & Raft
        Some(b"INFO") => Ok(CacheRequest::ClusterInfo),
        Some(b"CLUSTER") => {
            let sub = tokens.get(1).map_or("INFO", Cow::as_ref);
            if sub.eq_ignore_ascii_case("INFO") {
                Ok(CacheRequest::ClusterInfo)
            } else if sub.eq_ignore_ascii_case("NODES") {
                Ok(CacheRequest::ClusterNodes)
            } else if sub.eq_ignore_ascii_case("MEET") {
                let node_id = tokens
                    .get(2)
                    .ok_or_else(|| cache_err("usage: CLUSTER MEET <node_id> <host:port>"))
                    .map(cow_into_string)?;
                let addr = tokens
                    .get(3)
                    .ok_or_else(|| cache_err("usage: CLUSTER MEET <node_id> <host:port>"))
                    .map(cow_into_string)?;
                Ok(CacheRequest::ClusterMeet { node_id, addr })
            } else if sub.eq_ignore_ascii_case("KEYSLOT") || sub.eq_ignore_ascii_case("SHARD") {
                let key = tokens
                    .get(2)
                    .ok_or_else(|| cache_err("usage: CLUSTER KEYSLOT <key>"))
                    .map(cow_into_string)?;
                Ok(CacheRequest::ClusterKeySlot { key })
            } else {
                Err(cache_err(format!(
                    "unknown CLUSTER subcommand {} (INFO|NODES|MEET|KEYSLOT)",
                    sub.to_ascii_uppercase()
                )))
            }
        }
        Some(b"RAFT") => {
            let sub = tokens.get(1).map_or("STATUS", Cow::as_ref);
            if sub.eq_ignore_ascii_case("STATUS") {
                Ok(CacheRequest::RaftStatus)
            } else if sub.eq_ignore_ascii_case("ELECT") || sub.eq_ignore_ascii_case("ELECTION") {
                Ok(CacheRequest::RaftTriggerElection)
            } else {
                Err(cache_err(format!(
                    "unknown RAFT subcommand {} (STATUS|ELECT)",
                    sub.to_ascii_uppercase()
                )))
            }
        }
        _ => Err(cache_err(format!(
            "unknown command '{}'",
            cmd_str.to_ascii_uppercase()
        ))),
    }
}
