//! Row reuse: query results are built from pooled rows. A reused row must never carry a
//! field, vector or value from an earlier result. Queries with different shapes run
//! interleaved on one database, many times over, and every answer must equal the same
//! query on a fresh database that ran nothing before it.

use lin::Db;

fn seed(db: &mut Db) {
    db.run("col cust { id: text, email: text, name: text }").unwrap();
    db.run("col ord { id: text, user_id: text, total: f64, note: text }").unwrap();
    let mut cust = String::from("insert cust [");
    for i in 0..60 {
        if i > 0 {
            cust.push(',');
        }
        cust.push_str(&format!(r#"{{ id: "u{i}", email: "u{i}@x", name: "User {i}" }}"#));
    }
    cust.push(']');
    db.run(&cust).unwrap();
    let mut ord = String::from("insert ord [");
    for i in 0..400 {
        if i > 0 {
            ord.push(',');
        }
        let note = if i % 3 == 0 { format!(r#", note: "n{i}""#) } else { String::new() };
        ord.push_str(&format!(
            r#"{{ id: "o{i}", user_id: "u{}", total: {:?}{note} }}"#,
            i % 60,
            (i % 97) as f64 + 0.25
        ));
    }
    ord.push(']');
    db.run(&ord).unwrap();
    let mut docs = String::from("insert docs [");
    for i in 0..40 {
        if i > 0 {
            docs.push(',');
        }
        let topic = ["pooled rows", "vector reuse", "stale fields", "projection"][i % 4];
        docs.push_str(&format!(
            r#"{{ uri: "raw://p/{i}", title: "Doc {i} {topic}", layer: "raw", body: "about {topic}" }}"#
        ));
    }
    docs.push(']');
    db.run(&docs).unwrap();
    db.run("fk ord.user_id -> cust.id").unwrap();
}

const QUERIES: &[&str] = &[
    "ord | { id, total } | take all",
    "ord | total > 50 | { id, note } | take all",
    "ord | take all",
    "cust | { email } | take all",
    "ord | join cust on user_id | { id, cust.email } | take all",
    "ord | join cust on user_id | take 30",
    "ord | count by user_id | take all",
    "ord | sum total by user_id | take all",
    "ord | sort total desc | { id, total } | take 25",
    r#"docs | search lex "stale" | { uri, title } | take all"#,
    r#"docs | search vec "vector reuse" | { uri } | take 5"#,
    r#"docs | search vec "vector reuse" | take 3"#,
    "docs | { title, embedding } | take 5",
    r#"docs | uri == "raw://p/3" | { id, title }"#,
    "ord | window row_number by user_id sort total asc as w | { id, w } | take all",
    r#"ord | id == "o7""#,
    "cust | take all",
];

fn answer(db: &mut Db, q: &str) -> Vec<String> {
    let rows = db.run(q).unwrap_or_else(|e| panic!("{q}: {e}")).rows.clone();
    let mut keys: Vec<String> = rows
        .iter()
        .map(|r| {
            r.iter()
                .map(|(k, c)| match c {
                    lin::Cell::Vec(v) => format!("{k}=vec{}:{:08x}", v.len(), v.iter().fold(0u32, |h, x| h.rotate_left(5) ^ x.to_bits())),
                    other => match other.text() {
                        Some(t) => format!("{k}={t}"),
                        None => format!("{k}={other:?}"),
                    },
                })
                .collect::<Vec<_>>()
                .join(";")
        })
        .collect();
    if !q.contains("search") && !q.contains("sort") {
        keys.sort();
    }
    keys
}

#[test]
fn reused_rows_never_carry_earlier_fields() {
    // Reference answers, each from a database that ran nothing else.
    let want: Vec<Vec<String>> = QUERIES
        .iter()
        .map(|q| {
            let mut fresh = Db::empty();
            seed(&mut fresh);
            answer(&mut fresh, q)
        })
        .collect();
    let mut db = Db::empty();
    seed(&mut db);
    // Interleave in several ord, including writes and rolled-back transactions
    // between queries, so pooled rows cycle through every shape.
    for round in 0..12 {
        for step in 0..QUERIES.len() {
            let i = (step * 7 + round * 3) % QUERIES.len();
            assert_eq!(answer(&mut db, QUERIES[i]), want[i], "round {round}: {}", QUERIES[i]);
            if step % 5 == 0 {
                let mut tx = db.transaction().unwrap();
                tx.run(r#"insert ord { id: "tmp", user_id: "u1", total: 1.0, note: "tmp" }"#).unwrap();
                tx.run(r#"ord | id == "tmp" | { id, note }"#).unwrap();
                tx.rollback().unwrap();
            }
        }
        // A cursor drains rows lazily, through the same pool.
        let mut cursor_rows = 0;
        let q = lin::parse("ord | total > 10 | { id, total } | take all").unwrap();
        let lin::Stmt::Query(q) = q else { unreachable!() };
        for row in db.cursor(&q).unwrap() {
            let row = row.unwrap();
            assert_eq!(row.len(), 2, "round {round}: cursor row has extra fields: {row:?}");
            cursor_rows += 1;
        }
        assert!(cursor_rows > 0);
    }
}
