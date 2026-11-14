//! Cold-tier joins on a non-`id` key against the in-memory tier on the same data, for the
//! key sets the `key in [...]` pushdown does not cover: integer keys and more distinct
//! text keys than it accepts.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use lin::Db;

static SEQ: AtomicU64 = AtomicU64::new(0);

fn tmp() -> PathBuf {
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("lin-cold-join-{}-{n}", std::process::id()));
    let _ = fs::remove_dir_all(&p);
    fs::create_dir_all(&p).unwrap();
    p
}

fn norm(db: &mut Db, q: &str) -> Vec<String> {
    let handle = db.run(q).unwrap();
    let mut out: Vec<String> = handle
        .rows
        .iter()
        .map(|r| {
            r.iter()
                .map(|(k, c)| match (c.text(), c.as_f64()) {
                    (Some(t), _) => format!("{k}={t}"),
                    (None, Some(n)) => format!("{k}={n}"),
                    _ => format!("{k}={c:?}"),
                })
                .collect::<Vec<_>>()
                .join(";")
        })
        .collect();
    out.sort();
    out
}

fn insert_all(db: &mut Db, collection: &str, n: usize, row: impl Fn(usize) -> String) {
    for chunk in 0..n.div_ceil(1000) {
        let mut src = format!("insert {collection} [");
        for i in (chunk * 1000)..((chunk + 1) * 1000).min(n) {
            if i > chunk * 1000 {
                src.push(',');
            }
            src.push_str(&row(i));
        }
        src.push(']');
        db.run(&src).unwrap();
    }
}

const PARENTS: usize = 12_000;
const CHILDREN: usize = 9_000;

/// `num` is an integer key with two parents per value; `code` is a text key that is
/// unique per parent, so it is stored as plain text rather than dictionary codes.
fn seed(db: &mut Db) {
    db.run(
        "col parents { id: text, num: i64, code: text, label: text }\n\
         col kids { id: text, pnum: i64, pcode: text, n: i64 }",
    )
    .unwrap();
    insert_all(db, "parents", PARENTS, |i| {
        let num = if i % 50 == 49 { String::new() } else { format!(", num: {}", i % 6000) };
        format!(r#"{{ id: "p{i}"{num}, code: "code-{i}-{}", label: "L{i}" }}"#, i * 7919)
    });
    insert_all(db, "kids", CHILDREN, |i| {
        // Some children point at keys no parent has, some have no key at all.
        let pnum = if i % 40 == 39 { String::new() } else { format!(", pnum: {}", (i * 3) % 6500) };
        let j = (i * 5) % 13_000;
        format!(r#"{{ id: "c{i}"{pnum}, pcode: "code-{j}-{}", n: {i} }}"#, j * 7919)
    });
    // Declared after the load: the join key comes from the declaration, and an insert
    // under it would refuse the children whose key no parent has.
    db.run("fk kids.pnum -> parents.num\nfk kids.pcode -> parents.code").unwrap();
}

#[test]
fn cold_join_on_integer_and_large_text_key_sets_matches_in_memory() {
    let cold_dir = tmp();
    let opts = || lin::OpenOpts {
        sync: lin::SyncMode::Normal,
        cold: true,
    };
    {
        let mut db = Db::open_with(&cold_dir, opts()).unwrap();
        seed(&mut db);
        db.checkpoint().unwrap();
        db.close().unwrap();
    }
    // `fk` is a session-only declaration: the session declares it again after reopen.
    let mut cold = Db::open_with(&cold_dir, opts()).unwrap();
    cold.run("fk kids.pnum -> parents.num\nfk kids.pcode -> parents.code").unwrap();
    let mut hot = Db::empty();
    seed(&mut hot);
    assert!(cold.store.is_disk_backed("parents"));

    let queries = [
        // Integer key.
        "kids | n < 300 | join parents on pnum | take all",
        "kids | n < 300 | join parents on pnum | { id, parents.label } | take all",
        "kids | n >= 8900 | join left parents on pnum | take all",
        "kids | join parents on pnum | take all",
        // Text key, more distinct values than the `in [...]` pushdown takes.
        "kids | join parents on pcode | take all",
        "kids | n < 6000 | join left parents on pcode | take all",
        // Text key, few values: the pushdown path, for comparison.
        "kids | n < 50 | join parents on pcode | take all",
    ];
    for phase in ["segments only", "with overlay"] {
        if phase == "with overlay" {
            for db in [&mut cold, &mut hot] {
                db.run(
                    r#"insert parents [
                        { id: "px1", num: 6400, code: "code-12500-98987500", label: "late one" },
                        { id: "px2", num: 12, code: "late-code", label: "late two" }
                    ]"#,
                )
                .unwrap();
                db.run(r#"update parents[id == "p30"] cas each { num: 6450, label: "moved" }"#).unwrap();
                db.run(r#"delete parents[id == "p45"] cas each"#).unwrap();
            }
            assert!(cold.store.is_disk_backed("parents"), "overlay DML promoted parents");
        }
        for q in queries {
            let c = norm(&mut cold, q);
            let h = norm(&mut hot, q);
            assert!(!c.is_empty(), "{q} ({phase})");
            assert_eq!(c.len(), h.len(), "row count for {q} ({phase})");
            assert_eq!(c, h, "{q} ({phase})");
        }
    }
    assert!(cold.store.is_disk_backed("parents"), "join promoted parents");
}
