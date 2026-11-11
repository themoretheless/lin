use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::time::Duration;

use crate::error::Error;

use super::cache_err;
use super::protocol::{CacheRequest, CacheResponse, read_frame, write_frame};

/// Send a single request-response RPC to a `CacheNode` over TCP.
pub fn send_rpc(addr: impl ToSocketAddrs, req: &CacheRequest) -> Result<CacheResponse, Error> {
    let addr_resolved: SocketAddr = addr
        .to_socket_addrs()
        .map_err(cache_err)?
        .next()
        .ok_or_else(|| cache_err("unable to resolve address"))?;
    let mut stream =
        TcpStream::connect_timeout(&addr_resolved, Duration::from_secs(2)).map_err(cache_err)?;
    let _ = stream.set_nodelay(true);
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));
    write_frame(&mut stream, req)?;
    read_frame(&mut stream)
}

/// Persistent TCP client for `CacheNode`.
pub struct CacheClient {
    addr: String,
    stream: TcpStream,
}

impl CacheClient {
    pub fn connect(addr: impl Into<String>) -> Result<Self, Error> {
        let addr = addr.into();
        let stream = TcpStream::connect(&addr).map_err(cache_err)?;
        let _ = stream.set_nodelay(true);
        Ok(Self { addr, stream })
    }

    pub fn addr(&self) -> &str {
        &self.addr
    }

    pub fn send(&mut self, req: &CacheRequest) -> Result<CacheResponse, Error> {
        write_frame(&mut self.stream, req)?;
        read_frame(&mut self.stream)
    }

