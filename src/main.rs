use std::io::{self, Read};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::ExitCode;
use std::thread;
use std::time::Duration;

use lin::{Db, GraphFmt, explain_as};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (data, follower, rest) = parse_global(&args);
    match rest.first().map(String::as_str) {
        Some("explain") => cmd_explain(&rest[1..], data, follower),
        Some("run") => cmd_run(&rest[1..], data, follower),
        Some("backup") => cmd_backup(&rest[1..], data),
        Some("apply-wal") => cmd_apply_wal(&rest[1..], data, follower),
        Some("wal-serve") => cmd_wal_serve(&rest[1..], data),
        Some("follower") => cmd_follower(&rest[1..], data),
        Some("cache") | Some("cacha") => cmd_cache(&rest[1..], data),
        Some("stats") => cmd_stats(data, follower),
        Some("version") | Some("--version") => {
            println!("lin {}", lin::VERSION);
            ExitCode::SUCCESS
        }
        _ => usage(2),
    }
}

fn parse_global(args: &[String]) -> (Option<PathBuf>, bool, Vec<String>) {
    let mut data = None;
    let mut follower = false;
    let mut rest = Vec::new();
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--data" {
            i += 1;
            match args.get(i) {
                Some(p) => {
                    data = Some(PathBuf::from(p));
                    i += 1;
                }
                None => data = Some(PathBuf::from(".lin")),
            }
        } else if args[i] == "--follower" {
            follower = true;
            i += 1;
        } else {
            rest.push(args[i].clone());
            i += 1;
        }
    }
    (data, follower, rest)
}

fn cmd_explain(args: &[String], data: Option<PathBuf>, follower: bool) -> ExitCode {
    let mut graph = None;
    let mut rest = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--graph" => {
                i += 1;
                match args.get(i).map(String::as_str) {
                    Some("mermaid") | Some("graph") => graph = Some(GraphFmt::Mermaid),
                    Some("dot") => graph = Some(GraphFmt::Dot),
                    Some(other) => {
                        eprintln!("unknown --graph {other:?} (mermaid|dot)");
                        return ExitCode::from(2);
                    }
                    None => {
                        eprintln!("usage: lin explain --graph mermaid|dot '<query>'");
                        return ExitCode::from(2);
                    }
                }
                i += 1;
            }
            "--" => {
                rest.extend(args[i + 1..].iter().cloned());
                break;
            }
            s => {
                rest.push(s.to_string());
                i += 1;
            }
        }
    }
    let q = rest.join(" ");
    if q.is_empty() {
        eprintln!("usage: lin [--data <dir>] explain [--graph mermaid|dot] '<query>'");
        return ExitCode::from(2);
    }
    match with_data(data, follower, |db| match db {
        Some(db) => db.explain_as(&q, graph),
        None => explain_as(&q, graph),
    }) {
        Ok(s) => {
            print!("{s}");
            if !s.ends_with('\n') {
                println!();
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{e}");
            ExitCode::from(1)
        }
    }
}

