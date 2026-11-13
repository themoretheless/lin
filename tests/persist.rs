use std::fs::{self, OpenOptions};
use std::io::Write;
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::Duration;

use lin::{Db, Store};

static SEQ: AtomicU64 = AtomicU64::new(0);

fn tmp() -> PathBuf {
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("lin-persist-{}-{n}", std::process::id()));
    let _ = fs::remove_dir_all(&p);
    fs::create_dir_all(&p).unwrap();
    p
}

/// Query text for cold-vs-hot comparisons: projections get an explicit `take all`, because
/// the implicit 50-row limit would keep different rows on tiers with different row order.
fn unlimited(q: &str) -> String {
    if q.contains('{') && !q.contains("take") {
        format!("{q} | take all")
    } else {
        q.to_string()
    }
}

/// Sort key that ignores how a tier represents a value (inline vs shared text).
fn row_key(row: &lin::Row) -> String {
    row.iter()
        .map(|(k, c)| match (c.text(), c.as_f64()) {
            (Some(t), _) => format!("{k}={t}"),
            (None, Some(n)) => format!("{k}={n}"),
            _ => format!("{k}={c:?}"),
        })
        .collect::<Vec<_>>()
        .join(";")
}

fn text<'a>(row: &'a lin::Row, k: &str) -> &'a str {
    row.get(k).and_then(|c| c.text()).unwrap_or("")
}

