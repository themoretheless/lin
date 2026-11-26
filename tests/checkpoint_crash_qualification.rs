//! A crash at any point of a checkpoint leaves a database that opens with exactly the
//! committed data. A checkpoint writes, in order: the full-text index file, the snapshot
//! (through a temporary file and a rename), `head`, then truncates the log. Each crash
//! image below takes the files written so far from after the checkpoint and the rest
//! from before it, optionally with the temporary files a crash would leave behind.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use lin::Db;

static SEQ: AtomicU64 = AtomicU64::new(0);

fn tmp() -> PathBuf {
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("lin-ckpt-crash-{}-{n}", std::process::id()));
    let _ = fs::remove_dir_all(&p);
    fs::create_dir_all(&p).unwrap();
    p
}

/// Every regular file of a data directory except the lock, by name.
fn files(dir: &Path) -> Vec<(String, Vec<u8>)> {
    let mut out: Vec<(String, Vec<u8>)> = fs::read_dir(dir)
        .unwrap()
        .filter_map(|e| {
            let e = e.unwrap();
            let name = e.file_name().to_string_lossy().to_string();
            (e.file_type().unwrap().is_file() && name != "LOCK")
                .then(|| (name, fs::read(e.path()).unwrap()))
        })
        .collect();
    out.sort();
    out
}