fn cmd_run(args: &[String], data: Option<PathBuf>, follower: bool) -> ExitCode {
    let mut show_plan = false;
    let mut use_gpu = false;
    let mut file: Option<PathBuf> = None;
    let mut rest = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--gpu" => {
                use_gpu = true;
                i += 1;
            }
            "--explain" => {
                show_plan = true;
                i += 1;
            }
            "--file" => {
                i += 1;
                match args.get(i) {
                    Some(p) => {
                        file = Some(PathBuf::from(p));
                        i += 1;
                    }
                    None => {
                        eprintln!(
                            "usage: lin [--data <dir>] run [--gpu] [--explain] [--file <path> | - | '<query>']"
                        );
                        return ExitCode::from(2);
                    }
                }
            }
            "--" => {
                rest.extend(args[i + 1..].iter().cloned());
                break;
            }
            s => {
                rest.push(s.to_string());
                i += 1;
            }
        }
    }
    let q = match load_program(file, &rest) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(2);
        }
    };
    if q.trim().is_empty() {
        eprintln!(
            "usage: lin [--data <dir>] run [--gpu] [--explain] [--file <path> | - | '<query>']"
        );
        return ExitCode::from(2);
    }
    #[cfg(not(feature = "gpu"))]
    if use_gpu {
        eprintln!("GPU support requires building Lin with --features gpu");
        return ExitCode::from(2);
    }
    #[cfg(feature = "gpu")]
    let gpu = if use_gpu {
        match lin::gpu::GpuCompute::new() {
            Ok(gpu) => Some(std::sync::Arc::new(gpu)),
            Err(error) => {
                eprintln!("{error}");
                return ExitCode::from(1);
            }
        }
    } else {
        None
    };
    match with_data(data, follower, |db| match db {
        Some(db) => {
            #[cfg(feature = "gpu")]
            if let Some(gpu) = &gpu {
                db.enable_gpu(gpu.clone());
            }
            let h = db.run(&q)?;
            let plan = if show_plan {
                Some(db.explain_as(&q, None)?)
            } else {
                None
            };
            Ok((h, plan))
        }
        None => {
            #[cfg(feature = "gpu")]
            if let Some(gpu) = &gpu {
                let mut db = Db::fixture().with_gpu(gpu.clone());
                let h = db.run(&q)?;
                let plan = if show_plan {
                    Some(db.explain_as(&q, None)?)
                } else {
                    None
                };
                return Ok((h, plan));
            }
            let h = lin::run(&q)?;
            let plan = if show_plan {
                Some(explain_as(&q, None)?)
            } else {
                None
            };
            Ok((h, plan))
        }
    }) {
        Ok((h, plan)) => {
            println!("{}", h.compact());
            if let Some(p) = plan {
                println!();
                print!("{p}");
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{e}");
            ExitCode::from(1)
        }
    }
}

fn with_data<T>(
    data: Option<PathBuf>,
    follower: bool,
    f: impl FnOnce(Option<&mut Db>) -> Result<T, lin::Error>,
) -> Result<T, lin::Error> {
    match data {
        Some(path) => {
            let mut db = if follower {
                Db::open_follower(path)?
            } else {
                Db::open(path)?
            };
            let out = f(Some(&mut db));
            let close = db.close();
            let result = match (out, close) {
                (Ok(v), Ok(())) => Ok(v),
                (Err(e), _) => Err(e),
                (Ok(_), Err(e)) => Err(e),
            };
            drop(db);
            result
        }
        None => {
            if follower {
                return Err(lin::Error::runtime("--follower requires --data <dir>"));
            }
            f(None)
        }
    }
}

fn load_program(file: Option<PathBuf>, rest: &[String]) -> Result<String, String> {
    if let Some(path) = file {
        return std::fs::read_to_string(&path).map_err(|e| format!("read {}: {e}", path.display()));
    }
    let joined = rest.join(" ");
    if joined == "-" {
        let mut buf = String::new();
        io::stdin()
            .read_to_string(&mut buf)
            .map_err(|e| format!("read stdin: {e}"))?;
        return Ok(buf);
    }
    Ok(joined)
}

fn usage(code: u8) -> ExitCode {
    eprintln!(
        "usage:\n  \
         lin [--data <dir>] [--follower] run [--gpu] [--explain] [--file <path> | - | '<query>']\n  \
         lin [--data <dir>] [--follower] explain [--graph mermaid|dot] '<query>'\n  \
         lin --data <dir> wal-serve [--listen HOST:PORT] [--token TOKEN] [--tls-cert PEM --tls-key PEM]\n  \
         lin --data <dir> follower status|bootstrap|sync …\n  \
         lin --data <dir> --follower apply-wal <frames.bin | ->\n  \
         lin [--data <dir>] backup export <file.json>\n  \
         lin backup import <file.json> [--data <dir>]\n  \
         lin [--data <dir>] cache serve [--node-id ID] [--listen HOST:PORT] [--peer ID=HOST:PORT] [--max-entries N]\n  \
         lin cache cli [--connect HOST:PORT] [COMMAND...]\n  \
         lin [--data <dir>] [--follower] stats\n  \
         lin version"
    );
    ExitCode::from(code)
}

