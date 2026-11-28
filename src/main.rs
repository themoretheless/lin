use std::io::{self, Read};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use lin::{Db, GraphFmt, explain_as};

/// `--cold`: open the data directory with the disk tier enabled.
static COLD: AtomicBool = AtomicBool::new(false);
/// `--json-errors`: emit structured JSON diagnostics on stderr with stable exit codes.
static JSON_ERRORS: AtomicBool = AtomicBool::new(false);

fn main() -> ExitCode {
    restore_default_sigpipe();
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
        Some("serve") => cmd_serve(&rest[1..], data, follower),
        Some("client") => cmd_client(&rest[1..]),
        Some("shard") => cmd_shard(&rest[1..]),
        Some("remote") => cmd_remote(&rest[1..], data),
        Some("stats") => cmd_stats(data, follower),
        Some("cluster") => cmd_cluster(&rest[1..], data, follower),
        Some("compact") => cmd_compact(&rest[1..], data, follower),
        Some("repair-catalog-hash") => cmd_repair_catalog_hash(data),
        Some("recover") => cmd_recover(&rest[1..], data),
        Some("inspect") => cmd_inspect(&rest[1..], data),
        Some("import") => cmd_import(&rest[1..], data, follower),
        Some("export") => cmd_export(&rest[1..], data, follower),
        Some("health") => cmd_health(data, follower),
        Some("readiness") => cmd_readiness(data, follower),
        Some("config") => cmd_config(&rest[1..], data, follower),
        Some("completions") => cmd_completions(&rest[1..]),
        Some("repl") => cmd_repl(&rest[1..], data, follower),
        Some("version") | Some("--version") => cmd_version(&rest[1..]),
        _ => usage(2),
    }
}

/// The Rust runtime ignores SIGPIPE, so writing to a closed pipe (`lin run ... | head`)
/// turns into a `failed printing to stdout` panic and exit code 101. Command-line tools
/// are expected to stop quietly instead: restore the default action.
fn restore_default_sigpipe() {
    #[cfg(unix)]
    // SAFETY: called first thing in `main`, before any other thread exists; setting a
    // signal's disposition to its default has no other preconditions.
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
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
        } else if args[i] == "--cold" {
            COLD.store(true, Ordering::Relaxed);
            i += 1;
        } else if args[i] == "--json-errors" {
            JSON_ERRORS.store(true, Ordering::Relaxed);
            i += 1;
        } else {
            rest.push(args[i].clone());
            i += 1;
        }
    }
    (data, follower, rest)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RunOutputFormat {
    Compact,
    Table,
    Json,
    Jsonl,
}

fn exit_code_for_error(err: &lin::Error) -> u8 {
    use lin::error::{ErrorCode, Phase};
    match err.code() {
        ErrorCode::Canceled => 130,
        ErrorCode::CorruptData | ErrorCode::UnsupportedFormat | ErrorCode::Io => 4,
        _ if err.message.starts_with("persist:") || err.message.contains("corrupt") => 4,
        _ => match err.phase {
            Phase::Compile => 2,
            Phase::Runtime => 3,
        },
    }
}

fn emit_lin_error(err: &lin::Error) -> ExitCode {
    let code_u8 = exit_code_for_error(err);
    if JSON_ERRORS.load(Ordering::Relaxed) {
        let phase = match err.phase {
            lin::error::Phase::Compile => "compile",
            lin::error::Phase::Runtime => "runtime",
        };
        let payload = serde_json::json!({
            "code": err.code().as_str(),
            "phase": phase,
            "message": err.message,
            "exit_code": code_u8,
        });
        eprintln!("{payload}");
    } else {
        eprintln!("{err}");
    }
    ExitCode::from(code_u8)
}

fn emit_usage_error(msg: &str) -> ExitCode {
    if JSON_ERRORS.load(Ordering::Relaxed) {
        let payload = serde_json::json!({
            "code": "E_USAGE",
            "phase": "cli",
            "message": msg,
            "exit_code": 2,
        });
        eprintln!("{payload}");
    } else {
        eprintln!("{msg}");
    }
    ExitCode::from(2)
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
        Err(e) => emit_lin_error(&e),
    }
}

fn cmd_run(args: &[String], data: Option<PathBuf>, follower: bool) -> ExitCode {
    let mut show_plan = false;
    let mut use_gpu = false;
    let mut format = RunOutputFormat::Compact;
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
            "--format" => {
                i += 1;
                match args.get(i).map(String::as_str) {
                    Some("table") => format = RunOutputFormat::Table,
                    Some("json") => format = RunOutputFormat::Json,
                    Some("jsonl") => format = RunOutputFormat::Jsonl,
                    Some("compact") => format = RunOutputFormat::Compact,
                    Some(other) => {
                        return emit_usage_error(&format!(
                            "unknown --format {other:?} (expected table|json|jsonl)"
                        ));
                    }
                    None => {
                        return emit_usage_error(
                            "usage: lin [--data <dir>] run [--format table|json|jsonl] '<query>'",
                        );
                    }
                }
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
                        return emit_usage_error(
                            "usage: lin [--data <dir>] run [--gpu] [--explain] [--format table|json|jsonl] [--file <path> | - | '<query>']",
                        );
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
        Err(e) => return emit_usage_error(&e),
    };
    if q.trim().is_empty() {
        return emit_usage_error(
            "usage: lin [--data <dir>] run [--gpu] [--explain] [--format table|json|jsonl] [--file <path> | - | '<query>']",
        );
    }
    #[cfg(not(feature = "gpu"))]
    if use_gpu {
        return emit_usage_error("GPU support requires building Lin with --features gpu");
    }
    #[cfg(feature = "gpu")]
    let gpu = if use_gpu {
        match lin::gpu::GpuCompute::new() {
            Ok(gpu) => Some(std::sync::Arc::new(gpu)),
            Err(error) => return emit_lin_error(&error),
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
            use io::Write as _;
            let stdout = io::stdout();
            let mut out = io::BufWriter::with_capacity(64 * 1024, stdout.lock());
            write_handle_formatted(&mut out, &h, format);
            if let Some(p) = plan {
                let _ = writeln!(out);
                let _ = out.write_all(p.as_bytes());
            }
            let _ = out.flush();
            ExitCode::SUCCESS
        }
        Err(e) => emit_lin_error(&e),
    }
}

fn cell_to_json_value(cell: &lin::Cell) -> serde_json::Value {
    match cell {
        lin::Cell::Null => serde_json::Value::Null,
        lin::Cell::Bool(b) => serde_json::Value::Bool(*b),
        lin::Cell::Int(n) | lin::Cell::Time(n) => serde_json::Value::Number((*n).into()),
        lin::Cell::Float(f) => serde_json::Number::from_f64(*f)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        lin::Cell::Text(s) => serde_json::Value::String(s.to_string()),
        lin::Cell::SmallText(s) => serde_json::Value::String(s.as_str().to_string()),
        lin::Cell::Vec(v) => serde_json::Value::Array(
            v.iter()
                .map(|&x| {
                    serde_json::Number::from_f64(f64::from(x))
                        .map(serde_json::Value::Number)
                        .unwrap_or(serde_json::Value::Null)
                })
                .collect(),
        ),
    }
}

fn row_to_json_map(row: &lin::Row) -> serde_json::Map<String, serde_json::Value> {
    let mut map = serde_json::Map::new();
    for (k, v) in row {
        if !matches!(v, lin::Cell::Null) {
            map.insert(k.clone(), cell_to_json_value(v));
        }
    }
    map
}

fn cell_display_table(cell: &lin::Cell) -> String {
    match cell {
        lin::Cell::Null => String::new(),
        lin::Cell::Bool(b) => b.to_string(),
        lin::Cell::Int(n) | lin::Cell::Time(n) => n.to_string(),
        lin::Cell::Float(f) => f.to_string(),
        lin::Cell::Text(s) => s.to_string(),
        lin::Cell::SmallText(s) => s.as_str().to_string(),
        lin::Cell::Vec(v) => format!("vec[{}]", v.len()),
    }
}

