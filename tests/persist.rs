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
            r.insert("id", lin::Cell::from_text(&format!("nd-{i}")));
            r.insert("uri", lin::Cell::from_text(&format!("raw://nd/{i}")));
            r.insert("title", lin::Cell::from_text(&format!("Native {i}")));
            r.insert(
                "wing",
                lin::Cell::from_text(if i % 2 == 0 { "even" } else { "odd" }),
            );
            r.insert("layer", lin::Cell::from_text("wiki"));
            r.insert("hash", lin::Cell::from_text(&format!("hash-{i}")));
            r.insert("body", lin::Cell::from_text(&body));
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
        // Zone maps prune under `or`: a block is skipped only when every branch rules it out.
        r#"orders | total < 50 or total > 8950 | { id, total }"#,
        r#"orders | total == 4096 or total == 8191 or total == 0 | { id }"#,
        r#"orders | (total > 100 and total < 120) or (total >= 8100 and total < 8110) | { id, total }"#,
        r#"orders | (total < 50 or total > 8950) and total != 20 | count"#,
        r#"orders | total < 50 or user_id == "u3" | count"#,
        r#"orders | total < -5 or total > 100000 | count"#,
        r#"orders | not (total < 8990) or total == 4095 | { id }"#,
        r#"orders | total > 4000 or total < 10 | { id }"#,
        r#"orders | not total > 10 | { id }"#,
        // Whole rows (no projection): only matching rows are materialized.
        r#"orders | total > 8990 | take all"#,
        r#"orders | total >= 4095 and total <= 4097 | take all"#,
        r#"orders | total > 100000 | take all"#,
        r#"orders | user_id == "u3" and total < 20 | take all"#,
        r#"orders | total < 5 or total > 8995 | take all"#,
    ] {
        let mut c = cold.run(&unlimited(q)).unwrap().rows.clone();
        let mut h = hot.run(&unlimited(q)).unwrap().rows.clone();
        // `ts` was seeded as `ago 1d` at two different instants; it is not under test.
        for r in c.iter_mut().chain(h.iter_mut()) {
            r.remove("ts");
        }
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
        // Changing a single-column unique key is checked against the disk engine, so the
        // collection stays on disk; collisions are rejected.
        db.run(&format!(
            r#"update docs[id == "p11"] cas "{h9}" {{ uri: "raw://p/renamed" }}"#
        ))
        .unwrap();
        assert!(db.store.is_disk_backed("docs"), "unique-key update promoted docs");
        assert_eq!(count(&mut db, r#"docs | uri == "raw://p/renamed" | count"#), 1);
        assert_eq!(count(&mut db, r#"docs | uri == "raw://p/11" | count"#), 0);
        for (src, why) in [
            // taken by a row in a segment
            (r#"update docs[id == "p13"] cas each { uri: "raw://p/20" }"#, "segment"),
            // taken by the row renamed just above (lives in the overlay)
            (r#"update docs[id == "p13"] cas each { uri: "raw://p/renamed" }"#, "overlay"),
            // two rows of one statement would end up with the same key
            (r#"update docs[id == "p13" or id == "p15"] cas each { uri: "raw://p/shared" }"#, "statement"),
        ] {
            let err = db.run(src).unwrap_err();
            assert!(err.to_string().contains("duplicate uri"), "{why}: {err}");
        }
        assert_eq!(count(&mut db, r#"docs | uri == "raw://p/13" | count"#), 1);
        assert_eq!(count(&mut db, r#"docs | uri == "raw://p/shared" | count"#), 0);
        // Re-assigning a row its own key is not a collision; a freed key can be reused.
        db.run(r#"update docs[id == "p13"] cas each { uri: "raw://p/13", room: "same" }"#)
            .unwrap();
        db.run(r#"update docs[id == "p15"] cas each { uri: "raw://p/11" }"#)
            .unwrap();
        assert_eq!(count(&mut db, r#"docs | uri == "raw://p/11" | count"#), 1);
        assert!(db.store.is_disk_backed("docs"));
        db.checkpoint().unwrap();
        db.close().unwrap();
        let mut db = Db::open_with(&dir, cold_opts()).unwrap();
        assert!(db.store.is_disk_backed("docs"));
        assert_eq!(count(&mut db, r#"docs | room == "inbox" | count"#), 29);
        assert_eq!(count(&mut db, r#"docs | room == "same" | count"#), 1);
        assert_eq!(count(&mut db, r#"docs | uri == "raw://p/renamed" | count"#), 1);
        assert_eq!(count(&mut db, r#"docs | uri == "raw://p/11" | count"#), 1);

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

    // A batch of several thousand rows also stays in the overlay.
    {
        let mut big = String::from("insert orders [\n");
        for i in 0..6_000 {
            if i > 0 {
                big.push(',');
            }
            big.push_str(&format!(
                r#"{{ id: "bulk{i}", user_id: "u{}", total: 424242 }}"#,
                i % 40
            ));
        }
        big.push_str("\n]");
        assert_eq!(db.run(&big).unwrap().done.n, 6_000);
        assert!(db.store.is_disk_backed("orders"), "6k-row insert promoted orders");
        assert_eq!(count(&mut db, r#"orders | total == 424242 | count"#), 6_000);
        // ...and a repeat of it is rejected as a whole.
        let err = db.run(&big).unwrap_err();
        assert!(err.to_string().contains("duplicate id"), "{err}");
        assert_eq!(count(&mut db, r#"orders | total == 424242 | count"#), 6_000);
        db.run(r#"delete orders[total == 424242] cas each"#).unwrap();
        assert_eq!(count(&mut db, r#"orders | total == 424242 | count"#), 0);
        assert!(db.store.is_disk_backed("orders"));
    }

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
        // No projection after the join: the row join probes the cold right side by id.
        r#"orders | join users on user_id | take all"#,
        r#"orders | total > 100 | join users on user_id | take all"#,
        r#"orders | user_id == "u7" | join users on user_id | total > 5 | take all"#,
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

    // `run` (which returns rows) and reads through the writer share the same groups; every
    // caller must get its own statement's result back, not a neighbour's.
    let mut run_tasks = Vec::new();
    for i in 0..24 {
        let adb = adb.clone();
        run_tasks.push(tokio::spawn(async move {
            if i % 3 == 0 {
                let h = adb
                    .run(r#"docs | id == "seed" | { id }"#)
                    .await
                    .unwrap();
                (i, false, h)
            } else {
                let h = adb
                    .run(format!(
                        r#"insert docs {{ id: "r{i}", uri: "raw://g/r{i}", title: "run {i}", layer: "wiki" }}"#
                    ))
                    .await
                    .unwrap();
                (i, true, h)
            }
        }));
    }
    for t in run_tasks {
        let (i, wrote, h) = t.await.unwrap();
        assert_eq!(h.rows.len(), 1, "run {i}");
        let want = if wrote { format!("r{i}") } else { "seed".to_string() };
        assert_eq!(text(&h.rows[0], "id"), want, "run {i} got another statement's rows");
        if wrote {
            gens.push(h.done.r#gen);
            ok += 1;
        }
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
    assert_eq!(db.run(r#"docs | id == "r22""#).unwrap().rows.len(), 1);
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
    for db in [&mut cold, &mut hot] {
        db.run(&orders(6_000, 11_000)).unwrap();
    }
    // Force the in-memory route (as an HNSW build or a primary-key change would), so the
    // next checkpoint spills the whole collection again.
    cold.store.promote_disk_col("orders");
    assert!(!cold.store.is_disk_backed("orders"));
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

#[test]
fn cold_transaction_commit_and_rollback_stay_disk_backed() {
    fn copy_dir_no_lock(src: &std::path::Path, dst: &std::path::Path) {
        fs::create_dir_all(dst).unwrap();
        for entry in fs::read_dir(src).unwrap() {
            let entry = entry.unwrap();
            let name = entry.file_name();
            if name.to_string_lossy().ends_with(".lock") {
                continue;
            }
            let src_path = entry.path();
            let dst_path = dst.join(&name);
            if src_path.is_dir() {
                copy_dir_no_lock(&src_path, &dst_path);
            } else {
                fs::copy(&src_path, &dst_path).unwrap();
            }
        }
    }

    let cold_opts = || lin::OpenOpts {
        sync: lin::SyncMode::Full,
        cold: true,
    };
    let dir = tmp();
    let total_seed = 64usize;
    {
        let mut db = Db::open_with(&dir, cold_opts()).unwrap();
        let mut src = String::from("insert docs [\n");
        for i in 0..total_seed {
            let wing = if i % 2 == 0 { "even" } else { "odd" };
            src.push_str(&format!(
                "  {{ id: \"t{i}\", uri: \"raw://tx/{i}\", title: \"seed {i}\", wing: \"{wing}\", layer: \"wiki\", body: \"txbase body {i}\" }},\n"
            ));
        }
        src.push(']');
        db.run(&src).unwrap();
        db.checkpoint().unwrap();
        db.close().unwrap();
    }

    let mut db = Db::open_with(&dir, cold_opts()).unwrap();
    assert!(db.store.is_disk_backed("docs"), "docs should reopen cold");

    let update_prep = db
        .prepare(r#"update docs[id == "t1"] cas each { title: "txuniquealpha", wing: "txprep" }"#)
        .unwrap();
    let delete_prep = db
        .prepare(r#"delete docs[id == "t2"] cas each"#)
        .unwrap();

    // 1) Explicit rollback: uncommitted overlay DML & FTS stay disk-backed and vanish cleanly.
    {
        let mut tx = db.transaction().unwrap();
        tx.run(r#"insert docs { uri: "raw://tx/extra-rb", title: "extra rb", wing: "new", layer: "wiki", body: "txuniquerb" }"#)
            .unwrap();
        assert_eq!(tx.run_prepared(&update_prep).unwrap().done.n, 1);
        assert_eq!(tx.run_prepared(&delete_prep).unwrap().done.n, 1);
        let pred_upd = tx
            .run(r#"update docs[id == "t3"] cas each { wing: "txwing" }"#)
            .unwrap();
        assert_eq!(pred_upd.done.n, 1);
        assert!(
            tx.db().store.is_disk_backed("docs"),
            "in-tx cold DML must stay disk-backed"
        );
        assert_eq!(
            tx.run(r#"docs | count"#).unwrap().scalar_as::<i64>().unwrap(),
            total_seed as i64
        );
        assert_eq!(
            tx.run(r#"docs | search lex "txuniquealpha" | take all"#)
                .unwrap()
                .rows
                .len(),
            1
        );
        assert_eq!(
            text(
                &tx.run(r#"docs | id == "t1" | { title }"#).unwrap().rows[0],
                "title"
            ),
            "txuniquealpha"
        );
        assert_eq!(
            tx.run(r#"docs | id == "t2" | count"#)
                .unwrap()
                .scalar_as::<i64>()
                .unwrap(),
            0
        );
        tx.rollback().unwrap();
    }
    assert!(db.store.is_disk_backed("docs"), "rollback promoted docs");
    assert_eq!(
        db.run(r#"docs | count"#).unwrap().scalar_as::<i64>().unwrap(),
        total_seed as i64
    );
    assert_eq!(
        db.run(r#"docs | search lex "txuniquealpha" | take all"#)
            .unwrap()
            .rows
            .len(),
        0
    );
    assert_eq!(
        text(
            &db.run(r#"docs | id == "t1" | { title, body }"#).unwrap().rows[0],
            "title"
        ),
        "seed 1"
    );
    assert_eq!(
        db.run(r#"docs | id == "t2" | count"#)
            .unwrap()
            .scalar_as::<i64>()
            .unwrap(),
        1
    );
    assert_eq!(
        text(
            &db.run(r#"docs | id == "t3" | { wing }"#).unwrap().rows[0],
            "wing"
        ),
        "odd"
    );

    // 2) Drop without commit rolls back overlay savepoint without promoting docs.
    {
        let mut tx = db.transaction().unwrap();
        assert_eq!(tx.run_prepared(&update_prep).unwrap().done.n, 1);
        assert!(tx.db().store.is_disk_backed("docs"));
    }
    assert!(db.store.is_disk_backed("docs"), "drop rollback promoted docs");
    assert_eq!(
        text(
            &db.run(r#"docs | id == "t1" | { title }"#).unwrap().rows[0],
            "title"
        ),
        "seed 1"
    );
    assert_eq!(
        db.run(r#"docs | search lex "txuniquealpha" | take all"#)
            .unwrap()
            .rows
            .len(),
        0
    );

    // 3) Commit + un-checkpointed WAL replay + checkpoint + reopen.
    {
        let mut tx = db.transaction().unwrap();
        tx.run(r#"insert docs { uri: "raw://tx/extra-ok", title: "extra ok", wing: "new", layer: "wiki", body: "txuniqueok" }"#)
            .unwrap();
        assert_eq!(tx.run_prepared(&update_prep).unwrap().done.n, 1);
        assert_eq!(tx.run_prepared(&delete_prep).unwrap().done.n, 1);
        assert!(tx.db().store.is_disk_backed("docs"));
        tx.commit().unwrap();
    }
    let check_committed = |d: &mut Db, stage: &str| {
        assert!(d.store.is_disk_backed("docs"), "{stage}: promoted docs");
        assert_eq!(
            d.run(r#"docs | count"#).unwrap().scalar_as::<i64>().unwrap(),
            total_seed as i64,
            "{stage}: count"
        );
        assert_eq!(
            text(
                &d.run(r#"docs | id == "t1" | { title }"#).unwrap().rows[0],
                "title"
            ),
            "txuniquealpha",
            "{stage}: updated title"
        );
        assert_eq!(
            d.run(r#"docs | id == "t2" | count"#)
                .unwrap()
                .scalar_as::<i64>()
                .unwrap(),
            0,
            "{stage}: deleted row"
        );
        assert_eq!(
            d.run(r#"docs | uri == "raw://tx/extra-ok" | count"#)
                .unwrap()
                .scalar_as::<i64>()
                .unwrap(),
            1,
            "{stage}: inserted row"
        );
        assert_eq!(
            d.run(r#"docs | search lex "txuniquealpha" | take all"#)
                .unwrap()
                .rows
                .len(),
            1,
            "{stage}: fts after tx"
        );
    };
    check_committed(&mut db, "after tx.commit");

    // Copy open DB directory before `db.close()` checkpoints it, to verify pure WAL replay.
    let replay_dir = tmp();
    copy_dir_no_lock(&dir, &replay_dir);
    {
        let mut replayed = Db::open_with(&replay_dir, cold_opts()).unwrap();
        check_committed(&mut replayed, "un-checkpointed WAL replay");
        replayed.close().unwrap();
    }
    let _ = fs::remove_dir_all(&replay_dir);

    db.checkpoint().unwrap();
    db.close().unwrap();
    let mut db = Db::open_with(&dir, cold_opts()).unwrap();
    check_committed(&mut db, "checkpoint + reopen");
    db.close().unwrap();
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn cold_program_cas_each_and_let_stays_disk_backed() {
    let cold_opts = || lin::OpenOpts {
        sync: lin::SyncMode::Full,
        cold: true,
    };
    let dir = tmp();
    {
        let mut db = Db::open_with(&dir, cold_opts()).unwrap();
        let mut src = String::from("insert docs [\n");
        for i in 0..64 {
            let wing = if i % 2 == 0 { "alpha" } else { "beta" };
            src.push_str(&format!(
                "  {{ id: \"p{i}\", uri: \"raw://prog/{i}\", title: \"prog {i}\", wing: \"{wing}\", layer: \"wiki\", body: \"prog body {i}\" }},\n"
            ));
        }
        src.push(']');
        db.run(&src).unwrap();
        db.checkpoint().unwrap();
        db.close().unwrap();
    }

    let mut db = Db::open_with(&dir, cold_opts()).unwrap();
    assert!(db.store.is_disk_backed("docs"));

    // Multi-statement program with `let`, `insert`, `update cas each`, `delete cas each`, and query.
    let res = db
        .run(
            r#"
            let target = docs | uri == "raw://prog/1" | take 1
            insert docs { uri: "raw://prog/new", title: "prog inserted", wing: "alpha", layer: "wiki", body: "prog new body" }
            update docs[uri == "raw://prog/1"] cas each { title: "prog ok" }
            delete docs[uri == "raw://prog/2"] cas each
            docs | wing == "alpha" | count
            "#,
        )
        .unwrap();
    assert!(
        db.store.is_disk_backed("docs"),
        "cold program with let + cas each + query must stay disk-backed"
    );
    assert_eq!(res.rows.len(), 1);
    assert_eq!(res.scalar_as::<i64>().unwrap(), 32);
    assert_eq!(
        text(
            &db.run(r#"docs | uri == "raw://prog/1" | { title }"#).unwrap().rows[0],
            "title"
        ),
        "prog ok"
    );
    assert_eq!(
        db.run(r#"docs | uri == "raw://prog/2" | count"#)
            .unwrap()
            .scalar_as::<i64>()
            .unwrap(),
        0
    );

    // Mid-pack `cas each` conflict on the same row must reject and atomically roll back earlier insert.
    let err = db
        .run(
            r#"
            insert docs { uri: "raw://prog/should-rollback", title: "nope", wing: "alpha", layer: "wiki", body: "rb" }
            update docs[uri == "raw://prog/3"] cas each { title: "first" }
            update docs[uri == "raw://prog/3"] cas each { title: "second" }
            "#,
        )
        .unwrap_err();
    assert!(
        err.to_string().contains("cas"),
        "expected mid-pack cas conflict error, got: {err}"
    );
    assert!(
        db.store.is_disk_backed("docs"),
        "failed cold program must keep docs disk-backed"
    );
    assert_eq!(
        db.run(r#"docs | uri == "raw://prog/should-rollback" | count"#)
            .unwrap()
            .scalar_as::<i64>()
            .unwrap(),
        0
    );
    assert_eq!(
        text(
            &db.run(r#"docs | uri == "raw://prog/3" | { title }"#).unwrap().rows[0],
            "title"
        ),
        "prog 3"
    );

    db.close().unwrap();
    let _ = fs::remove_dir_all(&dir);
}

/// `Db::insert_batch` behaves like the equivalent `insert` statement: same rows, same
/// rejections, atomic, durable, and it leaves a cold collection on disk.
#[test]
fn insert_batch_matches_insert_statement() {
    use lin::{Cell, RecordBatch};
    fn batch(ids: std::ops::Range<usize>) -> RecordBatch {
        let names: Arc<[String]> = ["id", "user_id", "total"]
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>()
            .into();
        let mut b = RecordBatch::with_capacity(names, ids.len());
        for i in ids {
            b.push_row(&[
                Cell::from_text(format!("b{i}")),
                Cell::from_text(format!("u{}", i % 40)),
                // Every tenth row has no total.
                if i % 10 == 9 { Cell::Null } else { Cell::Float(i as f64 * 1.5) },
            ]);
        }
        b
    }
    fn text_insert(ids: std::ops::Range<usize>) -> String {
        let mut s = String::from("insert orders [\n");
        for (n, i) in ids.enumerate() {
            if n > 0 {
                s.push(',');
            }
            if i % 10 == 9 {
                s.push_str(&format!(r#"{{ id: "b{i}", user_id: "u{}" }}"#, i % 40));
            } else {
                s.push_str(&format!(
                    r#"{{ id: "b{i}", user_id: "u{}", total: {:?} }}"#,
                    i % 40,
                    i as f64 * 1.5
                ));
            }
        }
        s.push_str("\n]");
        s
    }
    fn seed(db: &mut Db) {
        let mut users = String::from("insert users [\n");
        for i in 0..40 {
            if i > 0 {
                users.push(',');
            }
            users.push_str(&format!(r#"{{ id: "u{i}", email: "user{i}@example.com" }}"#));
        }
        users.push_str("\n]");
        db.run(&users).unwrap();
        let mut orders = String::from("insert orders [\n");
        for i in 0..64 {
            if i > 0 {
                orders.push(',');
            }
            orders.push_str(&format!(r#"{{ id: "o{i}", user_id: "u{}", total: {i} }}"#, i % 40));
        }
        orders.push_str("\n]");
        db.run(&orders).unwrap();
    }
    let dump = |db: &mut Db| -> Vec<String> {
        let mut rows: Vec<String> = db
            .run(r#"orders | { id, user_id, total } | take all"#)
            .unwrap()
            .rows
            .iter()
            .map(row_key)
            .collect();
        rows.sort();
        rows
    };

    let cold_dir = tmp();
    let ref_dir = tmp();
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
    let mut reference = Db::open(&ref_dir).unwrap();
    seed(&mut reference);
    assert!(cold.store.is_disk_backed("orders"));

    // Same data through the two front doors.
    let gen0 = cold.store.r#gen;
    let done = cold.insert_batch("orders", &batch(0..300)).unwrap();
    assert_eq!(done.n, 300);
    assert_eq!(done.r#gen, gen0 + 1, "one statement, one generation");
    reference.run(&text_insert(0..300)).unwrap();
    assert_eq!(dump(&mut cold), dump(&mut reference));
    assert!(cold.store.is_disk_backed("orders"), "insert_batch promoted orders");

    // An in-memory database takes the same call.
    let mut hot = Db::empty();
    seed(&mut hot);
    assert_eq!(hot.insert_batch("orders", &batch(0..300)).unwrap().n, 300);
    assert_eq!(dump(&mut hot), dump(&mut reference));

    // Rejections are the statement's own, and nothing is written.
    let before = dump(&mut cold);
    let dup = cold.insert_batch("orders", &batch(290..310)).unwrap_err();
    assert!(dup.to_string().contains("duplicate id"), "{dup}");
    let mut orphan = batch(400..402);
    orphan.cols[1] = lin::Column::from(vec![Cell::from_text("u1"), Cell::from_text("ghost")]);
    let fk = cold.insert_batch("orders", &orphan).unwrap_err();
    assert!(fk.to_string().contains("fk"), "{fk}");
    assert!(cold.insert_batch("no_such_collection", &batch(500..501)).is_err());
    let mut ragged = batch(600..603);
    ragged.cols[2].truncate(1);
    assert!(cold.insert_batch("orders", &ragged).is_err());
    let mut with_vec = batch(700..701);
    with_vec.cols[2] = lin::Column::from(vec![Cell::Vec(Arc::from([1.0f32, 2.0]))]);
    assert!(cold.insert_batch("orders", &with_vec).is_err());
    assert_eq!(dump(&mut cold), before, "a rejected batch left rows behind");
    assert_eq!(cold.insert_batch("orders", &batch(0..0)).unwrap().n, 0);

    // Durable without a checkpoint, and after one.
    cold.close().unwrap();
    let mut cold = Db::open_with(&cold_dir, cold_opts()).unwrap();
    assert_eq!(dump(&mut cold), dump(&mut reference));
    cold.checkpoint().unwrap();
    cold.close().unwrap();
    let mut cold = Db::open_with(&cold_dir, cold_opts()).unwrap();
    assert_eq!(dump(&mut cold), dump(&mut reference));
    assert!(cold.store.is_disk_backed("orders"));
    cold.close().unwrap();
    reference.close().unwrap();
    let _ = fs::remove_dir_all(&cold_dir);
    let _ = fs::remove_dir_all(&ref_dir);
}

/// A numeric column whose first values are integers must keep fractional values written
/// later: through the overlay flush, through a full re-spill from memory, and in aggregates
/// that then see integer and float segments side by side.
#[test]
fn cold_numeric_column_keeps_fractions_after_integers() {
    fn seed(db: &mut Db) {
        let mut users = String::from("insert users [\n");
        for i in 0..40 {
            if i > 0 {
                users.push(',');
            }
            users.push_str(&format!(r#"{{ id: "u{i}", email: "user{i}@example.com" }}"#));
        }
        users.push_str("\n]");
        db.run(&users).unwrap();
        let mut orders = String::from("insert orders [\n");
        for i in 0..64 {
            if i > 0 {
                orders.push(',');
            }
            // Integer literals only: the disk column starts out as an integer column.
            orders.push_str(&format!(r#"{{ id: "o{i}", user_id: "u{}", total: {i} }}"#, i % 40));
        }
        orders.push_str("\n]");
        db.run(&orders).unwrap();
    }
    fn fractions(db: &mut Db, tag: &str, n: usize) {
        let mut s = String::from("insert orders [\n");
        for i in 0..n {
            if i > 0 {
                s.push(',');
            }
            s.push_str(&format!(
                r#"{{ id: "{tag}{i}", user_id: "u{}", total: {}.25 }}"#,
                i % 40,
                i
            ));
        }
        s.push_str("\n]");
        db.run(&s).unwrap();
    }
    const QUERIES: &[&str] = &[
        r#"orders | { id, total } | take all"#,
        r#"orders | sum total"#,
        r#"orders | sum total by user_id"#,
        r#"orders | total > 1 and total < 2 | { id, total } | take all"#,
        r#"orders | total > 10 | count"#,
        r#"orders | total >= 3.25 and total <= 3.25 | count"#,
        r#"orders | id == "ov3" | { total }"#,
    ];
    let compare = |cold: &mut Db, hot: &mut Db, stage: &str| {
        for q in QUERIES {
            let mut c: Vec<String> = cold.run(q).unwrap().rows.iter().map(row_key).collect();
            let mut h: Vec<String> = hot.run(q).unwrap().rows.iter().map(row_key).collect();
            c.sort();
            h.sort();
            assert_eq!(c, h, "{stage}: {q}");
        }
    };
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

    // 1. Small batch -> overlay -> delta segment written by the checkpoint.
    let mut cold = Db::open_with(&cold_dir, cold_opts()).unwrap();
    fractions(&mut cold, "ov", 20);
    fractions(&mut hot, "ov", 20);
    assert!(cold.store.is_disk_backed("orders"));
    compare(&mut cold, &mut hot, "overlay");
    cold.checkpoint().unwrap();
    compare(&mut cold, &mut hot, "overlay flushed");
    cold.close().unwrap();
    let mut cold = Db::open_with(&cold_dir, cold_opts()).unwrap();
    compare(&mut cold, &mut hot, "overlay flushed + reopen");

    // 2. Collection promoted to memory, written there, then spilled again as a whole.
    cold.store.promote_disk_col("orders");
    assert!(!cold.store.is_disk_backed("orders"));
    fractions(&mut cold, "big", 5_000);
    fractions(&mut hot, "big", 5_000);
    cold.checkpoint().unwrap();
    cold.close().unwrap();
    let mut cold = Db::open_with(&cold_dir, cold_opts()).unwrap();
    assert!(cold.store.is_disk_backed("orders"));
    compare(&mut cold, &mut hot, "re-spilled + reopen");

    cold.close().unwrap();
    hot.close().unwrap();
    let _ = fs::remove_dir_all(&cold_dir);
    let _ = fs::remove_dir_all(&hot_dir);
}

/// Predicates with `or`, `not`, `!=`, `in`, `~` and null tests are evaluated on raw column
/// data in the disk scan; nulls make that subtle (`null != x` is true, `null > x` is false),
/// so every shape is compared with the in-memory executor on data full of nulls.
#[test]
fn cold_boolean_and_null_predicates_match_in_memory() {
    const N: usize = 5_000;
    fn seed(db: &mut Db) {
        db.run("col ev { id: text, kind: text, n: i64, score: f64 }").unwrap();
        for chunk in 0..(N / 1000) {
            let mut s = String::from("insert ev [\n");
            for i in (chunk * 1000)..((chunk + 1) * 1000) {
                if i > chunk * 1000 {
                    s.push(',');
                }
                s.push_str(&format!(r#"{{ id: "e{i}""#));
                if i % 7 != 0 {
                    s.push_str(&format!(r#", kind: "{}""#, ["red", "green", "blue", "grey"][i % 4]));
                }
                if i % 5 != 0 {
                    s.push_str(&format!(", n: {}", (i * 31) % 200));
                }
                if i % 11 != 0 {
                    s.push_str(&format!(", score: {}.5", (i * 17) % 90));
                }
                s.push_str(" }");
            }
            s.push_str("\n]");
            db.run(&s).unwrap();
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
    assert!(cold.store.is_disk_backed("ev"));

    let preds = [
        r#"n > 100 or kind == "red""#,
        r#"n > 150 or score < 5.5"#,
        r#"not n > 100"#,
        r#"not (n > 100 or kind == "red")"#,
        r#"not kind == "red""#,
        r#"n != 62"#,
        r#"kind != "blue""#,
        r#"kind != "no-such-kind""#,
        r#"n is null"#,
        r#"n is not null and kind is null"#,
        r#"kind is null or score is null"#,
        r#"not (n is null)"#,
        r#"kind in ["red", "grey"] or n < 3"#,
        r#"not kind in ["red", "grey"]"#,
        r#"kind ~ "re" and not n > 50"#,
        r#"not kind ~ "re""#,
        r#"(n > 20 and n < 40) or (score > 80 and kind == "green")"#,
        r#"n >= 0 and n <= 199 and n != 100 and kind != "grey""#,
        r#"score == 44.5 or n == 31"#,
        r#"not (score == 44.5) and n == 31"#,
        r#"n > 1000 or kind == "purple""#,
        r#"not (n > 1000 or kind == "purple")"#,
        r#"n > 50 and (kind == "red" or kind == "blue") and not score > 40"#,
    ];
    let mut checked = 0usize;
    for p in preds {
        for tail in ["{ id }", "{ id, kind, n, score }", "count"] {
            let q = format!("ev | {p} | {tail}");
            let q = unlimited(&q);
            let mut c: Vec<String> = cold.run(&q).unwrap().rows.iter().map(row_key).collect();
            let mut h: Vec<String> = hot.run(&q).unwrap().rows.iter().map(row_key).collect();
            c.sort();
            h.sort();
            assert_eq!(c.len(), h.len(), "{q}");
            assert_eq!(c, h, "{q}");
            checked += c.len();
        }
        // Whole rows too (no projection).
        let q = format!("ev | {p} | take all");
        let mut c: Vec<String> = cold.run(&q).unwrap().rows.iter().map(row_key).collect();
        let mut h: Vec<String> = hot.run(&q).unwrap().rows.iter().map(row_key).collect();
        c.sort();
        h.sort();
        assert_eq!(c, h, "{q}");
    }
    assert!(checked > 20_000, "queries matched too few rows to mean much: {checked}");
    assert!(cold.store.is_disk_backed("ev"));
    cold.close().unwrap();
    hot.close().unwrap();
    let _ = fs::remove_dir_all(&cold_dir);
    let _ = fs::remove_dir_all(&hot_dir);
}

/// A bulk insert whose rows differ in shape (missing fields, an integer and a fraction in
/// the same column) must come back from WAL replay exactly as it was inserted. The compact
/// columnar WAL record cannot express that, so such batches must not be logged with it.
#[test]
fn bulk_insert_with_missing_fields_survives_wal_replay() {
    fn copy_dir(src: &std::path::Path, dst: &std::path::Path) {
        fs::create_dir_all(dst).unwrap();
        for e in fs::read_dir(src).unwrap().flatten() {
            let (p, name) = (e.path(), e.file_name());
            if name.to_string_lossy().to_lowercase().contains("lock") {
                continue;
            }
            if p.is_dir() {
                copy_dir(&p, &dst.join(&name));
            } else {
                fs::copy(&p, dst.join(&name)).unwrap();
            }
        }
    }
    let dump = |db: &mut Db, collection: &str| -> Vec<String> {
        let mut v: Vec<String> = db
            .run(&format!("{collection} | take all"))
            .unwrap()
            .rows
            .iter()
            .map(|r| {
                let mut r = r.clone();
                r.remove("ts");
                r.remove("embedding");
                format!("{r:?}")
            })
            .collect();
        v.sort();
        v
    };
    let dir = tmp();
    let copy = tmp();
    let mut db = Db::open(&dir).unwrap();
    db.run(r#"insert users [ { id: "u1", email: "a@b.c" }, { id: "u2", email: "d@e.f" } ]"#)
        .unwrap();
    // Missing fields in different rows; integer and fractional totals side by side.
    db.run(
        r#"insert orders [
            { id: "a", user_id: "u1", total: 5.5 },
            { id: "b", user_id: "u2" },
            { id: "c", total: 7.5 },
            { id: "d", user_id: "u1", total: 9 },
            { id: "e", user_id: "u2", total: 0 }
        ]"#,
    )
    .unwrap();
    db.run(
        r#"insert docs [
            { id: "d1", uri: "raw://r/1", title: "has room", layer: "wiki", room: "inbox", body: "x" },
            { id: "d2", uri: "raw://r/2", title: "no room, no body", layer: "wiki" },
            { id: "d3", uri: "raw://r/3", title: "", layer: "wiki", wing: "w" }
        ]"#,
    )
    .unwrap();
    // A regular batch keeps using the compact record; it must round-trip too.
    db.run(
        r#"insert orders [
            { id: "r1", user_id: "u1", total: 1.5 },
            { id: "r2", user_id: "u2", total: 2.5 }
        ]"#,
    )
    .unwrap();
    // Large batches take the pre-encoding route; one with gaps must not.
    for (tag, n) in [("m", 700usize), ("l", 5_000)] {
        let mut s = String::from("insert orders [\n");
        for i in 0..n {
            if i > 0 {
                s.push(',');
            }
            match i % 4 {
                0 => s.push_str(&format!(r#"{{ id: "{tag}{i}", user_id: "u1", total: {i}.5 }}"#)),
                1 => s.push_str(&format!(r#"{{ id: "{tag}{i}", user_id: "u2" }}"#)),
                2 => s.push_str(&format!(r#"{{ id: "{tag}{i}", total: {i} }}"#)),
                _ => s.push_str(&format!(r#"{{ id: "{tag}{i}", user_id: "u1", total: 0 }}"#)),
            }
        }
        s.push_str("\n]");
        db.run(&s).unwrap();
    }
    let live_orders = dump(&mut db, "orders");
    let live_docs = dump(&mut db, "docs");
    assert_eq!(live_orders.len(), 7 + 5_700);

    // Copy the directory while the writer is open: only the WAL carries these inserts.
    copy_dir(&dir, &copy);
    let mut replayed = Db::open(&copy).unwrap();
    assert_eq!(dump(&mut replayed, "orders"), live_orders);
    assert_eq!(dump(&mut replayed, "docs"), live_docs);
    for (q, want) in [
        (r#"orders | total is null | count"#, 1 + 175 + 1250),
        (r#"orders | user_id is null | count"#, 1 + 175 + 1250),
        (r#"orders | total == 0 | count"#, 1 + 175 + 1250),
        (r#"docs | room is null | count"#, 2),
        (r#"docs | title == "" | count"#, 1),
    ] {
        assert_eq!(replayed.run(q).unwrap().scalar_as::<i64>().unwrap(), want, "{q}");
        assert_eq!(db.run(q).unwrap().scalar_as::<i64>().unwrap(), want, "live {q}");
    }
    replayed.close().unwrap();
    db.close().unwrap();
    let _ = fs::remove_dir_all(&dir);
    let _ = fs::remove_dir_all(&copy);
}

/// Deletes are logged by key, not as whole rows: a statement removing thousands of
/// documents (bodies and embeddings included) fits one WAL record, and replay removes
/// exactly the same rows for documents, plain collections and facts.
#[test]
fn large_delete_fits_one_wal_record_and_replays() {
    fn copy_dir(src: &std::path::Path, dst: &std::path::Path) {
        fs::create_dir_all(dst).unwrap();
        for e in fs::read_dir(src).unwrap().flatten() {
            let (p, name) = (e.path(), e.file_name());
            if name.to_string_lossy().to_lowercase().contains("lock") {
                continue;
            }
            if p.is_dir() {
                copy_dir(&p, &dst.join(&name));
            } else {
                fs::copy(&p, dst.join(&name)).unwrap();
            }
        }
    }
    let ids = |db: &mut Db, q: &str| -> Vec<String> {
        let mut v: Vec<String> = db
            .run(q)
            .unwrap()
            .rows
            .iter()
            .map(|r| format!("{r:?}"))
            .collect();
        v.sort();
        v
    };
    let dir = tmp();
    let copy = tmp();
    let mut db = Db::open(&dir).unwrap();
    db.run(r#"insert users [ { id: "u1", email: "a@b.c" }, { id: "u2", email: "d@e.f" } ]"#)
        .unwrap();
    // 6 000 documents in batches (each has a body and an embedding).
    for chunk in 0..6 {
        let mut s = String::from("insert docs [\n");
        for i in (chunk * 1000)..((chunk + 1) * 1000) {
            if i > chunk * 1000 {
                s.push(',');
            }
            s.push_str(&format!(
                r#"{{ id: "d{i}", uri: "raw://big/{i}", title: "Doc {i}", wing: "{}", layer: "wiki", body: "body of document number {i} with some text" }}"#,
                if i % 6 == 0 { "keep" } else { "drop" }
            ));
        }
        s.push_str("\n]");
        db.run(&s).unwrap();
    }
    db.run(
        r#"insert orders [
            { id: "o1", user_id: "u1", total: 1 },
            { id: "o2", user_id: "u2", total: 2 },
            { id: "o3", user_id: "u1", total: 3 }
        ]"#,
    )
    .unwrap();
    db.run(r#"append facts { s: "lin", p: tagged, o: "x" }"#).unwrap();
    db.run(r#"append facts { s: "lin", p: tagged, o: "y" }"#).unwrap();
    db.checkpoint().unwrap();

    // One statement, 5 000 documents: used to exceed the 16 MiB record limit.
    let del = db.run(r#"delete docs[wing == "drop"] cas each"#).unwrap();
    assert_eq!(del.done.n, 5_000);
    assert_eq!(del.rows.len(), 5_000, "the caller still gets the full deleted rows");
    assert!(del.rows[0].contains_key("body"), "returned rows are not slimmed");
    db.run(r#"delete orders[total > 1] cas each"#).unwrap();
    db.run(r#"delete facts[s == "lin" and p == tagged and o == "x"]"#)
        .unwrap();

    let queries = [
        r#"docs | { id } | take all"#,
        r#"orders | { id } | take all"#,
        r#"facts | take all"#,
    ];
    let live: Vec<Vec<String>> = queries.iter().map(|q| ids(&mut db, q)).collect();
    assert_eq!(live[0].len(), 1_000);
    assert_eq!(live[1].len(), 1);
    assert_eq!(
        db.run(r#"docs | search lex "document" | take all"#)
            .unwrap()
            .rows
            .len(),
        1_000
    );

    // Only the WAL carries the deletes into the copy.
    copy_dir(&dir, &copy);
    let mut replayed = Db::open(&copy).unwrap();
    for (q, want) in queries.iter().zip(&live) {
        assert_eq!(&ids(&mut replayed, q), want, "{q}");
    }
    assert_eq!(
        replayed
            .run(r#"docs | search lex "document" | take all"#)
            .unwrap()
            .rows
            .len(),
        1_000
    );
    replayed.close().unwrap();
    db.close().unwrap();
    let _ = fs::remove_dir_all(&dir);
    let _ = fs::remove_dir_all(&copy);
}

/// `insert ... with edge` into a cold collection keeps it on disk, resolves page targets
/// against the disk engine, and produces the same edges as the in-memory path, both live
/// and after WAL replay.
#[test]
fn cold_insert_with_edges_stays_disk_backed() {
    fn copy_dir(src: &std::path::Path, dst: &std::path::Path) {
        fs::create_dir_all(dst).unwrap();
        for e in fs::read_dir(src).unwrap().flatten() {
            let (p, name) = (e.path(), e.file_name());
            if name.to_string_lossy().to_lowercase().contains("lock") {
                continue;
            }
            if p.is_dir() {
                copy_dir(&p, &dst.join(&name));
            } else {
                fs::copy(&p, dst.join(&name)).unwrap();
            }
        }
    }
    fn seed(db: &mut Db) {
        let mut rows = String::from("insert docs [\n");
        for i in 0..64 {
            if i > 0 {
                rows.push(',');
            }
            rows.push_str(&format!(
                r#"{{ id: "p{i}", uri: "wiki://page/{i}", title: "Page {i}", layer: "wiki", body: "b{i}" }}"#
            ));
        }
        rows.push_str("\n]");
        db.run(&rows).unwrap();
    }
    const STATEMENTS: &[&str] = &[
        // target is a cold document, addressed by uri -> edge points at its id
        r#"insert docs { id: "n1", uri: "raw://e/1", title: "one", layer: "raw" } with edge wikilink -> page "wiki://page/7""#,
        // unknown page -> edge keeps the uri
        r#"insert docs { id: "n2", uri: "raw://e/2", title: "two", layer: "raw" } with edge wikilink -> page "wiki://nowhere""#,
        // target is a row that itself lives in the overlay
        r#"insert docs { id: "n3", uri: "raw://e/3", title: "three", layer: "raw" } with edge wikilink -> page "raw://e/1""#,
    ];
    let cold_dir = tmp();
    let hot_dir = tmp();
    let copy = tmp();
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
    assert!(cold.store.is_disk_backed("docs"));
    for stmt in STATEMENTS {
        cold.run(stmt).unwrap();
        hot.run(stmt).unwrap();
        assert!(cold.store.is_disk_backed("docs"), "promoted by: {stmt}");
    }
    let edges = |db: &Db| -> Vec<String> {
        let mut v: Vec<String> = db
            .store
            .edges
            .iter()
            .map(|e| format!("{} {} -> {}", e.rel, e.from, e.to))
            .collect();
        v.sort();
        v
    };
    let expected = vec![
        "wikilink n1 -> p7".to_string(),
        "wikilink n2 -> wiki://nowhere".to_string(),
        "wikilink n3 -> n1".to_string(),
    ];
    assert_eq!(edges(&hot), expected, "in-memory reference");
    assert_eq!(edges(&cold), expected);
    let hop = cold
        .run(r#"docs | id == "n1" | hop wikilink | { id, title }"#)
        .unwrap();
    assert!(
        hop.rows.iter().any(|r| text(r, "id") == "p7"),
        "hop from a cold-inserted doc: {:?}",
        hop.rows
    );
    assert_eq!(cold.run(r#"docs | count"#).unwrap().scalar_as::<i64>().unwrap(), 67);

    // A duplicate with an edge is rejected and adds no edge.
    let err = cold
        .run(r#"insert docs { id: "n1", uri: "raw://e/dup", title: "dup", layer: "raw" } with edge wikilink -> page "wiki://page/1""#)
        .unwrap_err();
    assert!(err.to_string().contains("duplicate id"), "{err}");
    assert_eq!(edges(&cold), expected);

    copy_dir(&cold_dir, &copy);
    let replayed = Db::open_with(&copy, cold_opts()).unwrap();
    assert_eq!(edges(&replayed), expected, "edges after WAL replay");
    assert_eq!(replayed.stats().docs, 67);
    drop(replayed);
    cold.close().unwrap();
    hot.close().unwrap();
    for d in [&cold_dir, &hot_dir, &copy] {
        let _ = fs::remove_dir_all(d);
    }
}

/// A projection over a `uri` point lookup must apply to rows that still live in the
/// overlay of a cold collection, not only to rows already in segments.
#[test]
fn cold_uri_lookup_projects_overlay_rows() {
    let dir = tmp();
    let opts = || lin::OpenOpts {
        sync: lin::SyncMode::Full,
        cold: true,
    };
    {
        let mut db = Db::open_with(&dir, opts()).unwrap();
        let mut src = String::from("insert docs [");
        for i in 0..200 {
            if i > 0 {
                src.push(',');
            }
            src.push_str(&format!(
                r#"{{ uri: "raw://d{i}", title: "T{i}", layer: "wiki", wing: "w{}" }}"#,
                i % 4
            ));
        }
        src.push(']');
        db.run(&src).unwrap();
        db.checkpoint().unwrap();
        db.close().unwrap();
    }
    let mut db = Db::open_with(&dir, opts()).unwrap();
    assert!(db.store.is_disk_backed("docs"));
    db.run(r#"insert docs { uri: "raw://new", title: "NEW", layer: "wiki", wing: "wn" }"#)
        .unwrap();
    assert!(db.store.is_disk_backed("docs"));
    for (uri, title, wing) in [("raw://new", "NEW", "wn"), ("raw://d7", "T7", "w3")] {
        for q in [
            format!(r#"docs | uri == "{uri}" | pick {{ title, wing }}"#),
            format!(r#"docs | uri == "{uri}" | {{ title, wing }} | take all"#),
        ] {
            let handle = db.run(&q).unwrap();
            let rows = &handle.rows;
            assert_eq!(rows.len(), 1, "{q}");
            let keys: Vec<&str> = rows[0].keys().map(String::as_str).collect();
            assert_eq!(keys, ["title", "wing"], "{q}");
            assert_eq!(rows[0].get("title").and_then(lin::Cell::text), Some(title), "{q}");
            assert_eq!(rows[0].get("wing").and_then(lin::Cell::text), Some(wing), "{q}");
        }
    }
    let handle = db.run(r#"docs | uri == "raw://new""#).unwrap();
    let rows = &handle.rows;
    assert!(rows[0].contains_key("id") && rows[0].contains_key("layer"));
}

/// Concurrent `AsyncDb::transaction` calls share commit groups with each other and with
/// plain statements: each caller gets its own result, a failed transaction rolls back
/// alone, and everything acknowledged survives a reopen without a checkpoint.
#[cfg(feature = "async")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn async_transactions_share_commit_groups() {
    let dir = tmp();
    let opts = || lin::OpenOpts {
        sync: lin::SyncMode::Full,
        cold: false,
    };
    let adb = lin::AsyncDb::new(Db::open_with(&dir, opts()).unwrap());
    const N: usize = 40;
    let mut tasks = Vec::new();
    for i in 0..N {
        let adb = adb.clone();
        tasks.push(tokio::spawn(async move {
            if i % 4 == 3 {
                // A plain statement in the same groups.
                let done = adb
                    .execute(format!(
                        r#"insert docs {{ id: "s{i}", uri: "raw://t/s{i}", title: "stmt", layer: "wiki" }}"#
                    ))
                    .await;
                return (i, done.map(|d| d.n as i64));
            }
            let res = adb
                .transaction(move |tx| {
                    tx.execute(&format!(
                        r#"insert docs {{ id: "a{i}", uri: "raw://t/a{i}", title: "first", layer: "wiki" }}"#
                    ))?;
                    tx.execute(&format!(
                        r#"insert docs {{ id: "b{i}", uri: "raw://t/b{i}", title: "second", layer: "wiki" }}"#
                    ))?;
                    if i % 4 == 2 {
                        // Fails after two writes: both must be rolled back.
                        return Err(lin::Error::runtime(format!("abort {i}")));
                    }
                    Ok(i as i64 * 10)
                })
                .await;
            (i, res)
        }));
    }
    let mut committed = Vec::new();
    for t in tasks {
        let (i, res) = t.await.unwrap();
        match i % 4 {
            3 => assert_eq!(res.unwrap(), 1, "statement {i}"),
            2 => assert!(res.unwrap_err().to_string().contains(&format!("abort {i}"))),
            _ => {
                assert_eq!(res.unwrap(), i as i64 * 10, "transaction {i} got a foreign result");
                committed.push(i);
            }
        }
    }
    let count = |rows: &lin::Handle| match rows.rows[0].get("hits") {
        Some(lin::Cell::Int(n)) => *n,
        other => panic!("{other:?}"),
    };
    let expect = (committed.len() * 2 + N / 4) as i64;
    assert_eq!(count(&adb.run("docs | count").await.unwrap()), expect);

    // No checkpoint, no close: the acknowledged commits must be in the log already.
    let inner = adb.inner().clone();
    drop(adb);
    let wal_bytes = std::fs::metadata(dir.join("log")).unwrap().len();
    assert!(wal_bytes > 0);
    let copy = tmp();
    for name in ["log", "head", "snapshot"] {
        if dir.join(name).exists() {
            std::fs::copy(dir.join(name), copy.join(name)).unwrap();
        }
    }
    drop(inner);
    let mut reopened = Db::open_with(&copy, opts()).unwrap();
    assert_eq!(count(&reopened.run("docs | count").unwrap()), expect);
    for i in committed {
        let q = format!(r#"docs | id == "a{i}" or id == "b{i}" | count"#);
        assert_eq!(count(&reopened.run(&q).unwrap()), 2, "{q}");
    }
    let aborted = reopened.run(r#"docs | title == "first" | count"#).unwrap();
    assert_eq!(count(&aborted), (N / 2) as i64);
}

/// A join on a foreign key that targets a non-`id` field of a cold collection matches
/// the same join over in-memory data, including one-to-many matches and left joins.
#[test]
fn cold_join_on_non_id_key_matches_in_memory() {
    const SCHEMA: &str = "col parents { id: text, key: text, label: text }\n\
                          col children { id: text, parent: text, n: i64 }\n\
                          fk children.parent -> parents.key";
    fn seed(db: &mut Db) {
        db.run(SCHEMA).unwrap();
        let mut src = String::from("insert parents [");
        for i in 0..300 {
            if i > 0 {
                src.push(',');
            }
            // Keys repeat: every key has two parents.
            src.push_str(&format!(r#"{{ id: "p{i}", key: "k{}", label: "L{i}" }}"#, i % 150));
        }
        src.push(']');
        db.run(&src).unwrap();
        let mut src = String::from("insert children [");
        for i in 0..600 {
            if i > 0 {
                src.push(',');
            }
            src.push_str(&format!(r#"{{ id: "c{i}", parent: "k{}", n: {i} }}"#, i % 150));
        }
        src.push(']');
        db.run(&src).unwrap();
    }
    let cold_dir = tmp();
    let opts = || lin::OpenOpts {
        sync: lin::SyncMode::Full,
        cold: true,
    };
    {
        let mut db = Db::open_with(&cold_dir, opts()).unwrap();
        seed(&mut db);
        db.checkpoint().unwrap();
        db.close().unwrap();
    }
    // `fk` is a session-only declaration: the database must reopen without it, and the
    // session declares it again.
    let mut cold = Db::open_with(&cold_dir, opts()).unwrap();
    cold.run("fk children.parent -> parents.key").unwrap();
    let mut hot = Db::empty();
    seed(&mut hot);
    assert!(cold.store.is_disk_backed("parents"));
    let norm = |db: &mut Db, q: &str| -> Vec<String> {
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
    };
    for q in [
        "children | join parents on parent | take all",
        "children | n < 40 | join parents on parent | take all",
        "children | n < 40 | join parents on parent | { id, parents.label } | take all",
        "children | n >= 590 | join left parents on parent | take all",
    ] {
        let c = norm(&mut cold, q);
        let h = norm(&mut hot, q);
        assert!(!c.is_empty(), "{q}");
        assert_eq!(c, h, "{q}");
    }
    // Each child matches both parents sharing its key.
    assert_eq!(norm(&mut cold, "children | n < 40 | join parents on parent | take all").len(), 80);
    assert!(cold.store.is_disk_backed("parents"), "join promoted parents");
}

/// `fk` and `guard` are session-only declarations. Declaring them must not change the
/// catalog hash stored on disk, otherwise the database cannot be reopened.
#[test]
fn session_only_declarations_do_not_block_reopen() {
    let dir = tmp();
    {
        let mut db = Db::open(&dir).unwrap();
        db.run(
            "col parents { id: text, key: text }\n\
             col children { id: text, parent: text, note: text }\n\
             fk children.parent -> parents.key",
        )
        .unwrap();
        db.run(r#"insert parents { id: "p1", key: "k1" }"#).unwrap();
        db.run(r#"insert children { id: "c1", parent: "k1", note: "n" }"#).unwrap();
        // The declaration is enforced while the session lives.
        assert!(db.run(r#"insert children { id: "c2", parent: "missing" }"#).is_err());
        db.close().unwrap();
    }
    for _ in 0..2 {
        let mut db = Db::open(&dir).unwrap();
        let handle = db.run("children | count").unwrap();
        assert_eq!(handle.rows[0].get("hits"), Some(&lin::Cell::Int(1)));
        drop(handle);
        db.checkpoint().unwrap();
        db.close().unwrap();
    }
}

/// A cursor over a cold collection streams from the disk engine: same rows as the
/// in-memory cursor, without pulling the collection into memory, including rows that
/// only exist in the overlay and rows deleted there.
#[test]
fn cold_cursor_streams_without_materializing() {
    use lin::Queryable;
    use lin::query::pred;
    const N: usize = 9000;
    fn seed(db: &mut Db) {
        for i in 0..7 {
            db.run(&format!(r#"insert users {{ id: "u{i}", email: "u{i}@example.com" }}"#))
                .unwrap();
        }
        for chunk in 0..(N / 1000) {
            let mut src = String::from("insert orders [");
            for i in (chunk * 1000)..((chunk + 1) * 1000) {
                if i > chunk * 1000 {
                    src.push(',');
                }
                src.push_str(&format!(
                    r#"{{ id: "o{i}", user_id: "u{}", total: {i} }}"#,
                    i % 7
                ));
            }
            src.push(']');
            db.run(&src).unwrap();
        }
    }
    fn overlay(db: &mut Db) {
        db.run(r#"insert orders { id: "extra", user_id: "u3", total: 8500 }"#).unwrap();
        db.run(r#"delete orders[id == "o8600"] cas each"#).unwrap();
    }
    let cold_dir = tmp();
    let opts = || lin::OpenOpts {
        sync: lin::SyncMode::Full,
        cold: true,
    };
    {
        let mut db = Db::open_with(&cold_dir, opts()).unwrap();
        seed(&mut db);
        db.checkpoint().unwrap();
        db.close().unwrap();
    }
    let mut cold = Db::open_with(&cold_dir, opts()).unwrap();
    overlay(&mut cold);
    let mut hot = Db::empty();
    seed(&mut hot);
    overlay(&mut hot);
    assert!(cold.store.is_disk_backed("orders"));

    let queries = [
        Queryable::from("orders").take(10),
        Queryable::from("orders").select(["id", "total"]).skip(5).take(20),
        Queryable::from("orders").filter(pred::gt("total", 8400)).take_all(),
        Queryable::from("orders")
            .filter(pred::and(pred::ge("total", 4090), pred::lt("total", 4100)))
            .select(["id", "user_id"])
            .take_all(),
        Queryable::from("orders")
            .filter(pred::or(pred::lt("total", 3), pred::eq("id", "extra")))
            .take_all(),
        Queryable::from("orders").filter(pred::eq("user_id", "u3")).skip(100).take(40),
        Queryable::from("orders").filter(pred::lt("total", 0)).take_all(),
        // A field no row sets: projected as null on both tiers.
        Queryable::from("orders").select(["id", "ts"]).take(3),
    ];
    let key = |r: &lin::Row| -> String {
        r.iter()
            .map(|(k, c)| match (c.text(), c.as_f64()) {
                (Some(t), _) => format!("{k}={t}"),
                (None, Some(n)) => format!("{k}={n}"),
                _ => format!("{k}={c:?}"),
            })
            .collect::<Vec<_>>()
            .join(";")
    };
    for (i, q) in queries.iter().enumerate() {
        let cur = q.cursor(&cold).unwrap();
        assert!(cur.is_lazy(), "query {i}");
        let got: Vec<String> = cur.map(|r| key(&r.unwrap())).collect();
        let want: Vec<String> = q.cursor(&hot).unwrap().map(|r| key(&r.unwrap())).collect();
        // Limited scans without a total order may pick different rows per tier.
        let ordered = matches!(i, 2 | 3 | 4 | 6);
        if ordered {
            let (mut a, mut b) = (got.clone(), want.clone());
            a.sort();
            b.sort();
            assert_eq!(a, b, "query {i}");
        } else {
            assert_eq!(got.len(), want.len(), "query {i}");
        }
        // The cursor agrees with the statement path on the same database.
        let stmt = q.to_vec(&mut cold).unwrap();
        assert_eq!(got.len(), stmt.len(), "query {i}: cursor vs to_vec");
    }
    let deleted = Queryable::from("orders").filter(pred::eq("id", "o8600")).take_all();
    assert_eq!(deleted.cursor(&cold).unwrap().count(), 0);
    assert!(
        !cold.store.disk_rows_materialized("orders"),
        "a cursor pulled the cold collection into memory"
    );
}

/// `insert_batch` takes no query text, so the parser's AST size quota must not cap it:
/// a batch far larger than what an `insert` statement may carry goes in atomically, into
/// memory and into the overlay of a cold collection, and duplicates are still rejected.
#[test]
fn insert_batch_is_not_capped_by_the_ast_quota() {
    use lin::{Cell, RecordBatch};
    const N: usize = 60_000;
    fn batch(ids: std::ops::Range<usize>) -> RecordBatch {
        let names: Arc<[String]> = ["id", "grp", "k", "v"]
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>()
            .into();
        let mut b = RecordBatch::with_capacity(names, ids.len());
        for i in ids {
            b.push_row(&[
                Cell::from_text(&format!("e{i}")),
                Cell::from_text(&format!("g{}", i % 97)),
                Cell::Int((i * 7 % 1000) as i64),
                Cell::Float(i as f64 + 0.5),
            ]);
        }
        b
    }
    let count = |db: &mut Db, q: &str| -> i64 {
        let handle = db.run(q).unwrap();
        match handle.rows[0].get("hits") {
            Some(Cell::Int(n)) => *n,
            other => panic!("{other:?}"),
        }
    };
    let dir = tmp();
    let opts = || lin::OpenOpts {
        sync: lin::SyncMode::Normal,
        cold: true,
    };
    {
        let mut db = Db::open_with(&dir, opts()).unwrap();
        db.run("col ev { id: text, grp: text, k: i64, v: f64 }").unwrap();
        assert_eq!(db.insert_batch("ev", &batch(0..N)).unwrap().n, N);
        assert_eq!(count(&mut db, "ev | count"), N as i64);
        db.checkpoint().unwrap();
        db.close().unwrap();
    }
    let mut db = Db::open_with(&dir, opts()).unwrap();
    assert!(db.store.is_disk_backed("ev"));
    assert_eq!(db.insert_batch("ev", &batch(N..2 * N)).unwrap().n, N);
    assert!(db.store.is_disk_backed("ev"), "insert_batch promoted the collection");
    assert_eq!(count(&mut db, "ev | count"), 2 * N as i64);
    assert_eq!(count(&mut db, "ev | k == 7 | count"), (2 * N / 1000) as i64);
    // One duplicate anywhere in the batch (against a segment row, an overlay row, or
    // inside the batch itself) rejects the whole batch.
    for range in [N - 1..N + 10, 2 * N - 1..2 * N + 10] {
        let err = db.insert_batch("ev", &batch(range)).unwrap_err();
        assert!(err.to_string().contains("duplicate id"), "{err}");
    }
    let mut dup = batch(3 * N..3 * N + 5);
    dup.push_row(&[
        Cell::from_text(&format!("e{}", 3 * N)),
        Cell::from_text("g0"),
        Cell::Int(1),
        Cell::Float(1.5),
    ]);
    assert!(db.insert_batch("ev", &dup).unwrap_err().to_string().contains("duplicate id"));
    assert_eq!(count(&mut db, "ev | count"), 2 * N as i64);

    // More rows than one segment holds still go to the overlay, survive a crash-style
    // reopen (WAL replay, no checkpoint) and then a checkpoint that flushes them.
    const BIG: usize = 150_000;
    assert_eq!(db.insert_batch("ev", &batch(10 * N..10 * N + BIG)).unwrap().n, BIG);
    assert!(db.store.is_disk_backed("ev"), "a large insert_batch promoted the collection");
    let total = (2 * N + BIG) as i64;
    assert_eq!(count(&mut db, "ev | count"), total);
    let copy = tmp();
    fn copy_dir(from: &std::path::Path, to: &std::path::Path) {
        std::fs::create_dir_all(to).unwrap();
        for entry in std::fs::read_dir(from).unwrap() {
            let entry = entry.unwrap();
            let dst = to.join(entry.file_name());
            if entry.file_type().unwrap().is_dir() {
                copy_dir(&entry.path(), &dst);
            } else if entry.file_name() != "LOCK" {
                std::fs::copy(entry.path(), dst).unwrap();
            }
        }
    }
    copy_dir(&dir, &copy);
    let mut replayed = Db::open_with(&copy, opts()).unwrap();
    assert!(replayed.store.is_disk_backed("ev"));
    assert_eq!(count(&mut replayed, "ev | count"), total);
    replayed.checkpoint().unwrap();
    replayed.close().unwrap();
    let mut reopened = Db::open_with(&copy, opts()).unwrap();
    assert_eq!(count(&mut reopened, "ev | count"), total);
    assert_eq!(
        count(&mut reopened, &format!(r#"ev | id == "e{}" | count"#, 10 * N + BIG - 1)),
        1
    );
}

/// Composite and partial unique indexes are enforced on a cold collection straight from
/// the disk engine: every statement gets the same verdict as on in-memory data, and the
/// collection is never pulled into memory.
#[test]
fn cold_composite_and_partial_unique_indexes_match_in_memory() {
    fn seed(db: &mut Db) {
        db.run(
            "col acct { id: text, org: text, name: text, state: text }\n\
             index acct unique [org, name]\n\
             index acct unique [name] where state == \"active\"",
        )
        .unwrap();
        let mut src = String::from("insert acct [");
        for i in 0..100 {
            if i > 0 {
                src.push(',');
            }
            src.push_str(&format!(
                r#"{{ id: "a{i}", org: "o{}", name: "n{i}", state: "{}" }}"#,
                i % 10,
                if i % 2 == 0 { "active" } else { "off" }
            ));
        }
        src.push(']');
        db.run(&src).unwrap();
    }
    let dir = tmp();
    let opts = || lin::OpenOpts {
        sync: lin::SyncMode::Full,
        cold: true,
    };
    {
        let mut db = Db::open_with(&dir, opts()).unwrap();
        seed(&mut db);
        db.checkpoint().unwrap();
        db.close().unwrap();
    }
    let mut cold = Db::open_with(&dir, opts()).unwrap();
    let mut hot = Db::empty();
    seed(&mut hot);
    assert!(cold.store.is_disk_backed("acct"));

    // (statement, expected to succeed)
    let steps: &[(&str, bool)] = &[
        (r#"insert acct { id: "x1", org: "o1", name: "fresh", state: "off" }"#, true),
        // Composite key (o0, n10) exists.
        (r#"insert acct { id: "x2", org: "o0", name: "n10", state: "off" }"#, false),
        // Same name in another org, outside the partial index: allowed.
        (r#"insert acct { id: "x3", org: "o9", name: "n10", state: "off" }"#, true),
        // n10 is already active: the partial index rejects a second active one.
        (r#"insert acct { id: "x4", org: "o8", name: "n10", state: "active" }"#, false),
        // n11 exists but is not active, so the first active n11 is fine...
        (r#"insert acct { id: "x5", org: "o8", name: "n11", state: "active" }"#, true),
        // ...and the second one collides with a row that only lives in the overlay.
        (r#"insert acct { id: "x6", org: "o7", name: "n11", state: "active" }"#, false),
        // A duplicate inside one statement.
        (
            r#"insert acct [{ id: "x7", org: "z", name: "dup", state: "off" }, { id: "x8", org: "z", name: "dup", state: "off" }]"#,
            false,
        ),
        // Updates: move onto an active name, onto a composite key, and into the partial
        // index by changing only the predicate's field.
        (r#"update acct[id == "a20"] cas each { name: "n22" }"#, false),
        (r#"update acct[id == "a21"] cas each { state: "active" }"#, true),
        (r#"update acct[id == "a23"] cas each { name: "n21" }"#, true),
        (r#"update acct[id == "a23"] cas each { state: "active" }"#, false),
        (r#"update acct[id == "a24"] cas each { org: "o5", name: "n25" }"#, false),
        (r#"update acct[id == "a26"] cas each { name: "n26" }"#, true),
        (r#"update acct[id == "a28"] cas each { name: "renamed" }"#, true),
    ];
    for (stmt, ok) in steps {
        let h = hot.run(stmt).map(|_| ()).map_err(|e| e.to_string());
        let c = cold.run(stmt).map(|_| ()).map_err(|e| e.to_string());
        assert_eq!(h.is_ok(), *ok, "in-memory verdict for {stmt}: {h:?}");
        assert_eq!(c.is_ok(), *ok, "cold verdict for {stmt}: {c:?}");
        if let (Err(h), Err(c)) = (&h, &c) {
            assert!(h.contains("unique index") && c.contains("unique index"), "{stmt}: {h} / {c}");
        }
        assert!(cold.store.is_disk_backed("acct"), "{stmt} promoted the collection");
    }
    assert!(!cold.store.disk_rows_materialized("acct"));
    let snapshot = |db: &mut Db| -> Vec<String> {
        let handle = db.run("acct | take all").unwrap();
        let mut rows: Vec<String> = handle
            .rows
            .iter()
            .map(|r| {
                ["id", "org", "name", "state"]
                    .iter()
                    .map(|f| r.get(*f).and_then(lin::Cell::text).unwrap_or("-").to_string())
                    .collect::<Vec<_>>()
                    .join("|")
            })
            .collect();
        rows.sort();
        rows
    };
    assert_eq!(snapshot(&mut cold), snapshot(&mut hot));
}

/// Unique keys that are integers, booleans or simply absent get the same verdict on a
/// cold collection as in memory; float keys fall back to the in-memory path and stay
/// correct there.
#[test]
fn cold_non_text_unique_keys_match_in_memory() {
    fn seed(db: &mut Db) {
        db.run(
            "col slot { id: text, room: text, n: i64, primary: bool, w: f64 }\n\
             index slot unique [room, n]\n\
             index slot unique [room] where primary == true\n\
             col meas { id: text, w: f64 }\n\
             index meas unique [w]",
        )
        .unwrap();
        let mut src = String::from("insert slot [");
        for i in 0..80 {
            if i > 0 {
                src.push(',');
            }
            src.push_str(&format!(
                r#"{{ id: "s{i}", room: "r{}", n: {}, primary: {} }}"#,
                i % 8,
                i / 8,
                i < 8
            ));
        }
        src.push(']');
        db.run(&src).unwrap();
        let mut src = String::from("insert meas [");
        for i in 0..80 {
            if i > 0 {
                src.push(',');
            }
            // 100.75, 101.75, ... and the whole number 7 stored as a float.
            src.push_str(&format!(r#"{{ id: "m{i}", w: {} }}"#, if i == 0 { "7.0".to_string() } else { format!("{}.75", 100 + i) }));
        }
        src.push(']');
        db.run(&src).unwrap();
    }
    let dir = tmp();
    let opts = || lin::OpenOpts {
        sync: lin::SyncMode::Full,
        cold: true,
    };
    {
        let mut db = Db::open_with(&dir, opts()).unwrap();
        seed(&mut db);
        db.checkpoint().unwrap();
        db.close().unwrap();
    }
    let mut cold = Db::open_with(&dir, opts()).unwrap();
    let mut hot = Db::empty();
    seed(&mut hot);
    assert!(cold.store.is_disk_backed("slot"));
    assert!(cold.store.is_disk_backed("meas"));
    let steps: &[&str] = &[
        r#"insert slot { id: "x1", room: "r1", n: 99, primary: false }"#,
        // (r1, 3) exists.
        r#"insert slot { id: "x2", room: "r1", n: 3, primary: false }"#,
        // r2 already has a primary slot.
        r#"insert slot { id: "x3", room: "r2", n: 500, primary: true }"#,
        // A new room, then a second primary in it.
        r#"insert slot { id: "x4", room: "new", n: 1, primary: true }"#,
        r#"insert slot { id: "x5", room: "new", n: 2, primary: true }"#,
        // No `n` at all: a null key part. The second such row in the same room collides.
        r#"insert slot { id: "x6", room: "r3", primary: false }"#,
        r#"insert slot { id: "x7", room: "r3", primary: false }"#,
        r#"insert slot { id: "x8", room: "r4", primary: false }"#,
        // Float keys: unique on their own, equal to a stored integer, and fractional.
        r#"insert meas { id: "f1", w: 2.5 }"#,
        r#"insert meas { id: "f2", w: 2.5 }"#,
        r#"insert meas { id: "f3", w: 105.75 }"#,
        r#"insert meas { id: "f4", w: 3.0 }"#,
        r#"insert meas { id: "f5", w: 3 }"#,
        r#"insert meas { id: "f6", w: 7 }"#,
        r#"insert meas { id: "f7", w: 3.25 }"#,
        r#"update meas[id == "m9"] cas each { w: 2.5 }"#,
        r#"update meas[id == "m9"] cas each { w: 11.5 }"#,
        // Updates onto an integer key and into the partial index.
        r#"update slot[id == "s20"] cas each { n: 9 }"#,
        r#"update slot[id == "s20"] cas each { n: 77 }"#,
        r#"update slot[id == "s21"] cas each { primary: true }"#,
    ];
    for stmt in steps {
        let h = hot.run(stmt).map(|_| ()).map_err(|e| e.to_string());
        let c = cold.run(stmt).map(|_| ()).map_err(|e| e.to_string());
        assert_eq!(c.is_ok(), h.is_ok(), "{stmt}: cold {c:?}, in-memory {h:?}");
        assert!(cold.store.is_disk_backed("slot"), "{stmt} promoted the collection");
        assert!(cold.store.is_disk_backed("meas"), "{stmt} promoted the collection");
    }
    assert!(!cold.store.disk_rows_materialized("slot"));
    assert!(!cold.store.disk_rows_materialized("meas"));
    let meas = |db: &mut Db| -> Vec<String> {
        let handle = db.run("meas | take all").unwrap();
        let mut rows: Vec<String> = handle
            .rows
            .iter()
            .map(|r| {
                format!(
                    "{:?}|{:?}",
                    r.get("id").and_then(lin::Cell::text),
                    r.get("w").and_then(lin::Cell::as_f64)
                )
            })
            .collect();
        rows.sort();
        rows
    };
    let (mc, mh) = (meas(&mut cold), meas(&mut hot));
    assert_eq!(mc, mh);
    // f1, f4 and f7 went in; f2, f3, f5 and f6 collided.
    assert_eq!(mc.len(), 83, "{mc:?}");
    let snapshot = |db: &mut Db| -> Vec<String> {
        let handle = db.run("slot | take all").unwrap();
        let mut rows: Vec<String> = handle
            .rows
            .iter()
            .map(|r| {
                format!(
                    "{:?}|{:?}|{:?}|{:?}",
                    r.get("id").and_then(lin::Cell::text),
                    r.get("room").and_then(lin::Cell::text),
                    r.get("n").and_then(lin::Cell::as_f64),
                    r.get("primary")
                )
            })
            .collect();
        rows.sort();
        rows
    };
    assert_eq!(snapshot(&mut cold), snapshot(&mut hot));
    // At least one of each verdict happened, so the comparison above is not vacuous.
    let count = snapshot(&mut cold).len();
    assert!(count > 80 && count < 80 + 8, "{count}");
}

/// A cold collection with a vector index declared: DML goes to the overlay (the HNSW
/// graph holds no row positions for a disk-backed collection), and vector search keeps
/// seeing both the rows in segments and the rows written afterwards.
#[test]
fn cold_dml_with_vector_index_keeps_search_complete() {
    fn seed(db: &mut Db) {
        let mut src = String::from("insert docs [");
        for i in 0..200 {
            if i > 0 {
                src.push(',');
            }
            let topic = ["wal shipping", "vector search", "zone maps", "group commit"][i % 4];
            src.push_str(&format!(
                r#"{{ uri: "raw://v{i}", title: "{topic} note {i}", body: "{topic} details {i}", layer: "wiki" }}"#
            ));
        }
        src.push(']');
        db.run(&src).unwrap();
    }
    fn writes(db: &mut Db) {
        db.run(r#"insert docs { uri: "raw://fresh", title: "wal shipping fresh", body: "wal shipping fresh details", layer: "wiki" }"#)
            .unwrap();
        db.run(r#"delete docs[uri == "raw://v4"] cas each"#).unwrap();
    }
    let dir = tmp();
    let opts = || lin::OpenOpts {
        sync: lin::SyncMode::Full,
        cold: true,
    };
    {
        let mut db = Db::open_with(&dir, opts()).unwrap();
        seed(&mut db);
        db.checkpoint().unwrap();
        db.close().unwrap();
    }
    let mut cold = Db::open_with(&dir, opts()).unwrap();
    assert!(cold.store.is_disk_backed("docs"));
    // Declared once the collection is on disk: the graph is registered but holds no rows.
    cold.run("index docs [embedding]").unwrap();
    assert!(cold.store.is_disk_backed("docs"), "declaring the index promoted the collection");
    assert_eq!(cold.hnsw_resident_memory("docs").is_some(), true);
    writes(&mut cold);
    assert!(cold.store.is_disk_backed("docs"), "DML promoted a collection with a vector index");
    let mut hot = Db::empty();
    seed(&mut hot);
    hot.run("index docs [embedding]").unwrap();
    writes(&mut hot);

    let uris = |db: &mut Db, q: &str| -> Vec<String> {
        let handle = db.run(q).unwrap();
        let mut out: Vec<String> = handle
            .rows
            .iter()
            .filter_map(|r| r.get("uri").and_then(lin::Cell::text).map(str::to_string))
            .collect();
        out.sort();
        out
    };
    let q = r#"docs | search vec "wal shipping" | take 60"#;
    // A cursor over the cold collection searches it out of core: same rows as the
    // statement, and the collection is not pulled into memory.
    {
        let probe = &cold;
        assert!(!probe.store.disk_rows_materialized("docs"), "docs was pulled into memory earlier");
        let cur = lin::Queryable::from("docs").search_vec("wal shipping").take(60).cursor(probe).unwrap();
        let mut via_cursor: Vec<String> = cur
            .filter_map(|r| r.ok()?.get("uri").and_then(lin::Cell::text).map(str::to_string))
            .collect();
        via_cursor.sort();
        assert!(via_cursor.len() >= 40, "cursor search found {}", via_cursor.len());
        assert!(!probe.store.disk_rows_materialized("docs"), "a search cursor pulled docs into memory");
    }
    let c = uris(&mut cold, q);
    let h = uris(&mut hot, q);
    assert!(c.len() >= 40, "search over the cold collection found only {} rows", c.len());
    assert!(c.contains(&"raw://fresh".to_string()), "the overlay row is missing from search");
    assert!(!c.contains(&"raw://v4".to_string()), "a deleted row is still found");
    assert!(c.contains(&"raw://v0".to_string()), "segment rows are missing from search");
    // The in-memory side answers from the approximate graph, the cold side from an exact
    // scan, so the exact answer contains everything the graph returned.
    assert!(!h.is_empty());
    for uri in &h {
        assert!(c.contains(uri), "{uri} is found in memory but not on the cold collection");
    }

    // If the collection is promoted later, the graph is filled from all its rows: a
    // graph holding only rows inserted after the promotion would hide the rest.
    cold.store.promote_disk_col("docs");
    assert!(!cold.store.is_disk_backed("docs"));
    cold.run(r#"insert docs { uri: "raw://late", title: "wal shipping late", body: "wal shipping late details", layer: "wiki" }"#)
        .unwrap();
    let after = uris(&mut cold, q);
    assert!(after.contains(&"raw://late".to_string()));
    assert!(after.len() >= 40, "after promotion search found only {} rows", after.len());
    assert!(after.contains(&"raw://v0".to_string()) || after.contains(&"raw://v8".to_string()));

    // A checkpoint spills the promoted collection again. Within the session it keeps its
    // in-memory copy and its filled graph; after a reopen the collection is on disk, its
    // graph is empty again, and DML stays in the overlay.
    cold.checkpoint().unwrap();
    cold.close().unwrap();
    drop(cold);
    let mut reopened = Db::open_with(&dir, opts()).unwrap();
    assert!(reopened.store.is_disk_backed("docs"));
    reopened
        .run(r#"insert docs { uri: "raw://after-reopen", title: "wal shipping again", body: "wal shipping again", layer: "wiki" }"#)
        .unwrap();
    assert!(reopened.store.is_disk_backed("docs"), "DML after reopen promoted the collection");
    let found = uris(&mut reopened, q);
    assert!(found.contains(&"raw://after-reopen".to_string()));
    assert!(found.contains(&"raw://late".to_string()));
    assert!(found.len() >= 40);
}

/// `insert ... with edge` inside a multi-statement program and inside a transaction over
/// a cold collection: the collection stays on disk, a failed program or a rolled-back
/// transaction leaves neither its rows nor its edges, and committed ones survive a reopen.
#[test]
fn cold_insert_with_edges_in_program_and_transaction() {
    let dir = tmp();
    let opts = || lin::OpenOpts {
        sync: lin::SyncMode::Full,
        cold: true,
    };
    {
        let mut db = Db::open_with(&dir, opts()).unwrap();
        let mut src = String::from("insert docs [");
        for i in 0..200 {
            if i > 0 {
                src.push(',');
            }
            src.push_str(&format!(
                r#"{{ id: "d{i}", uri: "wiki://page/{i}", title: "page {i}", layer: "wiki" }}"#
            ));
        }
        src.push(']');
        db.run(&src).unwrap();
        db.checkpoint().unwrap();
        db.close().unwrap();
    }
    let mut db = Db::open_with(&dir, opts()).unwrap();
    assert!(db.store.is_disk_backed("docs"));
    let edges_before = db.store.edges.len();
    let edge = |n: &str, target: &str| {
        format!(
            r#"insert docs {{ id: "{n}", uri: "raw://e/{n}", title: "{n}", layer: "raw" }} with edge wikilink -> page "wiki://page/{target}""#
        )
    };

    // A program whose second statement fails: the first statement's row and edge go too.
    let failing = format!("{}\n{}", edge("p1", "1"), r#"insert docs { id: "d5", uri: "raw://dup", title: "dup", layer: "raw" }"#);
    assert!(db.run(&failing).unwrap_err().to_string().contains("duplicate id"));
    assert_eq!(db.store.edges.len(), edges_before, "a failed program left an edge behind");
    assert!(db.store.is_disk_backed("docs"));

    // A program that succeeds.
    db.run(&format!("{}\n{}", edge("p2", "2"), edge("p3", "3"))).unwrap();
    assert_eq!(db.store.edges.len(), edges_before + 2);
    assert!(db.store.is_disk_backed("docs"), "a program with edges promoted the collection");

    // A transaction that is rolled back, then one that commits.
    let err = db
        .transaction_fn(|tx| {
            tx.execute(&edge("t1", "4"))?;
            Err::<(), _>(lin::Error::runtime("abort"))
        })
        .unwrap_err();
    assert!(err.to_string().contains("abort"));
    assert_eq!(db.store.edges.len(), edges_before + 2, "a rolled-back transaction left an edge");
    db.transaction_fn(|tx| {
        tx.execute(&edge("t2", "6"))?;
        tx.execute(&edge("t3", "7"))?;
        Ok(())
    })
    .unwrap();
    assert_eq!(db.store.edges.len(), edges_before + 4);
    assert!(db.store.is_disk_backed("docs"), "a transaction with edges promoted the collection");

    let count = |db: &mut Db, q: &str| -> i64 {
        let handle = db.run(q).unwrap();
        match handle.rows[0].get("hits") {
            Some(lin::Cell::Int(n)) => *n,
            other => panic!("{other:?}"),
        }
    };
    let ids = r#"docs | id in ["p1", "p2", "p3", "t1", "t2", "t3"] | count"#;
    assert_eq!(count(&mut db, ids), 4);
    // The edge of p2 points at the document behind the page target.
    assert!(db.store.edges.iter().any(|e| e.from == "p2" && e.to == "d2"));

    // Crash-style reopen: everything committed comes back from the log.
    let copy = tmp();
    for entry in std::fs::read_dir(&dir).unwrap() {
        let entry = entry.unwrap();
        if entry.file_type().unwrap().is_file() && entry.file_name() != "LOCK" {
            std::fs::copy(entry.path(), copy.join(entry.file_name())).unwrap();
        }
    }
    fn copy_tree(from: &std::path::Path, to: &std::path::Path) {
        std::fs::create_dir_all(to).unwrap();
        for entry in std::fs::read_dir(from).unwrap() {
            let entry = entry.unwrap();
            let dst = to.join(entry.file_name());
            if entry.file_type().unwrap().is_dir() {
                copy_tree(&entry.path(), &dst);
            } else if entry.file_name() != "LOCK" {
                std::fs::copy(entry.path(), dst).unwrap();
            }
        }
    }
    copy_tree(&dir, &copy);
    let mut reopened = Db::open_with(&copy, opts()).unwrap();
    assert_eq!(count(&mut reopened, ids), 4);
    assert_eq!(reopened.store.edges.len(), edges_before + 4);
}

/// `update` may rename a row's `id`, but never onto an id another row holds and never
/// for several rows at once: the primary key stays unique, in memory and on a cold
/// collection alike.
#[test]
fn update_keeps_the_primary_key_unique() {
    fn seed(db: &mut Db) {
        db.run("col acct { id: text, name: text }").unwrap();
        let mut src = String::from("insert acct [");
        for i in 0..120 {
            if i > 0 {
                src.push(',');
            }
            src.push_str(&format!(r#"{{ id: "a{i}", name: "n{}" }}"#, i % 3));
        }
        src.push(']');
        db.run(&src).unwrap();
    }
    let dir = tmp();
    let opts = || lin::OpenOpts {
        sync: lin::SyncMode::Full,
        cold: true,
    };
    {
        let mut db = Db::open_with(&dir, opts()).unwrap();
        seed(&mut db);
        db.checkpoint().unwrap();
        db.close().unwrap();
    }
    let mut cold = Db::open_with(&dir, opts()).unwrap();
    assert!(cold.store.is_disk_backed("acct"));
    let mut hot = Db::empty();
    seed(&mut hot);
    let count = |db: &mut Db, q: &str| -> i64 {
        let handle = db.run(q).unwrap();
        match handle.rows[0].get("hits") {
            Some(lin::Cell::Int(n)) => *n,
            other => panic!("{other:?}"),
        }
    };
    for (label, db) in [("in-memory", &mut hot), ("cold", &mut cold)] {
        // Rename to a free id: the row is found under the new id only, by scan and by
        // point lookup alike.
        db.run(r#"update acct[id == "a1"] cas each { id: "z1" }"#).unwrap();
        assert_eq!(count(db, r#"acct | id == "z1" | count"#), 1, "{label}");
        assert_eq!(count(db, r#"acct | id == "a1" | count"#), 0, "{label}");
        let point = |db: &mut Db, id: &str| -> Vec<String> {
            let handle = db.run(&format!(r#"acct | id == "{id}""#)).unwrap();
            handle
                .rows
                .iter()
                .filter_map(|r| r.get("id").and_then(lin::Cell::text).map(str::to_string))
                .collect()
        };
        assert_eq!(point(db, "z1"), ["z1"], "{label}");
        assert!(point(db, "a1").is_empty(), "{label}: the old id still resolves");
        // Onto an id that is taken.
        let err = db.run(r#"update acct[id == "a2"] cas each { id: "a3" }"#).unwrap_err();
        assert!(err.to_string().contains("duplicate id: a3"), "{label}: {err}");
        // Onto its own id: nothing to reject.
        db.run(r#"update acct[id == "a4"] cas each { id: "a4", name: "kept" }"#).unwrap();
        // Several rows cannot share one new id.
        let err = db.run(r#"update acct[name == "n0"] cas each { id: "same" }"#).unwrap_err();
        assert!(err.to_string().contains("duplicate id: same"), "{label}: {err}");
        assert_eq!(count(db, "acct | count"), 120, "{label}");
        assert_eq!(count(db, r#"acct | id == "a3" | count"#), 1, "{label}");
        assert_eq!(count(db, r#"acct | id == "a2" | count"#), 1, "{label}");
        assert_eq!(count(db, r#"acct | id == "same" | count"#), 0, "{label}");
        assert_eq!(count(db, r#"acct | name == "kept" | count"#), 1, "{label}");
        // Rename a row back onto an id that was freed by an earlier rename.
        db.run(r#"update acct[id == "a5"] cas each { id: "a1" }"#).unwrap();
        assert_eq!(count(db, r#"acct | id == "a1" | count"#), 1, "{label}");
        assert_eq!(count(db, r#"acct | id == "a5" | count"#), 0, "{label}");
    }
    // A rename rolled back with its transaction leaves the old id in place.
    for (label, db) in [("in-memory", &mut hot), ("cold", &mut cold)] {
        let err = db
            .transaction_fn(|tx| {
                tx.execute(r#"update acct[id == "a6"] cas each { id: "gone" }"#)?;
                Err::<(), _>(lin::Error::runtime("abort"))
            })
            .unwrap_err();
        assert!(err.to_string().contains("abort"), "{label}: {err}");
        assert_eq!(count(db, r#"acct | id == "a6" | count"#), 1, "{label}");
        assert_eq!(count(db, r#"acct | id == "gone" | count"#), 0, "{label}");
        let handle = db.run(r#"acct | id == "a6""#).unwrap();
        assert_eq!(handle.rows.len(), 1, "{label}: point lookup after rollback");
    }
    // The cold side renamed through the overlay, without pulling the collection in.
    assert!(cold.store.is_disk_backed("acct"), "renaming an id promoted the collection");
    assert!(!cold.store.disk_rows_materialized("acct"));

    // Crash-style reopen replays the renames from the log; a checkpoint then folds them
    // into segments.
    fn copy_tree(from: &std::path::Path, to: &std::path::Path) {
        std::fs::create_dir_all(to).unwrap();
        for entry in std::fs::read_dir(from).unwrap() {
            let entry = entry.unwrap();
            let dst = to.join(entry.file_name());
            if entry.file_type().unwrap().is_dir() {
                copy_tree(&entry.path(), &dst);
            } else if entry.file_name() != "LOCK" {
                std::fs::copy(entry.path(), dst).unwrap();
            }
        }
    }
    let copy = tmp();
    copy_tree(&dir, &copy);
    for stage in ["replayed", "checkpointed"] {
        let mut db = Db::open_with(&copy, opts()).unwrap();
        assert!(db.store.is_disk_backed("acct"), "{stage}");
        assert_eq!(count(&mut db, "acct | count"), 120, "{stage}");
        for (id, n) in [("z1", 1), ("a1", 1), ("a5", 0), ("a4", 1), ("same", 0)] {
            assert_eq!(count(&mut db, &format!(r#"acct | id == "{id}" | count"#)), n, "{stage}: {id}");
        }
        db.checkpoint().unwrap();
        db.close().unwrap();
    }
}

/// A join cursor over cold collections streams the left side from disk and probes the
/// right side by `id`: same rows as on in-memory data, neither collection pulled into
/// memory, and overlay writes on both sides visible.
#[test]
fn cold_join_cursor_streams_without_materializing() {
    use lin::Queryable;
    use lin::query::pred;
    fn seed(db: &mut Db) {
        let mut src = String::from("insert users [");
        for i in 0..200 {
            if i > 0 {
                src.push(',');
            }
            src.push_str(&format!(r#"{{ id: "u{i}", email: "u{i}@example.com" }}"#));
        }
        src.push(']');
        db.run(&src).unwrap();
        let mut src = String::from("insert orders [");
        for i in 0..3000 {
            if i > 0 {
                src.push(',');
            }
            src.push_str(&format!(r#"{{ id: "o{i}", user_id: "u{}", total: {i} }}"#, i % 200));
        }
        src.push(']');
        db.run(&src).unwrap();
    }
    fn overlay(db: &mut Db) {
        db.run(r#"insert users { id: "u999", email: "late@example.com" }"#).unwrap();
        db.run(r#"insert orders { id: "o9999", user_id: "u999", total: 99999 }"#).unwrap();
        db.run(r#"delete orders[id == "o2999"] cas each"#).unwrap();
    }
    let dir = tmp();
    let opts = || lin::OpenOpts {
        sync: lin::SyncMode::Full,
        cold: true,
    };
    {
        let mut db = Db::open_with(&dir, opts()).unwrap();
        seed(&mut db);
        db.checkpoint().unwrap();
        db.close().unwrap();
    }
    let mut hot = Db::empty();
    seed(&mut hot);
    overlay(&mut hot);
    {
        let mut db = Db::open_with(&dir, opts()).unwrap();
        overlay(&mut db);
        db.close().unwrap();
    }
    let key = |r: &lin::Row| -> String {
        r.iter()
            .map(|(k, c)| match (c.text(), c.as_f64()) {
                (Some(t), _) => format!("{k}={t}"),
                (None, Some(n)) => format!("{k}={n}"),
                _ => format!("{k}={c:?}"),
            })
            .collect::<Vec<_>>()
            .join(";")
    };
    let queries = [
        ("filtered", Queryable::from("orders").filter(pred::gt("total", 2990)).join("users", "user_id").take_all(), true),
        ("projected", Queryable::from("orders").filter(pred::lt("total", 40)).join("users", "user_id").select(["id", "users.email"]).take_all(), true),
        ("limited", Queryable::from("orders").join("users", "user_id").skip(5).take(10), false),
        ("overlay rows", Queryable::from("orders").filter(pred::eq("id", "o9999")).join("users", "user_id").take_all(), true),
    ];
    for (label, q, ordered) in queries {
        // A fresh handle per query: materialization, once it happens, sticks.
        let db = Db::open_with(&dir, opts()).unwrap();
        let cur = q.cursor(&db).unwrap();
        assert!(cur.is_lazy(), "{label}");
        let mut got: Vec<String> = cur.map(|r| key(&r.unwrap())).collect();
        let mut want: Vec<String> = q.cursor(&hot).unwrap().map(|r| key(&r.unwrap())).collect();
        assert!(!got.is_empty(), "{label}");
        if ordered {
            got.sort();
            want.sort();
            assert_eq!(got, want, "{label}");
        } else {
            assert_eq!(got.len(), want.len(), "{label}");
        }
        assert!(!db.store.disk_rows_materialized("orders"), "{label}: orders pulled into memory");
        assert!(!db.store.disk_rows_materialized("users"), "{label}: users pulled into memory");
    }
}

/// `insert_batch` into a cold collection takes the columnar path: it must accept and
/// reject exactly what the equivalent `insert` statement does (unknown fields, type
/// mismatches, duplicate ids), leave null cells out as absent fields, and keep the
/// collection on disk.
#[test]
fn cold_insert_batch_matches_insert_statement() {
    use lin::{Cell, RecordBatch};
    fn batch(names: &[&str], rows: &[Vec<Cell>]) -> RecordBatch {
        let names: Arc<[String]> =
            names.iter().map(|s| s.to_string()).collect::<Vec<_>>().into();
        let mut b = RecordBatch::with_capacity(names, rows.len());
        for r in rows {
            b.push_row(r);
        }
        b
    }
    fn seed(db: &mut Db) {
        db.run("col ev { id: text, grp: text, k: i64, v: f64, ok: bool }").unwrap();
        let mut src = String::from("insert ev [");
        for i in 0..300 {
            if i > 0 {
                src.push(',');
            }
            src.push_str(&format!(r#"{{ id: "e{i}", grp: "g{}", k: {i}, v: {i}.5, ok: true }}"#, i % 7));
        }
        src.push(']');
        db.run(&src).unwrap();
    }
    let dir = tmp();
    let opts = || lin::OpenOpts {
        sync: lin::SyncMode::Full,
        cold: true,
    };
    {
        let mut db = Db::open_with(&dir, opts()).unwrap();
        seed(&mut db);
        db.checkpoint().unwrap();
        db.close().unwrap();
    }
    let mut cold = Db::open_with(&dir, opts()).unwrap();
    assert!(cold.store.is_disk_backed("ev"));
    let mut hot = Db::empty();
    seed(&mut hot);

    let t = |s: &str| Cell::from_text(s);
    let cases: Vec<(&str, RecordBatch, bool)> = vec![
        (
            "plain",
            batch(
                &["id", "grp", "k", "v", "ok"],
                &[
                    vec![t("n1"), t("g1"), Cell::Int(1), Cell::Float(1.5), Cell::Bool(false)],
                    vec![t("n2"), t("g2"), Cell::Int(2), Cell::Float(2.5), Cell::Bool(true)],
                ],
            ),
            true,
        ),
        (
            "nulls are absent fields",
            batch(&["id", "grp", "k"], &[vec![t("n3"), Cell::Null, Cell::Int(3)], vec![t("n4"), t("g4"), Cell::Null]]),
            true,
        ),
        // An integer is accepted for an f64 field, as in the statement.
        ("int for f64", batch(&["id", "v"], &[vec![t("n5"), Cell::Int(5)]]), true),
        ("unknown field", batch(&["id", "nope"], &[vec![t("n6"), Cell::Int(1)]]), false),
        ("text for i64", batch(&["id", "k"], &[vec![t("n7"), t("seven")]]), false),
        ("bool for text", batch(&["id", "grp"], &[vec![t("n8"), Cell::Bool(true)]]), false),
        ("duplicate of a segment row", batch(&["id"], &[vec![t("n9")], vec![t("e5")]]), false),
        ("duplicate inside the batch", batch(&["id"], &[vec![t("n10")], vec![t("n10")]]), false),
        // Mixed kinds in one column: every kind is checked, not just the first.
        ("mixed column", batch(&["id", "k"], &[vec![t("n11"), Cell::Int(1)], vec![t("n12"), t("x")]]), false),
    ];
    for (label, b, ok) in &cases {
        let c = cold.insert_batch("ev", b).map(|d| d.n).map_err(|e| e.to_string());
        let h = hot.insert_batch("ev", b).map(|d| d.n).map_err(|e| e.to_string());
        assert_eq!(c.is_ok(), *ok, "{label}: cold {c:?}");
        assert_eq!(h.is_ok(), *ok, "{label}: in-memory {h:?}");
        if let (Err(c), Err(h)) = (&c, &h) {
            assert_eq!(c, h, "{label}: different errors");
        }
        assert!(cold.store.is_disk_backed("ev"), "{label} promoted the collection");
    }
    let snapshot = |db: &mut Db| -> Vec<String> {
        let handle = db.run("ev | take all").unwrap();
        let mut rows: Vec<String> = handle
            .rows
            .iter()
            .map(|r| {
                let mut keys: Vec<String> = r
                    .iter()
                    .filter(|(k, _)| k.as_str() != "embedding")
                    .map(|(k, c)| match (c.text(), c.as_f64()) {
                        (Some(s), _) => format!("{k}={s}"),
                        (None, Some(n)) => format!("{k}={n}"),
                        _ => format!("{k}={c:?}"),
                    })
                    .collect();
                keys.sort();
                keys.join(";")
            })
            .collect();
        rows.sort();
        rows
    };
    assert_eq!(snapshot(&mut cold), snapshot(&mut hot));
    // Absent fields stay absent on disk too.
    let handle = cold.run(r#"ev | id == "n3""#).unwrap();
    assert!(!handle.rows[0].contains_key("grp") || handle.rows[0].get("grp") == Some(&Cell::Null));
}

/// `hop` from a cold collection, as a statement and as a cursor: the reached documents
/// are read by point lookups on the engine, so the collection stays on disk, and the
/// result matches the same walk over in-memory data.
#[test]
fn cold_hop_resolves_targets_without_materializing() {
    use lin::Queryable;
    use lin::query::pred;
    fn seed(db: &mut Db) {
        let mut src = String::from("insert docs [");
        for i in 0..300 {
            if i > 0 {
                src.push(',');
            }
            src.push_str(&format!(
                r#"{{ id: "d{i}", uri: "wiki://p/{i}", title: "page {i}", layer: "wiki" }}"#
            ));
        }
        src.push(']');
        db.run(&src).unwrap();
        for i in 0..20 {
            db.run(&format!(
                r#"insert docs {{ id: "n{i}", uri: "raw://n/{i}", title: "note {i}", layer: "raw" }} with edge wikilink -> page "wiki://p/{}""#,
                i * 3
            ))
            .unwrap();
        }
    }
    let dir = tmp();
    let opts = || lin::OpenOpts {
        sync: lin::SyncMode::Full,
        cold: true,
    };
    {
        let mut db = Db::open_with(&dir, opts()).unwrap();
        seed(&mut db);
        db.checkpoint().unwrap();
        db.close().unwrap();
    }
    let mut hot = Db::empty();
    seed(&mut hot);
    let ids = |rows: &[lin::Row]| -> Vec<String> {
        let mut v: Vec<String> = rows
            .iter()
            .filter_map(|r| r.get("id").and_then(lin::Cell::text).map(str::to_string))
            .collect();
        v.sort();
        v
    };
    let q = r#"docs | layer == "raw" | hop wikilink | take all"#;
    let want = ids(&hot.run(q).unwrap().rows);
    assert_eq!(want.len(), 20);
    {
        let mut db = Db::open_with(&dir, opts()).unwrap();
        let got = ids(&db.run(q).unwrap().rows);
        assert_eq!(got, want);
        assert!(!db.store.disk_rows_materialized("docs"), "a hop statement pulled docs into memory");
    }
    {
        let db = Db::open_with(&dir, opts()).unwrap();
        let rows: Vec<lin::Row> = Queryable::from("docs")
            .filter(pred::eq("layer", "raw"))
            .hop("wikilink")
            .take_all()
            .cursor(&db)
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        assert_eq!(ids(&rows), want);
        assert!(!db.store.disk_rows_materialized("docs"), "a hop cursor pulled docs into memory");
    }
}

/// Lexical search over a cold collection, as a statement and as a cursor: hits are read
/// by `id` from the engine, so the collection stays on disk, and the results match the
/// same search over in-memory data, with and without a projection, `skip` and `take`.
#[test]
fn cold_lexical_search_without_materializing() {
    use lin::Queryable;
    fn seed(db: &mut Db) {
        let mut src = String::from("insert docs [");
        for i in 0..400 {
            if i > 0 {
                src.push(',');
            }
            let topic = ["wal shipping", "vector search", "zone maps", "group commit"][i % 4];
            src.push_str(&format!(
                r#"{{ id: "d{i}", uri: "wiki://p/{i}", title: "{topic} {i}", body: "notes on {topic} number {i}", layer: "wiki" }}"#
            ));
        }
        src.push(']');
        db.run(&src).unwrap();
    }
    let dir = tmp();
    let opts = || lin::OpenOpts {
        sync: lin::SyncMode::Full,
        cold: true,
    };
    {
        let mut db = Db::open_with(&dir, opts()).unwrap();
        seed(&mut db);
        db.checkpoint().unwrap();
        db.close().unwrap();
    }
    let mut hot = Db::empty();
    seed(&mut hot);
    let ids = |rows: &[lin::Row]| -> Vec<String> {
        let mut v: Vec<String> = rows
            .iter()
            .filter_map(|r| r.get("id").and_then(lin::Cell::text).map(str::to_string))
            .collect();
        v.sort();
        v
    };
    for q in [
        r#"docs | search lex "shipping" | take all"#,
        r#"docs | search lex "zone maps" | { id, title } | take all"#,
        r#"docs | search lex "nothingmatches" | take all"#,
    ] {
        let want = ids(&hot.run(q).unwrap().rows);
        let mut db = Db::open_with(&dir, opts()).unwrap();
        let handle = db.run(q).unwrap();
        assert_eq!(ids(&handle.rows), want, "{q}");
        if q.contains("{ id, title }") {
            let keys: Vec<&str> = handle.rows[0].keys().map(String::as_str).collect();
            assert_eq!(keys, ["id", "title"], "{q}");
        }
        drop(handle);
        assert!(!db.store.disk_rows_materialized("docs"), "{q}: docs pulled into memory");
    }
    // Ranked and limited: the same number of rows, all of them real hits.
    {
        let mut db = Db::open_with(&dir, opts()).unwrap();
        let q = r#"docs | search lex "shipping" | skip 3 | take 10"#;
        let in_order = |rows: &[lin::Row]| -> Vec<String> {
            rows.iter()
                .filter_map(|r| r.get("id").and_then(lin::Cell::text).map(str::to_string))
                .collect()
        };
        let got = in_order(&db.run(q).unwrap().rows);
        assert_eq!(got.len(), 10);
        assert_eq!(got, in_order(&hot.run(q).unwrap().rows), "ranking differs from in-memory");
        assert!(!db.store.disk_rows_materialized("docs"));
    }
    // The cursor takes the same path.
    {
        let db = Db::open_with(&dir, opts()).unwrap();
        let rows: Vec<lin::Row> = Queryable::from("docs")
            .search_lex("shipping")
            .take_all()
            .cursor(&db)
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        assert_eq!(ids(&rows), ids(&hot.run(r#"docs | search lex "shipping" | take all"#).unwrap().rows));
        assert!(!db.store.disk_rows_materialized("docs"), "a lexical cursor pulled docs into memory");
    }
    // Hybrid search merges the lexical and vector legs by reciprocal rank: the same set of
    // documents as in memory, still without loading the collection.
    for q in [
        r#"docs | search "shipping" | take all"#,
        r#"docs | search "zone maps" | { id, title } | take all"#,
    ] {
        let want = ids(&hot.run(q).unwrap().rows);
        let mut db = Db::open_with(&dir, opts()).unwrap();
        let got = ids(&db.run(q).unwrap().rows);
        assert!(!want.is_empty(), "{q}");
        assert_eq!(got, want, "{q}");
        assert!(!db.store.disk_rows_materialized("docs"), "{q}: docs pulled into memory");
    }
    {
        let mut db = Db::open_with(&dir, opts()).unwrap();
        let q = r#"docs | search "shipping" | take 10"#;
        let got = ids(&db.run(q).unwrap().rows);
        let all = ids(&hot.run(r#"docs | search "shipping" | take all"#).unwrap().rows);
        assert_eq!(got.len(), 10);
        assert!(got.iter().all(|id| all.contains(id)), "{got:?}");
        let rows: Vec<lin::Row> = Queryable::from("docs")
            .search("shipping")
            .take(10)
            .cursor(&db)
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        assert_eq!(ids(&rows), got, "cursor and statement differ");
        assert!(!db.store.disk_rows_materialized("docs"), "hybrid search pulled docs into memory");
    }
    // After a write the index is rebuilt from the rows; the answer must stay correct.
    {
        let mut db = Db::open_with(&dir, opts()).unwrap();
        db.run(r#"insert docs { id: "fresh", uri: "raw://fresh", title: "wal shipping fresh", body: "fresh shipping note", layer: "raw" }"#).unwrap();
        hot.run(r#"insert docs { id: "fresh", uri: "raw://fresh", title: "wal shipping fresh", body: "fresh shipping note", layer: "raw" }"#).unwrap();
        let q = r#"docs | search lex "shipping" | take all"#;
        assert_eq!(ids(&db.run(q).unwrap().rows), ids(&hot.run(q).unwrap().rows));
        // The dropped index is rebuilt from the engine, not from loaded rows, and keeps
        // following later writes, deletes included.
        assert!(!db.store.disk_rows_materialized("docs"), "search after a write loaded docs");
        for stmt in [
            r#"delete docs[id == "d0"] cas each"#,
            r#"insert docs { id: "fresh2", uri: "raw://fresh2", title: "shipping again", layer: "raw" }"#,
        ] {
            db.run(stmt).unwrap();
            hot.run(stmt).unwrap();
            for q in [
                r#"docs | search lex "shipping" | take all"#,
                r#"docs | search "shipping" | take all"#,
            ] {
                assert_eq!(ids(&db.run(q).unwrap().rows), ids(&hot.run(q).unwrap().rows), "{stmt}: {q}");
            }
            assert!(!db.store.disk_rows_materialized("docs"), "{stmt}: docs pulled into memory");
        }
    }
}

/// `insert_batch` into an in-memory collection runs as an `InsertRows` statement built
/// straight from the columns: the same rows, errors, generated ids, embeddings and log
/// records as the equivalent `insert`, before and after a reopen that replays the log.
#[test]
fn memory_insert_batch_matches_insert_statement() {
    use lin::{Cell, RecordBatch};
    fn batch(names: &[&str], rows: &[Vec<Cell>]) -> RecordBatch {
        let names: Arc<[String]> =
            names.iter().map(|s| s.to_string()).collect::<Vec<_>>().into();
        let mut b = RecordBatch::with_capacity(names, rows.len());
        for r in rows {
            b.push_row(r);
        }
        b
    }
    fn statement(collection: &str, b: &RecordBatch) -> String {
        let rows: Vec<String> = b
            .to_rows()
            .iter()
            .map(|r| {
                let fields: Vec<String> = r
                    .iter()
                    .filter(|(_, c)| !matches!(c, Cell::Null))
                    .map(|(k, c)| match c {
                        Cell::Int(n) => format!("{k}: {n}"),
                        Cell::Float(x) => format!("{k}: {x:?}"),
                        Cell::Bool(v) => format!("{k}: {v}"),
                        other => format!("{k}: {:?}", other.text().unwrap_or("")),
                    })
                    .collect();
                format!("{{ {} }}", fields.join(", "))
            })
            .collect();
        format!("insert {collection} [{}]", rows.join(", "))
    }
    let t = |s: &str| Cell::from_text(s);
    let big: Vec<Vec<Cell>> = (0..3000)
        .map(|i| vec![t(&format!("b{i}")), t(&format!("g{}", i % 9)), Cell::Int(i), Cell::Float(i as f64 + 0.25)])
        .collect();
    let cases: Vec<(&str, &str, RecordBatch, bool)> = vec![
        ("plain", "ev", batch(&["id", "grp", "k", "v"], &big), true),
        (
            "absent fields and generated ids",
            "ev",
            batch(&["id", "grp", "k"], &[vec![Cell::Null, t("g1"), Cell::Int(1)], vec![t("n2"), Cell::Null, Cell::Int(2)], vec![Cell::Null, t("g3"), Cell::Null]]),
            true,
        ),
        (
            "documents get embeddings and hashes",
            "docs",
            batch(&["uri", "title", "body", "layer"], &[vec![t("raw://a"), t("A"), t("alpha body"), t("raw")], vec![t("raw://b"), t("B"), t("beta body"), t("raw")]]),
            true,
        ),
        ("duplicate id", "ev", batch(&["id"], &[vec![t("x1")], vec![t("b7")]]), false),
        ("duplicate uri", "docs", batch(&["uri", "title", "layer"], &[vec![t("raw://a"), t("again"), t("raw")]]), false),
        ("type mismatch", "ev", batch(&["id", "k"], &[vec![t("x2"), t("seven")]]), false),
        ("unknown field", "ev", batch(&["id", "nope"], &[vec![t("x3"), Cell::Int(1)]]), false),
    ];
    let dir_batch = tmp();
    let dir_stmt = tmp();
    let snapshot = |db: &mut Db| -> Vec<String> {
        let mut out = Vec::new();
        for q in ["ev | take all", "docs | take all"] {
            let handle = db.run(q).unwrap();
            let mut rows: Vec<String> = handle
                .rows
                .iter()
                .map(|r| {
                    r.iter()
                        // Generated ids differ between the two databases only by
                        // allocation order, which is the same here; keep them.
                        .map(|(k, c)| match (c.text(), c.as_f64(), c.as_vec()) {
                            (Some(s), _, _) => format!("{k}={s}"),
                            (None, Some(n), _) => format!("{k}={n}"),
                            (_, _, Some(v)) => format!("{k}=vec{}", v.len()),
                            _ => format!("{k}={c:?}"),
                        })
                        .collect::<Vec<_>>()
                        .join(";")
                })
                .collect();
            rows.sort();
            out.push(rows.join("\n"));
        }
        out
    };
    {
        let mut by_batch = Db::open(&dir_batch).unwrap();
        let mut by_stmt = Db::open(&dir_stmt).unwrap();
        for db in [&mut by_batch, &mut by_stmt] {
            db.run("col ev { id: text, grp: text, k: i64, v: f64 }").unwrap();
        }
        for (label, collection, b, ok) in &cases {
            let gen_before = by_batch.r#gen();
            let got = by_batch.insert_batch(collection, b).map(|d| d.n);
            let want = by_stmt.run(&statement(collection, b)).map(|h| h.done.n);
            assert_eq!(got.is_ok(), *ok, "{label}: insert_batch {got:?}");
            assert_eq!(want.is_ok(), *ok, "{label}: statement {want:?}");
            match (&got, &want) {
                (Ok(g), Ok(w)) => assert_eq!(g, w, "{label}"),
                (Err(g), Err(w)) => {
                    assert_eq!(g.to_string(), w.to_string(), "{label}");
                    assert_eq!(by_batch.r#gen(), gen_before, "{label}: a failed batch moved the generation");
                }
                _ => {}
            }
        }
        assert_eq!(snapshot(&mut by_batch), snapshot(&mut by_stmt));
        // Nothing stays staged after a failed batch: a later statement is unaffected.
        by_batch.run(r#"insert ev { id: "after", k: 1 }"#).unwrap();
        by_stmt.run(r#"insert ev { id: "after", k: 1 }"#).unwrap();
        assert_eq!(snapshot(&mut by_batch), snapshot(&mut by_stmt));
    }
    // A reopen replays both logs to the same state.
    let mut by_batch = Db::open(&dir_batch).unwrap();
    let mut by_stmt = Db::open(&dir_stmt).unwrap();
    assert_eq!(snapshot(&mut by_batch), snapshot(&mut by_stmt));
}
