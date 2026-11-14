//! Cross-version on-disk format check, driven by `scripts/format-compat.sh`.
//!
//! The script builds this file against the current tree and against a previous release,
//! so it may only use API that both have. `write <dir>` creates a cold-tier database
//! (checkpointed segments plus uncheckpointed overlay DML in the WAL); `read <dir>`
//! prints the answers to a fixed set of queries in a canonical form the script diffs.

use std::process::ExitCode;

const QUERIES: &[&str] = &[
    "items | count",
    "items | count by grp | take all",
    "items | sum v by grp | take all",
    "items | n >= 100 | n < 140 | sort id | pick { id, n, v } | take all",
    "items | grp == \"g3\" | sort id | take all",
    "items | id == \"i0007\"",
    "items | id == \"i0499\"",
    "items | id == \"new1\"",
    "items | note ~ \"needle\" | sort id | pick { id, note } | take all",
    "items | max n",
    "items | min v",
];

fn open(dir: &str) -> Result<lin::Db, lin::Error> {
    lin::Db::open_with(
        dir,
        lin::OpenOpts {
            sync: lin::SyncMode::Normal,
            cold: true,
        },
    )
}

fn write(dir: &str) -> Result<(), lin::Error> {
    let mut db = open(dir)?;
    db.run("col items { id: text, grp: text, n: i64, v: f64, note: text }")?;
    let mut src = String::from("insert items [");
    for i in 0..500 {
        if i > 0 {
            src.push(',');
        }
        let note = if i % 37 == 0 { "has a needle inside" } else { "plain" };
        src.push_str(&format!(
            r#"{{ id: "i{i:04}", grp: "g{}", n: {i}, v: {:?}, note: "{note}" }}"#,
            i % 7,
            i as f64 * 0.5
        ));
    }
    src.push(']');
    db.run(&src)?;
    db.checkpoint()?;
    // Left in the WAL on purpose: the other version has to replay it over the segments.
    db.run(r#"insert items { id: "new1", grp: "g3", n: 1000, v: 0.25, note: "needle late" }"#)?;
    db.run(r#"update items[id == "i0007"] cas each { n: 7007 }"#)?;
    db.run(r#"delete items[id == "i0499"] cas each"#)?;
    Ok(())
}

fn read(dir: &str) -> Result<(), lin::Error> {
    let mut db = open(dir)?;
    for q in QUERIES {
        println!("# {q}");
        let rows = db.run(q)?.rows.clone();
        for row in rows.iter() {
            let fields: Vec<String> = row
                .iter()
                // Short and shared text are the same value; which one a path returns is
                // not part of the format.
                .map(|(k, c)| format!("{k}={c:?}").replace("=SmallText(", "=Text("))
                .collect();
            println!("{}", fields.join(" "));
        }
    }
    Ok(())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let result = match (args.get(1).map(String::as_str), args.get(2)) {
        (Some("write"), Some(dir)) => write(dir),
        (Some("read"), Some(dir)) => read(dir),
        _ => {
            eprintln!("usage: format_compat <write|read> <dir>");
            return ExitCode::from(2);
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("format_compat: {e}");
            ExitCode::FAILURE
        }
    }
}