fn cmd_stats(data: Option<PathBuf>, follower: bool) -> ExitCode {
    match with_data(data, follower, |db| {
        let s = match db {
            Some(db) => db.stats(),
            None => {
                let tmp = Db::fixture();
                tmp.stats()
            }
        };
        Ok(s)
    }) {
        Ok(s) => {
            println!(
                "gen={} docs={} facts={} edges={} next_id={} log_bytes={} reopen_ms={} reopen_setup_ms={:.3} reopen_snapshot_ms={:.3} reopen_wal_ms={:.3} reopen_metadata_ms={:.3} reopen_indexes_ms={:.3} reopen_row_maps_ms={:.3} reopen_fts_ms={:.3} writes_since_snapshot={} sync_normal={} cold={} append_rows={} append_rows_per_s={:.0}",
                s.r#gen,
                s.docs,
                s.facts,
                s.edges,
                s.next_id,
                s.log_bytes,
                s.reopen_ms,
                s.reopen.setup_ms,
                s.reopen.snapshot_ms,
                s.reopen.wal_ms,
                s.reopen.metadata_ms,
                s.reopen.indexes_ms,
                s.reopen.row_maps_ms,
                s.reopen.fts_ms,
                s.writes_since_snapshot,
                s.sync_normal,
                s.cold,
                s.append_rows,
                s.append_rows_per_s()
            );
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{e}");
            ExitCode::from(1)
        }
    }
}

fn cmd_apply_wal(args: &[String], data: Option<PathBuf>, follower: bool) -> ExitCode {
    let Some(dir) = data else {
        eprintln!("usage: lin --data <dir> --follower apply-wal <frames.bin | ->");
        return ExitCode::from(2);
    };
    if !follower {
        eprintln!("apply-wal requires --follower (refuses durable primary)");
        return ExitCode::from(2);
    }
    let src = args.first().map(String::as_str).unwrap_or("-");
    let frames = if src == "-" {
        let mut buf = Vec::new();
        match io::stdin().read_to_end(&mut buf) {
            Ok(_) => buf,
            Err(e) => {
                eprintln!("read stdin: {e}");
                return ExitCode::from(1);
            }
        }
    } else {
        match std::fs::read(src) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("read {src}: {e}");
                return ExitCode::from(1);
            }
        }
    };
    match Db::open_follower(&dir) {
        Ok(mut db) => {
            let since = db.r#gen();
            match db.apply_wal(&frames) {
                Ok(n) => {
                    let new_gen = db.r#gen();
                    let _ = db.close();
                    println!(
                        "applied {n} frames since={since} gen={new_gen} → {}",
                        dir.display()
                    );
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("{e}");
                    ExitCode::from(1)
                }
            }
        }
        Err(e) => {
            eprintln!("{e}");
            ExitCode::from(1)
        }
    }
}