fn write_handle_formatted(out: &mut impl io::Write, h: &lin::Handle, format: RunOutputFormat) {
    match format {
        RunOutputFormat::Compact => {
            let mut line = String::with_capacity(128);
            for row in &h.rows {
                line.clear();
                line.push('{');
                let mut first = true;
                for (k, v) in row {
                    if !matches!(v, lin::Cell::Null) {
                        if !first {
                            line.push_str(", ");
                        }
                        first = false;
                        line.push_str(k);
                        line.push_str(": ");
                        v.compact_into(&mut line);
                    }
                }
                line.push_str("}\n");
                let _ = out.write_all(line.as_bytes());
            }
            if let Some(msg) = &h.message {
                let _ = writeln!(out, "{msg}");
            }
            let _ = writeln!(out, "gen={} n={}", h.done.r#gen, h.done.n);
        }
        RunOutputFormat::Jsonl => {
            for row in &h.rows {
                let val = serde_json::Value::Object(row_to_json_map(row));
                let _ = serde_json::to_writer(&mut *out, &val);
                let _ = out.write_all(b"\n");
            }
        }
        RunOutputFormat::Json => {
            let rows_json: Vec<serde_json::Value> = h
                .rows
                .iter()
                .map(|r| serde_json::Value::Object(row_to_json_map(r)))
                .collect();
            let obj = serde_json::json!({
                "rows": rows_json,
                "gen": h.done.r#gen,
                "n": h.done.n,
                "message": h.message,
            });
            let _ = serde_json::to_writer(&mut *out, &obj);
            let _ = out.write_all(b"\n");
        }
        RunOutputFormat::Table => {
            let mut cols_set = std::collections::BTreeSet::new();
            for row in &h.rows {
                for (k, v) in row {
                    if !matches!(v, lin::Cell::Null) {
                        cols_set.insert(k.clone());
                    }
                }
            }
            let cols: Vec<String> = cols_set.into_iter().collect();
            if !cols.is_empty() {
                let mut widths: Vec<usize> = cols.iter().map(String::len).collect();
                let mut rendered: Vec<Vec<String>> = Vec::with_capacity(h.rows.len());
                for row in &h.rows {
                    let mut rvec = Vec::with_capacity(cols.len());
                    for (ci, cname) in cols.iter().enumerate() {
                        let s = row
                            .get(cname.as_str())
                            .map(cell_display_table)
                            .unwrap_or_default();
                        widths[ci] = widths[ci].max(s.len());
                        rvec.push(s);
                    }
                    rendered.push(rvec);
                }
                for (ci, cname) in cols.iter().enumerate() {
                    if ci > 0 {
                        let _ = out.write_all(b" | ");
                    }
                    let _ = write!(out, "{:<width$}", cname, width = widths[ci]);
                }
                let _ = out.write_all(b"\n");
                for (ci, &w) in widths.iter().enumerate() {
                    if ci > 0 {
                        let _ = out.write_all(b"-+-");
                    }
                    let _ = out.write_all("-".repeat(w).as_bytes());
                }
                let _ = out.write_all(b"\n");
                for rvec in &rendered {
                    for (ci, val) in rvec.iter().enumerate() {
                        if ci > 0 {
                            let _ = out.write_all(b" | ");
                        }
                        let _ = write!(out, "{:<width$}", val, width = widths[ci]);
                    }
                    let _ = out.write_all(b"\n");
                }
            }
            if let Some(msg) = &h.message {
                let _ = writeln!(out, "{msg}");
            }
            let _ = writeln!(out, "gen={} n={}", h.done.r#gen, h.done.n);
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
            let cold = COLD.load(Ordering::Relaxed);
            let mut db = if follower {
                if cold {
                    return Err(lin::Error::runtime("--cold is not supported with --follower"));
                }
                Db::open_follower(path)?
            } else if cold {
                Db::open_with(
                    path,
                    lin::OpenOpts {
                        cold: true,
                        ..lin::OpenOpts::default()
                    },
                )?
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
    let msg = "usage:\n  \
         lin [--data <dir>] [--follower] [--json-errors] run [--gpu] [--explain] [--format table|json|jsonl] [--file <path> | - | '<query>']\n  \
         lin [--data <dir>] [--follower] explain [--graph mermaid|dot] '<query>'\n  \
         lin [--data <dir>] import --collection <col> --format jsonl|csv [--file <path>] [--reject-file <path>]\n  \
         lin [--data <dir>] export --query '<query>' --format jsonl|csv [--out <path>]\n  \
         lin [--data <dir>] inspect storage|wal|indexes|locks\n  \
         lin [--data <dir>] health | readiness\n  \
         lin [--data <dir>] config effective\n  \
         lin [--data <dir>] repl [--format table|json|jsonl]\n  \
         lin completions bash|zsh|fish\n  \
         lin --data <dir> wal-serve [--listen HOST:PORT] [--sinks N] [--token TOKEN] [--tls-cert PEM --tls-key PEM]\n  \
         lin --data <dir> follower status|bootstrap|sync …\n  \
         lin --data <dir> --follower apply-wal <frames.bin | ->\n  \
         lin [--data <dir>] backup export <file.json>\n  \
         lin backup import <file.json> [--data <dir>]\n  \
         lin backup verify <file.json>\n  \
         lin [--data <dir>] cache serve [--node-id ID] [--listen HOST:PORT] [--peer ID=HOST:PORT] [--max-entries N]\n  \
         lin cache cli [--connect HOST:PORT] [COMMAND...]\n  \
         lin [--data <dir>] [--cold] [--follower] serve [--listen HOST:PORT] [--token TOKEN] [--tls-cert PEM --tls-key PEM] [--primary HOST:PORT] [--group-window-ms MS] [--max-group-size N] [--max-group-bytes BYTES]\n  \
         lin client --connect HOST:PORT [--replica HOST:PORT ...] [--consistency weak|session|strong|gen:N] [--token TOKEN] [--tls-ca PEM | --tls-insecure] [--workload-group NAME] [--file <path> | - | '<query>']\n  \
         lin shard --shards H1:P1,H2:P2 [--key COL=FIELD ...] [--strategy jump|mod] [--token TOKEN] [--file <path> | - | '<query>']\n  \
         lin --data <dir> remote push|pull|hydrate|evict --store fs://<dir>|s3://<bucket>[/<prefix>] [--endpoint URL] [--region R] [--max-cache-bytes N] [--lazy] [COLLECTION | '<query>']\n  \
         lin [--data <dir>] [--follower] stats\n  \
         lin --data <dir> --cold cluster <collection> [<field> | --none]\n  \
         lin --data <dir> --cold compact <collection>\n  \
         global: --cold opens <dir> with the disk tier; --json-errors emits structured JSON diagnostics\n  \
         lin version [--capabilities]";
    if JSON_ERRORS.load(Ordering::Relaxed) {
        let payload = serde_json::json!({
            "code": "E_USAGE",
            "phase": "cli",
            "message": msg,
            "exit_code": code,
        });
        eprintln!("{payload}");
    } else {
        eprintln!("{msg}");
    }
    ExitCode::from(code)
}

/// `cluster <collection>` prints the cluster key; `cluster <collection> <field>` sets it;
/// `cluster <collection> --none` clears it.
fn cmd_cluster(args: &[String], data: Option<PathBuf>, follower: bool) -> ExitCode {
    let (Some(collection), true) = (args.first(), args.len() <= 2) else {
        return usage(2);
    };
    let field = args.get(1).map(String::as_str);
    let out = cold_command(data, follower, |db| match field {
        None => Ok(match db.cluster_key(collection) {
            Some(key) => format!("{collection}: cluster key {key}"),
            None => format!("{collection}: no cluster key"),
        }),
        Some("--none") => {
            db.set_cluster_key(collection, None)?;
            Ok(format!("{collection}: cluster key cleared"))
        }
        Some(key) => {
            db.set_cluster_key(collection, Some(key))?;
            Ok(format!("{collection}: cluster key {key}"))
        }
    });
    report(out)
}

/// `compact <collection>`: checkpoint, reclaim deleted rows and merge the collection's
/// segments (in cluster-key order when one is declared).
fn cmd_compact(args: &[String], data: Option<PathBuf>, follower: bool) -> ExitCode {
    let [collection] = args else {
        return usage(2);
    };
    report(cold_command(data, follower, |db| {
        db.compact_collection(collection)?;
        Ok(format!("{collection}: compacted"))
    }))
}

fn cold_command(
    data: Option<PathBuf>,
    follower: bool,
    f: impl FnOnce(&mut Db) -> Result<String, lin::Error>,
) -> Result<String, lin::Error> {
    if !COLD.load(Ordering::Relaxed) {
        return Err(lin::Error::runtime("this command requires --cold"));
    }
    with_data(data, follower, |db| match db {
        Some(db) => f(db),
        None => Err(lin::Error::runtime("this command requires --data <dir>")),
    })
}

fn report(out: Result<String, lin::Error>) -> ExitCode {
    match out {
        Ok(line) => {
            println!("{line}");
            ExitCode::SUCCESS
        }
        Err(e) => emit_lin_error(&e),
    }
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
            "usage: lin --data <dir> wal-serve [--listen HOST:PORT] [--sinks N] [--token TOKEN] [--tls-cert PEM --tls-key PEM]"
        );
        return ExitCode::from(2);
    };
    let mut listen = "127.0.0.1:9876".to_string();
    let mut token: Option<String> = std::env::var("LIN_SHIP_TOKEN")
        .ok()
        .filter(|s| !s.is_empty());
    let mut cert: Option<PathBuf> = None;
    let mut key: Option<PathBuf> = None;
    let mut sinks = 16usize;
    let mut max_lag_gens: Option<u64> = None;
    let mut lock_writer = false;
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
                            "usage: lin --data <dir> wal-serve [--listen HOST:PORT] [--sinks N] [--token TOKEN] [--tls-cert PEM --tls-key PEM]"
                        );
                        return ExitCode::from(2);
                    }
                }
            }
            "--sinks" => {
                i += 1;
                match args.get(i).and_then(|a| a.parse::<usize>().ok()) {
                    Some(n) if (1..=lin::ship::MAX_SINKS).contains(&n) => {
                        sinks = n;
                        i += 1;
                    }
                    _ => {
                        eprintln!("--sinks requires a number from 1 to {}", lin::ship::MAX_SINKS);
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
            "--max-lag" => {
                i += 1;
                match args.get(i).and_then(|a| a.parse::<u64>().ok()) {
                    Some(n) => {
                        max_lag_gens = (n > 0).then_some(n);
                        i += 1;
                    }
                    None => {
                        eprintln!("--max-lag requires a non-negative generation count");
                        return ExitCode::from(2);
                    }
                }
            }
            "--lock-writer" => {
                lock_writer = true;
                i += 1;
            }
            other => {
                eprintln!("unknown wal-serve arg {other}");
                return ExitCode::from(2);
            }
        }
    }
    let tls = match load_tls_server(cert, key) {
        Ok(t) => t,
        Err(code) => return code,
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
        "wal-serve {} on {addr} tls={} token={} sinks={sinks} max_lag={:?} mode={}",
        dir.display(),
        tls.is_some(),
        token.is_some(),
        max_lag_gens,
        if lock_writer { "writer-locked" } else { "live-reader" }
    );
    let opts = lin::ship::ServeOpts { token, tls };
    if lock_writer {
        match Db::open(&dir) {
            Ok(db) => match if sinks > 1 {
                lin::ship::serve_blocking_fanout(&db, listener, &opts, sinks)
            } else {
                lin::ship::serve_blocking_opts(&db, listener, &opts)
            } {
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
    } else {
        let live = lin::ship::LiveServeOpts {
            serve: opts,
            max_sinks: sinks,
            max_lag_gens,
            hub: None,
        };
        match lin::ship::serve_dir_live(&dir, listener, &live) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("{e}");
                ExitCode::from(1)
            }
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
            let mut wait_ms: Option<u32> = None;
            let mut follower_id: Option<String> = None;
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
                    "--wait-ms" => {
                        i += 1;
                        match args.get(i).and_then(|a| a.parse::<u32>().ok()) {
                            Some(ms) => {
                                wait_ms = Some(ms.min(30_000));
                                i += 1;
                            }
                            None => {
                                eprintln!("--wait-ms requires milliseconds (0..=30000)");
                                return ExitCode::from(2);
                            }
                        }
                    }
                    "--follower-id" => {
                        i += 1;
                        match args.get(i) {
                            Some(id) => {
                                follower_id = Some(id.clone());
                                i += 1;
                            }
                            None => {
                                eprintln!("--follower-id requires a value");
                                return ExitCode::from(2);
                            }
                        }
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
            let tls = match load_tls_client(tls_ca, tls_insecure) {
                Ok(t) => t,
                Err(code) => return code,
            };
            let pull = lin::ship::PullOpts {
                token,
                tls,
                server_name: lin::ship::tls_server_name(&addr),
            };
            loop {
                match sync_once(
                    &dir,
                    &addr,
                    &pull,
                    wait_ms,
                    follower_id.as_deref(),
                ) {
                    Ok(()) => {}
                    Err(code) => return code,
                }
                match loop_secs {
                    Some(0) if wait_ms.unwrap_or(0) > 0 => {}
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
                 lin --data <dir> follower sync --from HOST:PORT [--loop SECS] [--wait-ms MS] [--follower-id ID] [--token TOKEN] [--tls-ca PEM | --tls-insecure]"
            );
            ExitCode::from(2)
        }
    }
}

fn sync_once(
    dir: &PathBuf,
    addr: &str,
    pull: &lin::ship::PullOpts,
    wait_ms: Option<u32>,
    follower_id: Option<&str>,
) -> Result<(), ExitCode> {
    let mut db = Db::open_follower(dir).map_err(|e| {
        eprintln!("{e}");
        ExitCode::from(1)
    })?;
    let since = db.r#gen();
    let frames = if wait_ms.is_some() || follower_id.is_some() {
        let res = lin::ship::pull_wait(addr, since, wait_ms.unwrap_or(0), follower_id, pull)
            .map_err(|e| {
                eprintln!("{e}");
                ExitCode::from(1)
            })?;
        if res.status == lin::ship::ShipPullStatus::LagExceeded {
            eprintln!(
                "wal ship: follower lag exceeded (since={since}, primary={}); hint: re-bootstrap from a fresh primary backup",
                res.primary_gen
            );
            return Err(ExitCode::from(1));
        }
        res.frames
    } else {
        lin::ship::pull_with(addr, since, pull).map_err(|e| {
            eprintln!("{e}");
            ExitCode::from(1)
        })?
    };
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
            // Allow `--data` and `--force` after import path as well.
            let mut data = data;
            let mut force = args.iter().any(|a| a == "--force" || a == "-f");
            let mut i = 2;
            while i < args.len() {
                if args[i] == "--data" {
                    i += 1;
                    data = Some(PathBuf::from(
                        args.get(i).map(String::as_str).unwrap_or(".lin"),
                    ));
                    i += 1;
                } else if args[i] == "--force" || args[i] == "-f" {
                    force = true;
                    i += 1;
                } else {
                    i += 1;
                }
            }
            match data {
                Some(dir) => match Db::import_backup_into_checked(path, &dir, force) {
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
                    let ce = lin::Error::corrupt(e.message);
                    emit_lin_error(&ce)
                }
                },
            }
        }
        Some("verify") => {
            let Some(path) = args.get(1) else {
                return emit_usage_error("usage: lin backup verify <file>");
            };
            match lin::persist::read_backup(std::path::Path::new(path)) {
                Ok(snap) => {
                    let total_rows: usize = snap.collections.values().map(Vec::len).sum();
                    let out = serde_json::json!({
                        "valid": true,
                        "path": path,
                        "gen": snap.r#gen,
                        "catalog_hash": snap.catalog_hash,
                        "embed_id": snap.embed_id,
                        "collections": snap.collections.len(),
                        "rows": total_rows,
                        "edges": snap.edges.len(),
                    });
                    println!("{out}");
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    let ce = lin::Error::corrupt(e.message);
                    emit_lin_error(&ce)
                }
            }
        }
        _ => emit_usage_error("usage: lin backup export|import|verify …"),
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
                                use io::Write as _;
                                let stdout = io::stdout();
                                let mut out = io::BufWriter::new(stdout.lock());
                                let _ = writeln!(out, "{}", resp.format_cli());
                                let _ = out.flush();
                                if is_sub {
                                    while let Ok(msg) = client.read_next() {
                                        let _ = writeln!(out, "{}", msg.format_cli());
                                        let _ = out.flush();
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

            // Interactive REPL mode. With commands piped in (stdin not a terminal) the
            // banner and prompts are left out, so the output is just the replies.
            use std::io::{BufRead, IsTerminal, Write};
            let stdin = io::stdin();
            let interactive = stdin.is_terminal();
            let stdout_handle = io::stdout();
            let mut stdout = io::BufWriter::new(stdout_handle.lock());
            if interactive {
                let _ = writeln!(
                    stdout,
                    "Connected to cacha (Lin Distributed Cache) at {}. Type HELP or QUIT.",
                    client.addr()
                );
            }
            let mut lines = stdin.lock().lines();
            loop {
                if interactive {
                    let _ = write!(stdout, "cacha ({})> ", client.addr());
                    let _ = stdout.flush();
                }
                let Some(Ok(line)) = lines.next() else {
                    if interactive {
                        let _ = writeln!(stdout);
                    }
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
                    let _ = writeln!(
                        stdout,
                        "Commands:\n  \
                         Strings: GET k | SET k v [EX s | PX ms] | DEL k.. | EXISTS k | INCR k | DECR k | INCRBY k n | EXPIRE k s | TTL k | KEYS prefix*\n  \
                         Hashes:  HSET k f v | HGET k f | HDEL k f.. | HGETALL k\n  \
                         Lists:   LPUSH k v.. | RPUSH k v.. | LPOP k | RPOP k | LRANGE k start stop\n  \
                         Sets:    SADD k m.. | SREM k m.. | SMEMBERS k | SISMEMBER k m\n  \
                         Pub/Sub: PUBLISH ch msg | SUBSCRIBE ch..\n  \
                         Lin DB:  LIN <pipe-query>   (e.g. `LIN docs | take 3` or `LIN cache_kv | count`)\n  \
                         Cluster: INFO | CLUSTER INFO | CLUSTER NODES | CLUSTER MEET id addr | CLUSTER KEYSLOT k | RAFT STATUS | RAFT ELECT"
                    );
                    if interactive {
                        let _ = stdout.flush();
                    }
                    continue;
                }
                match lin::cache::parse_cli_command(trimmed) {
                    Ok(req) => {
                        let is_sub = matches!(req, lin::cache::CacheRequest::Subscribe { .. });
                        match client.send(&req) {
                            Ok(resp) => {
                                let _ = writeln!(stdout, "{}", resp.format_cli());
                                if is_sub {
                                    let _ = writeln!(
                                        stdout,
                                        "(listening for messages — Ctrl+C to exit)"
                                    );
                                    let _ = stdout.flush();
                                    while let Ok(msg) = client.read_next() {
                                        let _ = writeln!(stdout, "{}", msg.format_cli());
                                        let _ = stdout.flush();
                                    }
                                    break;
                                }
                                if interactive {
                                    let _ = stdout.flush();
                                }
                            }
                            Err(e) => eprintln!("(io error) {e}"),
                        }
                    }
                    Err(e) => eprintln!("(syntax error) {e}"),
                }
            }
            let _ = stdout.flush();
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

fn parse_consistency_flag(s: &str) -> Result<lin::Consistency, String> {
    match s {
        "weak" => Ok(lin::Consistency::Weak),
        "session" => Ok(lin::Consistency::Session),
        "strong" => Ok(lin::Consistency::Strong),
        other => {
            if let Some(rest) = other.strip_prefix("gen:") {
                let g = rest
                    .parse::<u64>()
                    .map_err(|_| format!("invalid generation in consistency {other:?}"))?;
                Ok(lin::Consistency::AtLeastGen(g))
            } else {
                Err(format!(
                    "unknown consistency {other:?}; expected weak, session, strong, or gen:<n>"
                ))
            }
        }
    }
}

fn load_tls_server(
    cert: Option<PathBuf>,
    key: Option<PathBuf>,
) -> Result<Option<lin::TlsServer>, ExitCode> {
    match (cert, key) {
        (Some(c), Some(k)) => match (std::fs::read(&c), std::fs::read(&k)) {
            (Ok(cert_pem), Ok(key_pem)) => Ok(Some(lin::TlsServer { cert_pem, key_pem })),
            (Err(e), _) => {
                eprintln!("tls-cert: {e}");
                Err(ExitCode::from(1))
            }
            (_, Err(e)) => {
                eprintln!("tls-key: {e}");
                Err(ExitCode::from(1))
            }
        },
        (None, None) => Ok(None),
        _ => {
            eprintln!("tls requires both --tls-cert and --tls-key");
            Err(ExitCode::from(2))
        }
    }
}

fn load_tls_client(
    tls_ca: Option<PathBuf>,
    tls_insecure: bool,
) -> Result<lin::TlsClient, ExitCode> {
    if tls_insecure {
        Ok(lin::TlsClient::Insecure)
    } else if let Some(ca_path) = tls_ca {
        match std::fs::read(&ca_path) {
            Ok(pem) => Ok(lin::TlsClient::CaPem(pem)),
            Err(e) => {
                eprintln!("tls-ca: {e}");
                Err(ExitCode::from(1))
            }
        }
    } else {
        Ok(lin::TlsClient::Off)
    }
}

fn is_non_query_leading_kw(src: &str) -> bool {
    let bytes = src.as_bytes();
    let mut i = 0usize;
    loop {
        while i < bytes.len() {
            let b = bytes[i];
            if b.is_ascii_whitespace() {
                i += 1;
            } else if b >= 0x80 {
                if let Some(c) = src[i..].chars().next()
                    && c.is_whitespace()
                {
                    i += c.len_utf8();
                } else {
                    break;
                }
            } else {
                break;
            }
        }
        if i + 1 < bytes.len() && bytes[i] == b'-' && bytes[i + 1] == b'-' {
            i += 2;
            match memchr::memchr(b'\n', &bytes[i..]) {
                Some(nl) => {
                    i += nl + 1;
                    continue;
                }
                None => return false,
            }
        }
        break;
    }
    let Some(&first) = bytes.get(i) else {
        return false;
    };
    if !(first.is_ascii_alphabetic() || first == b'_') {
        return false;
    }
    let mut end = i + 1;
    while end < bytes.len() && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
        end += 1;
    }
    matches!(
        &src[i..end],
        "let"
            | "delete"
            | "append"
            | "insert"
            | "update"
            | "reembed"
            | "index"
            | "owned"
            | "col"
            | "rel"
            | "fk"
            | "guard"
            | "filter"
            | "unfilter"
            | "idb"
            | "pull"
            | "push"
            | "pin"
            | "unpin"
            | "snapshot"
            | "restore"
    )
}

fn write_cell_cli(w: &mut impl io::Write, cell: &lin::Cell) -> io::Result<()> {
    match cell {
        lin::Cell::Null => w.write_all(b"null"),
        lin::Cell::Bool(b) => w.write_all(if *b { b"true" } else { b"false" }),
        lin::Cell::Int(i) => write!(w, "{i}"),
        lin::Cell::Float(f) => write!(w, "{f}"),
        lin::Cell::Time(t) => write!(w, "timestamp({t})"),
        lin::Cell::Text(s) => write!(w, "{s:?}"),
        lin::Cell::SmallText(s) => write!(w, "{:?}", s.as_str()),
        lin::Cell::Vec(vec) => write!(w, "vec[{}]", vec.len()),
    }
}

fn write_batch_cli(w: &mut impl io::Write, batch: &lin::RecordBatch) -> io::Result<()> {
    let n = batch.n();
    if n == 0 {
        return w.write_all(b"(0 rows)");
    }
    let width = batch.names.len().min(batch.cols.len());
    let mut col_order: Vec<usize> = (0..width).collect();
    col_order.sort_by(|&a, &b| batch.names[a].cmp(&batch.names[b]).then_with(|| a.cmp(&b)));
    // Match `Row::insert` overwrite semantics when duplicate column names exist: last wins.
    let mut write_pos = 0usize;
    for idx in 0..col_order.len() {
        if idx + 1 < col_order.len()
            && batch.names[col_order[idx]] == batch.names[col_order[idx + 1]]
        {
            continue;
        }
        col_order[write_pos] = col_order[idx];
        write_pos += 1;
    }
    col_order.truncate(write_pos);

    for row_idx in 0..n {
        w.write_all(b"{")?;
        for (pos, &col_idx) in col_order.iter().enumerate() {
            if pos > 0 {
                w.write_all(b", ")?;
            }
            w.write_all(batch.names[col_idx].as_bytes())?;
            w.write_all(b": ")?;
            match &batch.cols[col_idx] {
                lin::Column::Nulls { .. } => w.write_all(b"null")?,
                lin::Column::Int { vals, valid } => {
                    if valid.is_valid(row_idx) {
                        write!(w, "{}", vals[row_idx])?;
                    } else {
                        w.write_all(b"null")?;
                    }
                }
                lin::Column::Time { vals, valid } => {
                    if valid.is_valid(row_idx) {
                        write!(w, "timestamp({})", vals[row_idx])?;
                    } else {
                        w.write_all(b"null")?;
                    }
                }
                lin::Column::Float { vals, valid } => {
                    if valid.is_valid(row_idx) {
                        write!(w, "{}", vals[row_idx])?;
                    } else {
                        w.write_all(b"null")?;
                    }
                }
                lin::Column::Bool { vals, valid } => {
                    if valid.is_valid(row_idx) {
                        w.write_all(if vals[row_idx] { b"true" } else { b"false" })?;
                    } else {
                        w.write_all(b"null")?;
                    }
                }
                col @ (lin::Column::Text { .. } | lin::Column::Dict { .. }) => {
                    match col.text_at(row_idx) {
                        Some(s) => write!(w, "{s:?}")?,
                        None => w.write_all(b"null")?,
                    }
                }
                lin::Column::Cells(cells) => {
                    let cell = cells.get(row_idx).unwrap_or(&lin::Cell::Null);
                    write_cell_cli(w, cell)?;
                }
            }
        }
        w.write_all(b"}\n")?;
    }
    Ok(())
}

fn cmd_serve(args: &[String], data: Option<PathBuf>, follower: bool) -> ExitCode {
    let cold = COLD.load(Ordering::Relaxed);
    let mut listen = "127.0.0.1:7443".to_string();
    let mut token: Option<String> = None;
    let mut tls_cert: Option<PathBuf> = None;
    let mut tls_key: Option<PathBuf> = None;
    let mut primary: Option<String> = None;
    let mut tls_ca: Option<PathBuf> = None;
    let mut tls_insecure = false;
    let mut window_ms: u64 = 2;
    let mut max_group_size: usize = 64;
    let mut max_group_bytes: usize = 4 * 1024 * 1024;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--listen" => {
                i += 1;
                let Some(v) = args.get(i) else {
                    eprintln!("--listen requires HOST:PORT");
                    return ExitCode::from(2);
                };
                listen = v.clone();
            }
            "--token" => {
                i += 1;
                let Some(v) = args.get(i) else {
                    eprintln!("--token requires TOKEN");
                    return ExitCode::from(2);
                };
                token = Some(v.clone());
            }
            "--tls-cert" => {
                i += 1;
                let Some(v) = args.get(i) else {
                    eprintln!("--tls-cert requires path");
                    return ExitCode::from(2);
                };
                tls_cert = Some(PathBuf::from(v));
            }
            "--tls-key" => {
                i += 1;
                let Some(v) = args.get(i) else {
                    eprintln!("--tls-key requires path");
                    return ExitCode::from(2);
                };
                tls_key = Some(PathBuf::from(v));
            }
            "--primary" => {
                i += 1;
                let Some(v) = args.get(i) else {
                    eprintln!("--primary requires HOST:PORT");
                    return ExitCode::from(2);
                };
                primary = Some(v.clone());
            }
            "--tls-ca" => {
                i += 1;
                let Some(v) = args.get(i) else {
                    eprintln!("--tls-ca requires path");
                    return ExitCode::from(2);
                };
                tls_ca = Some(PathBuf::from(v));
            }
            "--tls-insecure" => tls_insecure = true,
            "--group-window-ms" => {
                i += 1;
                let Some(Ok(v)) = args.get(i).map(|s| s.parse::<u64>()) else {
                    eprintln!("--group-window-ms requires integer milliseconds");
                    return ExitCode::from(2);
                };
                window_ms = v;
            }
            "--max-group-size" => {
                i += 1;
                let Some(Ok(v)) = args.get(i).map(|s| s.parse::<usize>()) else {
                    eprintln!("--max-group-size requires positive integer");
                    return ExitCode::from(2);
                };
                max_group_size = v.max(1);
            }
            "--max-group-bytes" => {
                i += 1;
                let Some(Ok(v)) = args.get(i).map(|s| s.parse::<usize>()) else {
                    eprintln!("--max-group-bytes requires positive integer");
                    return ExitCode::from(2);
                };
                max_group_bytes = v.max(1);
            }
            other => {
                eprintln!("unknown serve arg {other}");
                return ExitCode::from(2);
            }
        }
        i += 1;
    }

    let tls = match load_tls_server(tls_cert, tls_key) {
        Ok(t) => t,
        Err(code) => return code,
    };

    let client_tls = match load_tls_client(tls_ca, tls_insecure) {
        Ok(t) => t,
        Err(code) => return code,
    };

    let role = if follower || primary.is_some() {
        lin::NodeRole::Follower
    } else {
        lin::NodeRole::Primary
    };
    let follower_opts = primary.map(|paddr| {
        let server_name = lin::ship::tls_server_name(&paddr);
        lin::FollowerSyncOpts::new(
            paddr,
            lin::ClientOpts {
                token: token.clone(),
                tls: client_tls,
                server_name,
                ..lin::ClientOpts::default()
            },
        )
    });

    let open_opts = lin::OpenOpts {
        cold,
        ..lin::OpenOpts::default()
    };
    let db_res = match data {
        Some(dir) => {
            if role == lin::NodeRole::Follower {
                Db::open_follower_with(dir, open_opts)
            } else {
                Db::open_with(dir, open_opts)
            }
        }
        None => {
            if role == lin::NodeRole::Follower && follower_opts.is_none() {
                return report(Err(lin::Error::runtime("--follower requires --data <dir> or --primary")));
            }
            Ok(Db::empty())
        }
    };
    let db = match db_res {
        Ok(d) => d,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(1);
        }
    };

    let cfg = lin::ServerConfig {
        bind_addr: listen,
        role,
        follower: follower_opts,
        token,
        tls,
        group_commit_window: Duration::from_millis(window_ms),
        max_group_size,
        max_group_bytes,
        ..lin::ServerConfig::default()
    };

    match lin::LinServer::start(db, cfg) {
        Ok(handle) => {
            eprintln!("lin serve listening on {} (role={role:?})", handle.local_addr());
            loop {
                thread::sleep(Duration::from_secs(3600));
            }
        }
        Err(e) => {
            eprintln!("{e}");
            ExitCode::from(1)
        }
    }
}

