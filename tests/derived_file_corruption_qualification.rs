//! Files derived from the data (full-text postings, index files written beside the
//! snapshot) are a cache: damaging or truncating one must not change any answer. Opening
//! either rebuilds what it cannot trust or refuses to start; it never serves results from
//! a bad derived file.

use std::path::{Path, PathBuf};

use lin::Db;

fn tmp() -> PathBuf {
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let n = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let d = std::env::temp_dir().join(format!("lin-derived-{}-{n}-{seq}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d
}

const QUERIES: &[&str] = &[
    r#"docs | search "alpha" | { uri } | take 50"#,
    r#"docs | search "bravo charlie" | { uri } | take 50"#,
    "t | k == 3 | { id } | take all",
    "t | k > 6 | count",
    r#"docs | uri == "raw://d/7" | { title }"#,
];

fn answers(db: &mut Db) -> Vec<String> {
    QUERIES
        .iter()
        .map(|q| {
            let mut rows: Vec<String> = match db.run(q) {
                Ok(h) => h.rows.iter().map(|r| format!("{r:?}")).collect(),
                Err(e) => vec![format!("error: {e}")],
            };
            if !q.contains("search") {
                rows.sort();
            }
            format!("{q} => {}", rows.join(" | "))
        })
        .collect()
}

fn collect_derived_files(root: &Path, dir: &Path, out: &mut Vec<PathBuf>) {
    for e in std::fs::read_dir(dir).unwrap() {
        let e = e.unwrap();
        let path = e.path();
        let ft = e.file_type().unwrap();
        if ft.is_dir() {
            collect_derived_files(root, &path, out);
        } else if ft.is_file() {
            let rel = path.strip_prefix(root).unwrap().to_path_buf();
            let top = rel
                .components()
                .next()
                .and_then(|c| c.as_os_str().to_str())
                .unwrap_or("");
            let is_primary = rel.components().count() == 1
                && (["LOCK", "FENCE", "log", "head"].contains(&top) || top.starts_with("snapshot"));
            if !is_primary {
                out.push(rel);
            }
        }
    }
}

fn copy_data_dir(src: &Path, dst: &Path) {
    for e in std::fs::read_dir(src).unwrap() {
        let e = e.unwrap();
        let name = e.file_name();
        let name_str = name.to_string_lossy();
        if name_str == "LOCK" || name_str == "FENCE" {
            continue;
        }
        let ft = e.file_type().unwrap();
        let to = dst.join(&name);
        if ft.is_dir() {
            std::fs::create_dir_all(&to).unwrap();
            copy_data_dir(&e.path(), &to);
        } else if ft.is_file() {
            std::fs::copy(e.path(), &to).unwrap();
        }
    }
}

#[test]
fn damaged_derived_files_never_change_answers() {
    let dir = tmp();
    let want;
    {
        let mut db = Db::open(&dir).unwrap();
        db.run("col t { id: text, k: i64 }").unwrap();
        db.run("index t [k]").unwrap();
        let ins = (0..300)
            .map(|i| format!(r#"{{ id: "r{i}", k: {} }}"#, i % 10))
            .collect::<Vec<_>>()
            .join(",");
        db.run(&format!("insert t [{ins}]")).unwrap();
        let words = ["alpha", "bravo", "charlie", "delta"];
        let docs = (0..60)
            .map(|i| {
                format!(
                    r#"{{ uri: "raw://d/{i}", title: "{} {} doc {i}", layer: "raw" }}"#,
                    words[i % 4],
                    words[(i / 4) % 4]
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        db.run(&format!("insert docs [{docs}]")).unwrap();
        db.checkpoint().unwrap();
        want = answers(&mut db);
        db.close().unwrap();
    }
    assert!(want.iter().all(|a| !a.contains("error")), "{want:?}");
    assert!(want[0].contains("raw://d/") && want[1].contains("raw://d/"), "search found nothing: {want:?}");

    let mut rels = Vec::new();
    collect_derived_files(&dir, &dir, &mut rels);
    rels.sort();
    assert!(!rels.is_empty(), "no derived files written");
    assert!(
        rels.iter().any(|p| p.starts_with("fts")),
        "expected FTS derived files in {rels:?}"
    );

    let mut wrong = Vec::new();
    for rel in &rels {
        let good = std::fs::read(dir.join(rel)).unwrap();
        let mut variants: Vec<(String, Vec<u8>)> = vec![
            ("empty".into(), Vec::new()),
            ("half".into(), good[..good.len() / 2].to_vec()),
            ("tail-3".into(), good[..good.len().saturating_sub(3)].to_vec()),
        ];
        if rel.starts_with("fts") && good.len() >= 8 && &good[good.len() - 8..good.len() - 4] == b"LCRC" {
            // Earlier trailer-less `LIN\x05` FTS files must also open cleanly with identical answers.
            variants.push(("legacy-no-trailer".into(), good[..good.len() - 8].to_vec()));
        }
        let step = (good.len() / 40).max(1);
        let mut i = 0;
        while i < good.len() {
            let mut b = good.clone();
            b[i] ^= 0x5a;
            variants.push((format!("byte {i}"), b));
            i += step;
        }
        for (what, bytes) in variants {
            let image = tmp();
            copy_data_dir(&dir, &image);
            std::fs::write(image.join(rel), &bytes).unwrap();
            let got = std::panic::catch_unwind(|| Db::open(&image).map(|mut db| answers(&mut db)));
            match got {
                Err(_) => wrong.push(format!("{} {what}: panic", rel.display())),
                Ok(Ok(a)) if a != want => wrong.push(format!(
                    "{} {what}: {:?}",
                    rel.display(),
                    a.iter().zip(&want).find(|(x, y)| x != y).map(|(x, _)| x)
                )),
                _ => {}
            }
            let _ = std::fs::remove_dir_all(&image);
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        wrong.is_empty(),
        "derived files {rels:?} changed answers: {:#?}",
        &wrong[..wrong.len().min(8)]
    );
}