fn cmd_wal_serve(args: &[String], data: Option<PathBuf>) -> ExitCode {
    let Some(dir) = data else {
        eprintln!(
            "usage: lin --data <dir> wal-serve [--listen HOST:PORT] [--token TOKEN] [--tls-cert PEM --tls-key PEM]"
        );
        return ExitCode::from(2);
    };
    let mut listen = "127.0.0.1:9876".to_string();
    let mut token: Option<String> = std::env::var("LIN_SHIP_TOKEN")
        .ok()
        .filter(|s| !s.is_empty());
    let mut cert: Option<PathBuf> = None;
    let mut key: Option<PathBuf> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--listen" => {
                i += 1;
                match args.get(i) {
                    Some(a) => {
                        listen = a.clone();
                        i += 1;
                    }
                    None => {
                        eprintln!(
                            "usage: lin --data <dir> wal-serve [--listen HOST:PORT] [--token TOKEN] [--tls-cert PEM --tls-key PEM]"
                        );
                        return ExitCode::from(2);
                    }
                }
            }
            "--token" => {
                i += 1;
                match args.get(i) {
                    Some(a) => {
                        token = Some(a.clone());
                        i += 1;
                    }
                    None => {
                        eprintln!("--token requires a value");
                        return ExitCode::from(2);
                    }
                }
            }
            "--tls-cert" => {
                i += 1;
                match args.get(i) {
                    Some(a) => {
                        cert = Some(PathBuf::from(a));
                        i += 1;
                    }
                    None => {
                        eprintln!("--tls-cert requires a path");
                        return ExitCode::from(2);
                    }
                }
            }
            "--tls-key" => {
                i += 1;
                match args.get(i) {
                    Some(a) => {
                        key = Some(PathBuf::from(a));
                        i += 1;
                    }
                    None => {
                        eprintln!("--tls-key requires a path");
                        return ExitCode::from(2);
                    }
                }
            }
            other => {
                eprintln!("unknown wal-serve arg {other}");
                return ExitCode::from(2);
            }
        }
    }
    let tls = match (cert, key) {
        (Some(c), Some(k)) => match (std::fs::read(&c), std::fs::read(&k)) {
            (Ok(cert_pem), Ok(key_pem)) => Some(lin::ship::TlsServer { cert_pem, key_pem }),
            (Err(e), _) => {
                eprintln!("tls-cert: {e}");
                return ExitCode::from(1);
            }
            (_, Err(e)) => {
                eprintln!("tls-key: {e}");
                return ExitCode::from(1);
            }
        },
        (None, None) => None,
        _ => {
            eprintln!("tls requires both --tls-cert and --tls-key");
            return ExitCode::from(2);
        }
    };
    if tls.is_none() && !listen.starts_with("127.") && !listen.starts_with("[::1]") {
        eprintln!("warning: plaintext wal-serve on {listen}; use --tls-cert/--tls-key and --token");
    }
    let listener = match TcpListener::bind(&listen) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("bind {listen}: {e}");
            return ExitCode::from(1);
        }
    };
    let addr = listener
        .local_addr()
        .map(|a| a.to_string())
        .unwrap_or(listen);
    eprintln!(
        "wal-serve {} on {addr} tls={} token={} one-sink (writer lock held)",
        dir.display(),
        tls.is_some(),
        token.is_some()
    );
    let opts = lin::ship::ServeOpts { token, tls };
    match Db::open(&dir) {
        Ok(db) => match lin::ship::serve_blocking_opts(&db, listener, &opts) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("{e}");
                ExitCode::from(1)
            }
        },
        Err(e) => {
            eprintln!("{e}");
            ExitCode::from(1)
        }
    }
}