fn cmd_client(args: &[String]) -> ExitCode {
    let mut connect: Option<String> = None;
    let mut replicas: Vec<String> = Vec::new();
    let mut consistency = lin::Consistency::Session;
    let mut token: Option<String> = None;
    let mut tls_ca: Option<PathBuf> = None;
    let mut tls_insecure = false;
    let mut wg: Option<String> = None;
    let mut file: Option<PathBuf> = None;
    let mut rest = Vec::new();

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--connect" => {
                i += 1;
                connect = args.get(i).cloned();
            }
            "--replica" => {
                i += 1;
                if let Some(v) = args.get(i) {
                    replicas.push(v.clone());
                }
            }
            "--consistency" => {
                i += 1;
                let Some(v) = args.get(i) else {
                    eprintln!("--consistency requires weak|session|strong|gen:N");
                    return ExitCode::from(2);
                };
                match parse_consistency_flag(v) {
                    Ok(c) => consistency = c,
                    Err(e) => {
                        eprintln!("{e}");
                        return ExitCode::from(2);
                    }
                }
            }
            "--token" => {
                i += 1;
                token = args.get(i).cloned();
            }
            "--tls-ca" => {
                i += 1;
                tls_ca = args.get(i).map(PathBuf::from);
            }
            "--tls-insecure" => tls_insecure = true,
            "--workload-group" => {
                i += 1;
                wg = args.get(i).cloned();
            }
            "--file" => {
                i += 1;
                file = args.get(i).map(PathBuf::from);
            }
            _ => rest.push(args[i].clone()),
        }
        i += 1;
    }

    let Some(primary_addr) = connect else {
        eprintln!("client requires --connect HOST:PORT");
        return ExitCode::from(2);
    };
    let linql = match load_program(file, &rest) {
        Ok(s) if !s.trim().is_empty() => s,
        Ok(_) => return usage(2),
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(2);
        }
    };

    let tls = match load_tls_client(tls_ca, tls_insecure) {
        Ok(t) => t,
        Err(code) => return code,
    };

    let opts = lin::ClientOpts {
        token,
        tls,
        server_name: lin::ship::tls_server_name(&primary_addr),
        consistency,
        workload_group: wg,
        ..lin::ClientOpts::default()
    };

    let res = if replicas.is_empty() {
        lin::ServerClient::connect(primary_addr, opts)
            .and_then(|c| c.execute_with_consistency(&linql, consistency))
    } else {
        lin::ReplicaPoolClient::connect(primary_addr, &replicas, opts).and_then(|pool| {
            let is_read_query = !is_non_query_leading_kw(&linql)
                && lin::parse(&linql).is_ok_and(|s| matches!(s, lin::Stmt::Query(_)));
            if is_read_query {
                let (_, r) = pool.query_with_consistency(&linql, consistency)?;
                Ok(r)
            } else {
                pool.execute(&linql)
            }
        })
    };

    match res {
        Ok(receipt) => {
            use io::Write as _;
            let stdout = io::stdout();
            let mut out = io::BufWriter::with_capacity(64 * 1024, stdout.lock());
            if let Some(batch) = &receipt.batch {
                let _ = write_batch_cli(&mut out, batch);
            }
            let _ = writeln!(
                out,
                "gen={} affected={} group={} elapsed_us={}",
                receipt.r#gen, receipt.affected, receipt.group_size, receipt.elapsed_us
            );
            let _ = out.flush();
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{e}");
            ExitCode::from(1)
        }
    }
}

