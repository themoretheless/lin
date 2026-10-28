//! Equivalent owned id/title point lookup; legacy compare point_get cases remain unchanged.
use airbug_bench::{Config, DropPolicy, Suite};
use std::hint::black_box;
use std::time::Duration;
type Pair = (String, String);
const N: usize = 10_000;
const SQLITE_QUERY: &str = "SELECT id, title FROM docs WHERE id = ?1";
const DUCK_QUERY: &str = "SELECT id, title FROM docs WHERE id = ?";
fn doc(i: usize) -> Pair {
    (
        format!("doc-{i:06}"),
        match i % 3 {
            0 => format!("ЁЖ 🦔 title \"{i}\" \\ tail"),
            1 => format!("long title {i} {}", "λ".repeat(40)),
            _ => format!("short {i}"),
        },
    )
}
fn docs() -> Vec<Pair> {
    (0..N).map(doc).collect()
}
fn key(hit: bool) -> String {
    if hit { "doc-000042" } else { "missing-doc" }.into()
}
fn expected(hit: bool) -> Vec<Pair> {
    if hit { vec![doc(42)] } else { vec![] }
}
fn validate(actual: &[Pair], expected: &[Pair]) -> airbug_bench::Result<()> {
    if actual == expected {
        Ok(())
    } else {
        Err(airbug_bench::error(
            "point_values: id/title bytes or multiplicity mismatch",
        ))
    }
}
struct LinInput {
    db: lin::Db,
    query: lin::Prepared,
    expected: Vec<Pair>,
}
fn lin_input(hit: bool) -> LinInput {
    let mut db = lin::Db::empty();
    let data = docs();
    for chunk in data.chunks(500) {
        let values = chunk
            .iter()
            .map(|(id, title)| {
                format!(
                    "{{id:{},title:{}}}",
                    serde_json::to_string(id).unwrap(),
                    serde_json::to_string(title).unwrap()
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        db.run(&format!("insert docs [{values}]"))
            .expect("lin seed");
    }
    let query = db
        .prepare(&format!(
            "docs | id == {} | {{id,title}}",
            serde_json::to_string(&key(hit)).unwrap()
        ))
        .expect("lin prepare");
    LinInput {
        db,
        query,
        expected: expected(hit),
    }
}
fn lin_rows(s: &mut LinInput) -> Vec<Pair> {
    let handle = s.query.run(&mut s.db).expect("lin point rows");
    black_box(
        handle
            .rows
            .iter()
            .map(|row| {
                (
                    row.get("id")
                        .and_then(lin::Cell::text)
                        .expect("id")
                        .to_owned(),
                    row.get("id")
                        .and_then(lin::Cell::text)
                        .expect("title")
                        .to_owned(),
                )
            })
            .collect(),
    )
}
struct SqliteInput {
    conn: rusqlite::Connection,
    key: String,
    expected: Vec<Pair>,
}
fn sqlite_input(hit: bool) -> SqliteInput {
    let mut conn = rusqlite::Connection::open_in_memory().expect("sqlite open");
    conn.execute_batch("CREATE TABLE docs(id TEXT PRIMARY KEY, title TEXT NOT NULL)")
        .expect("sqlite schema");
    {
        let tx = conn.transaction().expect("sqlite tx");
        {
            let mut stmt = tx.prepare("INSERT INTO docs VALUES(?1,?2)").unwrap();
            for (id, title) in docs() {
                stmt.execute(rusqlite::params![id, title]).unwrap();
            }
        }
        tx.commit().unwrap();
    }
    drop(conn.prepare_cached(SQLITE_QUERY).expect("sqlite prepare"));
    SqliteInput {
        conn,
        key: key(hit),
        expected: expected(hit),
    }
}
fn sqlite_rows(s: &mut SqliteInput) -> Vec<Pair> {
    let mut stmt = s.conn.prepare_cached(SQLITE_QUERY).expect("sqlite cached");
    let mut rows = stmt.query(rusqlite::params![s.key]).expect("sqlite query");
    let mut output = Vec::new();
    while let Some(row) = rows.next().expect("sqlite next") {
        output.push((
            row.get::<_, String>(0).expect("id"),
            row.get::<_, String>(1).expect("title"),
        ));
    }
    black_box(output)
}
struct DuckInput {
    conn: duckdb::Connection,
    key: String,
    expected: Vec<Pair>,
}
fn duck_input(hit: bool) -> DuckInput {
    let mut conn = duckdb::Connection::open_in_memory().expect("duck open");
    conn.execute_batch("CREATE TABLE docs(id VARCHAR PRIMARY KEY, title VARCHAR NOT NULL)")
        .expect("duck schema");
    {
        let tx = conn.transaction().expect("duck tx");
        {
            let mut stmt = tx.prepare("INSERT INTO docs VALUES(?,?)").unwrap();
            for (id, title) in docs() {
                stmt.execute(duckdb::params![id, title]).unwrap();
            }
        }
        tx.commit().unwrap();
    }
    drop(conn.prepare_cached(DUCK_QUERY).expect("duck prepare"));
    DuckInput {
        conn,
        key: key(hit),
        expected: expected(hit),
    }
}
fn duck_rows(s: &mut DuckInput) -> Vec<Pair> {
    let mut stmt = s.conn.prepare_cached(DUCK_QUERY).expect("duck cached");
    let mut rows = stmt.query(duckdb::params![s.key]).expect("duck query");
    let mut output = Vec::new();
    while let Some(row) = rows.next().expect("duck next") {
        output.push((
            row.get::<_, String>(0).expect("id"),
            row.get::<_, String>(1).expect("title"),
        ));
    }
    black_box(output)
}
fn verify_oracle() -> airbug_bench::Result<()> {
    for hit in [false, true] {
        let mut l = lin_input(hit);
        let actual = lin_rows(&mut l);
        validate(&actual, &l.expected)?;
        let mut s = sqlite_input(hit);
        let actual = sqlite_rows(&mut s);
        validate(&actual, &s.expected)?;
        let mut d = duck_input(hit);
        let actual = duck_rows(&mut d);
        validate(&actual, &d.expected)?;
    }
    let good = expected(true);
    let mut wrong_id = good.clone();
    wrong_id[0].0.push('x');
    let mut wrong_title = good.clone();
    wrong_title[0].1.push('x');
    let mut duplicate = good.clone();
    duplicate.push(good[0].clone());
    for bad in [wrong_id, wrong_title, duplicate, vec![]] {
        assert!(validate(&bad, &good).is_err());
    }
    assert!(validate(&good, &[]).is_err());
    println!(
        "{{\"full_value_checks\":6,\"negative_oracle_checks\":5,\"performance_qualified\":false}}"
    );
    Ok(())
}
fn main() -> airbug_bench::Result<()> {
    let args = std::env::args()
        .skip(1)
        .filter(|x| x != "--bench")
        .collect::<Vec<_>>();
    if args == ["--verify-oracle"] {
        return verify_oracle();
    }
    let mut suite = Suite::new("point_values");
    suite.config(Config::profile("quick")?);
    for hit in [false, true] {
        let label = if hit { "hit" } else { "miss" };
        suite
            .bench_checked(
                &format!("{label}/lin"),
                move || lin_input(hit),
                lin_rows,
                |s, rows| validate(rows, &s.expected),
                DropPolicy::InsideTiming,
            )
            .parameter("rows", N);
        suite
            .bench_checked(
                &format!("{label}/sqlite"),
                move || sqlite_input(hit),
                sqlite_rows,
                |s, rows| validate(rows, &s.expected),
                DropPolicy::InsideTiming,
            )
            .parameter("rows", N);
        suite
            .bench_checked(
                &format!("{label}/duckdb"),
                move || duck_input(hit),
                duck_rows,
                |s, rows| validate(rows, &s.expected),
                DropPolicy::InsideTiming,
            )
            .parameter("rows", N);
    }
    run_suite(suite, &args)
}

fn run_suite(mut suite: Suite<'_>, args: &[String]) -> airbug_bench::Result<()> {
    let mut config = Config::profile("quick")?;
    let mut profile = None;
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--profile" {
            i += 1;
            profile = Some(
                args.get(i)
                    .ok_or_else(|| airbug_bench::error("--profile requires value"))?
                    .clone(),
            );
        }
        i += 1;
    }
    if let Some(p) = &profile {
        config = Config::profile(p)?;
    }
    let mut selection = airbug_bench::Selection::default();
    let mut list = false;
    let mut json = false;
    let mut output = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--profile" => {
                i += 1;
            }
            "--list" => list = true,
            "--json" => json = true,
            "--exact" => selection.exact = true,
            "--glob" => selection.glob = true,
            "--filter" => {
                i += 1;
                selection.pattern = args
                    .get(i)
                    .ok_or_else(|| airbug_bench::error("--filter requires value"))?
                    .clone();
            }
            "--tag" => {
                i += 1;
                selection.tags.push(
                    args.get(i)
                        .ok_or_else(|| airbug_bench::error("--tag requires value"))?
                        .clone(),
                );
            }
            "--exclude" => {
                i += 1;
                selection.exclude.push(
                    args.get(i)
                        .ok_or_else(|| airbug_bench::error("--exclude requires glob"))?
                        .clone(),
                );
            }
            "--samples" => {
                i += 1;
                config.samples = args
                    .get(i)
                    .ok_or_else(|| airbug_bench::error("--samples requires value"))?
                    .parse()?;
            }
            "--max-iterations" => {
                i += 1;
                config.max_iterations = args
                    .get(i)
                    .ok_or_else(|| airbug_bench::error("--max-iterations requires value"))?
                    .parse()?;
            }
            "--sample-ms" => {
                i += 1;
                config.sample_time = Duration::from_millis(
                    args.get(i)
                        .ok_or_else(|| airbug_bench::error("--sample-ms requires value"))?
                        .parse()?,
                );
            }
            "--warmup-ms" => {
                i += 1;
                config.warmup = Duration::from_millis(
                    args.get(i)
                        .ok_or_else(|| airbug_bench::error("--warmup-ms requires value"))?
                        .parse()?,
                );
            }
            "--output" => {
                i += 1;
                output = Some(
                    args.get(i)
                        .ok_or_else(|| airbug_bench::error("--output requires directory"))?
                        .clone(),
                );
            }
            "--help" | "-h" => {
                println!(
                    "--list --profile quick|normal|thorough --filter TEXT [--exact|--glob] --exclude GLOB --tag TAG --samples N --max-iterations N --sample-ms N --warmup-ms N --json --output NEW_DIRECTORY"
                );
                return Ok(());
            }
            other => return Err(airbug_bench::error(format!("unknown argument {other}"))),
        }
        i += 1;
    }
    suite.config(config);
    selection.validate()?;
    if list {
        for id in suite.list_selected(&selection) {
            println!("{id}");
        }
        return Ok(());
    }
    if cfg!(debug_assertions) {
        return Err(airbug_bench::error(
            "benchmarks require an optimized build; use cargo bench or cargo run --release",
        ));
    }
    let selected = suite.list_selected(&selection);
    let total = selected.len();
    if let Some(p) = output.as_ref() {
        let dir = std::path::Path::new(p);
        if let Some(parent) = dir.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent).map_err(|e| airbug_bench::error(e.to_string()))?;
            }
        }
        if !dir.exists() {
            std::fs::create_dir(dir).map_err(|e| airbug_bench::error(e.to_string()))?;
        }
        let progress = serde_json::json!({
            "state": "running",
            "completed": 0,
            "total": total,
            "variant": "point_values"
        });
        std::fs::write(
            dir.join("progress.json"),
            serde_json::to_string_pretty(&progress)?,
        )
        .map_err(|e| airbug_bench::error(e.to_string()))?;
        if let Ok(url) = std::env::var("AIRBUG_DASH_URL") {
            eprintln!("airbug dash: {url}");
        } else if let Ok(hub) = std::env::var("AIRBUG_HUB") {
            eprintln!("airbug dash: {hub}/#/bench (see register response)");
        } else {
            eprintln!("airbug dash: http://127.0.0.1:8790/#/bench");
        }
    } else {
        eprintln!("airbug dash: http://127.0.0.1:8790/#/bench");
    }
    let run_result = suite.run_selected(&selection);
    if let Some(p) = output.as_ref() {
        let dir = std::path::Path::new(p);
        let final_state = if run_result.is_ok() {
            "complete"
        } else {
            "failed"
        };
        let _ = std::fs::write(
            dir.join("status-final.json"),
            serde_json::to_string_pretty(&serde_json::json!({
                "state": final_state,
                "error": run_result.as_ref().err().map(|e| e.to_string()),
            }))?,
        );
        let progress = serde_json::json!({
            "state": final_state,
            "completed": total,
            "total": total,
            "variant": "point_values"
        });
        let _ = std::fs::write(
            dir.join("progress.json"),
            serde_json::to_string_pretty(&progress)?,
        );
    }
    let run = run_result?;
    if let Some(p) = output {
        let dir = std::path::Path::new(&p);
        // Hub may have already created the directory; write artifacts in place.
        let run_path = dir.join("run.json");
        if run_path.exists() {
            return Err(airbug_bench::error(format!(
                "run.json already exists in {}",
                dir.display()
            )));
        }
        std::fs::write(&run_path, serde_json::to_string_pretty(&run)?)
            .map_err(|e| airbug_bench::error(e.to_string()))?;
        let html = airbug_bench::report::html_run(&run)?;
        std::fs::write(dir.join("report.html"), html)
            .map_err(|e| airbug_bench::error(e.to_string()))?;
    }
    if json {
        println!("RBENCH_RESULT={}", serde_json::to_string(&run)?);
    } else {
        println!("{}", airbug_bench::report::markdown(&run)?);
    }
    Ok(())
}