fn cmd_follower(args: &[String], data: Option<PathBuf>) -> ExitCode {
    match args.first().map(String::as_str) {
        Some("status") => {
            let Some(dir) = data else {
                eprintln!("usage: lin --data <dir> follower status");
                return ExitCode::from(2);
            };
            match Db::open_follower(&dir) {
                Ok(mut db) => {
                    let s = db.stats();
                    println!(
                        "follower={} gen={} docs={} facts={} edges={} log_bytes={} embed_id={}",
                        db.is_follower(),
                        s.r#gen,
                        s.docs,
                        s.facts,
                        s.edges,
                        s.log_bytes,
                        db.store.embed_id
                    );
                    let _ = db.close();
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("{e}");
                    ExitCode::from(1)
                }
            }
        }
        Some("bootstrap") => {
            let mut backup: Option<PathBuf> = None;
            let mut i = 1;
            while i < args.len() {
                if args[i] == "--backup" {
                    i += 1;
                    match args.get(i) {
                        Some(p) => {
                            backup = Some(PathBuf::from(p));
                            i += 1;
                        }
                        None => break,
                    }
                } else {
                    i += 1;
                }
            }
            let Some(dir) = data else {
                eprintln!("usage: lin --data <dir> follower bootstrap --backup <file>");
                return ExitCode::from(2);
            };
            let Some(bak) = backup else {
                eprintln!("usage: lin --data <dir> follower bootstrap --backup <file>");
                return ExitCode::from(2);
            };
            match Db::bootstrap_follower(&bak, &dir) {
                Ok(mut db) => {
                    let s = db.stats();
                    println!(
                        "bootstrapped {} → {}  gen={} docs={}",
                        bak.display(),
                        dir.display(),
                        s.r#gen,
                        s.docs
                    );
                    let _ = db.close();
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("{e}");
                    ExitCode::from(1)
                }
            }
        }
        Some("sync") => {
            let Some(dir) = data else {
                eprintln!(
                    "usage: lin --data <dir> follower sync --from HOST:PORT [--loop SECS] [--token TOKEN] [--tls-ca PEM | --tls-insecure]"
                );
                return ExitCode::from(2);
            };
            let mut from: Option<String> = None;
            let mut loop_secs: Option<u64> = None;
            let mut token: Option<String> = std::env::var("LIN_SHIP_TOKEN")
                .ok()
                .filter(|s| !s.is_empty());
            let mut tls_ca: Option<PathBuf> = None;
            let mut tls_insecure = false;
            let mut i = 1;
            while i < args.len() {
                match args[i].as_str() {
                    "--from" => {
                        i += 1;
                        match args.get(i) {
                            Some(a) => {
                                from = Some(a.clone());
                                i += 1;
                            }
                            None => break,
                        }
                    }
                    "--loop" => {
                        i += 1;
                        match args.get(i).and_then(|s| s.parse().ok()) {
                            Some(n) => {
                                loop_secs = Some(n);
                                i += 1;
                            }
                            None => {
                                eprintln!("--loop requires seconds");
                                return ExitCode::from(2);
                            }
                        }
                    }
                    "--token" => {
                        i += 1;
                        match args.get(i) {
                            Some(a) => {
                                token = Some(a.clone());
                                i += 1;
                            }
                            None => {
                                eprintln!("--token requires a value");
                                return ExitCode::from(2);
                            }
                        }
                    }
                    "--tls-ca" => {
                        i += 1;
                        match args.get(i) {
                            Some(a) => {
                                tls_ca = Some(PathBuf::from(a));
                                i += 1;
                            }
                            None => {
                                eprintln!("--tls-ca requires a path");
                                return ExitCode::from(2);
                            }
                        }
                    }
                    "--tls-insecure" => {
                        tls_insecure = true;
                        i += 1;
                    }
                    other => {
                        eprintln!("unknown follower sync arg {other}");
                        return ExitCode::from(2);
                    }
                }
            }
            let Some(addr) = from else {
                eprintln!(
                    "usage: lin --data <dir> follower sync --from HOST:PORT [--loop SECS] [--token TOKEN] [--tls-ca PEM | --tls-insecure]"
                );
                return ExitCode::from(2);
            };
            let tls = if tls_insecure {
                lin::ship::TlsClient::Insecure
            } else if let Some(p) = tls_ca {
                match std::fs::read(&p) {
                    Ok(pem) => lin::ship::TlsClient::CaPem(pem),
                    Err(e) => {
                        eprintln!("tls-ca: {e}");
                        return ExitCode::from(1);
                    }
                }
            } else {
                lin::ship::TlsClient::Off
            };
            let pull = lin::ship::PullOpts {
                token,
                tls,
                server_name: lin::ship::tls_server_name(&addr),
            };
            loop {
                match sync_once(&dir, &addr, &pull) {
                    Ok(()) => {}
                    Err(code) => return code,
                }
                match loop_secs {
                    Some(secs) => thread::sleep(Duration::from_secs(secs.max(1))),
                    None => return ExitCode::SUCCESS,
                }
            }
        }
        _ => {
            eprintln!(
                "usage:\n  \
                 lin --data <dir> follower status\n  \
                 lin --data <dir> follower bootstrap --backup <file>\n  \
                 lin --data <dir> follower sync --from HOST:PORT [--loop SECS] [--token TOKEN] [--tls-ca PEM | --tls-insecure]"
            );
            ExitCode::from(2)
        }
    }
}