    pub fn read_next(&mut self) -> Result<CacheResponse, Error> {
        read_frame(&mut self.stream)
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

/// Parse a CLI command string (e.g., `SET k v EX 10`, `HSET user name Alice`, `LIN docs | take 5`) into a `CacheRequest`.
pub fn parse_cli_command(line: &str) -> Result<CacheRequest, Error> {
    let tokens = tokenize_cli(line);
    let Some(cmd) = tokens.first() else {
        return Err(cache_err("empty command"));
    };
    let upper = cmd.to_ascii_uppercase();
    match upper.as_str() {
        "PING" => Ok(CacheRequest::Ping(tokens.get(1).cloned())),
        "GET" => {
            let key = tokens
                .get(1)
                .ok_or_else(|| cache_err("usage: GET <key>"))?
                .clone();
            Ok(CacheRequest::Get { key })
        }
        "SET" => {
            let key = tokens
                .get(1)
                .ok_or_else(|| cache_err("usage: SET <key> <value> [EX <sec> | PX <ms>]"))?
                .clone();
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
                match opt.to_ascii_uppercase().as_str() {
                    "EX" => ttl_ms = Some(n * 1000),
                    "PX" => ttl_ms = Some(n),
                    other => return Err(cache_err(format!("unknown SET option {other}"))),
                }
            }
            Ok(CacheRequest::Set {
                key,
                value: val,
                ttl_ms,
            })
        }
        "DEL" => {
            if tokens.len() < 2 {
                return Err(cache_err("usage: DEL <key> [key ...]"));
            }
            Ok(CacheRequest::Del {
                keys: tokens[1..].to_vec(),
            })
        }
        "EXISTS" => {
            let key = tokens
                .get(1)
                .ok_or_else(|| cache_err("usage: EXISTS <key>"))?
                .clone();
            Ok(CacheRequest::Exists { key })
        }
        "INCR" => {
            let key = tokens
                .get(1)
                .ok_or_else(|| cache_err("usage: INCR <key>"))?
                .clone();
            Ok(CacheRequest::IncrBy { key, delta: 1 })
        }
        "DECR" => {
            let key = tokens
                .get(1)
                .ok_or_else(|| cache_err("usage: DECR <key>"))?
                .clone();
            Ok(CacheRequest::IncrBy { key, delta: -1 })
        }
        "INCRBY" => {
            let key = tokens
                .get(1)
                .ok_or_else(|| cache_err("usage: INCRBY <key> <delta>"))?
                .clone();
            let delta: i64 = tokens
                .get(2)
                .ok_or_else(|| cache_err("usage: INCRBY <key> <delta>"))?
                .parse()
                .map_err(cache_err)?;
            Ok(CacheRequest::IncrBy { key, delta })
        }
        "EXPIRE" => {
            let key = tokens
                .get(1)
                .ok_or_else(|| cache_err("usage: EXPIRE <key> <seconds>"))?
                .clone();
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
        "PEXPIRE" => {
            let key = tokens
                .get(1)
                .ok_or_else(|| cache_err("usage: PEXPIRE <key> <ms>"))?
                .clone();
            let ttl_ms: u64 = tokens
                .get(2)
                .ok_or_else(|| cache_err("usage: PEXPIRE <key> <ms>"))?
                .parse()
                .map_err(cache_err)?;
            Ok(CacheRequest::Expire { key, ttl_ms })
        }
        "PERSIST" => {
            let key = tokens
                .get(1)
                .ok_or_else(|| cache_err("usage: PERSIST <key>"))?
                .clone();
            Ok(CacheRequest::Persist { key })
        }
        "TTL" | "PTTL" => {
            let key = tokens
                .get(1)
                .ok_or_else(|| cache_err("usage: TTL <key>"))?
                .clone();
            Ok(CacheRequest::Ttl { key })
        }
        "KEYS" | "SCAN" => {
            let prefix = tokens.get(1).cloned().unwrap_or_else(|| "*".into());
            Ok(CacheRequest::Keys { prefix })
        }
        "FLUSHALL" | "FLUSHDB" => Ok(CacheRequest::FlushAll),

        // Hashes
        "HSET" => {
            let key = tokens
                .get(1)
                .ok_or_else(|| cache_err("usage: HSET <key> <field> <value>"))?
                .clone();
            let field = tokens
                .get(2)
                .ok_or_else(|| cache_err("usage: HSET <key> <field> <value>"))?
                .clone();
            let value = tokens
                .get(3)
                .ok_or_else(|| cache_err("usage: HSET <key> <field> <value>"))?
                .as_bytes()
                .to_vec();
            Ok(CacheRequest::HSet { key, field, value })
        }
        "HGET" => {
            let key = tokens
                .get(1)
                .ok_or_else(|| cache_err("usage: HGET <key> <field>"))?
                .clone();
            let field = tokens
                .get(2)
                .ok_or_else(|| cache_err("usage: HGET <key> <field>"))?
                .clone();
            Ok(CacheRequest::HGet { key, field })
        }
        "HDEL" => {
            if tokens.len() < 3 {
                return Err(cache_err("usage: HDEL <key> <field> [field ...]"));
            }
            Ok(CacheRequest::HDel {
                key: tokens[1].clone(),
                fields: tokens[2..].to_vec(),
            })
        }
        "HGETALL" => {
            let key = tokens
                .get(1)
                .ok_or_else(|| cache_err("usage: HGETALL <key>"))?
                .clone();
            Ok(CacheRequest::HGetAll { key })
        }

        // Lists
        "LPUSH" => {
            if tokens.len() < 3 {
                return Err(cache_err("usage: LPUSH <key> <val> [val ...]"));
            }
            Ok(CacheRequest::LPush {
                key: tokens[1].clone(),
                values: tokens[2..].iter().map(|s| s.as_bytes().to_vec()).collect(),
            })
        }
        "RPUSH" => {
            if tokens.len() < 3 {
                return Err(cache_err("usage: RPUSH <key> <val> [val ...]"));
            }
            Ok(CacheRequest::RPush {
                key: tokens[1].clone(),
                values: tokens[2..].iter().map(|s| s.as_bytes().to_vec()).collect(),
            })
        }
        "LPOP" => {
            let key = tokens
                .get(1)
                .ok_or_else(|| cache_err("usage: LPOP <key>"))?
                .clone();
            Ok(CacheRequest::LPop { key })
        }
        "RPOP" => {
            let key = tokens
                .get(1)
                .ok_or_else(|| cache_err("usage: RPOP <key>"))?
                .clone();
            Ok(CacheRequest::RPop { key })
        }
        "LRANGE" => {
            let key = tokens
                .get(1)
                .ok_or_else(|| cache_err("usage: LRANGE <key> <start> <stop>"))?
                .clone();
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
        "SADD" => {
            if tokens.len() < 3 {
                return Err(cache_err("usage: SADD <key> <member> [member ...]"));
            }
            Ok(CacheRequest::SAdd {
                key: tokens[1].clone(),
                members: tokens[2..].to_vec(),
            })
        }
        "SREM" => {
            if tokens.len() < 3 {
                return Err(cache_err("usage: SREM <key> <member> [member ...]"));
            }
            Ok(CacheRequest::SRem {
                key: tokens[1].clone(),
                members: tokens[2..].to_vec(),
            })
        }
        "SMEMBERS" => {
            let key = tokens
                .get(1)
                .ok_or_else(|| cache_err("usage: SMEMBERS <key>"))?
                .clone();
            Ok(CacheRequest::SMembers { key })
        }
        "SISMEMBER" => {
            let key = tokens
                .get(1)
                .ok_or_else(|| cache_err("usage: SISMEMBER <key> <member>"))?
                .clone();
            let member = tokens
                .get(2)
                .ok_or_else(|| cache_err("usage: SISMEMBER <key> <member>"))?
                .clone();
            Ok(CacheRequest::SIsMember { key, member })
        }

        // Pub/Sub
        "PUBLISH" => {
            let channel = tokens
                .get(1)
                .ok_or_else(|| cache_err("usage: PUBLISH <channel> <message>"))?
                .clone();
            let message = tokens
                .get(2)
                .ok_or_else(|| cache_err("usage: PUBLISH <channel> <message>"))?
                .as_bytes()
                .to_vec();
            Ok(CacheRequest::Publish { channel, message })
        }
        "SUBSCRIBE" => {
            if tokens.len() < 2 {
                return Err(cache_err("usage: SUBSCRIBE <channel> [channel ...]"));
            }
            Ok(CacheRequest::Subscribe {
                channels: tokens[1..].to_vec(),
            })
        }

        // Native Lin Pipe Query
        "LIN" | "QUERY" => {
            let rest = line
                .trim_start()
                .split_once(char::is_whitespace)
                .map(|x| x.1.trim())
                .unwrap_or("");
            if rest.is_empty() {
                return Err(cache_err("usage: LIN <lin-pipe-query>"));
            }
            Ok(CacheRequest::LinQuery {
                query: rest.to_string(),
            })
        }

        // Cluster & Raft
        "INFO" => Ok(CacheRequest::ClusterInfo),
        "CLUSTER" => {
            let sub = tokens
                .get(1)
                .map(|s| s.to_ascii_uppercase())
                .unwrap_or_else(|| "INFO".into());
            match sub.as_str() {
                "INFO" => Ok(CacheRequest::ClusterInfo),
                "NODES" => Ok(CacheRequest::ClusterNodes),
                "MEET" => {
                    let node_id = tokens
                        .get(2)
                        .ok_or_else(|| cache_err("usage: CLUSTER MEET <node_id> <host:port>"))?
                        .clone();
                    let addr = tokens
                        .get(3)
                        .ok_or_else(|| cache_err("usage: CLUSTER MEET <node_id> <host:port>"))?
                        .clone();
                    Ok(CacheRequest::ClusterMeet { node_id, addr })
                }
                "KEYSLOT" | "SHARD" => {
                    let key = tokens
                        .get(2)
                        .ok_or_else(|| cache_err("usage: CLUSTER KEYSLOT <key>"))?
                        .clone();
                    Ok(CacheRequest::ClusterKeySlot { key })
                }
                other => Err(cache_err(format!(
                    "unknown CLUSTER subcommand {other} (INFO|NODES|MEET|KEYSLOT)"
                ))),
            }
        }
        "RAFT" => {
            let sub = tokens
                .get(1)
                .map(|s| s.to_ascii_uppercase())
                .unwrap_or_else(|| "STATUS".into());
            match sub.as_str() {
                "STATUS" => Ok(CacheRequest::RaftStatus),
                "ELECT" | "ELECTION" => Ok(CacheRequest::RaftTriggerElection),
                other => Err(cache_err(format!(
                    "unknown RAFT subcommand {other} (STATUS|ELECT)"
                ))),
            }
        }
        other => Err(cache_err(format!("unknown command '{other}'"))),
    }
}
