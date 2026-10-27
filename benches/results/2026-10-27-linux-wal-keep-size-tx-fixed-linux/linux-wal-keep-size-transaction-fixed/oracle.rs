#[cfg(test)]
mod linux_keep_size_model {
    use super::*;
    #[test]
    fn bounded_geometry_never_reserves_historical_sparse_prefix() {
        const M: u64 = 1024 * 1024;
        assert_eq!(linux_wal_reservation_range(0, 0, 0, false), None);
        assert_eq!(linux_wal_reservation_range(0, M, 100, false), None);
        assert_eq!(linux_wal_reservation_range(6, 0, M, true), None);
        assert_eq!(linux_wal_reservation_range(6, 0, 1, false), Some((6, M as i64)));
        assert_eq!(linux_wal_reservation_range(6, 0, M + 1, false), Some((6, (2 * M) as i64)));
        assert_eq!(linux_wal_reservation_range(16 * 1024 * M, 0, 1, false), Some(((16 * 1024 * M) as i64, M as i64)));
        assert_eq!(linux_wal_reservation_range(6, 0, u64::MAX, false), None);
        assert_eq!(linux_wal_reservation_range(i64::MAX as u64, 0, 1, false), None);
    }
    #[test]
    fn independent_wide_arithmetic_boundary_model() {
        const M: u128 = 1024 * 1024;
        let logicals = [0, 1, 4095, 4096, M as u64 - 1, M as u64, M as u64 + 1, (1u64 << 32) - 1, 1u64 << 32, i64::MAX as u64 - M as u64, i64::MAX as u64, u64::MAX];
        let adds = [0, 1, 4095, M as u64 - 1, M as u64, M as u64 + 1, (16 * M) as u64, (16 * M + 13) as u64, (16 * M + 14) as u64, u32::MAX as u64, u64::MAX];
        let mut checked = 0;
        for logical in logicals {
            for additional in adds {
                for allocated in [0, logical, logical.saturating_add(additional), u64::MAX] {
                    for empty_only in [false, true] {
                        let want = if additional == 0 || additional as u128 > 16 * M + 13 || (empty_only && logical != 0) {
                            None
                        } else {
                            let required = logical as u128 + additional as u128;
                            let rounded = ((additional as u128 + M - 1) / M) * M;
                            if required > u64::MAX as u128 || allocated as u128 >= required || logical as u128 + rounded > i64::MAX as u128 {
                                None
                            } else {
                                Some((logical as i64, rounded as i64))
                            }
                        };
                        let got = linux_wal_reservation_range(logical, allocated, additional, empty_only);
                        assert_eq!(got, want, "{logical} {allocated} {additional} {empty_only}");
                        if let Some((offset, length)) = got {
                            assert_eq!(offset as u64, logical);
                            assert!(length as u128 >= additional as u128 && (length as u128) < additional as u128 + M);
                        }
                        checked += 1;
                    }
                }
            }
        }
        assert_eq!(checked, 1056);
    }
}

