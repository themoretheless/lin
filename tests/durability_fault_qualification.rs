//! Comprehensive qualification suite for Durability, WAL Framing/Codecs, Crash Recovery,
//! Checkpoints, Transactions, and Deterministic Fault Injection (`D02`–`D06`, `D43`).

use std::cell::Cell as StdCell;
use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use lin::persist::{
    CodecCapabilities, HEAD_NAME, LOG_NAME, MAX_SNAPSHOT_COLLECTIONS, MAX_SNAPSHOT_EDGES,
    MAX_SNAPSHOT_SECTION_BYTES, OpenMemOpts, Pack, SNAPSHOT_MAGIC, SNAPSHOT_NAME, SnapshotRef,
    SyncMode as PersistSyncMode, decode_snapshot_bytes_checked, dry_run_recovery,
    encode_snapshot_ref_to_writer, for_each_wal_frame, inspect_snapshot_file, inspect_wal_bytes,
    inspect_wal_file, log_path, snapshot_path,
};
use lin::{Db, OpenOpts, Quotas, Store, SyncMode, Transaction};

static TEST_SEQ: AtomicU64 = AtomicU64::new(0);

fn temp_dir(tag: &str) -> PathBuf {
    let n = TEST_SEQ.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("lin-w1-dur-{tag}-{}-{n}", std::process::id()));
    let _ = fs::remove_dir_all(&p);
    fs::create_dir_all(&p).unwrap();
    p
}

fn snapshot_dir_files(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut out = BTreeMap::new();
    let Ok(rd) = fs::read_dir(dir) else {
        return out;
    };
    for ent in rd {
        let ent = ent.unwrap();
        let ft = ent.file_type().unwrap();
        let name = ent.file_name().to_string_lossy().to_string();
        if ft.is_file() {
            out.insert(name, fs::read(ent.path()).unwrap());
        } else if ft.is_dir() {
            for sub in fs::read_dir(ent.path()).unwrap() {
                let sub = sub.unwrap();
                if sub.file_type().unwrap().is_file() {
                    let sub_name = format!("{name}/{}", sub.file_name().to_string_lossy());
                    out.insert(sub_name, fs::read(sub.path()).unwrap());
                }
            }
        }
    }
    out
}

fn restore_dir_files(dst: &Path, files: &BTreeMap<String, Vec<u8>>) {
    let _ = fs::remove_dir_all(dst);
    fs::create_dir_all(dst).unwrap();
    for (rel, bytes) in files {
        if rel == "LOCK" {
            continue;
        }
        let target = dst.join(rel);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(target, bytes).unwrap();
    }
}

fn build_v3_frame(codec: u8, payload: &[u8]) -> Vec<u8> {
    let env_len = (1 + payload.len() + 4) as u32;
    let mut out = Vec::with_capacity(8 + env_len as usize);
    out.extend_from_slice(b"LIN\x06");
    out.extend_from_slice(&env_len.to_le_bytes());
    out.push(codec);
    out.extend_from_slice(payload);
    let crc = lin::crc::hardware_crc32c(&out[8..]).to_le_bytes();
    out.extend_from_slice(&crc);
    out
}

// ============================================================================
// 1. Codec Capabilities, Deterministic WAL Encoding & Golden Frame Corpus
//    (`LIN-0066`..`LIN-0068`, `LIN-0077`..`LIN-0079`)
// ============================================================================

#[test]
fn codec_capabilities_reflect_supported_wal_and_snapshot_formats() {
    let codecs = CodecCapabilities::supported_wal_codecs();
    for required in [1u8, 2, 3, 5] {
        assert!(
            codecs.contains(&required),
            "expected codec {required} in {codecs:?}"
        );
        assert!(CodecCapabilities::supports_wal_codec(required));
    }
    assert_eq!(
        CodecCapabilities::supports_wal_codec(4),
        cfg!(any(feature = "compress-wal", feature = "compress-flate"))
    );
    for unsupported in [0u8, 6, 7, 9, 255] {
        assert!(
            !CodecCapabilities::supports_wal_codec(unsupported),
            "codec {unsupported} must not be advertised"
        );
    }

    let snap_magics = CodecCapabilities::supported_snapshot_magics();
    assert!(snap_magics.contains(&b"\x03LIN\x04".as_slice()));
    assert!(snap_magics.contains(&b"LIN\x04".as_slice()));
    assert!(CodecCapabilities::supports_snapshot_magic(b"\x03LIN\x04payload"));
    assert!(CodecCapabilities::supports_snapshot_magic(b"LIN\x04payload"));
    assert!(!CodecCapabilities::supports_snapshot_magic(b"\xffBAD!"));

    // Unknown codec inside a valid CRC32C LIN\x06 envelope must be rejected cleanly.
    let unknown_frame = build_v3_frame(9, &[1, 2, 3, 4]);
    let rep = inspect_wal_bytes(&unknown_frame);
    assert!(rep.frames.is_empty());
    assert_eq!(rep.damaged_offset, Some(0));
    assert!(
        rep.damaged_reason
            .as_deref()
            .unwrap_or("")
            .contains("unsupported wal codec 9"),
        "{rep:?}"
    );
    let err = for_each_wal_frame(&unknown_frame, |_| Ok(())).unwrap_err();
    assert!(err.message.contains("unknown wal codec"), "{err}");
}

