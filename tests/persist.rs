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
        assert!(dir.join("cold").join("docs.bin").exists());
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
        assert!(dir.join("cold").join("docs.bin").exists());
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