#[cfg(all(test, target_os = "linux"))]
mod linux_keep_size_oracle {
    use super::*;
    use std::os::unix::fs::MetadataExt;
    use std::sync::atomic::{AtomicU64, Ordering};
    const M: usize = 1024 * 1024;
    static SEQ: AtomicU64 = AtomicU64::new(0);
    struct Scratch(std::path::PathBuf);
    impl Scratch {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!("lin-linux-keep-size-{}-{}", std::process::id(), SEQ.fetch_add(1, Ordering::Relaxed)));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Scratch { fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); } }
    fn raw(path: &Path) -> File { OpenOptions::new().read(true).write(true).create_new(true).open(path).unwrap() }
    fn state(path: &Path) -> serde_json::Value {
        let m = fs::metadata(path).unwrap();
        serde_json::json!({"logical_bytes":m.len(),"allocated_bytes":m.blocks().saturating_mul(512)})
    }
    #[test]
    fn actual_keep_size_preserves_eof_cursor_prefix_and_reports_support() {
        let root = Scratch::new();let path = root.0.join("file");let mut f = raw(&path);
        let reserved = try_reserve_wal_space_linux(&f, M, false);
        assert_eq!(f.metadata().unwrap().len(), 0);assert_eq!(f.stream_position().unwrap(), 0);
        f.write_all(b"prefix").unwrap();f.seek(SeekFrom::Start(2)).unwrap();
        let grown = try_reserve_wal_space_linux(&f, 2 * M, false);
        assert_eq!(f.metadata().unwrap().len(), 6);assert_eq!(f.stream_position().unwrap(), 2);assert_eq!(fs::read(&path).unwrap(), b"prefix");
        assert_eq!(try_reserve_wal_space_linux(&f, usize::MAX, false).unwrap(), false);
        eprintln!("LIN_KEEP_SIZE_PROBE {}", serde_json::json!({"case":"physical","reserved":matches!(&reserved,Ok(true)),"grown":matches!(&grown,Ok(true)),"first_error":reserved.as_ref().err().and_then(|e|e.raw_os_error()),"state":state(&path)}));
    }
    #[test]
    fn read_only_hint_failure_leaves_ordinary_io_and_file_unchanged() {
        let root = Scratch::new();let path = root.0.join("file");let mut f = raw(&path);f.write_all(b"prefix").unwrap();drop(f);
        let mut ro = File::open(&path).unwrap();ro.seek(SeekFrom::Start(3)).unwrap();
        assert!(try_reserve_wal_space_linux(&ro, 2 * M, false).is_err());
        reserve_wal_space(&ro, 2 * M, false);
        assert_eq!(ro.stream_position().unwrap(), 3);assert_eq!(fs::metadata(&path).unwrap().len(), 6);assert_eq!(fs::read(&path).unwrap(), b"prefix");
        drop(ro);let mut rw = OpenOptions::new().read(true).write(true).open(&path).unwrap();rw.seek(SeekFrom::End(0)).unwrap();rw.write_all(b"suffix").unwrap();durable_sync(&rw).unwrap();assert_eq!(fs::read(&path).unwrap(), b"prefixsuffix");
    }
    #[test]
    fn reservation_preserves_full_frames_replay_export_and_truncate_append() {
        let root = Scratch::new();let mut a = raw(&log_path(&root.0));let control = root.0.join("control");let mut b = raw(&control);
        let row = crate::store::Row::from([("id".into(),crate::store::Cell::Text("a".into())),("embedding".into(),crate::store::Cell::Vec(vec![0.0, -0.0, 1.0].into()))]);
        let mut expected = Vec::new();let mut expected_wire = Vec::new();let mut na = 0;let mut nb = 0;
        for generation in 1..=3 {
            if generation == 2 {
                let before = fs::read(log_path(&root.0)).unwrap();let pos = a.stream_position().unwrap();reserve_wal_space(&a, M, false);assert_eq!(a.stream_position().unwrap(), pos);assert_eq!(fs::read(log_path(&root.0)).unwrap(), before);
            }
            if generation == 3 {truncate_log(&mut a, na).unwrap();assert_eq!(a.stream_position().unwrap(), na);}
            let rec = LogRecord { r#gen:generation,next_id:4,pack:Pack::Update{collection:"docs".into(),rows:vec![row.clone()]} };
            let payload=rmp_serde::to_vec_named(&rec).unwrap();let codec=LOG_MAGIC_V1[3];let mut crc = !0u32;
            for byte in std::iter::once(codec).chain(payload.iter().copied()) {
                crc ^= u32::from(byte);
                for _ in 0..8 { crc = (crc >> 1) ^ (0x82f63b78u32 & 0u32.wrapping_sub(crc & 1)); }
            }
            let checksum = !crc;
            expected_wire.extend_from_slice(&LOG_MAGIC_CHECKSUM);expected_wire.extend_from_slice(&((payload.len()+5)as u32).to_le_bytes());expected_wire.push(codec);expected_wire.extend_from_slice(&payload);expected_wire.extend_from_slice(&checksum.to_le_bytes());expected.push(payload);
            append_record(&mut a,&rec,&mut Vec::new(),SyncMode::Full,&mut na).unwrap();append_record(&mut b,&rec,&mut Vec::new(),SyncMode::Full,&mut nb).unwrap();
            assert_eq!(na,nb);assert_eq!(a.metadata().unwrap().len(),na);assert_eq!(fs::read(log_path(&root.0)).unwrap(),fs::read(&control).unwrap());assert_eq!(fs::read(&control).unwrap(),expected_wire);
        }
        let exported = export_wal_since(&root.0,0).unwrap();assert_eq!(exported,fs::read(&control).unwrap());let mut seen = Vec::new();let end = replay_log(&mut a,0,0,|r|{seen.push(rmp_serde::to_vec_named(&r).unwrap());Ok(())}).unwrap();assert_eq!(seen,expected);assert_eq!(end,na);
        let mut shipped = Vec::new();assert_eq!(for_each_wal_frame_raw(&exported,|raw,_|{shipped.extend_from_slice(raw);Ok(())}).unwrap(),3);assert_eq!(shipped,exported);
    }
    #[test]
    fn db_open_seed_rollback_checkpoint_reopen_and_follower_keep_full_values() {
        let root = Scratch::new();let mut db = crate::Db::open(&root.0).unwrap();let opened = state(&log_path(&root.0));assert_eq!(fs::metadata(log_path(&root.0)).unwrap().len(),0);
        db.run("index docs [wing, ts]").unwrap();let records=(0..1000).map(|i|format!(r#"{{id:"a-{i}",uri:"u:{i}",title:"doc {i}",body:"body {i}",wing:"rag",layer:"wiki",ts:timestamp(123456)}}"#)).collect::<Vec<_>>().join(",");db.run(&format!("insert docs [{records}]")).unwrap();let seeded = state(&log_path(&root.0));
        let before=db.store.collection("docs").to_vec();let cas=before[0]["hash"].text().unwrap().to_owned();let update=db.prepare(&format!(r#"update docs[id=="a-0"] cas "{cas}" {{wing:"sys"}}"#)).unwrap();let initial=db.export_wal_since(0).unwrap();assert_eq!(initial,fs::read(log_path(&root.0)).unwrap());
        {let mut tx=db.transaction().unwrap();tx.run_prepared(&update).unwrap();tx.rollback().unwrap();}assert_eq!(db.store.collection("docs"),before.as_slice());assert_eq!(db.export_wal_since(0).unwrap(),initial);
        update.run(&mut db).unwrap();let expected=db.store.collection("docs").to_vec();let frames=db.export_wal_since(0).unwrap();let mut follower=crate::Db::empty();follower.apply_wal(&frames).unwrap();assert_eq!(follower.store.collection("docs"),expected.as_slice());drop(follower);
        db.checkpoint().unwrap();assert_eq!(fs::metadata(log_path(&root.0)).unwrap().len(),0);let compacted=state(&log_path(&root.0));
        db.run(&format!(r#"update docs[id=="a-0"] cas "{cas}" {{wing:"after-checkpoint"}}"#)).unwrap();let expected=db.store.collection("docs").to_vec();let after_small=state(&log_path(&root.0));let frames=db.export_wal_since(0).unwrap();assert_eq!(frames,fs::read(log_path(&root.0)).unwrap());assert_eq!(for_each_wal_frame(&frames,|_|Ok(())).unwrap(),1);
        drop(db);let closed=state(&log_path(&root.0));let db=crate::Db::open(&root.0).unwrap();assert_eq!(db.store.collection("docs"),expected.as_slice());drop(db);
        eprintln!("LIN_KEEP_SIZE_PROBE {}",serde_json::json!({"case":"db_cycle","opened":opened,"seeded":seeded,"compacted":compacted,"after_small_commit":after_small,"closed":closed,"rows":1000}));
    }
}
