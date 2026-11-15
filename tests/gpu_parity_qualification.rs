//! The GPU backend answers exactly like the CPU on text and numeric edge cases: UTF-8
//! equality and substring filters over non-ASCII text, case-insensitive and anchored
//! regexes, word matches next to punctuation and combining marks, grouped counts and
//! sums with large integers (exact, as on the CPU), compensated float sums with NaN and
//! infinities, bit for bit. Needs a hardware GPU, like the other GPU tests.

#![cfg(feature = "gpu")]

use std::sync::Arc;

use lin::{Cell, Db};

const TITLES: &[&str] = &[
    "ΟΔΟΣ καλη", "μια οδος", "Ёжик в тумане", "ёжик", "漢字 テキスト", "emoji 🦀 crab",
    "café latte", "cafe\u{301} latte", "word, word.", "wording", "a-word b_word", "", "WAL log", "wal",
    "İstanbul", "straße", "tab\tsep", "end word",
];

fn load(db: &mut Db) {
    db.run("col n { id: text, g: i64, v: i64, f: f64 }").unwrap();
    for (i, t) in TITLES.iter().enumerate() {
        db.run(&format!("insert docs {{ uri: \"raw://p/{i}\", title: {t:?}, layer: \"raw\" }}")).unwrap();
    }
    let p = db.prepare("insert n { id: $id, g: $g, v: $v, f: $f }").unwrap();
    for i in 0..300i64 {
        let v = match i % 7 { 0 => i64::MAX / 3, 1 => i64::MIN / 3, _ => i * 1_000_003 };
        let f = match i % 50 { 0 => f64::NAN, 1 => f64::INFINITY, 2 => f64::NEG_INFINITY, _ => i as f64 * 0.37 };
        p.execute_with(db, &[("id", Cell::text_arc(format!("n{i}"))), ("g", Cell::Int(i % 5)), ("v", Cell::Int(v)), ("f", Cell::Float(f))]).unwrap();
    }
}

fn answer(db: &mut Db, q: &str) -> String {
    match db.run(q) {
        Ok(h) => {
            let mut rows: Vec<String> = h.rows.iter().map(|r| {
                let mut f: Vec<String> = r.iter().map(|(k, v)| match v {
                    Cell::Float(x) if x.is_nan() => format!("{k}=NaN"),
                    other => format!("{k}={other:?}"),
                }).collect();
                f.sort();
                f.join(",")
            }).collect();
            if !q.contains("search") && !q.contains("sort") {
                rows.sort();
            }
            rows.join(" | ")
        }
        Err(e) => format!("error: {e}"),
    }
}

#[test]
#[ignore = "requires a hardware GPU"]
fn gpu_matches_cpu_on_edge_cases() {
    let gpu = Arc::new(lin::gpu::GpuCompute::new().unwrap());
    let mut cpu = Db::empty();
    load(&mut cpu);
    let mut dev = Db::empty().with_gpu(gpu.clone());
    load(&mut dev);
    let queries = [
        r#"docs | title == "ёжик" | { uri } | take all"#,
        r#"docs | title == "ΟΔΟΣ καλη" | { uri } | take all"#,
        r#"docs | title ~ /οδος/i | { uri } | take all"#,
        r#"docs | title ~ /ΟΔΟΣ/i | { uri } | take all"#,
        r#"docs | title ~ /ёжик/i | { uri } | take all"#,
        r#"docs | title ~ /^wal$/i | { uri } | take all"#,
        r#"docs | title ~ /word\b/ | { uri } | take all"#,
        r#"docs | title ~ /🦀/ | { uri } | take all"#,
        r#"docs | title ~ /café|cafe/ | { uri } | take all"#,
        r#"docs | title ~ /^$/ | { uri } | take all"#,
        r#"docs | title has "word" | { uri } | take all"#,
        r#"docs | title has "latte" | { uri } | take all"#,
        r#"docs | search lex "ΟΔΟΣ" | { uri } | take all"#,
        r#"docs | search lex "word" | { uri } | take all"#,
        "n | count by g",
        "n | sum v by g",
        "n | sum v",
        "n | sum f by g",
        "n | sum f",
        "n | max f",
        "n | min f",
        "n | f > 10 | count",
        "n | sort f | { id } | take all",
        "n | sort v desc | { id } | take 20",
    ];
    for q in queries {
        assert_eq!(answer(&mut dev, q), answer(&mut cpu, q), "{q}");
    }
}
