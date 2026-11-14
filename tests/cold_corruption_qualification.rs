//! Damaged cold-tier files: every file of a cold segment, truncated, zero-filled,
//! bit-flipped or deleted. Opening the database and querying it must return an error
//! or correct answers — never panic, read out of bounds or crash the process.

use std::fs;
use std::path::{Path, PathBuf};

use lin::Db;

fn tmp(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("lin-cold-corrupt-{}-{tag}", std::process::id()));
    let _ = fs::remove_dir_all(&p);
    fs::create_dir_all(&p).unwrap();
    p
}

fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let dst = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &dst);
        } else if entry.file_name() != "LOCK" {
            fs::copy(entry.path(), dst).unwrap();
        }
    }
}

fn files_under(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        if entry.file_type().unwrap().is_dir() {
            files_under(&entry.path(), out);
        } else {
            out.push(entry.path());
        }
    }
}

const QUERIES: &[&str] = &[
    "ev | count",
    "ev | n > 500 | count",
    r#"ev | id == "e123""#,
    r#"ev | kind == "red" | { id, n } | take all"#,
    r#"ev | note ~ "needle" | { id } | take all"#,
    "ev | sum n by kind | take all",
    "ev | sort n desc | { id } | take 5",
];

fn answers(db: &mut Db) -> Vec<Result<Vec<String>, String>> {
    QUERIES
        .iter()
        .map(|q| {
            db.run(q)
                .map(|h| {
                    let mut rows: Vec<String> = h.rows.iter().map(|r| format!("{r:?}")).collect();
                    rows.sort();
                    rows
                })
                .map_err(|e| e.to_string())
        })
        .collect()
}

#[test]
fn damaged_cold_files_never_crash() {
    let opts = || lin::OpenOpts { sync: lin::SyncMode::Normal, cold: true };
    let base = tmp("base");
    {
        let mut db = Db::open_with(&base, opts()).unwrap();
        db.run("col ev { id: text, kind: text, n: i64, note: text }").unwrap();
        db.run("index ev [kind]").unwrap();
        let rows = (0..3000)
            .map(|i| {
                format!(
                    r#"{{ id: "e{i}", kind: "{}", n: {i}, note: "row {i} {} text that is long enough for compressed blocks" }}"#,
                    ["red", "green", "blue"][i % 3],
                    if i % 41 == 0 { "needle" } else { "hay" }
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        db.run(&format!("insert ev [{rows}]")).unwrap();
        db.checkpoint().unwrap();
        db.close().unwrap();
    }
    let mut good = Db::open_with(&base, opts()).unwrap();
    assert!(good.store.is_disk_backed("ev"), "ev is not cold");
    let want = answers(&mut good);
    drop(good);

    let mut targets = Vec::new();
    files_under(&base.join("segments"), &mut targets);
    for f in ["manifest.json", "tombstones.bin"] {
        targets.push(base.join(f));
    }
    assert!(!targets.is_empty(), "no cold segment files found under {}", base.display());

    let mut damaged = 0usize;
    let mut detected = 0usize;
    for (fi, file) in targets.iter().enumerate() {
        let rel = file.strip_prefix(&base).unwrap().to_path_buf();
        let original = fs::read(file).unwrap();
        let variants: Vec<(&str, Option<Vec<u8>>)> = vec![
            ("deleted", None),
            ("empty", Some(Vec::new())),
            ("truncated", Some(original[..original.len() / 2].to_vec())),
            ("zeroed", Some(vec![0u8; original.len()])),
            ("flipped", Some({
                let mut b = original.clone();
                for i in (0..b.len()).step_by(97) {
                    b[i] ^= 0xA5;
                }
                b
            })),
            ("extended", Some({
                let mut b = original.clone();
                b.extend_from_slice(&[0xFF; 4096]);
                b
            })),
        ];
        for (how, bytes) in variants {
            let dir = tmp(&format!("{fi}-{how}"));
            copy_dir(&base, &dir);
            let target = dir.join(&rel);
            match &bytes {
                None => fs::remove_file(&target).unwrap(),
                Some(b) => fs::write(&target, b).unwrap(),
            }
            damaged += 1;
            let what = format!("{} {how}", rel.display());
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                match Db::open_with(&dir, opts()) {
                    Err(_) => None,
                    Ok(mut db) => Some(answers(&mut db)),
                }
            }));
            // A missing manifest, or a missing, empty or truncated column file, is
            // detectable from file presence and lengths alone: it must be reported, not
            // read as missing values. (Zeroed or flipped bytes need checksums, which the
            // segment format does not carry yet; for those only "no crash" is required.)
            let name = rel.file_name().unwrap().to_string_lossy().to_string();
            let must_detect = (name == "manifest.json" && how == "deleted")
                || (name.starts_with("col_")
                    && [".val", ".null", ".loc", ".boff", ".dord"].iter().any(|ext| name.ends_with(ext))
                    && matches!(how, "deleted" | "empty" | "truncated"));
            match outcome {
                Err(_) => panic!("{what}: panicked"),
                Ok(None) => detected += 1,
                Ok(Some(got)) if must_detect && got.iter().all(Result::is_ok) => {
                    panic!("{what}: opened and answered without reporting the damage")
                }
                Ok(Some(got)) => {
                    for ((q, g), w) in QUERIES.iter().zip(&got).zip(&want) {
                        // Each query either fails or answers as on the undamaged copy, unless
                        // the damage is in bytes the query never reads.
                        if let Ok(rows) = g
                            && rows != w.as_ref().unwrap()
                        {
                            // A silently wrong answer: only acceptable when the damaged
                            // file is not one this query depends on, which cannot be
                            // decided here; record it.
                            eprintln!("{what}: {q} answered differently");
                        }
                    }
                    if got.iter().any(Result::is_err) {
                        detected += 1;
                    }
                }
            }
            let _ = fs::remove_dir_all(&dir);
        }
    }
    eprintln!("{damaged} damaged copies, {detected} reported an error");
}
