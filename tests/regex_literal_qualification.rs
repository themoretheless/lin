//! Regex literals in filters: an invalid pattern or one past the compiled-size limit is a
//! query error, never a panic and never a silent "matches nothing"; a pattern with
//! nested quantifiers runs in linear time; and valid patterns match exactly as the regex
//! crate does on the same strings.

use lin::Db;

fn seed() -> (Db, Vec<(String, String)>) {
    let mut db = Db::empty();
    db.run("col t { id: text, s: text }").unwrap();
    let rows: Vec<(String, String)> = (0..60).map(|i| (format!("r{i}"), match i % 6 {
        0 => format!("order-{i}"),
        1 => "aaaaaaaaaaaaaaaaaaaaaaaaaaaaab".to_string(),
        2 => "Mixed Case".to_string(),
        3 => String::new(),
        4 => "line\nbreak".to_string(),
        _ => format!("{i}{i}"),
    })).collect();
    let lit = rows.iter().map(|(id, s)| format!("{{ id: \"{id}\", s: {s:?} }}")).collect::<Vec<_>>().join(",");
    db.run(&format!("insert t [{lit}]")).unwrap();
    (db, rows)
}

#[test]
fn invalid_and_oversized_patterns_are_errors() {
    let (mut db, _) = seed();
    for bad in [r#"(unclosed"#, r#"[z-a]"#, r#"*leading"#, r#"a{5,2}"#, r#"(?P<x>a)(?P<x>b)"#] {
        let q = format!("t | s regex {bad:?} | count");
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| db.run(&q)));
        assert!(r.is_ok(), "{q}: panicked");
        assert!(r.unwrap().is_err(), "{q}: accepted an invalid pattern");
    }
    let huge = format!("t | s regex \"(a{{1000}}){{1000}}\" | count");
    assert!(db.run(&huge).is_err(), "pattern past the size limit accepted");
}

#[test]
fn valid_patterns_match_like_the_regex_crate() {
    let (mut db, rows) = seed();
    for pat in [r"^order-\d+$", r"(a+)+b$", r"(?i)mixed", r"^$", r"line\nbreak", r".", "", "^", "$", r"\b", r"(?m)^break$", r"^break$", r"\bCase\b", r"x*"] {
        let re = regex::Regex::new(pat).unwrap();
        let mut want: Vec<&str> = rows.iter().filter(|(_, s)| re.is_match(s)).map(|(id, _)| id.as_str()).collect();
        want.sort();
        let q = format!("t | s regex {pat:?} | {{ id }} | take all");
        let start = std::time::Instant::now();
        let mut got: Vec<String> = db.run(&q).unwrap_or_else(|e| panic!("{q}: {e}")).rows.iter().map(|r| r.get("id").and_then(|c| c.text()).unwrap().to_string()).collect();
        assert!(start.elapsed() < std::time::Duration::from_secs(5), "{q}: too slow");
        got.sort();
        assert_eq!(got, want, "{q}");
    }
}