fn cmd_shard(args: &[String]) -> ExitCode {
    let mut shard_addrs: Vec<String> = Vec::new();
    let mut shard_keys = std::collections::BTreeMap::new();
    let mut default_shard_key: Option<String> = None;
    let mut strategy = lin::ShardStrategy::JumpConsistentHash;
    let mut token: Option<String> = None;
    let mut consistency = lin::Consistency::Session;
    let mut file: Option<PathBuf> = None;
    let mut rest = Vec::new();

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--shards" => {
                i += 1;
                let Some(v) = args.get(i) else {
                    eprintln!("--shards requires H1:P1,H2:P2");
                    return ExitCode::from(2);
                };
                shard_addrs.extend(v.split(',').filter(|s| !s.is_empty()).map(str::to_string));
            }
            "--shard" => {
                i += 1;
                if let Some(v) = args.get(i) {
                    shard_addrs.push(v.clone());
                }
            }
            "--key" => {
                i += 1;
                let Some(v) = args.get(i) else {
                    eprintln!("--key requires COL=FIELD");
                    return ExitCode::from(2);
                };
                if let Some((c, f)) = v.split_once('=') {
                    shard_keys.insert(c.to_string(), f.to_string());
                } else {
                    eprintln!("--key expects COL=FIELD, got {v:?}");
                    return ExitCode::from(2);
                }
            }
            "--default-key" => {
                i += 1;
                default_shard_key = args.get(i).cloned();
            }
            "--strategy" => {
                i += 1;
                match args.get(i).map(String::as_str) {
                    Some("jump") => strategy = lin::ShardStrategy::JumpConsistentHash,
                    Some("mod") => strategy = lin::ShardStrategy::HashMod,
                    _ => {
                        eprintln!("--strategy must be jump or mod");
                        return ExitCode::from(2);
                    }
                }
            }
            "--token" => {
                i += 1;
                token = args.get(i).cloned();
            }
            "--consistency" => {
                i += 1;
                let Some(v) = args.get(i) else {
                    return usage(2);
                };
                match parse_consistency_flag(v) {
                    Ok(c) => consistency = c,
                    Err(e) => {
                        eprintln!("{e}");
                        return ExitCode::from(2);
                    }
                }
            }
            "--file" => {
                i += 1;
                file = args.get(i).map(PathBuf::from);
            }
            _ => rest.push(args[i].clone()),
        }
        i += 1;
    }

    if shard_addrs.is_empty() {
        eprintln!("shard requires --shards H1:P1,H2:P2");
        return ExitCode::from(2);
    }
    let linql = match load_program(file, &rest) {
        Ok(s) if !s.trim().is_empty() => s,
        Ok(_) => return usage(2),
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(2);
        }
    };

    let cfg = lin::ShardConfig {
        shard_addrs,
        shard_keys,
        default_shard_key,
        strategy,
        client: lin::ClientOpts {
            token,
            consistency,
            ..lin::ClientOpts::default()
        },
        ..lin::ShardConfig::default()
    };
    let router = match lin::ShardRouter::connect(cfg) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(1);
        }
    };

    match router.execute(&linql) {
        Ok(receipt) => {
            use io::Write as _;
            let stdout = io::stdout();
            let mut out = io::BufWriter::with_capacity(64 * 1024, stdout.lock());
            if let Some(batch) = &receipt.batch {
                let _ = write_batch_cli(&mut out, batch);
            }
            let _ = writeln!(
                out,
                "gen={} affected={} elapsed_us={}",
                receipt.r#gen, receipt.affected, receipt.elapsed_us
            );
            let _ = out.flush();
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{e}");
            ExitCode::from(1)
        }
    }
}

