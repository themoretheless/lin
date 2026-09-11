use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use lin::{Db, Store};

static SEQ: AtomicU64 = AtomicU64::new(0);

fn tmp() -> PathBuf {
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("lin-persist-{}-{n}", std::process::id()));
    let _ = fs::remove_dir_all(&p);
    fs::create_dir_all(&p).unwrap();
    p
}

fn text<'a>(row: &'a lin::Row, k: &str) -> &'a str {
    row.get(k).and_then(|c| c.text()).unwrap_or("")
}

#[test]
fn insert_survives_reopen() {
    let dir = tmp();
    let id;
    {
        let mut db = Db::open(&dir).unwrap();
        let ins = db
            .run(r#"insert docs { uri: "raw://n/p", title: "persist", layer: "wiki", body: "hello" }"#)
            .unwrap();
        assert_eq!(ins.done.n, 1);
        assert_eq!(ins.done.r#gen, 1);
        id = text(&ins.rows[0], "id").to_string();
        drop(db);
    }
    {
        let mut db = Db::open(&dir).unwrap();
        let q = db
            .run(r#"docs | uri == "raw://n/p" | { id, title }"#)
            .unwrap();
        assert_eq!(q.done.n, 1);
        assert_eq!(text(&q.rows[0], "title"), "persist");
        assert_eq!(text(&q.rows[0], "id"), id);
        assert_eq!(db.store.r#gen, 1);
        db.close().unwrap();
    }
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn cas_after_reopen() {
    let dir = tmp();
    let (id, hash);
    {
        let mut db = Db::open(&dir).unwrap();
        let ins = db
            .run(r#"insert docs { uri: "raw://n/cas", title: "cas", layer: "wiki", body: "x" }"#)
            .unwrap();
        id = text(&ins.rows[0], "id").to_string();
        hash = text(&ins.rows[0], "hash").to_string();
        db.close().unwrap();
    }
    {
        let mut db = Db::open(&dir).unwrap();
        let err = db
            .run(&format!(
                r#"update docs[id == "{id}"] cas "nope" {{ room: "inbox" }}"#
            ))
            .unwrap_err();
        assert!(err.to_string().contains("cas mismatch"), "{err}");
        let ok = db
            .run(&format!(
                r#"update docs[id == "{id}"] cas "{hash}" {{ room: "inbox" }}"#
            ))
            .unwrap();
        assert_eq!(text(&ok.rows[0], "room"), "inbox");
        db.close().unwrap();
    }
    {
        let mut db = Db::open(&dir).unwrap();
        let q = db
            .run(&format!(r#"docs | id == "{id}" | {{ id, room }}"#))
            .unwrap();
        assert_eq!(text(&q.rows[0], "room"), "inbox");
        db.close().unwrap();
    }
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn truncated_last_record_ignored() {
    let dir = tmp();
    {
        let mut db = Db::open(&dir).unwrap();
        db.run(r#"insert docs { uri: "raw://n/keep", title: "keep", layer: "wiki" }"#)
            .unwrap();
        db.close().unwrap();
    }
    {
        let mut log = OpenOptions::new()
            .append(true)
            .open(dir.join("log"))
            .unwrap();
        log.write_all(&80u32.to_le_bytes()).unwrap();
        log.write_all(&[1, 2, 3]).unwrap();
        log.flush().unwrap();
    }
    {
        let mut db = Db::open(&dir).unwrap();
        let q = db.run(r#"docs | uri == "raw://n/keep""#).unwrap();
        assert_eq!(q.done.n, 1, "complete record must survive truncated tail");
        db.run(r#"insert docs { uri: "raw://n/after", title: "after", layer: "wiki" }"#)
            .unwrap();
        db.close().unwrap();
    }
    {
        let mut db = Db::open(&dir).unwrap();
        let q = db.run(r#"docs | take all"#).unwrap();
        assert!(q.rows.iter().any(|r| text(r, "uri") == "raw://n/keep"));
        assert!(q.rows.iter().any(|r| text(r, "uri") == "raw://n/after"));
        db.close().unwrap();
    }
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn append_idempotent_after_reopen() {
    let dir = tmp();
    let gen1;
    {
        let mut db = Db::open(&dir).unwrap();
        let a = db
            .run(r#"append facts { s: "lin", p: tagged, o: "db" }"#)
            .unwrap();
        gen1 = a.done.r#gen;
        assert_eq!(gen1, 1);
        db.close().unwrap();
    }
    {
        let mut db = Db::open(&dir).unwrap();
        let b = db
            .run(r#"append facts { s: "lin", p: tagged, o: "db" }"#)
            .unwrap();
        assert_eq!(b.done.r#gen, gen1, "idempotent append must not bump gen");
        assert_eq!(b.message.as_deref(), Some("idempotent"));
        let n = db.run(r#"facts | s == "lin""#).unwrap();
        assert_eq!(n.rows.len(), 1);
        db.close().unwrap();
    }
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn insert_with_edge_survives() {
    let dir = tmp();
    let id;
    {
        let mut db = Db::open(&dir).unwrap();
        db.run(
            r#"insert docs { uri: "wiki://rag-overview", title: "RAG overview", layer: "wiki" }"#,
        )
        .unwrap();
        let ins = db
            .run(
                r#"insert docs { uri: "raw://n/hop", title: "hopper", layer: "raw" } with edge wikilink -> page "wiki://rag-overview""#,
            )
            .unwrap();
        id = text(&ins.rows[0], "id").to_string();
        db.close().unwrap();
    }
    {
        let mut db = Db::open(&dir).unwrap();
        let hop = db
            .run(&format!(
                r#"docs | id == "{id}" | hop wikilink | {{ id, title }}"#
            ))
            .unwrap();
        assert!(
            hop.rows
                .iter()
                .any(|r| text(r, "uri") == "wiki://rag-overview"
                    || text(r, "title").contains("overview")),
            "{:?}",
            hop.rows
        );
        db.close().unwrap();
    }
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn delete_survives_reopen() {
    let dir = tmp();
    let id;
    {
        let mut db = Db::open(&dir).unwrap();
        let ins = db
            .run(r#"insert docs { uri: "raw://n/del", title: "gone", layer: "wiki", body: "x" }"#)
            .unwrap();
        id = text(&ins.rows[0], "id").to_string();
        let hash = text(&ins.rows[0], "hash").to_string();
        db.run(r#"append edge wikilink "a" -> "b""#).unwrap();
        db.run(r#"append facts { s: "lin", p: tagged, o: "zap" }"#)
            .unwrap();
        db.run(&format!(r#"delete docs[id == "{id}"] cas "{hash}""#))
            .unwrap();
        db.run(r#"delete edge wikilink "a" -> "b""#).unwrap();
        db.run(r#"delete facts[s == "lin" and p == tagged and o == "zap"]"#)
            .unwrap();
        db.close().unwrap();
    }
    {
        let mut db = Db::open(&dir).unwrap();
        let q = db.run(&format!(r#"docs | id == "{id}""#)).unwrap();
        assert_eq!(q.done.n, 0);
        let f = db.run(r#"facts | o == "zap""#).unwrap();
        assert_eq!(f.done.n, 0);
        db.close().unwrap();
    }
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn new_col_survives_reopen() {
    let dir = tmp();
    {
        let mut db = Db::open(&dir).unwrap();
        db.run(
            r#"col notes { title: text }
rel cites
insert notes { title: "kept" }"#,
        )
        .unwrap();
        db.close().unwrap();
    }
    {
        let mut db = Db::open(&dir).unwrap();
        let q = db.run(r#"notes | { title }"#).unwrap();
        assert_eq!(q.done.n, 1);
        assert_eq!(text(&q.rows[0], "title"), "kept");
        let rels = db.run(r#"catalog | name == "cites""#).unwrap();
        assert!(rels.done.n >= 1);
        db.close().unwrap();
    }
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn multi_stmt_one_gen_and_atomic_durable() {
    let dir = tmp();
    {
        let mut db = Db::open(&dir).unwrap();
        let h = db
            .run(
                r#"insert docs { uri: "raw://a", title: "A", layer: "wiki" }
append edge wikilink "id-1" -> "id-2""#,
            )
            .unwrap();
        assert_eq!(h.done.r#gen, 1);
        let before = db.store.r#gen;
        let e = db
            .run(
                r#"insert docs { uri: "raw://fail", title: "F", layer: "wiki" }
update docs[id == "missing"] cas "x" { room: "inbox" }"#,
            )
            .unwrap_err();
        assert!(e.to_string().contains("update: no matching row"), "{e}");
        assert_eq!(db.store.r#gen, before);
        db.close().unwrap();
    }
    {
        let mut db = Db::open(&dir).unwrap();
        assert_eq!(db.store.r#gen, 1);
        let ok = db.run(r#"docs | uri == "raw://a" | { title }"#).unwrap();
        assert_eq!(text(&ok.rows[0], "title"), "A");
        let fail = db.run(r#"docs | uri == "raw://fail""#).unwrap();
        assert_eq!(fail.done.n, 0);
        db.close().unwrap();
    }
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn store_open_api_and_snapshot() {
    let dir = tmp();
    {
        let mut db = Db::open(&dir).unwrap();
        db.run(r#"insert docs { uri: "raw://n/snap", title: "snap", layer: "wiki" }"#)
            .unwrap();
        db.close().unwrap();
    }
    assert!(dir.join("head").exists());
    assert!(dir.join("log").exists());
    assert!(dir.join("snapshot").exists());
    fs::remove_file(dir.join("log")).unwrap();
    {
        let store = Store::open(&dir).unwrap();
        assert_eq!(store.r#gen, 1);
        assert!(
            store
                .collection("docs")
                .iter()
                .any(|r| text(r, "uri") == "raw://n/snap")
        );
        drop(store);
    }
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn bulk_insert_survives_reopen() {
    let dir = tmp();
    {
        let mut db = Db::open(&dir).unwrap();
        let h = db
            .run(
                r#"insert docs [
      { uri: "raw://ba", title: "BA", layer: "wiki" },
      { uri: "raw://bb", title: "BB", layer: "wiki" }
    ]"#,
            )
            .unwrap();
        assert_eq!(h.done.n, 2);
        assert_eq!(h.done.r#gen, 1);
        db.close().unwrap();
    }
    {
        let mut db = Db::open(&dir).unwrap();
        let q = db
            .run(r#"docs | uri == "raw://ba" or uri == "raw://bb" | take all"#)
            .unwrap();
        assert_eq!(q.done.n, 2);
        db.close().unwrap();
    }
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn index_survives_reopen() {
    let dir = tmp();
    {
        let mut db = Db::open(&dir).unwrap();
        db.run("index docs [wing, ts]").unwrap();
        db.run(r#"insert docs { uri: "raw://ixp", title: "IXP", layer: "wiki", wing: "rag" }"#)
            .unwrap();
        db.close().unwrap();
    }
    {
        let mut db = Db::open(&dir).unwrap();
        let plan = db
            .explain_as(r#"docs | wing == "rag" and ts > ago 7d | { id }"#, None)
            .unwrap();
        assert!(plan.contains("index=docs[wing,ts]"), "{plan}");
        let q = db.run(r#"docs | uri == "raw://ixp""#).unwrap();
        assert_eq!(q.done.n, 1);
        db.close().unwrap();
    }
    let _ = fs::remove_dir_all(&dir);
}