fn statements(phase: usize) -> Vec<String> {
    let mut out = Vec::new();
    if phase == 0 {
        out.push("col acct { id: text, org: text, n: i64 }".to_string());
        out.push("index acct [org]".to_string());
    }
    let base = phase * 100;
    let mut acct = String::from("insert acct [");
    for i in 0..80 {
        if i > 0 {
            acct.push(',');
        }
        acct.push_str(&format!(r#"{{ id: "a{}", org: "o{}", n: {i} }}"#, base + i, i % 5));
    }
    acct.push(']');
    out.push(acct);
    for i in 0..12 {
        let topic = ["checkpoint crash", "vector search", "graph walk"][i % 3];
        out.push(format!(
            r#"insert docs {{ uri: "raw://c/{}", title: "Doc {} {topic}", layer: "raw", body: "phase {phase} text about {topic}" }}"#,
            base + i,
            base + i
        ));
    }
    out.push(format!(r#"append edge wikilink "raw://c/{base}" -> "raw://c/{}""#, base + 1));
    out.push(format!(r#"append facts {{ s: "phase", p: tagged, o: "p{phase}" }}"#));
    if phase == 1 {
        out.push(r#"update acct[org == "o2"] cas each { n: -5 }"#.to_string());
        out.push(r#"delete docs[uri == "raw://c/3"] cas each"#.to_string());
        out.push("col extra { id: text }".to_string());
        out.push(r#"insert extra { id: "e1" }"#.to_string());
    }
    out
}

fn answers(db: &mut Db) -> Vec<String> {
    let mut out = Vec::new();
    for q in [
        "acct | { id, org, n } | take all",
        "acct | org == \"o2\" | count",
        "docs | { uri, title } | take all",
        r#"docs | search lex "checkpoint" | { uri } | take all"#,
        r#"docs | search vec "graph walk" | { uri } | take 4"#,
        r#"docs | uri == "raw://c/100" | hop wikilink | { uri } | take all"#,
        "facts | s == \"phase\" | take all",
        "extra | take all",
    ] {
        let rows = match db.run(q) {
            Ok(h) => h.rows.clone(),
            Err(e) => {
                out.push(format!("{q}: error {e}"));
                continue;
            }
        };
        let mut keys: Vec<String> = rows
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
        // Search ranking is part of the answer; everything else is a set.
        if !q.contains("search") {
            keys.sort();
        }
        out.push(format!("{q}: {}", keys.join(" | ")));
    }
    out
}

#[test]
fn crash_at_every_checkpoint_step_recovers_committed_data() {
    // Reference: the same statements, never checkpointed and never closed.
    let reference_dir = tmp();
    let mut reference = Db::open(&reference_dir).unwrap();
    for phase in 0..2 {
        for stmt in statements(phase) {
            reference.run(&stmt).unwrap();
        }
    }
    let want = answers(&mut reference);
    assert!(want.iter().all(|a| !a.contains("error")), "{want:?}");

    // Phase 0 is checkpointed earlier, so "before" already has a snapshot and an index
    // file; phase 1 is in the log only.
    let dir = tmp();
    let mut db = Db::open(&dir).unwrap();
    for stmt in statements(0) {
        db.run(&stmt).unwrap();
    }
    db.checkpoint().unwrap();
    for stmt in statements(1) {
        db.run(&stmt).unwrap();
    }
    let before = files(&dir);
    db.checkpoint().unwrap();
    let after = files(&dir);
    drop(db);
    let log_after = after.iter().find(|(n, _)| n == "log").map(|(_, b)| b.len());
    assert_eq!(log_after, Some(0), "the checkpoint did not truncate the log");

    // Steps of the checkpoint, in order, by the files each one writes.
    let steps: [&[&str]; 4] = [&["fts"], &["snapshot"], &["head"], &["log"]];
    let get = |set: &[(String, Vec<u8>)], name: &str| -> Option<Vec<u8>> {
        set.iter().find(|(n, _)| n == name).map(|(_, b)| b.clone())
    };
    for done in 0..=steps.len() {
        for leftover in [false, true] {
            let image = tmp();
            for (name, bytes) in &before {
                fs::write(image.join(name), bytes).unwrap();
            }
            for step in &steps[..done] {
                for name in *step {
                    match get(&after, name) {
                        Some(bytes) => fs::write(image.join(name), bytes).unwrap(),
                        None => {
                            let _ = fs::remove_file(image.join(name));
                        }
                    }
                }
            }
            if leftover {
                // A crash inside the next step leaves its temporary file behind.
                if let Some(snap) = get(&after, "snapshot") {
                    fs::write(image.join("snapshot.bin.tmp"), &snap[..snap.len() / 2]).unwrap();
                }
                fs::write(image.join("head.tmp"), b"{\"partial").unwrap();
            }
            let what = format!("crash after {done} checkpoint steps, leftover temp files: {leftover}");
            let mut recovered = Db::open(&image).unwrap_or_else(|e| panic!("{what}: open: {e}"));
            assert_eq!(answers(&mut recovered), want, "{what}");
            // The recovered database keeps working and checkpoints again.
            recovered
                .run(r#"insert acct { id: "after-crash", org: "o9", n: 1 }"#)
                .unwrap();
            recovered.checkpoint().unwrap();
            recovered.close().unwrap();
            drop(recovered);
            let mut reopened = Db::open(&image).unwrap();
            let rows = reopened.run(r#"acct | id == "after-crash" | count"#).unwrap();
            assert_eq!(
                rows.rows[0].get("hits").and_then(|c| c.as_f64()),
                Some(1.0),
                "{what}: write after recovery"
            );
        }
    }
}

/// Checkpointing is idempotent: repeating it with no writes in between, and reopening
/// between checkpoints, leaves the same answers, and a checkpoint with nothing new to
/// write produces the same snapshot bytes.
#[test]
fn repeated_checkpoints_are_idempotent() {
    let dir = tmp();
    let mut db = Db::open(&dir).unwrap();
    for stmt in statements(0) {
        db.run(&stmt).unwrap();
    }
    db.checkpoint().unwrap();
    let want = answers(&mut db);
    let snap_of = |dir: &Path| -> Vec<u8> {
        files(dir).into_iter().find(|(n, _)| n.starts_with("snapshot")).map(|(_, b)| b).unwrap_or_default()
    };
    let first = snap_of(&dir);
    db.checkpoint().unwrap();
    db.checkpoint().unwrap();
    assert_eq!(answers(&mut db), want);
    assert_eq!(snap_of(&dir), first, "a checkpoint with nothing new changed the snapshot");
    db.close().unwrap();
    drop(db);
    for _ in 0..2 {
        let mut db = Db::open(&dir).unwrap();
        assert_eq!(answers(&mut db), want, "after reopen");
        db.checkpoint().unwrap();
        db.close().unwrap();
    }
}

#[test]
fn seeded_recovery_and_replica_replay_match_model() {
    use std::collections::BTreeMap;

    for seed in [1_u64, 7, 42, 0xfeed] {
        let mut dir = tmp();
        let mut primary = Db::open(&dir).unwrap();
        let mut replica = Db::fixture();
        let mut model = BTreeMap::new();
        let mut random = seed;
        primary.run("col sim { id: text, n: i64 }").unwrap();
        for step in 0..96 {
            random ^= random << 13;
            random ^= random >> 7;
            random ^= random << 17;
            let id = format!("k{}", (random >> 8) % 12);
            match random % 8 {
                0 => {
                    ship(&primary, &mut replica);
                    primary.checkpoint().unwrap();
                }
                1 => {
                    let image = tmp();
                    for (name, bytes) in files(&dir) {
                        fs::write(image.join(name), bytes).unwrap();
                    }
                    let frames = primary.export_wal_since(replica.r#gen()).unwrap();
                    if frames.len() > 3 && random & 0x10 != 0 {
                        use std::io::Write;
                        fs::OpenOptions::new()
                            .append(true)
                            .open(image.join(lin::persist::LOG_NAME))
                            .unwrap()
                            .write_all(&frames[..3])
                            .unwrap();
                    }
                    let mut recovered = Db::open(&image).unwrap();
                    assert_eq!(
                        recovered.r#gen(),
                        primary.r#gen(),
                        "seed={seed}, step={step}"
                    );
                    assert_eq!(
                        read_model(&mut recovered),
                        model,
                        "seed={seed}, step={step}"
                    );
                    ship(&recovered, &mut replica);
                    primary.close().unwrap();
                    fs::remove_dir_all(dir).unwrap();
                    primary = recovered;
                    dir = image;
                }
                2 => {
                    if model.remove(&id).is_some() {
                        primary
                            .run(&format!(r#"delete sim[id == "{id}"] cas each"#))
                            .unwrap();
                    }
                }
                3 => ship(&primary, &mut replica),
                _ => {
                    if model.insert(id.clone(), step).is_some() {
                        primary
                            .run(&format!(
                                r#"update sim[id == "{id}"] cas each {{ n: {step} }}"#
                            ))
                            .unwrap();
                    } else {
                        primary
                            .run(&format!(r#"insert sim {{ id: "{id}", n: {step} }}"#))
                            .unwrap();
                    }
                }
            }
            assert_eq!(read_model(&mut primary), model, "seed={seed}, step={step}");
            if replica.r#gen() == primary.r#gen() {
                assert_eq!(
                    read_model(&mut replica),
                    model,
                    "replica seed={seed}, step={step}"
                );
            }
        }
        ship(&primary, &mut replica);
        assert_eq!(read_model(&mut replica), model, "replica seed={seed}");
        primary.close().unwrap();
        fs::remove_dir_all(dir).unwrap();
    }

    fn ship(primary: &Db, replica: &mut Db) {
        let frames = primary.export_wal_since(replica.r#gen()).unwrap();
        replica.apply_wal(&frames).unwrap();
        assert_eq!(replica.r#gen(), primary.r#gen());
        assert_eq!(replica.apply_wal(&frames).unwrap(), 0);
    }

    fn read_model(db: &mut Db) -> BTreeMap<String, i64> {
        db.run("sim | take all")
            .unwrap()
            .rows
            .iter()
            .map(|row| {
                let id = row.get("id").unwrap().text().unwrap().to_owned();
                let lin::Cell::Int(n) = row.get("n").unwrap() else {
                    panic!("expected integer")
                };
                (id, *n)
            })
            .collect()
    }
}