fn sync_once(dir: &PathBuf, addr: &str, pull: &lin::ship::PullOpts) -> Result<(), ExitCode> {
    let mut db = Db::open_follower(dir).map_err(|e| {
        eprintln!("{e}");
        ExitCode::from(1)
    })?;
    let since = db.r#gen();
    let frames = lin::ship::pull_with(addr, since, pull).map_err(|e| {
        eprintln!("{e}");
        ExitCode::from(1)
    })?;
    let n = db.apply_wal(&frames).map_err(|e| {
        eprintln!("{e}");
        if e.to_string().contains("gap") {
            eprintln!("hint: re-bootstrap from a fresh primary backup");
        }
        ExitCode::from(1)
    })?;
    let new_gen = db.r#gen();
    let _ = db.close();
    println!("sync applied={n} since={since} gen={new_gen} from={addr}");
    Ok(())
}

fn cmd_backup(args: &[String], data: Option<PathBuf>) -> ExitCode {
    match args.first().map(String::as_str) {
        Some("export") => {
            let Some(path) = args.get(1) else {
                eprintln!("usage: lin [--data <dir>] backup export <file.json>");
                return ExitCode::from(2);
            };
            match with_data(data, false, |db| match db {
                Some(db) => db.export_backup(path),
                None => {
                    let mut tmp = Db::fixture();
                    tmp.export_backup(path)
                }
            }) {
                Ok(()) => {
                    println!("exported {path}");
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("{e}");
                    ExitCode::from(1)
                }
            }
        }
        Some("import") => {
            let Some(path) = args.get(1) else {
                eprintln!("usage: lin backup import <file.json> [--data <dir>]");
                return ExitCode::from(2);
            };
            // Allow `--data` after import path as well.
            let mut data = data;
            let mut i = 2;
            while i < args.len() {
                if args[i] == "--data" {
                    i += 1;
                    data = Some(PathBuf::from(
                        args.get(i).map(String::as_str).unwrap_or(".lin"),
                    ));
                    i += 1;
                } else {
                    i += 1;
                }
            }
            match data {
                Some(dir) => match Db::import_backup_into(path, &dir) {
                    Ok(mut db) => {
                        let s = db.stats();
                        let _ = db.close();
                        println!(
                            "imported {path} → {}  gen={} docs={}",
                            dir.display(),
                            s.r#gen,
                            s.docs
                        );
                        ExitCode::SUCCESS
                    }
                    Err(e) => {
                        eprintln!("{e}");
                        ExitCode::from(1)
                    }
                },
                None => match Db::import_backup(path) {
                    Ok(db) => {
                        let s = db.stats();
                        println!(
                            "imported {path} (memory) gen={} docs={} facts={} edges={}",
                            s.r#gen, s.docs, s.facts, s.edges
                        );
                        ExitCode::SUCCESS
                    }
                    Err(e) => {
                        eprintln!("{e}");
                        ExitCode::from(1)
                    }
                },
            }
        }
        _ => {
            eprintln!("usage: lin backup export|import …");
            ExitCode::from(2)
        }
    }
}