fn parse_remote_store_uri(
    uri: &str,
    prefix_override: &Option<String>,
    endpoint: &Option<String>,
    region: &str,
) -> Result<(std::sync::Arc<dyn lin::ColdObjectStore>, String), lin::Error> {
    if let Some(path) = uri.strip_prefix("fs://") {
        let store = lin::FsObjectStore::open(path)?;
        let prefix = prefix_override.clone().unwrap_or_default();
        return Ok((std::sync::Arc::new(store), prefix));
    }
    if let Some(rest) = uri.strip_prefix("s3://") {
        let (bucket, key_pfx) = match rest.split_once('/') {
            Some((b, p)) => (b.to_string(), p.to_string()),
            None => (rest.to_string(), String::new()),
        };
        let prefix = prefix_override.clone().unwrap_or(key_pfx);
        let cfg = lin::S3Config {
            endpoint: endpoint
                .clone()
                .unwrap_or_else(|| "http://127.0.0.1:9000".to_string()),
            region: region.to_string(),
            bucket,
            prefix: prefix.clone(),
            access_key: std::env::var("AWS_ACCESS_KEY_ID").unwrap_or_default(),
            secret_key: std::env::var("AWS_SECRET_ACCESS_KEY").unwrap_or_default(),
            session_token: std::env::var("AWS_SESSION_TOKEN").ok(),
            ..lin::S3Config::default()
        };
        let store = lin::S3HttpObjectStore::new(cfg)?;
        return Ok((std::sync::Arc::new(store), prefix));
    }
    Err(lin::Error::runtime(format!(
        "unsupported --store URI {uri:?}; expected fs://<dir> or s3://<bucket>[/<prefix>]"
    )))
}

fn cmd_remote(args: &[String], data: Option<PathBuf>) -> ExitCode {
    let Some(sub) = args.first().map(String::as_str) else {
        return usage(2);
    };
    let mut store_uri: Option<String> = None;
    let mut prefix: Option<String> = None;
    let mut endpoint: Option<String> = None;
    let mut region = "us-east-1".to_string();
    let mut lazy = true;
    let mut max_cache_bytes: u64 = 0;
    let mut prune = false;
    let mut rest = Vec::new();

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--store" => {
                i += 1;
                store_uri = args.get(i).cloned();
            }
            "--prefix" => {
                i += 1;
                prefix = args.get(i).cloned();
            }
            "--endpoint" => {
                i += 1;
                endpoint = args.get(i).cloned();
            }
            "--region" => {
                i += 1;
                if let Some(r) = args.get(i) {
                    region = r.clone();
                }
            }
            "--lazy" => lazy = true,
            "--eager" => lazy = false,
            "--prune" => prune = true,
            "--max-cache-bytes" => {
                i += 1;
                let Some(Ok(v)) = args.get(i).map(|s| s.parse::<u64>()) else {
                    eprintln!("--max-cache-bytes requires integer bytes");
                    return ExitCode::from(2);
                };
                max_cache_bytes = v;
            }
            _ => rest.push(args[i].clone()),
        }
        i += 1;
    }

    let Some(dir) = data else {
        eprintln!("remote {sub} requires --data <dir>");
        return ExitCode::from(2);
    };
    let Some(uri) = store_uri else {
        eprintln!("remote {sub} requires --store fs://<dir>|s3://<bucket>");
        return ExitCode::from(2);
    };
    let (store, eff_prefix) = match parse_remote_store_uri(&uri, &prefix, &endpoint, &region) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(1);
        }
    };

    let cfg = lin::RemoteCacheConfig {
        lazy,
        max_local_bytes: max_cache_bytes,
    };

    let out = match sub {
        "push" => {
            let mut db = match Db::open_with(
                &dir,
                lin::OpenOpts {
                    cold: true,
                    ..lin::OpenOpts::default()
                },
            ) {
                Ok(d) => d,
                Err(e) => return report(Err(e)),
            };
            let res = db
                .push_cold_to_object_store(store.as_ref(), &eff_prefix, prune)
                .map(|st| {
                    format!(
                        "gen={} uploaded={} reused={} deleted={} bytes={}",
                        st.generation,
                        st.segments_uploaded,
                        st.segments_reused,
                        st.segments_deleted_remote,
                        st.bytes_uploaded
                    )
                });
            let _ = db.close();
            res
        }
        "pull" => {
            let query = rest.join(" ");
            let mut db = match Db::open_cold_from_object_store(&dir, store, &eff_prefix, cfg) {
                Ok(d) => d,
                Err(e) => return report(Err(e)),
            };
            let res = if query.trim().is_empty() {
                Ok(format!(
                    "opened remote store gen={} local_cache_bytes={}",
                    db.r#gen(),
                    db.cold_local_cache_bytes()
                ))
            } else {
                db.run_remote(&query).map(|h| h.compact())
            };
            let _ = db.close();
            res
        }
        "hydrate" => {
            let Some(table) = rest.first() else {
                eprintln!("remote hydrate requires COLLECTION");
                return ExitCode::from(2);
            };
            let mut db = match Db::open_cold_from_object_store(&dir, store, &eff_prefix, cfg) {
                Ok(d) => d,
                Err(e) => return report(Err(e)),
            };
            let res = db.hydrate_cold_collection(table).map(|segs| {
                format!(
                    "hydrated {table}: segments={segs} local_cache_bytes={}",
                    db.cold_local_cache_bytes()
                )
            });
            let _ = db.close();
            res
        }
        "evict" => {
            let mut db = match Db::open_cold_from_object_store(&dir, store, &eff_prefix, cfg) {
                Ok(d) => d,
                Err(e) => return report(Err(e)),
            };
            let res = db.evict_cold_cache_to(max_cache_bytes).map(|evicted| {
                format!(
                    "evicted_bytes={evicted} local_cache_bytes={}",
                    db.cold_local_cache_bytes()
                )
            });
            let _ = db.close();
            res
        }
        other => {
            eprintln!("unknown remote subcommand {other}");
            return usage(2);
        }
    };
    report(out)
}

