//! Parser robustness and grammar: hostile input (deep nesting, huge term counts, huge
//! literals, NUL bytes, random bytes and random token soup) is rejected with an error in
//! bounded time and never panics or overflows the stack; CRLF source behaves like LF;
//! operator precedence and associativity match explicit parentheses on every truth
//! assignment.

use std::time::{Duration, Instant};

use lin::Db;


fn no_panic<T>(what: &str, f: impl FnOnce() -> T) -> T {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)) {
        Ok(v) => v,
        Err(_) => panic!("panicked on {what}"),
    }
}

#[test]
fn hostile_nesting_and_size_fail_cleanly() {
    let started = Instant::now();
    // Run on a thread with a modest stack: deep recursion must be refused, not crash.
    let handle = std::thread::Builder::new()
        .stack_size(2 * 1024 * 1024)
        .spawn(|| {
            for depth in [10usize, 100, 10_000, 200_000] {
                let src = format!("docs | {}id == \"x\"{}", "(".repeat(depth), ")".repeat(depth));
                let r = no_panic("deep parens", || lin::parse(&src));
                if depth <= 10 {
                    assert!(r.is_ok(), "depth {depth}: {r:?}");
                } else if depth >= 10_000 {
                    assert!(r.is_err(), "depth {depth} accepted");
                }
                let src = format!("docs | {}id == \"x\"", "not ".repeat(depth));
                let r = no_panic("deep not", || lin::parse(&src));
                if depth >= 10_000 {
                    assert!(r.is_err(), "not-depth {depth} accepted");
                }
            }
            // Long flat chains used to parse and then overflow the stack while being
            // checked, evaluated and dropped. They must answer or fail, never crash.
            let mut db = Db::empty();
            db.run("col c { id: text, n: i64 }").unwrap();
            let rows = (0..2000).map(|i| format!(r#"{{ id: "c{i}", n: {i} }}"#)).collect::<Vec<_>>();
            db.run(&format!("insert c [{}]", rows.join(","))).unwrap();
            for n in [1_000usize, 30_000, 100_000, 1_000_000] {
                for op in ["or", "and"] {
                    let terms = (0..n).map(|i| format!("id == \"c{}\"", i % 3000)).collect::<Vec<_>>().join(&format!(" {op} "));
                    let src = format!("c | {terms} | count");
                    let _ = no_panic("long chain", || lin::parse(&src));
                    let got = no_panic("long chain run", || db.run(&src).map(|h| h.rows.clone()));
                    if op == "or" && n <= 30_000 {
                        // Same answer as the equivalent `in` list.
                        let list = (0..n.min(3000)).map(|i| format!("\"c{i}\"")).collect::<Vec<_>>().join(",");
                        let want = db.run(&format!("c | id in [{list}] | count")).unwrap().rows.clone();
                        assert_eq!(got.expect("chain within the size quota"), want, "or-chain of {n}");
                    }
                }
            }
            let list = (0..1_000_000).map(|i| format!("\"v{i}\"")).collect::<Vec<_>>().join(",");
            let _ = no_panic("huge list", || lin::parse(&format!("docs | id in [{list}]")));
        })
        .unwrap();
    handle.join().expect("parser thread crashed");
    assert!(started.elapsed() < Duration::from_secs(60), "took {:?}", started.elapsed());
}

#[test]
fn literals_overflow_and_odd_bytes() {
    let mut db = Db::empty();
    db.run("col n { id: text, a: i64, f: f64 }").unwrap();
    db.run(r#"insert n [{ id: "lo", a: -9223372036854775808 }, { id: "hi", a: 9223372036854775807 }]"#)
        .unwrap();
    // The extremes parse exactly.
    let lo = db.run("n | a == -9223372036854775808 | { id }").unwrap().rows.clone();
    assert_eq!(lo.len(), 1);
    assert_eq!(lo[0].get("id").and_then(|c| c.text()), Some("lo"));
    // One past them is an error, not a wrapped value.
    for src in [
        "n | a == 9223372036854775808",
        "n | a == -9223372036854775809",
        "n | a == 99999999999999999999999999999999",
        r#"insert n { id: "x", a: 9223372036854775808 }"#,
    ] {
        let r = no_panic(src, || db.run(src));
        if let Ok(h) = &r {
            assert!(h.rows.is_empty(), "{src}: matched {:?}", h.rows);
        }
    }
    assert_eq!(db.run("n | count").unwrap().rows[0].get("hits").and_then(|c| c.as_f64()), Some(2.0));
    // NUL and other control bytes in the source: an error or a literal, never a panic.
    for src in ["n | id == \"a\0b\"", "n\0 | count", "n | id == \"\u{1}\"", "\0", "n | id == \"\\0\""] {
        let _ = no_panic(src, || db.run(src));
    }
    // CRLF line breaks behave like LF.
    let lf = db.run("n\n| a > 0\n| { id }").unwrap().rows.clone();
    let crlf = db.run("n\r\n| a > 0\r\n| { id }").unwrap().rows.clone();
    assert_eq!(lf, crlf);
    let program_crlf = "insert n { id: \"c1\", a: 1 }\r\ninsert n { id: \"c2\", a: 2 }\r\n";
    db.run(program_crlf).unwrap();
    assert_eq!(db.run("n | count").unwrap().rows[0].get("hits").and_then(|c| c.as_f64()), Some(4.0));
}

#[test]
fn precedence_and_associativity_match_explicit_parentheses() {
    let mut db = Db::empty();
    db.run("col b { id: text, a: i64, b: i64, c: i64, d: i64 }").unwrap();
    let mut src = String::from("insert b [");
    for m in 0..16 {
        if m > 0 {
            src.push(',');
        }
        src.push_str(&format!(
            r#"{{ id: "m{m}", a: {}, b: {}, c: {}, d: {} }}"#,
            m & 1,
            (m >> 1) & 1,
            (m >> 2) & 1,
            (m >> 3) & 1
        ));
    }
    src.push(']');
    db.run(&src).unwrap();
    let (a, b, c, d) = ("a == 1", "b == 1", "c == 1", "d == 1");
    let pairs = [
        (format!("{a} or {b} and {c}"), format!("{a} or ({b} and {c})")),
        (format!("{a} and {b} or {c}"), format!("({a} and {b}) or {c}")),
        (format!("not {a} and {b}"), format!("(not {a}) and {b}")),
        (format!("not {a} or {b}"), format!("(not {a}) or {b}")),
        (format!("{a} or {b} or {c} or {d}"), format!("(({a} or {b}) or {c}) or {d}")),
        (format!("{a} and {b} and {c} or {d}"), format!("(({a} and {b}) and {c}) or {d}")),
        (format!("{a} or {b} and {c} or {d}"), format!("{a} or ({b} and {c}) or {d}")),
        (format!("not not {a}"), a.to_string()),
        (format!("not ({a} or {b}) and {c}"), format!("(not {a}) and (not {b}) and {c}")),
    ];
    let ids = |db: &mut Db, p: &str| -> Vec<String> {
        let mut v: Vec<String> = db
            .run(&format!("b | {p} | {{ id }} | take all"))
            .unwrap_or_else(|e| panic!("{p}: {e}"))
            .rows
            .iter()
            .map(|r| r.get("id").and_then(|c| c.text()).unwrap().to_string())
            .collect();
        v.sort();
        v
    };
    for (implicit, explicit) in pairs {
        assert_eq!(ids(&mut db, &implicit), ids(&mut db, &explicit), "{implicit}  vs  {explicit}");
    }
}

#[test]
fn random_input_never_panics() {
    let mut db = Db::empty();
    db.run("col t { id: text, a: i64, s: text }").unwrap();
    db.run(r#"insert t [{ id: "x", a: 1, s: "s" }]"#).unwrap();
    let tokens = [
        "t", "docs", "|", "{", "}", "[", "]", "(", ")", ",", ":", "==", "!=", "<", ">=", "and", "or",
        "not", "in", "is", "null", "~", "has", "regex", "\"s\"", "\"\"", "1", "-1", "1.5", "a", "id",
        "s", "count", "by", "sum", "sort", "desc", "take", "all", "skip", "insert", "update", "delete",
        "cas", "each", "col", "index", "unique", "join", "on", "window", "as", "search", "lex", "vec",
        "hop", "pick", "let", "=", "$p", "\n", "/x/i", "9223372036854775807", "\"\\\"\"", "->",
    ];
    let mut x: u64 = 0x2545_F491_4F6C_DD1D;
    let mut next = move || {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        x
    };
    let started = Instant::now();
    for _ in 0..20_000 {
        let len = 1 + (next() % 14) as usize;
        let src: Vec<&str> = (0..len).map(|_| tokens[(next() % tokens.len() as u64) as usize]).collect();
        let src = src.join(" ");
        no_panic(&src, || {
            let _ = db.run(&src);
        });
    }
    for _ in 0..5_000 {
        let len = (next() % 40) as usize;
        let bytes: Vec<u8> = (0..len).map(|_| (next() % 256) as u8).collect();
        let src = String::from_utf8_lossy(&bytes).to_string();
        no_panic(&format!("{bytes:?}"), || {
            let _ = lin::parse(&src);
        });
    }
    // Whatever ran, the table is still answerable.
    db.run("t | count").unwrap();
    assert!(started.elapsed() < Duration::from_secs(120), "took {:?}", started.elapsed());
}
