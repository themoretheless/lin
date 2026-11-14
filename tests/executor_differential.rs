//! A mutated database answers like a fresh one: after rounds of random inserts, updates
//! and deletes, every query must return what the same query returns on a new database
//! loaded with the same final rows — full-text ranking, sorting with limits, joins,
//! grouping, windows — and cursors and batch execution must agree with `run`.

use lin::Db;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

const WORDS: &[&str] = &[
    "alpha", "bravo", "cursor", "delta", "echo", "foxtrot", "golf", "hotel", "index", "join",
    "kilo", "lima", "merge", "nova", "oscar", "papa", "query", "romeo", "sierra", "tango",
];

fn text(rng: &mut Rng, words: usize) -> String {
    (0..words).map(|_| WORDS[rng.below(WORDS.len() as u64) as usize]).collect::<Vec<_>>().join(" ")
}

fn schema(db: &mut Db) {
    db.run("col cust { id: text, name: text, tier: i64 }").unwrap();
    db.run("col ord { id: text, cust_id: text, total: f64, qty: i64 }").unwrap();
    db.run("index ord [cust_id]").unwrap();
    db.run("fk ord.cust_id -> cust.id").unwrap();
}

fn mutate(db: &mut Db, rng: &mut Rng, round: usize) {
    for k in 0..40 {
        let tag = format!("{round}-{k}");
        match rng.below(10) {
            0..=3 => {
                let title = text(rng, 3);
                let body = text(rng, 12);
                let layer = ["raw", "wiki"][rng.below(2) as usize];
                db.run(&format!(r#"insert docs {{ uri: "raw://x/{tag}", title: "{title}", layer: "{layer}", body: "{body}" }}"#)).unwrap();
            }
            4 => {
                let rows: Vec<String> = (0..5)
                    .map(|j| {
                        format!(
                            r#"{{ id: "o{tag}-{j}", cust_id: "c{}", total: {}.5, qty: {} }}"#,
                            rng.below(25),
                            rng.below(200),
                            rng.below(9)
                        )
                    })
                    .collect();
                db.run(&format!("insert ord [{}]", rows.join(","))).unwrap();
            }
            5 => {
                let w = WORDS[rng.below(WORDS.len() as u64) as usize];
                let title = text(rng, 2);
                let _ = db.run(&format!(r#"update docs[title ~ "{w}" and layer == "raw"] cas each {{ title: "{title} edited" }}"#));
            }
            6 => {
                let w = WORDS[rng.below(WORDS.len() as u64) as usize];
                let _ = db.run(&format!(r#"delete docs[title ~ "{w} {}"] cas each"#, WORDS[rng.below(WORDS.len() as u64) as usize]));
            }
            7 => {
                let c = rng.below(25);
                let _ = db.run(&format!(r#"update ord[cust_id == "c{c}"] cas each {{ total: {}.25 }}"#, rng.below(300)));
            }
            8 => {
                let _ = db.run(&format!(r#"delete ord[qty == {}] cas each"#, rng.below(9)));
            }
            _ => {
                let mut tx = db.transaction().unwrap();
                tx.run(&format!(r#"insert docs {{ uri: "raw://tx/{tag}", title: "rolled back", layer: "raw", body: "never" }}"#)).unwrap();
                let _ = tx.run(r#"delete ord[qty == 1] cas each"#);
                tx.rollback().unwrap();
            }
        }
    }
}

/// A new database holding exactly the final rows of `db`.
fn rebuild(db: &mut Db) -> Db {
    let mut fresh = Db::empty();
    schema(&mut fresh);
    let dump = |db: &mut Db, q: &str| db.run(q).unwrap().rows.clone();
    let quote = |c: Option<&lin::Cell>| match c {
        Some(lin::Cell::Int(n)) => n.to_string(),
        Some(lin::Cell::Float(f)) => format!("{f:?}"),
        Some(other) => format!("{:?}", other.text().unwrap_or("")),
        None => "null".into(),
    };
    for r in dump(db, "cust | take all") {
        fresh.run(&format!("insert cust {{ id: {}, name: {}, tier: {} }}", quote(r.get("id")), quote(r.get("name")), quote(r.get("tier")))).unwrap();
    }
    for r in dump(db, "ord | take all") {
        fresh.run(&format!("insert ord {{ id: {}, cust_id: {}, total: {}, qty: {} }}", quote(r.get("id")), quote(r.get("cust_id")), quote(r.get("total")), quote(r.get("qty")))).unwrap();
    }
    for r in dump(db, "docs | take all") {
        fresh.run(&format!("insert docs {{ uri: {}, title: {}, layer: {}, body: {} }}", quote(r.get("uri")), quote(r.get("title")), quote(r.get("layer")), quote(r.get("body")))).unwrap();
    }
    fresh
}

/// (query, compare rows in order?)
fn queries() -> Vec<(String, bool)> {
    let mut q = Vec::new();
    for w in ["alpha", "join query", "merge", "sierra tango", "nova"] {
        q.push((format!(r#"docs | search lex "{w}" | {{ uri }} | take 8"#), true));
        q.push((format!(r#"docs | search lex "{w}" | {{ uri }} | take all"#), true));
    }
    q.extend([
        (r#"docs | layer == "wiki" | sort uri | { uri, title } | take 9"#.to_string(), true),
        (r#"docs | title ~ "edited" | { uri, title } | take all"#.into(), false),
        ("docs | count by layer | take all".into(), false),
        ("ord | sort total desc | { total } | take 11".into(), true),
        ("ord | sort qty | { qty } | take all".into(), true),
        ("ord | join cust on cust_id | { id, cust.name } | take all".into(), false),
        ("ord | total > 100 | join cust on cust_id | { id, total, cust.tier } | take all".into(), false),
        ("ord | count by cust_id | take all".into(), false),
        ("ord | sum total by cust_id | take all".into(), false),
        ("ord | max total by qty | take all".into(), false),
        ("ord | cust_id == \"c3\" | { id, total } | take all".into(), false),
        ("ord | window rank by cust_id sort total desc as r | { id, r } | take all".into(), false),
    ]);
    q
}

fn answer(db: &mut Db, q: &str, ordered: bool) -> Vec<String> {
    let rows = db.run(q).unwrap_or_else(|e| panic!("{q}: {e}")).rows.clone();
    let mut keys: Vec<String> = rows
        .iter()
        .map(|r| {
            r.iter()
                .filter(|(k, _)| k.as_str() != "id" || !q.starts_with("docs"))
                .map(|(k, c)| format!("{k}={}", c.text().map(str::to_string).unwrap_or_else(|| format!("{c:?}"))))
                .collect::<Vec<_>>()
                .join(";")
        })
        .collect();
    if !ordered {
        keys.sort();
    }
    keys
}

#[test]
fn mutated_database_answers_like_a_fresh_one() {
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let mut db = Db::empty();
    schema(&mut db);
    let custs = (0..25).map(|c| format!(r#"{{ id: "c{c}", name: "Cust {c}", tier: {} }}"#, c % 3)).collect::<Vec<_>>();
    db.run(&format!("insert cust [{}]", custs.join(","))).unwrap();
    let mut non_empty = 0usize;
    let mut compared = 0usize;
    for round in 0..8 {
        mutate(&mut db, &mut rng, round);
        let mut fresh = rebuild(&mut db);
        for (q, ordered) in queries() {
            let got = answer(&mut db, &q, ordered);
            compared += 1;
            non_empty += usize::from(!got.is_empty());
            let want = answer(&mut fresh, &q, ordered);
            assert_eq!(got, want, "round {round}: {q}");
            // A cursor over the same query yields the same rows.
            let lin::Stmt::Query(parsed) = lin::parse(&q).unwrap() else { unreachable!() };
            let mut via_cursor: Vec<String> = db
                .cursor(&parsed)
                .unwrap()
                .map(|r| {
                    let r = r.unwrap();
                    r.iter()
                        .filter(|(k, _)| k.as_str() != "id" || !q.starts_with("docs"))
                        .map(|(k, c)| format!("{k}={}", c.text().map(str::to_string).unwrap_or_else(|| format!("{c:?}"))))
                        .collect::<Vec<_>>()
                        .join(";")
                })
                .collect();
            if !ordered {
                via_cursor.sort();
            }
            assert_eq!(via_cursor, got, "round {round}: cursor for {q}");
        }
    }
    // The comparison is not vacuous: data accumulated and most answers had rows.
    let docs = db.run("docs | count").unwrap().rows[0].get("hits").and_then(|c| c.as_f64()).unwrap();
    let ords = db.run("ord | count").unwrap().rows[0].get("hits").and_then(|c| c.as_f64()).unwrap();
    assert!(docs > 60.0 && ords > 10.0, "docs {docs}, ord {ords}");
    assert!(non_empty * 4 > compared * 3, "{non_empty} of {compared} answers had rows");
}