fn cmd_repair_catalog_hash(data: Option<PathBuf>) -> ExitCode {
    let Some(dir) = data else {
        eprintln!("usage: lin --data <dir> repair-catalog-hash");
        return ExitCode::from(2);
    };
    match Db::repair_catalog_hash(&dir) {
        Ok(hash) => {
            println!("repaired catalog_hash={hash} dir={}", dir.display());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{e}");
            ExitCode::from(1)
        }
    }
}

fn cmd_recover(args: &[String], data: Option<PathBuf>) -> ExitCode {
    if args.iter().any(|a| a == "--fix-catalog-hash") {
        return cmd_repair_catalog_hash(data);
    }
    emit_usage_error("usage: lin --data <dir> recover --fix-catalog-hash")
}

fn cmd_version(args: &[String]) -> ExitCode {
    let mut capabilities = false;
    for arg in args {
        match arg.as_str() {
            "--capabilities" | "--json" => capabilities = true,
            other => {
                return emit_usage_error(&format!(
                    "unknown version arg {other:?} (expected --capabilities)"
                ));
            }
        }
    }
    if !capabilities {
        println!("lin {}", lin::VERSION);
        return ExitCode::SUCCESS;
    }

    let feature_flags = [
        ("async", cfg!(feature = "async")),
        ("derive", cfg!(feature = "derive")),
        ("gpu", cfg!(feature = "gpu")),
        ("parallel", cfg!(feature = "parallel")),
        ("compress-flate", cfg!(feature = "compress-flate")),
        ("compress-lz4", cfg!(feature = "compress-lz4")),
        ("compress-zstd", cfg!(feature = "compress-zstd")),
        ("compress-wal", cfg!(feature = "compress-wal")),
        ("embed-ollama", cfg!(feature = "embed-ollama")),
        ("embed-onnx", cfg!(feature = "embed-onnx")),
    ];
    let mut features_map = serde_json::Map::new();
    let mut enabled_features = Vec::new();
    for (name, enabled) in feature_flags {
        features_map.insert(name.to_string(), serde_json::Value::Bool(enabled));
        if enabled {
            enabled_features.push(name);
        }
    }
    let snapshot_magics: Vec<String> = lin::persist::CodecCapabilities::supported_snapshot_magics()
        .iter()
        .map(|m| {
            m.iter()
                .map(|b| format!("{b:02x}"))
                .collect::<Vec<_>>()
                .join("")
        })
        .collect();
    let out = serde_json::json!({
        "version": lin::VERSION,
        "features": features_map,
        "enabled_features": enabled_features,
        "wal_codecs": lin::persist::CodecCapabilities::supported_wal_codecs(),
        "snapshot_magics_hex": snapshot_magics,
    });
    println!("{out}");
    ExitCode::SUCCESS
}

fn probe_nonblocking_exclusive_flock(path: &std::path::Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::fd::AsRawFd;
        let Ok(file) = std::fs::OpenOptions::new().read(true).write(true).open(path) else {
            return false;
        };
        // SAFETY: `file` is a valid open descriptor; non-blocking flock probes contention and unlocks immediately.
        unsafe {
            let rc = libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB);
            if rc == 0 {
                let _ = libc::flock(file.as_raw_fd(), libc::LOCK_UN);
                false
            } else {
                true
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        false
    }
}

fn cmd_inspect(args: &[String], data: Option<PathBuf>) -> ExitCode {
    match args.first().map(String::as_str) {
        Some("wal") => {
            let target_path = if let Some(explicit) = args.get(1) {
                PathBuf::from(explicit)
            } else if let Some(dir) = data {
                lin::persist::log_path(&dir)
            } else {
                return emit_usage_error("usage: lin [--data <dir>] inspect wal [<wal_file>]");
            };
            if !target_path.exists() {
                let out = serde_json::json!({
                    "exists": false,
                    "path": target_path.display().to_string(),
                    "frame_count": 0,
                    "valid_prefix_bytes": 0,
                    "damaged_offset": serde_json::Value::Null,
                    "frames": [],
                });
                println!("{out}");
                return ExitCode::SUCCESS;
            }
            match lin::persist::inspect_wal_file(&target_path) {
                Ok(rep) => {
                    let frames: Vec<serde_json::Value> = rep
                        .frames
                        .iter()
                        .map(|f| {
                            serde_json::json!({
                                "frame_index": f.frame_index,
                                "byte_offset": f.byte_offset,
                                "byte_len": f.byte_len,
                                "codec": f.codec,
                                "gen": f.r#gen,
                                "next_id": f.next_id,
                                "pack_kind": f.pack_kind,
                                "crc_valid": f.crc_valid,
                            })
                        })
                        .collect();
                    let out = serde_json::json!({
                        "exists": true,
                        "path": target_path.display().to_string(),
                        "frame_count": rep.frames.len(),
                        "valid_prefix_bytes": rep.valid_prefix_bytes,
                        "damaged_offset": rep.damaged_offset,
                        "damaged_reason": rep.damaged_reason,
                        "frames": frames,
                    });
                    println!("{out}");
                    if rep.damaged_offset.is_some() {
                        ExitCode::from(4)
                    } else {
                        ExitCode::SUCCESS
                    }
                }
                Err(e) => {
                    let ce = lin::Error::corrupt(e.message);
                    emit_lin_error(&ce)
                }
            }
        }
        Some("storage") => {
            let Some(dir) = data else {
                return emit_usage_error("usage: lin --data <dir> inspect storage");
            };
            let snap_path = lin::persist::snapshot_path(&dir);
            let wal_path = lin::persist::log_path(&dir);
            let snap_rep = match lin::persist::inspect_snapshot_file(&snap_path) {
                Ok(r) => r,
                Err(e) => return emit_lin_error(&e),
            };
            let wal_rep = if wal_path.exists() {
                match lin::persist::inspect_wal_file(&wal_path) {
                    Ok(r) => Some(r),
                    Err(e) => return emit_lin_error(&e),
                }
            } else {
                None
            };
            let recovery = match lin::persist::dry_run_recovery(&dir) {
                Ok(r) => r,
                Err(e) => return emit_lin_error(&e),
            };
            let head = lin::persist::read_head(&dir).ok().flatten();
            let out = serde_json::json!({
                "dir": dir.display().to_string(),
                "exists": dir.exists(),
                "head": head.map(|h| serde_json::json!({
                    "gen": h.r#gen,
                    "catalog_hash": h.catalog_hash,
                    "embed_id": h.embed_id,
                })),
                "snapshot": {
                    "exists": snap_rep.exists,
                    "byte_len": snap_rep.byte_len,
                    "format_version": snap_rep.format_version,
                    "crc_trailer_present": snap_rep.crc_trailer_present,
                    "crc_valid": snap_rep.crc_valid,
                    "gen": snap_rep.r#gen,
                    "collection_count": snap_rep.collection_count,
                    "cold_collections": snap_rep.cold_collections,
                },
                "wal": wal_rep.as_ref().map(|w| serde_json::json!({
                    "exists": true,
                    "frame_count": w.frames.len(),
                    "valid_prefix_bytes": w.valid_prefix_bytes,
                    "damaged_offset": w.damaged_offset,
                    "damaged_reason": w.damaged_reason,
                })).unwrap_or_else(|| serde_json::json!({
                    "exists": false,
                    "frame_count": 0,
                    "valid_prefix_bytes": 0,
                })),
                "recovery_dry_run": {
                    "snapshot_gen": recovery.snapshot_gen,
                    "head_gen": recovery.head_gen,
                    "replayable_frames": recovery.replayable_frames,
                    "final_gen": recovery.final_gen,
                    "truncate_to_offset": recovery.truncate_to_offset,
                    "damaged_offset": recovery.damaged_offset,
                    "decision_log": recovery.decision_log,
                }
            });
            println!("{out}");
            ExitCode::SUCCESS
        }
        Some("indexes") => {
            let cat_info = match data {
                Some(dir) => match Db::open_read(&dir) {
                    Ok(rdb) => {
                        let tmp_db = Db::builder()
                            .data_dir(&dir)
                            .read_only(true)
                            .build();
                        drop(rdb);
                        match tmp_db {
                            Ok(db) => lin::migration::inspect_db_catalog(&db),
                            Err(e) => return emit_lin_error(&e),
                        }
                    }
                    Err(e) => return emit_lin_error(&e),
                },
                None => {
                    let db = Db::fixture();
                    lin::migration::inspect_db_catalog(&db)
                }
            };
            let scalar: Vec<serde_json::Value> = cat_info
                .scalar_indexes
                .values()
                .map(|ix| {
                    serde_json::json!({
                        "label": ix.label,
                        "collection": ix.collection,
                        "unique": ix.unique,
                        "fields": ix.fields,
                        "partial_pred": ix.partial_pred,
                    })
                })
                .collect();
            let out = serde_json::json!({
                "schema_hash": cat_info.schema_hash,
                "embedder_id": cat_info.embedder_id,
                "scalar_indexes": scalar,
                "fts_fields": cat_info.fts_fields,
            });
            println!("{out}");
            ExitCode::SUCCESS
        }
        Some("locks") => {
            let Some(dir) = data else {
                return emit_usage_error("usage: lin --data <dir> inspect locks");
            };
            let lock_file = lin::persist::lock_path(&dir);
            let fence_file = lin::persist::fence_path(&dir);
            let lock_exists = lock_file.exists();
            let fence_exists = fence_file.exists();
            let writer_locked = lock_exists && probe_nonblocking_exclusive_flock(&lock_file);
            let fence_locked = fence_exists && probe_nonblocking_exclusive_flock(&fence_file);
            let out = serde_json::json!({
                "dir": dir.display().to_string(),
                "lock_path": lock_file.display().to_string(),
                "lock_exists": lock_exists,
                "writer_locked": writer_locked,
                "fence_path": fence_file.display().to_string(),
                "fence_exists": fence_exists,
                "fence_locked": fence_locked,
            });
            println!("{out}");
            ExitCode::SUCCESS
        }
        _ => emit_usage_error("usage: lin [--data <dir>] inspect storage|wal|indexes|locks"),
    }
}

