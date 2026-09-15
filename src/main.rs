use std::io::{self, Read};
use std::path::PathBuf;
use std::process::ExitCode;

use lin::{Db, GraphFmt, explain_as};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (data, follower, rest) = parse_global(&args);
    match rest.first().map(String::as_str) {
        Some("explain") => cmd_explain(&rest[1..], data, follower),
        Some("run") => cmd_run(&rest[1..], data, follower),
        Some("backup") => cmd_backup(&rest[1..], data),
        Some("apply-wal") => cmd_apply_wal(&rest[1..], data, follower),
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
    let mut file: Option<PathBuf> = None;
    let mut rest = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
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
                            "usage: lin [--data <dir>] run [--explain] [--file <path> | - | '<query>']"
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
        eprintln!("usage: lin [--data <dir>] run [--explain] [--file <path> | - | '<query>']");
        return ExitCode::from(2);
    }
    match with_data(data, follower, |db| match db {
        Some(db) => {
            let h = db.run(&q)?;
            let plan = if show_plan {
                Some(db.explain_as(&q, None)?)
            } else {
                None
            };
            Ok((h, plan))
        }
        None => {
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
                return Err(lin::Error::runtime(
                    "--follower requires --data <dir>",
                ));
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
         lin [--data <dir>] [--follower] run [--explain] [--file <path> | - | '<query>']\n  \
         lin [--data <dir>] [--follower] explain [--graph mermaid|dot] '<query>'\n  \
         lin --data <dir> --follower apply-wal <frames.bin | ->\n  \
         lin [--data <dir>] backup export <file.json>\n  \
         lin backup import <file.json> [--data <dir>]\n  \
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
                "gen={} docs={} facts={} edges={} next_id={} log_bytes={} reopen_ms={} writes_since_snapshot={} sync_normal={} cold={} append_rows={} append_rows_per_s={:.0}",
                s.r#gen,
                s.docs,
                s.facts,
                s.edges,
                s.next_id,
                s.log_bytes,
                s.reopen_ms,
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