#[test]
fn embedding_survives_durable_reopen() {
    let dir = tmp();
    {
        let mut db = Db::open(&dir).unwrap();
        db.run(
            r#"insert docs { uri: "raw://emb", title: "vector wal", layer: "wiki", body: "dense" }"#,
        )
        .unwrap();
        let q = db
            .run(r#"docs | uri == "raw://emb" | { id, title, embedding }"#)
            .unwrap();
        assert_eq!(q.done.n, 1);
        assert!(
            q.rows[0]
                .get("embedding")
                .and_then(|c| c.as_vec())
                .is_some(),
            "insert should auto-embed"
        );
        // WAL path (before checkpoint): ship frames into a fresh memory db.
        let frames = db.export_wal_since(0).unwrap();
        assert!(!frames.is_empty(), "expected WAL frames before checkpoint");
        let mut mem = Db::empty();
        assert_eq!(mem.apply_wal(&frames).unwrap(), 1);
        assert!(
            mem.run(r#"docs | uri == "raw://emb""#).unwrap().rows[0]
                .get("embedding")
                .and_then(|c| c.as_vec())
                .is_some()
        );
        db.close().unwrap();
    }
    {
        let mut db = Db::open(&dir).unwrap();
        let q = db
            .run(r#"docs | uri == "raw://emb" | { title, embedding }"#)
            .unwrap();
        assert_eq!(q.done.n, 1);
        let emb = q.rows[0]
            .get("embedding")
            .and_then(|c| c.as_vec())
            .expect("embedding after reopen");
        assert!(emb.len() >= 8, "dim={}", emb.len());
        let v = db
            .run(r#"docs | search vec "vector wal" | take 5"#)
            .unwrap();
        assert!(v.done.n >= 1);
        db.close().unwrap();
    }
    let _ = fs::remove_dir_all(&dir);
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
fn large_bulk_insert_survives_reopen() {
    let dir = tmp();
    {
        let mut db = Db::open(&dir).unwrap();
        let mut src = String::from("insert docs [\n");
        for i in 0..256 {
            if i != 0 {
                src.push_str(",\n");
            }
            src.push_str(&format!(
                r#"{{ uri: "raw://large/{i}", title: "large {i}", layer: "wiki" }}"#
            ));
        }
        src.push_str("\n]");
        let h = db.run(&src).unwrap();
        assert_eq!(h.done.n, 256);
        db.close().unwrap();
    }
    {
        let mut db = Db::open(&dir).unwrap();
        let q = db.run(r#"docs | count"#).unwrap();
        assert_eq!(q.scalar().unwrap().as_int(), Some(256));
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

#[test]
fn backup_export_import_roundtrip() {
    let dir = tmp();
    let bak = dir.join("backup.json");
    {
        let mut db = Db::open(&dir).unwrap();
        db.run(r#"insert docs { uri: "raw://bak", title: "Backup", layer: "wiki" }"#)
            .unwrap();
        db.run(r#"append facts { s: "bak", p: tagged, o: "ok" }"#)
            .unwrap();
        db.export_backup(&bak).unwrap();
        db.close().unwrap();
    }
    assert!(bak.is_file());
    let mem = Db::import_backup(&bak).unwrap();
    let s = mem.stats();
    assert!(s.docs >= 1, "{s:?}");
    assert!(s.facts >= 1, "{s:?}");

    let dir2 = tmp();
    let mut db2 = Db::import_backup_into(&bak, &dir2).unwrap();
    let q = db2.run(r#"docs | uri == "raw://bak" | { title }"#).unwrap();
    assert_eq!(q.done.n, 1);
    db2.close().unwrap();
    let _ = fs::remove_dir_all(&dir);
    let _ = fs::remove_dir_all(&dir2);
}

#[test]
fn reader_snapshot_is_frozen_and_read_only() {
    let mut db = Db::empty();
    db.run(r#"insert docs { uri: "raw://r1", title: "R1", layer: "wiki" }"#)
        .unwrap();
    let snap = db.reader();
    assert_eq!(snap.r#gen(), 1);
    let q = snap.run(r#"docs | uri == "raw://r1""#).unwrap();
    assert_eq!(q.done.n, 1);

    // Writer advances; snapshot stays at gen=1.
    db.run(r#"insert docs { uri: "raw://r2", title: "R2", layer: "wiki" }"#)
        .unwrap();
    assert_eq!(db.stats().r#gen, 2);
    assert_eq!(snap.r#gen(), 1);
    assert_eq!(snap.run(r#"docs | uri == "raw://r2""#).unwrap().done.n, 0);

    let err = snap
        .run(r#"insert docs { uri: "raw://x", title: "X", layer: "wiki" }"#)
        .unwrap_err();
    assert!(err.to_string().contains("read-only"), "{err}");
}

#[test]
fn shared_readers_across_threads() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<lin::ReadDb>();

    let mut db = Db::empty();
    db.run(r#"insert docs { uri: "raw://t", title: "T", layer: "wiki" }"#)
        .unwrap();
    let snap = db.reader();
    let mut handles = Vec::new();
    for _ in 0..4 {
        let r = snap.clone();
        handles.push(std::thread::spawn(move || {
            let q = r.run(r#"docs | uri == "raw://t""#).unwrap();
            assert_eq!(q.done.n, 1);
            assert_eq!(r.r#gen(), 1);
        }));
    }
    for h in handles {
        h.join().unwrap();
    }
}

#[test]
fn open_read_loads_durable_without_writer() {
    let dir = tmp();
    {
        let mut db = Db::open(&dir).unwrap();
        db.run(r#"insert docs { uri: "raw://or", title: "OR", layer: "wiki" }"#)
            .unwrap();
        db.close().unwrap();
    }
    let r = Db::open_read(&dir).unwrap();
    let q = r.run(r#"docs | uri == "raw://or""#).unwrap();
    assert_eq!(q.done.n, 1);
    let err = r
        .run(r#"insert docs { uri: "raw://no", title: "NO", layer: "wiki" }"#)
        .unwrap_err();
    assert!(err.to_string().contains("read-only"), "{err}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn checkpoint_compacts_log() {
    let dir = tmp();
    {
        let mut db = Db::open(&dir).unwrap();
        for i in 0..5 {
            db.run(&format!(
                r#"insert docs {{ uri: "raw://c{i}", title: "C{i}", layer: "wiki" }}"#
            ))
            .unwrap();
        }
        let before = fs::metadata(dir.join("log")).unwrap().len();
        assert!(before > 0, "log should have grown");
        db.checkpoint().unwrap();
        let after = fs::metadata(dir.join("log")).unwrap().len();
        assert_eq!(after, 0, "log compacted to empty");
        assert_eq!(db.stats().writes_since_snapshot, 0);
        db.close().unwrap();
    }
    {
        let mut db = Db::open(&dir).unwrap();
        let q = db
            .run(r#"docs | uri == "raw://c0" or uri == "raw://c4" | take all"#)
            .unwrap();
        assert_eq!(q.done.n, 2);
        let stats = db.stats();
        assert!(stats.reopen_ms > 0 || stats.r#gen >= 5);
        let phases = stats.reopen;
        assert!(phases.total_ms > 0.0, "{phases:?}");
        assert!(phases.snapshot_ms > 0.0, "{phases:?}");
        let measured = phases.setup_ms
            + phases.snapshot_ms
            + phases.wal_ms
            + phases.metadata_ms
            + phases.indexes_ms
            + phases.row_maps_ms
            + phases.fts_ms;
        assert!(measured <= phases.total_ms + 0.5, "{phases:?}");
        db.close().unwrap();
    }
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn writer_lock_rejects_second_open() {
    let dir = tmp();
    let mut a = Db::open(&dir).unwrap();
    a.run(r#"insert docs { uri: "raw://lock", title: "L", layer: "wiki" }"#)
        .unwrap();
    let err = match Db::open(&dir) {
        Ok(_) => panic!("second writer open should fail"),
        Err(e) => e,
    };
    assert!(
        err.to_string().contains("locked"),
        "expected lock error, got {err}"
    );
    // open_read uses FENCE shared — compatible with live writer.
    let reader = Db::open_read(&dir).expect("open_read beside writer");
    assert_eq!(
        reader.run(r#"docs | uri == "raw://lock""#).unwrap().done.n,
        1
    );
    drop(reader);
    a.close().unwrap();
    let mut b = Db::open(&dir).unwrap();
    assert_eq!(b.run(r#"docs | uri == "raw://lock""#).unwrap().done.n, 1);
    b.close().unwrap();
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn quotas_reject_excess_rows() {
    let mut db = Db::empty().with_quotas(lin::Quotas {
        max_rows: 2,
        max_edges: 10_000,
        max_log_bytes: 512 * 1024 * 1024,
        ..Default::default()
    });
    // fixture empty collections still count? empty() has empty docs/users/orders/facts — 0 rows
    db.run(r#"insert docs { uri: "raw://q1", title: "Q1", layer: "wiki" }"#)
        .unwrap();
    db.run(r#"insert docs { uri: "raw://q2", title: "Q2", layer: "wiki" }"#)
        .unwrap();
    let err = db
        .run(r#"insert docs { uri: "raw://q3", title: "Q3", layer: "wiki" }"#)
        .unwrap_err();
    assert!(err.to_string().contains("quota"), "{err}");
}

#[test]
fn sync_normal_survives_checkpoint() {
    let dir = tmp();
    {
        let mut db = Db::open_with(
            &dir,
            lin::OpenOpts {
                sync: lin::SyncMode::Normal,
                cold: false,
            },
        )
        .unwrap();
        db.run(r#"insert docs { uri: "raw://n", title: "N", layer: "wiki" }"#)
            .unwrap();
        db.checkpoint().unwrap();
        db.close().unwrap();
    }
    let mut db = Db::open(&dir).unwrap();
    assert_eq!(db.run(r#"docs | uri == "raw://n""#).unwrap().done.n, 1);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn cold_checkpoint_reopen() {
    let dir = tmp();
    {
        let mut db = Db::open_with(
            &dir,
            lin::OpenOpts {
                sync: lin::SyncMode::Full,
                cold: true,
            },
        )
        .unwrap();
        // COLD_MIN_ROWS = 32
        let mut rows = String::from("insert docs [\n");
        for i in 0..40 {
            if i > 0 {
                rows.push(',');
            }
            rows.push_str(&format!(
                r#"{{ uri: "raw://c/{i}", title: "T{i}", layer: "wiki" }}"#
            ));
        }
        rows.push_str("\n]");
        db.run(&rows).unwrap();
        db.checkpoint().unwrap();
        assert!(lin::is_disk_dir(&dir));
        assert!(dir.join("segments").exists());
        db.close().unwrap();
    }
    let mut db = Db::open_with(
        &dir,
        lin::OpenOpts {
            sync: lin::SyncMode::Full,
            cold: true,
        },
    )
    .unwrap();
    // Out-of-core reads work without promoting cold table into heap Vec<Row>
    assert!(db.store.is_disk_backed("docs"));
    assert_eq!(db.stats().docs, 40);
    assert_eq!(db.run(r#"docs | count"#).unwrap().scalar_as::<i64>().unwrap(), 40);
    assert_eq!(
        db.run(r#"docs | uri == "raw://c/7" | { title }"#)
            .unwrap()
            .rows[0]
            .get("title")
            .and_then(lin::Cell::text),
        Some("T7")
    );
    assert!(db.store.is_disk_backed("docs"));
    assert_eq!(db.run(r#"docs | take 100"#).unwrap().done.n, 40);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn wal_export_apply_roundtrip() {
    let a_dir = tmp();
    let frames;
    {
        let mut a = Db::open(&a_dir).unwrap();
        a.run(r#"insert docs { uri: "raw://w", title: "W", layer: "wiki" }"#)
            .unwrap();
        frames = a.export_wal_since(0).unwrap();
        assert!(!frames.is_empty());
        a.close().unwrap();
    }
    {
        let mut b = Db::empty();
        let n = b.apply_wal(&frames).unwrap();
        assert_eq!(n, 1);
        assert_eq!(b.run(r#"docs | uri == "raw://w""#).unwrap().done.n, 1);
        // Durable primary apply refused.
        let mut durable = Db::open(tmp()).unwrap();
        assert!(durable.apply_wal(&frames).is_err());
        durable.close().unwrap();
    }
    let _ = fs::remove_dir_all(&a_dir);
}

#[test]
fn durable_follower_hot_standby() {
    let primary_dir = tmp();
    let follower_dir = tmp();
    let bak = tmp().join("boot.linbak");

    {
        let mut primary = Db::open(&primary_dir).unwrap();
        primary
            .run(r#"insert docs { uri: "raw://a", title: "A", layer: "wiki" }"#)
            .unwrap();
        primary.export_backup(&bak).unwrap();
        // After backup checkpoint the log is compacted — ship post-bootstrap WAL.
        primary
            .run(r#"insert docs { uri: "raw://b", title: "B", layer: "wiki" }"#)
            .unwrap();
        let frames = primary.export_wal_since(0).unwrap();
        assert!(!frames.is_empty());

        let mut follower = Db::bootstrap_follower(&bak, &follower_dir).unwrap();
        assert!(follower.is_follower());
        assert_eq!(follower.r#gen(), 1);
        assert!(
            follower
                .run(r#"insert docs { uri: "raw://x", title: "X", layer: "wiki" }"#)
                .is_err(),
            "follower rejects user writes"
        );
        let n = follower.apply_wal(&frames).unwrap();
        assert_eq!(n, 1);
        assert_eq!(
            follower.run(r#"docs | uri == "raw://b""#).unwrap().done.n,
            1
        );
        assert_eq!(follower.run(r#"docs | take 10"#).unwrap().done.n, 2);
        follower.close().unwrap();
        primary.close().unwrap();
    }

    // Reopen follower: durable apply survived.
    {
        let mut follower = Db::open_follower(&follower_dir).unwrap();
        assert_eq!(follower.run(r#"docs | take 10"#).unwrap().done.n, 2);
        follower.close().unwrap();
    }
    // open_read on follower dir for query processes.
    {
        let r = Db::open_read(&follower_dir).unwrap();
        assert_eq!(r.run(r#"docs | uri == "raw://b""#).unwrap().done.n, 1);
    }

    let _ = fs::remove_dir_all(&primary_dir);
    let _ = fs::remove_dir_all(&follower_dir);
    let _ = fs::remove_file(&bak);
}

#[test]
fn follower_rejects_wal_gap() {
    let primary_dir = tmp();
    let follower_dir = tmp();
    {
        let mut primary = Db::open(&primary_dir).unwrap();
        primary
            .run(r#"insert docs { uri: "raw://1", title: "1", layer: "wiki" }"#)
            .unwrap();
        primary
            .run(r#"insert docs { uri: "raw://2", title: "2", layer: "wiki" }"#)
            .unwrap();
        let frames = primary.export_wal_since(0).unwrap();
        // Empty follower at gen 0 needs gen=1 first; skip by applying only later
        // frames via a truncated ship is simulated by applying frames twice with
        // a hole: apply none, then only gen=2 is impossible from export — instead
        // open empty follower and feed frames starting after a fake gen.
        let mut follower = Db::open_follower(&follower_dir).unwrap();
        // Apply all — ok from empty (gen 0 → 1,2…).
        assert!(follower.apply_wal(&frames).is_ok());
        follower.close().unwrap();
        primary.close().unwrap();
    }
    // Fresh follower without bootstrap cannot jump to mid-stream after primary
    // checkpoint emptied the log — export may be empty; gap when skipping gens:
    {
        let mut primary = Db::open(&primary_dir).unwrap();
        primary
            .run(r#"insert docs { uri: "raw://3", title: "3", layer: "wiki" }"#)
            .unwrap();
        let frames = primary.export_wal_since(0).unwrap();
        let mut other = Db::open_follower(tmp()).unwrap();
        // other at gen 0; frames start at gen after primary reopen (not 1).
        let err = other.apply_wal(&frames).unwrap_err();
        assert!(err.to_string().contains("gap"), "expected gap, got {err}");
        other.close().unwrap();
        primary.close().unwrap();
    }
    let _ = fs::remove_dir_all(&primary_dir);
    let _ = fs::remove_dir_all(&follower_dir);
}

#[test]
fn ship_tcp_follower_sync() {
    let primary_dir = tmp();
    let follower_dir = tmp();
    let bak = primary_dir.join("boot.linbak");

    let mut primary = Db::open(&primary_dir).unwrap();
    primary
        .run(r#"insert docs { uri: "raw://s0", title: "S0", layer: "wiki" }"#)
        .unwrap();
    primary.export_backup(&bak).unwrap();
    primary
        .run(r#"insert docs { uri: "raw://s1", title: "S1", layer: "wiki" }"#)
        .unwrap();
    // Keep primary open so post-backup WAL frames remain in the log.

    let mut follower = Db::bootstrap_follower(&bak, &follower_dir).unwrap();
    assert_eq!(follower.r#gen(), 1);

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let ready = Arc::new(Barrier::new(2));
    let ready2 = Arc::clone(&ready);
    let serve = thread::spawn(move || {
        ready2.wait();
        lin::ship::serve_one(&primary, &listener).unwrap();
        let _ = primary.close();
    });

    ready.wait();
    thread::sleep(Duration::from_millis(20));
    let frames = lin::ship::pull(addr, follower.r#gen()).unwrap();
    let n = follower.apply_wal(&frames).unwrap();
    assert_eq!(n, 1);
    assert_eq!(
        follower.run(r#"docs | uri == "raw://s1""#).unwrap().done.n,
        1
    );
    follower.close().unwrap();
    serve.join().unwrap();

    let r = Db::open_read(&follower_dir).unwrap();
    assert_eq!(r.run(r#"docs | take 10"#).unwrap().done.n, 2);

    let _ = fs::remove_dir_all(&primary_dir);
    let _ = fs::remove_dir_all(&follower_dir);
}

#[test]
fn memory_snapshot_restore() {
    let mut db = Db::empty();
    db.run(r#"insert docs { uri: "raw://p", title: "P", layer: "wiki" }"#)
        .unwrap();
    let pin = db.run(r#"pin "s1""#).unwrap();
    assert!(
        pin.message.as_deref().unwrap_or("").contains("memory pin"),
        "{:?}",
        pin.message
    );
    db.run(r#"insert docs { uri: "raw://p2", title: "P2", layer: "wiki" }"#)
        .unwrap();
    assert_eq!(db.run(r#"docs | take 10"#).unwrap().done.n, 2);
    db.run(r#"unpin "s1""#).unwrap();
    assert_eq!(db.run(r#"docs | take 10"#).unwrap().done.n, 1);
    // Legacy aliases still work.
    db.run(r#"snapshot "s2""#).unwrap();
    db.run(r#"restore "s2""#).unwrap();
}

#[test]
fn cold_backup_self_contained() {
    let dir = tmp();
    let bak = dir.join("backup.lin");
    {
        let mut db = Db::open_with(
            &dir,
            lin::OpenOpts {
                sync: lin::SyncMode::Full,
                cold: true,
            },
        )
        .unwrap();
        let mut rows = String::from("insert docs [\n");
        for i in 0..40 {
            if i > 0 {
                rows.push(',');
            }
            rows.push_str(&format!(
                r#"{{ uri: "raw://b/{i}", title: "T{i}", layer: "wiki" }}"#
            ));
        }
        rows.push_str("\n]");
        db.run(&rows).unwrap();
        db.checkpoint().unwrap();
        assert!(lin::is_disk_dir(&dir));
        assert!(dir.join("segments").exists());
        db.export_backup(&bak).unwrap();
        db.close().unwrap();
    }
    // Backup imports without the data dir / cold files.
    let mut mem = Db::import_backup(&bak).unwrap();
    assert_eq!(mem.run(r#"docs | take 100"#).unwrap().done.n, 40);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn open_read_does_not_see_commits_after_open() {
    let dir = tmp();
    let mut w = Db::open(&dir).unwrap();
    w.run(r#"insert docs { uri: "raw://before", title: "B", layer: "wiki" }"#)
        .unwrap();
    let r = Db::open_read(&dir).unwrap();
    assert_eq!(r.run(r#"docs | uri == "raw://before""#).unwrap().done.n, 1);
    w.run(r#"insert docs { uri: "raw://after", title: "A", layer: "wiki" }"#)
        .unwrap();
    assert_eq!(r.run(r#"docs | uri == "raw://after""#).unwrap().done.n, 0);
    drop(r);
    w.close().unwrap();
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn crash_exit_keeps_full_commit() {
    if let Ok(child) = std::env::var("LIN_CRASH_CHILD") {
        let mut db = Db::open(&child).unwrap();
        db.run(r#"insert docs { uri: "raw://crash", title: "K", layer: "wiki" }"#)
            .unwrap();
        unsafe { libc::_exit(1) };
    }
    let dir = tmp();
    let exe = std::env::current_exe().unwrap();
    let status = std::process::Command::new(&exe)
        .env("LIN_CRASH_CHILD", &dir)
        .args(["crash_exit_keeps_full_commit", "--exact"])
        .status()
        .unwrap();
    assert_eq!(status.code(), Some(1), "child should _exit(1) after commit");
    let mut db = Db::open(&dir).unwrap();
    assert_eq!(
        db.run(r#"docs | uri == "raw://crash""#).unwrap().done.n,
        1,
        "Full WAL must survive process abort without Drop/close"
    );
    db.close().unwrap();
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn crash_exit_keeps_entire_group_commit() {
    if let Ok(child) = std::env::var("LIN_GROUP_CRASH_CHILD") {
        let mut db = Db::open(&child).unwrap();
        db.run_group([
            r#"insert docs { uri: "raw://group-crash/a", title: "A", layer: "wiki" }"#,
            r#"insert docs { uri: "raw://group-crash/b", title: "B", layer: "wiki" }"#,
        ])
        .unwrap();
        unsafe { libc::_exit(1) };
    }
    let dir = tmp();
    let exe = std::env::current_exe().unwrap();
    let status = std::process::Command::new(&exe)
        .env("LIN_GROUP_CRASH_CHILD", &dir)
        .args(["crash_exit_keeps_entire_group_commit", "--exact"])
        .status()
        .unwrap();
    assert_eq!(status.code(), Some(1), "child should _exit after commit");
    let mut db = Db::open(&dir).unwrap();
    assert_eq!(
        db.run(r#"docs | uri == "raw://group-crash/a" or uri == "raw://group-crash/b" | take all"#)
            .unwrap()
            .done
            .n,
        2
    );
    db.close().unwrap();
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn wal_update_after_snapshot_replays_against_row_maps() {
    if let Ok(child) = std::env::var("LIN_UPDATE_REPLAY_CHILD") {
        let mut db = Db::open(&child).unwrap();
        let hash = std::env::var("LIN_UPDATE_REPLAY_HASH").unwrap();
        db.run(&format!(
            r#"update docs[uri == "raw://update-replay"] cas "{hash}" {{ title: "after" }}"#
        ))
        .unwrap();
        unsafe { libc::_exit(1) };
    }

    let dir = tmp();
    let mut db = Db::open(&dir).unwrap();
    let inserted = db
        .run(
        r#"insert docs { uri: "raw://update-replay", title: "before", layer: "wiki", body: "replay term" }"#,
    )
    .unwrap();
    let hash = text(&inserted.rows[0], "hash").to_string();
    db.close().unwrap();

    let exe = std::env::current_exe().unwrap();
    let status = std::process::Command::new(&exe)
        .env("LIN_UPDATE_REPLAY_CHILD", &dir)
        .env("LIN_UPDATE_REPLAY_HASH", hash)
        .args([
            "wal_update_after_snapshot_replays_against_row_maps",
            "--exact",
        ])
        .status()
        .unwrap();
    assert_eq!(status.code(), Some(1));

    let mut reopened = Db::open(&dir).unwrap();
    let row = reopened
        .run(r#"docs | uri == "raw://update-replay" | { title }"#)
        .unwrap();
    assert_eq!(text(&row.rows[0], "title"), "after");
    reopened.close().unwrap();
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn durable_fts_blob_survives_checkpoint_reopen() {
    let dir = tmp();
    {
        let mut db = Db::open(&dir).unwrap();
        db.run(
            r#"insert docs { uri: "raw://fts", title: "wal postings", layer: "wiki", body: "durable lex" }"#,
        )
        .unwrap();
        db.checkpoint().unwrap();
        assert!(
            dir.join("fts").join("docs.bin").is_file(),
            "checkpoint should write fts/docs.bin"
        );
        db.close().unwrap();
    }
    {
        let mut db = Db::open(&dir).unwrap();
        let q = db.run(r#"docs | search lex "postings" | take 5"#).unwrap();
        assert_eq!(q.done.n, 1, "loaded postings should find the row");
        db.close().unwrap();
    }
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn ship_tls_token_roundtrip() {
    let primary_dir = tmp();
    let follower_dir = tmp();
    let bak = primary_dir.join("boot.linbak");

    let cert = rcgen::generate_simple_self_signed(["localhost".into()]).unwrap();
    let tls = lin::ship::TlsServer {
        cert_pem: cert.cert.pem().into_bytes(),
        key_pem: cert.key_pair.serialize_pem().into_bytes(),
    };
    let ca = tls.cert_pem.clone();

    let mut primary = Db::open(&primary_dir).unwrap();
    primary
        .run(r#"insert docs { uri: "raw://tls0", title: "T0", layer: "wiki" }"#)
        .unwrap();
    primary.export_backup(&bak).unwrap();
    primary
        .run(r#"insert docs { uri: "raw://tls1", title: "T1", layer: "wiki" }"#)
        .unwrap();

    let mut follower = Db::bootstrap_follower(&bak, &follower_dir).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let serve_opts = lin::ship::ServeOpts {
        token: Some("s3cret".into()),
        tls: Some(tls),
    };
    let serve = thread::spawn(move || {
        lin::ship::serve_one_opts(&primary, &listener, &serve_opts).unwrap();
        let _ = primary.close();
    });
    thread::sleep(Duration::from_millis(40));
    let frames = lin::ship::pull_with(
        addr,
        follower.r#gen(),
        &lin::ship::PullOpts {
            token: Some("s3cret".into()),
            tls: lin::ship::TlsClient::CaPem(ca),
            server_name: None,
        },
    )
    .unwrap();
    let n = follower.apply_wal(&frames).unwrap();
    assert_eq!(n, 1);
    assert_eq!(
        follower
            .run(r#"docs | uri == "raw://tls1""#)
            .unwrap()
            .done
            .n,
        1
    );
    follower.close().unwrap();
    serve.join().unwrap();
    let _ = fs::remove_dir_all(&primary_dir);
    let _ = fs::remove_dir_all(&follower_dir);
}

#[test]
fn catalog_filter_survives_checkpoint_reopen() {
    let dir = tmp();
    {
        let mut db = Db::open(&dir).unwrap();
        db.run(
            r#"insert docs [
          { uri: "raw://w", title: "W", layer: "wiki" },
          { uri: "raw://r", title: "R", layer: "raw" }
        ]"#,
        )
        .unwrap();
        db.run(r#"filter docs layer == "wiki""#).unwrap();
        db.checkpoint().unwrap();
        db.close().unwrap();
    }
    {
        let mut db = Db::open(&dir).unwrap();
        assert_eq!(db.run(r#"docs | take all"#).unwrap().done.n, 1);
        assert_eq!(db.run(r#"docs all | take all"#).unwrap().done.n, 2);
        db.close().unwrap();
    }
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn owned_survives_checkpoint_reopen() {
    let dir = tmp();
    {
        let mut db = Db::open(&dir).unwrap();
        db.run("owned stamp { hash: text, ts: time }").unwrap();
        db.run("col notes { title: text, stamp }").unwrap();
        db.run(r#"insert notes { title: "n", hash: "h", ts: ago 1s }"#)
            .unwrap();
        db.checkpoint().unwrap();
        db.close().unwrap();
    }
    {
        let mut db = Db::open(&dir).unwrap();
        let h = db.run(r#"notes | { title, hash } | take all"#).unwrap();
        assert_eq!(h.done.n, 1);
        db.close().unwrap();
    }
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn large_insert_wal_contains_rows_elided_from_handle() {
    let dir = tmp();
    let mut primary = Db::open(&dir).unwrap();
    primary
        .run(r#"insert docs { uri: "raw://prefix", title: "before bulk", layer: "wiki" }"#)
        .unwrap();
    let records = (0..129)
        .map(|i| {
            format!(
                r#"{{ uri: "raw://bulk/{i}", title: "wal {i}", layer: "wiki", body: "body {i}" }}"#
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let handle = primary.run(&format!("insert docs [{records}]")).unwrap();
    assert!(handle.rows.is_empty(), "large result rows remain elided");
    let expected = primary.store.collection("docs").to_vec();
    assert_eq!(expected.len(), 130);
    let frames = primary.export_wal_since(0).unwrap();
    let mut replica = Db::empty();
    assert_eq!(replica.apply_wal(&frames).unwrap(), 2);
    assert!(
        replica.store.collection("docs") == expected.as_slice(),
        "WAL must include every field and embedding despite empty Handle.rows"
    );
    assert_eq!(handle.done.n, 129);
    primary.close().unwrap();
    let mut reopened = Db::open(&dir).unwrap();
    assert_eq!(
        reopened
            .run("docs | count")
            .unwrap()
            .scalar_as::<i64>()
            .unwrap(),
        130
    );
    reopened.close().unwrap();
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn absolute_timestamps_survive_bulk_wal_checkpoint_and_reopen() {
    let dir = tmp();
    let values = [i64::MIN, -1, 1700000000123, i64::MAX];
    let expected;
    {
        let mut primary = Db::open(&dir).unwrap();
        primary.run("index docs [ts]").unwrap();
        let records = (0..129)
            .map(|i| {
                format!(
                    r#"{{ id: "t{i}", uri: "time://{i}", ts: timestamp({}) }}"#,
                    values[i % values.len()]
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        assert_eq!(
            primary
                .run(&format!("insert docs [{records}]"))
                .unwrap()
                .done
                .n,
            129
        );
        expected = primary.store.collection("docs").to_vec();
        for (i, row) in expected.iter().enumerate() {
            assert_eq!(
                row.get("ts"),
                Some(&lin::Cell::Time(values[i % values.len()]))
            );
        }
        let frames = primary.export_wal_since(0).unwrap();
        let mut replica = Db::empty();
        replica.apply_wal(&frames).unwrap();
        assert_eq!(replica.store.collection("docs"), expected.as_slice());
        primary.checkpoint().unwrap();
    }
    let mut reopened = Db::open(&dir).unwrap();
    assert_eq!(reopened.store.collection("docs"), expected.as_slice());
    assert_eq!(
        reopened
            .run("docs | ts == timestamp(-1) | take all")
            .unwrap()
            .done
            .n,
        32
    );
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn large_sparse_embedding_insert_is_one_durable_frame_and_replays_exactly() {
    let dir = tmp();
    let expected;
    let log_len;
    {
        let mut primary = Db::open(&dir).unwrap();
        let records = (0..10_000).map(|i| format!(r#"{{ id: "sp{i}", uri: "sparse://{i}", title: "shared sparse title", body: "body {i}", ts: timestamp(1700000000123) }}"#)).collect::<Vec<_>>().join(",");
        let handle = primary.run(&format!("insert docs [{records}]")).unwrap();
        assert_eq!(handle.done.n, 10_000);
        expected = primary.store.collection("docs").to_vec();
        let frames = primary.export_wal_since(0).unwrap();
        assert_eq!(frames[8], 3);
        log_len = frames.len();
        let dense_vector_bytes = expected
            .iter()
            .map(|row| {
                row.get("embedding")
                    .and_then(lin::Cell::as_vec)
                    .map_or(4, |v| 4 + v.len() * 4)
            })
            .sum::<usize>();
        eprintln!(
            "SPARSE_WAL rows=10000 frame_bytes={log_len} dense_vector_bytes={dense_vector_bytes}"
        );
        assert!(log_len < 16 * 1024 * 1024);
        let mut replica = Db::empty();
        assert_eq!(replica.apply_wal(&frames).unwrap(), 1, "one atomic frame");
        assert_eq!(replica.store.collection("docs"), expected.as_slice());
        let replay_dir = tmp();
        drop(Db::open(&replay_dir).unwrap()); // Empty generation-zero checkpoint.
        fs::write(replay_dir.join("log"), &frames).unwrap();
        let replayed = Db::open(&replay_dir).unwrap();
        assert_eq!(
            replayed.store.collection("docs"),
            expected.as_slice(),
            "local WAL replay"
        );
        drop(replayed);
        fs::remove_dir_all(replay_dir).unwrap();
        primary.checkpoint().unwrap();
    }
    let reopened = Db::open(&dir).unwrap();
    assert_eq!(reopened.store.collection("docs"), expected.as_slice());
    drop(reopened);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn partial_index_predicate_survives_wal_and_checkpoint_reopen() {
    let dir = tmp();
    {
        let mut db = Db::open(&dir).unwrap();
        db.run(r#"index docs [wing] where layer == "wiki""#)
            .unwrap();
        db.run(
            r#"insert docs [
                { id: "d1", uri: "p://1", title: "One", layer: "wiki", wing: "rag" },
                { id: "d2", uri: "p://2", title: "Two", layer: "raw", wing: "rag" }
            ]"#,
        )
        .unwrap();
    }
    // Reopen from WAL only (before checkpoint)
    {
        let mut db = Db::open(&dir).unwrap();
        let idx = db
            .catalog
            .indexes
            .values()
            .find(|i| i.collection == "docs" && i.fields == vec!["wing"])
            .expect("index exists");
        assert!(
            idx.pred.is_some(),
            "partial index predicate should survive WAL replay"
        );
        // Querying raw layer by wing=="rag" must NOT mistakenly use the wiki-only partial index
        let rows = db.run(r#"docs | wing == "rag" | take all"#).unwrap().into_rows();
        assert_eq!(rows.len(), 2);
        db.checkpoint().unwrap();
    }
    // Reopen from checkpoint snapshot + cold storage
    {
        let mut db = Db::open(&dir).unwrap();
        let idx = db
            .catalog
            .indexes
            .values()
            .find(|i| i.collection == "docs" && i.fields == vec!["wing"])
            .expect("index exists");
        assert!(
            idx.pred.is_some(),
            "partial index predicate should survive checkpoint"
        );
        // Also verify point lookups and index lookups work against cold (mmap) collections
        let by_id = db.run(r#"docs | id == "d1" | { title }"#).unwrap().into_rows();
        assert_eq!(by_id.len(), 1);
        assert_eq!(by_id[0]["title"].text(), Some("One"));
        let by_uri = db.run(r#"docs | uri == "p://2" | { title }"#).unwrap().into_rows();
        assert_eq!(by_uri.len(), 1);
        assert_eq!(by_uri[0]["title"].text(), Some("Two"));
        let wiki_rag = db
            .run(r#"docs | layer == "wiki" and wing == "rag" | take all"#)
            .unwrap()
            .into_rows();
        assert_eq!(wiki_rag.len(), 1);
        assert_eq!(wiki_rag[0]["id"].text(), Some("d1"));
    }
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn last_acknowledged_prefix_recovers_after_torn_trailing_frame_and_continues_writing() {
    let dir = tmp();
    let committed_log: Vec<u8>;
    let uncommitted_tail: Vec<u8>;
    let expected_prefix: Vec<lin::Row>;
    {
        let mut db = Db::open(&dir).unwrap();
        for i in 1..=3 {
            db.run(&format!(
                r#"insert docs {{ id: "ack{i}", uri: "ack://{i}", title: "Ack {i}", layer: "wiki" }}"#
            ))
            .unwrap();
        }
        expected_prefix = db.store.collection("docs").to_vec();
        committed_log = fs::read(dir.join("log")).unwrap();
        db.run(r#"insert docs { id: "torn4", uri: "ack://4", title: "Torn 4", layer: "wiki" }"#)
            .unwrap();
        let full_log = fs::read(dir.join("log")).unwrap();
        uncommitted_tail = full_log[committed_log.len()..].to_vec();
        assert!(uncommitted_tail.len() > 16);
    }
    // Remove snapshot and head generated by Db::drop closing checkpoint,
    // so tests run against pure WAL replay.
    let _ = fs::remove_file(dir.join("snapshot"));
    let _ = fs::remove_file(dir.join("head"));

    // Sample representative cuts: all header offsets (1..=32), stepped body offsets, and all tail/checksum offsets.
    let mut cuts = Vec::new();
    let tail_len = uncommitted_tail.len();
    for cut in 1..=32.min(tail_len.saturating_sub(1)) {
        cuts.push(cut);
    }
    let body_end = tail_len.saturating_sub(32);
    if body_end > 32 {
        cuts.extend((33..body_end).step_by(64));
    }
    for cut in body_end.max(1)..tail_len {
        cuts.push(cut);
    }
    cuts.sort_unstable();
    cuts.dedup();

    // Every torn prefix of the 4th frame (header, body, or checksum) must recover the 3 acknowledged commits.
    for cut in cuts {
        let _ = fs::remove_file(dir.join("snapshot"));
        let _ = fs::remove_file(dir.join("head"));
        let mut bytes = committed_log.clone();
        bytes.extend_from_slice(&uncommitted_tail[..cut]);
        fs::write(dir.join("log"), &bytes).unwrap();
        let reopened = Db::open(&dir).unwrap();
        assert_eq!(
            reopened.r#gen(),
            3,
            "generation mismatch after torn tail cut={cut}/{}",
            tail_len
        );
        assert_eq!(
            reopened.store.collection("docs"),
            expected_prefix.as_slice(),
            "prefix state mismatch at cut={cut}"
        );
    }

    // Reopen after a torn tail and append a new committed write cleanly.
    let _ = fs::remove_file(dir.join("snapshot"));
    let _ = fs::remove_file(dir.join("head"));
    let mut half = committed_log.clone();
    half.extend_from_slice(&uncommitted_tail[..uncommitted_tail.len() / 2]);
    fs::write(dir.join("log"), &half).unwrap();
    {
        let mut db = Db::open(&dir).unwrap();
        db.run(
            r#"insert docs { id: "ack4", uri: "ack://4-clean", title: "Clean 4", layer: "wiki" }"#,
        )
        .unwrap();
        assert_eq!(db.r#gen(), 4);
    }
    let mut final_db = Db::open(&dir).unwrap();
    assert_eq!(final_db.r#gen(), 4);
    assert_eq!(
        final_db
            .run("docs | count")
            .unwrap()
            .scalar_as::<i64>()
            .unwrap(),
        4
    );
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn test_checkpoint_pause_budget_lin_0163() {
    let dir = tmp();
    let mut db = Db::open(&dir).unwrap();
    for i in 0..200 {
        db.run(&format!(
            r#"insert docs {{ id: "chk_{i}", uri: "raw://chk_{i}", title: "Checkpoint document {i} with payload text", layer: "wiki" }}"#
        ))
        .unwrap();
    }
    let t0 = std::time::Instant::now();
    db.checkpoint().unwrap();
    let pause = t0.elapsed();
    assert!(
        pause.as_millis() < 250,
        "checkpoint pause budget exceeded: {pause:?}"
    );
    assert_eq!(db.stats().writes_since_snapshot, 0);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn unified_db_opens_and_mutates_native_disk_dir() {
    let dir = tmp();
    {
        let mut eng = lin::DiskEngine::open(&dir, true).unwrap();
        eng.commit_ops(vec![lin::DiskWalOp::EnsureTable {
            schema: lin::DiskTableSchema {
                name: "docs".to_string(),
                columns: vec![
                    lin::DiskColSchema {
                        name: "id".to_string(),
                        kind: lin::DiskColKind::TextZstd,
                    },
                    lin::DiskColSchema {
                        name: "uri".to_string(),
                        kind: lin::DiskColKind::TextZstd,
                    },
                    lin::DiskColSchema {
                        name: "title".to_string(),
                        kind: lin::DiskColKind::TextZstd,
                    },
                    lin::DiskColSchema {
                        name: "wing".to_string(),
                        kind: lin::DiskColKind::TextDict,
                    },
                    lin::DiskColSchema {
                        name: "layer".to_string(),
                        kind: lin::DiskColKind::TextDict,
                    },
                    lin::DiskColSchema {
                        name: "body".to_string(),
                        kind: lin::DiskColKind::TextZstd,
                    },
                    lin::DiskColSchema {
                        name: "hash".to_string(),
                        kind: lin::DiskColKind::TextZstd,
                    },
                ],
                indexed_fields: vec!["uri".to_string(), "wing".to_string()],
                cluster_by: None,
            },
        }])
        .unwrap();

        let mut rows = Vec::with_capacity(50);
        for i in 0..50 {
            let mut r = lin::Row::new();
            let body = format!("native disk body {i}");
            r.insert("id".into(), lin::Cell::from_text(&format!("nd-{i}")));
            r.insert("uri".into(), lin::Cell::from_text(&format!("raw://nd/{i}")));
            r.insert("title".into(), lin::Cell::from_text(&format!("Native {i}")));
            r.insert(
                "wing".into(),
                lin::Cell::from_text(if i % 2 == 0 { "even" } else { "odd" }),
            );
            r.insert("layer".into(), lin::Cell::from_text("wiki"));
            r.insert("hash".into(), lin::Cell::from_text(&format!("hash-{i}")));
            r.insert("body".into(), lin::Cell::from_text(&body));
            rows.push(r);
        }
        eng.write_bulk_segment("docs", &rows).unwrap();
        eng.save_tombstones_and_manifest().unwrap();
    }

    // 1. Read-only snapshot directly on pure DiskEngine directory
    {
        let rdb = Db::open_read(&dir).unwrap();
        assert_eq!(rdb.stats().docs, 50);
        assert_eq!(
            rdb.run(r#"docs | count"#)
                .unwrap()
                .scalar_as::<i64>()
                .unwrap(),
            50
        );
        assert_eq!(
            rdb.run(r#"docs | wing == "even" | count"#)
                .unwrap()
                .scalar_as::<i64>()
                .unwrap(),
            25
        );
        let h = rdb
            .run(r#"docs | id == "nd-12" | { id, title }"#)
            .unwrap();
        assert_eq!(h.rows.len(), 1);
        assert_eq!(
            h.rows[0].get("title").and_then(lin::Cell::text),
            Some("Native 12")
        );
    }

    // 2. Read-write unified Db directly on pure DiskEngine directory
    {
        let mut db = Db::open(&dir).unwrap();
        assert!(db.is_durable());
        assert!(db.store.is_disk_backed("docs"));

        // Out-of-core group count + filter project without promoting into heap Vec<Row>
        let grouped = db.run(r#"docs | count by wing"#).unwrap();
        assert_eq!(grouped.rows.len(), 2);
        let proj = db
            .run(r#"docs | wing == "odd" | { id, title } | take 5"#)
            .unwrap();
        assert_eq!(proj.rows.len(), 5);
        assert!(db.store.is_disk_backed("docs"));

        // Durable write through unified Db -> commits to DiskEngine WAL/segments
        db.run(r#"insert docs { id: "nd-50", uri: "raw://nd/50", title: "Native 50", wing: "even", layer: "wiki", body: "extra" }"#)
            .unwrap();
        db.checkpoint().unwrap();
        db.close().unwrap();
    }

    // 3. Reopen and verify durable mutation persisted in DiskEngine
    {
        let mut db = Db::open(&dir).unwrap();
        assert_eq!(
            db.run(r#"docs | count"#)
                .unwrap()
                .scalar_as::<i64>()
                .unwrap(),
            51
        );
        let h = db
            .run(r#"docs | id == "nd-50" | { title, wing }"#)
            .unwrap();
        assert_eq!(h.rows.len(), 1);
        assert_eq!(
            h.rows[0].get("title").and_then(lin::Cell::text),
            Some("Native 50")
        );
        db.close().unwrap();
    }

    let _ = fs::remove_dir_all(&dir);
}

/// Zero-row disk aggregation fast paths (range count, substring count, columnar SUM)
/// must return exactly what the in-memory executor returns for the same data.
#[test]
fn cold_aggregates_match_in_memory() {
    fn seed(db: &mut Db, from: usize, to: usize) {
        for i in 0..5 {
            if from == 0 {
                db.run(&format!(
                    r#"insert users {{ id: "u{i}", email: "user{i}@example.com" }}"#
                ))
                .unwrap();
            }
        }
        let mut docs = String::from("insert docs [\n");
        let mut orders = String::from("insert orders [\n");
        for i in from..to {
            if i > from {
                docs.push(',');
                orders.push(',');
            }
            let wing = ["rag", "ops", "wiki"][i % 3];
            let title = if i % 4 == 0 { "wal replay" } else { "plain note" };
            docs.push_str(&format!(
                r#"{{ id: "d{i}", uri: "raw://agg/{i}", title: "{title} {i}", wing: "{wing}", layer: "wiki", ts: ago {}d }}"#,
                i % 20 + 1
            ));
            orders.push_str(&format!(
                r#"{{ id: "o{i}", user_id: "u{}", total: {}, ts: ago {}d }}"#,
                i % 5,
                (i * 7) % 250,
                i % 9 + 1
            ));
        }
        docs.push_str("\n]");
        orders.push_str("\n]");
        db.run(&docs).unwrap();
        db.run(&orders).unwrap();
    }

    const QUERIES: &[&str] = &[
        r#"docs | ts > ago 7d | count"#,
        r#"docs | ts <= ago 7d | count"#,
        r#"docs | wing == "rag" and ts > ago 7d | count"#,
        r#"docs | ts > ago 7d and wing == "rag" | count"#,
        r#"docs | wing == "rag" and ts > ago 7d | count by wing"#,
        r#"docs | wing == "nope" and ts > ago 7d | count by wing"#,
        r#"docs | ts > ago 7d | count by wing"#,
        r#"docs | title ~ "wal" | count"#,
        r#"docs | title ~ "absent" | count"#,
        r#"docs | wing ~ "ik" | count"#,
        r#"orders | sum total"#,
        r#"orders | sum total by user_id"#,
        r#"orders | total > 100 | count"#,
        r#"orders | total >= 100.5 | count"#,
        r#"orders | total < 0 | count"#,
        r#"orders | total > 100 | count by user_id"#,
        r#"orders | total > 100 | sum total"#,
        r#"orders | total > 100 | sum total by user_id"#,
        r#"orders | total > 100 | max total by user_id"#,
        r#"orders | total > 100 | { id, total }"#,
        r#"orders | total > 100 and user_id == "u1" | { id }"#,
        r#"docs | ts > ago 7d | { id, title }"#,
        r#"docs | ts > ago 7d and title ~ "wal" | { title, wing }"#,
        r#"docs | ts > ago 7d | { id } | take 3"#,
        r#"docs | title ~ "wal" and ts > ago 30d | { id, title }"#,
        r#"docs | wing ~ "ik" | { id, wing }"#,
        r#"docs | wing ~ "" | { id }"#,
        r#"docs | wing in ["rag", "wiki"] and title ~ "note" | { id }"#,
        r#"docs | wing ~ "zzz" | { id }"#,
    ];

    fn compare(cold: &mut Db, hot: &mut Db, stage: &str) {
        for q in QUERIES {
            let q = &unlimited(q);
            let c = cold.run(q).map(|h| h.rows.clone());
            let h = hot.run(q).map(|h| h.rows.clone());
            match (c, h) {
                (Ok(mut c), Ok(mut h)) => {
                    if q.contains("take") {
                        assert_eq!(c.len(), h.len(), "{stage}: {q}");
                    } else {
                        // Row order across storage tiers is not part of this contract.
                        c.sort_by_key(row_key);
                        h.sort_by_key(row_key);
                        assert_eq!(c, h, "{stage}: {q}");
                    }
                }
                (Err(_), Err(_)) => {}
                (c, h) => panic!("{stage}: {q}: cold {c:?} vs hot {h:?}"),
            }
        }
    }

    let cold_dir = tmp();
    let hot_dir = tmp();
    let cold_opts = || lin::OpenOpts {
        sync: lin::SyncMode::Full,
        cold: true,
    };
    {
        let mut db = Db::open_with(&cold_dir, cold_opts()).unwrap();
        seed(&mut db, 0, 60);
        db.checkpoint().unwrap();
        db.close().unwrap();
    }
    let mut cold = Db::open_with(&cold_dir, cold_opts()).unwrap();
    let mut hot = Db::open(&hot_dir).unwrap();
    seed(&mut hot, 0, 60);
    assert!(cold.store.is_disk_backed("docs"));
    assert!(cold.store.is_disk_backed("orders"));
    assert!(!hot.store.is_disk_backed("docs"));

    compare(&mut cold, &mut hot, "segments only");
    assert!(cold.store.is_disk_backed("docs"));
    assert!(cold.store.is_disk_backed("orders"));

    // Rows written after the checkpoint live outside the immutable segments.
    seed(&mut cold, 60, 75);
    seed(&mut hot, 60, 75);
    compare(&mut cold, &mut hot, "segments + delta");

    cold.close().unwrap();
    hot.close().unwrap();
    let _ = fs::remove_dir_all(&cold_dir);
    let _ = fs::remove_dir_all(&hot_dir);
}

/// Zone-map block skipping in the filtered disk scan must not drop or invent rows:
/// several 4096-row blocks with monotonic values, compared against the in-memory executor.
#[test]
fn cold_filter_zone_skipping_matches_in_memory() {
    const N: usize = 9000;
    fn seed(db: &mut Db) {
        for i in 0..5 {
            db.run(&format!(
                r#"insert users {{ id: "u{i}", email: "user{i}@example.com" }}"#
            ))
            .unwrap();
        }
        for chunk in 0..(N / 1000) {
            let mut orders = String::from("insert orders [\n");
            for i in (chunk * 1000)..((chunk + 1) * 1000) {
                if i > chunk * 1000 {
                    orders.push(',');
                }
                orders.push_str(&format!(
                    r#"{{ id: "o{i}", user_id: "u{}", total: {i}, ts: ago 1d }}"#,
                    i % 5
                ));
            }
            orders.push_str("\n]");
            db.run(&orders).unwrap();
        }
    }
    let cold_dir = tmp();
    let hot_dir = tmp();
    let cold_opts = || lin::OpenOpts {
        sync: lin::SyncMode::Full,
        cold: true,
    };
    {
        let mut db = Db::open_with(&cold_dir, cold_opts()).unwrap();
        seed(&mut db);
        db.checkpoint().unwrap();
        db.close().unwrap();
    }
    let mut cold = Db::open_with(&cold_dir, cold_opts()).unwrap();
    let mut hot = Db::open(&hot_dir).unwrap();
    seed(&mut hot);
    assert!(cold.store.is_disk_backed("orders"));
    for q in [
        r#"orders | total > 8000 | { id, total }"#,
        r#"orders | total >= 8191 | { id }"#,
        r#"orders | total < 4096 | count by user_id"#,
        r#"orders | total <= 100 | { id, user_id }"#,
        r#"orders | total > 4000 and total < 4200 | { id, total }"#,
        r#"orders | total > 4000 and total < 4200 | count by user_id"#,
        r#"orders | total > 4095 and total < 4097 | { id }"#,
        r#"orders | total > 100000 | { id }"#,
        r#"orders | total < 0 | count"#,
        r#"orders | total > 4000.5 | count"#,
        r#"orders | total > 4000 or total < 10 | { id }"#,
        r#"orders | not total > 10 | { id }"#,
    ] {
        let mut c = cold.run(&unlimited(q)).unwrap().rows.clone();
        let mut h = hot.run(&unlimited(q)).unwrap().rows.clone();
        c.sort_by_key(row_key);
        h.sort_by_key(row_key);
        assert_eq!(c.len(), h.len(), "{q}");
        assert_eq!(c, h, "{q}");
    }
    assert!(cold.store.is_disk_backed("orders"));

    // A cluster key can be declared through `Db`, is validated, and survives a reopen
    // (checked after the reopen further down).
    assert!(cold.set_cluster_key("orders", Some("no_such_column")).is_err());
    assert!(cold.set_cluster_key("no_such_table", Some("total")).is_err());
    assert_eq!(cold.cluster_key("orders"), None);
    cold.set_cluster_key("orders", Some("total")).unwrap();
    assert_eq!(cold.cluster_key("orders").as_deref(), Some("total"));
    assert!(hot.set_cluster_key("orders", Some("total")).is_err(), "no disk tier");

    // The columnar read path (`run_batch`) streams cells into typed columns; it must agree
    // with the in-memory batch, including limits and offsets.
    for q in [
        r#"orders | total > 8000 | { id, total } | take all"#,
        r#"orders | total > 4000 and total < 4200 | { id, total, user_id } | take all"#,
        r#"orders | total >= 8191 | { total, id } | take all"#,
        r#"orders | { id, total } | take all"#,
        r#"orders | total < 0 | { id } | take all"#,
        r#"orders | total > 100000 | { id, total } | take all"#,
    ] {
        let c = cold.run_batch(q).unwrap();
        let h = hot.run_batch(q).unwrap();
        assert_eq!(c.names, h.names, "{q}");
        let (mut cr, mut hr) = (c.to_rows(), h.to_rows());
        cr.sort_by_key(row_key);
        hr.sort_by_key(row_key);
        assert_eq!(cr.len(), hr.len(), "{q}");
        assert_eq!(
            cr.iter().map(row_key).collect::<Vec<_>>(),
            hr.iter().map(row_key).collect::<Vec<_>>(),
            "{q}"
        );
        if let Some(pos) = c.names.iter().position(|n| n == "total")
            && c.n() > 0
        {
            assert!(c.cols[pos].is_typed(), "{q}: total should be a dense column");
        }
    }
    for (q, want) in [
        (r#"orders | total > 8000 | { id } | take 7"#, 7),
        (r#"orders | total > 8000 | { id } | skip 990 | take 100"#, 9),
        (r#"orders | { id } | skip 5 | take 0"#, 0),
        (r#"orders | total > 8000 | { id }"#, 50),
    ] {
        assert_eq!(cold.run_batch(q).unwrap().n(), want, "{q}");
        assert_eq!(hot.run_batch(q).unwrap().n(), want, "{q}");
    }
    assert!(cold.store.is_disk_backed("orders"));
    // The key reaches the manifest with the next checkpoint.
    cold.checkpoint().unwrap();
    assert_eq!(cold.cluster_key("orders").as_deref(), Some("total"));
    assert!(cold.store.is_disk_backed("orders"));
    cold.close().unwrap();

    // Zone maps are persisted with the segment; a damaged file must be ignored (the engine
    // recomputes the statistics from the column), never trusted.
    fn zmaps(dir: &std::path::Path, out: &mut Vec<PathBuf>) {
        for e in fs::read_dir(dir).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                zmaps(&p, out);
            } else if p.extension().is_some_and(|x| x == "zmap") {
                out.push(p);
            }
        }
    }
    let mut files = Vec::new();
    zmaps(&cold_dir, &mut files);
    assert!(
        files.iter().any(|p| p.ends_with("col_total.zmap")),
        "expected a persisted zone map for orders.total, found {files:?}"
    );
    for p in &files {
        let mut bytes = fs::read(p).unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 0xFF;
        fs::write(p, bytes).unwrap();
    }
    let mut cold = Db::open_with(&cold_dir, cold_opts()).unwrap();
    assert_eq!(cold.cluster_key("orders").as_deref(), Some("total"));
    for q in [
        r#"orders | total > 8000 | { id, total }"#,
        r#"orders | total > 4000 and total < 4200 | count by user_id"#,
        r#"orders | total < 4096 | count"#,
    ] {
        let mut c = cold.run(&unlimited(q)).unwrap().rows.clone();
        let mut h = hot.run(&unlimited(q)).unwrap().rows.clone();
        c.sort_by_key(row_key);
        h.sort_by_key(row_key);
        assert_eq!(c, h, "corrupt zmap: {q}");
    }
    cold.close().unwrap();
    hot.close().unwrap();
    let _ = fs::remove_dir_all(&cold_dir);
    let _ = fs::remove_dir_all(&hot_dir);
}

/// Point `update` / `delete` on a cold collection must go through the disk overlay:
/// the collection stays disk-backed, changes are visible to every read path, and they
/// survive both WAL replay (no checkpoint) and checkpoint + reopen.
#[test]
fn cold_point_update_delete_stay_disk_backed() {
    let dir = tmp();
    let cold_opts = || lin::OpenOpts {
        sync: lin::SyncMode::Full,
        cold: true,
    };
    {
        let mut db = Db::open_with(&dir, cold_opts()).unwrap();
        let mut rows = String::from("insert docs [\n");
        for i in 0..64 {
            if i > 0 {
                rows.push(',');
            }
            let wing = if i % 2 == 0 { "even" } else { "odd" };
            rows.push_str(&format!(
                r#"{{ id: "p{i}", uri: "raw://p/{i}", title: "Title {i}", wing: "{wing}", layer: "wiki", body: "body {i}" }}"#
            ));
        }
        rows.push_str("\n]");
        db.run(&rows).unwrap();
        db.checkpoint().unwrap();
        db.close().unwrap();
    }

    let hash_of = |db: &mut Db, id: &str| -> String {
        let h = db
            .run(&format!(r#"docs | id == "{id}" | {{ hash }}"#))
            .unwrap();
        text(&h.rows[0], "hash").to_string()
    };
    let count = |db: &mut Db, q: &str| -> i64 { db.run(q).unwrap().scalar_as::<i64>().unwrap() };

    {
        let mut db = Db::open_with(&dir, cold_opts()).unwrap();
        assert!(db.store.is_disk_backed("docs"));
        // Warm the query caches so stale cached results would be observable below.
        assert_eq!(count(&mut db, r#"docs | wing == "even" | count"#), 32);
        assert_eq!(
            db.run(r#"docs | wing == "odd" | { id, title }"#).unwrap().rows.len(),
            32
        );

        // CAS mismatch is rejected and leaves the row untouched.
        let err = db
            .run(r#"update docs[id == "p3"] cas "nope" { title: "bad" }"#)
            .unwrap_err();
        assert!(err.to_string().contains("cas mismatch"), "{err}");

        let h3 = hash_of(&mut db, "p3");
        db.run(&format!(
            r#"update docs[id == "p3"] cas "{h3}" {{ title: "Changed 3", wing: "even" }}"#
        ))
        .unwrap();
        assert!(db.store.is_disk_backed("docs"), "update promoted docs");

        let h5 = hash_of(&mut db, "p5");
        db.run(&format!(r#"delete docs[id == "p5"] cas "{h5}""#))
            .unwrap();
        assert!(db.store.is_disk_backed("docs"), "delete promoted docs");

        let err = db
            .run(r#"delete docs[id == "p5"] cas "x""#)
            .unwrap_err();
        assert!(err.to_string().contains("no matching row"), "{err}");

        let check = |db: &mut Db, stage: &str| {
            assert_eq!(count(db, r#"docs | count"#), 63, "{stage}");
            assert_eq!(count(db, r#"docs | wing == "even" | count"#), 33, "{stage}");
            assert_eq!(count(db, r#"docs | wing == "odd" | count"#), 30, "{stage}");
            assert_eq!(count(db, r#"docs | title ~ "Changed" | count"#), 1, "{stage}");
            let h = db.run(r#"docs | id == "p3" | { title, wing }"#).unwrap();
            assert_eq!(text(&h.rows[0], "title"), "Changed 3", "{stage}");
            assert_eq!(text(&h.rows[0], "wing"), "even", "{stage}");
            assert_eq!(
                db.run(r#"docs | id == "p5""#).unwrap().rows.len(),
                0,
                "{stage}"
            );
            assert_eq!(
                db.run(r#"docs | wing == "odd" | { id, title }"#)
                    .unwrap()
                    .rows
                    .len(),
                30,
                "{stage}"
            );
            let grouped = db.run(r#"docs | count by wing"#).unwrap();
            assert_eq!(grouped.rows.len(), 2, "{stage}");
        };
        check(&mut db, "live overlay");
        assert!(db.store.is_disk_backed("docs"));

        // Predicate delete of several rows also stays on disk; then put the counts back
        // where `check` expects them by deleting nothing else.
        let err = db
            .run(r#"delete docs[title ~ "no such title"] cas each"#)
            .unwrap_err();
        assert!(err.to_string().contains("no matching row"), "{err}");
        let err = db
            .run(r#"delete docs[wing == "odd"] cas "nope""#)
            .unwrap_err();
        assert!(err.to_string().contains("cas mismatch"), "{err}");
        assert_eq!(count(&mut db, r#"docs | wing == "odd" | count"#), 30);
        assert!(db.store.is_disk_backed("docs"));
        // No checkpoint: the next open must rebuild the overlay from the WAL.
        db.close().unwrap();

        let mut db = Db::open_with(&dir, cold_opts()).unwrap();
        check(&mut db, "wal replay");
        db.checkpoint().unwrap();
        db.close().unwrap();

        let mut db = Db::open_with(&dir, cold_opts()).unwrap();
        check(&mut db, "checkpoint + reopen");
        assert!(db.store.is_disk_backed("docs"));

        // Predicate update of many rows: stays on disk, refreshes the content hash, and
        // keeps unrelated fields.
        let upd = db
            .run(r#"update docs[wing == "odd"] cas each { room: "inbox", title: "rewritten" }"#)
            .unwrap();
        assert_eq!(upd.done.n, 30);
        assert!(db.store.is_disk_backed("docs"), "predicate update promoted docs");
        assert_eq!(count(&mut db, r#"docs | room == "inbox" | count"#), 30);
        assert_eq!(count(&mut db, r#"docs | title ~ "rewritten" | count"#), 30);
        let h = db.run(r#"docs | id == "p7" | { title, room, uri }"#).unwrap();
        assert_eq!(text(&h.rows[0], "title"), "rewritten");
        assert_eq!(text(&h.rows[0], "room"), "inbox");
        assert_eq!(text(&h.rows[0], "uri"), "raw://p/7");
        let h9 = hash_of(&mut db, "p11");
        let err = db
            .run(r#"update docs[wing == "odd"] cas "stale" { room: "x" }"#)
            .unwrap_err();
        assert!(err.to_string().contains("cas mismatch"), "{err}");
        // Changing a unique key is still allowed, through the in-memory path.
        db.run(&format!(
            r#"update docs[id == "p11"] cas "{h9}" {{ uri: "raw://p/renamed" }}"#
        ))
        .unwrap();
        assert_eq!(count(&mut db, r#"docs | uri == "raw://p/renamed" | count"#), 1);
        db.checkpoint().unwrap();
        db.close().unwrap();
        let mut db = Db::open_with(&dir, cold_opts()).unwrap();
        assert!(db.store.is_disk_backed("docs"));
        assert_eq!(count(&mut db, r#"docs | room == "inbox" | count"#), 30);
        assert_eq!(count(&mut db, r#"docs | uri == "raw://p/renamed" | count"#), 1);

        let del = db.run(r#"delete docs[wing == "odd"] cas each"#).unwrap();
        assert_eq!(del.done.n, 30);
        assert_eq!(del.rows.len(), 30);
        assert!(db.store.is_disk_backed("docs"), "predicate delete promoted docs");
        assert_eq!(count(&mut db, r#"docs | count"#), 33);
        assert_eq!(count(&mut db, r#"docs | wing == "odd" | count"#), 0);
        assert_eq!(db.run(r#"docs | count by wing"#).unwrap().rows.len(), 1);
        db.close().unwrap();

        let mut db = Db::open_with(&dir, cold_opts()).unwrap();
        assert_eq!(count(&mut db, r#"docs | count"#), 33, "predicate delete replay");
        assert_eq!(count(&mut db, r#"docs | wing == "odd" | count"#), 0);
        db.close().unwrap();
    }
    let _ = fs::remove_dir_all(&dir);
}

/// Multi-segment cold table large enough for the per-segment parallel scan (`parallel`
/// feature): partial results merged in segment order must equal the in-memory executor.
#[test]
fn cold_multi_segment_aggregates_match_in_memory() {
    const N: usize = 70_000;
    fn seed(db: &mut Db, checkpoint_every: Option<usize>) {
        for i in 0..5 {
            db.run(&format!(
                r#"insert users {{ id: "u{i}", email: "user{i}@example.com" }}"#
            ))
            .unwrap();
        }
        for chunk in 0..(N / 5000) {
            let mut orders = String::from("insert orders [\n");
            for i in (chunk * 5000)..((chunk + 1) * 5000) {
                if i > chunk * 5000 {
                    orders.push(',');
                }
                orders.push_str(&format!(
                    r#"{{ id: "o{i}", user_id: "u{}", total: {}, ts: ago 1d }}"#,
                    i % 5,
                    (i * 13) % 1000
                ));
            }
            orders.push_str("\n]");
            db.run(&orders).unwrap();
            if let Some(every) = checkpoint_every
                && (chunk + 1) % every == 0
            {
                db.checkpoint().unwrap();
            }
        }
    }
    let cold_dir = tmp();
    let hot_dir = tmp();
    let cold_opts = || lin::OpenOpts {
        sync: lin::SyncMode::Full,
        cold: true,
    };
    {
        let mut db = Db::open_with(&cold_dir, cold_opts()).unwrap();
        seed(&mut db, Some(7));
        db.checkpoint().unwrap();
        db.close().unwrap();
    }
    let mut cold = Db::open_with(&cold_dir, cold_opts()).unwrap();
    let mut hot = Db::open(&hot_dir).unwrap();
    seed(&mut hot, None);
    assert!(cold.store.is_disk_backed("orders"));
    for q in [
        r#"orders | count"#,
        r#"orders | count by user_id"#,
        r#"orders | sum total"#,
        r#"orders | sum total by user_id"#,
        r#"orders | total > 500 | count"#,
        r#"orders | total > 500 and user_id == "u2" | count"#,
        r#"orders | total >= 990 | { id, total }"#,
        // Text equality on a dictionary-encoded column, alone and with a range.
        r#"orders | total >= 990 and user_id == "u2" | { id, total }"#,
        r#"orders | user_id == "u4" and total < 3 | { id, user_id }"#,
        r#"orders | user_id == "nobody" and total > 1 | { id }"#,
        r#"orders | total > 995 and user_id == "u1" and total < 999 | { id }"#,
        r#"orders | total > 990 and (user_id == "u1" or user_id == "u3") | { id }"#,
        // In-lists on a dictionary-encoded column.
        r#"orders | total > 990 and user_id in ["u1", "u3"] | { id, user_id }"#,
        r#"orders | user_id in ["u0"] and total < 2 | { id }"#,
        r#"orders | user_id in ["ghost", "phantom"] and total > 1 | { id }"#,
        r#"orders | not user_id in ["u1", "u2", "u3", "u4"] and total > 995 | { id }"#,
    ] {
        let mut c = cold.run(&unlimited(q)).unwrap().rows.clone();
        let mut h = hot.run(&unlimited(q)).unwrap().rows.clone();
        c.sort_by_key(row_key);
        h.sort_by_key(row_key);
        assert_eq!(c, h, "{q}");
    }
    assert!(cold.store.is_disk_backed("orders"));
    cold.close().unwrap();
    hot.close().unwrap();
    // Several checkpoints must have produced several `orders` segments, otherwise this
    // test would not cover the multi-segment merge at all.
    let order_segments = fs::read_dir(cold_dir.join("segments"))
        .map(|d| {
            d.flatten()
                .filter(|e| e.path().join("col_total.val").exists())
                .count()
        })
        .unwrap_or(0);
    assert!(
        order_segments > 1,
        "expected several orders segments, found {order_segments}"
    );
    let _ = fs::remove_dir_all(&cold_dir);
    let _ = fs::remove_dir_all(&hot_dir);
}

/// Inserts into a cold collection go to the disk overlay: uniqueness and foreign keys are
/// checked against the disk engine, the collection stays disk-backed, and the rows survive
/// WAL replay and checkpoint.
#[test]
fn cold_insert_stays_disk_backed() {
    let dir = tmp();
    let cold_opts = || lin::OpenOpts {
        sync: lin::SyncMode::Full,
        cold: true,
    };
    {
        let mut db = Db::open_with(&dir, cold_opts()).unwrap();
        for i in 0..40 {
            db.run(&format!(
                r#"insert users {{ id: "u{i}", email: "user{i}@example.com" }}"#
            ))
            .unwrap();
        }
        let mut docs = String::from("insert docs [\n");
        let mut orders = String::from("insert orders [\n");
        for i in 0..64 {
            if i > 0 {
                docs.push(',');
                orders.push(',');
            }
            docs.push_str(&format!(
                r#"{{ id: "c{i}", uri: "raw://c/{i}", title: "Seed {i}", wing: "seed", layer: "wiki", body: "seed body {i}" }}"#
            ));
            orders.push_str(&format!(
                r#"{{ id: "o{i}", user_id: "u{}", total: {i} }}"#,
                i % 40
            ));
        }
        docs.push_str("\n]");
        orders.push_str("\n]");
        db.run(&docs).unwrap();
        db.run(&orders).unwrap();
        db.checkpoint().unwrap();
        db.close().unwrap();
    }
    let count = |db: &mut Db, q: &str| -> i64 { db.run(q).unwrap().scalar_as::<i64>().unwrap() };

    let mut db = Db::open_with(&dir, cold_opts()).unwrap();
    for c in ["docs", "orders", "users"] {
        assert!(db.store.is_disk_backed(c), "{c} should start cold");
    }

    // Single row, explicit id.
    let ins = db
        .run(r#"insert docs { id: "n1", uri: "raw://n/1", title: "Fresh zebra", wing: "new", layer: "wiki", body: "fresh body" }"#)
        .unwrap();
    assert_eq!(ins.done.n, 1);
    assert_eq!(text(&ins.rows[0], "id"), "n1");
    assert!(!text(&ins.rows[0], "hash").is_empty());
    assert!(db.store.is_disk_backed("docs"), "insert promoted docs");

    // Generated id, then a small batch.
    let ins = db
        .run(r#"insert docs { uri: "raw://n/2", title: "Generated", wing: "new", layer: "wiki", body: "gen body" }"#)
        .unwrap();
    let gen_id = text(&ins.rows[0], "id").to_string();
    assert!(!gen_id.is_empty());
    let ins = db
        .run(
            r#"insert docs [
            { id: "n3", uri: "raw://n/3", title: "Batch a", wing: "new", layer: "wiki", body: "b3" },
            { id: "n4", uri: "raw://n/4", title: "Batch b", wing: "new", layer: "wiki", body: "b4" }
        ]"#,
        )
        .unwrap();
    assert_eq!(ins.done.n, 2);
    assert!(db.store.is_disk_backed("docs"), "batch insert promoted docs");

    // Uniqueness against segments, against the overlay, and inside one batch.
    for (src, want) in [
        (
            r#"insert docs { id: "c7", uri: "raw://n/x", title: "dup", layer: "wiki" }"#,
            "duplicate id",
        ),
        (
            r#"insert docs { id: "n1", uri: "raw://n/y", title: "dup", layer: "wiki" }"#,
            "duplicate id",
        ),
        (
            r#"insert docs { id: "z1", uri: "raw://c/9", title: "dup", layer: "wiki" }"#,
            "duplicate uri",
        ),
        (
            r#"insert docs { id: "z2", uri: "raw://n/3", title: "dup", layer: "wiki" }"#,
            "duplicate uri",
        ),
        (
            r#"insert docs [
                { id: "z3", uri: "raw://n/same", title: "dup", layer: "wiki" },
                { id: "z4", uri: "raw://n/same", title: "dup", layer: "wiki" }
            ]"#,
            "duplicate uri",
        ),
    ] {
        let res = db.run(src).map(|h| h.done.n);
        let Err(err) = res else {
            panic!("expected `{want}` for {src}, got {res:?}; disk-backed={}", db.store.is_disk_backed("docs"));
        };
        assert!(err.to_string().contains(want), "{src}: {err}");
    }
    assert!(db.store.is_disk_backed("docs"));

    // Foreign keys are resolved against the cold `users` collection.
    let err = db
        .run(r#"insert orders { id: "bad", user_id: "missing", total: 1 }"#)
        .unwrap_err();
    assert!(err.to_string().contains("fk"), "{err}");
    db.run(r#"insert orders { id: "good", user_id: "u3", total: 500 }"#)
        .unwrap();
    assert!(db.store.is_disk_backed("orders"), "insert promoted orders");
    assert!(db.store.is_disk_backed("users"), "fk check promoted users");
    // Whatever the in-memory executor decides about a repeated `users.email`, the cold
    // path must decide the same.
    {
        let mut hot = Db::fixture();
        hot.run(r#"insert users { id: "h1", email: "same@example.com" }"#)
            .unwrap();
        let hot_dup = hot
            .run(r#"insert users { id: "h2", email: "same@example.com" }"#)
            .is_err();
        let cold_dup = db
            .run(r#"insert users { id: "u-new", email: "user5@example.com" }"#)
            .is_err();
        assert_eq!(cold_dup, hot_dup, "users.email uniqueness differs");
        if !cold_dup {
            let h = db.run(r#"users | id == "u-new" | { id }"#).unwrap();
            assert_eq!(h.rows.len(), 1);
            db.run(r#"delete users[id == "u-new"] cas each"#).unwrap();
        }
    }
    assert!(db.store.is_disk_backed("users"));

    let check = |db: &mut Db, stage: &str| {
        assert_eq!(count(db, r#"docs | count"#), 68, "{stage}");
        assert_eq!(count(db, r#"docs | wing == "new" | count"#), 4, "{stage}");
        assert_eq!(count(db, r#"docs | title ~ "zebra" | count"#), 1, "{stage}");
        assert_eq!(count(db, r#"orders | count"#), 65, "{stage}");
        assert_eq!(count(db, r#"orders | total > 100 | count"#), 1, "{stage}");
        assert_eq!(count(db, r#"users | count"#), 40, "{stage}");
        let h = db.run(r#"docs | id == "n4" | { title }"#).unwrap();
        assert_eq!(text(&h.rows[0], "title"), "Batch b", "{stage}");
        let h = db
            .run(&format!(r#"docs | id == "{gen_id}" | {{ title }}"#))
            .unwrap();
        assert_eq!(text(&h.rows[0], "title"), "Generated", "{stage}");
    };
    check(&mut db, "live overlay");
    db.close().unwrap();

    let mut db = Db::open_with(&dir, cold_opts()).unwrap();
    check(&mut db, "wal replay");
    // A generated id must not be handed out twice after replay.
    let ins = db
        .run(r#"insert docs { uri: "raw://n/5", title: "After replay", wing: "late", layer: "wiki" }"#)
        .unwrap();
    assert_ne!(text(&ins.rows[0], "id"), gen_id);
    db.checkpoint().unwrap();
    db.close().unwrap();

    let mut db = Db::open_with(&dir, cold_opts()).unwrap();
    assert_eq!(count(&mut db, r#"docs | count"#), 69);
    assert_eq!(count(&mut db, r#"docs | wing == "new" | count"#), 4);
    assert_eq!(count(&mut db, r#"orders | count"#), 65);
    assert!(db.store.is_disk_backed("docs"));
    db.close().unwrap();
    let _ = fs::remove_dir_all(&dir);
}

/// FK join through `run_batch` with cold sides must match the in-memory join and must not
/// promote either collection.
#[test]
fn cold_join_batch_matches_in_memory() {
    fn seed(db: &mut Db) {
        let mut users = String::from("insert users [\n");
        for i in 0..50 {
            if i > 0 {
                users.push(',');
            }
            users.push_str(&format!(
                r#"{{ id: "u{i}", email: "user{i}@example.com" }}"#
            ));
        }
        users.push_str("\n]");
        db.run(&users).unwrap();
        let mut orders = String::from("insert orders [\n");
        for i in 0..400 {
            if i > 0 {
                orders.push(',');
            }
            orders.push_str(&format!(
                r#"{{ id: "o{i}", user_id: "u{}", total: {} }}"#,
                (i * 7) % 50,
                (i * 13) % 300
            ));
        }
        orders.push_str("\n]");
        db.run(&orders).unwrap();
    }
    let cold_dir = tmp();
    let hot_dir = tmp();
    let cold_opts = || lin::OpenOpts {
        sync: lin::SyncMode::Full,
        cold: true,
    };
    {
        let mut db = Db::open_with(&cold_dir, cold_opts()).unwrap();
        seed(&mut db);
        db.checkpoint().unwrap();
        db.close().unwrap();
    }
    let mut cold = Db::open_with(&cold_dir, cold_opts()).unwrap();
    let mut hot = Db::open(&hot_dir).unwrap();
    seed(&mut hot);
    assert!(cold.store.is_disk_backed("orders"));
    assert!(cold.store.is_disk_backed("users"));

    const QUERIES: &[&str] = &[
        r#"orders | join users on user_id | { id, users.email, total } | take all"#,
        r#"orders | total > 100 | join users on user_id | { id, users.email, total } | take all"#,
        r#"orders | total > 100 and user_id == "u7" | join users on user_id | { id, users.email } | take all"#,
        r#"orders | total < 0 | join users on user_id | { id, users.email } | take all"#,
        r#"orders | join users on user_id | { users.email, id } | skip 10 | take 25"#,
    ];
    // The tiers differ in representation (`Int` vs `Float` for `total`, inline vs shared
    // text), which is not what this test is about: compare values.
    fn norm(rows: Vec<lin::Row>) -> Vec<String> {
        let mut out: Vec<String> = rows
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
    let compare = |cold: &mut Db, hot: &mut Db, stage: &str| {
        for q in QUERIES {
            let c = norm(cold.run_batch(q).unwrap().to_rows());
            let h = norm(hot.run_batch(q).unwrap().to_rows());
            assert_eq!(c.len(), h.len(), "{stage}: {q}");
            if !q.contains("skip") {
                assert_eq!(c, h, "{stage}: {q}");
            }
            // The row API takes the same disk path for `join | project`.
            let cr = norm(cold.run(q).unwrap().rows.clone());
            assert_eq!(cr.len(), c.len(), "{stage}: run {q}");
            if !q.contains("skip") {
                assert_eq!(cr, c, "{stage}: run {q}");
            }
        }
    };
    compare(&mut cold, &mut hot, "segments");
    assert!(cold.store.is_disk_backed("orders"), "join promoted orders");
    assert!(cold.store.is_disk_backed("users"), "join promoted users");

    // Row lookups by id on a cold collection (used by the join paths that do not go
    // through the batch executor) must find rows, not consult empty in-memory maps.
    assert!(cold.store.get_by_id("users", "u7").is_some());
    assert!(cold.store.get_by_id("users", "nobody").is_none());
    assert_eq!(
        cold.store
            .get_by_id("orders", "o12")
            .and_then(|r| r.get("user_id"))
            .and_then(|c| c.text()),
        Some("u34")
    );
    for q in [
        r#"orders | total > 250 | join users on user_id | take all"#,
        r#"orders | join users on user_id | total > 290 | take all"#,
    ] {
        let c = norm(cold.run(q).unwrap().rows.clone());
        let h = norm(hot.run(q).unwrap().rows.clone());
        assert!(!h.is_empty(), "{q}");
        assert_eq!(c, h, "non-projected join: {q}");
    }
    assert!(cold.store.is_disk_backed("orders"));
    assert!(cold.store.is_disk_backed("users"));

    // Overlay rows on both sides, including an order whose user is added later.
    for db in [&mut cold, &mut hot] {
        db.run(r#"insert users { id: "u-late", email: "late@example.com" }"#)
            .unwrap();
        db.run(r#"insert orders { id: "o-late", user_id: "u-late", total: 250 }"#)
            .unwrap();
        db.run(r#"delete orders[id == "o3"] cas each"#).unwrap();
    }
    compare(&mut cold, &mut hot, "segments + overlay");
    assert!(cold.store.is_disk_backed("orders"));
    assert!(cold.store.is_disk_backed("users"));

    cold.close().unwrap();
    hot.close().unwrap();
    let _ = fs::remove_dir_all(&cold_dir);
    let _ = fs::remove_dir_all(&hot_dir);
}

/// Concurrent `AsyncDb::execute` calls share WAL syncs (group commit). Every statement must
/// still succeed or fail on its own, and everything acknowledged must survive a reopen
/// without a checkpoint.
#[cfg(feature = "async")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn async_group_commit_is_durable_and_per_statement() {
    let dir = tmp();
    let db = Db::open_with(
        &dir,
        lin::OpenOpts {
            sync: lin::SyncMode::Full,
            cold: false,
        },
    )
    .unwrap();
    let adb = lin::AsyncDb::new(db);
    adb.execute(r#"insert docs { id: "seed", uri: "raw://g/seed", title: "seed", layer: "wiki" }"#)
        .await
        .unwrap();

    const N: usize = 48;
    let mut tasks = Vec::new();
    for i in 0..N {
        let adb = adb.clone();
        tasks.push(tokio::spawn(async move {
            // Every sixth statement collides with the seed row and must fail alone.
            let src = if i % 6 == 5 {
                format!(r#"insert docs {{ id: "seed", uri: "raw://g/dup{i}", title: "dup", layer: "wiki" }}"#)
            } else {
                format!(r#"insert docs {{ id: "g{i}", uri: "raw://g/{i}", title: "group {i}", layer: "wiki" }}"#)
            };
            (i, adb.execute(src).await)
        }));
    }
    let mut ok = 0usize;
    let mut failed = 0usize;
    let mut gens = Vec::new();
    for t in tasks {
        let (i, res) = t.await.unwrap();
        match res {
            Ok(done) => {
                assert_ne!(i % 6, 5, "duplicate insert {i} was acknowledged");
                assert_eq!(done.n, 1);
                gens.push(done.r#gen);
                ok += 1;
            }
            Err(e) => {
                assert_eq!(i % 6, 5, "insert {i} failed: {e}");
                assert!(e.to_string().contains("duplicate id"), "{e}");
                failed += 1;
            }
        }
    }
    assert_eq!(ok, N - N / 6);
    assert_eq!(failed, N / 6);

    // Prepared writes join the same commit groups.
    let mut prepared_tasks = Vec::new();
    for i in 0..16 {
        let adb = adb.clone();
        prepared_tasks.push(tokio::spawn(async move {
            let p = adb
                .prepare(format!(
                    r#"insert docs {{ id: "p{i}", uri: "raw://g/p{i}", title: "prepared {i}", layer: "wiki" }}"#
                ))
                .await
                .unwrap();
            adb.execute_prepared(p).await
        }));
    }
    for t in prepared_tasks {
        let done = t.await.unwrap().unwrap();
        assert_eq!(done.n, 1);
        gens.push(done.r#gen);
        ok += 1;
    }
    gens.sort_unstable();
    gens.dedup();
    assert_eq!(gens.len(), ok, "each committed statement gets its own generation");

    // Drop without close or checkpoint: only the WAL carries the writes.
    drop(adb);
    let mut db = Db::open(&dir).unwrap();
    assert_eq!(
        db.run(r#"docs | count"#).unwrap().scalar_as::<i64>().unwrap(),
        (ok + 1) as i64
    );
    assert_eq!(db.run(r#"docs | id == "g7""#).unwrap().rows.len(), 1);
    assert_eq!(db.run(r#"docs | id == "p9""#).unwrap().rows.len(), 1);
    assert_eq!(db.run(r#"docs | uri == "raw://g/dup5""#).unwrap().rows.len(), 0);
    db.close().unwrap();
    let _ = fs::remove_dir_all(&dir);
}

/// A cluster key survives the collection being promoted into memory and spilled again,
/// and the re-spilled data still answers queries like the in-memory executor.
#[test]
fn cold_cluster_key_survives_promotion_and_respill() {
    fn orders(from: usize, to: usize) -> String {
        let mut s = String::from("insert orders [\n");
        for i in from..to {
            if i > from {
                s.push(',');
            }
            s.push_str(&format!(
                r#"{{ id: "o{i}", user_id: "u{}", total: {} }}"#,
                i % 5,
                (i * 7919) % 10_000
            ));
        }
        s.push_str("\n]");
        s
    }
    fn seed(db: &mut Db) {
        for i in 0..5 {
            db.run(&format!(
                r#"insert users {{ id: "u{i}", email: "user{i}@example.com" }}"#
            ))
            .unwrap();
        }
        db.run(&orders(0, 6_000)).unwrap();
    }
    let cold_dir = tmp();
    let hot_dir = tmp();
    let cold_opts = || lin::OpenOpts {
        sync: lin::SyncMode::Full,
        cold: true,
    };
    {
        let mut db = Db::open_with(&cold_dir, cold_opts()).unwrap();
        seed(&mut db);
        db.checkpoint().unwrap();
        db.close().unwrap();
    }
    let mut hot = Db::open(&hot_dir).unwrap();
    seed(&mut hot);

    let mut cold = Db::open_with(&cold_dir, cold_opts()).unwrap();
    cold.set_cluster_key("orders", Some("total")).unwrap();
    // A batch above the overlay limit goes through the in-memory path and promotes orders.
    for db in [&mut cold, &mut hot] {
        db.run(&orders(6_000, 11_000)).unwrap();
    }
    assert!(!cold.store.is_disk_backed("orders"), "large batch should promote");
    cold.checkpoint().unwrap();
    // The spilled copy is attached again on the next open; the key is remembered meanwhile.
    assert_eq!(cold.cluster_key("orders").as_deref(), Some("total"));
    cold.close().unwrap();

    let mut cold = Db::open_with(&cold_dir, cold_opts()).unwrap();
    assert!(cold.store.is_disk_backed("orders"));
    assert_eq!(cold.cluster_key("orders").as_deref(), Some("total"));
    for q in [
        r#"orders | count"#,
        r#"orders | total >= 5000 and total < 5100 | { id, total }"#,
        r#"orders | total < 50 | count by user_id"#,
        r#"orders | sum total by user_id"#,
    ] {
        let mut c = cold.run(&unlimited(q)).unwrap().rows.clone();
        let mut h = hot.run(&unlimited(q)).unwrap().rows.clone();
        c.sort_by_key(row_key);
        h.sort_by_key(row_key);
        assert_eq!(c, h, "{q}");
    }
    // Clearing the key is persisted too.
    cold.set_cluster_key("orders", None).unwrap();
    assert_eq!(cold.cluster_key("orders"), None);
    cold.checkpoint().unwrap();
    cold.close().unwrap();
    let cold = Db::open_with(&cold_dir, cold_opts()).unwrap();
    assert_eq!(cold.cluster_key("orders"), None);
    drop(cold);
    hot.close().unwrap();
    let _ = fs::remove_dir_all(&cold_dir);
    let _ = fs::remove_dir_all(&hot_dir);
}

/// A program of several statements on cold collections is atomic and stays on disk:
/// all of it is applied and survives replay, or none of it is.
#[test]
fn cold_multi_statement_program_is_atomic() {
    let dir = tmp();
    let cold_opts = || lin::OpenOpts {
        sync: lin::SyncMode::Full,
        cold: true,
    };
    {
        let mut db = Db::open_with(&dir, cold_opts()).unwrap();
        let mut rows = String::from("insert docs [\n");
        for i in 0..64 {
            if i > 0 {
                rows.push(',');
            }
            rows.push_str(&format!(
                r#"{{ id: "m{i}", uri: "raw://m/{i}", title: "Seed {i}", wing: "seed", layer: "wiki", body: "body {i}" }}"#
            ));
        }
        rows.push_str("\n]");
        db.run(&rows).unwrap();
        db.checkpoint().unwrap();
        db.close().unwrap();
    }
    let count = |db: &mut Db, q: &str| -> i64 { db.run(q).unwrap().scalar_as::<i64>().unwrap() };
    let hash_of = |db: &mut Db, id: &str| -> String {
        let h = db
            .run(&format!(r#"docs | id == "{id}" | {{ hash }}"#))
            .unwrap();
        text(&h.rows[0], "hash").to_string()
    };

    let mut db = Db::open_with(&dir, cold_opts()).unwrap();
    assert!(db.store.is_disk_backed("docs"));
    // Full-text search must track the overlay: through a rolled-back program, a committed
    // one, replay and checkpoint.
    let lex = |db: &mut Db, word: &str| -> usize {
        db.run(&format!(r#"docs | search lex "{word}" | take all"#))
            .unwrap()
            .rows
            .len()
    };
    assert_eq!(lex(&mut db, "seed"), 64);
    assert_eq!(lex(&mut db, "changed"), 0);
    let (h1, h2, h3) = (
        hash_of(&mut db, "m1"),
        hash_of(&mut db, "m2"),
        hash_of(&mut db, "m3"),
    );
    let gen0 = db.store.r#gen;

    // Third statement fails (stale cas): nothing from the program may remain.
    let err = db
        .run(&format!(
            r#"insert docs {{ id: "new1", uri: "raw://m/new1", title: "New one", wing: "prog", layer: "wiki" }}
update docs[id == "m1"] cas "{h1}" {{ title: "Changed 1" }}
delete docs[id == "m2"] cas "stale""#
        ))
        .unwrap_err();
    assert!(err.to_string().contains("cas mismatch"), "{err}");
    assert_eq!(db.store.r#gen, gen0, "failed program advanced the generation");
    assert!(db.store.is_disk_backed("docs"), "failed program promoted docs");
    assert_eq!(count(&mut db, r#"docs | count"#), 64);
    assert_eq!(db.run(r#"docs | id == "new1""#).unwrap().rows.len(), 0);
    assert_eq!(count(&mut db, r#"docs | title ~ "Changed" | count"#), 0);
    assert_eq!(count(&mut db, r#"docs | wing == "prog" | count"#), 0);
    assert_eq!(hash_of(&mut db, "m1"), h1);
    assert_eq!(lex(&mut db, "seed"), 64, "fts after rollback");
    assert_eq!(lex(&mut db, "changed"), 0, "fts after rollback");
    assert!(db.store.is_disk_backed("docs"), "search promoted docs");

    // A duplicate inside the program (second insert repeats the first id) also rolls back.
    let err = db
        .run(
            r#"insert docs { id: "dup1", uri: "raw://m/dup-a", title: "a", layer: "wiki" }
insert docs { id: "dup1", uri: "raw://m/dup-b", title: "b", layer: "wiki" }"#,
        )
        .unwrap_err();
    assert!(err.to_string().contains("duplicate id"), "{err}");
    assert_eq!(count(&mut db, r#"docs | count"#), 64);
    assert_eq!(db.run(r#"docs | id == "dup1""#).unwrap().rows.len(), 0);

    // Success: later statements see earlier ones, one generation for the whole program.
    let done = db
        .run(&format!(
            r#"insert docs {{ id: "new1", uri: "raw://m/new1", title: "New one", wing: "prog", layer: "wiki" }}
update docs[id == "m1"] cas "{h1}" {{ title: "Changed 1", wing: "prog" }}
delete docs[id == "m2"] cas "{h2}"
update docs[id == "m3"] cas "{h3}" {{ wing: "prog" }}"#
        ))
        .unwrap();
    assert_eq!(done.done.r#gen, gen0 + 1);
    assert!(db.store.is_disk_backed("docs"), "program promoted docs");
    let check = |db: &mut Db, stage: &str| {
        assert_eq!(count(db, r#"docs | count"#), 64, "{stage}");
        assert_eq!(count(db, r#"docs | wing == "prog" | count"#), 3, "{stage}");
        assert_eq!(count(db, r#"docs | title ~ "Changed" | count"#), 1, "{stage}");
        assert_eq!(db.run(r#"docs | id == "m2""#).unwrap().rows.len(), 0, "{stage}");
        assert_eq!(db.run(r#"docs | id == "new1""#).unwrap().rows.len(), 1, "{stage}");
    };
    check(&mut db, "live");
    // "Seed 1" became "Changed 1", "Seed 2" is gone, "New one" has neither word.
    assert_eq!(lex(&mut db, "seed"), 62, "fts live");
    assert_eq!(lex(&mut db, "changed"), 1, "fts live");
    assert!(db.store.is_disk_backed("docs"), "search promoted docs");
    db.close().unwrap();

    let mut db = Db::open_with(&dir, cold_opts()).unwrap();
    check(&mut db, "wal replay");
    assert_eq!(lex(&mut db, "seed"), 62, "fts after replay");
    assert_eq!(lex(&mut db, "changed"), 1, "fts after replay");
    db.checkpoint().unwrap();
    db.close().unwrap();
    let mut db = Db::open_with(&dir, cold_opts()).unwrap();
    check(&mut db, "checkpoint + reopen");
    assert_eq!(lex(&mut db, "seed"), 62, "fts after checkpoint");
    assert_eq!(lex(&mut db, "changed"), 1, "fts after checkpoint");
    assert!(db.store.is_disk_backed("docs"));
    db.close().unwrap();
    let _ = fs::remove_dir_all(&dir);
}

/// `Db::compact_collection` on a clustered cold collection reclaims deleted rows and leaves
/// the table ordered by its cluster key across all segments, without changing its contents.
#[test]
fn cold_compact_collection_orders_by_cluster_key() {
    const N: usize = 9_000;
    fn seed(db: &mut Db) {
        for i in 0..5 {
            db.run(&format!(
                r#"insert users {{ id: "u{i}", email: "user{i}@example.com" }}"#
            ))
            .unwrap();
        }
        for chunk in 0..(N / 1000) {
            let mut orders = String::from("insert orders [\n");
            for i in (chunk * 1000)..((chunk + 1) * 1000) {
                if i > chunk * 1000 {
                    orders.push(',');
                }
                orders.push_str(&format!(
                    r#"{{ id: "o{i}", user_id: "u{}", total: {} }}"#,
                    i % 5,
                    (i * 7919) % 100_000
                ));
            }
            orders.push_str("\n]");
            db.run(&orders).unwrap();
        }
    }
    let cold_dir = tmp();
    let hot_dir = tmp();
    let cold_opts = || lin::OpenOpts {
        sync: lin::SyncMode::Full,
        cold: true,
    };
    {
        let mut db = Db::open_with(&cold_dir, cold_opts()).unwrap();
        seed(&mut db);
        db.checkpoint().unwrap();
        db.close().unwrap();
    }
    let mut hot = Db::open(&hot_dir).unwrap();
    seed(&mut hot);
    let mut cold = Db::open_with(&cold_dir, cold_opts()).unwrap();
    assert!(hot.compact_collection("orders").is_err(), "nothing on disk to compact");

    // Overlay changes (small inserts, deletes) leave extra segments and tombstones behind.
    for db in [&mut cold, &mut hot] {
        db.run(r#"delete orders[total < 1000] cas each"#).unwrap();
        db.run(r#"insert orders { id: "late-a", user_id: "u1", total: 5 }"#)
            .unwrap();
        db.run(r#"insert orders { id: "late-b", user_id: "u2", total: 99999 }"#)
            .unwrap();
    }
    cold.set_cluster_key("orders", Some("total")).unwrap();
    cold.compact_collection("orders").unwrap();
    assert!(cold.store.is_disk_backed("orders"));
    let queries = [
        r#"orders | count"#,
        r#"orders | total < 2000 | { id, total }"#,
        r#"orders | total >= 50000 and total < 50500 | { id, total }"#,
        r#"orders | sum total by user_id"#,
        r#"orders | id == "late-b" | { total }"#,
    ];
    let compare = |cold: &mut Db, hot: &mut Db, stage: &str| {
        for q in queries {
            let mut c = cold.run(&unlimited(q)).unwrap().rows.clone();
            let mut h = hot.run(&unlimited(q)).unwrap().rows.clone();
            c.sort_by_key(row_key);
            h.sort_by_key(row_key);
            assert_eq!(c, h, "{stage}: {q}");
        }
    };
    compare(&mut cold, &mut hot, "after compaction");
    cold.close().unwrap();

    // Physical order: one pass over the table sees `total` ascending.
    {
        let eng = lin::DiskEngine::open_read_only(&cold_dir).unwrap();
        let mut prev = f64::NEG_INFINITY;
        let mut n = 0usize;
        eng.scan_projected("orders", Some(&["total"]), |row| {
            let v = row.get("total").and_then(|c| c.as_f64()).unwrap();
            assert!(prev <= v, "orders not in cluster-key order: {prev} then {v}");
            prev = v;
            n += 1;
            Ok(true)
        })
        .unwrap();
        assert_eq!(n as i64, hot.run(r#"orders | count"#).unwrap().scalar_as::<i64>().unwrap());
    }

    let mut cold = Db::open_with(&cold_dir, cold_opts()).unwrap();
    assert_eq!(cold.cluster_key("orders").as_deref(), Some("total"));
    compare(&mut cold, &mut hot, "after reopen");
    cold.close().unwrap();
    hot.close().unwrap();
    let _ = fs::remove_dir_all(&cold_dir);
    let _ = fs::remove_dir_all(&hot_dir);
}