fn cmd_cache(args: &[String], data: Option<PathBuf>) -> ExitCode {
    match args.first().map(String::as_str) {
        Some("serve") => {
            let mut cfg = lin::cache::CacheConfig::default();
            let mut peers: Vec<(String, String)> = Vec::new();
            let mut i = 1;
            while i < args.len() {
                match args[i].as_str() {
                    "--node-id" | "--id" => {
                        i += 1;
                        let Some(v) = args.get(i) else {
                            eprintln!("--node-id requires an identifier");
                            return ExitCode::from(2);
                        };
                        cfg.node_id = v.clone();
                        i += 1;
                    }
                    "--listen" | "--bind" => {
                        i += 1;
                        let Some(v) = args.get(i) else {
                            eprintln!("--listen requires HOST:PORT");
                            return ExitCode::from(2);
                        };
                        cfg.listen_addr = v.clone();
                        i += 1;
                    }
                    "--peer" => {
                        i += 1;
                        let Some(v) = args.get(i) else {
                            eprintln!("--peer requires ID=HOST:PORT");
                            return ExitCode::from(2);
                        };
                        if let Some((pid, paddr)) = v.split_once('=') {
                            peers.push((pid.to_string(), paddr.to_string()));
                        } else {
                            peers.push((v.clone(), v.clone()));
                        }
                        i += 1;
                    }
                    "--max-entries" => {
                        i += 1;
                        let Some(v) = args.get(i).and_then(|s| s.parse().ok()) else {
                            eprintln!("--max-entries requires a number");
                            return ExitCode::from(2);
                        };
                        cfg.max_entries = v;
                        i += 1;
                    }
                    "--max-bytes" => {
                        i += 1;
                        let Some(v) = args.get(i).and_then(|s| s.parse().ok()) else {
                            eprintln!("--max-bytes requires a number");
                            return ExitCode::from(2);
                        };
                        cfg.max_bytes = v;
                        i += 1;
                    }
                    "--vnodes" => {
                        i += 1;
                        let Some(v) = args.get(i).and_then(|s| s.parse().ok()) else {
                            eprintln!("--vnodes requires a number");
                            return ExitCode::from(2);
                        };
                        cfg.vnodes = v;
                        i += 1;
                    }
                    "--replicas" => {
                        i += 1;
                        let Some(v) = args.get(i).and_then(|s| s.parse().ok()) else {
                            eprintln!("--replicas requires a number");
                            return ExitCode::from(2);
                        };
                        cfg.replication_factor = v;
                        i += 1;
                    }
                    "--no-sharding" => {
                        cfg.sharding_enabled = false;
                        i += 1;
                    }
                    other => {
                        eprintln!("unknown cache serve option: {other}");
                        return ExitCode::from(2);
                    }
                }
            }

            let listener = match TcpListener::bind(&cfg.listen_addr) {
                Ok(l) => l,
                Err(e) => {
                    eprintln!("bind {}: {e}", cfg.listen_addr);
                    return ExitCode::from(1);
                }
            };
            if let Ok(local) = listener.local_addr() {
                cfg.listen_addr = local.to_string();
            }

            let node_res = match &data {
                Some(dir) => lin::cache::CacheNode::open_durable(dir, cfg.clone()),
                None => lin::cache::CacheNode::new_in_memory(cfg.clone()),
            };
            let node = match node_res {
                Ok(n) => n,
                Err(e) => {
                    eprintln!("{e}");
                    return ExitCode::from(1);
                }
            };

            for (pid, paddr) in &peers {
                node.meet(pid.clone(), paddr.clone());
                let _ = node.gossip_once_with(paddr);
            }

            eprintln!(
                "cacha node={} listening on {} durable={} peers={}",
                cfg.node_id,
                cfg.listen_addr,
                data.is_some(),
                peers.len()
            );

            match node.serve(listener) {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("{e}");
                    ExitCode::from(1)
                }
            }
        }
        Some("cli") => {
            let mut connect = "127.0.0.1:6380".to_string();
            let mut cmd_tokens: Vec<String> = Vec::new();
            let mut i = 1;
            while i < args.len() {
                match args[i].as_str() {
                    "--connect" | "-c" | "-h" => {
                        i += 1;
                        let Some(v) = args.get(i) else {
                            eprintln!("--connect requires HOST:PORT");
                            return ExitCode::from(2);
                        };
                        connect = v.clone();
                        i += 1;
                    }
                    "--" => {
                        cmd_tokens.extend(args[i + 1..].iter().cloned());
                        break;
                    }
                    other => {
                        cmd_tokens.push(other.to_string());
                        i += 1;
                    }
                }
            }

            let mut client = match lin::cache::CacheClient::connect(&connect) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("connect {connect}: {e}");
                    return ExitCode::from(1);
                }
            };

            if !cmd_tokens.is_empty() {
                let line = if cmd_tokens.len() == 1 {
                    cmd_tokens[0].clone()
                } else if cmd_tokens[0].eq_ignore_ascii_case("lin")
                    || cmd_tokens[0].eq_ignore_ascii_case("query")
                {
                    cmd_tokens.join(" ")
                } else {
                    cmd_tokens
                        .iter()
                        .map(|t| {
                            if t.contains(char::is_whitespace) {
                                format!("{t:?}")
                            } else {
                                t.clone()
                            }
                        })
                        .collect::<Vec<_>>()
                        .join(" ")
                };
                match lin::cache::parse_cli_command(&line) {
                    Ok(req) => {
                        let is_sub = matches!(req, lin::cache::CacheRequest::Subscribe { .. });
                        match client.send(&req) {
                            Ok(resp) => {
                                println!("{}", resp.format_cli());
                                if is_sub {
                                    while let Ok(msg) = client.read_next() {
                                        println!("{}", msg.format_cli());
                                    }
                                }
                                return ExitCode::SUCCESS;
                            }
                            Err(e) => {
                                eprintln!("{e}");
                                return ExitCode::from(1);
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("{e}");
                        return ExitCode::from(2);
                    }
                }
            }

            // Interactive REPL mode
            use std::io::{BufRead, Write};
            println!(
                "Connected to cacha (Lin Distributed Cache) at {}. Type HELP or QUIT.",
                client.addr()
            );
            let stdin = io::stdin();
            let mut stdout = io::stdout();
            let mut lines = stdin.lock().lines();
            loop {
                print!("cacha ({})> ", client.addr());
                let _ = stdout.flush();
                let Some(Ok(line)) = lines.next() else {
                    println!();
                    break;
                };
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                if trimmed.eq_ignore_ascii_case("quit") || trimmed.eq_ignore_ascii_case("exit") {
                    break;
                }
                if trimmed.eq_ignore_ascii_case("help") {
                    println!(
                        "Commands:\n  \
                         Strings: GET k | SET k v [EX s | PX ms] | DEL k.. | EXISTS k | INCR k | DECR k | INCRBY k n | EXPIRE k s | TTL k | KEYS prefix*\n  \
                         Hashes:  HSET k f v | HGET k f | HDEL k f.. | HGETALL k\n  \
                         Lists:   LPUSH k v.. | RPUSH k v.. | LPOP k | RPOP k | LRANGE k start stop\n  \
                         Sets:    SADD k m.. | SREM k m.. | SMEMBERS k | SISMEMBER k m\n  \
                         Pub/Sub: PUBLISH ch msg | SUBSCRIBE ch..\n  \
                         Lin DB:  LIN <pipe-query>   (e.g. `LIN docs | take 3` or `LIN cache_kv | count`)\n  \
                         Cluster: INFO | CLUSTER INFO | CLUSTER NODES | CLUSTER MEET id addr | CLUSTER KEYSLOT k | RAFT STATUS | RAFT ELECT"
                    );
                    continue;
                }
                match lin::cache::parse_cli_command(trimmed) {
                    Ok(req) => {
                        let is_sub = matches!(req, lin::cache::CacheRequest::Subscribe { .. });
                        match client.send(&req) {
                            Ok(resp) => {
                                println!("{}", resp.format_cli());
                                if is_sub {
                                    println!("(listening for messages — Ctrl+C to exit)");
                                    while let Ok(msg) = client.read_next() {
                                        println!("{}", msg.format_cli());
                                    }
                                    break;
                                }
                            }
                            Err(e) => eprintln!("(io error) {e}"),
                        }
                    }
                    Err(e) => eprintln!("(syntax error) {e}"),
                }
            }
            ExitCode::SUCCESS
        }
        _ => {
            eprintln!(
                "usage:\n  \
                 lin [--data <dir>] cache serve [--node-id ID] [--listen HOST:PORT] [--peer ID=HOST:PORT] [--max-entries N]\n  \
                 lin cache cli [--connect HOST:PORT] [COMMAND...]"
            );
            ExitCode::from(2)
        }
    }
}