fn cmd_import(args: &[String], data: Option<PathBuf>, follower: bool) -> ExitCode {
    if follower {
        return emit_lin_error(&lin::Error::permission_denied(
            "import is not allowed in --follower mode",
        ));
    }
    let mut collection: Option<String> = None;
    let mut format: Option<String> = None;
    let mut file: Option<PathBuf> = None;
    let mut reject_file: Option<PathBuf> = None;
    let mut batch_size: usize = 1024;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--collection" | "-c" => {
                i += 1;
                collection = args.get(i).cloned();
            }
            "--format" | "-f" => {
                i += 1;
                format = args.get(i).cloned();
            }
            "--file" | "--in" => {
                i += 1;
                file = args.get(i).map(PathBuf::from);
            }
            "--reject-file" => {
                i += 1;
                reject_file = args.get(i).map(PathBuf::from);
            }
            "--batch-size" => {
                i += 1;
                let Some(Ok(n)) = args.get(i).map(|s| s.parse::<usize>()) else {
                    return emit_usage_error("--batch-size requires a positive integer");
                };
                batch_size = n.max(1);
            }
            other => {
                return emit_usage_error(&format!("unknown import option {other:?}"));
            }
        }
        i += 1;
    }

    let Some(col) = collection else {
        return emit_usage_error(
            "usage: lin [--data <dir>] import --collection <col> --format jsonl|csv [--file <path>]",
        );
    };
    let Some(fmt) = format else {
        return emit_usage_error(
            "usage: lin [--data <dir>] import --collection <col> --format jsonl|csv [--file <path>]",
        );
    };
    if fmt != "jsonl" && fmt != "csv" {
        return emit_usage_error(&format!(
            "unsupported import --format {fmt:?} (expected jsonl|csv)"
        ));
    }

    let limits = lin::io_formats::ImportLimits {
        batch_size,
        ..lin::io_formats::ImportLimits::default()
    };
    let mapping = lin::io_formats::ImportSchemaMapping::default();

    let run_import = |db: &mut Db| -> Result<lin::io_formats::ImportReceipt, lin::Error> {
        let mut rej_file_opt = match &reject_file {
            Some(p) => Some(
                std::fs::File::create(p)
                    .map_err(|e| lin::Error::io(format!("create reject file {}: {e}", p.display())))?,
            ),
            None => None,
        };
        let reject_writer: Option<&mut dyn io::Write> = match rej_file_opt.as_mut() {
            Some(f) => Some(f),
            None => None,
        };
        let mut progress = |_bytes: u64, _rows: usize| {};
        let never_cancel = || false;

        match &file {
            Some(path) => {
                let f = std::fs::File::open(path)
                    .map_err(|e| lin::Error::io(format!("open {}: {e}", path.display())))?;
                let mut reader = io::BufReader::new(f);
                if fmt == "jsonl" {
                    lin::io_formats::import_jsonl_reader(
                        db,
                        &col,
                        &mut reader,
                        &limits,
                        &mapping,
                        reject_writer,
                        &mut progress,
                        &never_cancel,
                    )
                } else {
                    lin::io_formats::import_csv_reader(
                        db,
                        &col,
                        &mut reader,
                        &limits,
                        &mapping,
                        reject_writer,
                        &mut progress,
                        &never_cancel,
                    )
                }
            }
            None => {
                let stdin = io::stdin();
                let mut reader = stdin.lock();
                if fmt == "jsonl" {
                    lin::io_formats::import_jsonl_reader(
                        db,
                        &col,
                        &mut reader,
                        &limits,
                        &mapping,
                        reject_writer,
                        &mut progress,
                        &never_cancel,
                    )
                } else {
                    lin::io_formats::import_csv_reader(
                        db,
                        &col,
                        &mut reader,
                        &limits,
                        &mapping,
                        reject_writer,
                        &mut progress,
                        &never_cancel,
                    )
                }
            }
        }
    };

    let res = match data {
        Some(dir) => with_data(Some(dir), false, |db| run_import(db.unwrap())),
        None => {
            let mut db = Db::fixture();
            run_import(&mut db)
        }
    };

    match res {
        Ok(receipt) => {
            let out = serde_json::json!({
                "collection": col,
                "format": fmt,
                "imported_rows": receipt.imported_rows,
                "skipped_rows": receipt.skipped_rows,
                "rejected_rows": receipt.rejected_rows,
                "bytes_read": receipt.bytes_read,
                "batches_committed": receipt.batches_committed,
            });
            println!("{out}");
            ExitCode::SUCCESS
        }
        Err(e) => emit_lin_error(&e),
    }
}

fn cmd_export(args: &[String], data: Option<PathBuf>, follower: bool) -> ExitCode {
    let mut query: Option<String> = None;
    let mut format: Option<String> = None;
    let mut out_path: Option<PathBuf> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--query" | "-q" => {
                i += 1;
                query = args.get(i).cloned();
            }
            "--format" | "-f" => {
                i += 1;
                format = args.get(i).cloned();
            }
            "--out" | "-o" => {
                i += 1;
                out_path = args.get(i).map(PathBuf::from);
            }
            other => {
                return emit_usage_error(&format!("unknown export option {other:?}"));
            }
        }
        i += 1;
    }

    let Some(q) = query else {
        return emit_usage_error(
            "usage: lin [--data <dir>] export --query '<q>' --format jsonl|csv [--out <path>]",
        );
    };
    let Some(fmt) = format else {
        return emit_usage_error(
            "usage: lin [--data <dir>] export --query '<q>' --format jsonl|csv [--out <path>]",
        );
    };
    if fmt != "jsonl" && fmt != "csv" {
        return emit_usage_error(&format!(
            "unsupported export --format {fmt:?} (expected jsonl|csv)"
        ));
    }

    let do_export = |db: &mut Db| -> Result<usize, lin::Error> {
        match &out_path {
            Some(path) => {
                let f = std::fs::File::create(path)
                    .map_err(|e| lin::Error::io(format!("create {}: {e}", path.display())))?;
                let mut writer = io::BufWriter::new(f);
                if fmt == "jsonl" {
                    lin::io_formats::export_jsonl_writer(db, &q, &mut writer)
                } else {
                    lin::io_formats::export_csv_writer(db, &q, &mut writer)
                }
            }
            None => {
                let stdout = io::stdout();
                let mut writer = io::BufWriter::new(stdout.lock());
                if fmt == "jsonl" {
                    lin::io_formats::export_jsonl_writer(db, &q, &mut writer)
                } else {
                    lin::io_formats::export_csv_writer(db, &q, &mut writer)
                }
            }
        }
    };

    let res = match data {
        Some(dir) => with_data(Some(dir), follower, |db| do_export(db.unwrap())),
        None => {
            let mut db = Db::fixture();
            do_export(&mut db)
        }
    };

    match res {
        Ok(_) => ExitCode::SUCCESS,
        Err(e) => emit_lin_error(&e),
    }
}

fn build_health_snapshot(
    data: Option<PathBuf>,
    follower: bool,
    check_ready: bool,
) -> (serde_json::Value, ExitCode) {
    let cold = COLD.load(Ordering::Relaxed);
    match data {
        None => {
            let db = Db::fixture();
            let s = db.stats();
            let out = serde_json::json!({
                "status": "healthy",
                "ready": true,
                "mode": "memory",
                "version": lin::VERSION,
                "gen": s.r#gen,
                "docs": s.docs,
                "cold": cold,
                "follower": follower,
            });
            (out, ExitCode::SUCCESS)
        }
        Some(dir) => {
            let recovery = match lin::persist::dry_run_recovery(&dir) {
                Ok(r) => r,
                Err(e) => {
                    let out = serde_json::json!({
                        "status": "unhealthy",
                        "ready": false,
                        "dir": dir.display().to_string(),
                        "error": e.message,
                    });
                    return (out, ExitCode::from(4));
                }
            };
            let corrupt = recovery.damaged_offset.is_some() && recovery.truncate_to_offset.is_none();
            if corrupt {
                let out = serde_json::json!({
                    "status": "unhealthy",
                    "ready": false,
                    "dir": dir.display().to_string(),
                    "damaged_offset": recovery.damaged_offset,
                    "decision_log": recovery.decision_log,
                });
                return (out, ExitCode::from(4));
            }
            let writer_locked = {
                let lp = lin::persist::lock_path(&dir);
                lp.exists() && probe_nonblocking_exclusive_flock(&lp)
            };
            let ready = if check_ready {
                if !dir.exists() {
                    true
                } else {
                    Db::open_read(&dir).is_ok()
                }
            } else {
                true
            };
            let code = if ready {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(3)
            };
            let out = serde_json::json!({
                "status": if ready { "healthy" } else { "degraded" },
                "ready": ready,
                "mode": "durable",
                "dir": dir.display().to_string(),
                "exists": dir.exists(),
                "version": lin::VERSION,
                "gen": recovery.final_gen,
                "replayable_frames": recovery.replayable_frames,
                "writer_locked": writer_locked,
                "cold": cold,
                "follower": follower,
            });
            (out, code)
        }
    }
}

fn cmd_health(data: Option<PathBuf>, follower: bool) -> ExitCode {
    let (out, code) = build_health_snapshot(data, follower, false);
    println!("{out}");
    code
}

fn cmd_readiness(data: Option<PathBuf>, follower: bool) -> ExitCode {
    let (out, code) = build_health_snapshot(data, follower, true);
    println!("{out}");
    code
}

