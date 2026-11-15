//! Pull cursors: every query shape yields the same rows as `run`, through `next` and
//! through `next_projected`; an exhausted cursor stays exhausted; a cursor dropped half
//! way leaves the database usable for writes; and a cursor opened after a catalog change
//! sees the new catalog.

use lin::Db;

fn parse(q: &str) -> lin::Query {
    match lin::parse(q).unwrap() {
        lin::Stmt::Query(q) => q,
        _ => panic!("not a query: {q}"),
    }
}

fn key(r: &lin::Row) -> String {
    let mut f: Vec<String> = r.iter().map(|(k, v)| format!("{k}={v:?}")).collect();
    f.sort();
    f.join(",")
}

fn seed() -> Db {
    let mut db = Db::empty();
    db.run("col p { id: text, label: text }").unwrap();
    db.run("col c { id: text, pid: text, n: i64 }").unwrap();
    db.run("fk c.pid -> p.id").unwrap();
    db.run("index c [n]").unwrap();
    db.run(r#"insert p [{ id: "p1", label: "one" }, { id: "p2", label: "two" }]"#).unwrap();
    let rows = (0..1500).map(|i| format!(r#"{{ id: "c{i:04}", pid: "p{}", n: {} }}"#, 1 + i % 2, i % 50)).collect::<Vec<_>>().join(",");
    db.run(&format!("insert c [{rows}]")).unwrap();
    db
}

const QUERIES: &[&str] = &[
    "c | take all",
    "c | n == 7 | take all",
    "c | n > 40 | { id, n } | take all",
    "c | sort id | skip 100 | take 25",
    "c | take 3",
    "c | join p on pid | n < 2 | take all",
    "c | join p on pid | { id, p.label } | take 10",
    "c | n == 999 | take all",
];

#[test]
fn cursors_match_run_and_stay_exhausted() {
    let mut db = seed();
    for q in QUERIES {
        let mut want: Vec<String> = db.run(q).unwrap().rows.iter().map(key).collect();
        let parsed = parse(q);
        let mut cur = db.cursor(&parsed).unwrap();
        let mut got: Vec<String> = cur.by_ref().map(|r| key(&r.unwrap())).collect();
        assert!(cur.next().is_none() && cur.next().is_none(), "{q}: exhausted cursor yielded again");
        drop(cur);
        let mut proj: Vec<String> = Vec::new();
        let mut pc = db.cursor(&parsed).unwrap();
        while let Some(r) = pc.next_projected() {
            let r = r.unwrap();
            let mut f: Vec<String> = r.fields().iter().zip(r.cells().iter()).filter(|(_, v)| !matches!(v, lin::Cell::Null)).map(|(k, v)| format!("{k}={v:?}")).collect();
            f.sort();
            proj.push(f.join(","));
        }
        if !q.contains("sort") {
            want.sort();
            got.sort();
            proj.sort();
        }
        assert_eq!(got, want, "{q}: cursor rows");
        assert_eq!(proj, want, "{q}: projected cursor rows");
    }
}

#[test]
fn dropped_cursor_and_catalog_change() {
    let mut db = seed();
    {
        let parsed = parse("c | take all");
        let mut cur = db.cursor(&parsed).unwrap();
        for _ in 0..10 {
            cur.next().unwrap().unwrap();
        }
    }
    db.run(r#"insert c { id: "after", pid: "p1", n: 1 }"#).unwrap();
    db.run("col late { id: text }").unwrap();
    db.run(r#"insert late { id: "l" }"#).unwrap();
    let parsed = parse("late | take all");
    let rows: Vec<_> = db.cursor(&parsed).unwrap().map(|r| r.unwrap()).collect();
    assert_eq!(rows.len(), 1);
    let parsed = parse(r#"c | id == "after" | take all"#);
    assert_eq!(db.cursor(&parsed).unwrap().count(), 1);
}

/// Cursors over a union and over graph traversal give the same rows as `run`, and several
/// cursors open at once over the same database, read in interleaved order, each give
/// their own query's full answer.
#[test]
fn union_graph_and_interleaved_cursors() {
    let mut db = seed();
    db.run("col u { id: text }").unwrap();
    db.run(r#"insert u [{ id: "c0001" }, { id: "u-only" }]"#).unwrap();
    db.run(r#"insert docs [{ uri: "raw://cg/0", title: "G0", layer: "wiki" }, { uri: "raw://cg/1", title: "G1", layer: "wiki" }, { uri: "raw://cg/2", title: "G2", layer: "wiki" }]"#).unwrap();
    let id = |db: &mut Db, u: &str| db.run(&format!(r#"docs | uri == "{u}" | {{ id }}"#)).unwrap().rows[0].get("id").and_then(|c| c.text()).unwrap().to_string();
    let (g0, g1, g2) = (id(&mut db, "raw://cg/0"), id(&mut db, "raw://cg/1"), id(&mut db, "raw://cg/2"));
    db.run(&format!(r#"append edge wikilink "{g0}" -> "{g1}""#)).unwrap();
    db.run(&format!(r#"append edge wikilink "{g1}" -> "{g2}""#)).unwrap();
    db.run(&format!(r#"append edge wikilink "{g2}" -> "{g0}""#)).unwrap();
    let queries = [
        "c | n == 3 | { id } | union u | { id }".to_string(),
        format!(r#"docs | id == "{g0}" | hop wikilink depth=3 | {{ id }} | take all"#),
        format!(r#"docs | id == "{g0}" | match -wikilink*1..3-> b | {{ b.id }} | take all"#),
    ];
    for q in &queries {
        let mut want: Vec<String> = db.run(q).unwrap().rows.iter().map(key).collect();
        let parsed = parse(q);
        let mut got: Vec<String> = db.cursor(&parsed).unwrap().map(|r| key(&r.unwrap())).collect();
        want.sort();
        got.sort();
        assert_eq!(got, want, "{q}");
    }
    // Three cursors at once, read round-robin.
    let qs = ["c | n == 1 | take all", "c | n == 2 | take all", "p | take all"];
    let parsed: Vec<lin::Query> = qs.iter().map(|q| parse(q)).collect();
    let wants: Vec<usize> = qs.iter().map(|q| db.run(q).unwrap().rows.len()).collect();
    let mut cursors: Vec<_> = parsed.iter().map(|q| db.cursor(q).unwrap()).collect();
    let mut counts = vec![0usize; cursors.len()];
    let mut live = cursors.len();
    while live > 0 {
        live = 0;
        for (i, c) in cursors.iter_mut().enumerate() {
            if let Some(r) = c.next() {
                r.unwrap();
                counts[i] += 1;
                live += 1;
            }
        }
    }
    assert_eq!(counts, wants);
}
