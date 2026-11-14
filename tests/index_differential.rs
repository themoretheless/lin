//! Index answers against scans: the same queries on a database with secondary indexes
//! and on one without must return the same rows, for edge-case keys (i64 extremes,
//! integers that equal floats, missing values, short and long text sharing a prefix,
//! empty and non-ASCII text, duplicates) and every kind of range bound, before and after
//! updates that change or keep indexed keys, deletes, and a rolled-back transaction.

use lin::Db;

fn seed(db: &mut Db, indexed: bool) {
    db.run("col t { id: text, a: i64, b: text, f: f64, g: text }").unwrap();
    if indexed {
        db.run("index t [a]").unwrap();
        db.run("index t [b]").unwrap();
        db.run("index t [a, b]").unwrap();
        db.run("index t [f]").unwrap();
        db.run("index t [g, a]").unwrap();
    }
    let long = "a-long-text-value-that-does-not-fit-inline-anymore";
    let texts = ["", "a", "ab", "abc", "Ж", "жук", "日本", long, &format!("{long}-2"), "a b", "z"];
    let ints: [i64; 9] = [i64::MIN, i64::MIN + 1, -2, -1, 0, 1, 2, i64::MAX - 1, i64::MAX];
    let mut src = String::from("insert t [");
    for i in 0..400usize {
        if i > 0 {
            src.push(',');
        }
        src.push_str(&format!(r#"{{ id: "r{i}""#));
        if i % 9 != 4 {
            src.push_str(&format!(", a: {}", ints[i % ints.len()]));
        }
        if i % 7 != 3 {
            src.push_str(&format!(", b: {:?}", texts[i % texts.len()]));
        }
        if i % 5 != 2 {
            // Halves and whole numbers, so some floats equal integers in `a`.
            src.push_str(&format!(", f: {:?}", ((i % 11) as f64 - 5.0) / 2.0));
        }
        if i % 4 != 1 {
            src.push_str(&format!(", g: \"g{}\"", i % 3));
        }
        src.push_str(" }");
    }
    src.push(']');
    db.run(&src).unwrap();
}

fn queries() -> Vec<String> {
    let mut q: Vec<String> = Vec::new();
    for v in ["-9223372036854775808", "-1", "0", "1", "2", "9223372036854775807", "1.0", "1.5", "-0.5"] {
        for op in ["==", "!=", "<", "<=", ">", ">="] {
            q.push(format!("t | a {op} {v}"));
            q.push(format!("t | f {op} {v}"));
        }
    }
    // Text supports equality only (`<` on text is a type error).
    for v in [r#""""#, r#""a""#, r#""ab""#, r#""Ж""#, r#""a-long-text-value-that-does-not-fit-inline-anymore""#, r#""zz""#] {
        for op in ["==", "!="] {
            q.push(format!("t | b {op} {v}"));
        }
    }
    q.extend(
        [
            r#"t | a == 1 and b == "a""#,
            r#"t | a == 0 and b != "a""#,
            r#"t | a >= -1 and a <= 1 and b == "abc""#,
            r#"t | g == "g1" and a > 0"#,
            r#"t | g == "g2" and a <= -1"#,
            r#"t | g == "g0" and a == 9223372036854775807"#,
            r#"t | a in [0, 1, 9223372036854775807]"#,
            r#"t | b in ["", "a", "日本"]"#,
            r#"t | a is null"#,
            r#"t | b is null and a == 2"#,
            r#"t | g is null or a < -1"#,
            r#"t | not (a == 0)"#,
            r#"t | a > 0 or b == "z""#,
        ]
        .map(String::from),
    );
    let mut out = Vec::new();
    for base in q {
        out.push(format!("{base} | {{ id }} | take all"));
        out.push(format!("{base} | count"));
    }
    // Ordering over mixed and missing values.
    out.push("t | sort a | { id, a } | take all".to_string());
    out.push("t | sort f desc | { id, f } | take all".to_string());
    out.push("t | sort b | { id, b } | take all".to_string());
    out
}

fn answer(db: &mut Db, q: &str) -> Vec<String> {
    let rows = db.run(q).unwrap_or_else(|e| panic!("{q}: {e}")).rows.clone();
    let mut keys: Vec<String> = rows
        .iter()
        .map(|r| {
            r.iter()
                .map(|(k, c)| match c.text() {
                    Some(t) => format!("{k}={t:?}"),
                    None => format!("{k}={c:?}"),
                })
                .collect::<Vec<_>>()
                .join(";")
        })
        .collect();
    if !q.contains("| sort ") {
        keys.sort();
    }
    keys
}

fn compare(indexed: &mut Db, plain: &mut Db, phase: &str) {
    for q in queries() {
        let (i, p) = (answer(indexed, &q), answer(plain, &q));
        // Sorted outputs may order equal keys differently; compare the key sequence
        // exactly and the rows as a set.
        if q.contains("| sort ") {
            let key = |rows: &[String]| -> Vec<String> {
                rows.iter().map(|r| r.split(';').nth(1).unwrap_or("").to_string()).collect()
            };
            assert_eq!(key(&i), key(&p), "{phase}: order of {q}");
            let (mut si, mut sp) = (i.clone(), p.clone());
            si.sort();
            sp.sort();
            assert_eq!(si, sp, "{phase}: rows of {q}");
        } else {
            assert_eq!(i, p, "{phase}: {q}");
        }
    }
}

#[test]
fn index_answers_match_scans_on_edge_case_keys() {
    let mut indexed = Db::empty();
    let mut plain = Db::empty();
    seed(&mut indexed, true);
    seed(&mut plain, false);
    compare(&mut indexed, &mut plain, "after load");

    let writes = [
        // Changes an indexed key, keeps another, moves rows between null and non-null.
        r#"update t[a == 1] cas each { a: 42 }"#,
        r#"update t[b == "ab"] cas each { b: "ab" }"#,
        r#"update t[a == 42 and b == "ab"] cas each { b: "moved" }"#,
        r#"update t[g == "g2" and a == -2] cas each { g: "g0", a: 9223372036854775807 }"#,
        r#"update t[a is null] cas each { a: -1 }"#,
        r#"delete t[b == "Ж"] cas each"#,
        r#"delete t[a == 9223372036854775807 and g == "g2"] cas each"#,
        r#"insert t [{ id: "n1", a: 1, b: "a", f: 1.0, g: "g1" }, { id: "n2", b: "" }, { id: "n3", a: -9223372036854775808, f: -0.5 }]"#,
    ];
    for w in writes {
        indexed.run(w).unwrap_or_else(|e| panic!("{w}: {e}"));
        plain.run(w).unwrap_or_else(|e| panic!("{w}: {e}"));
    }
    compare(&mut indexed, &mut plain, "after writes");

    // A rolled-back transaction leaves the indexes exactly as the data.
    for db in [&mut indexed, &mut plain] {
        let mut tx = db.transaction().unwrap();
        tx.run(r#"update t[a == 42] cas each { a: 7, b: "tx" }"#).unwrap();
        tx.run(r#"delete t[g == "g0"] cas each"#).unwrap();
        tx.run(r#"insert t { id: "tx1", a: 7, b: "tx", g: "g0" }"#).unwrap();
        tx.rollback().unwrap();
    }
    compare(&mut indexed, &mut plain, "after rollback");
}