fn cmd_config(args: &[String], data: Option<PathBuf>, follower: bool) -> ExitCode {
    if args.first().map(String::as_str) != Some("effective") {
        return emit_usage_error("usage: lin [--data <dir>] config effective");
    }
    let mut env_overrides = serde_json::Map::new();
    let resolved_data = match data {
        Some(d) => Some(d.display().to_string()),
        None => std::env::var("LIN_DATA_DIR").ok().filter(|s| !s.is_empty()).inspect(|v| {
            env_overrides.insert(
                "LIN_DATA_DIR".to_string(),
                serde_json::Value::String(v.clone()),
            );
        }),
    };
    let mut cold = COLD.load(Ordering::Relaxed);
    if let Ok(v) = std::env::var("LIN_COLD")
        && !v.is_empty()
    {
        env_overrides.insert("LIN_COLD".to_string(), serde_json::Value::String(v.clone()));
        if matches!(v.to_ascii_lowercase().as_str(), "1" | "true" | "yes") {
            cold = true;
        }
    }
    let mut eff_follower = follower;
    if let Ok(v) = std::env::var("LIN_FOLLOWER")
        && !v.is_empty()
    {
        env_overrides.insert(
            "LIN_FOLLOWER".to_string(),
            serde_json::Value::String(v.clone()),
        );
        if matches!(v.to_ascii_lowercase().as_str(), "1" | "true" | "yes") {
            eff_follower = true;
        }
    }
    let mut quotas = lin::Quotas::default();
    if let Ok(v) = std::env::var("LIN_MAX_ROWS")
        && let Ok(n) = v.parse::<usize>()
    {
        quotas.max_rows = n;
        env_overrides.insert("LIN_MAX_ROWS".to_string(), serde_json::Value::Number(n.into()));
    }
    if let Ok(v) = std::env::var("LIN_MAX_LOG_BYTES")
        && let Ok(n) = v.parse::<u64>()
    {
        quotas.max_log_bytes = n;
        env_overrides.insert(
            "LIN_MAX_LOG_BYTES".to_string(),
            serde_json::Value::Number(n.into()),
        );
    }
    if let Ok(v) = std::env::var("LIN_MAX_QUERY_BYTES")
        && let Ok(n) = v.parse::<usize>()
    {
        quotas.max_query_bytes = n;
        env_overrides.insert(
            "LIN_MAX_QUERY_BYTES".to_string(),
            serde_json::Value::Number(n.into()),
        );
    }

    let out = serde_json::json!({
        "data_dir": resolved_data,
        "cold": cold,
        "follower": eff_follower,
        "json_errors": JSON_ERRORS.load(Ordering::Relaxed),
        "quotas": {
            "max_rows": quotas.max_rows,
            "max_edges": quotas.max_edges,
            "max_log_bytes": quotas.max_log_bytes,
            "max_query_bytes": quotas.max_query_bytes,
            "max_ast_depth": quotas.max_ast_depth,
            "max_output_bytes": quotas.max_output_bytes,
            "spill_enabled": quotas.spill_enabled,
        },
        "env_overrides": env_overrides,
    });
    println!("{out}");
    ExitCode::SUCCESS
}

fn cmd_completions(args: &[String]) -> ExitCode {
    match args.first().map(String::as_str) {
        Some("bash") => {
            println!(
                r#"_lin_completions() {{
    local cur prev cmds
    COMPREPLY=()
    cur="${{COMP_WORDS[COMP_CWORD]}}"
    prev="${{COMP_WORDS[COMP_CWORD-1]}}"
    cmds="run explain import export inspect health readiness backup config completions repl stats cluster compact serve client shard remote follower wal-serve apply-wal cache recover repair-catalog-hash version"
    if [[ ${{COMP_CWORD}} -eq 1 ]]; then
        COMPREPLY=( $(compgen -W "${{cmds}} --data --cold --follower --json-errors" -- "${{cur}}") )
        return 0
    fi
    case "${{prev}}" in
        --format) COMPREPLY=( $(compgen -W "table json jsonl csv" -- "${{cur}}") ) ;;
        inspect) COMPREPLY=( $(compgen -W "storage wal indexes locks" -- "${{cur}}") ) ;;
        backup) COMPREPLY=( $(compgen -W "export import verify" -- "${{cur}}") ) ;;
        completions) COMPREPLY=( $(compgen -W "bash zsh fish" -- "${{cur}}") ) ;;
        config) COMPREPLY=( $(compgen -W "effective" -- "${{cur}}") ) ;;
    esac
}}
complete -F _lin_completions lin"#
            );
            ExitCode::SUCCESS
        }
        Some("zsh") => {
            println!(
                r#"#compdef lin
_lin() {{
    local -a subcmds
    subcmds=(
        'run:Execute a LinQL query or statement'
        'explain:Explain a LinQL query plan'
        'import:Stream JSONL or CSV rows into a collection'
        'export:Stream query results to JSONL or CSV'
        'inspect:Inspect storage, WAL, indexes, or locks'
        'health:Report database health JSON'
        'readiness:Report database readiness JSON'
        'backup:Export, import, or verify portable backups'
        'config:Show effective runtime configuration'
        'completions:Generate shell completion scripts'
        'repl:Start interactive multiline LinQL shell'
        'stats:Show database counters'
        'version:Show version and build capabilities'
    )
    _arguments -C \
        '--data[Data directory]:dir:_files -/' \
        '--cold[Enable disk tier]' \
        '--follower[Open as read-only follower]' \
        '--json-errors[Emit structured JSON errors]' \
        '1:command:->cmd' \
        '*::arg:->args'
    case $state in
        cmd) _describe 'command' subcmds ;;
    esac
}}
_lin "$@""#
            );
            ExitCode::SUCCESS
        }
        Some("fish") => {
            println!(
                r#"complete -c lin -l data -d 'Data directory' -r
complete -c lin -l cold -d 'Enable disk tier'
complete -c lin -l follower -d 'Open as read-only follower'
complete -c lin -l json-errors -d 'Emit structured JSON errors'
for cmd in run explain import export inspect health readiness backup config completions repl stats cluster compact serve client shard remote follower wal-serve apply-wal cache recover repair-catalog-hash version
    complete -c lin -n '__fish_use_subcommand' -a $cmd
end"#
            );
            ExitCode::SUCCESS
        }
        _ => emit_usage_error("usage: lin completions bash|zsh|fish"),
    }
}

fn find_unquoted_semicolon(buf: &str) -> Option<usize> {
    let bytes = buf.as_bytes();
    let mut i = 0usize;
    let mut in_quote: Option<u8> = None;
    let mut in_comment = false;
    while i < bytes.len() {
        let b = bytes[i];
        if in_comment {
            if b == b'\n' {
                in_comment = false;
            }
            i += 1;
            continue;
        }
        if let Some(q) = in_quote {
            if b == b'\\' {
                i = (i + 2).min(bytes.len());
                continue;
            }
            if b == q {
                in_quote = None;
            }
            i += 1;
            continue;
        }
        if b == b'-' && bytes.get(i + 1) == Some(&b'-') {
            in_comment = true;
            i += 2;
            continue;
        }
        if b == b'"' || b == b'\'' {
            in_quote = Some(b);
            i += 1;
            continue;
        }
        if b == b';' {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn cmd_repl(args: &[String], data: Option<PathBuf>, follower: bool) -> ExitCode {
    use std::io::{BufRead, IsTerminal, Write};

    let mut format = RunOutputFormat::Compact;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--format" => {
                i += 1;
                match args.get(i).map(String::as_str) {
                    Some("table") => format = RunOutputFormat::Table,
                    Some("json") => format = RunOutputFormat::Json,
                    Some("jsonl") => format = RunOutputFormat::Jsonl,
                    Some("compact") => format = RunOutputFormat::Compact,
                    Some(other) => {
                        return emit_usage_error(&format!(
                            "unknown repl --format {other:?} (expected table|json|jsonl)"
                        ));
                    }
                    None => {
                        return emit_usage_error(
                            "usage: lin [--data <dir>] repl [--format table|json|jsonl]",
                        );
                    }
                }
            }
            other => {
                return emit_usage_error(&format!("unknown repl option {other:?}"));
            }
        }
        i += 1;
    }

    let cold = COLD.load(Ordering::Relaxed);
    let mut db = match &data {
        Some(dir) => {
            let res = if follower {
                Db::open_follower(dir)
            } else if cold {
                Db::open_with(
                    dir,
                    lin::OpenOpts {
                        cold: true,
                        ..lin::OpenOpts::default()
                    },
                )
            } else {
                Db::open(dir)
            };
            match res {
                Ok(d) => d,
                Err(e) => return emit_lin_error(&e),
            }
        }
        None => {
            if follower {
                return emit_lin_error(&lin::Error::runtime("--follower requires --data <dir>"));
            }
            Db::fixture()
        }
    };

    let stdin = io::stdin();
    let interactive = stdin.is_terminal();
    let stdout = io::stdout();
    let mut out = io::BufWriter::new(stdout.lock());
    if interactive {
        let _ = writeln!(
            out,
            "Lin {} REPL — end multiline statements with `;` (`:quit` to exit)",
            lin::VERSION
        );
        let _ = out.flush();
    }

    let mut pending = String::new();
    let mut lines = stdin.lock().lines();
    let mut last_err: Option<lin::Error> = None;

    loop {
        if interactive {
            let prompt = if pending.trim().is_empty() {
                "lin> "
            } else {
                "  .. "
            };
            let _ = write!(out, "{prompt}");
            let _ = out.flush();
        }
        let Some(line_res) = lines.next() else {
            if interactive {
                let _ = writeln!(out);
            }
            break;
        };
        let line = match line_res {
            Ok(l) => l,
            Err(e) => {
                last_err = Some(lin::Error::io(format!("repl stdin read error: {e}")));
                break;
            }
        };
        if pending.trim().is_empty() {
            let trimmed = line.trim();
            if matches!(trimmed, ":quit" | ":q" | ":exit" | "quit" | "exit") {
                break;
            }
            if trimmed.is_empty() {
                continue;
            }
        }
        if !pending.is_empty() {
            pending.push('\n');
        }
        pending.push_str(&line);

        while let Some(semi_idx) = find_unquoted_semicolon(&pending) {
            let stmt_src = pending[..semi_idx].trim().to_string();
            pending.drain(..=semi_idx);
            if stmt_src.is_empty() {
                continue;
            }
            if matches!(stmt_src.as_str(), ":quit" | ":q" | ":exit" | "quit" | "exit") {
                pending.clear();
                let _ = out.flush();
                let _ = db.close();
                return ExitCode::SUCCESS;
            }
            match db.run(&stmt_src) {
                Ok(h) => {
                    write_handle_formatted(&mut out, &h, format);
                    let _ = out.flush();
                }
                Err(e) => {
                    let _ = out.flush();
                    let _ = emit_lin_error(&e);
                    if !interactive {
                        last_err = Some(e);
                    }
                }
            }
        }
    }

    let tail = pending.trim();
    if !tail.is_empty() && !matches!(tail, ":quit" | ":q" | ":exit" | "quit" | "exit") {
        match db.run(tail) {
            Ok(h) => {
                write_handle_formatted(&mut out, &h, format);
            }
            Err(e) => {
                let _ = out.flush();
                let _ = emit_lin_error(&e);
                last_err = Some(e);
            }
        }
    }
    let _ = out.flush();
    if let Err(e) = db.close() {
        return emit_lin_error(&e);
    }
    if let Some(e) = last_err
        && !interactive
    {
        return ExitCode::from(exit_code_for_error(&e));
    }
    ExitCode::SUCCESS
}