#[test]
fn deterministic_wal_frame_bytes_across_repeated_runs_and_codecs() {
    let run_workload = |dict_enabled: bool| -> Vec<u8> {
        let dir = temp_dir("det-wal");
        let opts = OpenOpts {
            sync: SyncMode::Full,
            cold: false,
        }
        .with_wal_dictionary(dict_enabled);
        let mut db = Db::open_with(&dir, opts).unwrap();

        // Schema + dense columnar + sparse/dictionary vector bulk docs + facts + edges + update/delete
        db.run("col metrics { id: text, v: i64 }").unwrap();
        db.run("index metrics [v]").unwrap();

        let mut metrics_batch = String::from("insert metrics [");
        for i in 0..16 {
            if i > 0 {
                metrics_batch.push(',');
            }
            metrics_batch.push_str(&format!(r#"{{ id: "m{i:02}", v: {} }}"#, i * 10));
        }
        metrics_batch.push(']');
        db.run(&metrics_batch).unwrap();

        // Bulk docs (> 64 KiB of 384-dim HashingEmbedder vectors -> codec 3 when dict=false, codec 5 when dict=true)
        let mut docs_batch = String::from("insert docs [");
        for i in 0..80 {
            if i > 0 {
                docs_batch.push(',');
            }
            let topic = ["durability", "wal codec", "recovery", "checkpoint"][i % 4];
            docs_batch.push_str(&format!(
                r#"{{ id: "doc-{i:03}", uri: "raw://det/{i}", title: "Doc {i} {topic}", layer: "raw", body: "deterministic text about {topic}" }}"#
            ));
        }
        docs_batch.push(']');
        db.run(&docs_batch).unwrap();

        db.run(r#"append facts [ { s: "a", p: tagged, o: "b" }, { s: "b", p: tagged, o: "c" } ]"#)
            .unwrap();
        db.run(r#"append edge wikilink "raw://det/0" -> "raw://det/1""#)
            .unwrap();
        db.run(r#"update metrics[id == "m00"] cas each { v: 999 }"#)
            .unwrap();
        db.run(r#"delete metrics[id == "m15"] cas each"#).unwrap();

        let wal_bytes = fs::read(log_path(&dir)).unwrap();
        let exported = db.export_wal_since(0).unwrap();
        assert_eq!(wal_bytes, exported, "raw WAL and exported WAL must match");
        drop(db);
        let _ = fs::remove_dir_all(&dir);
        wal_bytes
    };

    for dict_mode in [false, true] {
        let a = run_workload(dict_mode);
        let b = run_workload(dict_mode);
        assert_eq!(
            a, b,
            "WAL bytes must be 100% deterministic across runs (dict={dict_mode})"
        );
        let report = inspect_wal_bytes(&a);
        assert!(report.damaged_offset.is_none(), "{report:?}");
        assert_eq!(report.valid_prefix_bytes, a.len() as u64);
        assert_eq!(report.frames.len(), 8);
        for (idx, f) in report.frames.iter().enumerate() {
            assert_eq!(f.frame_index, idx);
            assert_eq!(f.r#gen, (idx + 1) as u64);
            assert!(f.crc_valid);
            assert!(CodecCapabilities::supports_wal_codec(f.codec));
        }
        if dict_mode {
            assert!(
                report.frames.iter().any(|f| f.codec == 5),
                "expected dictionary codec 5 frame when wal_dictionary=true: {:?}",
                report.frames
            );
        } else {
            assert!(
                report.frames.iter().any(|f| f.codec == 3),
                "expected sparse codec 3 frame on high-zero vectors: {:?}",
                report.frames
            );
        }
    }
}

#[test]
fn historical_multi_version_wal_golden_corpus_replays_and_inspects() {
    let dir = temp_dir("golden-corpus");

    // Frame 0 (gen 1): Legacy v0 length-prefixed UTF-8 JSON LogRecord
    let json_rec = br#"{"gen":1,"next_id":2,"pack":{"type":"append_edge","rel":"wikilink","from":"u1","to":"u2"}}"#;
    let mut corpus = Vec::new();
    corpus.extend_from_slice(&(json_rec.len() as u32).to_le_bytes());
    corpus.extend_from_slice(json_rec);

    // Frame 1 (gen 2): Historical v1 `LIN\x01` unchecksummed MessagePack LogRecord
    let rec_v1 = lin::persist::LogRecord {
        r#gen: 2,
        next_id: 3,
        pack: Pack::AppendEdge {
            rel: "wikilink".to_string(),
            from: "u2".to_string(),
            to: "u3".to_string(),
        },
    };
    let msgpack_v1 = rmp_serde::encode::to_vec_named(&rec_v1).unwrap();
    corpus.extend_from_slice(b"LIN\x01");
    corpus.extend_from_slice(&(msgpack_v1.len() as u32).to_le_bytes());
    corpus.extend_from_slice(&msgpack_v1);

    // Frame 2 (gen 3): Historical v2 `LIN\x02` raw `V2_FACTS_BULK`
    let mut v2_payload = Vec::new();
    v2_payload.push(1u8); // V2_FACTS_BULK
    v2_payload.extend_from_slice(&3u64.to_le_bytes()); // gen = 3
    v2_payload.extend_from_slice(&4u64.to_le_bytes()); // next_id = 4
    v2_payload.extend_from_slice(&1u32.to_le_bytes()); // count = 1
    for s in ["lin", "era", "v2"] {
        v2_payload.extend_from_slice(&(s.len() as u32).to_le_bytes());
        v2_payload.extend_from_slice(s.as_bytes());
    }
    corpus.extend_from_slice(b"LIN\x02");
    corpus.extend_from_slice(&(v2_payload.len() as u32).to_le_bytes());
    corpus.extend_from_slice(&v2_payload);

    // Frame 3 (gen 4): Current v3 `LIN\x06` checksummed codec 1
    let rec_v3 = lin::persist::LogRecord {
        r#gen: 4,
        next_id: 5,
        pack: Pack::SchemaCol {
            name: "items".to_string(),
            append: false,
            fields: vec![
                ("id".to_string(), "text".to_string()),
                ("qty".to_string(), "i64".to_string()),
            ],
        },
    };
    let msgpack_v3 = rmp_serde::encode::to_vec_named(&rec_v3).unwrap();
    corpus.extend_from_slice(&build_v3_frame(1, &msgpack_v3));

    fs::write(log_path(&dir), &corpus).unwrap();

    let wal_rep = inspect_wal_file(&log_path(&dir)).unwrap();
    assert_eq!(wal_rep.frames.len(), 4);
    assert_eq!(wal_rep.valid_prefix_bytes, corpus.len() as u64);
    assert_eq!(wal_rep.damaged_offset, None);
    assert_eq!(wal_rep.frames[0].codec, 0);
    assert_eq!(wal_rep.frames[1].codec, 1);
    assert_eq!(wal_rep.frames[2].codec, 2);
    assert_eq!(wal_rep.frames[3].codec, 1);
    assert_eq!(wal_rep.frames[3].pack_kind, "schema_col");

    let dry = dry_run_recovery(&dir).unwrap();
    assert_eq!(dry.replayable_frames, 4);
    assert_eq!(dry.repayable_frames, 4);
    assert_eq!(dry.final_gen, 4);
    assert_eq!(dry.damaged_offset, None);

    let mut db = Db::open(&dir).unwrap();
    assert_eq!(db.store.r#gen, 4);
    assert_eq!(db.run(r#"facts | s == "lin" | count"#).unwrap().rows[0].get("hits").and_then(|c| c.as_f64()), Some(1.0));
    db.run(r#"insert items { id: "it1", qty: 7 }"#).unwrap();
    assert_eq!(db.run("items | count").unwrap().rows[0].get("hits").and_then(|c| c.as_f64()), Some(1.0));
    db.close().unwrap();
    let _ = fs::remove_dir_all(&dir);
}

// ============================================================================
// 2. WAL Inspector & Dry-Run Recovery Fault Matrix: Short Writes, CRC Flips,
//    Mid-Payload Corruption & Generation Gaps (`LIN-0151`, `LIN-0152`, `LIN-1575`,
//    `LIN-1709`, `LIN-1718`, `LIN-1719`)
// ============================================================================

#[test]
fn short_write_every_frame_segment_reports_exact_offset_hexdump_and_truncates_cleanly() {
    let base_dir = temp_dir("short-write-src");
    let full_wal = {
        let mut db = Db::open(&base_dir).unwrap();
        db.run(r#"insert docs { id: "d1", uri: "raw://sw/1", title: "One", layer: "raw", body: "first frame" }"#)
            .unwrap();
        db.run(r#"insert docs { id: "d2", uri: "raw://sw/2", title: "Two", layer: "raw", body: "second frame" }"#)
            .unwrap();
        db.run(r#"insert docs { id: "d3", uri: "raw://sw/3", title: "Three", layer: "raw", body: "third frame payload bytes" }"#)
            .unwrap();
        // Read live `log` BEFORE `Db::drop` runs its closing checkpoint!
        fs::read(log_path(&base_dir)).unwrap()
    };

    let full_rep = inspect_wal_bytes(&full_wal);
    assert_eq!(full_rep.frames.len(), 3);
    let f2 = &full_rep.frames[2];
    let f2_start = f2.byte_offset as usize;
    let f2_len = f2.byte_len as usize;
    assert_eq!(f2_start + f2_len, full_wal.len());

    // Cut inside magic (1, 3), length (4, 7), codec byte (8), mid-payload, and CRC trailer (end-3, end-1)
    let cut_offsets = [
        f2_start + 1,
        f2_start + 3,
        f2_start + 4,
        f2_start + 7,
        f2_start + 8,
        f2_start + 9,
        f2_start + f2_len / 2,
        full_wal.len() - 3,
        full_wal.len() - 1,
    ];

    for cut in cut_offsets {
        let case_dir = temp_dir("short-write-cut");
        fs::write(log_path(&case_dir), &full_wal[..cut]).unwrap();
        let before_disk = snapshot_dir_files(&case_dir);

        let wal_rep = inspect_wal_file(&log_path(&case_dir)).unwrap();
        assert_eq!(wal_rep.frames.len(), 2, "cut={cut}");
        assert_eq!(wal_rep.valid_prefix_bytes, f2_start as u64, "cut={cut}");
        assert_eq!(wal_rep.damaged_offset, Some(f2_start as u64), "cut={cut}");
        assert!(wal_rep.damaged_reason.as_deref().unwrap_or("").contains("truncated"), "{wal_rep:?}");
        let hex = wal_rep.failed_frame_hexdump.as_deref().expect("hexdump");
        assert!(!hex.is_empty() && hex.starts_with("4c"), "expected 'L' (0x4c) at start of hexdump: {hex}");

        let dry = dry_run_recovery(&case_dir).unwrap();
        assert_eq!(dry.replayable_frames, 2, "cut={cut}");
        assert_eq!(dry.final_gen, 2, "cut={cut}");
        assert_eq!(dry.truncate_to_offset, Some(f2_start as u64), "cut={cut}");
        assert_eq!(dry.damaged_offset, Some(f2_start as u64), "cut={cut}");
        assert!(dry.failed_frame_hexdump.is_some());
        assert!(dry.decision_log.iter().any(|l| l.contains("truncate log")));

        // Non-mutating guarantee: dry_run_recovery + inspect_wal_file touched nothing on disk!
        assert_eq!(snapshot_dir_files(&case_dir), before_disk, "inspector mutated disk!");

        // Actual open truncates to exact `f2_start` and preserves frames 1..=2
        let mut db = Db::open(&case_dir).unwrap();
        assert_eq!(db.store.r#gen, 2);
        assert_eq!(fs::metadata(log_path(&case_dir)).unwrap().len(), f2_start as u64);
        let hits = db.run("docs | count").unwrap().rows[0].get("hits").and_then(|c| c.as_f64());
        assert_eq!(hits, Some(2.0));
        drop(db);
        let _ = fs::remove_dir_all(&case_dir);
    }
    let _ = fs::remove_dir_all(&base_dir);
}

#[test]
fn flipped_crc_and_generation_gap_report_exact_byte_offsets_and_fail_closed_without_truncation() {
    let base_dir = temp_dir("crc-gap-src");
    let orig_wal = {
        let mut db = Db::open(&base_dir).unwrap();
        for i in 1..=3 {
            db.run(&format!(
                r#"insert docs {{ id: "c{i}", uri: "raw://cg/{i}", title: "T{i}", layer: "raw" }}"#
            ))
            .unwrap();
        }
        // Read live `log` BEFORE `Db::drop` runs its closing checkpoint!
        fs::read(log_path(&base_dir)).unwrap()
    };
    let orig_rep = inspect_wal_bytes(&orig_wal);
    assert_eq!(orig_rep.frames.len(), 3);
    let f1_off = orig_rep.frames[1].byte_offset as usize;
    let f1_len = orig_rep.frames[1].byte_len as usize;
    let f2_off = orig_rep.frames[2].byte_offset as usize;

    // Case A: Flip 1 bit in Frame #1's CRC32C trailer
    {
        let crc_dir = temp_dir("crc-flip");
        let mut bad = orig_wal.clone();
        bad[f1_off + f1_len - 1] ^= 0x01;
        fs::write(log_path(&crc_dir), &bad).unwrap();

        let rep = inspect_wal_bytes(&bad);
        assert_eq!(rep.frames.len(), 1);
        assert_eq!(rep.valid_prefix_bytes, f1_off as u64);
        assert_eq!(rep.damaged_offset, Some(f1_off as u64));
        assert!(rep.damaged_reason.as_deref().unwrap_or("").contains("crc mismatch"));
        assert!(rep.failed_frame_hexdump.is_some());

        let dry = dry_run_recovery(&crc_dir).unwrap();
        assert_eq!(dry.replayable_frames, 1);
        assert_eq!(dry.damaged_offset, Some(f1_off as u64));
        assert_eq!(dry.truncate_to_offset, None, "must not truncate over mid-log CRC error");
        assert!(dry.failed_frame_hexdump.is_some());

        let err = match Db::open(&crc_dir) {
            Ok(_) => panic!("expected corrupt WAL open failure"),
            Err(e) => e,
        };
        assert!(
            err.message.contains(&format!("byte offset {f1_off}")),
            "expected failing byte offset {f1_off} in error: {err}"
        );
        // Log file must remain untouched on disk!
        assert_eq!(fs::read(log_path(&crc_dir)).unwrap(), bad);

        let ship_err = for_each_wal_frame(&bad, |_| Ok(())).unwrap_err();
        assert!(
            ship_err.message.contains(&format!("offset {f1_off}")),
            "{ship_err}"
        );
        let _ = fs::remove_dir_all(&crc_dir);
    }

    // Case B: Generation gap (Frame #0 gen=1, Frame #2 gen=3 — missing Frame #1 gen=2!)
    {
        let gap_dir = temp_dir("gen-gap");
        let mut gapped = Vec::new();
        gapped.extend_from_slice(&orig_wal[..f1_off]);
        gapped.extend_from_slice(&orig_wal[f2_off..]);
        fs::write(log_path(&gap_dir), &gapped).unwrap();

        let wal_rep = inspect_wal_bytes(&gapped);
        assert_eq!(wal_rep.frames.len(), 2, "individual frames are valid CRC frames");
        assert_eq!(wal_rep.frames[0].r#gen, 1);
        assert_eq!(wal_rep.frames[1].r#gen, 3);
        let gap_frame_offset = wal_rep.frames[1].byte_offset;
        assert_eq!(gap_frame_offset, f1_off as u64);

        let dry = dry_run_recovery(&gap_dir).unwrap();
        assert_eq!(dry.replayable_frames, 1);
        assert_eq!(dry.final_gen, 1);
        assert_eq!(dry.truncate_to_offset, None);
        assert_eq!(dry.damaged_offset, Some(gap_frame_offset));
        assert!(dry.failed_frame_hexdump.is_some());
        assert!(
            dry.decision_log.iter().any(|d| d.contains("generation gap")),
            "{:?}",
            dry.decision_log
        );

        let err = match Db::open(&gap_dir) {
            Ok(_) => panic!("expected generation gap error"),
            Err(e) => e,
        };
        assert!(err.message.contains("generation gap"), "{err}");
        assert_eq!(fs::read(log_path(&gap_dir)).unwrap(), gapped);
        let _ = fs::remove_dir_all(&gap_dir);
    }

    let _ = fs::remove_dir_all(&base_dir);
}

// ============================================================================
// 3. Recovery Progress Callbacks, Mid-Replay Cancellation & Idempotent Recovery
//    (`LIN-0149`, `LIN-0150`, `LIN-1704`)
// ============================================================================

#[test]
fn recovery_hooks_report_monotonic_progress_and_mid_replay_cancel_leaves_disk_untouched() {
    // Immediate cancellation on a non-existent path must not even create the directory!
    let phantom = std::env::temp_dir().join(format!("lin-cancel-phantom-{}", std::process::id()));
    let _ = fs::remove_dir_all(&phantom);
    let cat = Db::fixture().catalog.clone();
    let cancel_err = Store::open_with_recovery_hooks(
        &phantom,
        &cat,
        OpenMemOpts::default(),
        |_| panic!("progress must not fire on immediate cancel"),
        || true,
    )
    .err()
    .expect("expected immediate cancel error");
    assert_eq!(cancel_err.message, "recovery canceled");
    assert!(!phantom.exists(), "cancelled open created directory!");

    // Build a DB with a checkpoint at gen=2, 5 WAL frames (gens 3..=7), AND a torn trailing stub
    let src_dir = temp_dir("recovery-hooks-src");
    let crash_files = {
        let mut db = Db::open(&src_dir).unwrap();
        db.run(r#"insert docs { id: "r1", uri: "raw://rh/1", title: "R1", layer: "raw" }"#)
            .unwrap();
        db.run(r#"insert docs { id: "r2", uri: "raw://rh/2", title: "R2", layer: "raw" }"#)
            .unwrap();
        db.checkpoint().unwrap();
        for i in 3..=7 {
            db.run(&format!(
                r#"insert docs {{ id: "r{i}", uri: "raw://rh/{i}", title: "R{i}", layer: "raw" }}"#
            ))
            .unwrap();
        }
        // Capture live directory state BEFORE `Db::drop` compacts the WAL!
        snapshot_dir_files(&src_dir)
    };
    let _ = fs::remove_dir_all(&src_dir);
    let dir = temp_dir("recovery-hooks");
    restore_dir_files(&dir, &crash_files);
    // Append a 5-byte torn header stub to `log` so a real open would truncate `log` AND update `head`!
    {
        let mut f = OpenOptions::new().append(true).open(log_path(&dir)).unwrap();
        f.write_all(b"LIN\x06\x99").unwrap();
        f.flush().unwrap();
    }
    let disk_before_cancel = snapshot_dir_files(&dir);

    // Cancel right after frame #3 (`current_gen == 5`) is reported!
    let mut progress_seen = Vec::new();
    let should_cancel = StdCell::new(false);
    let err = Store::open_with_recovery_hooks(
        &dir,
        &cat,
        OpenMemOpts::default(),
        |p| {
            progress_seen.push(p);
            if p.frames_replayed == 3 {
                should_cancel.set(true);
            }
        },
        || should_cancel.get(),
    )
    .err()
    .expect("expected mid-replay cancel error");
    assert_eq!(err.message, "recovery canceled");
    assert_eq!(progress_seen.len(), 3);
    assert_eq!(progress_seen[0].frames_replayed, 1);
    assert_eq!(progress_seen[0].current_gen, 3);
    assert_eq!(progress_seen[2].frames_replayed, 3);
    assert_eq!(progress_seen[2].current_gen, 5);
    assert!(progress_seen[0].bytes_offset < progress_seen[1].bytes_offset);
    assert!(progress_seen[1].bytes_offset < progress_seen[2].bytes_offset);

    // Every single file on disk (including torn WAL tail & stale head at gen=2) MUST be untouched!
    assert_eq!(
        snapshot_dir_files(&dir),
        disk_before_cancel,
        "mid-replay cancellation mutated files on disk!"
    );

    // Subsequent uncancelled recovery succeeds and replays all 5 frames cleanly (`LIN-1704`)
    let mut full_progress = Vec::new();
    let (store, persist) = Store::open_with_recovery_hooks(
        &dir,
        &cat,
        OpenMemOpts::default(),
        |p| full_progress.push(p),
        || false,
    )
    .unwrap();
    assert_eq!(full_progress.len(), 5);
    assert_eq!(full_progress.last().unwrap().current_gen, 7);
    assert_eq!(store.r#gen, 7);
    drop((store, persist));

    // Re-opening again is 100% idempotent
    for _ in 0..2 {
        let mut db = Db::open(&dir).unwrap();
        assert_eq!(db.store.r#gen, 7);
        assert_eq!(
            db.run("docs | count").unwrap().rows[0].get("hits").and_then(|c| c.as_f64()),
            Some(7.0)
        );
        db.close().unwrap();
    }
    let _ = fs::remove_dir_all(&dir);
}

// ============================================================================
// 4. Snapshot Decode Bounds, Inspector, Checkpoint Metrics & Stage Provenance
//    (`LIN-0165`, `LIN-0171`, `LIN-0172`, `LIN-0188`..`LIN-0192`, `LIN-1711`)
// ============================================================================

#[test]
fn crafted_oversized_snapshot_headers_rejected_with_clear_runtime_errors() {
    // 1. Oversized `header_len` (> MAX_SNAPSHOT_SECTION_BYTES)
    {
        let mut crafted = Vec::new();
        crafted.push(0x03);
        crafted.extend_from_slice(&SNAPSHOT_MAGIC);
        crafted.extend_from_slice(&((MAX_SNAPSHOT_SECTION_BYTES as u32) + 64).to_le_bytes());
        crafted.extend_from_slice(&[0u8; 32]);
        let err = decode_snapshot_bytes_checked(&crafted).unwrap_err();
        assert_eq!(err.phase, lin::error::Phase::Runtime);
        assert!(
            err.message.contains("MAX_SNAPSHOT_SECTION_BYTES"),
            "{err}"
        );
    }

    // 2. Crafted MessagePack header claiming > MAX_SNAPSHOT_EDGES without allocating 67M edges
    {
        let mut hdr = Vec::new();
        hdr.push(0x81); // fixmap of 1 entry
        hdr.push(0xa5); // fixstr "edges"
        hdr.extend_from_slice(b"edges");
        hdr.push(0xdd); // array32
        hdr.extend_from_slice(&((MAX_SNAPSHOT_EDGES as u32) + 10).to_be_bytes());

        let mut crafted = Vec::new();
        crafted.push(0x03);
        crafted.extend_from_slice(&SNAPSHOT_MAGIC);
        crafted.extend_from_slice(&(hdr.len() as u32).to_le_bytes());
        crafted.extend_from_slice(&hdr);
        crafted.extend_from_slice(&0u32.to_le_bytes()); // chunk_count = 0

        let err = decode_snapshot_bytes_checked(&crafted).unwrap_err();
        assert_eq!(err.phase, lin::error::Phase::Runtime);
        assert!(err.message.contains("MAX_SNAPSHOT_EDGES"), "{err}");

        // Verify `read_snapshot` and `read_backup` also propagate the explicit limit error!
        let dir = temp_dir("snap-edge-limit");
        fs::write(snapshot_path(&dir), &crafted).unwrap();
        let dir_err = lin::persist::read_snapshot(&dir).unwrap_err();
        assert!(dir_err.message.contains("MAX_SNAPSHOT_EDGES"), "{dir_err}");
        let bak_err = lin::persist::read_backup(&snapshot_path(&dir)).unwrap_err();
        assert!(bak_err.message.contains("MAX_SNAPSHOT_EDGES"), "{bak_err}");
        let _ = fs::remove_dir_all(&dir);
    }

    // 3. Crafted MessagePack header claiming > MAX_SNAPSHOT_COLLECTIONS in `extra_collections`
    {
        let mut hdr = Vec::new();
        hdr.push(0x81); // fixmap(1)
        hdr.push(0xb1); // fixstr(17) "extra_collections"
        hdr.extend_from_slice(b"extra_collections");
        hdr.push(0xdf); // map32
        hdr.extend_from_slice(&((MAX_SNAPSHOT_COLLECTIONS as u32) + 5).to_be_bytes());

        let mut crafted = Vec::new();
        crafted.push(0x03);
        crafted.extend_from_slice(&SNAPSHOT_MAGIC);
        crafted.extend_from_slice(&(hdr.len() as u32).to_le_bytes());
        crafted.extend_from_slice(&hdr);
        crafted.extend_from_slice(&0u32.to_le_bytes());

        let err = decode_snapshot_bytes_checked(&crafted).unwrap_err();
        assert_eq!(err.phase, lin::error::Phase::Runtime);
        assert!(err.message.contains("MAX_SNAPSHOT_COLLECTIONS"), "{err}");
    }

    // 4. Crafted `chunk_count = u32::MAX` & oversized chunk `data_len` must never OOM
    {
        let empty_collections = BTreeMap::new();
        let empty_extra_cols = BTreeMap::new();
        let empty_rels = BTreeMap::new();
        let empty_idxs = BTreeMap::new();
        let empty_filters = BTreeMap::new();
        let empty_owned = BTreeMap::new();
        let snap_ref = SnapshotRef {
            r#gen: 1,
            embed_id: "hashing",
            next_id: 2,
            catalog_hash: "abc",
            log_offset: 0,
            collections: &empty_collections,
            edges: &[],
            extra_collections: &empty_extra_cols,
            extra_rels: &empty_rels,
            extra_indexes: &empty_idxs,
            extra_filters: &empty_filters,
            extra_owned: &empty_owned,
            cold_collections: &[],
        };
        let mut valid_snap = Vec::new();
        encode_snapshot_ref_to_writer(&mut valid_snap, &snap_ref).unwrap();

        let hdr_len = u32::from_le_bytes(valid_snap[5..9].try_into().unwrap()) as usize;
        let chunk_count_off = 9 + hdr_len;

        // 4a: Huge chunk_count
        let mut huge_chunks = valid_snap[..chunk_count_off].to_vec();
        huge_chunks.extend_from_slice(&u32::MAX.to_le_bytes());
        let err = decode_snapshot_bytes_checked(&huge_chunks).unwrap_err();
        assert!(err.message.contains("exceeds maximum section count"), "{err}");

        // 4b: 1 chunk with `data_len > MAX_SNAPSHOT_SECTION_BYTES`
        let mut huge_sec = valid_snap[..chunk_count_off].to_vec();
        huge_sec.extend_from_slice(&1u32.to_le_bytes());
        huge_sec.extend_from_slice(&4u16.to_le_bytes());
        huge_sec.extend_from_slice(b"docs");
        huge_sec.extend_from_slice(&((MAX_SNAPSHOT_SECTION_BYTES as u32) + 1).to_le_bytes());
        let sec_err = decode_snapshot_bytes_checked(&huge_sec).unwrap_err();
        assert!(sec_err.message.contains("MAX_SNAPSHOT_SECTION_BYTES"), "{sec_err}");
    }
}

#[test]
fn snapshot_inspector_checkpoint_metrics_debt_forecast_and_stage_error_provenance() {
    let dir = temp_dir("ckpt-metrics");
    let missing_rep = inspect_snapshot_file(&snapshot_path(&dir)).unwrap();
    assert!(!missing_rep.exists);
    assert_eq!(missing_rep.byte_len, 0);

    let cat = Db::fixture().catalog.clone();
    let opts = OpenMemOpts {
        sync: PersistSyncMode::Full,
        cold: true,
    };
    let (mut store, mut persist) = Store::open_with(&dir, &cat, opts).unwrap();
    assert_eq!(store.checkpoint_debt_bytes(&persist), 0);
    let base_forecast = store.forecast_checkpoint_bytes();
    assert!(base_forecast > 50);

    // Commit >= 40 rows into `orders` so `cold: true` spills `orders` into cold tier, plus 3 `users` in RAM
    let mut order_rows = Vec::new();
    for i in 0..40 {
        let mut r = lin::Row::new();
        r.insert("id", lin::Cell::Text(Arc::from(format!("ord-{i:02}"))));
        r.insert("user_id", lin::Cell::Text(Arc::from("u1")));
        r.insert("total", lin::Cell::Int(i * 10));
        order_rows.push(r);
    }
    let ord_pack = lin::persist::rows_to_insert_pack("orders", &order_rows, Vec::new());
    store.durable_commit(&mut persist, ord_pack).unwrap();
    for r in order_rows {
        store.collections.entry("orders".to_string()).or_default().push(r);
    }
    store.r#gen += 1;

    let mut user_rows = Vec::new();
    for i in 0..3 {
        let mut r = lin::Row::new();
        r.insert("id", lin::Cell::Text(Arc::from(format!("u{i}"))));
        r.insert("email", lin::Cell::Text(Arc::from(format!("u{i}@lin.test"))));
        user_rows.push(r);
    }
    let usr_pack = lin::persist::rows_to_insert_pack("users", &user_rows, Vec::new());
    store.durable_commit(&mut persist, usr_pack).unwrap();
    for r in user_rows {
        store.collections.entry("users".to_string()).or_default().push(r);
    }
    store.r#gen += 1;

    let debt = store.checkpoint_debt_bytes(&persist);
    assert!(debt > 0, "expected positive WAL debt before checkpoint");
    let grown_forecast = store.forecast_checkpoint_bytes();
    assert!(
        grown_forecast > base_forecast,
        "forecast should grow with rows ({grown_forecast} vs {base_forecast})"
    );

    let metrics = store.checkpoint_with_metrics(&mut persist).unwrap();
    assert_eq!(metrics.generation, 2);
    assert_eq!(metrics.wal_bytes_before, debt);
    assert_eq!(metrics.forecast_snapshot_bytes, grown_forecast);
    assert!(metrics.duration_ms >= 0.0);
    assert_eq!(metrics.cold_tables, 1, "orders (40 rows >= 32) should spill to cold tier");
    assert_eq!(metrics.in_memory_rows, 3, "users (3 rows < 32) stays in memory");
    assert_eq!(store.checkpoint_debt_bytes(&persist), 0, "debt must reset to 0 after checkpoint");

    // Inspect written snapshot file
    let snap_rep = inspect_snapshot_file(&snapshot_path(&dir)).unwrap();
    assert!(snap_rep.exists);
    assert!(snap_rep.byte_len > 0);
    assert_eq!(snap_rep.format_version, Some(0x03));
    assert!(snap_rep.crc_trailer_present);
    assert!(snap_rep.crc_valid);
    assert_eq!(snap_rep.r#gen, Some(2));
    assert!(snap_rep.cold_collections.contains(&"orders".to_string()));

    // Verify stage error provenance (`LIN-0192`) on `snapshot_write` and `head_write`
    let snap_tmp_dir = snapshot_path(&dir).with_extension("bin.tmp");
    fs::create_dir_all(&snap_tmp_dir).unwrap();
    let snap_stage_err = store.checkpoint_with_metrics(&mut persist).unwrap_err();
    assert!(
        snap_stage_err.message.contains("checkpoint [snapshot_write]:"),
        "expected snapshot_write stage provenance: {snap_stage_err}"
    );
    fs::remove_dir_all(&snap_tmp_dir).unwrap();

    let head_tmp_dir = dir.join("head.tmp");
    fs::create_dir_all(&head_tmp_dir).unwrap();
    let head_stage_err = store.checkpoint_with_metrics(&mut persist).unwrap_err();
    assert!(
        head_stage_err.message.contains("checkpoint [head_write]:"),
        "expected head_write stage provenance: {head_stage_err}"
    );
    fs::remove_dir_all(&head_tmp_dir).unwrap();

    // Flip 1 bit in snapshot body and verify `inspect_snapshot_file` flags `crc_valid == false`
    drop((store, persist));
    let mut snap_bytes = fs::read(snapshot_path(&dir)).unwrap();
    let mid = snap_bytes.len() / 2;
    snap_bytes[mid] ^= 0x40;
    let corrupt_snap_path = dir.join("corrupt_snapshot");
    fs::write(&corrupt_snap_path, &snap_bytes).unwrap();
    let bad_snap_rep = inspect_snapshot_file(&corrupt_snap_path).unwrap();
    assert!(bad_snap_rep.exists);
    assert_eq!(bad_snap_rep.format_version, Some(0x03));
    assert!(bad_snap_rep.crc_trailer_present);
    assert!(!bad_snap_rep.crc_valid);
    assert_eq!(bad_snap_rep.r#gen, None);

    let _ = fs::remove_dir_all(&dir);
}

// ============================================================================
// 5. Mixed Sync Modes, Nested Parent Dir Durability, Quotas, Schema Replay,
//    Follower WAL Tail Shipping & Crash Chaos Matrix
//    (`LIN-0085`, `LIN-0087`, `LIN-0108`, `LIN-0110`, `LIN-0116`, `LIN-0154`..`LIN-0156`,
//     `LIN-0167`, `LIN-0180`, `LIN-0185`, `LIN-0186`, `LIN-0197`, `LIN-1685`..`LIN-1692`,
//     `LIN-1711`, `LIN-1714`)
// ============================================================================

#[test]
fn mixed_sync_modes_nested_parent_creation_and_quota_exhaustion_preserve_prior_commits() {
    let root = temp_dir("sync-quota");
    let nested_dir = root.join("deep").join("nested").join("durable_db");

    // 1. Nested parent directory creation + SyncMode::Normal -> SyncMode::Full transitions
    {
        let mut db = Db::open_with(
            &nested_dir,
            OpenOpts {
                sync: SyncMode::Normal,
                cold: false,
            },
        )
        .unwrap();
        db.run(r#"insert docs { id: "s1", uri: "raw://sq/1", title: "Normal1", layer: "raw" }"#)
            .unwrap();
        db.close().unwrap();
    }
    {
        let mut db = Db::open_with(
            &nested_dir,
            OpenOpts {
                sync: SyncMode::Full,
                cold: false,
            },
        )
        .unwrap();
        assert_eq!(db.store.r#gen, 1);
        db.run(r#"insert docs { id: "s2", uri: "raw://sq/2", title: "Full2", layer: "raw" }"#)
            .unwrap();
        // Backup into another nested non-existent directory (`LIN-0180`)
        let nested_bak = root.join("backups").join("tier1").join("snap.bak");
        db.export_backup(&nested_bak).unwrap();
        assert!(nested_bak.exists());

        // Enforce strict row & aggregate quotas (`LIN-0116`, `LIN-0197`)
        db = db.with_quotas(Quotas {
            max_rows: 2,
            max_aggregate_bytes: 32,
            ..Default::default()
        });
        let row_err = db
            .run(r#"insert docs { id: "s3", uri: "raw://sq/3", title: "Overflow", layer: "raw" }"#)
            .unwrap_err();
        assert!(row_err.message.contains("max_rows"), "{row_err}");
        assert_eq!(db.store.r#gen, 2, "failed quota write must not advance gen");

        let agg_err = db.run("docs | count by title").unwrap_err();
        assert!(agg_err.message.contains("max_aggregate_bytes"), "{agg_err}");
    }

    // Reopen and verify prior 2 committed rows are 100% intact and uncorrupted!
    {
        let mut db = Db::open(&nested_dir).unwrap();
        assert_eq!(db.store.r#gen, 2);
        assert_eq!(
            db.run("docs | count").unwrap().rows[0].get("hits").and_then(|c| c.as_f64()),
            Some(2.0)
        );
        db.close().unwrap();
    }
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn schema_replay_reader_fencing_follower_catchup_and_checkpoint_chaos_matrix() {
    let primary_dir = temp_dir("chaos-primary");
    let follower_dir = temp_dir("chaos-follower");
    let backup_file = temp_dir("chaos-bak").join("boot.snap");

    let mut primary = Db::open(&primary_dir).unwrap();
    primary.run("col ledger { id: text, acct: text, cents: i64 }").unwrap();
    primary.run("index ledger [acct]").unwrap();
    primary.run("owned tag_meta { owner: text }").unwrap();
    primary
        .run(r#"insert ledger [ { id: "l1", acct: "A", cents: 100 }, { id: "l2", acct: "B", cents: 250 } ]"#)
        .unwrap();
    primary.checkpoint().unwrap();

    // Bootstrap follower from portable backup (`LIN-0186`)
    primary.export_backup(&backup_file).unwrap();
    let mut follower = Db::bootstrap_follower(&backup_file, &follower_dir).unwrap();
    assert_eq!(follower.store.r#gen, primary.store.r#gen);

    // Concurrent `open_read` holds shared reader fence at checkpointed gen while primary appends WAL (`LIN-0167`, `LIN-0185`)
    let reader_before = Db::open_read(&primary_dir).unwrap();
    let gen_at_ckpt = primary.store.r#gen;

    primary.run("col audit { id: text, note: text }").unwrap();
    primary
        .run(r#"insert audit { id: "au1", note: "post-ckpt schema + data" }"#)
        .unwrap();
    primary
        .run(r#"update ledger[id == "l1"] cas each { cents: 175 }"#)
        .unwrap();

    // Reader opened before post-checkpoint writes still sees isolated snapshot state (`cents == 100`)
    assert_eq!(
        reader_before
            .run(r#"ledger | id == "l1" | { cents }"#)
            .unwrap()
            .rows[0]
            .get("cents")
            .and_then(|c| c.as_f64()),
        Some(100.0)
    );
    drop(reader_before);

    // Capture pre-checkpoint image (WAL tail contains `col audit` + insert + update)
    let state_before_ckpt = snapshot_dir_files(&primary_dir);

    // Export WAL tail to follower BEFORE primary checkpoints and compacts WAL
    let wal_tail = primary.export_wal_since(follower.store.r#gen).unwrap();
    assert!(!wal_tail.is_empty());
    assert_eq!(follower.apply_wal(&wal_tail).unwrap(), 3);
    assert_eq!(follower.store.r#gen, primary.store.r#gen);

    // Checkpoint primary (compacts WAL to 0 bytes) and capture post-checkpoint image
    primary.checkpoint().unwrap();
    assert_eq!(fs::metadata(log_path(&primary_dir)).unwrap().len(), 0);
    let state_after_ckpt = snapshot_dir_files(&primary_dir);

    // Post-checkpoint write + follower catch-up across checkpoint boundary
    primary
        .run(r#"insert ledger { id: "l3", acct: "A", cents: 50 }"#)
        .unwrap();
    let post_ckpt_tail = primary.export_wal_since(follower.store.r#gen).unwrap();
    assert_eq!(follower.apply_wal(&post_ckpt_tail).unwrap(), 1);
    assert_eq!(
        follower
            .run(r#"ledger | acct == "A" | count"#)
            .unwrap()
            .rows[0]
            .get("hits")
            .and_then(|c| c.as_f64()),
        Some(2.0)
    );

    // Checkpoint Chaos Matrix (`LIN-1685`..`LIN-1692`, `LIN-1711`, `LIN-1714`):
    // Simulate crashes:
    // 0: before snapshot rename (leftover `snapshot.bin.tmp` + `fts/*.tmp` + pre-ckpt `snapshot`, `head`, `log`)
    // 1: after snapshot rename, before `head` update (new `snapshot`, old `head`, full `log`, leftover `head.tmp`)
    // 2: after `head` update, before WAL truncate (new `snapshot`, new `head`, full `log`)
    // 3: after WAL truncate (new `snapshot`, new `head`, empty `log`)
    for stage in 0..=3 {
        let sim_dir = temp_dir("chaos-stage");
        restore_dir_files(&sim_dir, &state_before_ckpt);
        if stage >= 1
            && let Some(snap_b) = state_after_ckpt.get(SNAPSHOT_NAME)
        {
            fs::write(sim_dir.join(SNAPSHOT_NAME), snap_b).unwrap();
        }
        if stage >= 2
            && let Some(head_b) = state_after_ckpt.get(HEAD_NAME)
        {
            fs::write(sim_dir.join(HEAD_NAME), head_b).unwrap();
        }
        if stage >= 3
            && let Some(log_b) = state_after_ckpt.get(LOG_NAME)
        {
            fs::write(sim_dir.join(LOG_NAME), log_b).unwrap();
        }
        // Inject interrupted leftover `.tmp` files (`LIN-1711`)
        fs::write(sim_dir.join("snapshot.bin.tmp"), b"interrupted-partial-snap").unwrap();
        fs::write(sim_dir.join("head.tmp"), b"{\"partial_head").unwrap();
        fs::create_dir_all(sim_dir.join("fts")).unwrap();
        fs::write(sim_dir.join("fts/docs.tmp"), b"stale-fts-tmp").unwrap();

        let dry = dry_run_recovery(&sim_dir).unwrap();
        assert_eq!(dry.final_gen, gen_at_ckpt + 3, "stage={stage}: {dry:?}");
        assert_eq!(dry.damaged_offset, None, "stage={stage}: {dry:?}");

        let mut recovered = Db::open(&sim_dir).unwrap();
        assert_eq!(recovered.store.r#gen, gen_at_ckpt + 3, "stage={stage}");
        assert_eq!(
            recovered
                .run(r#"ledger | id == "l1" | { cents }"#)
                .unwrap()
                .rows[0]
                .get("cents")
                .and_then(|c| c.as_f64()),
            Some(175.0),
            "stage={stage}"
        );
        assert_eq!(
            recovered.run("audit | count").unwrap().rows[0].get("hits").and_then(|c| c.as_f64()),
            Some(1.0),
            "stage={stage}"
        );

        // Next checkpoint sweeps stale `fts/*.tmp` and overwrites any temp artifacts cleanly (`LIN-1711`)
        recovered.checkpoint().unwrap();
        assert!(!sim_dir.join("fts/docs.tmp").exists(), "fts/*.tmp must be swept on checkpoint");
        recovered.close().unwrap();
        let _ = fs::remove_dir_all(&sim_dir);
    }

    // Schema catalog_hash mismatch detection (`LIN-0156`): tamper `head.catalog_hash` with bogus hash
    {
        let mismatch_dir = temp_dir("cat-mismatch");
        restore_dir_files(&mismatch_dir, &state_after_ckpt);
        let bogus_head = lin::persist::Head {
            r#gen: gen_at_ckpt + 3,
            catalog_hash: "0000000000000000".to_string(),
            embed_id: "hashing".to_string(),
        };
        lin::persist::write_head(&mismatch_dir, &bogus_head).unwrap();
        let err = match Db::open(&mismatch_dir) {
            Ok(_) => panic!("expected catalog_hash mismatch error"),
            Err(e) => e,
        };
        assert!(err.message.contains("catalog_hash mismatch"), "{err}");
        let _ = fs::remove_dir_all(&mismatch_dir);
    }

    let _ = fs::remove_dir_all(&primary_dir);
    let _ = fs::remove_dir_all(&follower_dir);
    let _ = fs::remove_dir_all(backup_file.parent().unwrap());
}

// ============================================================================
// 6. Transaction Replay IDs, Phase Diagnostics, Rollback Reason Counters &
//    Nested Transaction Rejection (`LIN-0212`, `LIN-0220`, `LIN-0237`,
//    `LIN-0238`, `LIN-0240`)
// ============================================================================

#[test]
fn transaction_ids_diagnostics_rollback_reason_counters_and_nested_rejection() {
    let dir = temp_dir("tx-qual");
    let mut db = Db::open_with(
        &dir,
        OpenOpts {
            sync: SyncMode::Full,
            cold: true,
        },
    )
    .unwrap();

    // Seed `users.u1` (for FK `orders.user_id -> users.id`) + cold-backed `orders` (>= 32 rows + checkpoint)
    db.run(r#"insert users { id: "u1", email: "u1@lin.test" }"#)
        .unwrap();
    let mut seed = String::from("insert orders [");
    for i in 0..36 {
        if i > 0 {
            seed.push(',');
        }
        seed.push_str(&format!(r#"{{ id: "o{i:02}", user_id: "u1", total: {i} }}"#));
    }
    seed.push(']');
    db.run(&seed).unwrap();
    db.close().unwrap();
    drop(db);
    // Reopen with `cold: true` so `orders` is attached lazily via `StoreDiskTier` (`disk_savepoint = true` on tx write)
    let mut db = Db::open_with(
        &dir,
        OpenOpts {
            sync: SyncMode::Full,
            cold: true,
        },
    )
    .unwrap();

    Transaction::reset_rollback_counters();
    assert_eq!(
        Transaction::rollback_counters(),
        lin::persist::RollbackReasonCounters {
            explicit: 0,
            dropped_uncommitted: 0,
            statement_error: 0,
            commit_error: 0,
        }
    );

    // 1. Monotonic `tx_id`, append/cold `diagnostics()`, and `begin_nested()` rejection (`LIN-0220`, `LIN-0237`, `LIN-0240`)
    let tx_id_1;
    {
        let mut tx = db.transaction().unwrap();
        tx_id_1 = tx.tx_id();
        assert!(tx_id_1 >= 1);
        let d0 = tx.diagnostics();
        assert_eq!(d0.tx_id, tx_id_1);
        assert_eq!(d0.statements_run, 0);
        assert_eq!(d0.journal_entries, 0);
        assert!(!d0.has_full_backup);

        // Reject nested interactive transaction dynamically while keeping outer tx intact
        let nested_err = tx.begin_nested().unwrap_err();
        assert!(
            nested_err
                .message
                .contains("nested interactive transactions are unsupported"),
            "{nested_err}"
        );
        assert!(nested_err.message.contains(&tx_id_1.to_string()));

        // Write to cold-backed `orders` -> triggers `disk_savepoint` (or full backup) + journal entry
        tx.execute(r#"insert orders { id: "o99", user_id: "u1", total: 990 }"#)
            .unwrap();
        tx.run(r#"orders | id == "o99" | { total }"#).unwrap();

        let d1 = tx.diagnostics();
        assert_eq!(d1.tx_id, tx_id_1);
        assert_eq!(d1.statements_run, 2);
        assert!(d1.journal_entries >= 1);
        assert!(d1.disk_savepoint || d1.has_full_backup, "{d1:?}");
        tx.commit().unwrap();
    }

    // 2. Explicit rollback (`explicit` += 1) and `has_full_backup` on schema DDL
    let tx_id_2;
    {
        let mut tx = db.transaction().unwrap();
        tx_id_2 = tx.tx_id();
        assert!(tx_id_2 > tx_id_1, "tx_id must be strictly monotonic");
        tx.run("col temp_tbl { id: text }").unwrap();
        tx.run(r#"insert temp_tbl { id: "t1" }"#).unwrap();
        let diag = tx.diagnostics();
        assert_eq!(diag.statements_run, 2);
        assert!(diag.has_full_backup, "schema DDL in tx must take full backup: {diag:?}");
        tx.rollback().unwrap();
    }
    let c_after_explicit = Transaction::rollback_counters();
    assert_eq!(c_after_explicit.explicit, 1);
    assert_eq!(c_after_explicit.dropped_uncommitted, 0);
    assert_eq!(c_after_explicit.statement_error, 0);
    assert_eq!(c_after_explicit.commit_error, 0);
    assert!(db.run("temp_tbl | count").is_err(), "rolled-back schema must disappear");

    // 3. Drop uncommitted rollback (`dropped_uncommitted` += 1)
    {
        let mut tx = db.transaction().unwrap();
        assert!(tx.tx_id() > tx_id_2);
        tx.execute(r#"insert docs { id: "drop-doc", uri: "raw://tx/drop", title: "Dropped", layer: "raw" }"#)
            .unwrap();
        // Dropped here without commit()!
    }
    let c_after_drop = Transaction::rollback_counters();
    assert_eq!(c_after_drop.explicit, 1);
    assert_eq!(c_after_drop.dropped_uncommitted, 1);
    assert_eq!(c_after_drop.statement_error, 0);
    assert_eq!(c_after_drop.commit_error, 0);
    assert_eq!(
        db.run(r#"docs | id == "drop-doc" | count"#).unwrap().rows[0]
            .get("hits")
            .and_then(|c| c.as_f64()),
        Some(0.0)
    );

    // 4. Statement error / journal quota failure rollback (`statement_error` += 1, `LIN-0212`)
    db = db.with_quotas(Quotas {
        max_tx_mutations: 1,
        ..Default::default()
    });
    let fn_err = db
        .transaction_fn(|tx| {
            tx.execute(r#"insert docs { id: "ok-1", uri: "raw://tx/ok1", title: "Ok1", layer: "raw" }"#)?;
            // Second mutation exceeds `max_tx_mutations: 1`!
            tx.execute(r#"insert docs { id: "fail-2", uri: "raw://tx/f2", title: "Fail2", layer: "raw" }"#)?;
            Ok(())
        })
        .unwrap_err();
    assert!(fn_err.message.contains("max_tx_mutations"), "{fn_err}");
    let c_after_stmt_err = Transaction::rollback_counters();
    assert_eq!(c_after_stmt_err.explicit, 1);
    assert_eq!(c_after_stmt_err.dropped_uncommitted, 1);
    assert_eq!(c_after_stmt_err.statement_error, 1);
    assert_eq!(c_after_stmt_err.commit_error, 0);
    assert_eq!(
        db.run(r#"docs | id == "ok-1" | count"#).unwrap().rows[0]
            .get("hits")
            .and_then(|c| c.as_f64()),
        Some(0.0),
        "first insert must roll back cleanly after second insert hits quota"
    );

    // 5. Commit-time quota failure (`commit_error` += 1)
    db = db.with_quotas(Quotas {
        max_rows: 10_000,
        max_tx_mutations: 100,
        ..Default::default()
    });
    {
        // Current total rows is 38 (1 `users` + 37 `orders`). Set `max_rows: 38`, insert 1 doc in tx
        // so `tx.commit()` aborts via `check_quotas()` and rolls back!
        db = db.with_quotas(Quotas {
            max_rows: 38,
            max_tx_mutations: 100,
            ..Default::default()
        });
        let mut tx = db.transaction().unwrap();
        tx.execute(r#"insert docs { id: "commit-fail", uri: "raw://tx/cf", title: "CF", layer: "raw" }"#)
            .unwrap();
        let commit_err = tx.commit().unwrap_err();
        assert!(commit_err.message.contains("max_rows"), "{commit_err}");
    }
    let c_final = Transaction::rollback_counters();
    assert_eq!(c_final.explicit, 1);
    assert_eq!(c_final.dropped_uncommitted, 1);
    assert_eq!(c_final.statement_error, 1);
    assert_eq!(c_final.commit_error, 1);
    assert_eq!(c_final, lin::persist::rollback_counters());
    assert_eq!(
        db.run(r#"docs | id == "commit-fail" | count"#).unwrap().rows[0]
            .get("hits")
            .and_then(|c| c.as_f64()),
        Some(0.0),
        "aborted commit must roll back in-memory state"
    );

    db.close().unwrap();
    let _ = fs::remove_dir_all(&dir);
}
